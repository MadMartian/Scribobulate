//! Toolbar construction: file / edit / format / view / zoom / split command
//! buttons. Every button is action-driven (`set_action_name`), so sensitivity is
//! managed by the GAction machinery — no manual click handlers here.
//!
//! The toolbar is built as **six per-section boxes** (invariants I1–I7, see
//! [`super::viewactions`]),
//! one per visually-delimited group, in the canonical
//! [`crate::app::TBTN_SECTION_IDS`] order (`file, edit, format, view, split,
//! zoom`). Each section box's *first child* is its own leading vertical
//! `Separator` — uniformly, including `file` — so hiding a section
//! (`set_visible(false)` on its box) hides that section's separator with it, and
//! a doubled or a missing rule between visible sections is structurally
//! impossible. The `win.show-tbtn-<id>` actions (`viewactions.rs`) toggle these
//! boxes' visibility; the boxes are appended once and only ever `set_visible`d,
//! never removed/re-appended, so left-to-right order is preserved regardless of
//! toggle sequence (invariant I7).
use super::*;
use crate::icons::Icon;

/// What [`build_toolbar`] hands back for `build_window` to thread onward: the
/// toolbar box; the six per-section boxes (canonical `TBTN_SECTION_IDS` order, so the
/// section-visibility actions can toggle them); the Format bar box (the focus-gate
/// ancestor — the *inner* handle, wrapped by the `format` section box, not replaced by
/// it); the heading `MenuButton` (sensitivity mirrors `win.format`); the Insert↔Edit
/// Link/Image button set; the open-documents `MenuButton` (stored per window so its
/// label can track the active document — its menu-model is bound to the window's
/// `documents_menu` after the chrome exists); and the reading-theme `MenuButton`
/// (stored per window so its label/sensitivity can track the active theme + mode).
type BuiltToolbar = (
    gtk::Box,
    [gtk::Box; 6],
    gtk::Box,
    gtk::MenuButton,
    Vec<(FmtInsertKind, gtk::Button)>,
    gtk::MenuButton,
    gtk::MenuButton,
);

/// Build the main toolbar. See [`BuiltToolbar`] for the returned handles.
pub(super) fn build_toolbar() -> BuiltToolbar {
    // ── toolbar ─────────────────────────────────────────────────────────────
    // Icon buttons for all commands except Exit; GTK action machinery manages
    // sensitivity automatically via set_action_name (no manual connect_clicked).
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    toolbar.set_margin_top(2);
    toolbar.set_margin_bottom(2);
    toolbar.set_margin_start(4);
    toolbar.set_margin_end(4);

    // Each section is its own box whose first child is a leading vertical
    // separator (uniform ownership — invariant I7).
    // `new_section` returns `[ leading-sep ]` ready for the group's buttons.
    fn new_section() -> gtk::Box {
        let sec = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        let sep = gtk::Separator::new(gtk::Orientation::Vertical);
        sep.set_margin_top(4);
        sep.set_margin_bottom(4);
        sep.set_margin_start(2);
        sep.set_margin_end(2);
        sec.append(&sep);
        sec
    }

    // ── file section ─────────────────────────────────────────────────────────
    let file_box = new_section();
    for cmd in FILE_CMDS.iter().filter(|c| c.action != "app.quit") {
        if cmd.is_toggle {
            // A boolean toggle (e.g. Auto-Reload): GtkToggleButton reflects the
            // stateful action's value automatically via set_action_name.
            let btn = gtk::ToggleButton::new();
            btn.set_icon_name(cmd.icon.name());
            crate::a11y::name_with_accel(&btn, cmd.label, cmd.accel);
            btn.add_css_class("flat");
            btn.set_action_name(Some(cmd.action));
            file_box.append(&btn);
        } else {
            let btn = gtk::Button::from_icon_name(cmd.icon.name());
            crate::a11y::name_with_accel(&btn, cmd.label, cmd.accel);
            btn.add_css_class("flat");
            btn.set_action_name(Some(cmd.action));
            file_box.append(&btn);
        }
    }

    // ── edit section ───────────────────────────────────────────────────────────
    let edit_box = new_section();
    for cmd in EDIT_CMDS.iter() {
        let btn = gtk::Button::from_icon_name(cmd.icon.name());
        crate::a11y::name_with_accel(&btn, cmd.label, cmd.accel);
        btn.add_css_class("flat");
        btn.set_action_name(Some(cmd.action));
        edit_box.append(&btn);
    }

    // ── format section ─────────────────────────────────────────────────────────
    // Its own separator-delimited group. `format_box` is collected in one box so
    // the focus gate can test "focus is inside the Format toolbar" with a single
    // is_ancestor check; the section box merely WRAPS it (is_ancestor is
    // transitive, so wrapping keeps the gate valid — do not hand the wrapper
    // where `format_box` is expected). The same row layout backs the Stage-2
    // caret overlay (build_format_bar).
    let format_section = new_section();
    let (format_box, heading_btn, tb_edit_btns) = build_format_bar();
    format_section.append(&format_box);

    // ── view section ───────────────────────────────────────────────────────────
    let view_box = new_section();

    // Back / Forward (TDD §23) lead the section, mirroring their leading position
    // in the View menu and a browser's own left-to-right order. Plain buttons
    // driven by `win.nav-back`/`win.nav-forward` via `set_action_name`, so their
    // insensitivity is the actions' own (POLICY's single-`GAction` rule) and there
    // is nothing to keep in sync here.
    //
    // `go-previous-symbolic` / `go-next-symbolic` are freedesktop-spec names
    // already used by the find bar's match steppers, so they are known present in
    // the themes this app is verified against, and both are SYMBOLIC (a
    // non-symbolic icon's baked colour goes invisible on a dark-variant resolve —
    // ScrAP-169). They are direction glyphs rather than history glyphs by design:
    // no common theme ships a distinct "browser back" icon, and the arrows are the
    // convention every browser and file manager uses.
    for (dir, icon) in [
        (crate::winstate::NavDir::Back, Icon::GoPrevious),
        (crate::winstate::NavDir::Forward, Icon::GoNext),
    ] {
        let action = format!("win.{}", crate::window::nav_action_name(dir));
        let btn = gtk::Button::from_icon_name(icon.name());
        crate::a11y::name_from_action(&btn, &action);
        btn.add_css_class("flat");
        btn.set_action_name(Some(&action));
        view_box.append(&btn);
    }

    for cmd in VIEW_CMDS.iter() {
        let btn = gtk::ToggleButton::new();
        btn.set_icon_name(cmd.icon.name());
        crate::a11y::name_with_accel(&btn, cmd.label, cmd.accel);
        btn.add_css_class("flat");
        btn.set_action_name(Some("win.view-mode"));
        btn.set_action_target_value(Some(&cmd.action_target.to_variant()));
        view_box.append(&btn);
    }

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
    view_box.append(&documents_btn);

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
    view_box.append(&move_tab_btn);

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
    // reason for shunning GtkDropDown — its "(None)" empty-state caption, ScrAP-19 — does
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
        .label(&crate::theme::active().name)
        .focus_on_click(false)
        .menu_model(&crate::app::build_reading_theme_toolbar_menu())
        .build();
    // Named for what the control IS, not what it currently shows: its visible label is
    // the ACTIVE theme's name and `refresh_theme_button` rewrites it on every switch, so
    // a name derived from the label would announce a value where AT expects an identity.
    crate::a11y::name(&theme_btn, "Reading Theme");
    theme_btn.add_css_class("flat");
    view_box.append(&theme_btn);

    // Outline sidebar toggle — a boolean win.outline action (default on); reflects
    // and drives the panel's visibility like the view-mode toggles.
    let outline_btn = gtk::ToggleButton::new();
    outline_btn.set_icon_name(Icon::ViewList.name());
    crate::a11y::name_from_action(&outline_btn, "win.outline");
    outline_btn.add_css_class("flat");
    outline_btn.set_action_name(Some("win.outline"));
    view_box.append(&outline_btn);

    // Annotations viewer toggle — the sibling of the outline toggle (win.annotations,
    // default off). Same view section, per the Action CAM (this toggle appears in the
    // View menu AND this toolbar section, one GAction shared). A SYMBOLIC name (GTK4Rs/AP-125:
    // a non-symbolic icon's baked colour goes invisible on a dark-variant resolve).
    // `mail-mark-important-symbolic` (a marker/flag glyph, the annotation metaphor) was
    // verified present in Adwaita, breeze, AND breeze-dark on this system, so no
    // gresource fallback is needed — unlike expand-all/collapse-all-symbolic (ScrAP-39).
    let annotations_btn = gtk::ToggleButton::new();
    annotations_btn.set_icon_name(Icon::MailMarkImportant.name());
    crate::a11y::name_from_action(&annotations_btn, "win.annotations");
    annotations_btn.add_css_class("flat");
    annotations_btn.set_action_name(Some("win.annotations"));
    view_box.append(&annotations_btn);

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
    view_box.append(&go_to_line_btn);

    // "Show Unsafe Images" toggle — when on, remote (http/https) image URLs and
    // local images outside the document folder are loaded.  Driven by the
    // win.show-unsafe-images stateful boolean action registered below.
    // Icon is `Icon::EmblemPhotos`; the previous `image-x-generic-symbolic` is
    // missing from Breeze and rendered broken there. The replacement then turned out
    // to be missing from gvsbuild's Adwaita and rendered broken on Windows, so it is
    // now bundled in the GResource rather than swapped for a third name (see the Icon
    // variant's doc — ScrAP-39, surfaced by the icon-resolution test).
    let unsafe_images_btn = gtk::ToggleButton::new();
    unsafe_images_btn.set_icon_name(Icon::EmblemPhotos.name());
    crate::a11y::name(&unsafe_images_btn, "Show Unsafe Images");
    unsafe_images_btn.add_css_class("flat");
    unsafe_images_btn.set_action_name(Some("win.show-unsafe-images"));
    view_box.append(&unsafe_images_btn);

    // ── split section ─────────────────────────────────────────────────────────
    // Swap and reorient toggle buttons; meaningful only in split mode (gated by
    // apply_mode_action_state via set_action_name — sensitivity is automatic).
    let split_box = new_section();
    {
        let split_swap_btn = gtk::ToggleButton::new();
        split_swap_btn.set_icon_name(Icon::ObjectFlipHorizontal.name());
        crate::a11y::name(&split_swap_btn, "Swap Panes");
        split_swap_btn.add_css_class("flat");
        split_swap_btn.set_action_name(Some("win.split-swap"));
        split_box.append(&split_swap_btn);

        let split_orient_btn = gtk::ToggleButton::new();
        split_orient_btn.set_icon_name(Icon::ObjectFlipVertical.name());
        crate::a11y::name(&split_orient_btn, "Vertical Split");
        split_orient_btn.add_css_class("flat");
        split_orient_btn.set_action_name(Some("win.split-orientation"));
        split_box.append(&split_orient_btn);
    }

    // ── zoom section ─────────────────────────────────────────────────────────
    // Three flat icon buttons for zoom-in / zoom-reset / zoom-out, separated from
    // the view-mode controls. Sensitivity is managed entirely by the win.zoom-*
    // actions via set_action_name — no manual connect_clicked needed.
    let zoom_box = new_section();
    {
        let zoom_in = gtk::Button::from_icon_name(Icon::ZoomIn.name());
        crate::a11y::name_from_action(&zoom_in, "win.zoom-in");
        zoom_in.add_css_class("flat");
        zoom_in.set_action_name(Some("win.zoom-in"));
        zoom_box.append(&zoom_in);
        let zoom_reset = gtk::Button::from_icon_name(Icon::ZoomOriginal.name());
        crate::a11y::name_from_action(&zoom_reset, "win.zoom-reset");
        zoom_reset.add_css_class("flat");
        zoom_reset.set_action_name(Some("win.zoom-reset"));
        zoom_box.append(&zoom_reset);
        let zoom_out = gtk::Button::from_icon_name(Icon::ZoomOut.name());
        crate::a11y::name_from_action(&zoom_out, "win.zoom-out");
        zoom_out.add_css_class("flat");
        zoom_out.set_action_name(Some("win.zoom-out"));
        zoom_box.append(&zoom_out);
    }

    // Append the six section boxes ONCE, in canonical `TBTN_SECTION_IDS` order.
    // Afterwards only `set_visible` ever toggles them (never remove/append), so
    // their left-to-right order is fixed regardless of the show/hide sequence
    // (invariant I7). Order here MUST match `TBTN_SECTION_IDS`.
    toolbar.append(&file_box);
    toolbar.append(&edit_box);
    toolbar.append(&format_section);
    toolbar.append(&view_box);
    toolbar.append(&split_box);
    toolbar.append(&zoom_box);

    let sections = [
        file_box,
        edit_box,
        format_section,
        view_box,
        split_box,
        zoom_box,
    ];
    (
        toolbar,
        sections,
        format_box,
        heading_btn,
        tb_edit_btns,
        documents_btn,
        theme_btn,
    )
}
