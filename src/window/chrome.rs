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
    pub status_label: gtk::Label,
    pub pos_label: gtk::Label,
    pub status_bar: gtk::Box,
    pub find_entry: gtk::SearchEntry,
    pub find_prev_btn: gtk::Button,
    pub find_next_btn: gtk::Button,
    pub match_count_label: gtk::Label,
    pub close_find_btn: gtk::Button,
    pub replace_entry: gtk::Entry,
    pub replace_btn: gtk::Button,
    pub replace_all_btn: gtk::Button,
    pub replace_row: gtk::Box,
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
fn restore_sidebar_split(paned: &gtk::Paned, fraction: f64) {
    // The handler disconnects itself, so it needs its own id — which `connect` only
    // returns after the closure has been built. The shared cell is the handover. It
    // holds no widget, so it closes no reference cycle (ScrAP-60); the closure reaches
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
/// Reads through `session::sidebar_split_fraction`, which declines to answer for a
/// hidden or unmapped sidebar — so a toggle that zeroes the sidebar's height cannot
/// overwrite the remembered split with a degenerate one.
fn track_sidebar_split(paned: &gtk::Paned) {
    paned.connect_position_notify(|paned| {
        let Some(fraction) =
            crate::session::sidebar_split_fraction(paned.position(), paned.height())
        else {
            return;
        };
        // Resolve the host window at emission time rather than capturing one: this
        // widget outlives nothing here, but a captured window would be the ScrAP-60
        // cycle and a stale one after a cross-window move (GTK4Rs/AP-52).
        if let Some(chrome) = paned
            .root()
            .and_then(|r| r.downcast::<ApplicationWindow>().ok())
            .and_then(|w| winstate::chrome(&w))
        {
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
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe_images: bool,
    folds: &crate::fold::FoldState,
    fold_epoch: u64,
) -> gtk::Widget {
    let widget = render(md, doc_dir, zoom, allow_unsafe_images, folds, fold_epoch);
    widget.set_vexpand(true);
    attach_context_menu(&widget);
    widget
}

/// Assemble the window chrome around `toolbar`, render the initial content into the
/// swappable slot, and set the assembled tree as the window's child.
pub(super) fn build_chrome(
    window: &ApplicationWindow,
    toolbar: &gtk::Box,
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
        md,
        doc_dir,
        zoom_level,
        show_unsafe_images,
        &crate::fold::FoldState::default(),
        // A first build has no earlier source to be stale against.
        0,
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
    let info_toast = super::toast::make_info_toast();
    let content_overlay = gtk::Overlay::new();
    content_overlay.set_vexpand(true);
    content_overlay.set_child(Some(tabs.widget()));
    content_overlay.add_overlay(&conflict_toast);
    content_overlay.add_overlay(&recovery_toast);
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
    let outline_pane = super::SidebarPane::new(
        "Outline",
        "win.outline",
        "Hide outline (F9)",
        &[outline_expand_all, outline_collapse_all],
        180,
    );

    // Annotations section: title + close ×, no header buttons (a flat list has no
    // fold-all). The × drives the same win.annotations action as the menu/toolbar.
    let annotations_pane = super::SidebarPane::new(
        "Annotations",
        "win.annotations",
        "Hide annotations",
        &[],
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
    // them navigation targets (ScrAP-119).
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

    // Footer status bar: one accessible label driven by a
    // StatusStack. Sits outside the overlay as a fixed strip.
    let status_label = Label::new(None);
    status_label.set_xalign(0.0);
    status_label.set_hexpand(true);
    status_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status_label.set_accessible_role(gtk::AccessibleRole::Status);
    // Caret line/column indicator (TDD 9.21) — a separate, persistent label
    // at the strip's far end (status_label's hexpand pushes it there), not
    // routed through StatusStack: StatusStack shows only its single latest
    // entry (base or transient), which would make a constantly-updating
    // position indicator fight over the same line as "Unsaved changes" and
    // reload/conflict notices instead of coexisting with them. Hidden
    // whenever the active tab has no editor pane to report a position for
    // (preview mode) — `refresh_position_indicator` (actions.rs).
    let pos_label = Label::new(None);
    pos_label.add_css_class("dim-label");
    pos_label.set_visible(false);
    let status_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    status_bar.set_margin_start(6);
    status_bar.set_margin_end(6);
    status_bar.set_margin_top(2);
    status_bar.set_margin_bottom(2);
    status_bar.append(&status_label);
    status_bar.append(&pos_label);

    // ── find bar ─────────────────────────────────────────────────────────────
    // A GtkRevealer in outer_box between the content area and the footer.
    // It survives mode/content swaps because it lives outside content_box.

    // A placeholder is not a name — GTK publishes it as `Placeholder`, which AT reads as
    // a hint about the *content* rather than the field's identity, and which vanishes as
    // soon as the user types. The constructor takes the name for that reason.
    let find_entry = crate::widgets::textfield::named_search_entry("Find");
    find_entry.set_placeholder_text(Some("Find…"));

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

    let find_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    find_row.set_margin_top(4);
    find_row.set_margin_bottom(4);
    find_row.set_margin_start(6);
    find_row.set_margin_end(6);
    find_row.append(&find_entry);
    find_row.append(&find_prev_btn);
    find_row.append(&find_next_btn);
    find_row.append(&match_count_label);
    find_row.append(&close_find_btn);

    let replace_entry = crate::widgets::textfield::named_entry("Replace with", "");
    replace_entry.set_placeholder_text(Some("Replace with…"));

    let replace_btn = gtk::Button::with_label("Replace");
    replace_btn.add_css_class("flat");

    let replace_all_btn = gtk::Button::with_label("Replace All");
    replace_all_btn.add_css_class("flat");

    let replace_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    replace_row.set_margin_bottom(4);
    replace_row.set_margin_start(6);
    replace_row.set_margin_end(6);
    replace_row.append(&replace_entry);
    replace_row.append(&replace_btn);
    replace_row.append(&replace_all_btn);
    replace_row.set_visible(false);

    let find_bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    find_bar.append(&find_row);
    find_bar.append(&replace_row);

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
        info_toast,
        outline_scroller,
        annotations_scroller,
        outline_section,
        annotations_section,
        sidebar_paned,
        status_label,
        pos_label,
        status_bar,
        find_entry,
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
