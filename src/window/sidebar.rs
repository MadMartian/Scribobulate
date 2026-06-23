//! `SidebarPane` — the shared *chrome* of a sidebar section.
//!
//! The outline and the annotations viewer are two independent lists that live stacked
//! in one sidebar. Their **chrome** is identical — a bold `.heading` caption, optional
//! header action buttons, an in-pane close **×**, a separator, and a scroller whose
//! inner child is swapped on every document change — so it is abstracted here and both
//! panes use it unchanged. Their **internals differ in kind** (the outline is a
//! `GtkTreeListModel` tree keyed by positional index; the annotations list is a flat
//! `ListStore` keyed by span identity) and are deliberately **not** shared: a generic
//! covering both would need a tree/flat switch and an index-or-identity key union, which
//! is more complex than the two concrete builders and hides the one difference that most
//! matters. So this seam is the chrome only.
//!
//! Ownership note: the pane's `root` box's `:visible` is driven by its toggle action
//! (`win.outline` / `win.annotations`) through
//! [`reconcile_sidebar_visibility`](super::annotations_nav::reconcile_sidebar_visibility),
//! never set here — this module only *builds* the widgets.

use gtk::prelude::*;

/// One built sidebar section's persistent handles.
pub(crate) struct SidebarPane {
    /// The section container: `[ header ][ separator ][ scroller ]`, vertical. Its
    /// `:visible` is the pane's show/hide state, toggled by the pane's `win.*` action.
    pub(crate) root: gtk::Box,
    /// The scroller whose inner child (the list `GtkListView`, or the "No …"
    /// placeholder `GtkLabel`) is rebuilt on every document change. Persists across
    /// rebuilds so no signal is orphaned.
    pub(crate) scroller: gtk::ScrolledWindow,
}

impl SidebarPane {
    /// Build a sidebar section titled `title`, whose in-pane **×** activates
    /// `close_action` (e.g. `"win.outline"`), with `header_buttons` inserted between
    /// the title and the ×, and its scroller reserving `min_content_width` px.
    ///
    /// `close_tooltip` is the ×'s tooltip (e.g. `"Hide outline (F9)"`). The × is a
    /// *secondary* control that shares the same `GAction` as the menu/toolbar toggle,
    /// so the pane has one source-of-truth for its visibility (Action CAM).
    pub(crate) fn new(
        title: &str,
        close_action: &str,
        close_tooltip: &str,
        header_buttons: &[gtk::Button],
        min_content_width: i32,
    ) -> Self {
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_width(min_content_width)
            // A short min-content-height so that when BOTH panes are shown neither
            // ScrolledWindow crushes the other to ~0 (a ScrolledWindow reports a tiny
            // minimum — the collapsible-sections analogue of the Paned hazard the plan
            // warns about); both vexpand, so they still share the sidebar height.
            .min_content_height(80)
            .vexpand(true)
            .build();

        let title_label = gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .hexpand(true)
            .build();
        // `.heading` is a base-GTK4 section-title class (no libadwaita — ScrAP-25).
        title_label.add_css_class("heading");

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        header.set_margin_start(12);
        header.set_margin_end(6);
        header.set_margin_top(6);
        header.set_margin_bottom(6);
        header.append(&title_label);
        for btn in header_buttons {
            header.append(btn);
        }

        // Optional in-pane close affordance. The HIG-canonical control is the external
        // toolbar / menu toggle (kept as primary); this × is acceptable secondary
        // polish. It activates the SAME stateful action, so the controls share one
        // source of truth (the action's boolean state).
        let close = gtk::Button::from_icon_name(crate::icons::Icon::WindowClose.name());
        close.add_css_class("flat");
        close.set_valign(gtk::Align::Center);
        crate::a11y::name(&close, close_tooltip);
        close.set_action_name(Some(close_action));
        header.append(&close);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_vexpand(true);
        root.append(&header);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&scroller);

        Self { root, scroller }
    }
}

/// A sidebar scroller's current inner child as a `GtkListView`, or `None` when it is
/// the empty-state placeholder (`GtkLabel`). The single "child might be the placeholder,
/// not a ListView" guard every caller that walks scroller → ListView → model would
/// otherwise hand-roll, owned here once (used by both `outline_nav` and `annotations_nav`).
pub(crate) fn list_view_of(scroller: &gtk::ScrolledWindow) -> Option<gtk::ListView> {
    scroller.child().and_downcast::<gtk::ListView>()
}
