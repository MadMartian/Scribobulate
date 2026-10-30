use crate::app::{
    dialog_dir_for, remember_dialog_dir, EDIT_CMDS, FILE_CMDS, FORMAT_CMDS, VIEW_CMDS,
};
use crate::codeview::CodePreviewView;
use crate::config::config;
use crate::format::{self, FormatCmd};
use crate::outline::{build_tree, extract_headings};
use crate::outline_view::build_outline_content;
use crate::preview::{
    cell_search_targets, preview_top_line, re_render, refresh_annotations_in_place, render,
    restore_preview_scroll_to_line, restore_preview_scroll_to_line_fresh,
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
mod arrangement;
mod contextmenu;
mod editbar;
mod editor_annotate;
mod find;
mod foldreveal;
mod foldsplice;
mod outline_nav;
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod outline_reload_tests;
pub(crate) mod panescroll;
mod reload;
mod rename;
mod save;
mod scrollsync;
#[cfg(test)]
pub(crate) mod testkit;
mod zoom;

use actions::*;
use editbar::*;
use find::*;
use reload::*;
use rename::*;
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
use annotate::{register_annotate_action, register_annotation_step_actions};
pub(crate) use annotations_nav::{reconcile_sidebar_visibility, refresh_annotations};
pub(crate) use chrome::render_and_wire_preview;
pub(crate) use contextmenu::attach_context_menu;
pub(crate) use rename::update_rename_action_state;
// The two find types `TabState` stores. The module itself stays private — the engine is
// window-internal; only the shapes the per-tab state has to *hold* are named crate-wide.
pub(crate) use backingloss::{clear_backing_loss, mark_backing_lost};
pub(crate) use find::{
    row_label, FindCursor, FindHistory, FindOptions, FindScope, PreviewFindCache, PreviewScope,
    RenderKey,
};
pub(crate) use findbar::refresh_preview_find_highlight;
pub(crate) use foldreveal::defer_with_tab;
pub(crate) use foldsplice::splice_disclosure_in_place;
pub(crate) use imagerefusal::install as install_image_refusal_notice;
pub(crate) use lifecycle::quit_all_windows;
pub(crate) use outline_nav::apply_scroll_spy;
pub(crate) use outline_nav::refresh_outline;
pub(crate) use outline_nav::wire_persistent_editor_scroll_spy;
pub(crate) use outline_nav::wire_scroll_spy;
pub(crate) use outline_nav::{outline_collapse_all, outline_expand_all};
pub(crate) use reload::adopt_disk_text;
pub(crate) use reload::check_and_reload;
pub(crate) use reload::check_and_reload_tab;
pub(crate) use reload::write_loaded_text;
pub(crate) use restore::{apply_tab_layout, restore_session};
pub(crate) use save::refresh_dirty_status;
pub(crate) use scrollsync::content_reading_position;
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) use swap::{clear_snapshot_failure_for_test, report_snapshot_failure_for_test};
#[cfg(test)]
pub(crate) use swap::{close_windows_opened_since_for_test, toplevels_for_test};
pub(crate) use swap::{discard_tab_swap, sync_tab_swap, wire_swap_snapshots};
pub(crate) use swaprecovery::recover_after_restore;
pub(crate) use tabs::add_new_document_tab;
pub(crate) use tabs::badge_tab_label;
/// Reached directly only by `farscroll`'s integration tests, so they exercise the
/// editor the app actually ships; carries their cfg, not a bare `#[cfg(test)]`.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) use tabs::build_tab_editor;
pub(crate) use tabs::create_tab_in_window;
pub(crate) use tabs::start_deferred_prerender_pump;
pub(crate) use tabs::update_window_title;
/// Test-only at window level: the tab context menu's own builder reaches it through
/// `tabs`, and the one crate-level consumer is `app::mnemonics`' guard, which derives
/// its check from this enumeration rather than mirroring it.
#[cfg(test)]
pub(crate) use tabs::TabMenuItem;
pub(crate) use tabs::{host_window, window_of_content_box};
pub(crate) use toast::{sync_backing_loss_toast, sync_recovery_toast};
pub(crate) use zoom::rerender_preview_from_live_edit;
pub(crate) use zoom::rerender_preview_in_place;
pub(crate) use zoom::rerender_tab_preview_in_place;
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) use zoom::zoom_css_rule_for_test;
pub(crate) use zoom::RenderShape;

// Decomposed window sub-builders (see TECH.md module map).
mod backingloss;
mod chrome;
mod chrome_fit;
mod copylink;
mod editoractions;
mod export;
mod export_pdf;
mod findbar;
mod imagerefusal;
mod lifecycle;
mod linknav;
mod livepreview;
mod navhistory;
mod restore;
mod sidebar;
mod sidebarfilter;
mod statusbar;
pub(crate) use statusbar::{
    clear_hover_target, defer_until_export_stops, note_buffer_changed, note_selection_changed,
    refresh_position_indicator, refresh_status_indicators, refresh_zoom_indicator,
    set_hover_target, ExportProgress, StatusBar,
};
mod splitview;
mod swap;
mod swaprecovery;
mod tabs;
mod toast;
mod toolbar;
pub(crate) mod toolbarchrome;
mod toolbarmenu;
pub(crate) mod undo;
mod viewactions;
pub(crate) mod wheelcoalesce;
mod zoomwheel;

use chrome::*;
use copylink::*;
use editoractions::*;
use findbar::*;
use lifecycle::*;
pub(crate) use linknav::activate_doc_link;
use livepreview::*;
use navhistory::*;
pub(crate) use navhistory::{
    nav_action_name, record_in_document_jump, refresh_nav_history_actions,
};
use sidebar::*;
pub(crate) use sidebarfilter::{FilterBar, SidebarPaneKind};
pub(crate) use splitview::SplitView;
use tabs::*;
// The tab strip now lives under `crate::widgets::tab`; re-export it here so the
// long-standing `crate::window::TabView` path (used by `winstate` and the
// window sub-builders) keeps resolving after the move to `src/widgets/`.
pub(crate) use crate::widgets::tab::TabView;
use toolbar::*;
use viewactions::*;
pub(crate) use zoomwheel::install as install_zoom_wheel;

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
/// mode is deliberately NOT here — every tab starts at Preview and, when
/// restoring, is switched into its real mode afterward through the actual
/// `win.view-mode` GAction (the split arrangement is app-wide —
/// `window::arrangement`)
/// (`restore::apply_restored_tab_state`) so the content genuinely rebuilds,
/// rather than only a stored flag being set that nothing then reads.
struct WindowInit {
    /// The size to open at. A window with a `place` opens at the place's size
    /// instead, applied by `winplace::present`, which alone fits it to the monitors
    /// here; these then only stand in for a place that recorded no size.
    width: i32,
    height: i32,
    /// Where to show the window (`winplace`), or `None` to let the window manager
    /// place it. Only session restore has one.
    place: Option<winplace::Place>,
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
            place: None,
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
/// current chrome, with no cache to fall out of step with it. The TOOLBAR is not
/// read here — it is app-wide (`toolbarchrome::current`), so there is no "which
/// window's?" question for it to answer.
pub(crate) fn read_window_chrome(window: &ApplicationWindow) -> crate::session::ChromeSession {
    crate::session::ChromeSession {
        show_statusbar: bool_action_state(window, "show-statusbar", true),
        outline_visible: bool_action_state(window, "outline", true),
        annotations_visible: bool_action_state(window, "annotations", false),
        // The divider is geometry, not an action, so it is the one chrome field with
        // no `win.*` toggle to read it off — it comes from the live cache the paned
        // keeps up to date (see `WindowChrome::sidebar_split` for why not the widget).
        sidebar_split: crate::winstate::chrome(window).map_or(
            crate::session::ChromeSession::default().sidebar_split,
            |ch| ch.sidebar_split.get(),
        ),
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
/// The most characters the toolbar's reading-theme combo shows before ellipsising.
///
/// The counterpart to `DOCUMENTS_BUTTON_MAX_CHARS`, and it exists for a sharper reason
/// than tidiness: a theme's display name comes out of a **user-supplied theme file**, so
/// it has no length anyone here controls. The toolbar packs each button at its natural
/// width and takes its own minimum width from the widest of them, so an over-long theme
/// name becomes the window's minimum width — which is the exact shape of the defect that
/// made the toolbar unable to wrap at all (a single ~555px Format box), arriving through
/// a theme file instead of a container. Capping keeps the window's floor a property of
/// the chrome rather than of whatever theme happens to be loaded.
const THEME_BUTTON_MAX_CHARS: usize = 22;

/// The reading-theme combo's label — the theme's symbol and name, ellipsised to
/// [`THEME_BUTTON_MAX_CHARS`]. The menu items it opens are NOT capped: a dropdown sizes
/// itself and never constrains the window, so only the button needs bounding.
pub(crate) fn theme_button_label(name: &str, symbol: Option<&str>) -> String {
    crate::window::tabs::ellipsize(
        &crate::theme::Themes::chooser_label(name, symbol),
        THEME_BUTTON_MAX_CHARS,
    )
}

pub(crate) fn new_window_from_source(
    app: &Application,
    title: &str,
    md: &str,
    file_path: Option<&std::path::Path>,
    source: Option<&ApplicationWindow>,
) -> ApplicationWindow {
    build_window(app, title, md, file_path, &inherit_from(source))
}

/// Hard minimum window width (px) — the sanity-backstop floor enforced via
/// `set_size_request` in [`build_window`].
///
/// This used to be 720 — sized so a single editor/preview pane plus the
/// ~240px outline sidebar stayed comfortable — because the toolbar was a
/// non-wrapping row, and 720 was picked wide enough that the toolbar's own
/// content-derived minimum (for the default 3 sections) rarely exceeded it,
/// leaving THIS explicit floor as the thing that actually stopped a drag.
/// That's backwards now that the toolbar wraps its buttons onto extra rows
/// instead of needing room for all of them on one
/// ([`crate::widgets::wrapbox::ToolbarWrapBox`]): a high explicit floor here
/// would silently override the wrap and reproduce the exact same "stops well
/// above one icon row, never gets a chance to wrap" symptom the wrap was
/// built to fix, since `set_size_request` and the content-derived minimum
/// combine as `MAX(content_derived_minimum, size_request)` — whichever is
/// larger wins, and a stale 720 here would always win on any normal monitor.
///
/// So this is now a bare sanity backstop against a window collapsing to a
/// genuinely unusable width, not a width chosen to fit any particular chrome.
/// The REAL floor above this is whatever the widget tree's own content-derived
/// minimum is — the toolbar's widest single item (wrapping handles the rest),
/// the tab strip, and the outline sidebar — which GTK enforces on its own. See
/// `toolbarchrome::apply_to` (invariant I5).
///
/// **Raising this re-breaks the wrap**, and silently: the floor wins the `MAX`
/// above long before a reader can drag narrow enough to see a second row.
/// `the_toolbar_wraps_instead_of_summing_every_sections_width` (below) is the
/// guard — it fails the moment this creeps past one toolbar item's width — and
/// it is the reason no monitor-aware clamp is applied to this value: at 360 the
/// floor is already below any display the app can be used on at all, so a clamp
/// against monitor geometry could never fire and would only read as if it might.
const MIN_WINDOW_WIDTH: i32 = 360;

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
    // the multi-window zoom regression (GTK4Rs/AP-77). Created empty here so it can be
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

    // Read once, before the window exists: the widest monitor on this desktop.
    // `None` (no display/monitor — headless/test) means the clamp below is a no-op
    // and today's behaviour is unchanged.
    let widest_monitor = chrome_fit::widest_monitor_width();

    let window = ApplicationWindow::builder()
        .application(app)
        .title(title)
        // Never OPEN wider than any monitor here can show. A narrower initial size is
        // still only a request (GTK grows it back up to whatever the content-derived
        // minimum needs), so this never shrinks a window below what it can actually
        // display; it only stops a wider-than-necessary `init.width` — a configured
        // default wider than this screen — from opening already off-edge. A restored
        // window's size is the place's, fitted by `winplace::present`, not this.
        //
        // Clamped to the WIDEST monitor, not the one this window is about to land on:
        // that monitor is the window manager's choice and an unrealized toplevel
        // cannot be asked for it, so guessing narrows a window the reader was going to
        // put on the big screen. See `chrome_fit::widest_monitor_width`.
        .default_width(widest_monitor.map_or(init.width, |m| init.width.min(m)))
        .default_height(init.height)
        // No `.show_menubar(true)`: we build our OWN per-window GtkPopoverMenuBar
        // in `build_chrome` (GTK4Rs/AP-76) so the View ▸ Documents submenu can
        // list THIS window's tabs. The auto-built app menubar would have shared one
        // model across every window.
        // Compile-checked and resolution-tested like every other icon name
        // (GTK4Rs/AP-48). Resolves from the bundled GResource, so it works on
        // Windows and from an uninstalled `cargo run`, not just where
        // `packaging/linux/install.sh` has populated the hicolor theme.
        .icon_name(crate::icons::Icon::App.name())
        .build();

    // Enforce a hard usability floor on the window width. GTK takes
    // `MAX(content_derived_minimum, size_request)`. The toolbar wraps its buttons
    // onto extra rows rather than growing the width floor with every section shown
    // (`widgets::wrapbox::ToolbarWrapBox`), so its content-derived minimum is only
    // ever its widest single item — well under this floor — and THIS
    // `set_size_request` is what actually stops the "drag to a useless sliver"
    // degenerate case, not the toolbar's own content minimum. `-1` height leaves
    // the vertical minimum content-derived. This is the deliberate counterpart to
    // the "no set_size_request, default_width only" min-width geometry (invariant
    // I5): an explicit floor is correct precisely because the content floor is gone.
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
    // load-bearing — see the zoom-provider note above and GTK4Rs/AP-77.
    window.add_css_class(&format!(
        "scrib-win-{}",
        winstate::WindowId::of(&window).raw()
    ));
    zoom_css_provider.load_from_data(&zoom::zoom_css_rule(&window, zoom_level));

    // Keyboard-shortcuts help overlay. `set_help_overlay` owns the
    // window, creates this window's `win.show-help-overlay` action (opened with
    // F1 / Ctrl+?, registered in `setup::register_accelerators`), and hides it on
    // close so it can reopen. Per-window like the menubar (GTK4Rs/AP-76).
    window.set_help_overlay(Some(&crate::app::make_shortcuts_window()));

    // ── toolbar & chrome ─────────────────────────────────────────────────────
    let (toolbar, section_boxes, format_items, heading_btn, tb_edit_btns, documents_btn, theme_btn) =
        build_toolbar();
    let doc_dir = file_path.and_then(|p| p.parent());
    // Allocated before the first render, which names the tab it is shown in.
    let tab_id = winstate::alloc_tab_id();
    let chrome = build_chrome(
        tab_id,
        &window,
        &toolbar,
        md,
        doc_dir,
        zoom_level,
        init.show_unsafe_images,
        init.chrome.sidebar_split,
    );

    // Bind the open-documents combo box to THIS window's `documents_menu` — the
    // same per-window GMenu the menubar's View ▸ Documents submenu observes (built
    // inside `build_chrome`, so it exists only now). One model, two surfaces: any
    // `refresh_documents_menu` rebuild updates both popups and neither can drift.
    documents_btn.set_menu_model(Some(&chrome.documents_menu));

    // Status-bar and sidebar visibility are PER-WINDOW state (the operator's
    // decision), so — exactly like `zoom_level` — they arrive threaded through
    // this window's own `WindowInit` and are never read from an app-wide place.
    // Whoever built this window already decided which window's chrome it starts
    // with: `inherit_from` (the source/active window's own live values, for
    // every new-window path), `restore::restore_window` (THIS window's own
    // persisted `WindowSession::chrome`), or `WindowInit::default()` (all shown
    // — only the genuine no-source startup fallbacks). The TOOLBAR is not among
    // them: it is app-wide, so it is read from the application below rather than
    // threaded through here (`toolbarchrome`).
    let chrome_init = init.chrome;

    // ── action registration ──────────────────────────────────────────────────
    register_editor_actions(&window, &heading_btn);
    // File ▸ Export. One action for both sinks, sensitivity from one gate — the same
    // single-source-of-truth shape every other command has (POLICY; ScrAP-9).
    export::register_export_action(&window);
    register_annotate_action(&window);
    register_annotation_step_actions(&window);
    // Before the view actions: the split forwarders seed their ticks from it.
    arrangement::ensure_registered(app);
    // The app-wide toolbar layout, registered (and seeded from the session) by
    // whichever window is built first; this window then takes the LIVE value.
    toolbarchrome::ensure_registered(app);
    toolbarchrome::seed_window(&window, &toolbar, &section_boxes);
    // Right-click on the bar offers the same toggles as View ▸ Toolbar (TDD 9.40).
    toolbarmenu::attach_toolbar_menu(&toolbar);
    register_view_actions(
        &window,
        &chrome.status_bar,
        &SidebarSections {
            outline_section: &chrome.outline_section,
            annotations_section: &chrome.annotations_section,
            sidebar_paned: &chrome.sidebar_paned,
        },
        &ChromeVisibility {
            show_statusbar: chrome_init.show_statusbar,
            // Per-TAB, unlike the window-scoped chrome around it — this tab's
            // own restored value.
            show_unsafe_images: init.show_unsafe_images,
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
    // App-wide split arrangement: a freshly built `SplitView` starts un-swapped
    // and side by side, so apply the live value — the same seed
    // `create_tab_in_window` gives every later tab.
    core.split.set_arrangement(arrangement::current(app));

    // ── window-level find/format furniture (built ONCE per window, not per tab) ─
    // The find bar's shared widgets live in WindowChrome; its closures fetch the
    // active tab's engine fresh via state(window). Wired here against the first
    // tab's search context.
    wire_find_bar(&window, &chrome);
    // Gate the Format and Go To Line commands on editor focus.
    setup_editor_focus_gate(&window, &format_items, &chrome.find_bar_revealer);
    // Stage-2 caret formatting overlay: ONE per window (GTK4Rs/AP-106), stored in
    // WindowChrome and re-parented to the active tab's editor on every switch.
    // Built once here — its heading-menu font resolution happens once ever, O(1)
    // (GTK4Rs/AP-106). `assemble_tab_core` already wired this first tab's
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
        toolbar,
        section_boxes,
        tb_edit_btns,
        documents_btn,
        theme_btn,
        format_overlay,
        ov_edit_btns,
        zoom_level,
        chrome_init.sidebar_split,
        zoom_css_provider,
    );
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
    // so attach the window's caret card over its editor here — every later tab
    // re-targets it on its own activation
    // (`on_active_tab_changed::retarget_format_overlay`). This first attachment is
    // what triggers the card's one-and-only heading-menu font resolution.
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
    // buffer `has-selection` and primary-clipboard (`GTK4Rs/AP-28` table-cell) handlers
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

    winplace::watch(&window);
    match &init.place {
        Some(place) => winplace::present(&window, place),
        None => window.present(),
    }
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
    toolbar: crate::widgets::wrapbox::ToolbarWrapBox,
    toolbar_sections: [Vec<gtk::Widget>; 6],
    tb_edit_btns: Vec<(FmtInsertKind, gtk::Button)>,
    documents_btn: gtk::MenuButton,
    theme_btn: gtk::MenuButton,
    format_overlay: crate::widgets::floating_card::FloatingCard,
    ov_edit_btns: Vec<(FmtInsertKind, gtk::Button)>,
    zoom_level: f64,
    sidebar_split: f64,
    zoom_css_provider: gtk::CssProvider,
) -> Rc<winstate::WindowChrome> {
    Rc::new(winstate::WindowChrome {
        toolbar,
        toolbar_sections,
        outline_scroller: chrome.outline_scroller.clone(),
        annotations_scroller: chrome.annotations_scroller.clone(),
        conflict_toast: chrome.conflict_toast.clone(),
        recovery_toast: chrome.recovery_toast.clone(),
        recovery_toast_label: chrome.recovery_toast_label.clone(),
        backing_loss_toast: chrome.backing_loss_toast.clone(),
        backing_loss_toast_label: chrome.backing_loss_toast_label.clone(),
        info_toast: chrome.info_toast.clone(),
        status: RefCell::new(winstate::StatusStack::new(chrome.statusbar.message.clone())),
        statusbar: chrome.statusbar.clone(),
        annotations_title: chrome.annotations_title.clone(),
        outline_filter: chrome.outline_filter.clone(),
        annotations_filter: chrome.annotations_filter.clone(),
        text_stats_timer: RefCell::new(None),
        selection_count: Cell::new(None),
        export_op: RefCell::new(None),
        after_export: RefCell::new(Vec::new()),
        find_bar_revealer: chrome.find_bar_revealer.clone(),
        find_entry: chrome.find_entry.clone(),
        match_count_label: chrome.match_count_label.clone(),
        replace_row: chrome.replace_row.clone(),
        replace_entry: chrome.replace_entry.clone(),
        find_history_btn: chrome.find_history_btn.clone(),
        replace_history_btn: chrome.replace_history_btn.clone(),
        fmt_edit_btns: tb_edit_btns,
        documents_btn,
        theme_btn,
        format_overlay,
        format_overlay_timer: RefCell::new(None),
        overlay_edit_btns: ov_edit_btns,
        fmt_edit_state: Cell::new(None),
        focused_pane: Cell::new(winstate::FocusedPane::Editor),
        ctx_link: RefCell::new(None),
        zoom_level: Cell::new(zoom_level),
        sidebar_split: Cell::new(sidebar_split),
        zoom_css_provider,
        tabs: chrome.tabs.clone(),
        documents_menu: chrome.documents_menu.clone(),
        format_insert_menu: chrome.format_insert_menu.clone(),
        menubar: chrome.menubar.clone(),
        menu_model: chrome.menu_model.clone(),
        format_menu_kind: Cell::new(None),
        format_menu_pending: Cell::new(None),
        format_menu_refresh_scheduled: Cell::new(false),
        documents_refresh_scheduled: Cell::new(false),
    })
}

/// Wire the tab strip's active-tab-changed callback plus the drag-and-drop /
/// close / context-menu handlers every window's strip needs. All are wired
/// unconditionally: `move_tab_to_new_window` relies on `wire_tab_arrival` being
/// present on every window it might build (see tabs.rs), and `wire_tab_bar_dnd`
/// owns the single `GtkDragSource`/`GtkDropTarget` pair that drives in-strip
/// reorder AND cross-window move AND drag-to-desktop (module doc, widgets/tab) —
/// no per-tab-label wiring needed anymore (GTK4Rs/AP-60 is now moot: there is no
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
    actions::connect_selection_states_on_activation(window);
}

/// Register all of the window's teardown handlers in one place, in the order
/// their firing depends on. `connect_destroy` handlers fire in connection order,
/// so this order is load-bearing:
/// 1. Cancel this window's pending per-window timers, and detach the single
///    caret card from its pane, while the chrome is still registered.
///    Destroying a window does not remove a GLib source armed
///    against it: every timer left running here outlives the window and fires
///    into a torn-down one, which is the class GTK4Rs/AP-128 records.
/// 2. Remove this window's zoom CSS rule from the shared display (the provider
///    reference is held by this closure so it outlives the window).
/// 3. Unregister the window's typed state LAST, so both handlers above still
///    resolve it via `winstate::chrome`/`state`.
fn register_window_destroy_handlers(window: &ApplicationWindow, zoom_provider: gtk::CssProvider) {
    window.connect_destroy(|w| {
        if let Some(chrome) = winstate::chrome(w) {
            // Cancel any pending ~40 ms format-overlay timer so it can't fire a
            // `popup()` during window teardown (GTK4Rs/AP-128/GTK4Rs/AP-128). Belt to the timer body's
            // own `is_realized()` gate; `take()` no-ops if it already fired.
            if let Some(id) = chrome.format_overlay_timer.borrow_mut().take() {
                id.remove();
            }
            // The status bar's word-count debounce, for the same reason: it is armed
            // per window from the buffer's `changed`, so a window built and destroyed
            // without ever settling leaves it running against a chrome nobody can see.
            // `take()` no-ops if it already fired. The selection count arms no timer at
            // all — see `window::statusbar::note_selection_changed`.
            if let Some(id) = chrome.text_stats_timer.borrow_mut().take() {
                id.remove();
            }
            crate::widgets::floating_card::FloatingCardExt::detach(&chrome.format_overlay);
        }
    });
    window.connect_destroy(move |_| {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_remove_provider_for_display(&display, &zoom_provider);
        }
    });
    window.connect_destroy(winstate::unregister);
}

/// `pub(crate)` rather than private: `test_app` is the one helper that builds a real
/// application, and other modules' integration tests need a window assembled by the
/// PRODUCTION path rather than a stand-in they wired themselves — which is exactly the
/// distinction a wiring test cannot make about itself (`clipboard`'s middle-click pair).
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) mod gtk_integration_tests {
    use super::*;

    /// Build a registered, non-unique test app.
    ///
    /// `pub(super)` so every GTK test under `window/` reaches the SAME helper rather than
    /// each re-deriving one. A helper that only its own module can import is a helper the
    /// next module will quietly reimplement, slightly differently (GEP-25).
    // M37's one home for this helper, re-exported at `pub(crate)` because other modules'
    // integration tests need a window built by the PRODUCTION path rather than a stand-in
    // they wired themselves — `clipboard`'s middle-click pair is exactly that distinction.
    pub(crate) use crate::window::testkit::test_app;

    /// The chrome toggles a WINDOW owns, as `win.*` action names. The toolbar is
    /// deliberately absent: it is app-wide (`toolbarchrome`), so there is nothing
    /// per-window about it to inherit or to keep independent, and asserting on it
    /// here would assert the opposite of its contract.
    ///
    /// These do NOT share one default — the status bar and outline start shown, the
    /// annotations viewer starts hidden — so a test over this list asserts a FLIP
    /// from whatever each one currently is, never a literal `true`. Anchoring on a
    /// literal is what made the list silently wrong to extend.
    const CHROME_ACTIONS: [&str; 3] = ["show-statusbar", "outline", "annotations"];

    /// A window spawned from a source window inherits that window's chrome —
    /// the per-window counterpart of zoom's inheritance, and for the same
    /// reason: a brand-new window has nothing else to constrain its
    /// window-scoped state, so the source window is the only evidence of what
    /// the user wants. Covers every per-window toggle through the real
    /// `new_window_from_source` funnel every new-window path uses.
    #[gtktest::test]
    fn a_new_window_inherits_every_chrome_toggle_from_its_source_window() {
        let app = test_app("com.extollit.scribobulate.integrationtest.chromeinherit");

        let win_a = new_window(&app, "IT-A", "alpha", None);
        // Flip each toggle away from its own default, whatever that is — see
        // CHROME_ACTIONS on why a literal would not do. The flipped value is then
        // what B must show: matching a default would prove nothing.
        let flipped: Vec<(&str, bool)> = CHROME_ACTIONS
            .iter()
            .map(|&name| {
                let want = !bool_action_state(&win_a, name, false);
                change_action_state(&win_a, name, &want.to_variant());
                assert_eq!(
                    bool_action_state(&win_a, name, !want),
                    want,
                    "{name} flipped in the source window"
                );
                (name, want)
            })
            .collect();

        let win_b = new_window_from_source(&app, "IT-B", "beta", None, Some(&win_a));
        for &(name, want) in &flipped {
            assert_eq!(
                bool_action_state(&win_b, name, !want),
                want,
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
        change_action_state(&win_a, "show-statusbar", &false.to_variant());

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
            !bool_action_state(&win_b, "show-statusbar", true),
            "win.new-window must inherit the active window's chrome"
        );
    }

    /// A window with NO source window (the app-startup fallbacks) starts at each
    /// toggle's own documented default. The fresh default is only correct when
    /// there is genuinely nothing to inherit from — which is exactly what
    /// `new_window` means.
    ///
    /// The defaults are named here rather than derived from `ChromeSession::default()`,
    /// because deriving would make this test agree with the code by construction and
    /// stop noticing a default that silently changed.
    #[gtktest::test]
    fn a_window_with_no_source_starts_at_the_documented_chrome_defaults() {
        let app = test_app("com.extollit.scribobulate.integrationtest.nosourcechrome");
        let win = new_window(&app, "IT", "alpha", None);
        for (name, want) in [
            ("show-statusbar", true),
            ("outline", true),
            // Hidden by default — most documents carry no annotations (TDD 20.13).
            ("annotations", false),
        ] {
            assert_eq!(
                bool_action_state(&win, name, !want),
                want,
                "{name} must start at its documented default"
            );
        }
    }

    /// The toolbar's own content-derived minimum width, with every section
    /// shown, must stay small — proving the `ToolbarWrapBox` wrap (TDD 9.38)
    /// is actually doing its job and not silently
    /// degrading back to a non-wrapping row's ~1633px sum-of-all-sections
    /// minimum. Pins the exact symptom a prior pass missed: the wrap can be
    /// measurably correct in isolation while an unrelated, unchanged
    /// `set_size_request` floor elsewhere still masks it end-to-end — this
    /// test targets the toolbar's OWN minimum specifically, so a regression
    /// here can't hide behind that floor the way the user-visible one did.
    #[gtktest::test]
    fn the_toolbar_wraps_instead_of_summing_every_sections_width() {
        let app = test_app("com.extollit.scribobulate.integrationtest.toolbarwrap");
        let win = new_window(&app, "IT-wrap", "# H\n\ntext", None);
        for id in crate::app::TBTN_SECTION_IDS {
            // App-scoped: the toolbar layout is one app-wide preference
            // (`toolbarchrome`), so this reaches every window, this one included.
            app.change_action_state(&toolbarchrome::section_action(id), &true.to_variant());
        }
        win.present();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(200),
        );

        // Walk the whole tree (first-child AND next-sibling — the toolbar is
        // `outer_box`'s SECOND child when the menubar is present) to find the
        // toolbar's `ToolbarWrapBox` — there's no stored handle on `Chrome`
        // (it's swapped into `outer_box` by value in `build_chrome` and never
        // kept), so this is the same walk a reader inspecting the live tree
        // would do.
        fn find_wrap_box(widget: &gtk::Widget) -> Option<crate::widgets::wrapbox::ToolbarWrapBox> {
            if let Ok(wb) = widget
                .clone()
                .downcast::<crate::widgets::wrapbox::ToolbarWrapBox>()
            {
                return Some(wb);
            }
            let mut child = widget.first_child();
            while let Some(c) = child {
                if let Some(wb) = find_wrap_box(&c) {
                    return Some(wb);
                }
                child = c.next_sibling();
            }
            None
        }
        let toolbar = find_wrap_box(&win.child().expect("window has a child"))
            .expect("toolbar ToolbarWrapBox must be somewhere under the window's child");

        // Each section's OWN natural width (the wrap box's direct children —
        // no wrapper widget in between, unlike GtkFlowBoxChild) — the wrap
        // box's minimum, with wrap working, should land close to the single
        // WIDEST of these (whichever section that is), never near their sum.
        let mut widest_section = 0;
        let mut child = toolbar.first_child();
        while let Some(c) = child {
            let (_, snat, _, _) = c.measure(gtk::Orientation::Horizontal, -1);
            widest_section = widest_section.max(snat);
            child = c.next_sibling();
        }

        let (min_w, _, _, _) = toolbar.measure(gtk::Orientation::Horizontal, -1);
        assert!(
            min_w < widest_section + 50,
            "toolbar's content-derived minimum width with all six sections shown was \
             {min_w}px, but the single widest section only needs {widest_section}px — a \
             wrapping toolbar's minimum should track the widest SECTION, never the sum of \
             every section (which would put it well over 2000px here), so this gap means \
             the ToolbarWrapBox wrap has regressed"
        );

        // The end state this whole change was for, and the one assertion worth
        // keeping now that it holds: with EVERY section shown, the toolbar does
        // not set the window's floor at all — `MIN_WINDOW_WIDTH`, a deliberate
        // sanity backstop, does. That is what lets the window reach a narrow
        // display no matter which sections a reader has ticked.
        //
        // This replaces an earlier assertion that `MIN_WINDOW_WIDTH` must stay
        // BELOW a section's width. That was true only while the Format bar was
        // still one opaque ~555px item and so still dominated the floor; it
        // encoded the half-finished state as the requirement, and it would now
        // fail against the finished one. The guard against the original bug —
        // a too-high explicit floor silently masking the wrap — is preserved,
        // because a `MIN_WINDOW_WIDTH` raised back toward a chrome-fitting
        // width would break the inequality below from the other side.
        assert!(
            min_w < MIN_WINDOW_WIDTH,
            "the toolbar's own minimum width with all six sections shown is {min_w}px, which \
             is at or above MIN_WINDOW_WIDTH ({MIN_WINDOW_WIDTH}px) — so the TOOLBAR is once \
             again setting the window's floor and a narrow display cannot fit the window \
             whatever the reader hides. Either an item stopped wrapping (a section packed \
             into one container again) or a single item grew unbounded (a label showing a \
             value with no cap — see THEME_BUTTON_MAX_CHARS and DOCUMENTS_BUTTON_MAX_CHARS)"
        );
    }

    /// The whole point of the wrap, asserted end to end rather than on the
    /// toolbar alone: with **every** toolbar section shown, both sidebars open
    /// and a document whose headings and file name are long enough to stretch
    /// anything that stretches, the WINDOW's own minimum width is still only
    /// `MIN_WINDOW_WIDTH` — i.e. no piece of chrome sets the floor, so the app
    /// fits a genuinely narrow display no matter how it is configured.
    ///
    /// `the_toolbar_wraps_instead_of_summing_every_sections_width` guards the
    /// toolbar's own contribution. This one exists because that is not the same
    /// claim: the toolbar was merely the largest of several contributors, and a
    /// future regression in the sidebar, the find bar or the status bar would
    /// put the floor back up while leaving the toolbar test perfectly green.
    #[gtktest::test]
    fn no_chrome_sets_the_windows_width_floor_above_the_backstop() {
        let app = test_app("com.extollit.scribobulate.integrationtest.widthfloor");
        let doc = "# Supercalifragilisticexpialidocious Heading That Refuses To End\n\n\
                   ## Another Extremely Long Heading For The Outline To Chew On\n\ntext\n";
        let win = new_window(&app, "IT-widthfloor", doc, None);
        for id in crate::app::TBTN_SECTION_IDS {
            // App-scoped: the toolbar layout is one app-wide preference
            // (`toolbarchrome`), so this reaches every window, this one included.
            app.change_action_state(&toolbarchrome::section_action(id), &true.to_variant());
        }
        change_action_state(&win, "outline", &true.to_variant());
        change_action_state(&win, "annotations", &true.to_variant());
        // The find bar OPEN, with its replace row shown, so its option toggles and both
        // its text fields are in the tree. It is the second-largest contributor after
        // the toolbar, and the one that grew most recently: a field with no width cap
        // makes its row's minimum the field's natural width, and that is the whole
        // failure this assertion is placed here to see.
        actions::simple_action(&win, "find-replace")
            .expect("win.find-replace is registered")
            .activate(None);
        win.present();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(400),
        );

        let (win_min, _, _, _) = win.measure(gtk::Orientation::Horizontal, -1);
        assert_eq!(
            win_min, MIN_WINDOW_WIDTH,
            "the window's minimum width is {win_min}px with everything shown, but \
             MIN_WINDOW_WIDTH is {MIN_WINDOW_WIDTH}px — some piece of chrome is setting the \
             floor instead of the backstop, so a narrow display can no longer fit the \
             window. Measure each direct child of the window's root box to find which: the \
             toolbar, the sidebar paned, the find-bar revealer and the status bar are the \
             four that have ever been the answer. The find bar is open here, so an \
             uncapped find or replace field is one of the candidates"
        );
    }

    /// The editor-focus gate stays open while focus is on a Format toolbar
    /// button — the "sticky" clause of `setup_editor_focus_gate`, which is what
    /// stops `win.format` greying itself out the instant the user reaches for
    /// the control they are trying to use.
    ///
    /// **Worth a test of its own because this clause can only fail silently.**
    /// It used to be one `is_ancestor(format_box)` call against the single box
    /// the Format section was packed into. That box is gone — Format is now a
    /// flat list of individually-wrappable items, so the window can get narrow
    /// — and the gate tests membership against that list instead. Both spellings
    /// compile and both look right; if the list ever stops being the widgets the
    /// toolbar actually shows, the gate simply starts closing on a Format press
    /// and nothing anywhere says so.
    #[gtktest::test]
    fn focus_on_a_format_button_does_not_close_the_editor_gate() {
        let app = test_app("com.extollit.scribobulate.integrationtest.fmtgate");
        let win = new_window(&app, "IT-fmtgate", "# H\n\nsome text", None);
        app.change_action_state(&toolbarchrome::section_action("format"), &true.to_variant());
        change_action_state(&win, "view-mode", &"edit".to_variant());
        win.present();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(300),
        );

        let editor = state(&win).expect("a tab").editor.clone();
        editor.grab_focus();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(200),
        );
        assert!(
            simple_action(&win, "format")
                .expect("win.format")
                .is_enabled(),
            "precondition: editor focus opens the gate"
        );

        // The Bold button, found the way a reader reaches it — by walking the
        // live toolbar — rather than from a handle the test was handed, so the
        // test fails if the button the toolbar shows is not the one the gate
        // was told about.
        fn find_bold(w: &gtk::Widget) -> Option<gtk::Widget> {
            if let Some(b) = w.clone().downcast_ref::<gtk::Button>() {
                if b.action_name().as_deref() == Some("win.format")
                    && b.action_target_value()
                        .and_then(|v| v.str().map(|s| s.to_string()))
                        .as_deref()
                        == Some("bold")
                {
                    return Some(w.clone());
                }
            }
            let mut c = w.first_child();
            while let Some(ch) = c {
                if let Some(found) = find_bold(&ch) {
                    return Some(found);
                }
                c = ch.next_sibling();
            }
            None
        }
        let bold = find_bold(&win.child().expect("window child"))
            .expect("a win.format::bold button is somewhere in the toolbar");

        bold.grab_focus();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(200),
        );
        assert!(
            simple_action(&win, "format")
                .expect("win.format")
                .is_enabled(),
            "focus moved onto a Format toolbar button and the editor gate CLOSED — \
             win.format is now disabled, so pressing the button a reader just reached \
             for would do nothing. The gate's sticky clause no longer recognises that \
             widget as part of the Format surface: it tests membership against the item \
             list handed to `setup_editor_focus_gate`, so that list and the widgets the \
             toolbar actually packs have drifted apart"
        );
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

            // Hide A's status bar and outline; B is left entirely untouched.
            change_action_state(&win_a, "show-statusbar", &false.to_variant());
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
                    .any(|w| !w.chrome.show_statusbar && !w.chrome.outline_visible),
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
                    .any(|w| w.chrome.show_statusbar && w.chrome.outline_visible),
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

            // B's state BEFORE anything is touched, so the assertion below is
            // "unchanged" rather than a literal that only half the list satisfies.
            let b_before: Vec<(&str, bool)> = CHROME_ACTIONS
                .iter()
                .map(|&name| (name, bool_action_state(&win_b, name, false)))
                .collect();

            for name in CHROME_ACTIONS {
                let want = !bool_action_state(&win_a, name, false);
                change_action_state(&win_a, name, &want.to_variant());
            }

            // Runtime: B's own toggles are untouched.
            for &(name, before) in &b_before {
                assert_eq!(
                    bool_action_state(&win_b, name, !before),
                    before,
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
                .filter(|w| !w.chrome.show_statusbar)
                .count();
            assert_eq!(
                hidden,
                1,
                "exactly one of the two windows has its status bar hidden: {:?}",
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
                show_statusbar: false,
                outline_visible: false,
                annotations_visible: false,
                sidebar_split: crate::session::ChromeSession::default().sidebar_split,
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

            let outlines: Vec<bool> = windows
                .iter()
                .map(|w| bool_action_state(w, "outline", true))
                .collect();
            assert!(
                outlines.contains(&true) && outlines.contains(&false),
                "each restored window must get its OWN persisted chrome, not one \
                 shared answer: {outlines:?}"
            );

            // The window restored hidden must also have its sidebar widget
            // actually hidden — not merely the action state set.
            let hidden_win = windows
                .iter()
                .find(|w| !bool_action_state(w, "outline", true))
                .expect("one window restored with its outline hidden");
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

    /// The sidebar `GtkPaned`, resolved the way production resolves it: by ancestry
    /// from a section's scroller, never from a stored handle.
    ///
    /// Deliberately the same walk as `reconcile_sidebar_visibility`, so a layout change
    /// that breaks that function breaks these tests too — a helper that took a shortcut
    /// the production code cannot take would assert about a tree nobody navigates.
    fn sidebar_paned_of(scroller: &gtk::ScrolledWindow) -> gtk::Paned {
        scroller
            .parent()
            .expect("scroller sits in its section")
            .parent()
            .expect("section sits in the sidebar")
            .downcast::<gtk::Paned>()
            .expect("the sidebar is the vertical GtkPaned, one level above a section")
    }

    /// The sidebar container is a `GtkPaned` exactly ONE level above each section
    /// (TDD 20.21).
    ///
    /// `reconcile_sidebar_visibility` resolves it by ancestry rather than by a stored
    /// handle, so this is the shape that function assumes and nothing else checks:
    /// wrapping the two sections in one more container would leave it setting
    /// `:visible` on an inner widget while the real sidebar stayed shown — a
    /// four-state rule that silently stops working, with the build still compiling.
    #[gtktest::test]
    fn the_sidebar_container_is_the_pane_one_level_above_a_section() {
        let app = test_app("com.extollit.scribobulate.integrationtest.sidebarshape");
        let win = new_window(&app, "IT-shape", "# H\n\ntext {>>note<<}", None);
        let ch = winstate::chrome(&win).expect("chrome registered");
        for scroller in [&ch.outline_scroller, &ch.annotations_scroller] {
            let sidebar = sidebar_paned_of(scroller);
            assert_eq!(sidebar.orientation(), gtk::Orientation::Vertical);
        }
        assert_eq!(
            sidebar_paned_of(&ch.outline_scroller),
            sidebar_paned_of(&ch.annotations_scroller),
            "both sections hang off the SAME paned — one divider, not two sidebars"
        );
    }

    /// Hiding a section and showing it again returns the reader to the divider position
    /// they chose (TDD 20.21).
    ///
    /// GtkPaned only holds this because `size_allocate` skips `calc_position` entirely
    /// while one child is invisible (gtkpaned.c 4.6.9 `:1380-1408`) — a property of the
    /// widget, not of anything this code does, which is exactly why it is asserted here
    /// rather than assumed: a future re-layout that hid a section by some other means
    /// (rebuilding the pane, reparenting it) would lose the position with nothing else
    /// to notice.
    #[gtktest::test]
    fn the_sidebar_divider_position_survives_a_pane_toggle_round_trip() {
        let app = test_app("com.extollit.scribobulate.integrationtest.sidebardivider");
        let win = new_window(&app, "IT-divider", "# H\n\ntext {>>note<<}", None);
        win.set_default_size(800, 600);
        win.present();
        change_action_state(&win, "annotations", &true.to_variant());
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(400),
        );
        let ch = winstate::chrome(&win).expect("chrome registered");
        let sidebar = sidebar_paned_of(&ch.outline_scroller);
        // Waited for, not assumed from the drain above: the first window of a fresh
        // process on Quartz can take longer than any fixed span to be allocated, and
        // an unallocated paned reads position 0 and height 0.
        crate::testpump::until(
            crate::testpump::Clock::Frame,
            "the sidebar is allocated",
            || sidebar.height() > 0,
        );

        // Move the divider somewhere the default split would never land on its own.
        let chosen = sidebar.position() + 40;
        sidebar.set_position(chosen);
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(200),
        );
        assert_eq!(
            sidebar.position(),
            chosen,
            "precondition: the divider took the position we asked for"
        );

        for on in [false, true] {
            change_action_state(&win, "annotations", &on.to_variant());
            crate::testpump::drain_for(
                crate::testpump::Clock::Frame,
                std::time::Duration::from_millis(200),
            );
        }
        assert_eq!(
            sidebar.position(),
            chosen,
            "hiding a section and showing it again keeps the reader's split"
        );
    }

    /// A window opens on the divider position it was restored with, and reports back the
    /// position the reader dragged it to (TDD 20.21) — the two halves of "per window,
    /// across process lifecycles", each of which is silently useless without the other.
    ///
    /// The restore half needs a REAL allocation to be meaningful: a fraction has nothing
    /// to multiply until the sidebar has a height, which is why `restore_sidebar_split`
    /// hangs off `notify::max-position` rather than build time or `map`. So this test
    /// presents the window and pumps the frame clock rather than asserting on a freshly
    /// built tree, where the position would be a clamped zero for either implementation
    /// and the assertion could not tell them apart (ScrAP-78).
    #[gtktest::test]
    fn the_sidebar_divider_is_restored_from_the_session_and_read_back_for_it() {
        let app = test_app("com.extollit.scribobulate.integrationtest.sidebarpersist");
        let win = build_window(
            &app,
            "IT-persist",
            "# H\n\ntext {>>note<<}",
            None,
            &WindowInit {
                chrome: crate::session::ChromeSession {
                    annotations_visible: true,
                    sidebar_split: 0.25,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        // Tall enough that a quarter of the sidebar clears the outline section's minimum
        // height; asserted below, because under that floor the divider is clamped
        // (`shrink=false`, TDD 20.21) and this test would be measuring the clamp.
        win.set_default_size(800, 760);
        win.present();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(400),
        );
        let ch = winstate::chrome(&win).expect("chrome registered");
        let sidebar = sidebar_paned_of(&ch.outline_scroller);
        // Waited for, not assumed from the drain above: the first window of a fresh
        // process on Quartz can take longer than any fixed span to be allocated, and
        // an unallocated paned reads position 0 and height 0.
        crate::testpump::until(
            crate::testpump::Clock::Frame,
            "the sidebar is allocated",
            || sidebar.height() > 0,
        );

        let quarter = crate::session::sidebar_divider_position(0.25, sidebar.height())
            .expect("the sidebar has a height once presented");
        let (outline_min, _, _, _) = sidebar
            .start_child()
            .expect("the outline section is the start child")
            .measure(gtk::Orientation::Vertical, -1);
        assert!(
            quarter >= outline_min,
            "precondition: a quarter of the sidebar ({quarter}px) must clear the outline \
             section's minimum ({outline_min}px), or the divider is clamped to that floor"
        );
        assert_eq!(
            sidebar.position(),
            quarter,
            "the window opened on its restored split, not the even default"
        );

        // ...and the read-back half: drag it somewhere else, and that is what a session
        // save would write down for THIS window.
        let moved = sidebar.height() * 2 / 3;
        sidebar.set_position(moved);
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(200),
        );
        let persisted = read_window_chrome(&win).sidebar_split;
        let expected = crate::session::sidebar_split_fraction(sidebar.position(), sidebar.height())
            .expect("a dragged divider is a meaningful reading");
        assert!(
            (persisted - expected).abs() < 1e-9,
            "read_window_chrome persists the dragged split ({persisted} vs {expected})"
        );
        assert!(
            (persisted - 0.25).abs() > 1e-9,
            "precondition: the drag actually moved the divider off its restored value"
        );
    }

    /// The divider cannot take a section away — only its action can (TDD 20.21).
    ///
    /// This is what `shrink_*_child(false)` buys, and it is load-bearing rather than
    /// cosmetic: a section draggable to zero would be gone from the screen while
    /// `win.outline` / `win.annotations` still reported it shown, which is precisely the
    /// divergence the four-state rule (TDD 20.9) exists to prevent.
    #[gtktest::test]
    fn the_divider_cannot_crush_a_sidebar_section_away() {
        let app = test_app("com.extollit.scribobulate.integrationtest.sidebarfloor");
        let win = new_window(&app, "IT-floor", "# H\n\ntext {>>note<<}", None);
        win.set_default_size(800, 600);
        win.present();
        change_action_state(&win, "annotations", &true.to_variant());
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(400),
        );
        let ch = winstate::chrome(&win).expect("chrome registered");
        let sidebar = sidebar_paned_of(&ch.outline_scroller);

        // The scroller's own min-content-height, which `shrink=false` promotes into the
        // divider's travel limit. Named rather than repeated, so the two cannot drift.
        const SECTION_FLOOR: i32 = 80;
        // Assert on the PANED'S POSITION, never on a section's `height()`. A crushed
        // child does not report a small height: `gtk_paned_size_allocate` hands it its
        // NATURAL height and shifts it off the top of the pane instead
        // (`start_child_allocation.y -= …`, gtkpaned.c 4.6.9 `:1370-1374`), so a section
        // dragged to nothing still measures its full ~110px while being invisible on
        // screen. Measured: with `shrink=true` this test PASSED on `height()` alone
        // (GTK4Rs/AP-317) — the guard's original form could not see the very thing it was
        // written to catch. The position IS the start child's allocated share, and it is
        // what GTK clamps, so it answers the question `height()` only appears to.
        for extreme in [0, i32::MAX / 2] {
            sidebar.set_position(extreme);
            crate::testpump::drain_for(
                crate::testpump::Clock::Frame,
                std::time::Duration::from_millis(200),
            );
            let (split, total) = (sidebar.position(), sidebar.height());
            assert!(
                split >= SECTION_FLOOR,
                "dragging the divider to {extreme} left the outline {split}px of \
                 {total}px, below its {SECTION_FLOOR}px floor — it can now be hidden by \
                 drag, behind its action's back"
            );
            assert!(
                total - split >= SECTION_FLOOR,
                "dragging the divider to {extreme} left the annotations {}px of \
                 {total}px, below its {SECTION_FLOOR}px floor — it can now be hidden by \
                 drag, behind its action's back",
                total - split
            );
        }
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
        annotations_nav::navigate_to_annotation(&win, src_start, crate::codeview::CardFocus::Leave);

        let st = state(&win).expect("tab state");
        let expected_char = md[..src_start.raw()].chars().count() as i32;
        assert_eq!(
            st.editor_buf.property::<i32>("cursor-position"),
            expected_char,
            "split-mode activation places the editor caret on the annotation, char-correct \
             past multi-byte text (TDD 20.4 / 20.14)"
        );
    }

    /// **Pure-edit mode has an annotation walk at all.** The command used to reach for
    /// the preview's marker layer directly, so in edit mode — where there is no preview
    /// view — it returned before doing anything: the mode a reviewer does most of their
    /// keyboard work in had no Next/Previous Annotation, silently, and because the
    /// action is deliberately always-enabled there was not even a greyed-out control to
    /// say so.
    ///
    /// Two annotations and both directions, because a one-annotation forward-only check
    /// would pass on an implementation that always answers "the first one".
    #[gtktest::test]
    fn edit_mode_steps_the_editor_caret_between_annotations_in_both_directions() {
        let app = test_app("com.extollit.scribobulate.integrationtest.annstepedit");
        // Multi-byte text before BOTH annotations, so a byte-vs-char slip in the caret
        // conversion would land the walk on the wrong character (TDD 20.14's hazard,
        // reached from the caret side).
        let md = "café ☕ {==first==}{>>note one<<} then ☕☕ {==second==}{>>note two<<} end.";
        let win = new_window(&app, "IT-annstepedit", md, None);
        change_action_state(&win, "view-mode", &"edit".to_variant());

        let entries = crate::annotations::extract_entries(md);
        assert_eq!(entries.len(), 2, "fixture has two annotations");
        let char_of = |src: crate::span::OriginalByteOffset| md[..src.raw()].chars().count() as i32;
        let st = state(&win).expect("tab state");
        let caret = || st.editor_buf.property::<i32>("cursor-position");

        st.editor_buf.place_cursor(&st.editor_buf.start_iter());
        annotations_nav::step_annotation(&win, crate::annotations::Direction::Next);
        assert_eq!(
            caret(),
            char_of(entries[0].src_span.start),
            "the first step in edit mode must move the editor caret to the first annotation"
        );

        annotations_nav::step_annotation(&win, crate::annotations::Direction::Next);
        assert_eq!(
            caret(),
            char_of(entries[1].src_span.start),
            "and the second step must ADVANCE — the walk is measured from the caret it \
             just moved, so a step that did not move it would repeat forever"
        );

        annotations_nav::step_annotation(&win, crate::annotations::Direction::Previous);
        assert_eq!(
            caret(),
            char_of(entries[0].src_span.start),
            "Previous must come back one, not lap the document"
        );
    }

    /// Revealing a sidebar pane focuses its list, because nothing else can: the lists sit
    /// several Tab stops behind the tab bar and the pane's own ×, and a reader who showed
    /// the annotations viewer in order to use it was left unable to reach it.
    ///
    /// Asserted through `focus_widget()` containment rather than `has_focus()` on the
    /// list — a `GtkListView` delegates focus to the row it lands on, so the narrower
    /// probe answers false on a perfectly working focus move (GTK4Rs/AP-119's shape).
    #[gtktest::test]
    fn revealing_the_annotations_pane_focuses_its_list() {
        let app = test_app("com.extollit.scribobulate.integrationtest.annfocusreveal");
        let md = "Intro.\n\nA {==claim==}{>>a note<<} here.\n\nMore prose.\n";
        let win = new_window(&app, "IT-annfocusreveal", md, None);
        win.present();
        let st = state(&win).expect("tab state");
        let scroller = st.chrome().annotations_scroller.clone();

        change_action_state(&win, "annotations", &true.to_variant());
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the annotations pane to hand the keyboard to its list",
            || {
                sidebar::list_view_of(&scroller)
                    .zip(GtkWindowExt::focus(&win))
                    .is_some_and(|(list, focused)| focused.is_ancestor(&list) || focused == list)
            },
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

#[cfg(test)]
mod combo_label_tests {
    use super::{theme_button_label, THEME_BUTTON_MAX_CHARS};

    #[test]
    fn a_short_theme_name_is_left_exactly_as_it_is() {
        assert_eq!(theme_button_label("Sepia", None), "Sepia");
    }

    #[test]
    fn a_symbol_is_kept_and_counted_toward_the_cap() {
        // The symbol and its separator are part of what the button has to draw, so a
        // cap that measured the name alone would let the button grow past it.
        let out = theme_button_label("Sepia", Some("\u{1f4d6}"));
        assert!(
            out.starts_with('\u{1f4d6}'),
            "the symbol leads the label: {out}"
        );
        assert!(out.chars().count() <= THEME_BUTTON_MAX_CHARS);
    }

    #[test]
    fn an_unbounded_theme_name_cannot_set_the_windows_minimum_width() {
        // The point of the cap. A theme's display name comes out of a user-supplied
        // file, so without this the toolbar's widest item — and so the window's floor
        // — is whatever someone typed into a theme header.
        let absurd = "A Theme Whose Author Was Paid By The Character And Meant It";
        let out = theme_button_label(absurd, None);
        assert!(
            out.chars().count() <= THEME_BUTTON_MAX_CHARS,
            "label {out:?} is {} chars, past the {THEME_BUTTON_MAX_CHARS} cap",
            out.chars().count()
        );
        assert!(out.ends_with('\u{2026}'), "a cut label says so: {out:?}");
    }

    #[test]
    fn a_name_exactly_at_the_cap_is_not_cut() {
        let exact: String = "x".repeat(THEME_BUTTON_MAX_CHARS);
        assert_eq!(theme_button_label(&exact, None), exact);
    }

    #[test]
    fn a_multibyte_name_is_cut_on_a_character_never_inside_one() {
        // Sliced by char, never by byte — a cut that split a UTF-8 sequence would
        // panic rather than merely look wrong.
        let out = theme_button_label(&"é".repeat(80), None);
        assert!(out.chars().count() <= THEME_BUTTON_MAX_CHARS);
    }
}
