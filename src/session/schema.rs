//! The on-disk session SHAPE: the structs `session.toml` serialises to and from,
//! and the scope each value is held at.
//!
//! **Scope is the one thing to get right here**, and it is a decision per FIELD
//! rather than a property of the type:
//!
//! | Scope | Held by | Examples |
//! |---|---|---|
//! | App-wide | [`Session`] itself | reading theme, animations, split arrangement, toolbar layout |
//! | Per window | [`ChromeSession`] inside [`WindowSession`] | status bar, sidebars, geometry, zoom |
//! | Per tab | [`TabSession`] | path, view mode, unsafe images, find match options and history |
//!
//! A field filed at the wrong scope neither fails to compile nor fails a round
//! trip — it simply answers the wrong question later, and every such move so far
//! has cost a schema version (see [`super::migrate`]). Each type's scope note
//! records what was decided and why, so the next field lands beside a reason
//! rather than beside a guess.
//!
//! **FIELD ORDER IS LOAD-BEARING** in three of these structs — for TOML, not for
//! Rust: a table's scalar keys must all be emitted before any sub-table, or
//! `toml::to_string` fails with `ValueAfterTable` and [`super::save`] log-and-drops
//! the whole session. Each affected struct says so at its own declaration.

use crate::config::config;
use crate::winstate::ViewMode;
use std::path::PathBuf;

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
    /// This tab's own "Show Unsafe Images" toggle (per-tab, unlike zoom).
    pub show_unsafe_images: bool,
    /// This tab's find **match options** — case sensitivity, whole word, regular
    /// expression.
    ///
    /// **Additive, like `doc_id`**: the struct is `#[serde(default)]` and so is
    /// `FindOptions`, so a session file written before this field existed yields the
    /// default — a case-insensitive literal, which is the behaviour the find bar had
    /// before the options existed. No version bump and no migration function.
    ///
    /// **The live query is deliberately NOT here.** A committed query survives as the
    /// head of this tab's search history; restoring a search *in force* would put the
    /// reader into a search they did not just ask for, so the bar restores closed. The
    /// options are different in kind: they are how the reader reads, not what they are
    /// currently looking for.
    ///
    /// This tab's committed search terms, most recent first — the head of it is the
    /// last thing the reader searched for in this tab.
    ///
    /// **A history is restored; a SEARCH is not.** The find bar comes back closed with
    /// nothing in force, because restoring a search the reader did not just ask for is
    /// a different thing from keeping the terms reachable. Additive and lenient like
    /// `doc_id`: a malformed list is repaired on load (`FindHistory::sanitised`) rather
    /// than failing the session, which would cost every window and tab to protect a
    /// convenience.
    pub find_history: crate::window::FindHistory,
    /// The replacement-text counterpart of [`find_history`](Self::find_history).
    pub replace_history: crate::window::FindHistory,
    /// FIELD ORDER: this is a sub-TABLE, so it must stay LAST — a scalar emitted after
    /// it makes `toml::to_string` fail with `ValueAfterTable` and [`super::save`]
    /// log-and-drop the whole session (see this module's header).
    ///
    /// The two histories above serialise as ARRAYS, not tables, so they are scalars for
    /// this purpose and correctly precede it.
    pub find_options: crate::window::FindOptions,
}

/// One persisted window: its place on the desktop, its shared zoom level, its own
/// chrome visibility, and its tabs.
///
/// FIELD ORDER IS LOAD-BEARING for TOML: a table's scalar keys must all be
/// emitted before any sub-table, so the scalars come first, then `place`
/// (`[windows.place]`), then `chrome` (`[windows.chrome]`), then `tabs`
/// (`[[windows.tabs]]`). Re-ordering a table above a scalar makes
/// `toml::to_string` fail with `ValueAfterTable` and [`save`] log-and-drop the
/// whole session.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Clone, Debug)]
#[serde(default)]
pub(crate) struct WindowSession {
    pub zoom_level: f64,
    /// Index into `tabs` of the tab that was active when this window closed.
    pub active_tab: usize,
    /// Where the window sat on the desktop at quit (`winplace`): its default size,
    /// maximized and full screen everywhere, and on X11 its virtual desktop,
    /// activities, monitor, position and window states. Restored only by session
    /// restore; every other new window opens where the window manager puts it.
    ///
    /// Replaces the window's former top-level `width` / `height`, which recorded the
    /// window's ALLOCATION — a maximized window's full-screen size — rather than the
    /// size it returns to. A file written before this field has those two keys and no
    /// `place`; `migrate_pre_v7_size` carries them over.
    ///
    /// Read leniently ([`lenient_place`]): a table that does not read as a place costs
    /// this window its place, not the session every window.
    #[serde(deserialize_with = "lenient_place")]
    pub place: winplace::Place,
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
            zoom_level: 1.0,
            active_tab: 0,
            place: winplace::Place {
                width: config().window.width,
                height: config().window.height,
                ..winplace::Place::default()
            },
            chrome: ChromeSession::default(),
            tabs: vec![TabSession::default()],
        }
    }
}

/// A window's `place` table, or — when it does not read as a place, say a `desktop = -1`
/// a hand edit left — the default place keeping whatever integer size the table holds,
/// logged. Without this one bad key fails `WindowSession`, and with it the whole
/// `Session` parse, so every window and tab would be lost for the sake of one window's
/// position.
fn lenient_place<'de, D: serde::Deserializer<'de>>(d: D) -> Result<winplace::Place, D::Error> {
    use serde::Deserialize;
    let value = toml::Value::deserialize(d)?;
    match winplace::Place::deserialize(value.clone()) {
        Ok(place) => Ok(place),
        Err(e) => {
            log::warn!("session: a window's place could not be read ({e}); it keeps only its size");
            let default = WindowSession::default().place;
            let size = |key: &str| {
                value
                    .get(key)
                    .and_then(toml::Value::as_integer)
                    .and_then(|v| i32::try_from(v).ok())
            };
            Ok(winplace::Place {
                width: size("width").unwrap_or(default.width),
                height: size("height").unwrap_or(default.height),
                ..default
            })
        }
    }
}

/// Per-section toolbar visibility (the `S_i` states; see the
/// `window::toolbarchrome` invariants I1–I7). **App-wide**, exactly like the
/// `show_toolbar` it lives beside on [`Session`]. Each flag is the persisted
/// *state* `S_i` of the matching `app.show-tbtn-<id>` action; the *enabled*
/// attribute is always derived from `show_toolbar` at runtime (invariant I3) and
/// never stored. Field names are the canonical
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

    /// Whether `id` is the ONLY section currently shown — the predicate behind the
    /// last-section rule (TDD 9.22): hiding this one would leave an empty ~2px strip,
    /// so the request is reinterpreted as "hide the whole bar" instead.
    ///
    /// **Lives here, beside the flags, rather than in `window::toolbarchrome` where it
    /// is used.** It is the one genuine DECISION in that module — everything else there
    /// is widget and action plumbing — and `src/window/<name>.rs` is outside the
    /// coverage gate's measured set, so a decision left there is a decision nothing
    /// measures (POLICY § Build pipeline, coverage ratchet).
    ///
    /// An id that names no section answers `false`, including when every section is
    /// hidden. That is the safe direction in both cases: `false` takes the ORDINARY
    /// hide path, which is what the caller wants for a section that is not the last
    /// one, and an all-hidden bar is unreachable anyway (invariant I8 is what keeps it
    /// so).
    pub fn is_only_visible(self, id: &str) -> bool {
        let mut shown = crate::app::TBTN_SECTION_IDS
            .iter()
            .zip(self.to_array())
            .filter(|(_, on)| *on)
            .map(|(candidate, _)| *candidate);
        shown.next() == Some(id) && shown.next().is_none()
    }
}

/// One window's chrome visibility: its status bar and its two sidebar sections.
///
/// **Scope (operator decision): PER WINDOW.** Each field is the persisted state
/// of the matching `win.*` toggle action on ONE window — `win.show-statusbar`,
/// `win.outline`, `win.annotations`. Those actions always were per-window (each
/// handler only ever touched its own window's widgets); this type is where that
/// runtime truth is stored and persisted, so all three of storage, behaviour and
/// persistence give one answer.
///
/// **The TOOLBAR is no longer here.** `show_toolbar` and `toolbar_sections` moved
/// to [`Session`] in v6 (operator decision, 2026-09-21): a window is ephemeral, so
/// a toolbar layout scoped to one never feels saved — the same reasoning that moved
/// the split arrangement app-wide in v5. Both halves moved together because the
/// last-section rule couples them: unticking the final visible section reinterprets
/// as "hide the whole bar", and a per-window `show_toolbar` would apply that
/// reinterpretation in one window while leaving every other showing the empty strip
/// the rule exists to forbid.
///
/// Scope note (see the state-scope rule in the `winstate` module doc): this type is
/// for WINDOW-scoped state. App-wide state (`preview_theme` — one CSS provider, so
/// one value; the toolbar layout) and tab-scoped state (`show_unsafe_images`) do NOT
/// belong here; they have different owners and different inheritance rules.
///
/// FIELD ORDER IS LOAD-BEARING for TOML — see [`WindowSession`]'s note. Every field
/// here is now a scalar, so nothing may be added below that is not.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Clone, Copy, Debug)]
#[serde(default)]
pub(crate) struct ChromeSession {
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
    /// Where the reader last left the divider between the two sidebar sections, as
    /// the FRACTION of the sidebar's height given to the outline (TDD 20.21).
    ///
    /// A fraction rather than `GtkPaned`'s absolute px, for the same reason
    /// `SplitView` stores one: the ratio then survives a window resize, and a
    /// restore into a window that is not the height the value was recorded at —
    /// which a session restore routinely is, since the window geometry it is
    /// restored alongside can itself have been clamped by a smaller screen.
    ///
    /// Per window, like every other field here. Read through
    /// [`sidebar_divider_position`] rather than used directly, so a hand-edited or
    /// corrupt value cannot reach the layout (POLICY: a malformed file must never
    /// break layout, on the same principle that a malformed config never prevents
    /// startup). A session file predating this key restores to the even split.
    pub sidebar_split: f64,
}

impl Default for ChromeSession {
    /// A fresh window (and a session file with no `chrome` table) shows the
    /// statusbar and outline; the annotations viewer starts hidden
    /// (see `annotations_visible`) and the two sections split the sidebar evenly.
    ///
    /// `0.5` is also what `GtkPaned` derives on its own from the two sections'
    /// equal minimums when no position was ever set, so a restored default window
    /// and a never-persisted one look identical rather than merely similar.
    fn default() -> Self {
        Self {
            show_statusbar: true,
            outline_visible: true,
            annotations_visible: false,
            sidebar_split: 0.5,
        }
    }
}

/// The divider position in px for `fraction` of a sidebar `height` px tall, or
/// `None` when the sidebar has no usable height yet (unmapped, or hidden entirely)
/// and the question therefore has no answer worth acting on.
///
/// **This is the only route from the stored fraction to a `GtkPaned` position**, so
/// it is also where a value that never came from us is made harmless: a NaN, a
/// negative, a zero or a 3.7 all resolve to the even split rather than to a divider
/// jammed against an edge. Clamping at the point of USE rather than at parse is
/// deliberate — a file is not the only way a bad value can arrive, and a clamp on
/// the parse path alone would leave the arithmetic below trusting its input.
///
/// GTK clamps the result again to the two children's minimums (`shrink=false`), so
/// this function is not where the section floor is enforced — GTK4Rs/AP-317 is.
pub(crate) fn sidebar_divider_position(fraction: f64, height: i32) -> Option<i32> {
    if height <= 0 {
        return None;
    }
    let fraction = if fraction.is_finite() && fraction > 0.0 && fraction < 1.0 {
        fraction
    } else {
        ChromeSession::default().sidebar_split
    };
    Some((f64::from(height) * fraction).round() as i32)
}

/// The fraction to store for a divider sitting at `position` in a sidebar `height`
/// px tall, or `None` when the reading is not meaningful — an unmapped or hidden
/// sidebar (`height <= 0`), or a position GTK has pinned to an edge.
///
/// The `None` cases matter more than the arithmetic: a window closed with both
/// sidebar sections hidden has a zero-height sidebar, and reading `0.0` out of it
/// would overwrite a perfectly good remembered split with a degenerate one every
/// time — the reader would lose their layout by the act of hiding the sidebar
/// before quitting. Refusing to answer leaves the last good value standing.
pub(crate) fn sidebar_split_fraction(position: i32, height: i32) -> Option<f64> {
    if height <= 0 || position <= 0 || position >= height {
        return None;
    }
    Some(f64::from(position) / f64::from(height))
}

/// FIELD ORDER IS LOAD-BEARING for TOML — see [`WindowSession`]'s note. Every
/// scalar must stay above the `windows` array-of-tables.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
#[serde(default)]
pub(crate) struct Session {
    /// The selected preview reading theme's id (`src/theme/`). Genuinely
    /// app-wide, unlike the per-window chrome in [`ChromeSession`]: the theme is
    /// one app-wide CSS provider, so there is exactly one value and no
    /// "which window's?" question to answer. Defaults to the base theme, so an
    /// older session file with no `preview_theme` key restores today's
    /// desktop-derived appearance. An id naming a theme the user has since
    /// deleted resolves back to the base theme rather than failing.
    pub preview_theme: String,
    /// The reader's raw Play Animations choice (`app.play-animations` — TDD
    /// 27.5-27.7), genuinely app-wide like `preview_theme` above: one process-wide
    /// `GAction`, so one value. **Never** the effective (system "reduce
    /// animations"-adjusted) play state — that is recomputed live on every launch
    /// from `gtk-enable-animations`, so a system setting changed since the last run
    /// takes effect immediately rather than being baked into what was saved
    /// (`animation::policy`). `#[serde(default)]` on the struct means an older
    /// session file with no `play_animations` key restores
    /// [`crate::animation::policy::DEFAULT_CHOICE`] — on, matching a genuine first
    /// launch (TDD 27.6).
    pub play_animations: bool,
    /// Whether the split's editor and preview panes are swapped
    /// (`app.split-swap`), and whether the split is stacked top/bottom
    /// (`app.split-orientation`). App-wide like `preview_theme`: one preference for
    /// every tab of every window (TDD 7.3), since a value scoped to a short-lived
    /// tab or window never feels saved. A v3/v4 file carries them per window / per
    /// tab instead; [`migrate_pre_v5_split_arrangement`] recovers them.
    pub split_swap: bool,
    pub split_vertical: bool,
    /// Whether the whole toolbar is shown (`app.show-toolbar`). App-wide since v6,
    /// with [`toolbar_sections`](Self::toolbar_sections) below — see
    /// [`ChromeSession`] for why the two could not be split across scopes. A v3/v4/v5
    /// file carries it per window instead; [`migrate_pre_v6_toolbar`] recovers it.
    pub show_toolbar: bool,
    /// Which toolbar sections are shown (`app.show-tbtn-<id>`), app-wide since v6.
    ///
    /// A SUB-TABLE, so it must stay below every scalar above and above `windows`
    /// below — see [`WindowSession`]'s field-order note for what a misplacement costs.
    pub toolbar_sections: ToolbarSections,
    /// One entry per window open when the session was last saved. Empty means
    /// "no saved session yet" (fresh install) — `window::restore::restore_session`
    /// treats that as "fall back to the default single blank window".
    pub windows: Vec<WindowSession>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            preview_theme: crate::theme::SYSTEM_ID.to_string(),
            play_animations: crate::animation::policy::DEFAULT_CHOICE,
            split_swap: false,
            split_vertical: false,
            show_toolbar: true,
            toolbar_sections: ToolbarSections::default(),
            windows: Vec::new(),
        }
    }
}

/// A session with two windows whose chrome DIFFERS on every field — the shape a
/// v2 file could never express, and the reason a reader that collapses the two
/// cannot pass a round-trip test built on it. Shared with [`super::migrate`] and
/// [`super`], which both need a non-default session to write and read back.
#[cfg(test)]
pub(super) fn sample_session() -> Session {
    Session {
        preview_theme: "sepia".to_string(),
        play_animations: false,
        // Both non-default, so a writer that drops either fails the round trip.
        split_swap: true,
        split_vertical: true,
        // App-wide too, and both non-default for the same reason.
        show_toolbar: false,
        toolbar_sections: ToolbarSections {
            file: true,
            edit: false,
            format: true,
            view: true,
            split: false,
            zoom: true,
        },
        windows: vec![
            WindowSession {
                zoom_level: 1.5,
                active_tab: 1,
                // Every field of a place set (the booleans left `false` here are set in
                // window 1), so a part the session drops on the way through fails the
                // round trip.
                place: winplace::Place {
                    width: 1111,
                    height: 222,
                    maximized: true,
                    fullscreen: false,
                    monitor: Some("HDMI-0".into()),
                    x: Some(-12),
                    y: Some(34),
                    desktop: Some(2),
                    all_desktops: false,
                    activities: vec!["aaaa".into(), "bbbb".into()],
                    shaded: true,
                    above: true,
                    below: false,
                    no_border: true,
                },
                chrome: ChromeSession {
                    show_statusbar: true,
                    outline_visible: false,
                    annotations_visible: true,
                    sidebar_split: 0.25,
                },
                tabs: vec![
                    TabSession {
                        path: Some("/tmp/a.md".into()),
                        doc_id: None,
                        view_mode: ViewMode::Edit,
                        show_unsafe_images: false,
                        find_history: crate::window::FindHistory::default(),
                        replace_history: crate::window::FindHistory::default(),
                        find_options: crate::window::FindOptions::default(),
                    },
                    TabSession {
                        path: None,
                        doc_id: None,
                        view_mode: ViewMode::Split,
                        show_unsafe_images: true,
                        find_history: crate::window::FindHistory::default(),
                        replace_history: crate::window::FindHistory::default(),
                        find_options: crate::window::FindOptions {
                            case_sensitive: true,
                            whole_word: false,
                            regex: true,
                        },
                    },
                ],
            },
            WindowSession {
                zoom_level: 1.0,
                active_tab: 0,
                // The booleans window 0 leaves `false` are `true` here, so between the
                // two windows every field of a place is set.
                place: winplace::Place {
                    width: 800,
                    height: 600,
                    fullscreen: true,
                    all_desktops: true,
                    below: true,
                    ..winplace::Place::default()
                },
                // Deliberately the OPPOSITE of window 0's chrome on every
                // field, so any code that collapses the two windows onto one
                // shared answer fails here instead of coincidentally passing.
                chrome: ChromeSession {
                    show_statusbar: false,
                    outline_visible: true,
                    annotations_visible: false,
                    sidebar_split: 0.75,
                },
                tabs: vec![TabSession::default()],
            },
        ],
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::with_state_home_for_test as with_state_home;
    use crate::session::{load, parse, save};

    #[test]
    fn session_round_trips_through_toml() {
        let s = sample_session();
        let text = toml::to_string(&s).unwrap();
        let back: Session = toml::from_str(&text).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn a_place_that_does_not_read_costs_that_window_its_place_not_the_session() {
        let mut s = sample_session();
        s.windows[0].place.desktop = Some(1);
        let text = toml::to_string(&s)
            .unwrap()
            .replacen("desktop = 1", "desktop = -1", 1);
        let back = parse(&text);
        assert_eq!(back.windows.len(), 2, "both windows survive");
        assert_eq!(back.windows[0].tabs, s.windows[0].tabs);
        assert_eq!(back.windows[1], s.windows[1]);
        // The bad window keeps its size and nothing else of its place.
        assert_eq!(
            back.windows[0].place,
            winplace::Place {
                width: 1111,
                height: 222,
                ..WindowSession::default().place
            }
        );
    }

    #[test]
    fn partial_session_fills_defaults() {
        // A v3 file missing fields still loads (serde default), so an
        // older/edited state file never breaks startup. The `windows` key
        // (even empty) is what marks this as v2/v3 rather than a legacy file.
        let text = "windows = []\n";
        let s = parse(text);
        assert_eq!(s.preview_theme, Session::default().preview_theme);
        // An older session file with no `play_animations` key restores ON — a
        // first-launch-equivalent default (TDD 27.6), not the derived bool zero
        // value `#[serde(default)]` would silently substitute if this constant and
        // `Session::default()` ever disagreed.
        assert_eq!(s.play_animations, crate::animation::policy::DEFAULT_CHOICE);
        assert!(s.play_animations, "absent field must restore ON, not off");
        assert!(s.windows.is_empty());

        // A window entry with no `chrome` table at all fills the ChromeSession
        // default (toolbar/statusbar/outline shown, annotations hidden, sections
        // at their default — file/edit/view shown, format/split/zoom hidden).
        let s = parse("[[windows]]\nwidth = 900\n");
        assert_eq!(s.windows[0].place.width, 900);
        assert_eq!(s.windows[0].chrome, ChromeSession::default());
    }

    #[test]
    fn toolbar_sections_default_minimal_and_round_trip() {
        // Default (operator decision): file/edit/view shown; format/split/zoom hidden.
        assert_eq!(
            ToolbarSections::default().to_array(),
            [true, true, false, true, false, false]
        );

        // A file with no `toolbar_sections` table inherits that default (a file
        // predating the feature, or a fresh profile, opens with the short toolbar).
        let s = parse("[[windows]]\nwidth = 900\n");
        assert_eq!(s.toolbar_sections, ToolbarSections::default());
        assert!(s.show_toolbar, "an absent whole-bar toggle restores shown");

        // A partial table fills every omitted field from Default: an explicit
        // `edit = false` overrides, the omitted `file` inherits default-shown, and
        // the omitted `format` inherits default-hidden.
        let s = parse("[[windows]]\nwidth = 900\n[toolbar_sections]\nedit = false\n");
        assert!(s.toolbar_sections.file); // omitted → default true
        assert!(!s.toolbar_sections.edit); // explicit false
        assert!(!s.toolbar_sections.format); // omitted → default false

        // `set` by canonical ID mirrors the field (both a true→false and a
        // false→true flip), and the whole struct survives a TOML round-trip.
        let mut ts = ToolbarSections::default();
        ts.set("edit", false); // was default-shown
        ts.set("format", true); // was default-hidden
        assert_eq!(ts.to_array(), [true, false, true, true, false, false]);
        let text = toml::to_string(&Session {
            show_toolbar: false,
            toolbar_sections: ts,
            windows: vec![WindowSession::default()],
            ..Session::default()
        })
        .unwrap();
        let back = parse(&text);
        assert_eq!(back.toolbar_sections, ts);
        assert!(!back.show_toolbar);
    }

    #[test]
    fn sidebar_split_defaults_to_an_even_split_and_round_trips() {
        assert_eq!(ChromeSession::default().sidebar_split, 0.5);

        let s = parse("[[windows]]\nwidth = 900\n");
        assert_eq!(
            s.windows[0].chrome.sidebar_split, 0.5,
            "a file predating the divider restores the even split"
        );

        let s = parse("[[windows]]\nwidth = 900\n[windows.chrome]\nsidebar_split = 0.3\n");
        assert_eq!(s.windows[0].chrome.sidebar_split, 0.3);

        let text = toml::to_string(&Session {
            windows: vec![WindowSession {
                chrome: ChromeSession {
                    sidebar_split: 0.75,
                    ..ChromeSession::default()
                },
                ..WindowSession::default()
            }],
            ..Session::default()
        })
        .unwrap();
        assert_eq!(parse(&text).windows[0].chrome.sidebar_split, 0.75);
    }

    /// The stored fraction reaches the layout only through `sidebar_divider_position`,
    /// so that is where a value we did not write has to become harmless — a
    /// hand-edited or corrupt session file must not be able to jam the divider
    /// against an edge, or drive it off the pane entirely.
    #[test]
    fn a_corrupt_sidebar_split_falls_back_to_the_even_split() {
        for bad in [f64::NAN, f64::INFINITY, -1.0, 0.0, 1.0, 3.7] {
            assert_eq!(
                sidebar_divider_position(bad, 400),
                Some(200),
                "a stored {bad} must resolve to the even split, not to itself"
            );
        }
        // ...while a sane value is honoured rather than flattened along with them.
        assert_eq!(sidebar_divider_position(0.25, 400), Some(100));
    }

    /// Both directions decline to answer when the sidebar has no height to express a
    /// fraction against. This is not defensiveness: a window closed with BOTH sidebar
    /// sections hidden has exactly that shape, and answering `0.0` there would
    /// overwrite the reader's remembered split every time they tidied the sidebar away
    /// before quitting.
    #[test]
    fn a_sidebar_with_no_height_yields_no_split_reading() {
        assert_eq!(sidebar_divider_position(0.5, 0), None);
        assert_eq!(sidebar_divider_position(0.5, -10), None);
        assert_eq!(sidebar_split_fraction(120, 0), None);
        // A position pinned to either edge is GTK reporting a degenerate layout, not a
        // ratio the reader chose.
        assert_eq!(sidebar_split_fraction(0, 400), None);
        assert_eq!(sidebar_split_fraction(400, 400), None);
        assert_eq!(sidebar_split_fraction(100, 400), Some(0.25));
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
    fn is_only_visible_answers_the_last_section_question() {
        // The predicate behind the last-section rule (TDD 9.22), unit-tested here
        // because its caller (`window::toolbarchrome`) is outside the coverage gate.
        let none = ToolbarSections {
            file: false,
            edit: false,
            format: false,
            view: false,
            split: false,
            zoom: false,
        };

        // Exactly one shown, and it is the one asked about.
        let mut one = none;
        one.set("format", true);
        assert!(one.is_only_visible("format"));
        // ...and it is NOT any other section, including one that is hidden.
        assert!(!one.is_only_visible("file"));

        // Two shown: neither is "the last", so both take the ordinary hide path.
        let mut two = one;
        two.set("zoom", true);
        assert!(!two.is_only_visible("format"));
        assert!(!two.is_only_visible("zoom"));

        // The shipped default has three shown, so none of them is last.
        for id in crate::app::TBTN_SECTION_IDS {
            assert!(
                !ToolbarSections::default().is_only_visible(id),
                "{id} must not read as the last section under the default layout"
            );
        }

        // Neither an unknown id nor an all-hidden bar may answer true — an unknown
        // id must take the ordinary path, and an all-hidden bar is unreachable.
        assert!(!one.is_only_visible("nosuchsection"));
        assert!(!none.is_only_visible("file"));
        assert!(!none.is_only_visible("nosuchsection"));
    }
}
