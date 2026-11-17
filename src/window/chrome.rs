//! Window chrome / layout: the swappable content slot, the outline sidebar, the
//! status bar, the find bar, and the outer box. These persistent widgets survive
//! every content re-render and view-mode swap; `build_chrome` returns the handles
//! `new_window` wires up and stores in `TabState`.
use super::*;
use crate::icons::Icon;

/// Persistent chrome widgets handed back to `new_window`.
pub(super) struct Chrome {
    pub content_box: gtk::Box,
    /// The freshly-rendered initial preview `GtkScrolledWindow`. `build_chrome`
    /// no longer appends it to `content_box` directly — `build_window` mounts it
    /// into the tab's persistent [`SplitView`](super::SplitView) (which itself
    /// becomes `content_box`'s single child) once the editor exists.
    pub initial_preview: gtk::Widget,
    pub tabs: crate::widgets::tab::TabView,
    pub conflict_toast: gtk::Box,
    pub recovery_toast: gtk::Box,
    pub recovery_toast_label: gtk::Label,
    pub backing_loss_toast: gtk::Box,
    pub backing_loss_toast_label: gtk::Label,
    pub info_toast: winstate::InfoToast,
    /// The outline section's scroller (child swapped by `refresh_outline`).
    pub outline_scroller: gtk::ScrolledWindow,
    /// The annotations section's scroller (child swapped by `refresh_annotations`).
    pub annotations_scroller: gtk::ScrolledWindow,
    /// The outline collapsible section's container box; `:visible == outline_visible`.
    pub outline_section: gtk::Box,
    /// The annotations collapsible section's container box; `:visible == annotations_visible`.
    pub annotations_section: gtk::Box,
    /// The whole sidebar — the vertical `GtkPaned` holding the two sections, and
    /// itself `content_paned`'s start child. `:visible == (outline_visible ||
    /// annotations_visible)` (the four-state rule, TDD 20.9); recomputed by
    /// `reconcile_sidebar_visibility`.
    ///
    /// It is a `GtkPaned` rather than a `GtkBox` so the reader can give the two
    /// sections whatever share of the height their document needs (TDD 20.21) — and
    /// it must stay exactly ONE level above each section, because
    /// `reconcile_sidebar_visibility` reaches it by ancestry (`scroller → section →
    /// here`) rather than by handle.
    pub sidebar_paned: gtk::Paned,
    /// The footer status bar (`window::statusbar`).
    pub statusbar: super::statusbar::StatusBar,
    /// The status bar's root box — what View ▸ Status Bar hides and shows (9.17).
    pub status_bar: gtk::Box,
    /// The annotations viewer's heading label (TDD 20.22).
    pub annotations_title: gtk::Label,
    /// The outline's and the annotations viewer's filter bars (`window::sidebarfilter`).
    pub outline_filter: super::FilterBar,
    pub annotations_filter: super::FilterBar,
    pub find_entry: gtk::SearchEntry,
    /// The drop-down of this tab's recent searches, beside the find field.
    pub find_history_btn: gtk::MenuButton,
    /// The same, for the replacement field.
    pub replace_history_btn: gtk::MenuButton,
    pub find_prev_btn: gtk::Button,
    pub find_next_btn: gtk::Button,
    pub match_count_label: gtk::Label,
    pub close_find_btn: gtk::Button,
    pub replace_entry: gtk::Entry,
    pub replace_btn: gtk::Button,
    pub replace_all_btn: gtk::Button,
    pub replace_row: crate::widgets::wrapbox::ToolbarWrapBox,
    pub find_bar: gtk::Box,
    pub find_bar_revealer: gtk::Revealer,
    /// This window's OWN `View ▸ Documents` submenu model (GTK4Rs/AP-76) —
    /// filled per-window by `tabs.rs::refresh_documents_menu`.
    pub documents_menu: gtk::gio::Menu,
    /// This window's OWN `Format ▸` insert section — relabeled Insert↔Edit by
    /// `app::update_format_menu_labels`.
    pub format_insert_menu: gtk::gio::Menu,
    /// This window's self-built `GtkPopoverMenuBar` (GTK4Rs/AP-76). Retained
    /// so the nested-submenu-activation stray-menu workaround can walk
    /// its top-level item popovers and `popdown()` any GTK left mapped.
    /// `None` on macOS, where the menus live in the system menu bar instead.
    pub menubar: Option<gtk::PopoverMenuBar>,
    /// This window's whole menu model, retained for the platforms that render it
    /// outside the window (`platform::mac::menubar`).
    pub menu_model: gtk::gio::Menu,
}

/// Put this window's remembered sidebar divider back, once the sidebar has a height
/// to express it against (TDD 20.21).
///
/// **Not at build time, and not on `map`.** `gtk_paned_set_position` before the pane
/// has been allocated is clamped against a zero-size allocation and lost — the trap
/// `content_paned` below works around with a map-time re-apply, which is available to
/// it only because its position is a fixed px width rather than a fraction of a height
/// it cannot yet read. A fraction has nothing to multiply until the allocation exists.
/// So the hook is `notify::max-position`, which `GtkPaned` emits from
/// `gtk_paned_calc_position` — i.e. from a real `size_allocate`, which is precisely the
/// first moment the question has an answer. One-shot: it disconnects itself, so the
/// reader's later drags are never overwritten by a re-allocation (a window resize emits
/// it again).
/// How wide, in characters, a find-bar text field is — minimum and maximum both.
///
/// **A single fixed width, not a floor.** The find bar's rows are wrap boxes, which
/// give every child its NATURAL width, so an uncapped field's natural width becomes the
/// row's minimum and therefore the window's (TDD 9.38). Capping only the maximum would
/// leave the natural width free to grow; capping only the minimum would leave it free
/// to be the whole row. Pinning both is what makes the bar's contribution to the
/// window's floor a number chosen here rather than one emerging from a layout.
///
/// 24 characters holds an ordinary search term and a short regular expression, and sits
/// comfortably under `MIN_WINDOW_WIDTH`.
const FIND_FIELD_WIDTH_CHARS: i32 = 24;

/// Pin a find-bar field's width so it cannot decide the window's minimum.
///
/// Applies to BOTH fields from one place: they sit in two different rows built a
/// hundred lines apart, and a cap applied to one of them is a floor raised by the other
/// — which on macOS is not merely ugly, because a window is not grown to meet a risen
/// minimum there and the controls past the edge are silently not drawn.
fn cap_field_width(field: &impl IsA<gtk::Editable>) {
    let field = field.as_ref();
    field.set_width_chars(FIND_FIELD_WIDTH_CHARS);
    field.set_max_width_chars(FIND_FIELD_WIDTH_CHARS);
}

/// A find-bar history drop-down: a narrow button beside its field.
///
/// **A `GtkMenuButton`, not a combo box or a completion.** `GtkComboBoxText` and
/// `GtkEntryCompletion` are both deprecated as of GTK 4.10 and the gtk4-rs bindings
/// mark them `#[deprecated]`, which fails the zero-warning clippy gate; and the find
/// field has to stay a `GtkSearchEntry` for the `stop-search` binding that closes the
/// bar on Escape (GTK4Rs/AP-53). A sibling button leaves the entry exactly as it is.
///
/// **An icon and NO arrow — and the absence is chosen, not defaulted.** At this GTK
/// floor a `GtkMenuButton`'s child *type* decides whether it gets a down-indicator: the
/// `GtkImage` child type is the image by itself, while the `GtkLabel` child type builds
/// a box holding the label AND a `GtkBuiltinIcon` with css name `arrow`.
/// `always-show-arrow` (GTK 4.4, inside the 4.6 floor) would put that node back over an
/// image child, and it is deliberately not called. MEASURED here on GTK 4.6.9, both by
/// dumping the widget tree and by rendering the button: with it set, the `arrow` node
/// exists, reports itself visible, is ALLOCATED its share of the button — and PAINTS
/// NOTHING under Adwaita. The result is an icon shoved to the left of a button with an
/// empty half. Setting it is worse than not, and the reason is the same one that makes
/// the property look attractive: what the node draws is a theme's business, and the
/// theme does not consider this node its business.
///
/// This button was an EMPTY label before it carried an icon, and the reason it was
/// empty is the reason it is not a `set_label` now. It began as `set_label("▾")` — a
/// glyph rather than an icon name, deliberately, because an icon name is one more thing
/// to be missing from a host icon theme (GTK4Rs/AP-174, GTK4Rs/AP-48) — and it drew TWO
/// arrows on Windows, the application's chevron beside the toolkit's. `set_child` does
/// not avoid it either: a custom child is wrapped in the same `box[child, arrow]`. Note
/// what the pair of measurements says together: the toolkit's arrow is drawn on Windows
/// and not under Adwaita, so it is not something to build a layout around in either
/// direction. The empty label was the answer while the button had nothing to say; a
/// blank button says nothing about what it opens, which is what put an icon here.
///
/// The icon-theme hazard that argued against an icon name in the first place is
/// answered by BUNDLING rather than by avoiding the name — see
/// `data/resources.gresource.xml`, and note the host theme still wins where it has one.
fn history_button(accessible_name: &str) -> gtk::MenuButton {
    let btn = gtk::MenuButton::new();
    btn.set_icon_name(Icon::DocumentOpenRecent.name());
    btn.add_css_class("flat");
    // A freshly built window has committed nothing, and the bar is built before any tab
    // is registered — so the floor is set here and `findbar::sync_history_buttons`
    // raises it. The same deal `win.find-in-selection` takes at its registration.
    btn.set_sensitive(false);
    crate::a11y::name_with_tooltip(&btn, accessible_name, accessible_name);
    btn
}

fn restore_sidebar_split(paned: &gtk::Paned, fraction: f64) {
    // The handler disconnects itself, so it needs its own id — which `connect` only
    // returns after the closure has been built. The shared cell is the handover. It
    // holds no widget, so it closes no reference cycle (GTK4Rs/AP-63); the closure reaches
    // the paned through the emitter argument, never a captured clone.
    let handler: std::rc::Rc<std::cell::RefCell<Option<gtk::glib::SignalHandlerId>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot = handler.clone();
    let id = paned.connect_max_position_notify(move |paned| {
        let Some(position) = crate::session::sidebar_divider_position(fraction, paned.height())
        else {
            return; // no allocation yet — wait for the one that has a height
        };
        paned.set_position(position);
        if let Some(id) = slot.borrow_mut().take() {
            paned.disconnect(id);
        }
    });
    *handler.borrow_mut() = Some(id);
}

/// Keep the window's cached divider fraction in step with what the reader dragged, so
/// `read_window_chrome` has a meaningful value to persist at close (TDD 20.21).
///
/// Only a position the reader set is remembered. When the sidebar's HEIGHT changes, GtkPaned
/// moves the divider itself — proportionally, or clamped to a section's minimum when the
/// window is too short — and announces it with the same `notify::position` a drag
/// produces. GTK offers no flag saying which is which, so the height is the witness: a
/// divider that moved while the height moved is GTK's doing and is not stored, and the
/// remembered fraction is applied to the new height instead (which is what brings the
/// reader's split back when the window grows again). A height change reaches us as
/// `notify::max-position` (the divider's travel is height minus the end section's
/// minimum), in either order with the position notify, so both handlers keep `seen_height`
/// and the `resized` latch consistent whichever comes first.
///
/// Reads through `session::sidebar_split_fraction`, which declines to answer for a
/// hidden or unmapped sidebar — so a toggle that zeroes the sidebar's height cannot
/// overwrite the remembered split with a degenerate one.
fn track_sidebar_split(paned: &gtk::Paned) {
    use std::cell::Cell;
    use std::rc::Rc;
    // The sidebar height the divider was last seen at.
    let seen_height = Rc::new(Cell::new(0_i32));
    // Set by the max-position notify of a height change, consumed by the position notify
    // that follows it within the same allocation, and cleared by the next frame's tick
    // if none does.
    let resized = Rc::new(Cell::new(false));
    // Set while this code moves the divider, so its own notify is not read as a drag.
    let applying = Rc::new(Cell::new(false));

    fn chrome_of(paned: &gtk::Paned) -> Option<std::rc::Rc<crate::winstate::WindowChrome>> {
        // Resolve the host window at emission time rather than capturing one: a captured
        // window would be the GTK4Rs/AP-63 cycle and a stale one after a cross-window
        // move (GTK4Rs/AP-52).
        paned
            .root()
            .and_then(|r| r.downcast::<ApplicationWindow>().ok())
            .and_then(|w| winstate::chrome(&w))
    }

    {
        let (seen_height, resized, applying) =
            (seen_height.clone(), resized.clone(), applying.clone());
        paned.connect_max_position_notify(move |paned| {
            let height = paned.height();
            if height == seen_height.get() {
                return; // the end section's minimum moved, not the height
            }
            seen_height.set(height);
            resized.set(true);
            // The latch outlives only the allocation that set it: the next frame's tick
            // runs after layout, so a position notify that was never coming cannot leave
            // it set to swallow the reader's next drag.
            let latch = resized.clone();
            paned.add_tick_callback(move |_, _| {
                latch.set(false);
                gtk::glib::ControlFlow::Break
            });
            let Some(chrome) = chrome_of(paned) else {
                return;
            };
            let Some(position) =
                crate::session::sidebar_divider_position(chrome.sidebar_split.get(), height)
            else {
                return;
            };
            // Stay inside the divider's travel: a position set past it is accepted as the
            // property's value while GTK draws something else, and the two then disagree.
            let (lo, hi) = (paned.min_position(), paned.max_position());
            let position = if lo <= hi {
                position.clamp(lo, hi)
            } else {
                position
            };
            if position != paned.position() {
                applying.set(true);
                paned.set_position(position);
                applying.set(false);
            }
        });
    }
    paned.connect_position_notify(move |paned| {
        if applying.get() {
            return;
        }
        let height = paned.height();
        // A divider that moved while the height moved is GTK's (a clamp, or a
        // proportional resize), not the reader's.
        let height_moved = height != seen_height.get();
        seen_height.set(height);
        if resized.replace(false) || height_moved {
            return;
        }
        let Some(fraction) = crate::session::sidebar_split_fraction(paned.position(), height)
        else {
            return;
        };
        if let Some(chrome) = chrome_of(paned) {
            chrome.sidebar_split.set(fraction);
        }
    });
}

/// Render `md` into a preview widget wired the way every "swap this into
/// content_box" call site needs it: context menu attached and `vexpand` set so
/// it fills content_box vertically. Collapses the render+wire recipe that was
/// hand-repeated at every call site that builds a preview widget for
/// content_box.
pub(crate) fn render_and_wire_preview(
    notice_tab: crate::winstate::TabId,
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe_images: bool,
    folds: &crate::fold::FoldState,
) -> gtk::Widget {
    let widget = render(
        Some(notice_tab),
        md,
        doc_dir,
        zoom,
        allow_unsafe_images,
        folds,
    );
    widget.set_vexpand(true);
    attach_context_menu(&widget);
    widget
}

/// Assemble the window chrome around `toolbar`, render the initial content into the
/// swappable slot, and set the assembled tree as the window's child. `tab_id` is the
/// first tab's, already allocated, so its first render can name the tab it is shown in.
#[allow(clippy::too_many_arguments)] // one per input the window's first build is made of
pub(super) fn build_chrome(
    tab_id: crate::winstate::TabId,
    window: &ApplicationWindow,
    toolbar: &crate::widgets::wrapbox::ToolbarWrapBox,
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom_level: f64,
    show_unsafe_images: bool,
    sidebar_split: f64,
) -> Chrome {
    // ── layout ──────────────────────────────────────────────────────────────
    // content_box is the per-tab body / stack page. Its single child is the
    // tab's persistent `SplitView` (mounted by `build_window` once the editor
    // exists), not the raw preview — so the reused editor is never reparented on
    // a mode switch (ScrAP-58). The initial preview
    // is rendered here and handed back for `build_window` to install into that
    // SplitView.
    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_box.set_vexpand(true);
    // A FIRST build: there is no reader state yet, and saying so here is the point of
    // the argument existing (F-AP-B-101).
    let initial_preview = render_and_wire_preview(
        tab_id,
        md,
        doc_dir,
        zoom_level,
        show_unsafe_images,
        &crate::fold::FoldState::default(),
    );

    // ── tab strip (widgets/tab) ────────────────────────────────────────
    // content_box becomes this window's first (and, until a second tab is
    // added, only) tab page. `TabView`'s own widget — not content_box — now
    // occupies the overlay slot; content_box is unchanged otherwise (still the
    // thing view-mode swaps rebuild). The tab strip is ALWAYS shown, even at
    // exactly one page (TDD 15.7 — see
    // `window/tabs/update_window_title`'s doc comment for the full "why"),
    // and every tab is always reorderable/closable/context-menu-able by
    // construction — there is no `GtkNotebook`-style per-(container, child)
    // flag to re-apply on every new tab or cross-window arrival (formerly
    // GTK4Rs/AP-60's whole failure mode).
    let tabs = crate::widgets::tab::TabView::new();
    tabs.widget().set_vexpand(true);
    tabs.append_page(&content_box);
    // The initial tab is shown by GtkStack's default (first child) but never
    // travels through `switch_to_index`, so mark it active explicitly — otherwise
    // `active_idx`/`current_page` stay `None` for it, which silently breaks moving/
    // closing it (source left blank with stale outline) and Next/Previous Tab.
    tabs.mark_first_active();

    // The conflict toast floats over the content via a GtkOverlay; the content
    // slot itself is unchanged.
    //
    // Tried and reverted: wrapping ONLY `tabs.stack_widget()` in this overlay
    // and placing the tab strip as an external sibling above it (the
    // tree-topology fix GTK4Rs/AP-104 round 4's researcher consultation
    // recommended, matching libadwaita's AdwTabBar/AdwTabView split). Live
    // testing showed it made the residual "Trying to snapshot ... without a
    // current allocation" warning WORSE, not better — same mechanism, just
    // renaming which widget is nearest common ancestor, and the resulting
    // stuck-blank window grew from a single self-healing frame to over a
    // second, additionally swallowing the toolbar and tab strip rather than
    // just the content pane. Reverted to the combined `tabs.widget()`; see
    // GTK4Rs/AP-104 round 4 for the full account and the likely REAL
    // trigger (a newly-switched-to tab's own first-layout content
    // validation, independent of the tab strip entirely).
    let conflict_toast = super::toast::make_conflict_toast(window);
    let (recovery_toast, recovery_toast_label) = super::toast::make_recovery_toast(window);
    let (backing_loss_toast, backing_loss_toast_label) =
        super::toast::make_backing_loss_toast(window);
    let info_toast = super::toast::make_info_toast();
    let content_overlay = gtk::Overlay::new();
    content_overlay.set_vexpand(true);
    content_overlay.set_child(Some(tabs.widget()));
    content_overlay.add_overlay(&conflict_toast);
    content_overlay.add_overlay(&recovery_toast);
    content_overlay.add_overlay(&backing_loss_toast);
    content_overlay.add_overlay(info_toast.widget());

    // ── sidebar: two stacked collapsible sections ──────────────────────────────
    // A horizontal GtkPaned wraps the content: start child = the sidebar box, end
    // child = the content overlay. The Paned is a *sibling* of content_box's
    // swappable slot, so the sidebar survives every content re-render / mode swap.
    //
    // The sidebar stacks TWO `SidebarPane`s (outline over annotations) in a nested
    // VERTICAL GtkPaned, so the reader decides how the height is shared between them
    // (TDD 20.21). This deliberately departs from the GNOME HIG's advice against a
    // draggable divider between sidebar sections: these two are not fixed-size
    // groupings but independently scrolling LISTS whose useful heights are a property
    // of the document — a 60-heading outline beside two annotations wants nothing like
    // the 50/50 the previous GtkBox gave them, and there is no rule the app could
    // derive that would be right for the next document. The tool-window idiom
    // (JetBrains, VS Code) is the closer prior art, and it is the one the header
    // buttons already follow.
    //
    // Visibility is unchanged and remains the toggles' business, not the divider's:
    // each section's `:visible` is its own toggle's state; the whole sidebar's is
    // `outline_visible || annotations_visible` — so an empty sidebar disappears
    // entirely and the content reclaims the width (the four-state rule, TDD 20.9).
    // `reconcile_sidebar_visibility` recomputes all three. The two mechanisms cannot
    // fight, because the drag cannot reach zero: `shrink_*_child(false)` floors each
    // section at its own minimum (the header plus `SidebarPane`'s
    // `min_content_height`), so a section is hidden only by its action — never by the
    // divider, which would leave the action's state lying about what is on screen.
    //
    // Each pane's scroller inner child is (re)built from the current document —
    // `refresh_outline` / `refresh_annotations`.

    // Outline section: title + Expand-all / Collapse-all header buttons (JetBrains-
    // tool-window style — they (un)fold the WHOLE heading tree; each drives its own
    // non-stateful win.outline-expand-all / win.outline-collapse-all action, whose
    // handler reaches the CURRENT ListView's TreeListModel at click time and no-ops
    // on the "No headings" placeholder). Icons use the STANDARD
    // expand-all/collapse-all-symbolic names so a host theme that ships them
    // (e.g. breeze-dark on KDE) provides its own idiomatic glyphs; our matching
    // chevron art is bundled via gresource only as the fallback for themes lacking
    // those names, e.g. Adwaita (GTK4Rs/AP-48).
    let outline_expand_all = gtk::Button::from_icon_name(Icon::ExpandAll.name());
    crate::a11y::name(&outline_expand_all, "Expand all");
    outline_expand_all.set_action_name(Some("win.outline-expand-all"));
    let outline_collapse_all = gtk::Button::from_icon_name(Icon::CollapseAll.name());
    crate::a11y::name(&outline_collapse_all, "Collapse all");
    outline_collapse_all.set_action_name(Some("win.outline-collapse-all"));
    // Each pane's filter bar (TDD 12.26, 20.24): a search toggle in its header that
    // reveals a filter box under it. Built before the pane so the pane can place both.
    let outline_filter = super::FilterBar::new(super::SidebarPaneKind::Outline);
    let outline_pane = super::SidebarPane::new(
        "Outline",
        "win.outline",
        "Hide outline (F9)",
        &[outline_expand_all, outline_collapse_all],
        &outline_filter,
        180,
    );

    // Annotations section: title + close ×, no header buttons (a flat list has no
    // fold-all). The × drives the same win.annotations action as the menu/toolbar.
    let annotations_filter = super::FilterBar::new(super::SidebarPaneKind::Annotations);
    let annotations_pane = super::SidebarPane::new(
        "Annotations",
        "win.annotations",
        "Hide annotations",
        &[],
        &annotations_filter,
        180,
    );

    let sidebar_paned = gtk::Paned::new(gtk::Orientation::Vertical);
    sidebar_paned.add_css_class("sidebar");
    sidebar_paned.set_vexpand(true);
    sidebar_paned.set_start_child(Some(&outline_pane.root));
    sidebar_paned.set_end_child(Some(&annotations_pane.root));
    // Wide handle for the same reason `content_paned` below takes one, and it matters
    // more here: the narrow handle's hit-area is inflated 6px on EACH side and its drag
    // gesture runs in CAPTURE phase on the Paned, so a narrow handle would swallow
    // presses on the outline's bottom row and the annotations list's top row — both of
    // them navigation targets (GTK4Rs/AP-93).
    sidebar_paned.set_wide_handle(true);
    // Neither section may be dragged away to nothing: `shrink=false` floors the divider
    // at each child's own minimum, which keeps the pane's action the only thing that can
    // hide it (see the four-state note above). No `set_position` call, deliberately —
    // leaving `position-set` FALSE lets GtkPaned derive the split from the two minimums
    // (equal here, so an even split), and it also keeps the position out of the
    // set-before-allocation clamp `content_paned` has to work around below.
    sidebar_paned.set_shrink_start_child(false);
    sidebar_paned.set_shrink_end_child(false);
    // Both resizable, so a window-height change is shared rather than taken entirely
    // from one section.
    sidebar_paned.set_resize_start_child(true);
    sidebar_paned.set_resize_end_child(true);
    restore_sidebar_split(&sidebar_paned, sidebar_split);
    track_sidebar_split(&sidebar_paned);

    let outline_scroller = outline_pane.scroller.clone();
    let annotations_scroller = annotations_pane.scroller.clone();
    let outline_section = outline_pane.root.clone();
    let annotations_section = annotations_pane.root.clone();

    let content_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    content_paned.set_start_child(Some(&sidebar_paned));
    content_paned.set_end_child(Some(&content_overlay));
    content_paned.set_vexpand(true);
    // Wide handle — NOT for looks: with the default (narrow) handle, GtkPaned INFLATES
    // the handle's mouse hit-area by HANDLE_EXTRA_SIZE (6px) on EACH side
    // (`get_handle_area` → `graphene_rect_inset(area, -6, -6)`, gated on `!wide_handle`),
    // and its drag gesture is CAPTURE-phase on the Paned — an ANCESTOR of the preview.
    // So a press in the preview pane's left edge band was CLAIMED by the Paned's capture
    // gesture BEFORE the preview's own capture gesture ran, silently swallowing clicks on
    // the left portion of a task checkbox drawn in the left gutter (the marker sits at
    // buffer-x 7.5–20.5, i.e. right against the pane edge). `set_wide_handle(true)` sets
    // that inset to 0, so the hit-area is exactly the (still-narrow-looking) handle widget
    // and every content-pane press now reaches the preview. Researcher-diagnosed from
    // gtkpaned.c @ 4.6.9. The separator stays a thin divider
    // visually (verified) — the property gates the hit-inflation, not just the look.
    content_paned.set_wide_handle(true);
    // The sidebar keeps a usable minimum width and doesn't grow when the window
    // does (the content takes the extra space).
    content_paned.set_shrink_start_child(false);
    content_paned.set_resize_start_child(false);
    content_paned.set_resize_end_child(true);
    // Setting position before allocation clamps to 0 (researcher pitfall), so apply
    // the default width once, after the paned is mapped.
    // Capture the paned WEAKLY: a strong `content_paned.clone()` stored in its
    // OWN `map` signal handler is a self-cycle (paned → handler → closure → paned)
    // that never finalizes — and since the paned is the ancestor of the whole
    // content subtree, it stranded the entire per-window tab UI (TabBar, GtkStack,
    // SplitView, editor) on every window close (H2 — the dominant retainer). The
    // handler already receives the widget as its argument, so no capture is even
    // needed; use it directly.
    content_paned.connect_map(move |paned| {
        if paned.position() < 40 {
            paned.set_position(config().outline.width);
        }
    });

    // Footer status bar: messages at the left, indicators at the right, a fixed strip
    // outside the overlay. Its layout and refresh choke points are `window::statusbar`.
    let statusbar = super::statusbar::build();
    let status_bar = statusbar.root.clone();
    let annotations_title = annotations_pane.title.clone();

    // ── find bar ─────────────────────────────────────────────────────────────
    // A GtkRevealer in outer_box between the content area and the footer.
    // It survives mode/content swaps because it lives outside content_box.

    // A placeholder is not a name — GTK publishes it as `Placeholder`, which AT reads as
    // a hint about the *content* rather than the field's identity, and which vanishes as
    // soon as the user types. The constructor takes the name for that reason.
    let find_entry = crate::widgets::textfield::named_search_entry("Find");
    find_entry.set_placeholder_text(Some("Find…"));
    cap_field_width(&find_entry);
    let find_history_btn = history_button("Recent searches");

    let find_prev_btn = gtk::Button::from_icon_name(Icon::GoUp.name());
    crate::a11y::name_with_tooltip(
        &find_prev_btn,
        "Previous match",
        "Previous match (Shift+Enter)",
    );
    find_prev_btn.add_css_class("flat");

    let find_next_btn = gtk::Button::from_icon_name(Icon::GoDown.name());
    crate::a11y::name_with_tooltip(&find_next_btn, "Next match", "Next match (Enter)");
    find_next_btn.add_css_class("flat");

    let match_count_label = gtk::Label::new(None);
    match_count_label.set_visible(false);
    match_count_label.add_css_class("dim-label");

    let close_find_btn = gtk::Button::from_icon_name(Icon::WindowClose.name());
    crate::a11y::name_with_tooltip(&close_find_btn, "Close find bar", "Close (Escape)");
    close_find_btn.add_css_class("flat");

    // ── the match-option toggles ─────────────────────────────────────────────
    // After the field and before the navigation buttons, because that is what they
    // qualify: everything to their left is WHAT the reader is looking for, everything
    // to their right is how they move through what was found.
    //
    // Each is bound to its `win.` action by name — the same action the Edit menu's check
    // item drives — so neither surface holds state of its own (POLICY "One action per
    // command").
    //
    // The labels are ASCII WORDS, not the find-bar's conventional glyphs (`Aa`, `W`,
    // `.*`). A glyph only stands for the thing; `W` is not narrower than "Words" by
    // enough to buy the guess it demands, and `.*` is legible to somebody who already
    // knows what regular expressions are and opaque to everybody else. A tooltip is not
    // the answer to that: it is a second reading, and it is absent on a touch surface.
    //
    // CHECK BOXES rather than toggle buttons, because that is what these are — four
    // independent booleans, which is also exactly what the Edit menu shows for the same
    // four actions. A toggle button says "pressed", which is a weaker claim and the
    // reason three bare words in a toolbar read as stray text rather than as controls.
    let find_option_btns: Vec<gtk::CheckButton> = [
        ("find-match-case", "Case", "Match case", "Match case"),
        (
            "find-whole-word",
            "Words",
            "Whole word",
            "Match whole words only",
        ),
        (
            "find-regex",
            "Reg-Ex",
            "Regular expression",
            "Interpret the query as a regular expression",
        ),
    ]
    .into_iter()
    .map(|(action, glyph, name, tooltip)| {
        let btn = gtk::CheckButton::with_label(glyph);
        btn.set_action_name(Some(&format!("win.{action}")));
        crate::a11y::name_with_tooltip(&btn, name, tooltip);
        btn
    })
    .collect();

    // NOT a match option: it bounds WHERE matching is applied rather than what matches,
    // and the column it sits in is what says so. It still rides the same `win.` action,
    // so the Edit menu's check item and this one cannot disagree, and it is still a
    // FIND control: replace acts on what finding produced.
    let find_in_selection_check = gtk::CheckButton::with_label("Search in selection");
    find_in_selection_check
        .set_action_name(Some(&format!("win.{}", super::findbar::FIND_IN_SELECTION)));
    crate::a11y::name_with_tooltip(
        &find_in_selection_check,
        "Search in selection",
        "Confine the search to the selected passage",
    );

    // TWO COLUMNS, and the split carries the meaning the labels cannot: the left column
    // is how the query is READ, the right is where it is APPLIED. Four check boxes in
    // one line leave the reader to infer that from the wording; a column boundary
    // states it. The cost is honest and was weighed — the bar goes from two rows to
    // four, and that height comes out of the document view.
    //
    // A `GtkGrid` is ONE child of the wrap box, so the whole block wraps as a unit and
    // the columns cannot be split across a row boundary (TDD 9.38's "closely related
    // controls never split"). It also means the grid's natural width — not any single
    // check box's — is what the row contributes to the window's minimum, which is why
    // the labels stay short: at `MIN_WINDOW_WIDTH` of 360 the two columns together must
    // stay under the capped find field beside them, and
    // `no_chrome_sets_the_windows_width_floor_above_the_backstop` is the gate that says
    // whether they did.
    //
    // `halign(Start)` on every cell: a grid stretches its children to the column width
    // by default, which would leave each check box with a wide invisible click target
    // running to the next column.
    let find_option_grid = gtk::Grid::new();
    find_option_grid.set_row_spacing(2);
    find_option_grid.set_column_spacing(14);
    for (row, btn) in find_option_btns.iter().enumerate() {
        btn.set_halign(gtk::Align::Start);
        find_option_grid.attach(btn, 0, row as i32, 1, 1);
    }
    find_in_selection_check.set_halign(gtk::Align::Start);
    find_in_selection_check.set_valign(gtk::Align::Start);
    find_option_grid.attach(&find_in_selection_check, 1, 0, 1, 1);

    // A WRAP box, not a `GtkBox`: with everything shown this row is wider than a narrow
    // window, and a `GtkBox` would answer the sum of its children as its minimum width
    // and so raise the WINDOW's (TDD 9.38). Wrapping lets the row's contribution be its
    // widest single control instead.
    let find_row = crate::widgets::wrapbox::ToolbarWrapBox::new(4, 4);
    find_row.set_margin_top(4);
    find_row.set_margin_bottom(4);
    find_row.set_margin_start(6);
    find_row.set_margin_end(6);
    // The option grid is three rows tall, so everything beside it is a short child and
    // the wrap box would otherwise centre it — leaving the find field floating against
    // the middle of the check boxes instead of starting the bar. `Start` puts the field,
    // its history button and the navigation group on the grid's FIRST line, which is
    // where a reader looks for them.
    find_entry.set_valign(gtk::Align::Start);
    find_history_btn.set_valign(gtk::Align::Start);
    find_row.append(&find_entry);
    find_row.append(&find_history_btn);
    find_row.append(&find_option_grid);
    // Previous/Next travel together. `ToolbarWrapBox` wraps per child, so two separate
    // children may land on two rows — which is what happened the moment the option grid
    // grew tall enough to push them to the row's end: Prev sat at the right of one row
    // and Next opened the next. TDD 9.38 names a directional pair as a group that must
    // never split, and the toolbar answers it the same way, with one small box standing
    // as a single pack item (`toolbar::cluster`).
    // The readout travels with them. Prev/Next alone was not enough: with a live query
    // the count label appeared between the pair and what followed and pushed that onto a
    // row of its own. The label's width is not fixed — "1 of 10", "No matches" and
    // "Invalid pattern" are three different widths — so this group's natural width
    // varies with what the search is saying. That is safe only because the group stays
    // well under `MIN_WINDOW_WIDTH` even at its widest, and
    // `no_chrome_sets_the_windows_width_floor_above_the_backstop` is what says whether
    // that is still true.
    let find_nav_group = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    find_nav_group.append(&find_prev_btn);
    find_nav_group.append(&find_next_btn);
    find_nav_group.append(&match_count_label);
    find_nav_group.set_valign(gtk::Align::Start);
    find_row.append(&find_nav_group);

    let replace_entry = crate::widgets::textfield::named_entry("Replace with", "");
    replace_entry.set_placeholder_text(Some("Replace with…"));
    cap_field_width(&replace_entry);
    let replace_history_btn = history_button("Recent replacements");

    // NOT `.flat`. Every other control in this bar is an icon or a check box, whose
    // shape says what it is on its own; these two are bare words, and a flat button of
    // bare words is indistinguishable from a label. They are also the only two controls
    // here that CHANGE THE DOCUMENT, which is the last place to economise on the frame
    // that says "this is a button you are about to press".
    let replace_btn = gtk::Button::with_label("Replace");

    let replace_all_btn = gtk::Button::with_label("Replace All");

    let replace_row = crate::widgets::wrapbox::ToolbarWrapBox::new(4, 4);
    replace_row.set_margin_bottom(4);
    replace_row.set_margin_start(6);
    replace_row.set_margin_end(6);
    replace_row.append(&replace_entry);
    replace_row.append(&replace_history_btn);
    replace_row.append(&replace_btn);
    replace_row.append(&replace_all_btn);
    replace_row.set_visible(false);

    // The close button is NOT part of either row. It dismisses the whole bar, not the
    // row it happens to sit on, so it is pinned to the top-right corner of the bar and
    // the two wrapping rows are packed beside it. Inside the wrap box it was just
    // another item in the flow: it drifted left as the rows grew, sat directly above the
    // replace field at narrow widths, and read as that field's control.
    //
    // The spacer is `hexpand` on the BODY rather than a filler widget — a box gives its
    // extra width to the expanding child, so the body takes the slack and pushes the
    // close to the edge, with nothing in the tree that exists only to be empty.
    //
    // This does raise the window's minimum by the button's width, because the outer box
    // is a plain `GtkBox` and its minimum is the sum of the two. That is affordable only
    // while the widest wrap-box child plus this button stays under `MIN_WINDOW_WIDTH`,
    // and the gate that says so is
    // `no_chrome_sets_the_windows_width_floor_above_the_backstop`.
    let find_bar_body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    find_bar_body.set_hexpand(true);
    find_bar_body.append(&find_row);
    find_bar_body.append(&replace_row);

    close_find_btn.set_valign(gtk::Align::Start);
    close_find_btn.set_margin_top(8);
    close_find_btn.set_margin_end(6);

    let find_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    find_bar.append(&find_bar_body);
    find_bar.append(&close_find_btn);

    let find_bar_revealer = gtk::Revealer::new();
    find_bar_revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
    find_bar_revealer.set_child(Some(&find_bar));
    find_bar_revealer.set_reveal_child(false);

    // ── menu bar (per-window, self-built) ─────────────────────────────────────
    // We build our OWN GtkPopoverMenuBar per window (GTK4Rs/AP-76) instead of
    // relying on `ApplicationWindow.show_menubar(true)` + `app.set_menubar`,
    // because the View ▸ Documents submenu lists THIS window's tabs — per-window
    // content a single shared app model can't express. It sits at the very top of
    // the window, above the toolbar, exactly where the auto-built app menubar was.
    // `win.*` items resolve against this window's action muxer automatically once
    // the bar is in the tree (no explicit action-group insertion needed), and F10
    // still selects it (the bar self-registers on the window in `root()`).
    //
    // On macOS `bar` is `None` — that desktop keeps menus in the system menu bar,
    // where `platform::mac::menubar` exports this same model — so nothing is
    // packed and the toolbar becomes the window's first row (TDD 9.35).
    let menubar = crate::app::build_menubar();

    let outer_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    if let Some(bar) = &menubar.bar {
        outer_box.append(bar);
    }
    outer_box.append(toolbar);
    outer_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    outer_box.append(&content_paned);
    outer_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    outer_box.append(&find_bar_revealer);
    outer_box.append(&status_bar);
    window.set_child(Some(&outer_box));
    Chrome {
        content_box,
        initial_preview,
        tabs,
        conflict_toast,
        recovery_toast,
        recovery_toast_label,
        backing_loss_toast,
        backing_loss_toast_label,
        info_toast,
        outline_scroller,
        annotations_scroller,
        outline_section,
        annotations_section,
        sidebar_paned,
        statusbar,
        status_bar,
        annotations_title,
        outline_filter,
        annotations_filter,
        find_entry,
        find_history_btn,
        replace_history_btn,
        find_prev_btn,
        find_next_btn,
        match_count_label,
        close_find_btn,
        replace_entry,
        replace_btn,
        replace_all_btn,
        replace_row,
        find_bar,
        find_bar_revealer,
        documents_menu: menubar.documents_menu,
        format_insert_menu: menubar.format_insert_menu,
        menubar: menubar.bar,
        menu_model: menubar.model,
    }
}
