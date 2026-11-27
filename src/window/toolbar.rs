//! Toolbar construction: file / edit / format / annotate / view / split / zoom command
//! buttons. Every button is action-driven (`set_action_name`), so sensitivity is
//! managed by the GAction machinery — no manual click handlers here.
//!
//! The toolbar is organized into **sections** (invariants I1–I8, see
//! [`super::toolbarchrome`]), one per visually-delimited group, in the canonical
//! [`crate::app::TBTN_SECTION_IDS`] order (`file, edit, format, annotate, view,
//! split, zoom`). A section is no longer one GTK container widget, though: each
//! section is a *list* of individually-wrappable pack items — its own leading
//! vertical `Separator`, then either a single button or a small `cluster` box
//! for 2–3 buttons that must never split across a wrap row (a directional
//! pair like Back/Forward, or Zoom in/reset/out) — every item appended
//! directly to the shared [`crate::widgets::wrapbox::ToolbarWrapBox`]. This is
//! what lets the toolbar wrap **per button** (packing as many items onto the
//! top row as fit, moving only the overflow down) rather than per whole
//! section, while still letting `app.show-tbtn-<id>` hide/show a section as
//! one atomic unit (I2) by toggling every widget in its item list together.
//! The Format section is the one exception: `build_format_bar`'s output is
//! kept as a single opaque box (see its own comment below) and so is, in
//! effect, its own one-item "cluster" — the biggest closely-related group of
//! all.
//!
//! The [`crate::widgets::wrapbox::ToolbarWrapBox`] itself does the actual
//! wrapping (TDD 9.38): it measures each of its direct
//! children (separators, buttons, clusters) at natural width
//! and packs as many as fit onto the current row, wrapping only the overflow
//! onto a new row, flush against the left edge — so the window's
//! content-derived minimum width is only ever the widest SINGLE item, and
//! narrowing the window fills each row before starting the next rather than
//! dropping whole sections at once. (A `GtkFlowBox` was tried first and
//! reverted: it computes a shared column grid across every row it produces,
//! so a wrapped row whose items differ in size from the row that set that
//! grid comes out offset/padded rather than packed against the left edge —
//! reported as odd gaps opening up on the left. `ToolbarWrapBox` does its own
//! left-to-right packing instead, with no grid to misalign.)
use super::*;
use crate::app::Cmd;
use crate::icons::Icon;

/// One section's flattened, individually-wrappable pack items (leading
/// separator, then buttons/clusters in order) — every entry is also a direct
/// child of the toolbar's `ToolbarWrapBox`. `app.show-tbtn-<id>` toggles a
/// whole section by setting `visible` on every widget in its list together
/// (invariant I2's generalisation from "one box" to "one list").
type SectionItems = Vec<gtk::Widget>;

/// What [`build_toolbar`] hands back for `build_window` to thread onward: the
/// toolbar box; the sections' item lists (canonical `TBTN_SECTION_IDS`
/// order, so the section-visibility actions can toggle them); the Format
/// command widgets, which the editor-focus gate tests membership against (a
/// list, not a container — see the format section in `build_toolbar` for why
/// there is no box to be an ancestor of any more); the heading `MenuButton`
/// (sensitivity mirrors `win.format`); the Insert↔Edit Link/Image button set;
/// the open-documents `MenuButton` (stored per window so its label can track
/// the active document — its menu-model is bound to the window's
/// `documents_menu` after the chrome exists); and the reading-theme
/// `MenuButton` (stored per window so its label/sensitivity can track the
/// active theme + mode).
type BuiltToolbar = (
    crate::widgets::wrapbox::ToolbarWrapBox,
    [SectionItems; crate::app::TBTN_SECTION_COUNT],
    Vec<gtk::Widget>,
    gtk::MenuButton,
    Vec<(FmtInsertKind, gtk::Button)>,
    gtk::MenuButton,
    gtk::MenuButton,
);

/// Append `w` to `toolbar` (as an individually-wrappable pack item) and
/// record it in `items` (so the section it belongs to can be shown/hidden as
/// one unit later). Collapses the "append + track" pair that would otherwise
/// be repeated at every button/cluster below.
fn push(
    toolbar: &crate::widgets::wrapbox::ToolbarWrapBox,
    items: &mut SectionItems,
    w: impl IsA<gtk::Widget>,
) {
    toolbar.append(&w);
    items.push(w.upcast());
}

/// A section's own leading vertical separator — uniformly, including `file`
/// (see the module doc) — appended like any other item so hiding the section
/// hides it with the rest (I2), but never bundled INTO a `cluster`, so it
/// never keeps a section's first button from wrapping on its own.
fn make_sep() -> gtk::Separator {
    let sep = gtk::Separator::new(gtk::Orientation::Vertical);
    sep.set_margin_top(4);
    sep.set_margin_bottom(4);
    sep.set_margin_start(2);
    sep.set_margin_end(2);
    sep
}

/// Wrap 2–3 closely-related buttons (a directional pair, a segmented
/// mode-switch group, zoom in/reset/out, …) in one small box so
/// `ToolbarWrapBox` treats them as a single atomic pack item: the group is
/// never split across a wrap row even though the toolbar otherwise wraps at
/// per-button granularity (operator decision, user-requested — "unless they
/// are closely related, like the arrow keys").
fn cluster(children: impl IntoIterator<Item = gtk::Widget>) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    for child in children {
        b.append(&child);
    }
    b
}

/// Build one plain, action-driven icon button from a [`Cmd`] row — the
/// FILE_CMDS/EDIT_CMDS toolbar-button shape shared by everything except the
/// toggle buttons and the hand-built View-section controls below.
fn cmd_button(cmd: &Cmd) -> gtk::Button {
    let btn = gtk::Button::from_icon_name(cmd.icon.name());
    crate::a11y::name_with_accel(&btn, cmd.label, cmd.accel);
    btn.add_css_class("flat");
    btn.set_action_name(Some(cmd.action));
    btn
}

/// Build one action-driven toggle button from a [`Cmd`] row (e.g. Auto-Reload).
fn cmd_toggle_button(cmd: &Cmd) -> gtk::ToggleButton {
    let btn = gtk::ToggleButton::new();
    btn.set_icon_name(cmd.icon.name());
    crate::a11y::name_with_accel(&btn, cmd.label, cmd.accel);
    btn.add_css_class("flat");
    btn.set_action_name(Some(cmd.action));
    btn
}

/// Build the main toolbar. See [`BuiltToolbar`] for the returned handles.
pub(super) fn build_toolbar() -> BuiltToolbar {
    // ── toolbar ─────────────────────────────────────────────────────────────
    // Icon buttons for all commands except Exit; GTK action machinery manages
    // sensitivity automatically via set_action_name (no manual connect_clicked).
    let toolbar = crate::widgets::wrapbox::ToolbarWrapBox::new(2, 0);
    toolbar.set_margin_top(2);
    toolbar.set_margin_bottom(2);
    toolbar.set_margin_start(4);
    toolbar.set_margin_end(4);

    // ── file section ─────────────────────────────────────────────────────────
    let mut file_items: SectionItems = Vec::new();
    push(&toolbar, &mut file_items, make_sep());
    for cmd in FILE_CMDS.iter().filter(|c| c.action != "app.quit") {
        if cmd.is_toggle {
            // A boolean toggle (e.g. Auto-Reload): GtkToggleButton reflects the
            // stateful action's value automatically via set_action_name.
            push(&toolbar, &mut file_items, cmd_toggle_button(cmd));
        } else {
            push(&toolbar, &mut file_items, cmd_button(cmd));
        }
    }

    // ── edit section ───────────────────────────────────────────────────────────
    let mut edit_items: SectionItems = Vec::new();
    push(&toolbar, &mut edit_items, make_sep());
    // Undo / Redo — a complementary opposite-direction pair, kept together as
    // one cluster (the same treatment as Back/Forward and zoom in/out below)
    // rather than left free to land on different rows from each other.
    let undo_redo = cluster([
        cmd_button(&EDIT_CMDS[0]).upcast::<gtk::Widget>(), // win.undo
        cmd_button(&EDIT_CMDS[1]).upcast::<gtk::Widget>(), // win.redo
    ]);
    push(&toolbar, &mut edit_items, undo_redo);
    for cmd in EDIT_CMDS.iter().skip(2) {
        push(&toolbar, &mut edit_items, cmd_button(cmd));
    }

    // ── format section ─────────────────────────────────────────────────────────
    // Decomposed into individually-wrappable items like every other section.
    // It did not used to be: `build_format_bar`'s single ~555px box was handed
    // to the wrap box whole, and since the wrap box's horizontal minimum is the
    // widest of its direct children, that ONE item set the window's minimum
    // width — measured at 563px of a 563px floor on Linux, and hard-locking a
    // Windows toplevel at 881px, i.e. WORSE than the 720px floor the wrap was
    // built to remove. Keeping it opaque defeated the whole change on any
    // window where Format was shown.
    //
    // The two things that made it look un-splittable both turned out not to
    // need a container. The editor-focus gate tested "focus is inside the
    // Format toolbar" with one `is_ancestor` call; it now takes the item list
    // and tests membership against it, which is the same question asked
    // directly (`editbar::focusgate`). The Stage-2 caret overlay still wants a
    // single row, and still gets one — `build_format_bar` packs the same items
    // into a box for it, so the two surfaces cannot drift.
    let (fmt_items, heading_btn, tb_edit_btns) = crate::window::editbar::format_items();
    let mut format_items: SectionItems = Vec::new();
    push(&toolbar, &mut format_items, make_sep());
    for it in &fmt_items {
        push(&toolbar, &mut format_items, it.clone());
    }

    // ── annotate section ───────────────────────────────────────────────────────
    // Comment, Add, Replace and Delete: the Edit ▸ Annotate submenu's items, built
    // from the same `ANNOTATE_SUBMENU` table so the two surfaces cannot drift, each
    // bound to that item's action and target (one GAction per command). Labelled
    // text buttons, not icons: no theme icon reads as "suggest a replacement", and
    // the labels are the words the selection popover and both menus use.
    // `focus_on_click(false)`: a click must not take focus from the preview or
    // editor, or it would take the selection the command acts on with it.
    let mut annotate_items: SectionItems = Vec::new();
    push(&toolbar, &mut annotate_items, make_sep());
    for (label, action, target) in crate::app::ANNOTATE_SUBMENU {
        let btn = gtk::Button::with_label(label);
        crate::a11y::name(&btn, label);
        btn.add_css_class("flat");
        btn.set_focus_on_click(false);
        btn.set_action_name(Some(action));
        if let Some(target) = target {
            btn.set_action_target_value(Some(&target.to_variant()));
        }
        push(&toolbar, &mut annotate_items, btn);
    }

    // ── view section ───────────────────────────────────────────────────────────
    let mut view_items: SectionItems = Vec::new();
    push(&toolbar, &mut view_items, make_sep());

    // Back / Forward (TDD §23) lead the section, mirroring their leading position
    // in the View menu and a browser's own left-to-right order. Plain buttons
    // driven by `win.nav-back`/`win.nav-forward` via `set_action_name`, so their
    // insensitivity is the actions' own (POLICY's single-`GAction` rule) and there
    // is nothing to keep in sync here. Clustered together — the canonical
    // "closely related, like the arrow keys" pair — so they never wrap apart.
    //
    // `go-previous-symbolic` / `go-next-symbolic` are freedesktop-spec names
    // already used by the find bar's match steppers, so they are known present in
    // the themes this app is verified against, and both are SYMBOLIC (a
    // non-symbolic icon's baked colour goes invisible on a dark-variant resolve —
    // GTK4Rs/AP-174). They are direction glyphs rather than history glyphs by design:
    // no common theme ships a distinct "browser back" icon, and the arrows are the
    // convention every browser and file manager uses.
    let nav_cluster = cluster(
        [
            (crate::winstate::NavDir::Back, Icon::GoPrevious),
            (crate::winstate::NavDir::Forward, Icon::GoNext),
        ]
        .into_iter()
        .map(|(dir, icon)| {
            let action = format!("win.{}", crate::window::nav_action_name(dir));
            let btn = gtk::Button::from_icon_name(icon.name());
            crate::a11y::name_from_action(&btn, &action);
            btn.add_css_class("flat");
            btn.set_action_name(Some(&action));
            btn.upcast::<gtk::Widget>()
        }),
    );
    push(&toolbar, &mut view_items, nav_cluster);

    // Preview / Edit / Split — a segmented, mutually-exclusive group (each a
    // `win.view-mode` target); clustered so the group is never split mid-row,
    // the same way a real segmented control couldn't be.
    let mode_cluster = cluster(VIEW_CMDS.iter().map(|cmd| {
        let btn = gtk::ToggleButton::new();
        btn.set_icon_name(cmd.icon.name());
        crate::a11y::name_with_accel(&btn, cmd.label, cmd.accel);
        btn.add_css_class("flat");
        btn.set_action_name(Some("win.view-mode"));
        btn.set_action_target_value(Some(&cmd.action_target.to_variant()));
        btn.upcast::<gtk::Widget>()
    }));
    push(&toolbar, &mut view_items, mode_cluster);

    // Open-documents combo box — the toolbar surface of the View ▸ Documents
    // fast-switch list, grouped here with the tab-management controls. It is a
    // `GtkMenuButton` over THIS window's `documents_menu` GMenu — the *same* model
    // the menubar's View ▸ Documents submenu binds, so the two surfaces cannot
    // diverge: one model (one item per open tab, each a `win.select-tab::<id>`
    // radio), rebuilt in one place (`refresh_documents_menu`). Adding/closing a tab
    // updates both popups at once, and the active tab is checked in both. The
    // menu_model is bound later (`build_window`), because the model lives in
    // WindowChrome and the toolbar is built before the chrome.
    //
    // Presented labelled like the Reading Theme picker below — flat, focus-
    // preserving, with the built-in dropdown arrow — so the two toolbar comboboxes
    // read consistently; its label shows the ACTIVE document's name (refreshed by
    // `refresh_documents_button`), so it reads as a real combobox showing its
    // current value. `GtkMenuButton`, not `GtkDropDown`, for the single-GAction
    // reason spelled out at `theme_btn`: the model's items already carry the action
    // name + target, so GTK dispatches the switch itself — a GtkDropDown would need
    // a selected→action and a state→selected mirror hand-wired per surface, exactly
    // the divergence the one-GAction rule prevents. `focus_on_click(false)` (as the
    // theme picker and Go To Line): opening the dropdown must not steal editor focus
    // and trip the Format focus gate.
    let documents_btn = gtk::MenuButton::builder().focus_on_click(false).build();
    crate::a11y::name(&documents_btn, "Documents");
    documents_btn.add_css_class("flat");
    push(&toolbar, &mut view_items, documents_btn.clone());

    // Move Tab to New Window — operator exception: a window/tab-management
    // command that nonetheless earns a
    // toolbar button for being frequent enough to justify it, unlike New
    // Window / Close Tab which stay menu/keyboard-only. No freedesktop
    // standard icon exists for "detach tab" — `tab-detach-symbolic` (the name
    // several GNOME apps use for this exact action) was tried first but is
    // missing from both Adwaita and Breeze (rendered as a broken-image glyph
    // in the manual TDD sweep); `send-to-symbolic` IS a freedesktop-spec name
    // present in both, so it's used here instead despite being a looser
    // semantic match.
    let move_tab_btn = gtk::Button::from_icon_name(Icon::SendTo.name());
    crate::a11y::name_from_action(&move_tab_btn, "win.move-tab-new-window");
    move_tab_btn.add_css_class("flat");
    move_tab_btn.set_action_name(Some("win.move-tab-new-window"));
    push(&toolbar, &mut view_items, move_tab_btn);

    // Reading Theme — the toolbar surface of the reading-theme command, beside the
    // View ▸ Reading Theme menu. A GtkMenuButton over a PLAIN-item GMenu model
    // (`build_reading_theme_toolbar_menu`), whose items target the stateless
    // `app.pick-preview-theme` shim so they render plain — consistent with the Heading
    // picker (the View menu shows radios via the stateful `app.preview-theme`; both
    // drive one switch, so they can't diverge). GtkMenuButton, not GtkDropDown: a
    // MenuButton's items carry the action name + target, so GTK dispatches the change
    // itself. A
    // GtkDropDown has no action-name property, so it would have needed selected→action
    // and state→selected mirrors wired by hand — precisely the per-surface divergence
    // the single-GAction rule exists to prevent. (Note the Heading picker's OTHER
    // reason for shunning GtkDropDown — its "(None)" empty-state caption, GTK4Rs/AP-19 — does
    // NOT apply here: a theme is always active, so there is no no-selection state.)
    //
    // Presented as a labelled dropdown to match the Heading picker (`formatbar.rs`):
    // both are `flat`, focus-preserving `GtkMenuButton`s with a text label + the
    // built-in dropdown arrow, so the two toolbar comboboxes read consistently.
    // The label shows the ACTIVE theme's name (refreshed on switch by
    // `window::refresh_theme_button` via the re-render sweep), so the button reads as a
    // real combobox showing its current value — unlike the Heading picker's static
    // "(Hn)", because a theme HAS a well-defined current value where a caret's heading
    // level does not.
    let theme_btn = gtk::MenuButton::builder()
        .label(crate::window::theme_button_label(
            &crate::theme::active().name,
            None,
        ))
        .focus_on_click(false)
        .menu_model(&crate::app::build_reading_theme_toolbar_menu())
        .build();
    // Named for what the control IS, not what it currently shows: its visible label is
    // the ACTIVE theme's name and `refresh_theme_button` rewrites it on every switch, so
    // a name derived from the label would announce a value where AT expects an identity.
    crate::a11y::name(&theme_btn, "Reading Theme");
    theme_btn.add_css_class("flat");
    push(&toolbar, &mut view_items, theme_btn.clone());

    // Outline sidebar toggle — a boolean win.outline action (default on); reflects
    // and drives the panel's visibility like the view-mode toggles.
    let outline_btn = gtk::ToggleButton::new();
    outline_btn.set_icon_name(Icon::ViewList.name());
    crate::a11y::name_from_action(&outline_btn, "win.outline");
    outline_btn.add_css_class("flat");
    outline_btn.set_action_name(Some("win.outline"));
    push(&toolbar, &mut view_items, outline_btn);

    // Annotations viewer toggle — the sibling of the outline toggle (win.annotations,
    // default off). Same view section, per the Action CAM (this toggle appears in the
    // View menu AND this toolbar section, one GAction shared). A SYMBOLIC name (GTK4Rs/AP-125:
    // a non-symbolic icon's baked colour goes invisible on a dark-variant resolve).
    // An app-owned bundled name (`scribobulate-annotations-symbolic`, a speech bubble):
    // the generic `mail-mark-important-symbolic` resolved in the host theme to an
    // unrelated glyph (Breeze: a badged envelope), and a host theme beats the bundle.
    let annotations_btn = gtk::ToggleButton::new();
    annotations_btn.set_icon_name(Icon::Annotations.name());
    crate::a11y::name_from_action(&annotations_btn, "win.annotations");
    annotations_btn.add_css_class("flat");
    annotations_btn.set_action_name(Some("win.annotations"));
    push(&toolbar, &mut view_items, annotations_btn);

    // Go To Line — grouped next to Outline (both jump within the
    // document); "go-jump-symbolic" verified present in Adwaita and Breeze.
    // win.go-to-line drives this button, the View-menu item, and the
    // accelerator (single source of truth) — disabled in preview mode and off
    // editor focus (editoractions.rs / editbar.rs's setup_editor_focus_gate).
    // `set_focus_on_click(false)`, same reasoning as the Format buttons
    // (format_button below): the click must not steal focus from the editor,
    // or the focus gate would disable the action out from under its own click.
    let go_to_line_btn = gtk::Button::from_icon_name(Icon::GoJump.name());
    crate::a11y::name_from_action(&go_to_line_btn, "win.go-to-line");
    go_to_line_btn.add_css_class("flat");
    go_to_line_btn.set_focus_on_click(false);
    go_to_line_btn.set_action_name(Some("win.go-to-line"));
    push(&toolbar, &mut view_items, go_to_line_btn);

    // "Show Unsafe Images" toggle — when on, remote (http/https) image URLs and
    // local images outside the document folder are loaded.  Driven by the
    // win.show-unsafe-images stateful boolean action registered below.
    // Icon is `Icon::UnsafeImages`, an app-owned bundled name: every generic name tried
    // here was missing or drawn as something else on some host theme (see the Icon
    // variant's doc — GTK4Rs/AP-48, surfaced by the icon-resolution test).
    let unsafe_images_btn = gtk::ToggleButton::new();
    unsafe_images_btn.set_icon_name(Icon::UnsafeImages.name());
    crate::a11y::name(&unsafe_images_btn, "Show Unsafe Images");
    unsafe_images_btn.add_css_class("flat");
    unsafe_images_btn.set_action_name(Some("win.show-unsafe-images"));
    push(&toolbar, &mut view_items, unsafe_images_btn);

    // ── split section ─────────────────────────────────────────────────────────
    // Swap and reorient toggle buttons; meaningful only in split mode (gated by
    // apply_mode_action_state via set_action_name — sensitivity is automatic).
    // Clustered together — a two-button pair, the same treatment as Back/Forward.
    let mut split_items: SectionItems = Vec::new();
    push(&toolbar, &mut split_items, make_sep());
    let split_cluster = cluster({
        let split_swap_btn = gtk::ToggleButton::new();
        split_swap_btn.set_icon_name(Icon::ObjectFlipHorizontal.name());
        crate::a11y::name(&split_swap_btn, "Swap Panes");
        split_swap_btn.add_css_class("flat");
        split_swap_btn.set_action_name(Some("win.split-swap"));

        let split_orient_btn = gtk::ToggleButton::new();
        split_orient_btn.set_icon_name(Icon::ObjectFlipVertical.name());
        crate::a11y::name(&split_orient_btn, "Vertical Split");
        split_orient_btn.add_css_class("flat");
        split_orient_btn.set_action_name(Some("win.split-orientation"));

        [
            split_swap_btn.upcast::<gtk::Widget>(),
            split_orient_btn.upcast::<gtk::Widget>(),
        ]
    });
    push(&toolbar, &mut split_items, split_cluster);

    // ── zoom section ─────────────────────────────────────────────────────────
    // Three flat icon buttons for zoom-in / zoom-reset / zoom-out, separated from
    // the view-mode controls. Sensitivity is managed entirely by the win.zoom-*
    // actions via set_action_name — no manual connect_clicked needed. Clustered
    // together as one triplet, exactly the "closely related, like the arrow
    // keys" case — never split across a wrap row.
    let mut zoom_items: SectionItems = Vec::new();
    push(&toolbar, &mut zoom_items, make_sep());
    let zoom_cluster = cluster({
        let zoom_in = gtk::Button::from_icon_name(Icon::ZoomIn.name());
        crate::a11y::name_from_action(&zoom_in, "win.zoom-in");
        zoom_in.add_css_class("flat");
        zoom_in.set_action_name(Some("win.zoom-in"));

        let zoom_reset = gtk::Button::from_icon_name(Icon::ZoomOriginal.name());
        crate::a11y::name_from_action(&zoom_reset, "win.zoom-reset");
        zoom_reset.add_css_class("flat");
        zoom_reset.set_action_name(Some("win.zoom-reset"));

        let zoom_out = gtk::Button::from_icon_name(Icon::ZoomOut.name());
        crate::a11y::name_from_action(&zoom_out, "win.zoom-out");
        zoom_out.add_css_class("flat");
        zoom_out.set_action_name(Some("win.zoom-out"));

        [
            zoom_in.upcast::<gtk::Widget>(),
            zoom_reset.upcast::<gtk::Widget>(),
            zoom_out.upcast::<gtk::Widget>(),
        ]
    });
    push(&toolbar, &mut zoom_items, zoom_cluster);

    // Every item was already appended to `toolbar` directly (via `push`) as it
    // was built, in canonical `TBTN_SECTION_IDS` order — so left-to-right,
    // top-to-bottom order is fixed regardless of later show/hide toggling
    // (invariant I7), exactly as when a section was one box.
    let sections: [SectionItems; crate::app::TBTN_SECTION_COUNT] = [
        file_items,
        edit_items,
        format_items,
        annotate_items,
        view_items,
        split_items,
        zoom_items,
    ];
    (
        toolbar,
        sections,
        fmt_items,
        heading_btn,
        tb_edit_btns,
        documents_btn,
        theme_btn,
    )
}
