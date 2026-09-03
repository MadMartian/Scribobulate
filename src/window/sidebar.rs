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

use gtk::glib;
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
            // A short min-content-height, because a ScrolledWindow otherwise reports a
            // tiny minimum and would let the other section crush it to ~0. Since the
            // sidebar became a vertical GtkPaned (TDD 20.21) this value does a second
            // job: with `shrink_*_child(false)`, each section's minimum IS the divider's
            // travel limit, so it is what stops a drag from hiding a section the pane's
            // action still reports as shown. Both still vexpand, for the single-section
            // case where the Paned allocates one child the whole height.
            .min_content_height(80)
            .vexpand(true)
            .build();

        let title_label = gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .hexpand(true)
            .build();
        // `.heading` is a base-GTK4 section-title class (no libadwaita — GTK4Rs/AP-25).
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

        // Every sidebar pane holds a virtualized GtkListView, so every one of them is
        // exposed to the pre-4.10.1 list-scroll defect. Installed HERE rather than at
        // each pane's own construction so a pane added later inherits the fix instead
        // of having to remember it (see `wheelcoalesce`).
        super::wheelcoalesce::install(&scroller);

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

/// Move the keyboard focus into `scroller`'s list, deferred one idle turn.
///
/// Called when a sidebar toggle **reveals** its pane, which is the only keyboard route
/// to these lists there is: they sit several Tab stops behind the tab bar and the pane's
/// own close ×, and nothing else focuses them, so a reader who showed the annotations
/// list still could not get to it. Revealing a pane in order to use it and then not
/// being given it is the gap this closes; hiding is unaffected, and so is every
/// non-toggle path (a tab switch, a session restore) that merely *reconciles*
/// visibility — those must never move the focus, which is why this is called from the
/// toggle's own handler rather than from `reconcile_sidebar_visibility`.
///
/// Deferred because the pane became visible in this same turn and its list has no
/// allocation yet, and because a toggle activated from the menu bar is racing that
/// menu's pop-down focus-restore (ScrAP-107). A no-op on the empty-state placeholder,
/// which is a plain label and takes no focus.
pub(crate) fn focus_list_deferred(scroller: &gtk::ScrolledWindow) {
    let scroller = scroller.clone();
    glib::idle_add_local_once(move || {
        if let Some(list_view) = list_view_of(&scroller) {
            let _ = list_view.grab_focus();
        }
    });
}

/// Scroll `scroller`'s `GtkListView` so the currently selected row is in view with
/// the minimum amount of scrolling. No-op when the scroller holds the empty-state
/// placeholder, when nothing is selected, or when the row is already visible.
///
/// Prefers the GTK 4.6 `list.scroll-to-item` widget action on `GtkListBase`
/// (exact row heights, min scroll). That action **silently no-ops** when the item
/// has no size record yet (`get_allocation_along` fails — common on a fresh
/// ListView right after a child swap). Falls back to a uniform-height estimate on
/// the scroller's vadjustment. When the scroller is not yet laid out
/// (`page_size == 0`), retries on subsequent idles (bounded) so a tab-switch
/// reveal that races the first allocate still lands. `ListView::scroll_to` is
/// 4.12+ only (ScrAP-157 / GTK4Rs/AP-114). Does not change the selection, so
/// outline spy guards (GTK4Rs/AP-112) stay quiet.
pub(crate) fn reveal_selected_row(scroller: &gtk::ScrolledWindow) {
    reveal_selected_row_attempt(scroller, 0);
}

const REVEAL_LAYOUT_RETRIES: u8 = 8;

fn reveal_selected_row_attempt(scroller: &gtk::ScrolledWindow, attempt: u8) {
    let Some(list_view) = list_view_of(scroller) else {
        return;
    };
    // Drop any wheel travel the reader has accumulated but not yet been given: this
    // scroll supersedes it, and letting it land afterwards would be a second
    // adjustment write in the same frame — the one condition `wheelcoalesce` exists to
    // prevent.
    super::wheelcoalesce::cancel_pending(scroller);
    let Some(sel) = list_view
        .model()
        .and_then(|m| m.downcast::<gtk::SingleSelection>().ok())
    else {
        return;
    };
    let pos = sel.selected();
    if pos == gtk::INVALID_LIST_POSITION {
        return;
    }
    // Parameter type "u" — matches GtkListBase|list.scroll-to-item (guint position).
    let _ = list_view.activate_action("list.scroll-to-item", Some(&pos.to_variant()));

    let n = sel.n_items();
    if n == 0 {
        return;
    }
    let vadj = scroller.vadjustment();
    let page = vadj.page_size();
    let upper = vadj.upper();
    if page <= 0.0 || upper <= 0.0 {
        // Not laid out yet — common right after `set_child` on a tab switch.
        // Retry on later idles until the scroller has a real page_size (or we
        // exhaust the budget; an unmapped pane then simply stays put).
        if attempt < REVEAL_LAYOUT_RETRIES {
            let scroller = scroller.clone();
            glib::idle_add_local_once(move || {
                reveal_selected_row_attempt(&scroller, attempt + 1);
            });
        }
        return;
    }
    let row_h = upper / f64::from(n);
    let row_top = f64::from(pos) * row_h;
    let row_bottom = row_top + row_h;
    let value = vadj.value();
    if row_top >= value && row_bottom <= value + page {
        return; // already fully visible
    }
    // Minimum scroll: pin the row to the top edge if above, bottom edge if below.
    let target = if row_top < value {
        row_top
    } else {
        (row_bottom - page).max(0.0)
    };
    let max = (upper - page).max(0.0);
    crate::saferizer::scrollpos::jump(&vadj, target.clamp(0.0, max));
}
