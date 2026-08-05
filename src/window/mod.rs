use crate::app::{
    dialog_dir_for, remember_dialog_dir, EDIT_CMDS, FILE_CMDS, FORMAT_CMDS, VIEW_CMDS,
};
use crate::codeview::CodePreviewView;
use crate::config::config;
use crate::format::{self, FormatCmd};
use crate::outline::{build_tree, extract_headings};
use crate::outline_view::build_outline_content;
use crate::preview::{
    cell_search_targets, preview_scroll_fraction, preview_top_line, re_render,
    refresh_annotations_in_place, render, restore_preview_scroll, restore_preview_scroll_to_line,
    restore_preview_scroll_to_line_fresh, scroll_preview_to_heading,
};
use crate::winstate::{
    self, edit_actions_enabled, save_enabled, save_is_safe, state, FmtInsertKind, ScrollDriver,
    TabState, ViewMode,
};
use gtk::gdk;
use gtk::gio::SimpleAction;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, FileChooserAction, FileChooserNative, GestureClick, Label,
    ResponseType, TextView,
};
use sourceview::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

// window.rs was decomposed into focused submodules (see TECH.md module map).
mod actions;
mod annotate;
mod annotations_nav;
mod contextmenu;
mod editbar;
mod editor_annotate;
mod find;
mod outline_nav;
mod reload;
mod save;
mod scrollsync;
mod zoom;

use actions::*;
use editbar::*;
use find::*;
use reload::*;
use save::*;
use scrollsync::*;
use zoom::*;

// Cross-module entry points keep their `crate::window::NAME` paths.
pub(crate) use actions::connect_buf_to_copy_action;
pub(crate) use actions::focus_in_text_entry;
pub(crate) use actions::nested_submenu_app_stateful_action;
pub(crate) use actions::refresh_theme_button;
pub(crate) use actions::update_annotate_action_state;
pub(crate) use actions::update_save_action_state;
/// Re-exported so BOTH annotation comment cards can tag themselves with the same
/// const, for styling. The preview card lives outside this module, so without this
/// it can only reach the class by repeating the string — which is how the two
/// drifted into a coincidence-coupling in the first place.
pub(crate) use actions::ANNOTATION_CARD_CLASS;
pub(crate) use actions::{bool_action_state, change_action_state, set_action_enabled};
pub(crate) use annotate::apply_annotation_edit;
pub(crate) use annotate::document_source;
pub(crate) use annotate::refresh_preview_after_annotation;
use annotate::{register_annotate_action, register_next_annotation_action};
pub(crate) use annotations_nav::{reconcile_sidebar_visibility, refresh_annotations};
pub(crate) use chrome::render_and_wire_preview;
pub(crate) use contextmenu::attach_context_menu;
// The two find types `TabState` stores. The module itself stays private — the engine is
// window-internal; only the shapes the per-tab state has to *hold* are named crate-wide.
pub(crate) use find::{FindCursor, PreviewFindCache};
pub(crate) use findbar::refresh_preview_find_highlight;
pub(crate) use lifecycle::quit_all_windows;
pub(crate) use outline_nav::apply_scroll_spy;
pub(crate) use outline_nav::refresh_outline;
pub(crate) use outline_nav::wire_persistent_editor_scroll_spy;
pub(crate) use outline_nav::wire_scroll_spy;
pub(crate) use outline_nav::{outline_collapse_all, outline_expand_all};
pub(crate) use reload::check_and_reload;
pub(crate) use reload::check_and_reload_tab;
pub(crate) use restore::{apply_tab_layout, restore_session};
pub(crate) use save::refresh_dirty_status;
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) use swap::{clear_snapshot_failure_for_test, report_snapshot_failure_for_test};
pub(crate) use swap::{discard_tab_swap, sync_tab_swap, wire_swap_snapshots};
pub(crate) use swaprecovery::recover_after_restore;
pub(crate) use tabs::add_new_document_tab;
pub(crate) use tabs::badge_tab_label;
pub(crate) use tabs::create_tab_in_window;
pub(crate) use tabs::start_deferred_prerender_pump;
pub(crate) use tabs::{host_window, window_of_content_box};
pub(crate) use toast::sync_recovery_toast;
pub(crate) use zoom::rerender_preview_from_live_edit;
pub(crate) use zoom::rerender_tab_preview_in_place;

// Decomposed window sub-builders (see TECH.md module map).
mod chrome;
mod chrome_fit;
mod copylink;
mod editoractions;
mod findbar;
mod lifecycle;
mod linknav;
mod livepreview;
mod navhistory;
mod restore;
mod sidebar;
mod splitview;
mod swap;
mod swaprecovery;
mod tabs;
mod toast;
mod toolbar;
mod undo;
mod viewactions;

use chrome::*;
use copylink::*;
use editoractions::*;
use findbar::*;
use lifecycle::*;
pub(crate) use linknav::activate_doc_link;
use livepreview::*;
use navhistory::*;
pub(crate) use navhistory::{nav_action_name, refresh_nav_history_actions};
use sidebar::*;
pub(crate) use splitview::SplitView;
use tabs::*;
// The tab strip now lives under `crate::widgets::tab`; re-export it here so the
// long-standing `crate::window::TabView` path (used by `winstate` and the
// window sub-builders) keeps resolving after the move to `src/widgets/`.
pub(crate) use crate::widgets::tab::TabView;
use toolbar::*;
use viewactions::*;

/// Match the editor's GtkSourceView style scheme to the desktop dark/light theme. The
/// scheme system is independent of the GTK theme, so without this the editor pane stays
/// on the default LIGHT scheme while the rest of the app follows a dark theme. Prefers
/// Adwaita / Adwaita-dark (they match the GTK Adwaita palette), falling back to a generic
/// dark/light scheme when those aren't installed. Re-applied on a live theme switch.
pub(crate) fn apply_editor_style_scheme(buf: &sourceview::Buffer, dark: bool) {
    let mgr = sourceview::StyleSchemeManager::default();
    let pick = |names: &[&str]| names.iter().find_map(|n| mgr.scheme(n));
    let scheme = if dark {
        pick(&["Adwaita-dark", "classic-dark", "oblivion", "solarized-dark"])
    } else {
        pick(&["Adwaita", "classic"])
    };
    buf.set_style_scheme(scheme.as_ref());
}

// ── window factory ────────────────────────────────────────────────────────────

/// The numbers a freshly-built window's first tab needs, decoupled from
/// [`crate::session::WindowSession`]/`TabSession` (the on-disk shape) so
/// `build_window` has one shape to read regardless of whether it's building an
/// ad-hoc window (`new_window`, always sensible fresh-window defaults) or
/// replaying a persisted one (`window::restore`). View
/// mode and split arrangement are deliberately NOT here — every window/tab
/// starts at Preview/no-split and, when restoring, is switched into its real
/// mode afterward through the actual `win.view-mode`/`win.split-*` GActions
/// (`restore::apply_restored_tab_state`) so the content genuinely rebuilds,
/// rather than only a stored flag being set that nothing then reads.
struct WindowInit {
    width: i32,
    height: i32,
    zoom_level: f64,
    show_unsafe_images: bool,
    /// This window's own toolbar/status-bar/outline visibility — per window, so
    /// it is threaded through here exactly like `zoom_level` rather than read
    /// from any app-wide place.
    chrome: crate::session::ChromeSession,
}

impl Default for WindowInit {
    fn default() -> Self {
        Self {
            width: config().window.width,
            height: config().window.height,
            zoom_level: 1.0,
            show_unsafe_images: false,
            chrome: crate::session::ChromeSession::default(),
        }
    }
}

/// Read a window's OWN chrome visibility straight off its `win.*` toggle
/// actions — the single reader shared by both paths that need it: seeding a new
/// window from its source ([`inherit_from`]) and persisting each window's own
/// value (`lifecycle::persist_all_windows_session`).
///
/// The actions are the live source of truth: each toggle's change-state handler
/// only ever touches its own window, so its action state IS that window's
/// current chrome, with no cache to fall out of step with it. Only the section
/// *state* `S_i` is read — the derived *enabled* attribute is recomputed from
/// `show_toolbar` by `reconcile_toolbar_chrome` (invariant I3) and never stored.
pub(crate) fn read_window_chrome(window: &ApplicationWindow) -> crate::session::ChromeSession {
    let mut toolbar_sections = crate::session::ToolbarSections::default();
    for id in crate::app::TBTN_SECTION_IDS {
        toolbar_sections.set(
            id,
            bool_action_state(window, &format!("show-tbtn-{id}"), true),
        );
    }
    crate::session::ChromeSession {
        show_toolbar: bool_action_state(window, "show-toolbar", true),
        show_statusbar: bool_action_state(window, "show-statusbar", true),
        outline_visible: bool_action_state(window, "outline", true),
        annotations_visible: bool_action_state(window, "annotations", false),
        toolbar_sections,
    }
}

/// Everything a brand-new window inherits from the source/active window it was
/// spawned from — the ONE place that answers "what does a new window start
/// with?", so a future inheritable field is added here once instead of being
/// re-read at each new-window call site (where the next call site added would
/// forget it — gtk4-rs skill's enforced-choke-point principle).
///
/// `None` means there genuinely is no source window (the app-startup fallbacks),
/// and only then are the fresh defaults — 100% zoom, all chrome shown — correct
/// rather than arbitrary.
fn inherit_from(source: Option<&ApplicationWindow>) -> WindowInit {
    let Some(src) = source else {
        return WindowInit::default();
    };
    WindowInit {
        zoom_level: winstate::chrome(src).map_or(1.0, |c| c.zoom_level.get()),
        chrome: read_window_chrome(src),
        ..WindowInit::default()
    }
}

/// Build an ordinary new window with sensible fresh-window defaults (Preview
/// mode, no split, 100% zoom) — used only for the app-startup fallbacks that
/// genuinely have no source/active window to inherit from
/// (`app/appactions.rs`'s `app.new` None branch, `app/setup.rs`'s
/// no-active-window `on_activate`/no-saved-session paths). Every other
/// new-window path has a source or active window and should call
/// [`new_window_from_source`] instead. A window being restored FROM a saved
/// session instead goes through `restore::restore_window`, which calls
/// `build_window` directly with the persisted geometry/zoom.
pub(crate) fn new_window(
    app: &Application,
    title: &str,
    md: &str,
    file_path: Option<&std::path::Path>,
) -> ApplicationWindow {
    build_window(app, title, md, file_path, &WindowInit::default())
}

/// Same as [`new_window`], but seeds the new window's window-scoped state — its
/// zoom AND its chrome visibility — from `source`, the window it was spawned
/// from, instead of the fresh defaults. Used by every new-window path that has a
/// source or active window to inherit from: pop-out (Move Tab to New Window /
/// drag-to-desktop, via `spawn_bare_window_for_tab`), View ▸ New Window /
/// Ctrl+N, and Open-in-a-new-window.
///
/// Both inherited values are window-scoped (see the `winstate` module's
/// state-scope rule), and inherit for the same reason: a brand-new window has
/// nothing else to constrain them, so a fresh default is arbitrary rather than a
/// design, and the source window is the only evidence of what the user wants.
///
/// - **Zoom** is frequently an ACCESSIBILITY setting, not a per-document style
///   choice: someone running at 150% because that's what they can comfortably
///   read should not be forced to re-zoom every time a new window opens.
/// - **Chrome** is a workspace preference of the same kind: a user who has hidden
///   the toolbar has said they want the space, not that they want it back in the
///   next window they open.
///
/// The asymmetry with a move INTO an EXISTING window is deliberate and holds for
/// both, though the two reach it differently. Zoom must actively adopt the
/// destination's value (two tabs in one window physically cannot render at
/// different zooms under the shared per-window CSS class). Chrome adopts the
/// destination by CONSTRUCTION, needing no code at all: it is not tab-scoped, so
/// an arriving tab carries no chrome to impose and the destination's own
/// `win.*` toggles are simply left alone — a tab moved into a window whose
/// toolbar is hidden must not make that toolbar reappear. That is why nothing in
/// this change touches `wire_tab_arrival` (the `winstate` state-scope rule warns
/// that editing it for a window-scoped seeding change is the signal of a wrong turn).
pub(crate) fn new_window_from_source(
    app: &Application,
    title: &str,
    md: &str,
    file_path: Option<&std::path::Path>,
    source: Option<&ApplicationWindow>,
) -> ApplicationWindow {
    build_window(app, title, md, file_path, &inherit_from(source))
}

/// Hard minimum window width (px) — the usability floor enforced via
/// `set_size_request` in [`build_window`]. Sits *below* the default open width
/// (900) so dragging narrower stays a real gesture; chosen so a single
/// editor/preview pane plus the ~240px outline sidebar stays comfortable, and so
/// the window fits mainstream small screens (1280×720 / 1366×768). See
/// `viewactions::update_toolbar_min_width` (invariant I5). Tune here by feel.
const MIN_WINDOW_WIDTH: i32 = 720;

/// The shared window/first-tab construction every window goes through,
/// regardless of whether its initial numbers came from `WindowInit::default()`
/// (`new_window`) or a persisted `WindowSession` (`restore::restore_window`).
fn build_window(
    app: &Application,
    title: &str,
    md: &str,
    file_path: Option<&std::path::Path>,
    init: &WindowInit,
) -> ApplicationWindow {
    let zoom_level = init.zoom_level;

    // ── zoom CSS provider ────────────────────────────────────────────────────
    // One per-window CssProvider carries the single rule that scales the preview
    // text via Pango's CSS base font. Added to the display (not the widget) at
    // APPLICATION priority; removed on destroy. A clone is kept outside TabState
    // for the destroy handler, since the state is unregistered before the CSS
    // provider needs to be removed.
    //
    // The rule is loaded AFTER the window exists (below), because it is scoped to
    // that window's `.scrib-win-<id>` class — an unscoped `textview.scrib-preview`
    // selector on the shared display collides across windows (last-loaded wins),
    // the multi-window zoom regression (ScrAP-64). Created empty here so it can be
    // added to the display before the first render(); the scoped rule is applied
    // the moment the window (and its class) is built, before the initial preview.
    let zoom_css_provider = gtk::CssProvider::new();
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &zoom_css_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    let zoom_provider_destroy = zoom_css_provider.clone();

    let window = ApplicationWindow::builder()
        .application(app)
        .title(title)
        .default_width(init.width)
        .default_height(init.height)
        // No `.show_menubar(true)`: we build our OWN per-window GtkPopoverMenuBar
        // in `build_chrome` (ScrAP-63) so the View ▸ Documents submenu can
        // list THIS window's tabs. The auto-built app menubar would have shared one
        // model across every window.
        // Compile-checked and resolution-tested like every other icon name
        // (ScrAP-39). Resolves from the bundled GResource, so it works on
        // Windows and from an uninstalled `cargo run`, not just where
        // `install.sh` has populated the hicolor theme.
        .icon_name(crate::icons::Icon::App.name())
        .build();

    // Enforce a hard usability floor on the window width. GTK takes
    // `MAX(content_derived_minimum, size_request)`, so today — while the
    // non-wrapping toolbar's content minimum (~1633px, all sections shown)
    // dominates — this only bites once enough toolbar sections are hidden
    // (`View ▸ Toolbar`) to drop the content minimum below it, preventing the
    // "hide everything → drag to a useless sliver" degenerate case. The
    // large-min-width limitation itself is resolved in practice by the short
    // default toolbar (file/edit/view shown; the rest opt-in via View ▸ Toolbar).
    // `-1` height leaves the vertical minimum content-
    // derived. This is the deliberate counterpart to the "no set_size_request,
    // default_width only" min-width geometry (invariant I5): an explicit floor
    // is correct precisely because the content floor is (or will be) gone.
    window.set_size_request(MIN_WINDOW_WIDTH, -1);

    // Native Win32 frame ⇒ DWM owns the caption, and paints it light unless asked
    // otherwise. Wire it to follow the desktop's lightness from realize onward;
    // `re_render_all_windows` re-applies it on every flip. No-op on Linux, where
    // GTK paints its own decorations.
    #[cfg(windows)]
    crate::platform::win32::track_caption_theme(&window);

    // Same frame, second consequence: the OS owns the maximize button, so GTK never
    // learns it is maximized the way it does under CSD, and a popover's layout pass
    // would shrink a maximized window back to its pre-maximize size. Keeping GTK's
    // remembered size current while maximized removes the wrong number that path
    // reaches for.
    #[cfg(windows)]
    crate::platform::win32::track_maximized_size(&window);

    // Tag the window with its per-window zoom-scope class and load the scoped
    // rule now that the window (hence its `.scrib-win-<id>` node) exists. The
    // provider is already on the display; this is the first content it carries,
    // so the initial preview built below picks up the restored zoom. Scoping is
    // load-bearing — see the zoom-provider note above and ScrAP-64.
    window.add_css_class(&format!(
        "scrib-win-{}",
        winstate::WindowId::of(&window).raw()
    ));
    zoom_css_provider.load_from_data(&zoom::zoom_css_rule(&window, zoom_level));

    // Keyboard-shortcuts help overlay. `set_help_overlay` owns the
    // window, creates this window's `win.show-help-overlay` action (opened with
    // F1 / Ctrl+?, registered in `setup::register_accelerators`), and hides it on
    // close so it can reopen. Per-window like the menubar (ScrAP-63).
    window.set_help_overlay(Some(&crate::app::make_shortcuts_window()));

    // ── toolbar & chrome ─────────────────────────────────────────────────────
    let (toolbar, section_boxes, format_box, heading_btn, tb_edit_btns, documents_btn, theme_btn) =
        build_toolbar();
    let doc_dir = file_path.and_then(|p| p.parent());
    let chrome = build_chrome(
        &window,
        &toolbar,
        md,
        doc_dir,
        zoom_level,
        init.show_unsafe_images,
    );

    // Bind the open-documents combo box to THIS window's `documents_menu` — the
    // same per-window GMenu the menubar's View ▸ Documents submenu observes (built
    // inside `build_chrome`, so it exists only now). One model, two surfaces: any
    // `refresh_documents_menu` rebuild updates both popups and neither can drift.
    documents_btn.set_menu_model(Some(&chrome.documents_menu));

    // Toolbar/statusbar/outline visibility are PER-WINDOW state (the operator's
    // decision), so — exactly like `zoom_level` — they arrive threaded through
    // this window's own `WindowInit` and are never read from an app-wide place.
    // Whoever built this window already decided which window's chrome it starts
    // with: `inherit_from` (the source/active window's own live values, for
    // every new-window path), `restore::restore_window` (THIS window's own
    // persisted `WindowSession::chrome`), or `WindowInit::default()` (all shown
    // — only the genuine no-source startup fallbacks).
    let chrome_init = init.chrome;

    // ── action registration ──────────────────────────────────────────────────
    register_editor_actions(&window, &heading_btn);
    register_annotate_action(&window);
    register_next_annotation_action(&window);
    register_view_actions(
        &window,
        &toolbar,
        &section_boxes,
        &chrome.status_bar,
        &SidebarSections {
            outline_section: &chrome.outline_section,
            annotations_section: &chrome.annotations_section,
            sidebar_box: &chrome.sidebar_box,
        },
        &ChromeVisibility {
            show_toolbar: chrome_init.show_toolbar,
            show_statusbar: chrome_init.show_statusbar,
            // Per-TAB, unlike the window-scoped chrome around it — this tab's
            // own restored value.
            show_unsafe_images: init.show_unsafe_images,
            // `to_array` is in canonical `TBTN_SECTION_IDS` order, matching
            // `section_boxes`.
            section_states: chrome_init.toolbar_sections.to_array(),
            outline_visible: chrome_init.outline_visible,
            annotations_visible: chrome_init.annotations_visible,
        },
    );
    register_tab_actions(&window);
    // Back/Forward (TDD §23). Both start insensitive and are re-derived on every
    // tab switch; the window's first document is seeded into the history by
    // `winstate::register` below, since no switch callback ever fires for it.
    register_nav_history_actions(&window);
    // ── Phase 4-5: first tab's editor + preview + per-tab wiring ──────────────
    // Assembled by the SHARED `assemble_tab_core` — byte-for-byte the same
    // orchestration every later tab runs in `window/tabs/create_tab_in_window`.
    // `build_chrome` already created this tab's `content_box` and rendered its
    // initial preview (but did not mount it); the core mounts the persistent
    // splitter (the editor is created once and never reparented),
    // installs that preview, and wires the per-tab editor/search/occurrences/
    // caret-overlay/live-preview signals.
    let core = assemble_tab_core(&chrome.content_box, md, Some(&chrome.initial_preview));

    // ── window-level find/format furniture (built ONCE per window, not per tab) ─
    // The find bar's shared widgets live in WindowChrome; its closures fetch the
    // active tab's engine fresh via state(window). Wired here against the first
    // tab's search context.
    wire_find_bar(&window, &chrome);
    // Gate the Format and Go To Line commands on editor focus.
    setup_editor_focus_gate(&window, &format_box, &chrome.find_bar_revealer);
    // Stage-2 caret formatting overlay: ONE per window (ScrAP-61), stored in
    // WindowChrome and re-parented to the active tab's editor on every switch.
    // Built once here — its heading-menu font resolution happens once ever, O(1)
    // (ScrAP-61). `assemble_tab_core` already wired this first tab's
    // editor to drive it (via `wire_editor_format_overlay`), exactly as every
    // later tab wires its own. Unparented once on window destroy (below).
    let (format_overlay, ov_edit_btns) = build_format_overlay();

    // Register the typed per-window/per-tab state (replaces the per-window qdata
    // keys and the old WINDOW_SOURCES map). The freshly loaded content is, by
    // definition, clean, so source and saved_baseline both start as `md`.
    //
    // State is split into TabState (per document) and
    // WindowChrome (window-level furniture shared across tabs — see winstate.rs
    // module doc).
    let chrome_state = build_window_chrome_state(
        &chrome,
        tb_edit_btns,
        documents_btn,
        theme_btn,
        format_overlay,
        ov_edit_btns,
        zoom_level,
        zoom_css_provider,
    );
    let tab_id = winstate::alloc_tab_id();
    winstate::register(
        &window,
        chrome_state.clone(),
        // The path is set from the opened file up front (not deferred to
        // `attach_file_backing`) so `st.doc_dir()` is correct for
        // `new_window`'s OWN view-mode re-render — otherwise that second
        // render resolves images against a None doc dir and clobbers the
        // correct initial render (the blank-image bug).
        TabState::new(winstate::TabInit {
            id: tab_id,
            path: file_path.map(|p| p.to_path_buf()),
            text: md.to_string(),
            editor: core.editor,
            editor_buf: core.editor_buf,
            split: core.split,
            content_box: chrome.content_box.clone(),
            allow_unsafe_images: init.show_unsafe_images,
            search_settings: core.search_settings,
            search_context: core.search_context,
            chrome: chrome_state.clone(),
        }),
    );
    wire_tab_machinery(&window, &chrome_state.tabs);
    update_window_title(&window);
    // The first tab is active by construction but never fires `switch-page`
    // (its page was appended in `build_chrome`, before `wire_tab_switch_page`),
    // so parent the window's caret-overlay popover onto its editor here — every
    // later tab re-targets it on its own activation
    // (`on_active_tab_changed::retarget_format_overlay`). This first parenting is
    // what triggers the overlay's one-and-only heading-menu font resolution.
    if let Some(st) = winstate::state(&window) {
        retarget_format_overlay(&st);
    }
    // Register all window teardown handlers together (the connection order is
    // load-bearing — see the helper's doc comment).
    register_window_destroy_handlers(&window, zoom_provider_destroy);

    wire_format_surface_updates(&window);

    // Re-bind the Copy action to the now-mounted preview view. The
    // earlier `connect_buf_to_copy_action` inside `register_editor_actions` ran
    // BEFORE `assemble_tab_core` mounted the split, so `content_box` was empty,
    // `active_text_view` returned `None`, and it early-returned — leaving the
    // buffer `has-selection` and primary-clipboard (`ScrAP-110` table-cell) handlers
    // unwired for a default window that opens in Preview and never fires a
    // `view-mode` change (a restored/mode-switched window re-binds via the
    // change-state handler in `viewactions`, so this only closes the startup gap).
    // Now the split is mounted, the tab is registered, and Preview layout has
    // hidden the editor pane, so `active_text_view` resolves the preview view and
    // both selection handlers get wired (so a table-cell or body selection enables
    // Copy from the very first render, not only after a mode/tab switch).
    connect_buf_to_copy_action(&window);

    // Apply the editor-only action gating for the initial (always Preview) mode:
    // without this, Save / Insert Emoji / Cut / Delete / Change Case could show
    // a stale enabled state in every surface until the first mode switch. A
    // restored session's real view mode is applied afterward by the caller
    // (`restore::apply_restored_tab_state`), whose own `view-mode` change-state
    // call re-applies this gating for the actual restored mode.
    apply_mode_action_state(&window, ViewMode::Preview);

    // Build the initial outline tree (after the view mode is settled so it reads
    // the right source) and wire the scroll-spy for the initial mode.
    refresh_outline(&window);
    // Build the initial annotations list alongside it (same source-by-mode rule).
    refresh_annotations(&window);
    wire_scroll_spy(&window);

    wire_close_request(&window);
    // Crash recovery: leaving the editor pane — for a menu, the toolbar, another window,
    // another application — commits every outstanding snapshot at once rather than
    // waiting out the debounce.
    swap::wire_swap_focus_flush(&window);

    window.present();
    window
}

/// Assemble the window-level [`WindowChrome`](winstate::WindowChrome) — the
/// widgets and state shared across every tab — from the freshly built chrome
/// widgets, the toolbar/overlay Insert↔Edit button sets, and this window's zoom
/// provider. Split out of `build_window` so its wide record construction reads
/// as one named step. `format_overlay`, the button vecs and `zoom_css_provider`
/// are moved in (their final owner is this record).
// Pure field-forwarding constructor: every argument is a distinct owned widget /
// value moved verbatim into the record, so bundling them into a params struct
// would only add ceremony without reducing the real fan-in.
#[allow(clippy::too_many_arguments)]
fn build_window_chrome_state(
    chrome: &Chrome,
    tb_edit_btns: Vec<(FmtInsertKind, gtk::Button)>,
    documents_btn: gtk::MenuButton,
    theme_btn: gtk::MenuButton,
    format_overlay: gtk::Popover,
    ov_edit_btns: Vec<(FmtInsertKind, gtk::Button)>,
    zoom_level: f64,
    zoom_css_provider: gtk::CssProvider,
) -> Rc<winstate::WindowChrome> {
    Rc::new(winstate::WindowChrome {
        outline_scroller: chrome.outline_scroller.clone(),
        annotations_scroller: chrome.annotations_scroller.clone(),
        conflict_toast: chrome.conflict_toast.clone(),
        recovery_toast: chrome.recovery_toast.clone(),
        recovery_toast_label: chrome.recovery_toast_label.clone(),
        info_toast: chrome.info_toast.clone(),
        status: RefCell::new(winstate::StatusStack::new(chrome.status_label.clone())),
        pos_label: chrome.pos_label.clone(),
        find_bar_revealer: chrome.find_bar_revealer.clone(),
        find_entry: chrome.find_entry.clone(),
        match_count_label: chrome.match_count_label.clone(),
        replace_row: chrome.replace_row.clone(),
        fmt_edit_btns: tb_edit_btns,
        documents_btn,
        theme_btn,
        format_overlay: crate::saferizer::PersistentPopover::adopt(format_overlay),
        format_overlay_timer: RefCell::new(None),
        overlay_edit_btns: ov_edit_btns,
        fmt_edit_state: Cell::new(None),
        focused_pane: Cell::new(winstate::FocusedPane::Editor),
        ctx_link: RefCell::new(None),
        zoom_level: Cell::new(zoom_level),
        zoom_css_provider,
        tabs: chrome.tabs.clone(),
        documents_menu: chrome.documents_menu.clone(),
        format_insert_menu: chrome.format_insert_menu.clone(),
        menubar: chrome.menubar.clone(),
        format_menu_kind: Cell::new(None),
        documents_refresh_scheduled: Cell::new(false),
    })
}

/// Wire the tab strip's active-tab-changed callback plus the drag-and-drop /
/// close / context-menu handlers every window's strip needs. All are wired
/// unconditionally: `move_tab_to_new_window` relies on `wire_tab_arrival` being
/// present on every window it might build (see tabs.rs), and `wire_tab_bar_dnd`
/// owns the single `GtkDragSource`/`GtkDropTarget` pair that drives in-strip
/// reorder AND cross-window move AND drag-to-desktop (module doc, widgets/tab) —
/// no per-tab-label wiring needed anymore (ScrAP-50 is now moot: there is no
/// `GtkNotebook` internal DnD to have raced against in the first place).
fn wire_tab_machinery(window: &ApplicationWindow, tabs: &TabView) {
    wire_tab_switch_page(window, tabs);
    wire_tab_arrival(window, tabs);
    wire_tab_bar_dnd(window, tabs);
    wire_tab_close_and_menu(window, tabs);
}

/// Keep the Link/Image format surfaces (toolbar + overlay tooltips) relabeled
/// Insert↔Edit as the `format` action's enabled state changes (focus / mode)
/// and as this window becomes the active one. The per-buffer mark-set trigger
/// for an in-progress selection change is wired once per tab by
/// `wire_tab_buffer_signals`.
fn wire_format_surface_updates(window: &ApplicationWindow) {
    if let Some(format_action) = simple_action(window, "format") {
        format_action.connect_notify_local(
            Some("enabled"),
            glib::clone!(
                #[weak(rename_to = w)]
                window,
                move |_, _| {
                    update_format_edit_surfaces(&w);
                }
            ),
        );
    }
    // When this window becomes the active one, point the app menu at its state.
    window.connect_notify_local(
        Some("is-active"),
        glib::clone!(
            #[weak(rename_to = w)]
            window,
            move |_, _| {
                update_format_edit_surfaces(&w);
            }
        ),
    );
}

/// Register all of the window's teardown handlers in one place, in the order
/// their firing depends on. `connect_destroy` handlers fire in connection order,
/// so this order is load-bearing:
/// 1. Unparent the single caret-overlay popover while the chrome is still
///    registered — a `set_parent`ed popover is NOT auto-unparented, so its host
///    editor would otherwise finalize "with children left" and leak the popover
///    subtree (ScrAP-61).
/// 2. Remove this window's zoom CSS rule from the shared display (the provider
///    reference is held by this closure so it outlives the window).
/// 3. Unregister the window's typed state LAST, so both handlers above still
///    resolve it via `winstate::chrome`/`state`.
fn register_window_destroy_handlers(window: &ApplicationWindow, zoom_provider: gtk::CssProvider) {
    window.connect_destroy(|w| {
        if let Some(chrome) = winstate::chrome(w) {
            // Cancel any pending ~40 ms format-overlay timer so it can't fire a
            // `popup()` during window teardown (GTK4Rs/AP-128/ScrAP-152). Belt to the timer body's
            // own `is_realized()` gate; `take()` no-ops if it already fired.
            if let Some(id) = chrome.format_overlay_timer.borrow_mut().take() {
                id.remove();
            }
            // popdown-then-unparent (ScrAP-144), via the handle — the prior raw
            // `unparent()` here skipped the close path.
            chrome.format_overlay.teardown();
        }
    });
    window.connect_destroy(move |_| {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_remove_provider_for_display(&display, &zoom_provider);
        }
    });
    window.connect_destroy(winstate::unregister);
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// Build a registered, non-unique test app.
    ///
    /// `pub(super)` so every GTK test under `window/` reaches the SAME helper rather than
    /// each re-deriving one. A helper that only its own module can import is a helper the
    /// next module will quietly reimplement, slightly differently (ScrAP-219).
    pub(super) fn test_app(id: &str) -> gtk::Application {
        let app = gtk::Application::new(Some(id), gtk::gio::ApplicationFlags::NON_UNIQUE);
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");
        app
    }

    /// The four chrome toggles a window owns, as `win.*` action names.
    const CHROME_ACTIONS: [&str; 4] = [
        "show-toolbar",
        "show-statusbar",
        "outline",
        "show-tbtn-edit",
    ];

    /// A window spawned from a source window inherits that window's chrome —
    /// the per-window counterpart of zoom's inheritance, and for the same
    /// reason: a brand-new window has nothing else to constrain its
    /// window-scoped state, so the source window is the only evidence of what
    /// the user wants. Covers all four toggles (whole-bar, status bar, outline,
    /// and one section) through the real `new_window_from_source` funnel every
    /// new-window path uses.
    #[gtktest::test]
    fn a_new_window_inherits_every_chrome_toggle_from_its_source_window() {
        let app = test_app("com.extollit.scribobulate.integrationtest.chromeinherit");

        let win_a = new_window(&app, "IT-A", "alpha", None);
        for name in CHROME_ACTIONS {
            assert!(
                bool_action_state(&win_a, name, false),
                "{name} starts shown"
            );
            change_action_state(&win_a, name, &false.to_variant());
            assert!(!bool_action_state(&win_a, name, true), "{name} toggled off");
        }

        let win_b = new_window_from_source(&app, "IT-B", "beta", None, Some(&win_a));
        for name in CHROME_ACTIONS {
            assert!(
                !bool_action_state(&win_b, name, true),
                "a window spawned from a source window must inherit its {name}"
            );
        }
    }

    /// The same inheritance, driven through the real `win.new-window` surface
    /// (View ▸ New Window / Ctrl+N) rather than by calling the factory directly
    /// — the action is what a user actually reaches.
    #[gtktest::test]
    fn the_new_window_action_inherits_the_active_windows_chrome() {
        let app = test_app("com.extollit.scribobulate.integrationtest.chromeinherit2");

        let win_a = new_window(&app, "IT-A", "alpha", None);
        change_action_state(&win_a, "show-toolbar", &false.to_variant());

        let before: std::collections::HashSet<_> =
            app.windows().iter().map(|w| w.as_ptr() as usize).collect();
        gtk::prelude::WidgetExt::activate_action(&win_a, "win.new-window", None).unwrap();
        let win_b = app
            .windows()
            .into_iter()
            .find(|w| !before.contains(&(w.as_ptr() as usize)))
            .and_then(|w| w.downcast::<ApplicationWindow>().ok())
            .expect("win.new-window must open a new window");

        assert!(
            !bool_action_state(&win_b, "show-toolbar", true),
            "win.new-window must inherit the active window's chrome"
        );
    }

    /// A window with NO source window (the app-startup fallbacks) starts with
    /// all chrome shown. The fresh default is only correct when there is
    /// genuinely nothing to inherit from — which is exactly what `new_window`
    /// means.
    #[gtktest::test]
    fn a_window_with_no_source_starts_with_all_chrome_shown() {
        let app = test_app("com.extollit.scribobulate.integrationtest.nosourcechrome");
        let win = new_window(&app, "IT", "alpha", None);
        for name in CHROME_ACTIONS {
            assert!(
                bool_action_state(&win, name, false),
                "{name} shown by default"
            );
        }
    }

    /// EACH window persists ITS OWN chrome — not the closing window's.
    ///
    /// The bug this pins: `persist_all_windows_session` used to read one
    /// app-wide chrome value off the CLOSING window and stamp it on the whole
    /// session, so a toggle made in any other window was silently discarded
    /// ("last window to touch it wins"). With two windows disagreeing, that is
    /// straightforwardly unrepresentable — which is the point of the move.
    #[gtktest::test]
    fn each_window_persists_its_own_chrome_not_the_closing_windows() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = test_app("com.extollit.scribobulate.integrationtest.perwindowpersist");

            let win_a = new_window(&app, "IT-A", "alpha", None);
            let win_b = new_window(&app, "IT-B", "beta", None);

            // Hide A's toolbar and outline; B is left entirely untouched.
            change_action_state(&win_a, "show-toolbar", &false.to_variant());
            change_action_state(&win_a, "outline", &false.to_variant());

            // Closing B is what writes the session — and B is the window whose
            // chrome is still all-shown, i.e. exactly the "closing window" whose
            // value used to be stamped over everyone else's.
            win_b.close();

            let persisted = crate::session::load();
            assert_eq!(persisted.windows.len(), 2, "both windows are persisted");
            assert!(
                persisted
                    .windows
                    .iter()
                    .any(|w| !w.chrome.show_toolbar && !w.chrome.outline_visible),
                "the window whose chrome was toggled must persist its OWN values, \
                 even though a different window is the one closing: {:?}",
                persisted
                    .windows
                    .iter()
                    .map(|w| w.chrome)
                    .collect::<Vec<_>>()
            );
            assert!(
                persisted
                    .windows
                    .iter()
                    .any(|w| w.chrome.show_toolbar && w.chrome.outline_visible),
                "the untouched window must persist ITS own values, not the other's"
            );
        });
    }

    /// Toggling chrome in one window disturbs neither the other window's live
    /// state nor what it persists. The runtime half was always true (each
    /// toggle's handler only ever touched its own window's widgets); the
    /// PERSISTED half is what this change makes true, so the assertion is made
    /// on both.
    #[gtktest::test]
    fn a_chrome_toggle_in_one_window_leaves_the_other_window_alone() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = test_app("com.extollit.scribobulate.integrationtest.chromeindependent");

            let win_a = new_window(&app, "IT-A", "alpha", None);
            let win_b = new_window(&app, "IT-B", "beta", None);

            for name in CHROME_ACTIONS {
                change_action_state(&win_a, name, &false.to_variant());
            }

            // Runtime: B's own toggles are untouched.
            for name in CHROME_ACTIONS {
                assert!(
                    bool_action_state(&win_b, name, false),
                    "toggling {name} in window A must not change window B's state"
                );
            }

            // Persisted: the two windows are recorded as two different answers.
            win_a.close();
            let persisted = crate::session::load();
            assert_eq!(persisted.windows.len(), 2);
            let hidden = persisted
                .windows
                .iter()
                .filter(|w| !w.chrome.show_toolbar)
                .count();
            assert_eq!(
                hidden,
                1,
                "exactly one of the two windows has its toolbar hidden: {:?}",
                persisted
                    .windows
                    .iter()
                    .map(|w| w.chrome)
                    .collect::<Vec<_>>()
            );
        });
    }

    /// A persisted per-window chrome value is restored onto THAT window: two
    /// saved windows with different chrome restore as two windows with
    /// different chrome, action state and actual widget visibility alike.
    /// (The file round-trip half is covered by `session::tests`; this is the
    /// half that proves the value reaches the real widgets.)
    #[gtktest::test]
    fn each_restored_window_gets_its_own_persisted_chrome() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let hidden = crate::session::ChromeSession {
                show_toolbar: false,
                show_statusbar: false,
                outline_visible: false,
                annotations_visible: false,
                toolbar_sections: crate::session::ToolbarSections::default(),
            };
            crate::session::save(&crate::session::Session {
                windows: vec![
                    crate::session::WindowSession {
                        chrome: hidden,
                        ..Default::default()
                    },
                    crate::session::WindowSession::default(), // all shown
                ],
                ..Default::default()
            });

            let app = test_app("com.extollit.scribobulate.integrationtest.restorechrome");
            // Restore reads every tab's document off the main thread, so it is a
            // future now. `block_on` drives it on this same default main context —
            // iterating the loop until it completes — which is what the running
            // application does too, just without a synchronous caller waiting.
            assert!(
                gtk::glib::MainContext::default().block_on(restore_session(&app)),
                "the saved session restores"
            );

            let windows: Vec<ApplicationWindow> = app
                .windows()
                .into_iter()
                .filter_map(|w| w.downcast::<ApplicationWindow>().ok())
                .collect();
            assert_eq!(windows.len(), 2);

            let toolbars: Vec<bool> = windows
                .iter()
                .map(|w| bool_action_state(w, "show-toolbar", true))
                .collect();
            assert!(
                toolbars.contains(&true) && toolbars.contains(&false),
                "each restored window must get its OWN persisted chrome, not one \
                 shared answer: {toolbars:?}"
            );

            // The window restored hidden must also have its sidebar widget
            // actually hidden — not merely the action state set.
            let hidden_win = windows
                .iter()
                .find(|w| !bool_action_state(w, "show-toolbar", true))
                .expect("one window restored with its toolbar hidden");
            let chrome = winstate::chrome(hidden_win).expect("chrome registered");
            assert!(
                !chrome.outline_scroller.parent().unwrap().is_visible(),
                "the outline sidebar box itself must start hidden, not just the action state"
            );
        });
    }

    /// The annotations viewer lists exactly the comment-bearing annotations (TDD
    /// 20.1/20.2): a bare highlight (a rendering feature, not an annotation) and the
    /// inert suggested-edit kinds are excluded, so a document with two commented
    /// annotations plus one bare highlight shows two rows.
    #[gtktest::test]
    fn the_annotations_viewer_lists_only_comment_bearing_annotations() {
        let app = test_app("com.extollit.scribobulate.integrationtest.annlist");
        let md = "a {==c1==}{>>note one<<} b {==bare==} c {++ins++} d {>>point<<} e";
        let win = new_window(&app, "IT-ann", md, None);
        let ch = winstate::chrome(&win).expect("chrome registered");
        let lv = list_view_of(&ch.annotations_scroller).expect("annotations list is a ListView");
        let n = lv.model().expect("selection model").n_items();
        assert_eq!(
            n, 2,
            "two comment-bearing annotations; bare highlight + inert kind excluded"
        );
    }

    /// The four-state sidebar rule (TDD 20.9): the two panes toggle independently, and
    /// the whole sidebar disappears only when BOTH are off — driven through the real
    /// `win.outline` / `win.annotations` actions and `reconcile_sidebar_visibility`.
    #[gtktest::test]
    fn the_sidebar_hides_only_when_both_panes_are_off() {
        let app = test_app("com.extollit.scribobulate.integrationtest.ann4state");
        let win = new_window(&app, "IT-4state", "# H\n\ntext {>>note<<}", None);
        let ch = winstate::chrome(&win).expect("chrome registered");
        let outline_section = ch.outline_scroller.parent().unwrap();
        let annotations_section = ch.annotations_scroller.parent().unwrap();
        let sidebar = outline_section.parent().unwrap();

        // Defaults: outline on, annotations off → sidebar shown, only outline visible.
        assert!(outline_section.is_visible());
        assert!(!annotations_section.is_visible());
        assert!(sidebar.is_visible());

        // Annotations on → its section and the sidebar are shown.
        change_action_state(&win, "annotations", &true.to_variant());
        assert!(annotations_section.is_visible());
        assert!(sidebar.is_visible());

        // Both off → the empty sidebar disappears entirely.
        change_action_state(&win, "outline", &false.to_variant());
        change_action_state(&win, "annotations", &false.to_variant());
        assert!(!outline_section.is_visible());
        assert!(!annotations_section.is_visible());
        assert!(!sidebar.is_visible(), "empty sidebar disappears (TDD 20.9)");

        // Outline back on → sidebar reappears with only the outline.
        change_action_state(&win, "outline", &true.to_variant());
        assert!(sidebar.is_visible());
        assert!(outline_section.is_visible());
        assert!(!annotations_section.is_visible());
    }

    /// Viewer navigation resolves to the correct chip by SPAN IDENTITY, even with a
    /// bare highlight interposed (TDD 20.6): each listed annotation's `src_span` start
    /// resolves to its own margin-chip index via `marker_index_for_src`.
    #[gtktest::test]
    fn viewer_navigation_resolves_chips_by_identity() {
        let app = test_app("com.extollit.scribobulate.integrationtest.annident");
        let md = "a {==c1==}{>>note one<<} b {==bare==} c {==c2==}{>>note two<<} d";
        let win = new_window(&app, "IT-ident", md, None);
        let entries = crate::annotations::extract_entries(md);
        assert_eq!(entries.len(), 2, "two comment-bearing annotations");
        let view = preview_text_view(&win)
            .and_then(|tv| tv.downcast::<crate::codeview::CodePreviewView>().ok())
            .expect("preview view in preview mode");
        // Each entry's identity resolves to its own chip index, in order — the bare
        // highlight has no chip and does not perturb the mapping.
        assert_eq!(
            view.marker_index_for_src(entries[0].src_span.start.raw()),
            Some(0)
        );
        assert_eq!(
            view.marker_index_for_src(entries[1].src_span.start.raw()),
            Some(1)
        );
    }

    /// In split mode, activating a viewer row moves the editor CARET onto the annotation
    /// (TDD 20.4), landing on the right character even past multi-byte UTF-8 (TDD 20.14 —
    /// the byte src_span is converted to a char offset).
    #[gtktest::test]
    fn split_mode_annotation_activation_moves_the_editor_caret() {
        let app = test_app("com.extollit.scribobulate.integrationtest.annsplitcaret");
        // Multi-byte text BEFORE the annotation, so a byte-vs-char bug would misplace it.
        let md = "café ☕ note: {==the claim==}{>>fix this<<} end.";
        let win = new_window(&app, "IT-splitcaret", md, None);
        // Enter split mode so the editor is visible and caret-addressable.
        change_action_state(&win, "view-mode", &"split".to_variant());

        let entries = crate::annotations::extract_entries(md);
        assert_eq!(entries.len(), 1);
        let src_start = entries[0].src_span.start;
        annotations_nav::navigate_to_annotation(&win, src_start);

        let st = state(&win).expect("tab state");
        let expected_char = md[..src_start.raw()].chars().count() as i32;
        assert_eq!(
            st.editor_buf.property::<i32>("cursor-position"),
            expected_char,
            "split-mode activation places the editor caret on the annotation, char-correct \
             past multi-byte text (TDD 20.4 / 20.14)"
        );
    }

    /// `win.new-window` (View ▸ New Window / Ctrl+N) must seed
    /// the new window's zoom from the ACTIVE window's current zoom, not the
    /// `WindowInit::default()` 100% — zoom is window-scoped (see the `winstate`
    /// state-scope rule) and frequently an
    /// accessibility setting: forcing every new window back to 100% would
    /// make a 150%-reading user re-zoom forever.
    #[gtktest::test]
    fn new_window_action_inherits_the_active_windows_zoom() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.newwindowzoom"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");

        let win_a = new_window(&app, "IT-A", "alpha", None);
        winstate::chrome(&win_a)
            .expect("chrome registered")
            .zoom_level
            .set(1.5);

        let before: std::collections::HashSet<_> =
            app.windows().iter().map(|w| w.as_ptr() as usize).collect();
        gtk::prelude::WidgetExt::activate_action(&win_a, "win.new-window", None).unwrap();
        let win_b = app
            .windows()
            .into_iter()
            .find(|w| !before.contains(&(w.as_ptr() as usize)))
            .and_then(|w| w.downcast::<ApplicationWindow>().ok())
            .expect("win.new-window must open a new window");

        assert_eq!(
            winstate::chrome(&win_b)
                .expect("new window's chrome registered")
                .zoom_level
                .get(),
            1.5,
            "a window opened via win.new-window must inherit the active window's zoom"
        );
    }

    /// Popping a tab out to a brand-new window (View ▸ Move Tab
    /// to New Window / the tab context-menu entry — same funnel as
    /// drag-to-desktop, both routing through `spawn_window_hosting_tab` →
    /// `spawn_bare_window_for_tab`) must inherit the SOURCE window's zoom, not
    /// reset to 100%. Exercised through the `win.move-tab-new-window` GAction
    /// rather than calling `dnd`'s private helpers directly, matching this
    /// file's existing action-surface testing style.
    #[gtktest::test]
    fn pop_out_to_a_new_window_inherits_the_source_windows_zoom() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.popoutzoom"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");

        let win_a = new_window(&app, "IT-A", "alpha", None);
        winstate::chrome(&win_a)
            .expect("chrome registered")
            .zoom_level
            .set(1.5);
        // `win.move-tab-new-window` is disabled on a single-tab window (moving
        // a window's only tab would just leave an empty-of-purpose window
        // behind — `documents::update_window_title`) — add a second tab so
        // the action is enabled and actually fires.
        create_tab_in_window(&win_a, "second", None, false, false);

        let before: std::collections::HashSet<_> =
            app.windows().iter().map(|w| w.as_ptr() as usize).collect();
        gtk::prelude::WidgetExt::activate_action(&win_a, "win.move-tab-new-window", None).unwrap();
        let win_b = app
            .windows()
            .into_iter()
            .find(|w| !before.contains(&(w.as_ptr() as usize)))
            .and_then(|w| w.downcast::<ApplicationWindow>().ok())
            .expect("win.move-tab-new-window must open a new window");

        assert_eq!(
            winstate::chrome(&win_b)
                .expect("popped-out window's chrome registered")
                .zoom_level
                .get(),
            1.5,
            "a tab popped out to a new window must inherit its source window's zoom"
        );
    }

    /// Regression guard for the Q3 fix above: moving a tab into an EXISTING
    /// window must still be governed by the DESTINATION's zoom, unchanged.
    /// Zoom is window-scoped (one per-window CSS class, `WindowChrome.zoom_level`)
    /// — two tabs in one window physically cannot render at different zooms —
    /// so this is the one case the Q3 fix must NOT touch. This is the exact
    /// detach/append sequence `wire_tab_bar_dnd`'s cross-window drop handler
    /// performs; `wire_tab_arrival` is wired on every window at build time
    /// (`build_window`), so it fires here exactly as it would for a real drop.
    #[gtktest::test]
    fn moving_a_tab_into_an_existing_window_still_adopts_the_destinations_zoom() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.moveintoexistingzoom"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");

        let win_a = new_window(&app, "IT-A", "alpha", None);
        winstate::chrome(&win_a)
            .expect("chrome registered")
            .zoom_level
            .set(1.5);
        let win_b = new_window(&app, "IT-B", "beta", None);
        let dest_chrome = winstate::chrome(&win_b).expect("chrome registered");
        dest_chrome.zoom_level.set(1.0);

        let tab = state(&win_a).expect("win_a has its starter tab");
        let content_box = tab.content_box.clone();
        let source_chrome = winstate::chrome(&win_a).expect("chrome registered");
        source_chrome.tabs.detach_tab(&content_box);
        dest_chrome.tabs.append_page(&content_box);

        assert_eq!(
            winstate::chrome(&win_b)
                .expect("chrome still registered")
                .zoom_level
                .get(),
            1.0,
            "a tab moved into an existing window must NOT change that window's zoom \
             — it adopts the destination's, unmodified"
        );
    }
}
