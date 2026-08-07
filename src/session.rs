//! Cross-session persistence of window/tab layout (TDD 7.2).
//!
//! Stored as a tiny TOML file under `$XDG_STATE_HOME/scribobulate/` (state, not
//! config — this is machine-generated UI state, not user configuration). A
//! STANDALONE window close rewrites the whole file from the current set of live
//! windows (`window::lifecycle::persist_all_windows_session`), so the file always
//! reflects "every window still open right before this one closed" — not just the
//! closing window's own state. A COORDINATED quit (`window::quit_all_windows`)
//! instead snapshots all windows ONCE and FREEZES [`save`] (see [`FROZEN`]) while
//! it closes each window in turn, because `app.windows()` shrinks across that
//! sequence and per-close writes would otherwise leave only the last window
//! (GTK4Rs/AP-113, TDD 15.10).
//!
//! ## Schema (v3)
//!
//! One [`Session`] holds the app-wide `preview_theme` (genuinely app-wide: one
//! CSS provider, one value) plus a `Vec<WindowSession>`, one per open window:
//! its geometry, its **one shared zoom level** (zoom is a window-level
//! accessibility setting, not a per-tab one — operator
//! decision), its own [`ChromeSession`] (toolbar / status bar / outline
//! visibility — per window, not app-wide), and a `Vec<TabSession>` (path, view
//! mode, split arrangement, unsafe-images toggle — no per-tab zoom field) plus
//! which tab was active. No tab content is persisted (only its `path`, if any) —
//! an unsaved untitled tab restores blank, matching `create_tab_in_window`'s own
//! blank-tab convention (`window::restore`).
//!
//! ## Migrating pre-Phase-4 (v1) files
//!
//! The v1 schema was one flat struct describing a single window with a single
//! tab (no `windows` key at all). [`load`] distinguishes the two shapes by
//! checking for a top-level `windows` key (this crate's own writer always
//! includes one, even when empty) rather than trying both parses and hoping one
//! fails cleanly — `Session`'s `#[serde(default)]` would otherwise "succeed" on
//! a v1 file by silently treating every v1 field as unknown-and-ignored and
//! filling `windows` from `Default` (an empty list), discarding the old
//! session's window size, zoom, and view mode entirely instead of migrating
//! them. A v1 file migrates to one window with one tab (no path — v1 never
//! recorded one), carrying over its old flat width/height/zoom/view-mode/split/
//! unsafe-images fields.
//!
//! ## Migrating app-wide-chrome (v2) files
//!
//! v2 kept `show_toolbar` / `show_statusbar` / `toolbar_sections` /
//! `outline_visible` at the TOP level, app-wide. They are now per window
//! ([`ChromeSession`] inside each [`WindowSession`]), so a v2 file's top-level
//! values are read once by [`migrate_v2_app_wide_chrome`] and applied to EVERY
//! restored window — the only migration that preserves what the user actually
//! saw, since a v2 session had exactly one chrome answer for all of its windows.
//!
//! This migration is a strict ADD, never a parse gate: serde ignores unknown
//! fields, so a v2 file already deserializes into the v3 [`Session`] cleanly
//! (its top-level chrome keys simply drop, every window defaulting to
//! the ChromeSession default). [`V2AppWideChrome`] is therefore a SEPARATE, infallible,
//! all-`Option` parse of the same text — a v2 file that cannot be read for its
//! old chrome loses only the chrome, never a window. Presence of any top-level
//! chrome key is the v2 signal; this crate's v3 writer emits none of them, so
//! the migration cannot fire on a file it wrote itself.

use crate::config::config;
use crate::winstate::ViewMode;
use std::cell::Cell;
use std::path::{Path, PathBuf};

thread_local! {
    /// When set, [`save`] is a no-op. A coordinated app quit
    /// (`window::quit_all_windows`) snapshots the FULL multi-window session ONCE
    /// while every window is alive, then closes each window in turn — but each
    /// close fires its close-request, whose `persist_all_windows_session` would
    /// otherwise re-`save` a SHRINKING window set (the last window to close would
    /// persist only itself, wiping every other window — TDD 15.10). Freezing after
    /// the upfront snapshot keeps that snapshot intact through the close sequence;
    /// a cancelled close (the user aborts quit) un-freezes so normal per-close
    /// persistence resumes. GTK is single-threaded, so a `thread_local` is app-global.
    ///
    /// INVARIANT (QA L-2) — why the thaw is safe with several dirty windows: a
    /// coordinated quit of N windows, ≥2 of them dirty, leaves several Save/Discard/
    /// Cancel dialogs pending at once while `FROZEN == true`. Only an **abort** thaws
    /// — a Cancel/dismiss, or a Save the user backed out of (`window::save`'s confirm
    /// arms: Accept-but-unsaved and the Cancel branch call `set_frozen(false)`); a
    /// successful Save or a Discard proceeds through the close sequence WITHOUT
    /// thawing, so the freeze holds until the quit either completes (all windows
    /// gone) or is aborted. When an abort does thaw, correctness rests on the fact
    /// that `window::lifecycle::persist_all_windows_session` always re-snapshots ALL
    /// still-live windows from `app.windows()` — never just the closing one — so the
    /// resumed per-window `save` re-persists the whole current set, not a shrunk one.
    /// Do NOT change persistence to a single-window write, or thaw on a Save/Discard,
    /// without scoping this freeze to the quit operation itself — either would
    /// reintroduce the TDD-15.10 / GTK4Rs/AP-113 data-loss class this guard exists to
    /// prevent. (Worst case under the current design is bounded: a stale phantom
    /// window restored next launch, never lost edits.)
    static FROZEN: Cell<bool> = const { Cell::new(false) };
}

/// Freeze (`true`) or thaw (`false`) session persistence — see [`FROZEN`].
pub(crate) fn set_frozen(frozen: bool) {
    FROZEN.with(|f| f.set(frozen));
}

/// One persisted tab: everything `TabState` needs to reconstruct it, minus the
/// document's actual text (never persisted — see the module doc).
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct TabSession {
    /// Backing file path, or `None` for an untitled tab (restores blank).
    pub path: Option<PathBuf>,
    /// This tab's crash-recovery document id (`swapfile::DocId`), so a restored tab can
    /// be correlated with the swap file holding its unsaved content.
    ///
    /// **Additive and non-breaking**: the struct is `#[serde(default)]`, so a session
    /// file written before this field existed simply yields `None` and each such tab
    /// gets a fresh id on restore — no version bump, no migration function (contrast the
    /// v1→v3 machinery below).
    ///
    /// **A raw `String`, deliberately.** Deserialising into a validating newtype would
    /// let one hand-edited or corrupted id fail the *entire* session load, costing the
    /// user every window and tab to protect a field whose worst case is a regenerated
    /// id. It is validated where it is used instead (`swapfile::DocId::from_hex`), and a
    /// rejected value is simply replaced.
    pub doc_id: Option<String>,
    pub view_mode: ViewMode,
    pub split_swap: bool,
    pub split_vertical: bool,
    /// This tab's own "Show Unsafe Images" toggle (per-tab, unlike zoom).
    pub show_unsafe_images: bool,
}

/// One persisted window: its geometry, its shared zoom level, its own chrome
/// visibility, and its tabs.
///
/// FIELD ORDER IS LOAD-BEARING for TOML: a table's scalar keys must all be
/// emitted before any sub-table, so the scalars come first, then `chrome`
/// (`[windows.chrome]`), then `tabs` (`[[windows.tabs]]`). Re-ordering `chrome`
/// above a scalar makes `toml::to_string` fail with `ValueAfterTable` and
/// [`save`] log-and-drop the whole session.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Clone, Debug)]
#[serde(default)]
pub(crate) struct WindowSession {
    pub width: i32,
    pub height: i32,
    pub zoom_level: f64,
    /// Index into `tabs` of the tab that was active when this window closed.
    pub active_tab: usize,
    /// THIS window's own toolbar/status-bar/outline visibility (v3 — the
    /// operator's per-window decision). A v2 file has no per-window `chrome`
    /// table; `#[serde(default)]` fills it with the ChromeSession default and
    /// [`migrate_v2_app_wide_chrome`] then overwrites it with the file's
    /// top-level app-wide values.
    pub chrome: ChromeSession,
    /// Always has at least one entry when written by [`save`]; [`window::restore`]
    /// still defensively falls back to a single blank tab if it's ever empty
    /// (a hand-edited session file, for instance).
    pub tabs: Vec<TabSession>,
}

impl Default for WindowSession {
    fn default() -> Self {
        Self {
            width: config().window.width,
            height: config().window.height,
            zoom_level: 1.0,
            active_tab: 0,
            chrome: ChromeSession::default(),
            tabs: vec![TabSession::default()],
        }
    }
}

/// Pre-Phase-4 on-disk shape: one flat window/tab, no `windows` key. Kept only
/// as a [`load`] migration source — never written. A hand-trimmed old file
/// missing a field falls back to that field's value in [`Default`] below —
/// deliberately mirroring the pre-Phase-4 `Session::default()` (e.g. `width`/
/// `height` from `config()`, not zero) rather than the derived per-type
/// zero/false `Default` a field-by-field fallback would otherwise silently
/// substitute.
#[derive(serde::Deserialize)]
#[serde(default)]
struct LegacySession {
    width: i32,
    height: i32,
    view_mode: ViewMode,
    show_toolbar: bool,
    show_statusbar: bool,
    zoom_level: f64,
    show_unsafe_images: bool,
    split_swap: bool,
    split_vertical: bool,
}

impl Default for LegacySession {
    fn default() -> Self {
        Self {
            width: config().window.width,
            height: config().window.height,
            view_mode: ViewMode::Preview,
            show_toolbar: true,
            show_statusbar: true,
            zoom_level: 1.0,
            show_unsafe_images: false,
            split_swap: false,
            split_vertical: false,
        }
    }
}

impl From<LegacySession> for Session {
    fn from(l: LegacySession) -> Self {
        Session {
            // A legacy file predates reading themes entirely, so it restores the
            // base theme — i.e. exactly the appearance it was saved under.
            preview_theme: crate::theme::SYSTEM_ID.to_string(),
            windows: vec![WindowSession {
                width: l.width,
                height: l.height,
                zoom_level: l.zoom_level,
                active_tab: 0,
                // v1 described exactly ONE window, so its flat chrome fields are
                // that window's own chrome — no app-wide-to-per-window question
                // to answer here (unlike the v2 migration below).
                chrome: ChromeSession {
                    show_toolbar: l.show_toolbar,
                    show_statusbar: l.show_statusbar,
                    // A pre-toolbar-sections file predates per-section
                    // visibility, so its sections fall to the current default
                    // (file/edit/view shown; format/split/zoom hidden).
                    toolbar_sections: ToolbarSections::default(),
                    // A legacy file predates the outline sidebar's persistence
                    // entirely, so it restores shown — the behavior every
                    // pre-fix session already had (the toggle was never
                    // persisted, so a window always opened with the outline
                    // visible).
                    outline_visible: true,
                    // A legacy file predates the annotations viewer entirely, so
                    // it restores hidden — its default.
                    annotations_visible: false,
                },
                tabs: vec![TabSession {
                    path: None,
                    // A legacy file predates crash recovery entirely, so the restored
                    // tab keeps the fresh id it was born with.
                    doc_id: None,
                    view_mode: l.view_mode,
                    split_swap: l.split_swap,
                    split_vertical: l.split_vertical,
                    show_unsafe_images: l.show_unsafe_images,
                }],
            }],
        }
    }
}

/// Per-section toolbar visibility (the `S_i` states; see the
/// `window::viewactions` invariants I1–I7). Scoped to
/// one window, exactly like the `show_toolbar` it lives beside in
/// [`ChromeSession`]. Each flag is the persisted *state* `S_i` of that window's
/// matching `win.show-tbtn-<id>` action; the *enabled* attribute is always
/// derived from `show_toolbar` at runtime (invariant I3) and never stored.
/// Field names are the canonical
/// section IDs (`crate::app::TBTN_SECTION_IDS`) — keep the two in sync.
///
/// Default (operator decision, 2026-07-21): **`file`, `edit`, `view` shown;
/// `format`, `split`, `zoom` hidden.** A fresh profile opens with a short toolbar
/// carrying the commands most workflows use, and the user opts the rest in via
/// `View ▸ Toolbar` — most users enable only the sections meaningful to them, so
/// starting minimal beats starting maximal. This is safe for `format` even though
/// it is the editor focus-gate ancestor: a hidden Format bar simply never holds
/// focus (the gate's `is_ancestor(format_box)` branch is a sticky early-return),
/// so `win.format` still enables on editor focus and formatting stays available
/// via the Format menu and accelerators. Per-field `#[serde(default)]` means a
/// session file that omits a section flag inherits these values.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Clone, Copy, Debug)]
#[serde(default)]
pub(crate) struct ToolbarSections {
    pub file: bool,
    pub edit: bool,
    pub format: bool,
    pub view: bool,
    pub split: bool,
    pub zoom: bool,
}

impl Default for ToolbarSections {
    fn default() -> Self {
        Self {
            file: true,
            edit: true,
            format: false,
            view: true,
            split: false,
            zoom: false,
        }
    }
}

impl ToolbarSections {
    /// The six flags in canonical `TBTN_SECTION_IDS` order. Used to seed the
    /// section-box visibilities / action states at window build.
    pub fn to_array(self) -> [bool; 6] {
        [
            self.file,
            self.edit,
            self.format,
            self.view,
            self.split,
            self.zoom,
        ]
    }

    /// Set one section's flag by its canonical ID (used when snapshotting the
    /// live action states back into the session on save). An unknown ID is
    /// ignored — the caller only ever passes `TBTN_SECTION_IDS` members.
    pub fn set(&mut self, id: &str, on: bool) {
        match id {
            "file" => self.file = on,
            "edit" => self.edit = on,
            "format" => self.format = on,
            "view" => self.view = on,
            "split" => self.split = on,
            "zoom" => self.zoom = on,
            _ => {}
        }
    }
}

/// One window's chrome visibility: its toolbar, its per-section toolbar flags,
/// its status bar and its outline sidebar.
///
/// **Scope (operator decision): PER WINDOW.** Each field is the persisted state
/// of the matching `win.*` toggle action on ONE window — `win.show-toolbar`,
/// `win.show-tbtn-<id>`, `win.show-statusbar`, `win.outline`. Those actions
/// always were per-window (each handler only ever touched its own window's
/// widgets); this type is where that runtime truth is now stored and persisted,
/// so all three of storage, behaviour and persistence finally give one answer.
///
/// Scope note (see the state-scope rule in the `winstate` module doc): this type is
/// for WINDOW-scoped state. App-wide state (`preview_theme` — one CSS provider,
/// so one value) and tab-scoped state (`show_unsafe_images`) do NOT belong here;
/// they have different owners and different inheritance rules.
///
/// FIELD ORDER IS LOAD-BEARING for TOML — see [`WindowSession`]'s note.
/// `toolbar_sections` is a sub-table and must stay last.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Clone, Copy, Debug)]
#[serde(default)]
pub(crate) struct ChromeSession {
    pub show_toolbar: bool,
    pub show_statusbar: bool,
    /// Whether the outline sidebar (`win.outline` — F9, the View menu, the
    /// toolbar button, and the in-pane × all share it) was shown.
    pub outline_visible: bool,
    /// Whether the annotations viewer (`win.annotations` — the View menu, the
    /// toolbar button, and the in-pane × all share it) was shown. Persisted
    /// per-window alongside `outline_visible` (TDD 20.13). Defaults **hidden**:
    /// most documents carry no annotations, so showing an empty "No annotations"
    /// pane on every window would be gratuitous — a reviewer turns it on when
    /// reviewing. A pre-annotations session file (no key) restores to that default.
    pub annotations_visible: bool,
    pub toolbar_sections: ToolbarSections,
}

impl Default for ChromeSession {
    /// A fresh window (and a session file with no `chrome` table) shows the
    /// toolbar, statusbar, and outline; the annotations viewer starts hidden
    /// (see `annotations_visible`).
    fn default() -> Self {
        Self {
            show_toolbar: true,
            show_statusbar: true,
            outline_visible: true,
            annotations_visible: false,
            toolbar_sections: ToolbarSections::default(),
        }
    }
}

/// Deserialize-only view of a v2 file's TOP-LEVEL, app-wide chrome keys, kept
/// solely as a [`parse`] migration source — no version of this crate writes
/// these keys any more (they live per window now, in [`ChromeSession`]).
///
/// **Every field must stay `Option`.** That is the whole safety property, and it
/// is doing all of the work: `Option` makes ABSENT distinguishable from `false`,
/// which (a) lets a v2 file that predates `outline_visible`/`toolbar_sections`
/// migrate only the keys it actually has, and (b) makes
/// [`migrate_v2_app_wide_chrome`] inherently a NO-OP on a v3 file, which carries
/// none of these keys — so no "is this a v2 file?" test is needed, or even
/// meaningful. Give any field a derived `bool` default instead and a v3 file
/// reads as "a v2 file with everything switched off", hiding every window's
/// chrome on the next launch.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct V2AppWideChrome {
    show_toolbar: Option<bool>,
    show_statusbar: Option<bool>,
    outline_visible: Option<bool>,
    toolbar_sections: Option<ToolbarSections>,
}

/// Apply a v2 file's app-wide chrome to EVERY window in the already-parsed
/// session — the migration answer for the app-wide-to-per-window move.
///
/// Why "apply to every window" is right, and not merely convenient: a v2 session
/// had exactly ONE chrome answer that every window rendered, because the v2
/// build path seeded every window from that one value. Copying it to each window
/// therefore reproduces exactly what the user saw when they closed the app —
/// which is what session restore promises. There is no per-window value to
/// recover, because none was ever recorded.
///
/// Why this is not a parse gate: it runs AFTER `windows` has been parsed and it
/// only ever OVERWRITES chrome, so a v2 file whose old chrome keys are somehow
/// unreadable degrades to the default chrome and keeps every window — never the
/// reverse. Each key is applied independently (a v2 file predating
/// `toolbar_sections` or `outline_visible` has only some of them), which is also
/// why this needs no v2-detection test to guard it: on a v3 file every field is
/// `None` and every arm below is skipped, so calling this unconditionally is
/// already a no-op there.
fn migrate_v2_app_wide_chrome(session: &mut Session, v2: &V2AppWideChrome) {
    for w in &mut session.windows {
        if let Some(on) = v2.show_toolbar {
            w.chrome.show_toolbar = on;
        }
        if let Some(on) = v2.show_statusbar {
            w.chrome.show_statusbar = on;
        }
        if let Some(on) = v2.outline_visible {
            w.chrome.outline_visible = on;
        }
        if let Some(sections) = v2.toolbar_sections {
            w.chrome.toolbar_sections = sections;
        }
    }
}

/// FIELD ORDER IS LOAD-BEARING for TOML — see [`WindowSession`]'s note. The
/// scalar `preview_theme` must stay above the `windows` array-of-tables.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
#[serde(default)]
pub(crate) struct Session {
    /// The selected preview reading theme's id (`src/theme.rs`). Genuinely
    /// app-wide, unlike the per-window chrome in [`ChromeSession`]: the theme is
    /// one app-wide CSS provider, so there is exactly one value and no
    /// "which window's?" question to answer. Defaults to the base theme, so an
    /// older session file with no `preview_theme` key restores today's
    /// desktop-derived appearance. An id naming a theme the user has since
    /// deleted resolves back to the base theme rather than failing.
    pub preview_theme: String,
    /// One entry per window open when the session was last saved. Empty means
    /// "no saved session yet" (fresh install) — `window::restore::restore_session`
    /// treats that as "fall back to the default single blank window".
    pub windows: Vec<WindowSession>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            preview_theme: crate::theme::SYSTEM_ID.to_string(),
            windows: Vec::new(),
        }
    }
}

/// Where the session file lives, or `None` if no state directory can be located
/// (in which case save and load both no-op and the app simply starts fresh).
fn session_path() -> Option<PathBuf> {
    Some(state_directory()?.join("session.toml"))
}

/// The application's state directory (`…/scribobulate`), or `None` if none can be
/// located.
///
/// **The single state-directory lookup in the tree.** `session.toml` and the
/// crash-forensics log and reports (`forensics::state_directory`) both resolve
/// through here, so they cannot drift into different places, and the warning below
/// is emitted once for the process rather than once per consumer.
///
/// `XDG_STATE_HOME` is checked FIRST on **every** platform, and read **live** rather
/// than cached — [`with_state_home_for_test`] sets it at runtime to redirect the
/// tests at a temp dir, and 14 call sites across this module and `window/mod.rs`
/// depend on that still working. This is also why the lookup is hand-rolled from
/// `std::env` rather than delegating to `glib::user_state_dir()`: GLib caches its
/// answer on first call and would ignore the helper entirely.
///
/// Only the FALLBACK differs per platform, because `HOME` is a POSIX convention that
/// Windows does not set — assuming it there left this returning `None` forever, so
/// nothing persisted at all (ScrAP-167).
pub(crate) fn state_directory() -> Option<PathBuf> {
    let Some(base) = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(state_home_fallback)
    else {
        // Warn once, not per save/load. This hid for a whole port precisely
        // because the failure was silent — "nothing restored" is indistinguishable
        // from "nothing was ever saved" (ScrAP-167).
        //
        // Gated on the warning being DELIVERABLE, not just on having warned (QA round
        // 5, L-4). The first caller is `forensics::install`, which runs at
        // `logging.rs:131` — one line BEFORE `log::set_logger` at :132. With no logger
        // installed, `log::warn!` is a no-op against a max-level of `Off`, so that call
        // dropped the message into a void *and consumed the latch*, and every later
        // caller — the ones that run with a working logger — stayed silent forever.
        // A one-shot warning that fires exactly once, before anything can hear it, is
        // the ScrAP-167 silence rebuilt inside its own fix.
        if log::log_enabled!(log::Level::Warn) {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                log::warn!(
                    "no user state directory could be located \
                 (checked XDG_STATE_HOME, then the platform fallback); \
                 window geometry and open tabs will not persist"
                );
            });
        }
        return None;
    };
    Some(base.join("scribobulate"))
}

/// Create the state directory **private to the owning user**, and tighten it if an
/// earlier version left it open.
///
/// **The one place the state directory comes into existence.** Three call sites created
/// it with a bare `create_dir_all` — `session::save`, `forensics::sink`,
/// `forensics::report` — which is `0777 & !umask`, so under the common `umask 0002` it
/// landed at **0775**. MEASURED on the reference machine before this existed:
///
/// ```text
/// drwxrwxr-x  ~/.local/state/scribobulate
/// -rw-rw-r--  session.toml
/// ```
///
/// Group- and world-readable, and `session.toml` records **the paths of every open
/// document** along with window geometry. The crash reports beside it were correctly
/// `0600` via [`crate::forensics::private_options`], but a private file in a traversable
/// directory still leaks its *name*, and these names carry a timestamp and a pid.
///
/// **Why the directory and not each file.** `session.toml` is written through
/// `atomic_io::write_atomic`, which deliberately *relaxes* a new file from its private
/// creation mode to the umask default — correct, because the same function saves the
/// user's own documents, where forcing `0600` would be wrong. So there is no per-file fix
/// that does not either break document saving or get re-omitted at the next call site.
/// A `0700` directory subsumes all of it: nothing inside is reachable by another user
/// whatever its own mode says.
///
/// That is also the **platform-symmetric** answer. The Windows seat measured the identical
/// exposure there and reached the same place from the other direction: `OpenOptionsExt`
/// has no security-descriptor hook, so file-level privacy is not expressible at all, and
/// the fix is a PROTECTED DACL on the directory that everything inside then inherits.
/// POSIX inheritance is traversal rather than ACEs, but the seam is the same one, which is
/// what lets TDD 21.12 be stated once for both platforms instead of per-platform.
/// `0700` is also what the XDG Base Directory spec asks for.
///
/// Existing directories are tightened, not just new ones — otherwise every installation
/// that has already run keeps the open mode forever, which is the same
/// only-applies-on-creation trap that made the signal handler's `0600` a no-op
/// (GTK4Rs/AP-108/GTK4Rs/AP-130). Only this app's own leaf directory is touched, never a parent.
pub(crate) fn create_state_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        // Whether this is a MIGRATION or a fresh creation, decided before creating.
        //
        // An unconditional tighten would be simpler and equally safe — and that is what
        // this did first. It was wrong in a way worth recording: it made the creation
        // mode dead weight, because the chmod that followed corrected 0755 just as
        // happily as it corrected 0775. MUTATION-TESTED: changing `.mode(0o700)` to
        // `.mode(0o755)` left the guard GREEN. Two mechanisms where one carries the
        // property is a mechanism nothing tests, and this round is largely about those.
        //
        // Splitting them makes each load-bearing: creation privacy is what leaves no
        // window in which the directory exists world-readable (a chmod-after-create has
        // one), and the tighten is what reaches installations that already ran. Racing
        // this only loses if another process creates the directory between the check and
        // the create — and the only thing that does is another instance of this app,
        // which creates it 0700.
        let migrating = std::fs::symlink_metadata(dir).is_ok();
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        if migrating {
            // `create_dir_all` leaves an EXISTING directory's mode alone, so a tree made
            // by an earlier build stays 0775 until something narrows it. Best-effort: a
            // failure here must not stop the session being saved.
            if let Ok(meta) = std::fs::metadata(dir) {
                if meta.permissions().mode() & 0o077 != 0 {
                    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
                }
            }
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        // Windows needs a PROTECTED DACL, which `std::fs` cannot express — hence the
        // seam. `create_private_directory` creates with `SECURITY_ATTRIBUTES` so there
        // is no instant in which the directory exists unprotected, and tightens an
        // existing one via `SetNamedSecurityInfoW`, mirroring the create/migrate split
        // above for the same reason: a creation-time-only fix reaches nobody who has
        // already run the app.
        //
        // The error is PROPAGATED, not swallowed. Unlike the unix tighten there is no
        // `0700`-created floor underneath a failure here: if the DACL does not land the
        // directory may be genuinely writable by other local users, and callers writing
        // unsaved document text into it need to be able to decline. See
        // `platform/win32/privacy.rs` for the measured exposure this closes.
        crate::platform::win32::create_private_directory(dir)
    }
    #[cfg(all(not(unix), not(windows)))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// POSIX fallback: `~/.local/state`, per the XDG Base Directory spec.
#[cfg(unix)]
fn state_home_fallback() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state"))
}

/// Windows fallback: **Local** AppData, deliberately not Roaming. Session state is
/// window geometry, open tabs and per-monitor layout — machine-specific things that
/// should not follow a roaming profile onto a different machine with a different
/// screen setup. User *configuration* takes the opposite decision; see
/// `config::config_home_fallback`.
#[cfg(windows)]
fn state_home_fallback() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("AppData").join("Local"))
        })
}

/// Parse on-disk TOML text into a [`Session`], transparently migrating both
/// older shapes (see the module doc's two migration sections). Split out from
/// [`load`] so the migration logic is unit-testable without touching the
/// filesystem.
fn parse(text: &str) -> Session {
    // v2/v3 files always have a top-level `windows` key (see `save`) — its
    // absence is the v1 migration signal, not a failed/lenient parse of the
    // newer shape.
    let has_windows = toml::from_str::<toml::Table>(text)
        .map(|t| t.contains_key("windows"))
        .unwrap_or(false);
    if !has_windows {
        return toml::from_str::<LegacySession>(text)
            .map(Session::from)
            .unwrap_or_default();
    }
    // v2 and v3 share this parse: serde ignores unknown fields, so a v2 file's
    // top-level chrome keys drop here rather than failing (which would discard
    // every window). The second, all-`Option` parse below recovers them.
    let mut session: Session = toml::from_str(text).unwrap_or_default();
    // Unconditional: a v3 file has none of these keys, so every field is `None`
    // and this is a no-op (see `V2AppWideChrome`). A separate "is it v2?" test
    // would be unfalsifiable decoration.
    let v2 = toml::from_str::<V2AppWideChrome>(text).unwrap_or_default();
    migrate_v2_app_wide_chrome(&mut session, &v2);
    session
}

/// Load the saved session, falling back to defaults when missing or unparseable.
pub(crate) fn load() -> Session {
    let Some(path) = session_path() else {
        return Session::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => parse(&text),
        Err(_) => Session::default(),
    }
}

/// Persist the session (best effort — a write failure is non-fatal, but is
/// now logged rather than silently swallowed, QA round-1 L1: a permissions or
/// full-disk problem used to fail invisibly, leaving no trace for a future bug
/// report to grep). Written via `atomic_io::write_atomic` (QA round-1 M4):
/// `fs::write` truncates then streams, so a crash mid-write left a truncated
/// file that `parse` then reads as a legacy v1 file and migrates to a single
/// default window — every other window's saved layout silently gone.
/// write-temp-then-rename makes that torn-write window impossible.
pub(crate) fn save(session: &Session) {
    // TEST BUILDS ONLY — never compiled into the shipped app.
    //
    // A test must never write the *real* state directory. The integration tests
    // close real windows, which runs `window::lifecycle::persist_all_windows_session`
    // → here, and the path below resolves `XDG_STATE_HOME` live from the environment
    // — so with no override this overwrites the tester's own `session.toml` (their
    // open tabs, window geometry, theme) with whatever the test happened to build.
    // Measured, not theorised.
    //
    // `.cargo/config.toml`'s `[env]` supplies a scratch directory and is the primary
    // defence, but it only covers what Cargo launches. This assertion is the backstop
    // for what it cannot reach: the test binary run directly, an IDE/rust-analyzer
    // runner that bypasses Cargo's config, a CI step that scrubs the environment, or
    // a future edit to that config by someone who does not know what the line guards.
    // Hence the guard lives in the function that does the damage rather than in the
    // callers — there is no test seam on the production path, so a per-test override
    // would have to be remembered by every future window test, and forgetting it
    // fails silently AND destructively (the GTK4Rs/AP-108 shape).
    //
    // It converts that failure into a loud, harmless one. It cannot fire under the
    // normal `cargo test` route, where `[env]` always sets the variable. Scope note:
    // `#[cfg(test)]` covers the in-crate suite (unit + the `gtk-integration-tests`
    // modules), which is where the leak was; a `harness = false` target that does not
    // re-declare the module tree compiles `src/` without `cfg(test)` and so is not
    // covered — such a target must set `XDG_STATE_HOME` itself.
    #[cfg(test)]
    assert!(
        std::env::var_os("XDG_STATE_HOME").is_some(),
        "session::save reached in a test with no XDG_STATE_HOME override — this \
         would overwrite the developer's real session.toml. Run the suite through \
         Cargo so .cargo/config.toml's [env] applies, or set XDG_STATE_HOME to a \
         scratch directory (see session::with_state_home_for_test)."
    );

    // Suppressed during a coordinated quit's window-close sequence so the upfront
    // full-session snapshot is not overwritten by a shrinking set (see [`FROZEN`]).
    if FROZEN.with(|f| f.get()) {
        return;
    }
    let Some(path) = session_path() else { return };
    if let Some(dir) = path.parent() {
        // Through the one seam, so `session.toml` lands inside a private directory —
        // it records the paths of every open document.
        if let Err(e) = create_state_dir(dir) {
            log::warn!("session::save: could not create {}: {e}", dir.display());
            return;
        }
    }
    match toml::to_string(session) {
        Ok(text) => {
            if let Err(e) = crate::atomic_io::write_atomic(&path, &text) {
                log::warn!("session::save: could not write {}: {e}", path.display());
            }
        }
        Err(e) => log::warn!("session::save: could not serialize the session: {e}"),
    }
}

// Serialize tests (in THIS module and any other, e.g. a `gtk-integration-tests`
// module elsewhere) that mutate the process-global `XDG_STATE_HOME`: Rust runs
// tests on parallel (and, for the same worker pool, potentially reused) threads
// and the environment is shared, so concurrent set/remove would flake. Crate-
// visible so every test that needs a scratch session directory shares this ONE
// lock rather than each introducing its own (which would not actually
// serialize against each other).
#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run `f` with `XDG_STATE_HOME` pointed at `dir` for the duration, restoring
/// the prior value on the way out. Holds [`ENV_LOCK`] for the duration.
#[cfg(test)]
pub(crate) fn with_state_home_for_test<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::var_os("XDG_STATE_HOME");
    std::env::set_var("XDG_STATE_HOME", dir);
    let out = f();
    match prev {
        Some(v) => std::env::set_var("XDG_STATE_HOME", v),
        None => std::env::remove_var("XDG_STATE_HOME"),
    }
    out
}

#[cfg(test)]
mod tests {

    /// The state directory is private to its owner, and an over-permissive one is
    /// tightened (QA round 5, M-6's Linux half).
    ///
    /// MEASURED before the fix, on the reference machine under the default `umask 0002`:
    /// `drwxrwxr-x ~/.local/state/scribobulate` holding `-rw-rw-r-- session.toml` — the
    /// paths of every open document, world-readable. The crash reports beside it were
    /// already `0600`, which is why this went unnoticed: the per-file seam was correct
    /// and the directory containing it was not, so a private file sat in a traversable
    /// directory advertising its own name.
    ///
    /// Both halves are asserted because each is now load-bearing, and getting that true
    /// took a second attempt worth recording: the first version tightened
    /// unconditionally, which made the *creation* mode untested dead weight —
    /// MUTATION-TESTED, `.mode(0o700)` → `.mode(0o755)` stayed GREEN, because the chmod
    /// that followed corrected either one. `create_state_dir` now tightens only when
    /// migrating, so this kills both mutants: 0755-at-creation fails the fresh case, and
    /// dropping the tighten fails the stale case. The stale case is the
    /// only-applies-on-creation trap that made the signal handler's `0600` a no-op.
    ///
    /// **Both platforms now assert, by their own privacy mechanism.** Windows carries the
    /// property on a PROTECTED DACL rather than on mode bits, so the Windows arm reads
    /// the DACL back out of the OS and checks the same two things the unix arm does: that
    /// a freshly created directory is private, and that a directory an earlier build left
    /// open is TIGHTENED. Asserting the SDDL we passed in would prove nothing — this
    /// reads what the OS stored.
    #[test]
    fn the_state_directory_is_private_to_its_owner() {
        #[cfg(windows)]
        {
            use super::create_state_dir;

            // `P` in the DACL flags is the protected bit — inheritance severed. Without
            // it our ACEs would sit alongside the inherited permissive ones and the
            // directory would still be exposed, which is the exact defect this closes.
            // `AU` (Authenticated Users) and `BU` (BUILTIN\Users) are the two the
            // off-profile volume was granting; on a second volume they arrived as
            // `(I)(M)` — MODIFY, not merely read.
            let assert_private = |dir: &std::path::Path, case: &str| {
                let sddl = crate::platform::win32::directory_dacl_sddl(dir)
                    .unwrap_or_else(|| panic!("{case}: could not read the DACL back"));
                assert!(
                    sddl.starts_with("D:P"),
                    "{case}: DACL is not PROTECTED, so it still inherits: {sddl}",
                );
                for principal in [";AU)", ";BU)", ";WD)"] {
                    assert!(
                        !sddl.contains(principal),
                        "{case}: DACL still grants {principal}: {sddl}",
                    );
                }
            };

            let base = std::env::temp_dir().join(format!(
                "scrib-dacl-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&base);

            // Fresh creation: never open for an instant.
            let fresh = base.join("fresh");
            create_state_dir(&fresh).expect("create");
            assert_private(&fresh, "freshly created");

            // Migration: a directory an earlier build created with a bare
            // `create_dir_all`, inheriting whatever the parent granted.
            let stale = base.join("stale");
            std::fs::create_dir_all(&stale).unwrap();
            create_state_dir(&stale).expect("create over an existing directory");
            assert_private(&stale, "pre-existing directory");

            let _ = std::fs::remove_dir_all(&base);
        }
        #[cfg(all(not(unix), not(windows)))]
        {
            crate::testsymlink::skipped(
                "TDD 21.12 state-directory permissions",
                "neither POSIX modes nor Windows ACLs apply on this platform",
            );
        }
        #[cfg(unix)]
        {
            use super::create_state_dir;
            use std::os::unix::fs::PermissionsExt;
            use std::path::Path;
            let tmp = tempfile::tempdir().unwrap();
            let mode_of = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;

            // Freshly created, including intermediate components.
            let fresh = tmp.path().join("state").join("scribobulate");
            create_state_dir(&fresh).expect("create");
            assert_eq!(
                mode_of(&fresh),
                0o700,
                "a new state directory must not be readable by anyone else — it holds \
                 session.toml, which records the path of every open document"
            );

            // An existing directory left open by an earlier build is tightened.
            let stale = tmp.path().join("stale");
            std::fs::create_dir_all(&stale).unwrap();
            std::fs::set_permissions(&stale, std::fs::Permissions::from_mode(0o775)).unwrap();
            assert_eq!(mode_of(&stale), 0o775, "precondition: the pre-fix mode");
            create_state_dir(&stale).expect("create over an existing directory");
            assert_eq!(
                mode_of(&stale),
                0o700,
                "an installation that has already run keeps its open directory forever \
                 unless creation also narrows an existing one"
            );

            // And the point of doing this at the directory: a file inside keeps whatever
            // mode its own writer chose (write_atomic deliberately relaxes new files, so
            // that documents are not forced to 0600) and is still unreachable to others,
            // because reaching it requires traversing this directory.
            let inside = fresh.join("session.toml");
            std::fs::write(&inside, "x").unwrap();
            assert_eq!(
                mode_of(fresh.as_path()) & 0o077,
                0,
                "no group/other bits on the directory is what protects {}",
                inside.display()
            );
        }
    }

    use super::{
        load, parse, save, session_path, with_state_home_for_test, ChromeSession, Session,
        TabSession, ToolbarSections, ViewMode, WindowSession,
    };

    fn with_state_home<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
        with_state_home_for_test(dir, f)
    }

    /// Two windows with DIFFERENT chrome — the whole point of the per-window
    /// move, and the shape a v2 file could never express.
    fn sample_session() -> Session {
        Session {
            preview_theme: "sepia".to_string(),
            windows: vec![
                WindowSession {
                    width: 1111,
                    height: 222,
                    zoom_level: 1.5,
                    active_tab: 1,
                    chrome: ChromeSession {
                        show_toolbar: false,
                        show_statusbar: true,
                        outline_visible: false,
                        annotations_visible: true,
                        toolbar_sections: ToolbarSections {
                            file: true,
                            edit: false,
                            format: true,
                            view: true,
                            split: false,
                            zoom: true,
                        },
                    },
                    tabs: vec![
                        TabSession {
                            path: Some("/tmp/a.md".into()),
                            doc_id: None,
                            view_mode: ViewMode::Edit,
                            split_swap: false,
                            split_vertical: false,
                            show_unsafe_images: false,
                        },
                        TabSession {
                            path: None,
                            doc_id: None,
                            view_mode: ViewMode::Split,
                            split_swap: true,
                            split_vertical: true,
                            show_unsafe_images: true,
                        },
                    ],
                },
                WindowSession {
                    width: 800,
                    height: 600,
                    zoom_level: 1.0,
                    active_tab: 0,
                    // Deliberately the OPPOSITE of window 0's chrome on every
                    // field, so any code that collapses the two windows onto one
                    // shared answer fails here instead of coincidentally passing.
                    chrome: ChromeSession {
                        show_toolbar: true,
                        show_statusbar: false,
                        outline_visible: true,
                        annotations_visible: false,
                        toolbar_sections: ToolbarSections {
                            file: false,
                            edit: true,
                            format: false,
                            view: false,
                            split: true,
                            zoom: false,
                        },
                    },
                    tabs: vec![TabSession::default()],
                },
            ],
        }
    }

    /// The state directory must resolve on EVERY supported platform from the real
    /// environment — no `XDG_STATE_HOME` override in sight (ScrAP-167).
    ///
    /// This is the shape of test that would actually have caught it. A
    /// `#[cfg(windows)]` test asserting on a path would not have: the failure was
    /// that no path was produced *at all*, silently, because the only fallback was
    /// `HOME` — a POSIX convention Windows does not set. Every other session test
    /// goes through `with_state_home_for_test`, which sets `XDG_STATE_HOME` and so
    /// masks exactly this bug.
    #[test]
    fn state_directory_resolves_without_any_xdg_override() {
        let _g = super::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("XDG_STATE_HOME");
        std::env::remove_var("XDG_STATE_HOME");
        let resolved = session_path();
        match prev {
            Some(v) => std::env::set_var("XDG_STATE_HOME", v),
            None => std::env::remove_var("XDG_STATE_HOME"),
        }
        assert!(
            resolved.is_some(),
            "with XDG_STATE_HOME unset the platform fallback must still yield a \
             state path; returning None here means session save/load silently \
             no-ops and nothing persists (ScrAP-167)"
        );
    }

    #[test]
    fn save_then_load_round_trips_through_the_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            let s = sample_session();
            save(&s);
            assert!(
                session_path().unwrap().exists(),
                "save() should create <state>/scribobulate/session.toml"
            );
            assert_eq!(load(), s);
        });
    }

    #[test]
    fn frozen_save_is_suppressed_and_thaw_restores_it() {
        // TDD 15.10 regression: `quit_all_windows` snapshots all windows once and
        // freezes, so the per-window close sequence's `save`s must NOT overwrite the
        // snapshot with a shrinking set. A frozen save is a no-op; thawing resumes.
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            let full = sample_session();
            save(&full); // the upfront multi-window snapshot
            assert_eq!(load(), full);

            super::set_frozen(true);
            // A shrinking single-window write, as the last closing window would attempt.
            let shrunk = Session {
                preview_theme: full.preview_theme.clone(),
                windows: vec![full.windows[0].clone()],
            };
            save(&shrunk);
            assert_eq!(
                load(),
                full,
                "a frozen save must not overwrite the snapshot"
            );

            super::set_frozen(false);
            save(&shrunk);
            assert_eq!(load(), shrunk, "a thawed save persists normally");
        });
    }

    #[test]
    fn load_falls_back_to_default_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            // Nothing written under the fresh state dir → defaults: no windows,
            // so the caller falls back to a single blank welcome window.
            assert!(load().windows.is_empty());
        });
    }

    #[test]
    fn session_round_trips_through_toml() {
        let s = sample_session();
        let text = toml::to_string(&s).unwrap();
        let back: Session = toml::from_str(&text).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn partial_session_fills_defaults() {
        // A v3 file missing fields still loads (serde default), so an
        // older/edited state file never breaks startup. The `windows` key
        // (even empty) is what marks this as v2/v3 rather than a legacy file.
        let text = "windows = []\n";
        let s = parse(text);
        assert_eq!(s.preview_theme, Session::default().preview_theme);
        assert!(s.windows.is_empty());

        // A window entry with no `chrome` table at all fills the ChromeSession
        // default (toolbar/statusbar/outline shown, annotations hidden, sections
        // at their default — file/edit/view shown, format/split/zoom hidden).
        let s = parse("[[windows]]\nwidth = 900\n");
        assert_eq!(s.windows[0].width, 900);
        assert_eq!(s.windows[0].chrome, ChromeSession::default());
    }

    #[test]
    fn toolbar_sections_default_minimal_and_round_trip() {
        // Default (operator decision): file/edit/view shown; format/split/zoom hidden.
        assert_eq!(
            ToolbarSections::default().to_array(),
            [true, true, false, true, false, false]
        );

        // A window with no `toolbar_sections` table inherits that default (a file
        // predating the feature, or a fresh profile, opens with the short toolbar).
        let s = parse("[[windows]]\nwidth = 900\n");
        assert_eq!(
            s.windows[0].chrome.toolbar_sections,
            ToolbarSections::default()
        );

        // A partial table fills every omitted field from Default: an explicit
        // `edit = false` overrides, the omitted `file` inherits default-shown, and
        // the omitted `format` inherits default-hidden.
        let s =
            parse("[[windows]]\nwidth = 900\n[windows.chrome.toolbar_sections]\nedit = false\n");
        assert!(s.windows[0].chrome.toolbar_sections.file); // omitted → default true
        assert!(!s.windows[0].chrome.toolbar_sections.edit); // explicit false
        assert!(!s.windows[0].chrome.toolbar_sections.format); // omitted → default false

        // `set` by canonical ID mirrors the field (both a true→false and a
        // false→true flip), and the whole struct survives a TOML round-trip.
        let mut ts = ToolbarSections::default();
        ts.set("edit", false); // was default-shown
        ts.set("format", true); // was default-hidden
        assert_eq!(ts.to_array(), [true, false, true, true, false, false]);
        let text = toml::to_string(&Session {
            windows: vec![WindowSession {
                chrome: ChromeSession {
                    toolbar_sections: ts,
                    ..ChromeSession::default()
                },
                ..WindowSession::default()
            }],
            ..Session::default()
        })
        .unwrap();
        assert_eq!(parse(&text).windows[0].chrome.toolbar_sections, ts);
    }

    #[test]
    fn outline_visible_defaults_true_and_round_trips() {
        // Default true, so a session file predating the field (or missing it)
        // restores the always-shown behavior.
        assert!(ChromeSession::default().outline_visible);

        let s = parse("[[windows]]\nwidth = 900\n");
        assert!(
            s.windows[0].chrome.outline_visible,
            "missing key -> default true"
        );

        let s = parse("[[windows]]\nwidth = 900\n[windows.chrome]\noutline_visible = false\n");
        assert!(!s.windows[0].chrome.outline_visible);

        // Survives a full TOML round-trip alongside the rest of the schema.
        let text = toml::to_string(&Session {
            windows: vec![WindowSession {
                chrome: ChromeSession {
                    outline_visible: false,
                    ..ChromeSession::default()
                },
                ..WindowSession::default()
            }],
            ..Session::default()
        })
        .unwrap();
        assert!(!parse(&text).windows[0].chrome.outline_visible);
    }

    #[test]
    fn outline_visible_round_trips_through_the_filesystem() {
        // Save/load round-trip, modeled on
        // `save_then_load_round_trips_through_the_filesystem`.
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            let mut s = sample_session();
            s.windows[0].chrome.outline_visible = false;
            save(&s);
            assert!(!load().windows[0].chrome.outline_visible);

            s.windows[0].chrome.outline_visible = true;
            save(&s);
            assert!(load().windows[0].chrome.outline_visible);
        });
    }

    #[test]
    fn legacy_flat_session_restores_outline_visible_true() {
        // A pre-Phase-4 flat file predates the outline toggle's persistence
        // entirely -> restores shown, matching the always-shown behavior every
        // pre-fix session had.
        let s = parse("view_mode = \"edit\"\n");
        assert!(s.windows[0].chrome.outline_visible);
    }

    #[test]
    fn annotations_visible_defaults_false_and_round_trips() {
        // TDD 20.13: the annotations viewer's visibility persists per-window,
        // alongside outline_visible. Its default is HIDDEN, so a file with no key
        // (and a pre-annotations session file) restores hidden.
        assert!(!ChromeSession::default().annotations_visible);
        let s = parse("[[windows]]\nwidth = 900\n");
        assert!(
            !s.windows[0].chrome.annotations_visible,
            "missing key -> default false"
        );
        let s = parse("[[windows]]\nwidth = 900\n[windows.chrome]\nannotations_visible = true\n");
        assert!(s.windows[0].chrome.annotations_visible);

        // Survives a full save/load filesystem round-trip.
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            let mut s = sample_session();
            s.windows[0].chrome.annotations_visible = true;
            save(&s);
            assert!(load().windows[0].chrome.annotations_visible);
            s.windows[0].chrome.annotations_visible = false;
            save(&s);
            assert!(!load().windows[0].chrome.annotations_visible);
        });
    }

    #[test]
    fn legacy_file_restores_default_sections() {
        // A pre-feature flat file predates per-section visibility → its sections
        // fall to the current default (file/edit/view shown, format/split/zoom hidden).
        let s = parse("view_mode = \"edit\"\n");
        assert_eq!(
            s.windows[0].chrome.toolbar_sections,
            ToolbarSections::default()
        );
    }

    #[test]
    fn legacy_flat_session_migrates_to_one_window_one_tab() {
        let legacy = "\
width = 1234
height = 567
view_mode = \"split\"
show_toolbar = true
show_statusbar = false
zoom_level = 2.0
show_unsafe_images = true
split_swap = true
split_vertical = false
";
        let s = parse(legacy);
        assert_eq!(s.windows.len(), 1);
        let w = &s.windows[0];
        // v1 described one window, so its flat chrome IS that window's chrome.
        assert!(w.chrome.show_toolbar);
        assert!(!w.chrome.show_statusbar);
        assert_eq!(
            (w.width, w.height, w.zoom_level, w.active_tab),
            (1234, 567, 2.0, 0)
        );
        assert_eq!(w.tabs.len(), 1);
        let t = &w.tabs[0];
        assert_eq!(t.path, None);
        assert_eq!(t.view_mode, ViewMode::Split);
        assert!(t.split_swap);
        assert!(!t.split_vertical);
        assert!(t.show_unsafe_images);
    }

    #[test]
    fn legacy_session_missing_fields_still_migrates_via_its_own_defaults() {
        // An even older/hand-trimmed legacy file: only one field present.
        let s = parse("view_mode = \"edit\"\n");
        assert_eq!(s.windows.len(), 1);
        assert_eq!(s.windows[0].tabs[0].view_mode, ViewMode::Edit);
        assert_eq!(s.windows[0].width, super::config().window.width);
    }

    #[test]
    fn empty_file_migrates_as_legacy_with_all_defaults() {
        // An empty/corrupt file has no `windows` key either, so it takes the
        // legacy migration path rather than silently becoming zero windows.
        let s = parse("");
        assert_eq!(s.windows.len(), 1);
        assert_eq!(s.windows[0].tabs[0].path, None);
    }

    // ── v2 (app-wide chrome) → v3 (per-window chrome) migration ───────────────
    // The risk these pin is asymmetric: dropping a window is catastrophic and
    // dropping the chrome is cosmetic, so every case below asserts the windows
    // survive FIRST, then what happened to the chrome.

    /// A real v2 file: chrome at the top level, two windows carrying none.
    const V2_FILE: &str = "\
show_toolbar = false
show_statusbar = false
outline_visible = false
preview_theme = \"sepia\"

[toolbar_sections]
zoom = false

[[windows]]
width = 1111
height = 222
zoom_level = 1.5
active_tab = 0

[[windows.tabs]]
view_mode = \"edit\"

[[windows]]
width = 800
height = 600
zoom_level = 1.0
active_tab = 0

[[windows.tabs]]
view_mode = \"preview\"
";

    #[test]
    fn v2_file_keeps_every_window() {
        // The failure this exists to prevent is the expensive one: a session
        // file that no longer parses and silently drops every window is far
        // worse than the chrome bug being fixed. Removing four fields from
        // `Session` must not turn a v2 file into "no saved session".
        let s = parse(V2_FILE);
        assert_eq!(s.windows.len(), 2, "a v2 file must not lose a window");
        assert_eq!((s.windows[0].width, s.windows[0].height), (1111, 222));
        assert_eq!(s.windows[0].zoom_level, 1.5);
        assert_eq!(s.windows[1].width, 800);
        assert_eq!(s.windows[0].tabs[0].view_mode, ViewMode::Edit);
        assert_eq!(s.preview_theme, "sepia");
    }

    #[test]
    fn v2_app_wide_chrome_is_applied_to_every_window() {
        // The migration decision: a v2 session had ONE chrome answer that every
        // window rendered, so copying it onto each window reproduces exactly
        // what the user saw. Every field is asserted on BOTH windows — a
        // migration that only reached windows[0] would leave the second window
        // wrongly all-shown.
        let s = parse(V2_FILE);
        for (i, w) in s.windows.iter().enumerate() {
            assert!(!w.chrome.show_toolbar, "window {i}");
            assert!(!w.chrome.show_statusbar, "window {i}");
            assert!(!w.chrome.outline_visible, "window {i}");
            assert!(!w.chrome.toolbar_sections.zoom, "window {i}");
            // A key the v2 table omitted stays at its default (file is default-shown).
            assert!(w.chrome.toolbar_sections.file, "window {i}");
        }
    }

    #[test]
    fn v3_file_is_not_mistaken_for_v2_and_keeps_per_window_chrome() {
        // The migration must not fire on a file this crate wrote itself: the v3
        // writer emits no top-level chrome key, so `is_v2()` is false and each
        // window keeps its OWN value. `sample_session`'s two windows have
        // opposite chrome on every field, so a migration that wrongly fired
        // (or a reader that collapsed them) could not pass this.
        let text = toml::to_string(&sample_session()).unwrap();
        assert!(
            !text.starts_with("show_toolbar"),
            "the v3 writer must not emit top-level chrome: {text}"
        );
        assert_eq!(parse(&text), sample_session());
    }

    #[test]
    fn v2_migration_applies_only_the_keys_actually_present() {
        // An older v2 file predating `toolbar_sections`/`outline_visible` has
        // only some top-level keys. The present ones migrate; the absent ones
        // leave the per-window default alone rather than forcing `false`.
        let s = parse("show_toolbar = false\n[[windows]]\nwidth = 900\n");
        assert_eq!(s.windows.len(), 1);
        assert!(!s.windows[0].chrome.show_toolbar, "present key migrates");
        assert!(
            s.windows[0].chrome.show_statusbar,
            "absent key must not be forced off"
        );
        assert!(s.windows[0].chrome.outline_visible, "absent key");
        assert_eq!(
            s.windows[0].chrome.toolbar_sections,
            ToolbarSections::default(),
            "absent table"
        );
    }

    #[test]
    fn v2_file_with_no_windows_migrates_without_panicking() {
        // `windows = []` is the "v2, but nothing was open" shape — the
        // migration loop simply has nothing to apply to, and must not
        // manufacture a window (that would fabricate a phantom on restart).
        let s = parse("show_toolbar = false\nwindows = []\n");
        assert!(s.windows.is_empty());
    }

    #[test]
    fn v2_file_round_trips_to_v3_on_disk() {
        // End to end: a v2 file on disk loads, and re-saving it writes the v3
        // per-window shape that then reloads identically. Pins that
        // `toml::to_string` can actually serialize the nested
        // `[windows.chrome.toolbar_sections]` sub-table — a `ValueAfterTable`
        // error there would make `save` log-and-drop the whole session.
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            std::fs::create_dir_all(session_path().unwrap().parent().unwrap()).unwrap();
            std::fs::write(session_path().unwrap(), V2_FILE).unwrap();

            let migrated = load();
            assert_eq!(migrated.windows.len(), 2);
            assert!(!migrated.windows[1].chrome.show_toolbar);

            save(&migrated);
            let text = std::fs::read_to_string(session_path().unwrap()).unwrap();
            assert!(
                text.contains("[windows.chrome]"),
                "chrome must be written per window: {text}"
            );
            assert_eq!(
                load(),
                migrated,
                "the rewritten v3 file reloads identically"
            );
        });
    }

    #[test]
    fn two_windows_persist_independent_chrome() {
        // The scope change itself: two windows with different chrome survive a
        // save/load round trip as two different answers, which the app-wide
        // schema could not represent at all.
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            let s = sample_session();
            save(&s);
            let back = load();
            assert!(!back.windows[0].chrome.show_toolbar);
            assert!(back.windows[1].chrome.show_toolbar);
            assert!(back.windows[0].chrome.show_statusbar);
            assert!(!back.windows[1].chrome.show_statusbar);
            assert!(!back.windows[0].chrome.outline_visible);
            assert!(back.windows[1].chrome.outline_visible);
            assert_ne!(
                back.windows[0].chrome.toolbar_sections,
                back.windows[1].chrome.toolbar_sections
            );
        });
    }
}
