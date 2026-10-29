//! The sidebar filters' window wiring: each pane's search toggle, filter bar and box,
//! the `win.filter-outline` / `win.filter-annotations` commands, and the traffic between
//! the window-shared boxes and the active document's own filter (TDD 12.26–12.29,
//! 20.24–20.25). What a filter *means* is display-free, in `crate::sidebarfilter` and
//! `outline::filter`.
//!
//! # One truth per document, one box per window
//!
//! A filter belongs to the document (`TabState::outline_filter` / `annotations_filter`),
//! like the outline's folding; the bar that edits it is window chrome shared by every
//! tab. The list rebuild reads the document's filter, never the box. The box is re-synced
//! from the document whenever the tab it last reflected is not the active one
//! ([`sync_to_tab`], called from each pane's rebuild), and the box writes back only while
//! it reflects the active tab — the same "whoever is active owns the widget" split the
//! folding memory makes, applied to a widget that holds state of its own.
//!
//! # Why GtkSearchBar, and what it is left to do
//!
//! `GtkSearchBar` (4.6) already does three things the rubric asks for: revealing its box
//! grabs the focus into it, hiding it **clears the box** (so a closed filter hides no
//! rows, TDD 12.27), and a key-capture widget turns typing in the list into a filter
//! (20.25). Its Escape handling is the one piece overridden: GTK closes the bar on any
//! Escape, where the rubric clears a non-empty filter first and closes only an empty one
//! — see [`on_stop_search`] for how the override gets there before GTK's handler.
//!
//! # The toggle button is the bar's, not the command's
//!
//! The header's search toggle is bound to the bar's `search-mode-enabled` (GTK's own
//! search-button pattern), not to `win.filter-*`, because the two differ in one case:
//! the command pressed while the bar is open but the focus is elsewhere FOCUSES the box
//! (the reader asked for the filter — TDD 20.25), while a click on a pressed toggle
//! closes it. One action cannot be both; the button carries the command's shortcut in
//! its tooltip, and every other surface (menu, accelerator, Keyboard Shortcuts window)
//! is the action. The command is never disabled, so there is no sensitivity for the two
//! to disagree about.

use super::*;
use crate::sidebarfilter::PaneFilter;
use std::rc::Weak;

/// Which sidebar pane a filter belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SidebarPaneKind {
    Outline,
    Annotations,
}

impl SidebarPaneKind {
    /// The pane's show/hide toggle (`win.outline` / `win.annotations`).
    fn toggle_action(self) -> &'static str {
        match self {
            Self::Outline => "outline",
            Self::Annotations => "annotations",
        }
    }

    /// The filter command's action name, without its `win.` prefix.
    pub(crate) fn filter_action(self) -> &'static str {
        match self {
            Self::Outline => "filter-outline",
            Self::Annotations => "filter-annotations",
        }
    }

    /// The search toggle's and the box's accessible name (TDD 20.25).
    fn control_name(self) -> &'static str {
        match self {
            Self::Outline => "Filter outline",
            Self::Annotations => "Filter annotations",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Self::Outline => "Filter headings",
            Self::Annotations => "Filter comments",
        }
    }

    /// This pane's filter in a document's state.
    pub(crate) fn filter(self, st: &TabState) -> &RefCell<PaneFilter> {
        match self {
            Self::Outline => &st.outline_filter,
            Self::Annotations => &st.annotations_filter,
        }
    }

    fn bar(self, chrome: &winstate::WindowChrome) -> &FilterBar {
        match self {
            Self::Outline => &chrome.outline_filter,
            Self::Annotations => &chrome.annotations_filter,
        }
    }

    fn scroller(self, chrome: &winstate::WindowChrome) -> gtk::ScrolledWindow {
        match self {
            Self::Outline => chrome.outline_scroller.clone(),
            Self::Annotations => chrome.annotations_scroller.clone(),
        }
    }

    /// Rebuild the pane's list from the active document's cached headings or
    /// annotations and its current filter — no re-parse.
    fn rebuild(self, window: &ApplicationWindow) {
        match self {
            Self::Outline => {
                super::outline_nav::rebuild_outline_list(window);
                // Re-highlight the current section in the NEW list at once: a filter
                // change rebuilds the list with no expand or collapse, so the rebuild's
                // own spy hook never fires, and the list would otherwise come back with
                // only the last-ACTIVATED heading selected — or none — instead of the
                // section the reader is in (TDD 12.29), which is also the row closing the
                // filter hands the keyboard back to. Only here, not in every rebuild: a
                // mode switch deliberately restores the last-activated heading (12.13).
                // The spy's guards keep this from navigating.
                super::outline_nav::apply_scroll_spy(window);
            }
            Self::Annotations => super::annotations_nav::rebuild_annotations_list(window),
        }
    }

    /// Enter in the box: go to the first match (TDD 12.28).
    fn activate_first_match(self, window: &ApplicationWindow) {
        match self {
            Self::Outline => super::outline_nav::activate_first_outline_match(window),
            Self::Annotations => super::annotations_nav::activate_first_annotation(window),
        }
    }

    /// Down in the box: select the first match — which navigates — and focus its row
    /// (TDD 20.25).
    fn focus_first_match(self, window: &ApplicationWindow) -> bool {
        match self {
            Self::Outline => super::outline_nav::focus_first_outline_match(window),
            Self::Annotations => super::annotations_nav::focus_first_annotation(window),
        }
    }
}

/// One pane's filter chrome: the header's search toggle, the bar under the header, and
/// the box in it. Cheap to clone (GObject handles plus shared cells), held both by the
/// build-time chrome and by `WindowChrome`.
#[derive(Clone)]
pub(crate) struct FilterBar {
    pub(crate) button: gtk::ToggleButton,
    pub(crate) bar: gtk::SearchBar,
    pub(crate) entry: gtk::SearchEntry,
    /// The match count beside the box — "3 of 42" while the filter is active, empty
    /// otherwise (TDD 12.26, 20.24). In the bar rather than the pane heading because the
    /// heading shares its row with up to four buttons and a narrow sidebar cut the count
    /// off. A STATUS region (TDD 16.5), so a count that changes as the reader types is
    /// announced without the focus leaving the box.
    pub(crate) count: gtk::Label,
    /// The document the bar currently reflects (see the module header).
    owner: Rc<RefCell<Weak<TabState>>>,
    /// Set while [`sync_to_tab`] drives the widgets, so their signals are not mistaken
    /// for the reader editing the filter.
    syncing: Rc<Cell<bool>>,
    /// The filter command's deferred focus step, while it is still queued. A re-sync to
    /// a document cancels it (see [`activate_filter`]).
    pending_focus: Rc<Cell<Option<glib::SourceId>>>,
}

impl FilterBar {
    /// Build a pane's filter chrome. Every handler resolves its window from the widget
    /// at emission time (GTK4Rs/AP-52), so nothing here needs the window yet.
    pub(crate) fn new(pane: SidebarPaneKind) -> Self {
        // Through the application's field constructor, which names the field and wires
        // the macOS eager clipboard write and word navigation every field gets.
        let entry = crate::widgets::textfield::named_search_entry(pane.control_name());
        entry.set_placeholder_text(Some(pane.placeholder()));
        // BEFORE the bar connects to the entry: GtkSearchBar hangs its own close-on-
        // Escape on this same signal, and only a handler connected ahead of it can stop
        // the emission before it runs (see `on_stop_search`).
        entry.connect_stop_search(on_stop_search);
        entry.connect_search_changed(move |entry| on_search_changed(pane, entry));
        entry.connect_activate(move |entry| {
            if let Some(window) = host_window(entry) {
                pane.activate_first_match(&window);
            }
        });
        install_down_key(pane, &entry);

        let count = gtk::Label::new(None);
        count.add_css_class("dim-label");
        count.set_accessible_role(gtk::AccessibleRole::Status);
        count.set_margin_start(6);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.append(&entry);
        row.append(&count);

        let bar = gtk::SearchBar::new();
        bar.set_child(Some(&row));
        bar.connect_entry(&entry);
        bar.connect_search_mode_enabled_notify(move |bar| on_mode_changed(pane, bar));
        name_builtin_close(&bar);

        let button = gtk::ToggleButton::new();
        button.set_icon_name(crate::icons::Icon::SystemSearch.name());
        button.add_css_class("flat");
        button.set_valign(gtk::Align::Center);
        let accel =
            crate::app::inline_accel(&format!("win.{}", pane.filter_action())).unwrap_or_default();
        crate::a11y::name_with_accel(&button, pane.control_name(), accel);
        // Pressed exactly while the bar is open, in both directions, so an open filter is
        // always visible in the header (the plan's approach 2).
        button
            .bind_property("active", &bar, "search-mode-enabled")
            .bidirectional()
            .sync_create()
            .build();

        Self {
            button,
            bar,
            entry,
            count,
            owner: Rc::new(RefCell::new(Weak::new())),
            syncing: Rc::new(Cell::new(false)),
            pending_focus: Rc::new(Cell::new(None)),
        }
    }

    /// Show `matched` of `total` beside the box, or nothing when no filter is active.
    pub(crate) fn set_count(&self, counts: Option<(usize, usize)>) {
        let text = counts
            .map(|(matched, total)| crate::sidebarfilter::match_count(matched, total))
            .unwrap_or_default();
        if self.count.text() != text {
            self.count.set_text(&text);
        }
    }

    /// Whether the bar currently reflects `st`.
    fn reflects(&self, st: &Rc<TabState>) -> bool {
        self.owner
            .borrow()
            .upgrade()
            .is_some_and(|owner| Rc::ptr_eq(&owner, st))
    }
}

/// Name GtkSearchBar's own close button. The bar builds one into itself whether or not it
/// is shown (`show-close-button` only hides it; this bar keeps it hidden, the header's
/// toggle being the way out), and an unnamed button is exactly what the accessible-name
/// guard exists to catch — hidden today is not a promise that no theme or later change
/// shows it. The bar exposes no handle to it, so it is found as the one `GtkButton` in
/// the bar's own subtree outside the entry.
fn name_builtin_close(bar: &gtk::SearchBar) {
    fn walk(w: &gtk::Widget) {
        if w.is::<gtk::Editable>() {
            return;
        }
        if w.is::<gtk::Button>() {
            crate::a11y::name(w, "Close filter");
            return;
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            walk(&c);
            child = c.next_sibling();
        }
    }
    walk(bar.upcast_ref());
}

/// Whether the window's focus is `widget` or inside it — the entry delegates its focus
/// to an internal `GtkText` (GTK4Rs/AP-119), so `has_focus()` on it is never true.
fn focus_within(window: &ApplicationWindow, widget: &impl IsA<gtk::Widget>) -> bool {
    let widget = widget.as_ref();
    let mut w = GtkWindowExt::focus(window);
    while let Some(cur) = w {
        if &cur == widget {
            return true;
        }
        w = cur.parent();
    }
    false
}

/// Whether the keyboard focus is in `pane`'s filter box.
pub(crate) fn filter_box_has_focus(window: &ApplicationWindow, pane: SidebarPaneKind) -> bool {
    winstate::chrome(window).is_some_and(|chrome| focus_within(window, &pane.bar(&chrome).entry))
}

/// Make `pane`'s bar show `st`'s filter, unless it already reflects `st`.
///
/// Called at the top of each pane's list rebuild, which every route to a different
/// active document passes through — a tab switch, a cross-window move, a session
/// restore — so no route can leave the box showing another document's filter.
///
/// **The focus is put back** afterwards: revealing a GtkSearchBar grabs the focus into
/// its box, which is right when the reader opens it and wrong when a tab switch merely
/// shows that document's open filter (TDD 20.19: reconciling never moves the focus).
///
/// Text still being typed into the box for the outgoing document — inside the entry's
/// ~150 ms search delay — is written to that document first, so switching away does not
/// lose the last keystrokes.
pub(crate) fn sync_to_tab(window: &ApplicationWindow, pane: SidebarPaneKind, st: &Rc<TabState>) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    let fb = pane.bar(&chrome);
    if fb.reflects(st) {
        return;
    }
    cancel_pending_focus(fb);
    let outgoing = fb.owner.borrow().upgrade();
    if let Some(prev) = outgoing {
        if fb.bar.is_search_mode() {
            pane.filter(&prev).borrow_mut().text = fb.entry.text().to_string();
        }
    }
    let want = pane.filter(st).borrow().clone();
    let focus = GtkWindowExt::focus(window);
    fb.syncing.set(true);
    fb.bar.set_search_mode(want.open);
    if want.open && fb.entry.text() != want.text {
        fb.entry.set_text(&want.text);
    }
    fb.syncing.set(false);
    if GtkWindowExt::focus(window) != focus {
        GtkWindowExt::set_focus(window, focus.as_ref());
    }
    *fb.owner.borrow_mut() = Rc::downgrade(st);
}

/// The box's text settled (GtkSearchEntry's search delay): record it on the document and
/// rebuild the list.
fn on_search_changed(pane: SidebarPaneKind, entry: &gtk::SearchEntry) {
    let Some(window) = host_window(entry) else {
        return;
    };
    let (Some(chrome), Some(st)) = (winstate::chrome(&window), state(&window)) else {
        return;
    };
    let fb = pane.bar(&chrome);
    if fb.syncing.get() || !fb.reflects(&st) {
        return;
    }
    let text = entry.text().to_string();
    {
        let mut filter = pane.filter(&st).borrow_mut();
        if filter.text == text {
            return;
        }
        filter.text = text;
    }
    // A new result set starts at its top. Before the rebuild, so the fresh list is not
    // laid out against the previous one's scroll position.
    crate::saferizer::scrollpos::jump(&pane.scroller(&chrome).vadjustment(), 0.0);
    pane.rebuild(&window);
}

/// The bar opened or closed — by the toggle, the command, Escape, or a printable key
/// typed into the list.
fn on_mode_changed(pane: SidebarPaneKind, bar: &gtk::SearchBar) {
    let Some(window) = host_window(bar) else {
        return;
    };
    let (Some(chrome), Some(st)) = (winstate::chrome(&window), state(&window)) else {
        return;
    };
    let fb = pane.bar(&chrome);
    if fb.syncing.get() || !fb.reflects(&st) {
        return;
    }
    let open = bar.is_search_mode();
    {
        let mut filter = pane.filter(&st).borrow_mut();
        if filter.open == open {
            return;
        }
        filter.open = open;
        if !open {
            // GtkSearchBar has already emptied the box; say so here too, so a filter
            // that is out of sight holds nothing (TDD 12.27).
            filter.text.clear();
        }
    }
    pane.rebuild(&window);
    // Closed from inside the box (Escape on an empty box, or the command pressed there):
    // the box is leaving the screen, so hand the keyboard to the list rather than
    // stranding it (TDD 20.25) — at the row the list highlights, not its first row, or
    // the reader's next arrow press would navigate to the top of the document. A close from the toggle button leaves the focus on the
    // button, which is still there.
    if !open && focus_within(&window, &fb.entry) {
        super::sidebar::focus_selected_row_deferred(&pane.scroller(&chrome));
    }
}

/// Escape in the box. A non-empty box is cleared and stays open; an empty one is left to
/// GtkSearchBar, which closes the bar (and `on_mode_changed` then hands the focus to the
/// list).
///
/// GtkSearchBar closes on EVERY `stop-search`, through a handler it connects in
/// `gtk_search_bar_connect_entry`. Handlers run in connection order, so this one —
/// connected first, in [`FilterBar::new`] — stops the emission before GTK's runs. That is
/// the one supported way to veto a signal's later handlers without replacing the class
/// handler. (Escape reaches `stop-search` at all only because the box is a
/// GtkSearchEntry, whose class binding owns the key — GTK4Rs/AP-53.)
fn on_stop_search(entry: &gtk::SearchEntry) {
    if entry.text().is_empty() {
        return;
    }
    entry.set_text("");
    entry.stop_signal_emission_by_name("stop-search");
}

/// Down in the box goes to the first match: selects it, which navigates as an arrow key in
/// the list does, and puts the focus on its row (TDD 20.25).
///
/// On the entry's DELEGATE — the internal `GtkText` that actually holds the focus
/// (GTK4Rs/AP-301) — and at CAPTURE, ahead of the text's own key handling (GTK4Rs/AP-302).
/// A Down the list cannot take (nothing matches) is let through.
fn install_down_key(pane: SidebarPaneKind, entry: &gtk::SearchEntry) {
    let target: gtk::Widget = entry
        .delegate()
        .map(|d| d.upcast())
        .unwrap_or_else(|| entry.clone().upcast());
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(move |ctl, key, _, mods| {
        let plain = !mods.intersects(
            gdk::ModifierType::SHIFT_MASK
                | gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::SUPER_MASK
                | gdk::ModifierType::META_MASK,
        );
        if !plain || !matches!(key, gdk::Key::Down | gdk::Key::KP_Down) {
            return glib::Propagation::Proceed;
        }
        let Some(window) = ctl.widget().and_then(|w| host_window(&w)) else {
            return glib::Propagation::Proceed;
        };
        // Report the key handled only if there was a match to go to (GTK4Rs/AP-336): with
        // none, the key is let through untouched.
        if pane.focus_first_match(&window) {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    target.add_controller(keys);
}

/// `win.filter-outline` / `win.filter-annotations` (Shift+F9 / Shift+F8).
///
/// Pressed from inside the open box it closes the filter, as the search toggle would.
/// Otherwise it shows the pane if hidden, opens the filter, and puts the focus in the
/// box — also when the filter was already open and the focus elsewhere, which is the case
/// the toggle button cannot express (see the module header).
fn activate_filter(window: &ApplicationWindow, pane: SidebarPaneKind) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    let fb = pane.bar(&chrome);
    let shown = bool_action_state(window, pane.toggle_action(), false);
    if shown && fb.bar.is_search_mode() && focus_within(window, &fb.entry) {
        fb.bar.set_search_mode(false);
        return;
    }
    if !shown {
        // The toggle's own handler reconciles visibility and queues its list focus.
        change_action_state(window, pane.toggle_action(), &true.to_variant());
    }
    fb.bar.set_search_mode(true);
    // Deferred: the toggle just queued a focus move to the pane's list, and a command
    // activated from the menu bar races the menu's pop-down focus restore
    // (GTK4Rs/AP-116). Queued after both, at the same priority, this lands last.
    //
    // "Last" has no upper bound, though: a document still being laid out holds this
    // priority back (GTK4Rs/T-5), so the reader can switch documents — and even come
    // back — before it runs, and it would then pull the focus into the box on a tab
    // switch (TDD 20.19). So every re-sync of the box to a document cancels it
    // (`sync_to_tab`).
    cancel_pending_focus(fb);
    let entry = fb.entry.clone();
    let slot = fb.pending_focus.clone();
    let id = glib::idle_add_local_once(move || {
        // Fired: the id is spent, and removing it later would panic (GTK4Rs/AP-128).
        slot.take();
        entry.grab_focus();
    });
    fb.pending_focus.set(Some(id));
}

/// Drop the filter command's focus step if it has not run yet.
fn cancel_pending_focus(fb: &FilterBar) {
    if let Some(id) = fb.pending_focus.take() {
        id.remove();
    }
}

/// Register the two filter commands on `window`. Never disabled: a filter can always be
/// opened, even over an empty list.
pub(crate) fn register_filter_actions(window: &ApplicationWindow) {
    for pane in [SidebarPaneKind::Outline, SidebarPaneKind::Annotations] {
        let action = SimpleAction::new(pane.filter_action(), None);
        action.connect_activate(glib::clone!(
            #[weak]
            window,
            move |_, _| activate_filter(&window, pane)
        ));
        window.add_action(&action);
    }
}

/// The sidebar filters against a real window (TDD 12.26–12.29, 20.24–20.25).
///
/// What a query MEANS is unit-tested beside the matcher (`crate::sidebarfilter`,
/// `outline::filter`, `annotations::filter_entries`); these cases are about the parts
/// that only exist once widgets do — the box's signals reaching the document's state,
/// the rebuilt list, the folding memory the filtered tree must not write, the tab that
/// owns the box, the focus, and the names assistive technology reads.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::super::outline_nav::{outline_tree_model, scroll_spy_set_selection};
    use super::*;
    use crate::outline_view::HeadingObject;
    use crate::testpump::{drain_for, until, Clock};
    use gtk::glib::translate::IntoGlib;
    use std::time::Duration;

    /// Seven headings — `Guide > {Install > {Base setup, Extras}, Usage > Base commands}`,
    /// then `Appendix` — and three annotations, one of them a point comment.
    const DOC: &str = "# Guide\n\n## Install\n\n### Base setup\n\nbody\n\n### Extras\n\nbody\n\n\
    ## Usage\n\n### Base commands\n\nbody\n\n# Appendix\n\n\
    Text {==first claim==}{>>alpha note<<} and {>>beta<<} and {==second thing==}{>>gamma note<<}.\n";

    const HEADINGS: usize = 7;

    fn open(suffix: &str, doc: &str) -> ApplicationWindow {
        let app = crate::window::testkit::test_app_suffixed(suffix);
        let window = crate::window::new_window(&app, "IT", doc, None);
        window.set_default_size(1000, 700);
        window.present();
        until(
            Clock::Idle,
            "the window to map with its outline built",
            || window.is_mapped() && outline_tree_model(&window).is_some(),
        );
        window
    }

    fn chrome(window: &ApplicationWindow) -> Rc<winstate::WindowChrome> {
        winstate::chrome(window).expect("the window has chrome")
    }

    /// Open `pane`'s filter through its command and type `text` into the box, then wait for
    /// the box's search delay to hand the text to the document's state.
    fn type_filter(window: &ApplicationWindow, pane: SidebarPaneKind, text: &str) {
        let ch = chrome(window);
        let fb = pane.bar(&ch);
        if !fb.bar.is_search_mode() {
            window
                .lookup_action(pane.filter_action())
                .expect("the filter command is registered")
                .activate(None);
        }
        until(Clock::Idle, "the filter bar to open", || {
            fb.bar.is_search_mode()
        });
        fb.entry.set_text(text);
        until(
            Clock::Frame,
            "the box's search delay to record the text on the document",
            || state(window).is_some_and(|st| pane.filter(&st).borrow().text == text),
        );
    }

    /// The outline's rows as `(doc_index, is_context, markup)`, in list order.
    fn outline_rows(window: &ApplicationWindow) -> Vec<(usize, bool, Option<String>)> {
        let Some(model) = outline_tree_model(window) else {
            return Vec::new();
        };
        (0..model.n_items())
            .filter_map(|i| {
                model
                    .item(i)
                    .and_downcast::<gtk::TreeListRow>()
                    .and_then(|r| r.item())
                    .and_downcast::<HeadingObject>()
            })
            .map(|h| (h.doc_index(), h.is_context(), h.markup()))
            .collect()
    }

    fn doc_indices(window: &ApplicationWindow) -> Vec<usize> {
        outline_rows(window)
            .into_iter()
            .map(|(d, _, _)| d)
            .collect()
    }

    /// The empty-state placeholder's text, when the pane's scroller holds one. A
    /// `GtkScrolledWindow` wraps a non-scrollable child like a label in a `GtkViewport`.
    fn placeholder(scroller: &gtk::ScrolledWindow) -> Option<String> {
        let child = scroller.child()?;
        let child = match child.downcast::<gtk::Viewport>() {
            Ok(viewport) => viewport.child()?,
            Err(child) => child,
        };
        child
            .downcast::<gtk::Label>()
            .ok()
            .map(|l| l.text().to_string())
    }

    fn action_enabled(window: &ApplicationWindow, name: &str) -> bool {
        window.lookup_action(name).is_some_and(|a| a.is_enabled())
    }

    /// The outline row for heading `doc_index`, if it is materialised in the model.
    fn outline_row(window: &ApplicationWindow, doc_index: usize) -> Option<gtk::TreeListRow> {
        let model = outline_tree_model(window)?;
        (0..model.n_items()).find_map(|i| {
            let row = model.item(i).and_downcast::<gtk::TreeListRow>()?;
            let heading = row.item().and_downcast::<HeadingObject>()?;
            (heading.doc_index() == doc_index).then_some(row)
        })
    }

    /// **12.26** — a filtered outline shows every match under its section path, dims the
    /// path, highlights the match, counts matches of the total, and says so when nothing
    /// matches.
    #[gtktest::test]
    fn the_outline_narrows_to_matches_under_their_section_path() {
        let window = open("sidebarfilter.narrow", DOC);
        assert_eq!(doc_indices(&window), (0..HEADINGS).collect::<Vec<_>>());

        type_filter(&window, SidebarPaneKind::Outline, "base");
        let rows = outline_rows(&window);
        assert_eq!(
            rows.iter().map(|(d, c, _)| (*d, *c)).collect::<Vec<_>>(),
            vec![(0, true), (1, true), (2, false), (4, true), (5, false)],
            "matches under their dimmed ancestors; Extras and Appendix gone"
        );
        let base_setup = rows[2].2.as_deref().expect("a match carries markup");
        assert!(
            base_setup.contains(">Base</span> setup"),
            "the matched word is highlighted: {base_setup}"
        );
        assert!(rows[0].2.is_none(), "context rows are never highlighted");
        let ch = chrome(&window);
        assert_eq!(ch.outline_filter.count.text(), "2 of 7");
        assert!(!action_enabled(&window, "outline-expand-all"));
        assert!(!action_enabled(&window, "outline-collapse-all"));

        type_filter(&window, SidebarPaneKind::Outline, "zebra");
        assert_eq!(
            placeholder(&ch.outline_scroller).as_deref(),
            Some(crate::sidebarfilter::NO_MATCHING_HEADINGS)
        );
        assert_eq!(ch.outline_filter.count.text(), "0 of 7");
        window.destroy();
    }

    /// **12.27** — closing the filter restores the reader's folding exactly, including
    /// after a chevron was turned INSIDE the filtered tree, and restores Expand/Collapse all.
    ///
    /// Mutation-checked: dropping `records_folding`'s guard in `rebuild_outline_list` lets
    /// the filtered tree's collapse of "Usage" leak into the folding memory, and the last
    /// assertion on row 5 fails.
    #[gtktest::test]
    fn closing_the_outline_filter_restores_the_folding_from_before() {
        let window = open("sidebarfilter.folding", DOC);
        outline_row(&window, 1)
            .expect("Install has a row")
            .set_expanded(false);
        until(
            Clock::Idle,
            "the collapse of Install to be recorded",
            || {
                state(&window).is_some_and(|st| {
                    st.outline_collapsed
                        .borrow()
                        .collapsed_indexes(&st.outline_paths.borrow())
                        .contains(&1)
                })
            },
        );
        assert_eq!(doc_indices(&window), vec![0, 1, 4, 5, 6]);

        // The filter sees inside the collapsed section: the view's own model does not.
        type_filter(&window, SidebarPaneKind::Outline, "base");
        assert_eq!(doc_indices(&window), vec![0, 1, 2, 4, 5]);

        // A chevron turned in the FILTERED tree must not outlive the filter.
        outline_row(&window, 4)
            .expect("Usage is shown as context")
            .set_expanded(false);
        drain_for(Clock::Idle, Duration::from_millis(100));

        let ch = chrome(&window);
        ch.outline_filter.button.set_active(false);
        until(
            Clock::Idle,
            "closing the filter to reach the document",
            || state(&window).is_some_and(|st| !st.outline_filter.borrow().open),
        );
        let st = state(&window).unwrap();
        assert_eq!(*st.outline_filter.borrow(), PaneFilter::default());
        assert_eq!(ch.outline_filter.entry.text(), "", "closing always clears");
        assert_eq!(ch.outline_filter.count.text(), "");
        assert!(action_enabled(&window, "outline-expand-all"));
        assert!(action_enabled(&window, "outline-collapse-all"));
        assert_eq!(
            doc_indices(&window),
            vec![0, 1, 4, 5, 6],
            "Install still folded as the reader left it, Usage still open"
        );
        window.destroy();
    }

    /// **12.27 / 20.25** — Escape clears a non-empty box and keeps it open; on an empty box
    /// it closes the filter and returns the keyboard to the list.
    ///
    /// Mutation-checked: without `on_stop_search`'s early emission stop, GtkSearchBar closes
    /// the bar on the first Escape and the "still open" assertion fails.
    #[gtktest::test]
    fn escape_clears_a_filter_then_closes_it() {
        let window = open("sidebarfilter.escape", DOC);
        type_filter(&window, SidebarPaneKind::Outline, "base");
        let ch = chrome(&window);
        let fb = &ch.outline_filter;
        until(Clock::Idle, "the box to take the focus", || {
            focus_within(&window, &fb.entry)
        });

        fb.entry.emit_by_name::<()>("stop-search", &[]);
        until(Clock::Idle, "Escape to clear the filter", || {
            state(&window).is_some_and(|st| st.outline_filter.borrow().text.is_empty())
        });
        assert!(
            fb.bar.is_search_mode(),
            "a non-empty filter is cleared, not closed"
        );
        assert_eq!(doc_indices(&window), (0..HEADINGS).collect::<Vec<_>>());

        fb.entry.emit_by_name::<()>("stop-search", &[]);
        assert!(!fb.bar.is_search_mode(), "Escape on an empty box closes it");
        let list = super::super::sidebar::list_view_of(&ch.outline_scroller).expect("a list");
        until(Clock::Idle, "the focus to return to the list", || {
            focus_within(&window, &list)
        });
        window.destroy();
    }

    /// **12.28** — Enter in the box navigates to the first match, as activating its row
    /// would, and the filter stays applied.
    #[gtktest::test]
    fn enter_goes_to_the_first_match_and_keeps_the_filter() {
        let window = open("sidebarfilter.enter", DOC);
        window.change_action_state("view-mode", &"edit".to_variant());
        drain_for(Clock::Frame, Duration::from_millis(200));
        type_filter(&window, SidebarPaneKind::Outline, "base");
        let ch = chrome(&window);
        ch.outline_filter.entry.emit_by_name::<()>("activate", &[]);
        let st = state(&window).unwrap();
        let base_setup_line = DOC
            .lines()
            .position(|l| l == "### Base setup")
            .expect("fixture line") as i32;
        until(Clock::Idle, "the caret to reach the first match", || {
            st.editor_buf
                .iter_at_mark(&st.editor_buf.get_insert())
                .line()
                == base_setup_line
        });
        assert_eq!(
            st.outline_selected
                .borrow()
                .as_ref()
                .and_then(|p| p.last())
                .map(|step| step.title.as_str()),
            Some("Base setup"),
            "recorded as the reader's activated heading"
        );
        assert!(
            st.outline_filter.borrow().query().is_some(),
            "the filter stays"
        );
        assert_eq!(doc_indices(&window), vec![0, 1, 2, 4, 5]);
        window.destroy();
    }

    /// TDD 20.19 — the filter command puts the focus in the box from a deferred step, and
    /// a tab switch that happens before that step runs cancels it. A document still being
    /// laid out holds the step back (GTK4Rs/T-5), so the reader can switch away and back
    /// first; the late step then pulled the focus into the box on the switch back. In the
    /// suite it surfaced as `the_outline_filter_follows_the_document` failing now and then.
    ///
    /// The oracle is an absence, so the test first waits for the step to be SETTLED —
    /// cancelled or run — rather than for a fixed span it might simply not have reached
    /// yet. The sibling arm below runs the same command with no switch and sees the step
    /// land, so the step does run in this fixture when nothing cancels it.
    ///
    /// Mutation-checked: without the cancel in `sync_to_tab`, the assertion fails every run.
    #[gtktest::test]
    fn a_filter_focus_still_pending_at_a_tab_switch_is_dropped() {
        pending_filter_focus_after("sidebarfilter.pendingfocus", true);
    }

    /// The positive control for the test above: the same command with no tab switch puts
    /// the focus in the box once its step settles. Mutation-checked: dropping the
    /// `grab_focus` from the deferred step in `activate_filter` fails here.
    #[gtktest::test]
    fn a_filter_focus_with_no_tab_switch_lands_in_the_box() {
        pending_filter_focus_after("sidebarfilter.pendingfocus.control", false);
    }

    /// Whether the filter bar still holds a queued focus step.
    fn focus_step_queued(fb: &FilterBar) -> bool {
        let id = fb.pending_focus.take();
        let queued = id.is_some();
        fb.pending_focus.set(id);
        queued
    }

    /// Run the outline filter command from the editor, with or without a switch away and
    /// back before its focus step runs, and assert where the focus is once the step has
    /// settled: in the box with no switch, not in it with one.
    fn pending_filter_focus_after(app_suffix: &str, switch_tabs: bool) {
        let window = open(app_suffix, DOC);
        type_filter(&window, SidebarPaneKind::Outline, "base");
        let ch = chrome(&window);
        let first = state(&window).unwrap().id;
        crate::window::create_tab_in_window(&window, "# Other\n", None, false, false)
            .expect("a second tab");
        drain_for(Clock::Frame, Duration::from_millis(200));
        let second = state(&window).unwrap().id;
        let select = |id: crate::winstate::TabId| {
            crate::window::actions::change_action_state(
                &window,
                "select-tab",
                &id.to_string().to_variant(),
            )
        };
        select(first);
        drain_for(Clock::Frame, Duration::from_millis(200));
        state(&window).unwrap().editor.grab_focus();
        assert!(
            !focus_within(&window, &ch.outline_filter.entry),
            "precondition"
        );

        // The command, then away and back, all before the main loop runs its focus step.
        window
            .lookup_action(SidebarPaneKind::Outline.filter_action())
            .expect("the filter command is registered")
            .activate(None);
        if switch_tabs {
            select(second);
            select(first);
        }
        until(Clock::Idle, "the filter's focus step to be settled", || {
            !focus_step_queued(&ch.outline_filter)
        });
        assert!(
            ch.outline_filter.bar.is_search_mode(),
            "the filter is still open"
        );
        if switch_tabs {
            assert!(
                !focus_within(&window, &ch.outline_filter.entry),
                "a focus step queued before the switches landed after them"
            );
        } else {
            assert!(
                focus_within(&window, &ch.outline_filter.entry),
                "with no switch, the filter command's focus step puts the focus in the box"
            );
        }
        window.destroy();
    }

    /// **12.29** — the filter belongs to the document: another tab has its own, switching
    /// back restores it (without taking the focus), an edit re-applies it, and the
    /// scroll-spy's highlight falls on the nearest SHOWN heading.
    ///
    /// Mutation-checked: making `sync_to_tab` skip applying the document's filter to the box
    /// leaves the new tab showing the first tab's open filter, and the first assertion after
    /// the tab is created fails.
    #[gtktest::test]
    fn the_outline_filter_follows_the_document() {
        let window = open("sidebarfilter.follows", DOC);
        type_filter(&window, SidebarPaneKind::Outline, "base");
        let ch = chrome(&window);
        let first = state(&window).unwrap().id;

        crate::window::create_tab_in_window(&window, "# Other\n\n## Plain\n", None, false, false)
            .expect("a second tab");
        drain_for(Clock::Frame, Duration::from_millis(200));
        assert!(
            !ch.outline_filter.bar.is_search_mode(),
            "the new document has no filter"
        );
        assert_eq!(ch.outline_filter.entry.text(), "");
        assert_eq!(ch.outline_filter.count.text(), "");
        assert_eq!(doc_indices(&window), vec![0, 1]);

        // Park the focus in the document before switching back: showing that document's open
        // filter must not pull the focus into its box (TDD 20.19's reconcile rule).
        state(&window).unwrap().editor.grab_focus();
        crate::window::actions::change_action_state(
            &window,
            "select-tab",
            &first.to_string().to_variant(),
        );
        drain_for(Clock::Frame, Duration::from_millis(200));
        assert!(ch.outline_filter.bar.is_search_mode());
        assert_eq!(ch.outline_filter.entry.text(), "base");
        assert_eq!(ch.outline_filter.count.text(), "2 of 7");
        assert!(
            !focus_within(&window, &ch.outline_filter.entry),
            "a tab switch re-shows the filter without moving the focus into it"
        );

        // The spy's section filtered out → the nearest shown ancestor; none shown → nothing.
        scroll_spy_set_selection(&window, Some(3)); // Extras, under the shown Install
        let selected = |w: &ApplicationWindow| {
            super::super::sidebar::list_view_of(&chrome(w).outline_scroller)
                .and_then(|lv| lv.model())
                .and_then(|m| m.downcast::<gtk::SingleSelection>().ok())
                .and_then(|s| s.selected_item())
                .and_downcast::<gtk::TreeListRow>()
                .and_then(|r| r.item())
                .and_downcast::<HeadingObject>()
                .map(|h| h.doc_index())
        };
        assert_eq!(selected(&window), Some(1));
        scroll_spy_set_selection(&window, Some(6)); // Appendix: nothing of it is shown
        assert_eq!(selected(&window), None);

        // An edit re-applies the filter to the headings as they now stand.
        let st = state(&window).unwrap();
        st.set_source(&format!("{DOC}\n## Base extra\n"));
        crate::window::refresh_outline(&window);
        assert_eq!(ch.outline_filter.count.text(), "3 of 8");

        window.destroy();
    }

    /// **20.24** — the annotations viewer narrows to comments whose comment or quoted text
    /// holds every word, highlights them, counts M of N, says when nothing matches, and
    /// re-applies the filter when the annotations change.
    #[gtktest::test]
    fn the_annotations_list_narrows_to_matching_comments() {
        let window = open("sidebarfilter.annotations", DOC);
        type_filter(&window, SidebarPaneKind::Annotations, "note claim");
        let ch = chrome(&window);
        assert_eq!(ch.annotations_filter.count.text(), "1 of 3");
        assert_eq!(
            ch.annotations_title.label(),
            "Annotations (3)",
            "the heading keeps its unfiltered count; the filter bar carries M of N"
        );
        let list = super::super::sidebar::list_view_of(&ch.annotations_scroller).expect("a list");
        let model = list.model().expect("a model");
        assert_eq!(model.n_items(), 1);

        type_filter(&window, SidebarPaneKind::Annotations, "zzz");
        assert_eq!(
            placeholder(&ch.annotations_scroller).as_deref(),
            Some(crate::sidebarfilter::NO_MATCHING_COMMENTS)
        );
        assert_eq!(ch.annotations_filter.count.text(), "0 of 3");

        type_filter(&window, SidebarPaneKind::Annotations, "note");
        let st = state(&window).unwrap();
        st.set_source(&format!("{DOC}\nMore {{>>a new note<<}}.\n"));
        crate::window::refresh_annotations(&window);
        assert_eq!(ch.annotations_filter.count.text(), "3 of 4");
        assert_eq!(ch.annotations_title.label(), "Annotations (4)");

        ch.annotations_filter.bar.set_search_mode(false);
        assert_eq!(ch.annotations_title.label(), "Annotations (4)");
        assert_eq!(ch.annotations_filter.count.text(), "");
        window.destroy();
    }

    /// **20.25** — the filter command shows a hidden pane, opens its filter and focuses the
    /// box; pressed again from the box it closes it; Down moves into the list; the controls
    /// are named and announced.
    #[gtktest::test]
    fn a_filter_is_reachable_and_dismissable_from_the_keyboard() {
        let window = open("sidebarfilter.keyboard", DOC);
        let ch = chrome(&window);
        let fb = &ch.annotations_filter;
        let section = ch
            .annotations_scroller
            .parent()
            .expect("the scroller sits in its section");
        assert!(!section.is_visible(), "the annotations pane starts hidden");

        let command = window
            .lookup_action("filter-annotations")
            .expect("the command is registered");
        command.activate(None);
        until(Clock::Idle, "the box to take the focus", || {
            focus_within(&window, &fb.entry)
        });
        assert!(section.is_visible(), "the command shows its pane");
        assert!(fb.bar.is_search_mode());

        command.activate(None);
        assert!(
            !fb.bar.is_search_mode(),
            "pressed from the box, it closes the filter"
        );
        let list = super::super::sidebar::list_view_of(&ch.annotations_scroller).expect("a list");
        until(Clock::Idle, "the focus to return to the list", || {
            focus_within(&window, &list)
        });

        // Down, delivered to the box's key handler: onto the first shown row (deferred,
        // once the row exists).
        command.activate(None);
        until(Clock::Idle, "the box to take the focus again", || {
            focus_within(&window, &fb.entry)
        });
        let delegate: gtk::Widget = fb.entry.delegate().expect("a text delegate").upcast();
        let keys = delegate
            .observe_controllers()
            .into_iter()
            .filter_map(|c| c.ok())
            .filter_map(|c| c.downcast::<gtk::EventControllerKey>().ok())
            .find(|k| k.propagation_phase() == gtk::PropagationPhase::Capture)
            .expect("the Down handler sits at capture on the delegate");
        let handled = keys.emit_by_name::<bool>(
            "key-pressed",
            &[
                &gdk::Key::Down.into_glib(),
                &0u32,
                &gdk::ModifierType::empty(),
            ],
        );
        assert!(handled, "Down is taken when there is a row to go to");
        let list = super::super::sidebar::list_view_of(&ch.annotations_scroller).expect("a list");
        until(Clock::Frame, "Down to put the focus in the list", || {
            focus_within(&window, &list)
        });

        // Typing in the list opens the filter (GtkSearchBar's own capture), named controls,
        // and a heading that announces its count.
        assert_eq!(
            fb.bar.key_capture_widget(),
            Some(ch.annotations_scroller.clone().upcast())
        );
        for bar in [&ch.outline_filter, &ch.annotations_filter] {
            assert!(crate::a11y::has_name(&bar.button));
            assert!(crate::a11y::has_name(&bar.entry));
            let tip = bar.button.tooltip_text().expect("a tooltip").to_string();
            assert!(
                tip.contains("F9") || tip.contains("F8"),
                "the tooltip names the shortcut: {tip}"
            );
        }
        for bar in [&ch.outline_filter, &ch.annotations_filter] {
            assert_eq!(bar.count.accessible_role(), gtk::AccessibleRole::Status);
        }
        window.destroy();
    }

    /// The list row that holds the keyboard focus, as the heading it shows, and whether
    /// that row is the list's SELECTED one.
    fn focused_outline_row(window: &ApplicationWindow) -> Option<(usize, bool)> {
        let list = super::super::sidebar::list_view_of(&chrome(window).outline_scroller)?;
        let mut w = GtkWindowExt::focus(window);
        while let Some(cur) = w {
            if cur.parent().as_ref() == Some(list.upcast_ref()) {
                let heading = cur
                    .first_child()
                    .and_downcast::<gtk::TreeExpander>()
                    .and_then(|e| e.list_row())
                    .and_then(|r| r.item())
                    .and_downcast::<HeadingObject>()?;
                return Some((
                    heading.doc_index(),
                    cur.state_flags().contains(gtk::StateFlags::SELECTED),
                ));
            }
            w = cur.parent();
        }
        None
    }

    /// **20.25** — closing the filter from its box hands the keyboard back to the row the
    /// outline HIGHLIGHTS, scrolled into view, without navigating; not to the list's first
    /// row, from which the next arrow press navigated the document to its top.
    ///
    /// Fails on the pre-fix code (a bare `grab_focus` on the list): the focus lands on
    /// row 0, which is neither the highlighted row nor selected, and the list sits at the
    /// top.
    #[gtktest::test]
    fn closing_the_filter_returns_the_focus_to_the_highlighted_row() {
        let mut doc = String::from("# Top\n\n");
        for n in 0..60 {
            doc.push_str(&format!("## Section {n}\n\nbody\n\n"));
        }
        let window = open("sidebarfilter.refocus", &doc);
        window.set_default_size(1000, 500);
        window.change_action_state("view-mode", &"edit".to_variant());
        drain_for(Clock::Frame, Duration::from_millis(200));
        let st = state(&window).unwrap();
        // The caret in "Section 50": edit mode's scroll-spy highlights heading 51.
        let target = 51;
        let line = doc
            .lines()
            .position(|l| l == "## Section 50")
            .expect("fixture line") as i32;
        st.editor_buf
            .place_cursor(&st.editor_buf.iter_at_line(line).expect("the line exists"));
        until(
            Clock::Idle,
            "the spy to highlight the caret's section",
            || {
                focused_outline_row(&window).is_none()
                    && super::super::sidebar::list_view_of(&chrome(&window).outline_scroller)
                        .and_then(|lv| lv.model())
                        .and_then(|m| m.downcast::<gtk::SingleSelection>().ok())
                        .and_then(|s| s.selected_item())
                        .and_downcast::<gtk::TreeListRow>()
                        .and_then(|r| r.item())
                        .and_downcast::<HeadingObject>()
                        .is_some_and(|h| h.doc_index() == target)
            },
        );
        let caret = || st.editor_buf.property::<i32>("cursor-position");
        let caret_before = caret();

        type_filter(&window, SidebarPaneKind::Outline, "section");
        let ch = chrome(&window);
        until(Clock::Idle, "the box to take the focus", || {
            focus_within(&window, &ch.outline_filter.entry)
        });
        // Close from inside the box, as Shift+F9 does there.
        window
            .lookup_action(SidebarPaneKind::Outline.filter_action())
            .unwrap()
            .activate(None);
        until(Clock::Frame, "the focus to return to a list row", || {
            focused_outline_row(&window).is_some()
        });
        assert_eq!(
            focused_outline_row(&window),
            Some((target, true)),
            "the focus returns to the highlighted row, which stays the selected one"
        );
        // The scroll's figures go in the message: this assertion has failed once on CI
        // and never locally, and they say whether the reveal ran, ran against a stale
        // list size, or was undone afterwards.
        let vadj = ch.outline_scroller.vadjustment();
        assert!(
            vadj.value() > 0.0,
            "the highlighted row is scrolled into view, not the top of the list \
             (value {}, upper {}, page {})",
            vadj.value(),
            vadj.upper(),
            vadj.page_size()
        );
        assert_eq!(
            caret(),
            caret_before,
            "handing the focus back navigates nowhere"
        );
        assert!(
            st.outline_selected.borrow().is_none(),
            "…and records no activation"
        );
        window.destroy();
    }

    /// Emit Down on `pane`'s filter box, through the handler it installs, and report
    /// whether the key was taken.
    fn press_down_in_box(window: &ApplicationWindow, pane: SidebarPaneKind) -> bool {
        let ch = chrome(window);
        let delegate: gtk::Widget = pane
            .bar(&ch)
            .entry
            .delegate()
            .expect("a text delegate")
            .upcast();
        let keys = delegate
            .observe_controllers()
            .into_iter()
            .filter_map(|c| c.ok())
            .filter_map(|c| c.downcast::<gtk::EventControllerKey>().ok())
            .find(|k| k.propagation_phase() == gtk::PropagationPhase::Capture)
            .expect("the Down handler sits at capture on the delegate");
        keys.emit_by_name::<bool>(
            "key-pressed",
            &[
                &gdk::Key::Down.into_glib(),
                &0u32,
                &gdk::ModifierType::empty(),
            ],
        )
    }

    /// **20.25** — Down goes to the FIRST match — selects it, navigates there, and puts
    /// the focus on its row — even after the reader has gone to a match far down the list
    /// and scrolled there, so the highlighted row is neither the first match nor on
    /// screen with it.
    #[gtktest::test]
    fn down_goes_to_the_first_match_not_the_highlighted_row() {
        let mut doc = String::from("# Top\n\n");
        for n in 0..60 {
            doc.push_str(&format!("## Section {n}\n\nbody\n\n"));
        }
        let window = open("sidebarfilter.downfirst", &doc);
        window.set_default_size(1000, 500);
        window.change_action_state("view-mode", &"edit".to_variant());
        drain_for(Clock::Frame, Duration::from_millis(200));
        type_filter(&window, SidebarPaneKind::Outline, "section");
        let ch = chrome(&window);
        let st = state(&window).unwrap();
        let caret_line = || {
            st.editor_buf
                .iter_at_mark(&st.editor_buf.get_insert())
                .line()
        };
        let line_of = |heading: &str| doc.lines().position(|l| l == heading).unwrap() as i32;
        // The reader goes to a heading far down the filtered list, then scrolls there.
        let far = outline_row(&window, 56).expect("Section 55 has a row");
        let selection = super::super::sidebar::list_view_of(&ch.outline_scroller)
            .and_then(|lv| lv.model())
            .and_then(|m| m.downcast::<gtk::SingleSelection>().ok())
            .expect("a selection");
        selection.set_selected(far.position());
        until(Clock::Idle, "the prior navigation to land", || {
            caret_line() == line_of("## Section 55")
        });
        let vadj = ch.outline_scroller.vadjustment();
        until(Clock::Frame, "the outline list to be laid out", || {
            vadj.upper() > vadj.page_size() && vadj.page_size() > 0.0
        });
        crate::saferizer::scrollpos::jump(&vadj, vadj.upper());
        drain_for(Clock::Frame, Duration::from_millis(100));
        ch.outline_filter.entry.grab_focus();

        assert!(press_down_in_box(&window, SidebarPaneKind::Outline));
        until(Clock::Frame, "the focus to reach the first match", || {
            focused_outline_row(&window).is_some()
        });
        drain_for(Clock::Frame, Duration::from_millis(200));
        assert_eq!(
            focused_outline_row(&window),
            Some((1, true)),
            "the focus is on the first match (Section 0), which is now the selected row — \
             not the dimmed \"Top\" and not the previously highlighted Section 55"
        );
        assert_eq!(
            caret_line(),
            line_of("## Section 0"),
            "selecting the first match navigated there"
        );
        window.destroy();
    }

    /// **20.25**, annotations — Down goes to the first shown annotation (selects it and
    /// navigates, focus on its row) after the reader has gone to a later one and scrolled
    /// away from the top.
    #[gtktest::test]
    fn down_goes_to_the_first_annotation_not_the_last_visited() {
        let mut doc = String::from("# Notes\n\n");
        // Enough rows that GtkListView cannot keep them all materialised.
        for n in 0..300 {
            doc.push_str(&format!("Para {n} {{>>note {n}<<}}.\n\n"));
        }
        let window = open("sidebarfilter.downfirstann", &doc);
        window.set_default_size(1000, 500);
        window.change_action_state("annotations", &true.to_variant());
        type_filter(&window, SidebarPaneKind::Annotations, "note");
        let ch = chrome(&window);
        let st = state(&window).unwrap();
        let entry_start = |i: usize| st.annotation_entries.borrow()[i].src_span.start.raw();
        let list = super::super::sidebar::list_view_of(&ch.annotations_scroller).expect("a list");
        let selection = list
            .model()
            .and_then(|m| m.downcast::<gtk::SingleSelection>().ok())
            .expect("a selection");
        selection.set_selected(290);
        assert_eq!(
            st.annotations_selected.get(),
            Some(entry_start(290)),
            "precondition: the prior navigation went to annotation 290"
        );
        let vadj = ch.annotations_scroller.vadjustment();
        until(Clock::Frame, "the annotations list to be laid out", || {
            vadj.upper() > vadj.page_size() && vadj.page_size() > 0.0
        });
        crate::saferizer::scrollpos::jump(&vadj, vadj.upper());
        drain_for(Clock::Frame, Duration::from_millis(100));
        ch.annotations_filter.entry.grab_focus();

        assert!(press_down_in_box(&window, SidebarPaneKind::Annotations));
        assert_eq!(selection.selected(), 0, "Down selects the first annotation");
        assert_eq!(
            st.annotations_selected.get(),
            Some(entry_start(0)),
            "…which navigates to it"
        );
        let focused_comment = || {
            let mut w = GtkWindowExt::focus(&window);
            while let Some(cur) = w {
                if cur.parent().as_ref() == Some(list.upcast_ref()) {
                    return cur
                        .first_child()
                        .and_then(|b| b.first_child())
                        .and_downcast::<gtk::Label>()
                        .map(|l| l.text().to_string());
                }
                w = cur.parent();
            }
            None
        };
        until(
            Clock::Frame,
            "the focus to reach the first annotation",
            || focused_comment().as_deref() == Some("note 0"),
        );
        drain_for(Clock::Frame, Duration::from_millis(200));
        assert_eq!(
            focused_comment().as_deref(),
            Some("note 0"),
            "the focus stays on the first annotation's row"
        );
        window.destroy();
    }

    /// **12.29** — the scroll-spy's list reveal, which follows a tab switch, leaves the
    /// outline list where the reader has it while they are typing in its filter box.
    ///
    /// A tab switch is the reachable route: with an open filter in both tabs the box
    /// keeps the focus across it (`sync_to_tab` puts the focus back). A view-mode switch
    /// takes the focus out of the box, so it never reaches the guard.
    /// Mutation-checked: dropping the `filter_box_has_focus` guard in `wire_scroll_spy`
    /// scrolls the list to the caret's section and this fails.
    #[gtktest::test]
    fn a_tab_switch_does_not_scroll_the_list_under_the_filter_box() {
        let mut doc = String::from("# Top\n\n");
        for n in 0..60 {
            doc.push_str(&format!("## Section {n}\n\nbody\n\n"));
        }
        let window = open("sidebarfilter.spyreveal", &doc);
        window.set_default_size(1000, 500);
        window.change_action_state("view-mode", &"edit".to_variant());
        drain_for(Clock::Frame, Duration::from_millis(200));
        let st = state(&window).unwrap();
        // Tab A's section is far down: the spy's reveal would scroll the list to it.
        let line = doc.lines().position(|l| l == "## Section 50").unwrap() as i32;
        st.editor_buf
            .place_cursor(&st.editor_buf.iter_at_line(line).unwrap());
        type_filter(&window, SidebarPaneKind::Outline, "section");
        let ch = chrome(&window);
        let first = st.id;
        crate::window::create_tab_in_window(&window, &doc, None, false, false).unwrap();
        drain_for(Clock::Frame, Duration::from_millis(200));
        type_filter(&window, SidebarPaneKind::Outline, "section");
        let vadj = ch.outline_scroller.vadjustment();
        until(Clock::Frame, "the outline list to be laid out", || {
            vadj.upper() > vadj.page_size() && vadj.page_size() > 0.0
        });
        crate::saferizer::scrollpos::jump(&vadj, 0.0);
        ch.outline_filter.entry.grab_focus();
        until(Clock::Idle, "the box to take the focus", || {
            focus_within(&window, &ch.outline_filter.entry)
        });

        crate::window::actions::change_action_state(
            &window,
            "select-tab",
            &first.to_string().to_variant(),
        );
        drain_for(Clock::Frame, Duration::from_millis(400));
        assert!(
            focus_within(&window, &ch.outline_filter.entry),
            "precondition: the box kept the focus across the switch"
        );
        assert_eq!(vadj.value(), 0.0, "the list stays where the reader has it");
        window.destroy();
    }
}
