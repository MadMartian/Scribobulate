//! Reading session files written by SUPERSEDED schema versions.
//!
//! ## The shape every migration here shares
//!
//! Read this once and the per-version sections below need only say what MOVED.
//!
//! There are two kinds. A **v1** file is a wholly different shape, so it REPLACES
//! the parse ([`from_v1`]). Every later shape is the current one plus keys that
//! have since moved, so it is parsed normally and then ADDED to
//! ([`apply_superseded`]) — serde ignores unknown fields, so the relocated keys
//! simply drop out of the ordinary parse rather than failing it, which would
//! discard every window to recover a chrome flag.
//!
//! Each additive migration is driven by its own SEPARATE, infallible, all-`Option`
//! re-parse of the same text. Two properties follow, and together they are why
//! there is no "which version is this file?" test anywhere below:
//!
//! - **`Option` makes ABSENT distinguishable from `false`.** A file predating one
//!   of an old shape's keys migrates the keys it actually has, instead of reading a
//!   missing key as "switched off". Give any of these fields a derived `bool`
//!   default and an old file starts hiding chrome the user never hid. It also means
//!   a file that cannot be read for its old chrome loses only the chrome, never a
//!   window.
//! - **A migration is therefore INERT on a file the current writer produced**,
//!   which carries none of the superseded keys. Calling them all unconditionally is
//!   already a no-op there, so a version test would be unfalsifiable decoration.
//!
//! The corollary is what to watch when adding a schema version: that inertness
//! holds only while the current writer emits none of the keys a reader here looks
//! for. Reintroducing an old key NAME at the level an old shape used it — which v6
//! did, deliberately, for the toolbar — makes that reader fire on files we write
//! ourselves. That is safe only while the two agree exactly, and the honest
//! response is to delete the redundant reader rather than to reason about the
//! overlap.
//!
//! ## v1 — one flat window, no `windows` key
//!
//! The v1 schema described a single window with a single tab. It migrates to one
//! window with one tab (no path — v1 never recorded one), carrying over its flat
//! width/height/zoom/view-mode/split/unsafe-images fields. [`from_v1`] carries why
//! the detection is a direct test for the `windows` key rather than a hopeful
//! parse.
//!
//! ## v2 — chrome at the top level, app-wide
//!
//! v2 kept `show_toolbar` / `show_statusbar` / `toolbar_sections` /
//! `outline_visible` at the TOP level. The status bar and outline are now per
//! window, so [`migrate_v2_app_wide_chrome`] copies those two onto EVERY restored
//! window — the only answer that reproduces what the user actually saw, since a v2
//! session had exactly one chrome answer for all of its windows and no per-window
//! value was ever recorded. The other two need no migration at all: v6 put the
//! toolbar back at the top level under the same names, so they parse straight into
//! [`Session`].
//!
//! ## v3, v4 — the split arrangement per window / per tab
//!
//! v3 kept both halves per TAB; v4 moved the pane order (`split_swap`) into each
//! window's `chrome` and left the orientation (`split_vertical`) per tab. Both are
//! now one app-wide value, because tabs and windows are both short-lived and a
//! preference that dies with either never feels saved.
//! [`migrate_pre_v5_split_arrangement`] recovers it from **the first window's
//! ACTIVE tab** — the arrangement the user was last looking at, the writer listing
//! windows most-recently-focused first (`app.windows()`). Never a majority vote
//! across windows or tabs: that would sometimes restore an arrangement the user was
//! not looking at.
//!
//! ## v3, v4, v5 — the toolbar per window
//!
//! v3–v5 kept `show_toolbar` and `toolbar_sections` in each window's `chrome`
//! table. Both are now one app-wide value, for the reason that moved the split
//! arrangement in v5. [`migrate_pre_v6_toolbar`] recovers them from **the first
//! window**, on the same most-recently-focused rule and for the same reason.
//!
//! ## v2–v6 — the window's size at its own top level
//!
//! v2–v6 kept each window's `width` and `height` as scalars of its `[[windows]]`
//! table. They are now the size inside that window's `place` (`winplace::Place`),
//! which also carries where the window sat on the desktop.
//! [`migrate_pre_v7_size`] copies each window's pair across, by position. The old
//! value was the window's allocation, so a window saved maximized comes back at its
//! maximized size, un-maximized, once — the defect the place exists to fix, and no
//! better answer is recoverable from a file that never recorded the size it returns
//! to.

use super::schema::{ChromeSession, Session, TabSession, ToolbarSections, WindowSession};
use crate::config::config;
use crate::winstate::ViewMode;

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
            // A legacy file predates Play Animations entirely too, so it restores
            // the same default a genuine first launch gets (TDD 27.6).
            play_animations: crate::animation::policy::DEFAULT_CHOICE,
            // v1 described one window with one tab, so its flat arrangement IS the
            // app's — no "which window's?" question to answer.
            split_swap: l.split_swap,
            split_vertical: l.split_vertical,
            // v1 described one window, so its flat `show_toolbar` IS the app's — the
            // same no-question-to-answer reasoning as the arrangement above. A
            // pre-toolbar-sections file predates per-section visibility, so its
            // sections fall to the current default (file/edit/view shown;
            // format/split/zoom hidden).
            show_toolbar: l.show_toolbar,
            toolbar_sections: ToolbarSections::default(),
            windows: vec![WindowSession {
                place: winplace::Place {
                    width: l.width,
                    height: l.height,
                    ..winplace::Place::default()
                },
                zoom_level: l.zoom_level,
                active_tab: 0,
                // v1 described exactly ONE window, so its flat chrome fields are
                // that window's own chrome — no app-wide-to-per-window question
                // to answer here (unlike the v2 migration below).
                chrome: ChromeSession {
                    show_statusbar: l.show_statusbar,
                    // A legacy file predates the outline sidebar's persistence
                    // entirely, so it restores shown — the behavior every
                    // pre-fix session already had (the toggle was never
                    // persisted, so a window always opened with the outline
                    // visible).
                    outline_visible: true,
                    // A legacy file predates the annotations viewer entirely, so
                    // it restores hidden — its default.
                    annotations_visible: false,
                    // ...and therefore predates a divider between the two sidebar
                    // sections, there having been only one. The even split is the
                    // default, and the first drag records a real one.
                    sidebar_split: ChromeSession::default().sidebar_split,
                },
                tabs: vec![TabSession {
                    path: None,
                    // A legacy file predates crash recovery entirely, so the restored
                    // tab keeps the fresh id it was born with.
                    doc_id: None,
                    view_mode: l.view_mode,
                    show_unsafe_images: l.show_unsafe_images,
                    // A legacy file predates the find match options and the search
                    // history too, so the restored tab reads as the case-insensitive
                    // literal the bar was fixed at when that file was written, with
                    // nothing to offer in either drop-down.
                    find_history: crate::window::FindHistory::default(),
                    replace_history: crate::window::FindHistory::default(),
                    find_options: crate::window::FindOptions::default(),
                }],
            }],
        }
    }
}

/// Deserialize-only view of the v2 TOP-LEVEL chrome keys that are **still**
/// per-window today, kept solely as a [`parse`] migration source — no version of
/// this crate writes these two keys at the top level any more (they live per
/// window, in [`ChromeSession`]).
///
/// v2's other two top-level chrome keys — `show_toolbar` and `toolbar_sections` —
/// are deliberately **absent here**, and that is not an omission. v6 put both back
/// at the top level under the same names, so a v2 file's values deserialize
/// straight into [`Session`] with no migration step at all; reading them here too
/// would be a second path writing the same fields from the same bytes. The round
/// trip is exact because v2's app-wide answer is precisely what v6 wants.
///
/// **Every field must stay `Option`.** That is the whole safety property, and it
/// is doing all of the work: `Option` makes ABSENT distinguishable from `false`,
/// which (a) lets a v2 file that predates `outline_visible` migrate only the keys
/// it actually has, and (b) makes [`migrate_v2_app_wide_chrome`] inherently a
/// NO-OP on a v3 file, which carries neither key — so no "is this a v2 file?" test
/// is needed, or even meaningful. Give any field a derived `bool` default instead
/// and a v3 file reads as "a v2 file with everything switched off", hiding every
/// window's chrome on the next launch.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct V2AppWideChrome {
    show_statusbar: Option<bool>,
    outline_visible: Option<bool>,
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
/// It runs AFTER `windows` has been parsed and only ever OVERWRITES chrome, so a
/// v2 file whose old chrome keys are somehow unreadable degrades to the default
/// chrome and keeps every window — never the reverse.
fn migrate_v2_app_wide_chrome(session: &mut Session, v2: &V2AppWideChrome) {
    for w in &mut session.windows {
        if let Some(on) = v2.show_statusbar {
            w.chrome.show_statusbar = on;
        }
        if let Some(on) = v2.outline_visible {
            w.chrome.outline_visible = on;
        }
    }
}

/// Deserialize-only view of a v3/v4 file's per-window and per-tab split
/// arrangement keys, kept solely as a [`parse`] migration source — no version of
/// this crate writes them any more (the arrangement is app-wide, on [`Session`]).
///
/// Every leaf is an `Option`, for the same reason as [`V2AppWideChrome`]'s fields:
/// a file predating a key migrates without inventing a value, and reading this off
/// a v5 file (none of these keys) is a harmless all-`None` no-op.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct PreV5Session {
    windows: Vec<PreV5Window>,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct PreV5Window {
    active_tab: usize,
    /// v4's per-window pane order.
    chrome: PreV5Chrome,
    tabs: Vec<PreV5Tab>,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct PreV5Chrome {
    split_swap: Option<bool>,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct PreV5Tab {
    /// v3's per-tab pane order.
    split_swap: Option<bool>,
    /// v3/v4's per-tab orientation.
    split_vertical: Option<bool>,
}

/// Recover the app-wide split arrangement from a v3/v4 file, in the shape of
/// [`migrate_v2_app_wide_chrome`] above.
///
/// **The source is the FIRST window's ACTIVE tab** — the arrangement on screen in
/// the window the user used last (the writer iterates `app.windows()`, which GTK
/// orders most-recently-focused first). Never a majority vote across windows or
/// tabs: that would sometimes restore an arrangement the user was not looking at.
/// The pane order prefers v4's window-level value and falls back to v3's per-tab
/// one. An out-of-range `active_tab` (a hand-edited file) still takes a v4 window's
/// pane order, but fabricates no orientation.
fn migrate_pre_v5_split_arrangement(session: &mut Session, old: &PreV5Session) {
    let Some(window) = old.windows.first() else {
        return;
    };
    let active = window.tabs.get(window.active_tab);
    if let Some(on) = window
        .chrome
        .split_swap
        .or_else(|| active.and_then(|t| t.split_swap))
    {
        session.split_swap = on;
    }
    if let Some(on) = active.and_then(|t| t.split_vertical) {
        session.split_vertical = on;
    }
}

/// Deserialize-only view of a v3/v4/v5 file's PER-WINDOW toolbar keys, kept solely
/// as a [`parse`] migration source — the toolbar is app-wide now, on [`Session`].
///
/// A separate reader from [`PreV5Session`] rather than two more fields on it,
/// because the two describe different historical shapes and are read for different
/// fields. Sharing one struct would make each migration's "is this key absent?"
/// no-op property depend on the other's, which is the coupling that makes a
/// migration chain hard to reason about later.
///
/// Every leaf is an `Option`, for the same reason as [`V2AppWideChrome`]'s fields:
/// a file predating a key migrates without inventing a value, and reading this off
/// a v6 file (no per-window toolbar key) is a harmless all-`None` no-op.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct PreV6Session {
    windows: Vec<PreV6Window>,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct PreV6Window {
    chrome: PreV6Chrome,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct PreV6Chrome {
    show_toolbar: Option<bool>,
    toolbar_sections: Option<ToolbarSections>,
}

/// Recover the app-wide toolbar layout from a v3/v4/v5 file, in the shape of
/// [`migrate_pre_v5_split_arrangement`] above.
///
/// **The source is the FIRST window** — the one the user was last in, since the
/// writer iterates `app.windows()` and GTK orders that most-recently-focused
/// first. Never a majority vote across windows: that would sometimes restore a
/// toolbar the user was not looking at. The two halves are taken independently, so
/// a file predating per-section visibility still migrates its whole-bar toggle.
fn migrate_pre_v6_toolbar(session: &mut Session, old: &PreV6Session) {
    let Some(window) = old.windows.first() else {
        return;
    };
    if let Some(on) = window.chrome.show_toolbar {
        session.show_toolbar = on;
    }
    if let Some(sections) = window.chrome.toolbar_sections {
        session.toolbar_sections = sections;
    }
}

/// A v2–v6 file's per-window size, which v7 moved into each window's `place`. Both
/// halves are `Option` so a window carrying neither key (every v7 file) is left alone.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct PreV7Session {
    windows: Vec<PreV7Window>,
}

/// The size keys as raw values, so one that is not an integer size costs only that
/// window that half of its size, logged, rather than failing this whole parse and with
/// it, silently, every window's size.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct PreV7Window {
    width: Option<toml::Value>,
    height: Option<toml::Value>,
    /// Present on every window a v7 file wrote.
    place: Option<serde::de::IgnoredAny>,
}

/// Copy each window's old top-level `width` / `height` into its `place`, pairing
/// windows by position (the same list, parsed twice). Inert on a v7 file: a window with
/// a `place` table keeps the place's size, so a stray top-level key a hand edit left
/// behind is not what wins.
fn migrate_pre_v7_size(session: &mut Session, old: &PreV7Session) {
    let windows = session.windows.iter_mut().zip(&old.windows).enumerate();
    for (index, (window, sizes)) in windows {
        if sizes.place.is_some() {
            continue;
        }
        if let Some(width) = sizes
            .width
            .as_ref()
            .and_then(|v| pre_v7_size(v, index, "width"))
        {
            window.place.width = width;
        }
        if let Some(height) = sizes
            .height
            .as_ref()
            .and_then(|v| pre_v7_size(v, index, "height"))
        {
            window.place.height = height;
        }
    }
}

/// `value` as a size in pixels, or `None`, logged, when it is not one.
fn pre_v7_size(value: &toml::Value, window: usize, key: &str) -> Option<i32> {
    let size = value.as_integer().and_then(|v| i32::try_from(v).ok());
    if size.is_none() {
        log::warn!(
            "session: window {window}'s pre-v7 `{key}` is not an integer size (a {}); \
             that window opens at the default {key}",
            value.type_str()
        );
    }
    size
}

/// A v1 file migrated to a [`Session`], or `None` if this is not a v1 file.
///
/// **The v1 signal is the ABSENCE of a top-level `windows` key**, tested directly
/// rather than by trying both parses and hoping the wrong one fails cleanly. It
/// would not: [`Session`] is `#[serde(default)]`, so it "succeeds" on a v1 file by
/// treating every v1 field as unknown-and-ignored and filling `windows` from
/// `Default` — an empty list — which discards the old session's window size, zoom
/// and view mode entirely instead of migrating them. Every version this crate has
/// ever written includes the key, even when empty (see [`super::save`]).
pub(super) fn from_v1(text: &str) -> Option<Session> {
    let has_windows = toml::from_str::<toml::Table>(text)
        .map(|t| t.contains_key("windows"))
        .unwrap_or(false);
    if has_windows {
        return None;
    }
    Some(
        toml::from_str::<LegacySession>(text)
            .map(Session::from)
            .unwrap_or_default(),
    )
}

/// Apply every ADDITIVE migration to an already-parsed `session`, recovering the
/// keys each superseded shape kept somewhere else.
///
/// **The single entry point, and the single place a new schema version is wired
/// in.** Each call below is unconditional because each is inert on a file that does
/// not carry its keys — see the module doc for why that makes a version test
/// unfalsifiable decoration rather than a missing safeguard.
pub(super) fn apply_superseded(text: &str, session: &mut Session) {
    // v2 kept the status bar and outline at the top level, app-wide.
    let v2 = toml::from_str::<V2AppWideChrome>(text).unwrap_or_default();
    migrate_v2_app_wide_chrome(session, &v2);
    // v3/v4 kept the split arrangement per window / per tab.
    let pre_v5 = toml::from_str::<PreV5Session>(text).unwrap_or_default();
    migrate_pre_v5_split_arrangement(session, &pre_v5);
    // v3/v4/v5 kept the toolbar per window. A v2 file needs nothing here: its
    // top-level toolbar keys were already read by the caller's ordinary `Session`
    // parse, which is exactly where v6 wants them (see `V2AppWideChrome`).
    let pre_v6 = toml::from_str::<PreV6Session>(text).unwrap_or_default();
    migrate_pre_v6_toolbar(session, &pre_v6);
    // v2–v6 kept each window's size at its own top level. Its values are read raw, so
    // this parse fails only where `windows` is not a list of tables, which the caller's
    // own `Session` parse has already failed on.
    let pre_v7 = toml::from_str::<PreV7Session>(text).unwrap_or_default();
    migrate_pre_v7_size(session, &pre_v7);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::schema::sample_session;
    use crate::session::with_state_home_for_test as with_state_home;
    use crate::session::{load, parse, save, session_path};

    #[test]
    fn legacy_flat_session_restores_outline_visible_true() {
        // A pre-Phase-4 flat file predates the outline toggle's persistence
        // entirely -> restores shown, matching the always-shown behavior every
        // pre-fix session had.
        let s = parse("view_mode = \"edit\"\n");
        assert!(s.windows[0].chrome.outline_visible);
    }

    #[test]
    fn legacy_file_restores_default_sections() {
        // A pre-feature flat file predates per-section visibility → its sections
        // fall to the current default (file/edit/view shown, format/split/zoom hidden).
        let s = parse("view_mode = \"edit\"\n");
        assert_eq!(s.toolbar_sections, ToolbarSections::default());
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
        // v1 described one window with one tab, so its flat arrangement IS the app's.
        assert!(s.split_swap);
        assert!(!s.split_vertical);
        // ...its flat `show_toolbar` IS the app's, for the same reason.
        assert!(s.show_toolbar);
        let w = &s.windows[0];
        // ...and its flat per-window chrome IS that window's chrome.
        assert!(!w.chrome.show_statusbar);
        assert_eq!(
            (w.place.width, w.place.height, w.zoom_level, w.active_tab),
            (1234, 567, 2.0, 0)
        );
        assert_eq!(w.tabs.len(), 1);
        let t = &w.tabs[0];
        assert_eq!(t.path, None);
        assert_eq!(t.view_mode, ViewMode::Split);
        assert!(t.show_unsafe_images);
    }

    #[test]
    fn legacy_session_missing_fields_still_migrates_via_its_own_defaults() {
        // An even older/hand-trimmed legacy file: only one field present.
        let s = parse("view_mode = \"edit\"\n");
        assert_eq!(s.windows.len(), 1);
        assert_eq!(s.windows[0].tabs[0].view_mode, ViewMode::Edit);
        assert_eq!(s.windows[0].place.width, super::config().window.width);
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
        assert_eq!(
            (s.windows[0].place.width, s.windows[0].place.height),
            (1111, 222)
        );
        assert_eq!(s.windows[0].zoom_level, 1.5);
        assert_eq!(s.windows[1].place.width, 800);
        assert_eq!(s.windows[0].tabs[0].view_mode, ViewMode::Edit);
        assert_eq!(s.preview_theme, "sepia");
    }

    #[test]
    fn v2_app_wide_chrome_is_applied_to_every_window() {
        // The migration decision for the two keys that are still per-window: a v2
        // session had ONE chrome answer that every window rendered, so copying it
        // onto each window reproduces exactly what the user saw. Asserted on BOTH
        // windows — a migration that only reached windows[0] would leave the second
        // window wrongly all-shown.
        let s = parse(V2_FILE);
        for (i, w) in s.windows.iter().enumerate() {
            assert!(!w.chrome.show_statusbar, "window {i}");
            assert!(!w.chrome.outline_visible, "window {i}");
        }
        // The toolbar needs no migration at all: v6 put it back at the top level
        // under the same names, so v2's app-wide answer parses straight into
        // `Session`. That is the whole reason `V2AppWideChrome` does not carry it.
        assert!(!s.show_toolbar);
        assert!(!s.toolbar_sections.zoom);
        // A key the v2 table omitted stays at its default (file is default-shown).
        assert!(s.toolbar_sections.file);
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
        // only some top-level keys. The present ones are read; the absent ones
        // leave the default alone rather than forcing `false`.
        let s = parse("show_toolbar = false\n[[windows]]\nwidth = 900\n");
        assert_eq!(s.windows.len(), 1);
        assert!(!s.show_toolbar, "present key is read");
        assert!(
            s.windows[0].chrome.show_statusbar,
            "absent key must not be forced off"
        );
        assert!(s.windows[0].chrome.outline_visible, "absent key");
        assert_eq!(
            s.toolbar_sections,
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
    fn v2_file_round_trips_to_the_current_shape_on_disk() {
        // End to end: a v2 file on disk loads, and re-saving it writes the current
        // shape — per-window `[windows.chrome]`, app-wide `[toolbar_sections]` —
        // which then reloads identically. Pins that `toml::to_string` can actually
        // serialize both sub-tables in the order the structs declare them; a
        // `ValueAfterTable` error would make `save` log-and-drop the whole session.
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            std::fs::create_dir_all(session_path().unwrap().parent().unwrap()).unwrap();
            std::fs::write(session_path().unwrap(), V2_FILE).unwrap();

            let migrated = load();
            assert_eq!(migrated.windows.len(), 2);
            assert!(!migrated.show_toolbar);

            save(&migrated);
            let text = std::fs::read_to_string(session_path().unwrap()).unwrap();
            assert!(
                text.contains("[windows.chrome]"),
                "per-window chrome must be written per window: {text}"
            );
            assert!(
                text.contains("[toolbar_sections]"),
                "the app-wide toolbar must be written at the top level: {text}"
            );
            assert_eq!(load(), migrated, "the rewritten file reloads identically");
        });
    }

    // ── v3/v4 (per-window / per-tab arrangement) → v5 (app-wide) migration ────
    // Same asymmetric-risk framing as the v2 section above — a lost window is
    // catastrophic, a mis-recovered arrangement is cosmetic.

    /// A real v4 file: two windows whose pane orders disagree, the FIRST (the one
    /// used last) swapped; its ACTIVE tab (index 1) vertical while a majority of
    /// its tabs and the whole second window are not — the shape that tells "first
    /// window's active tab" apart from a majority vote or a last-writer rule.
    const V4_FILE: &str = "\
[[windows]]
active_tab = 1

[windows.chrome]
split_swap = true

[[windows.tabs]]
split_vertical = false

[[windows.tabs]]
split_vertical = true

[[windows.tabs]]
split_vertical = false

[[windows]]
active_tab = 0

[windows.chrome]
split_swap = false

[[windows.tabs]]
split_vertical = false
";

    #[test]
    fn v4_arrangement_migrates_from_the_first_windows_active_tab() {
        let s = parse(V4_FILE);
        assert_eq!(s.windows.len(), 2, "a v4 file must not lose a window");
        assert!(
            s.split_swap,
            "pane order comes from the first window's chrome"
        );
        assert!(
            s.split_vertical,
            "orientation comes from the first window's ACTIVE tab, not the majority"
        );
    }

    #[test]
    fn v4_orientation_follows_a_different_active_tab() {
        // Re-pointed at tab 0 (not vertical): proves the migration INDEXES by
        // `active_tab` rather than picking any vertical tab.
        let s = parse(&V4_FILE.replacen("active_tab = 1", "active_tab = 0", 1));
        assert!(!s.split_vertical);
    }

    #[test]
    fn v3_per_tab_pane_order_migrates_from_the_active_tab() {
        // v3 had no window-level pane order: the active tab's own value is it.
        let v3 = "\
[[windows]]
active_tab = 1

[[windows.tabs]]
split_swap = false

[[windows.tabs]]
split_swap = true
split_vertical = true
";
        let s = parse(v3);
        assert!(s.split_swap && s.split_vertical);
        let s = parse(&v3.replace("active_tab = 1", "active_tab = 0"));
        assert!(!s.split_swap && !s.split_vertical);
    }

    #[test]
    fn an_out_of_range_active_tab_fabricates_no_orientation() {
        // A hand-edited file must not panic; the window-level pane order still
        // stands, and the orientation stays at its default.
        let s = parse(&V4_FILE.replacen("active_tab = 1", "active_tab = 99", 1));
        assert!(s.split_swap);
        assert!(!s.split_vertical);
    }

    #[test]
    fn pre_v6_per_window_toolbar_migrates_from_the_first_window() {
        // v3/v4/v5 kept the toolbar in each window's `chrome` table. The FIRST
        // window is the source — `app.windows()` is ordered most-recently-focused
        // first, so it is the bar the user was actually looking at. The second
        // window here disagrees on every flag, so a migration reading the wrong one
        // (or averaging them) cannot pass.
        let s = parse(
            "\
[[windows]]
width = 900

[windows.chrome]
show_toolbar = false

[windows.chrome.toolbar_sections]
zoom = true
file = false

[[windows]]
width = 800

[windows.chrome]
show_toolbar = true

[windows.chrome.toolbar_sections]
zoom = false
file = true
",
        );
        assert_eq!(s.windows.len(), 2, "migration must not cost a window");
        assert!(!s.show_toolbar, "the first window's whole-bar toggle wins");
        assert!(s.toolbar_sections.zoom, "...and its sections with it");
        assert!(!s.toolbar_sections.file);
        // A key the old table omitted still falls to the current default rather
        // than being invented (edit is default-shown, format default-hidden).
        assert!(s.toolbar_sections.edit);
        assert!(!s.toolbar_sections.format);
    }

    #[test]
    fn a_pre_v6_file_with_only_the_whole_bar_toggle_keeps_the_default_sections() {
        // The two halves migrate independently, so a file predating per-section
        // visibility still recovers its whole-bar toggle instead of losing both.
        let s = parse("[[windows]]\nwidth = 900\n[windows.chrome]\nshow_toolbar = false\n");
        assert!(!s.show_toolbar);
        assert_eq!(s.toolbar_sections, ToolbarSections::default());
    }

    #[test]
    fn v6_file_is_not_re_migrated_from_its_own_per_window_chrome() {
        // The migration must be inert on a file this crate wrote itself: the v6
        // writer emits no per-window toolbar key, so there is nothing to read back
        // and the app-wide value survives a write/read round trip unchanged.
        let text = toml::to_string(&sample_session()).unwrap();
        assert!(
            !text.contains("[windows.chrome.toolbar_sections]"),
            "the v6 writer must not emit a per-window toolbar table: {text}"
        );
        let back = parse(&text);
        assert_eq!(back.show_toolbar, sample_session().show_toolbar);
        assert_eq!(back.toolbar_sections, sample_session().toolbar_sections);
    }

    #[test]
    fn v5_file_round_trips_its_app_wide_arrangement() {
        // The migration must not fire on a file this crate wrote itself.
        let text = toml::to_string(&sample_session()).unwrap();
        assert_eq!(parse(&text), sample_session());
    }

    // ── v2–v6 (size at the window's top level) → v7 (`place`) ──────────────────

    #[test]
    fn a_pre_v7_windows_size_moves_into_its_place_window_by_window() {
        let s = parse(V2_FILE);
        assert_eq!(s.windows.len(), 2);
        assert_eq!(
            (s.windows[0].place.width, s.windows[0].place.height),
            (1111, 222)
        );
        assert_eq!(
            (s.windows[1].place.width, s.windows[1].place.height),
            (800, 600)
        );
        // Nothing else about the place was recorded, so nothing else is invented.
        assert_eq!(
            s.windows[0].place,
            winplace::Place {
                width: 1111,
                height: 222,
                ..winplace::Place::default()
            }
        );
    }

    #[test]
    fn a_v7_place_is_not_overwritten_by_the_size_migration() {
        // A v7 file carries the size only inside `place`, so a written file round-trips
        // through the reader untouched.
        let text = toml::to_string(&sample_session()).unwrap();
        let s = parse(&text);
        assert_eq!(s.windows[0].place, sample_session().windows[0].place);
    }

    #[test]
    fn a_stray_top_level_size_beside_a_place_does_not_win() {
        // A hand edit left the v6 keys on a window that also has its v7 place.
        let text = "windows = [\n\
            { width = 1111, height = 222, place = { width = 640, height = 480 } },\n\
            { width = 900, height = 700 },\n\
            ]\n";
        let s = parse(text);
        assert_eq!(
            (s.windows[0].place.width, s.windows[0].place.height),
            (640, 480)
        );
        // The window with no place still migrates.
        assert_eq!(
            (s.windows[1].place.width, s.windows[1].place.height),
            (900, 700)
        );
    }

    #[test]
    fn a_pre_v7_size_that_is_not_an_integer_costs_only_that_half_of_that_window() {
        let text = "windows = [\n\
            { width = 1111.0, height = 222 },\n\
            { width = 800, height = 99999999999 },\n\
            ]\n";
        let s = parse(text);
        assert_eq!(s.windows.len(), 2);
        assert_eq!(s.windows[0].place.height, 222);
        assert_eq!(s.windows[1].place.width, 800);
        // The unreadable halves keep the default a window with no size gets.
        let default = WindowSession::default().place;
        assert_eq!(s.windows[0].place.width, default.width);
        assert_eq!(s.windows[1].place.height, default.height);
    }
}
