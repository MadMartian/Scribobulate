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

#[cfg(all(test, feature = "gtk-integration-tests"))]
thread_local! {
    /// What the deferred hand-back and the reveal did, in order, for the integration tests'
    /// failure messages: which path the focus took, and what the scroller read at each
    /// reveal. The sidebar-filter focus test has failed on CI with a list that read
    /// as never laid out, and could not show whether the reveal ran before layout and was
    /// dropped or the test simply looked too early. Compiled only into test builds.
    static REVEAL_TRACE: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
fn trace(event: impl FnOnce() -> String) {
    REVEAL_TRACE.with(|t| t.borrow_mut().push(event()));
}

#[cfg(not(all(test, feature = "gtk-integration-tests")))]
#[inline]
fn trace(_event: impl FnOnce() -> String) {}

/// Take and clear [`REVEAL_TRACE`].
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn take_reveal_trace() -> Vec<String> {
    REVEAL_TRACE.with(|t| std::mem::take(&mut *t.borrow_mut()))
}

/// One built sidebar section's persistent handles.
pub(crate) struct SidebarPane {
    /// The section container: `[ header ][ separator ][ filter bar ][ scroller ]`,
    /// vertical — the scroller stays a DIRECT child, because the visibility
    /// reconciliation reaches this box as the scroller's parent. Its
    /// `:visible` is the pane's show/hide state, toggled by the pane's `win.*` action.
    pub(crate) root: gtk::Box,
    /// The scroller whose inner child (the list `GtkListView`, or the "No …"
    /// placeholder `GtkLabel`) is rebuilt on every document change. Persists across
    /// rebuilds so no signal is orphaned.
    pub(crate) scroller: gtk::ScrolledWindow,
    /// The section's heading, kept so a section can report a count in it
    /// (the annotations viewer's "Annotations (N)", TDD 20.22).
    pub(crate) title: gtk::Label,
}

impl SidebarPane {
    /// Build a sidebar section titled `title`, whose in-pane **×** activates
    /// `close_action` (e.g. `"win.outline"`), with `header_buttons` inserted between
    /// the title and the ×, and its scroller reserving `min_content_width` px.
    ///
    /// `close_tooltip` is the ×'s tooltip (e.g. `"Hide outline (F9)"`). The × is a
    /// *secondary* control that shares the same `GAction` as the menu/toolbar toggle,
    /// so the pane has one source-of-truth for its visibility (Action CAM).
    ///
    /// `filter` is the pane's sidebar filter (`window::sidebarfilter`): its search
    /// toggle leads the header buttons and its bar sits between the header and the list,
    /// capturing typing from the list so a printable key there starts a filter.
    pub(crate) fn new(
        title: &str,
        close_action: &str,
        close_tooltip: &str,
        header_buttons: &[gtk::Button],
        filter: &super::FilterBar,
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
        header.append(&filter.button);
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
        root.append(&filter.bar);
        root.append(&scroller);
        // Typing a printable key while the list has the focus opens the filter with it
        // (GtkSearchBar's key capture — BUBBLE phase, so the list's own keys win first).
        // The scroller rather than the list because the list is rebuilt on every change
        // and the scroller persists.
        filter.bar.set_key_capture_widget(Some(&scroller));

        Self {
            root,
            scroller,
            title: title_label,
        }
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
/// menu's pop-down focus-restore (GTK4Rs/AP-116). A no-op on the empty-state placeholder,
/// which is a plain label and takes no focus.
pub(crate) fn focus_list_deferred(scroller: &gtk::ScrolledWindow) {
    let scroller = scroller.clone();
    glib::idle_add_local_once(move || {
        if let Some(list_view) = list_view_of(&scroller) {
            let _ = list_view.grab_focus();
        }
    });
}

/// Select position `pos` of `scroller`'s list — which navigates, exactly as an arrow key
/// or a click in the list does — then put the keyboard focus on that row, scrolled into
/// view. `false` when there is no list or no such position.
///
/// Down in a filter box (TDD 20.25). Selecting rather than only focusing is deliberate:
/// there is no focus-without-select route that holds from GTK 4.6 to 4.22. On 4.8.2+ a
/// later `grab_focus` re-applies the list's previous focus child and writes its cursor
/// back, and on 4.18+ a Down from an unselected cursor selects it where it stands. So
/// the order is: select through the model first, then focus the row widget carrying
/// `:selected` once it exists — the focus lands last and on the row the selection names,
/// leaving nothing for a later grab to snap back to.
pub(crate) fn select_and_focus_row(scroller: &gtk::ScrolledWindow, pos: u32) -> bool {
    let Some(sel) = list_view_of(scroller)
        .and_then(|lv| lv.model())
        .and_then(|m| m.downcast::<gtk::SingleSelection>().ok())
    else {
        return false;
    };
    if pos >= sel.n_items() {
        return false;
    }
    sel.set_selected(pos);
    focus_selected_row_deferred(scroller);
    true
}

/// Hand the keyboard back to `scroller`'s list at the row it has SELECTED — the section
/// the outline highlights, or the annotation last gone to — scrolled into view, deferred.
/// With nothing selected, the list itself takes the focus and nothing is selected.
///
/// The return leg of closing a sidebar filter (TDD 20.25). Plain `grab_focus` on a
/// `GtkListView` lands on its first materialised row, which after a rebuild is the top of
/// the list — so the reader's next arrow press navigated the document to its first
/// heading. Focusing a row widget does not select it (`gtk_list_item_widget_grab_focus`
/// only moves the focus; selection is the list's `list.select-item`), so handing the
/// focus back navigates nowhere.
///
/// The selected row is materialised only once it is on screen, so this reveals it first
/// and looks for it on the following frames (bounded), by the `:selected` state GTK sets
/// on the row widget. Should it never appear, the list takes the focus as before.
pub(crate) fn focus_selected_row_deferred(scroller: &gtk::ScrolledWindow) {
    let scroller = scroller.clone();
    glib::idle_add_local_once(move || {
        let Some(list_view) = list_view_of(&scroller) else {
            return;
        };
        let has_selection = list_view
            .model()
            .and_then(|m| m.downcast::<gtk::SingleSelection>().ok())
            .is_some_and(|sel| sel.selected() != gtk::INVALID_LIST_POSITION);
        if !has_selection {
            trace(|| "hand-back: no selection, list focused".into());
            let _ = list_view.grab_focus();
            return;
        }
        reveal_selected_row(&scroller);
        let is_selected = |row: &gtk::Widget| row.state_flags().contains(gtk::StateFlags::SELECTED);
        if focus_row_widget_where(&list_view, is_selected) {
            trace(|| "hand-back: row focused at once".into());
            return;
        }
        trace(|| "hand-back: row not materialised, retrying per frame".into());
        // Each frame that still lacks the row asks again: a reveal of a far row lands on
        // an ESTIMATE of its offset, and the estimate improves as rows are laid out.
        // Weak: the tick is owned by the list, a descendant of this scroller, so a
        // strong capture would keep the scroller alive from inside its own subtree
        // (GTK4Rs/AP-63) until the deadline ran out.
        let reveal_from = scroller.downgrade();
        focus_row_on_frames(
            &list_view,
            move |lv| {
                focus_row_widget_where(lv, is_selected) || {
                    if let Some(scroller) = reveal_from.upgrade().filter(|s| s.is_realized()) {
                        reveal_selected_row(&scroller);
                    }
                    false
                }
            },
            |lv| {
                trace(|| "hand-back: per-frame retry gave up, list focused".into());
                let _ = lv.grab_focus();
            },
        );
    });
}

/// Focus the first materialised ROW widget of `list_view` (the list's own child, not the
/// factory's content) satisfying `is_target`.
fn focus_row_widget_where(
    list_view: &gtk::ListView,
    is_target: impl Fn(&gtk::Widget) -> bool,
) -> bool {
    let mut row = list_view.first_child();
    while let Some(r) = row {
        if is_target(&r) {
            return r.grab_focus();
        }
        row = r.next_sibling();
    }
    false
}

/// How long a deferred row focus keeps looking for its row before giving up. Wall clock,
/// because the row appears only after a LAYOUT pass, which runs on the frame clock
/// (GTK4Rs/AP-122, GTK4Rs/AP-261): counting idles is the wrong clock and can run out
/// before a single frame has been drawn.
const ROW_FOCUS_DEADLINE: std::time::Duration = std::time::Duration::from_millis(1000);

/// Run `try_focus` once per frame of `list_view` until it reports success or the
/// deadline passes, then `give_up` if it never did.
fn focus_row_on_frames(
    list_view: &gtk::ListView,
    try_focus: impl Fn(&gtk::ListView) -> bool + 'static,
    give_up: impl Fn(&gtk::ListView) + 'static,
) {
    let deadline =
        glib::monotonic_time() + i64::try_from(ROW_FOCUS_DEADLINE.as_micros()).unwrap_or(i64::MAX);
    list_view.add_tick_callback(move |widget, _| {
        let Some(list_view) = widget.downcast_ref::<gtk::ListView>() else {
            return glib::ControlFlow::Break;
        };
        if try_focus(list_view) {
            return glib::ControlFlow::Break;
        }
        if glib::monotonic_time() >= deadline {
            give_up(list_view);
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
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
/// (`page_size == 0`), waits for its layout ([`reveal_after_layout`]) so a reveal that
/// races the first allocate still lands. `ListView::scroll_to` is
/// 4.12+ only (GTK4Rs/AP-143 / GTK4Rs/AP-114). Does not change the selection, so
/// outline spy guards (GTK4Rs/AP-112) stay quiet.
pub(crate) fn reveal_selected_row(scroller: &gtk::ScrolledWindow) {
    reveal_selected_row_now(scroller, "reveal");
}

thread_local! {
    /// The scrollers (and the adjustment each waits on) with a reveal parked until their
    /// list is first laid out. One entry per scroller, so the per-frame retries of a
    /// hand-back park one wait, not one per frame. Keyed by scroller (GTK4Rs/AP-332): two
    /// panes waiting at once must not displace each other.
    static AWAITING_LAYOUT: std::cell::RefCell<
        Vec<(glib::WeakRef<gtk::ScrolledWindow>, glib::WeakRef<gtk::Adjustment>)>,
    > = const { std::cell::RefCell::new(Vec::new()) };
}

/// Run the reveal again once `scroller`'s list has a size.
///
/// Issue D: this used to retry on a fixed number of idles, and on a loaded machine all of
/// them ran before GTK laid the list out, so the reveal gave up and the list stayed at the
/// top. Layout runs on the frame clock, not on idles (GTK4Rs/AP-261), and no count of
/// either is a bound on when it comes. So wait for the event itself: the vadjustment's
/// `changed`, which `gtk_adjustment_configure` emits when the list's allocation gives it a
/// page and an upper. The reveal then runs on the next idle, not inside that emission,
/// because the emission is in the middle of the list's `size_allocate` and the reveal
/// writes the adjustment and activates a list action. A pane that is hidden waits until it
/// is shown and laid out, and reveals then.
fn reveal_after_layout(scroller: &gtk::ScrolledWindow, vadj: &gtk::Adjustment) {
    let parked = AWAITING_LAYOUT.with(|w| {
        let mut w = w.borrow_mut();
        w.retain(|(s, a)| s.upgrade().is_some() && a.upgrade().is_some());
        if w.iter().any(|(s, a)| {
            s.upgrade().as_ref() == Some(scroller) && a.upgrade().as_ref() == Some(vadj)
        }) {
            return true;
        }
        w.push((scroller.downgrade(), vadj.downgrade()));
        false
    });
    if parked {
        trace(|| "reveal: not laid out, a wait is already parked".into());
        return;
    }
    trace(|| "reveal: not laid out, waiting for layout".into());
    // Weak: the adjustment belongs to the scroller, so a strong capture would keep the
    // scroller alive from its own adjustment's handler (GTK4Rs/AP-63).
    let weak = scroller.downgrade();
    let handler: std::rc::Rc<std::cell::Cell<Option<glib::SignalHandlerId>>> =
        std::rc::Rc::default();
    let own = std::rc::Rc::clone(&handler);
    let id = vadj.connect_changed(move |adj| {
        if adj.page_size() <= 0.0 || adj.upper() <= 0.0 {
            return;
        }
        if let Some(id) = own.take() {
            adj.disconnect(id);
        }
        AWAITING_LAYOUT.with(|w| {
            w.borrow_mut()
                .retain(|(_, a)| a.upgrade().is_some_and(|a| &a != adj));
        });
        let Some(scroller) = weak.upgrade() else {
            return;
        };
        glib::idle_add_local_once(move || {
            reveal_selected_row_now(&scroller, "reveal after layout");
        });
    });
    handler.set(Some(id));
}

fn reveal_selected_row_now(scroller: &gtk::ScrolledWindow, which: &'static str) {
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
        // Not laid out yet — common right after `set_child` on a tab switch, or a pane
        // shown this turn.
        trace(|| {
            format!(
                "{which}: not laid out (page {page}, upper {upper}, mapped {})",
                scroller.is_mapped()
            )
        });
        reveal_after_layout(scroller, &vadj);
        return;
    }
    let row_h = upper / f64::from(n);
    let row_top = f64::from(pos) * row_h;
    let row_bottom = row_top + row_h;
    let value = vadj.value();
    if row_top >= value && row_bottom <= value + page {
        trace(|| {
            format!("{which}: row already visible (value {value}, upper {upper}, page {page})")
        });
        return; // already fully visible
    }
    // Minimum scroll: pin the row to the top edge if above, bottom edge if below.
    let target = if row_top < value {
        row_top
    } else {
        (row_bottom - page).max(0.0)
    };
    let max = (upper - page).max(0.0);
    trace(|| {
        format!(
            "{which}: jumped {value} -> {} (upper {upper}, page {page})",
            target.clamp(0.0, max)
        )
    });
    crate::saferizer::scrollpos::jump(&vadj, target.clamp(0.0, max));
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::testpump::{until, until_or_for, Clock};
    use std::time::Duration;

    /// A sidebar-like scroller holding a 200-row list with row 150 selected.
    fn scroller_with_far_selection() -> gtk::ScrolledWindow {
        let rows: Vec<String> = (0..200).map(|n| format!("Row {n}")).collect();
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        let sel = gtk::SingleSelection::new(Some(gtk::StringList::new(&rows)));
        sel.set_autoselect(false);
        sel.set_selected(150);
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            if let Some(item) = item.downcast_ref::<gtk::ListItem>() {
                item.set_child(Some(&gtk::Label::new(None)));
            }
        });
        factory.connect_bind(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let text = item
                .item()
                .and_downcast::<gtk::StringObject>()
                .map(|s| s.string());
            if let (Some(label), Some(text)) = (item.child().and_downcast::<gtk::Label>(), text) {
                label.set_text(&text);
            }
        });
        let list = gtk::ListView::new(Some(sel), Some(factory));
        gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&list)
            .build()
    }

    /// Issue D. A reveal asked for before the list is laid out still lands, however long the
    /// layout takes: the reveal is asked for while the scroller is in no window at all, the
    /// loop runs well past the eight idles the old fixed retry budget allowed, and only then
    /// does the list get a window and a size. Mutation guard: restore a bounded retry in
    /// place of `reveal_after_layout` and the list stays at the top.
    #[gtktest::test]
    fn a_reveal_requested_before_layout_lands_when_the_list_is_laid_out() {
        let scroller = scroller_with_far_selection();
        reveal_selected_row(&scroller);
        reveal_selected_row(&scroller);
        // Let every default-priority idle chain finish — the old retries were one — by
        // waiting for a LOW-priority idle, which runs only once none of them is pending. A
        // fixed span is not enough: on macOS the drain did not reach the retries at all.
        let drained = std::rc::Rc::new(std::cell::Cell::new(false));
        let mark = std::rc::Rc::clone(&drained);
        glib::idle_add_local_full(glib::Priority::LOW, move || {
            mark.set(true);
            glib::ControlFlow::Break
        });
        until(
            Clock::Idle,
            "every default-priority idle to have run",
            || drained.get(),
        );
        let vadj = scroller.vadjustment();
        assert_eq!(
            (vadj.page_size(), vadj.upper()),
            (0.0, 0.0),
            "the list is still unlaid out after the drain, as the test needs"
        );

        let window = gtk::Window::builder()
            .default_width(300)
            .default_height(300)
            .child(&scroller)
            .build();
        window.present();
        until(Clock::Frame, "the list to be laid out", || {
            vadj.page_size() > 0.0 && vadj.upper() > 0.0
        });
        let revealed = until_or_for(Clock::Frame, Duration::from_secs(2), || vadj.value() > 0.0);
        let (value, upper, page) = (vadj.value(), vadj.upper(), vadj.page_size());
        window.destroy();
        assert!(
            revealed,
            "the far selected row is scrolled into view once the list is laid out \
             (value {value}, upper {upper}, page {page}; trace: {:?})",
            take_reveal_trace()
        );
    }
}
