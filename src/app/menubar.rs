//! Per-window menubar construction. The menubar MODEL is built per window
//! (GTK4Rs/AP-76) — because `View ▸ Documents` lists THAT window's tabs and
//! `Format ▸`'s insert section is relabeled per that window's selection — so the
//! whole `GtkPopoverMenuBar` is assembled here per window rather than once on the
//! GApplication.
//!
//! Built from `FILE_CMDS` / `EDIT_CMDS` / `VIEW_CMDS` / `FORMAT_CMDS` so label and
//! accel stay in sync with the context menu and `set_accels_for_action`.

use super::commands::{
    inline_accel, Cmd, EDIT_CMDS, FILE_CMDS, FORMAT_CMDS, TBTN_SECTION_IDS, VIEW_CMDS,
};
use super::mnemonics::mnem;
use crate::winstate::FmtInsertKind;
use gtk::gio::{Menu, MenuItem};
use gtk::prelude::*;

/// Set a menu item's displayed `accel` attribute from a **declared** accelerator
/// string, re-spelled for the host by [`crate::accel::for_host`] — the same
/// transform `register_accelerators` applies, so the hint GTK draws beside the
/// item is the key the app actually binds on this platform.
///
/// Every `accel` attribute in this file goes through here. Setting the attribute
/// from a raw descriptor string is what would silently re-introduce the Ctrl-on-macOS
/// bug for one menu while the binding moved to Command — a per-surface spelling,
/// which is exactly what POLICY's accelerator single-source-of-truth rule forbids.
/// A no-op for an empty accel, so callers need no `is_empty` branch of their own.
fn set_accel(item: &MenuItem, accel: &str) {
    if accel.is_empty() {
        return;
    }
    item.set_attribute_value("accel", Some(&crate::accel::for_host(accel).to_variant()));
}

/// [`set_accel`] for a command with no Cmd-table row, reading its canonical
/// accelerator from the single `INLINE_ACCEL_CMDS` table (QA M-4). The menu hint
/// is thus derived from the SAME string `register_accelerators` binds and the
/// shortcuts window / toolbar tooltip show — the four can no longer drift.
/// `panic`s if `action` isn't an inline command (a compile-time wiring invariant).
fn set_inline_accel(item: &MenuItem, action: &str) {
    let accel = inline_accel(action)
        .unwrap_or_else(|| panic!("menu accel: {action} missing from INLINE_ACCEL_CMDS"));
    set_accel(item, accel);
}

/// Build one `win.format::<target>` menu item with an optional accel hint. Shared by
/// the menu construction and the live Insert↔Edit relabel so they cannot drift.
/// The label is routed through `mnem` so Format items carry their access keys too
/// (including across the Insert↔Edit relabel — both forms are in `MENU_MNEMONICS`).
fn make_format_item(label: &str, target: &str, accel: &str) -> MenuItem {
    let item = MenuItem::new(Some(&mnem(label)), None);
    item.set_action_and_target_value(Some("win.format"), Some(&target.to_variant()));
    set_accel(&item, accel);
    item
}

/// Relabel `window`'s OWN Format menu Link/Image items Insert↔Edit for its editor
/// selection `kind` (`None` = neither). Skips when that window's menu already shows
/// `kind`. Driven per-window by `window::update_format_edit_surfaces`. Per-window
/// since the menubar migration (GTK4Rs/AP-76): each window's Format menu now
/// reflects its OWN selection rather than the last-focused window's leaking across
/// every menubar.
pub(crate) fn update_format_menu_labels(
    window: &gtk::ApplicationWindow,
    kind: Option<FmtInsertKind>,
) {
    let Some(chrome) = crate::winstate::chrome(window) else {
        return;
    };
    if chrome.format_menu_kind.replace(kind) == kind {
        return;
    }
    let menu = &chrome.format_insert_menu;
    for (idx, k) in [(0_i32, FmtInsertKind::Link), (1, FmtInsertKind::Image)] {
        let accel = FORMAT_CMDS
            .iter()
            .find(|c| c.target == k.target())
            .map(|c| c.accel)
            .unwrap_or("");
        let item = make_format_item(k.label(kind == Some(k)), k.target(), accel);
        menu.remove(idx);
        menu.insert_item(idx, &item);
    }
}

/// One window's menubar: a self-built `GtkPopoverMenuBar` plus handles to the two
/// submenus whose CONTENT is mutated at runtime — `View ▸ Documents` (the
/// open-tab list) and `Format ▸`'s insert section (Link/Image Insert↔Edit). Both
/// must be per-window (GTK4Rs/AP-76), so the whole model is built here per
/// window rather than once on the GApplication.
pub(crate) struct BuiltMenubar {
    /// The in-window `GtkPopoverMenuBar`, on the platforms whose desktop puts the
    /// menus inside the window.
    ///
    /// `None` on macOS, where the menus belong in the *system* menu bar
    /// (`platform::mac::menubar` exports [`model`](Self::model) there). This is
    /// not merely a style choice: the system reveals a fullscreen window's title
    /// bar whenever the pointer nears the top edge, and that overlay lands
    /// exactly on the window's own first row — so an in-window menu bar there is
    /// unreachable by the gesture that reaches for it (TDD 9.35).
    pub bar: Option<gtk::PopoverMenuBar>,
    /// The assembled menu model itself — the top-level File/Edit/Format/View/Help
    /// submenus. Handed back so a platform that renders menus outside the window
    /// can export the *same* model the in-window bar would have shown, rather
    /// than assembling a second one that could drift from it.
    pub model: Menu,
    /// `View ▸ Documents` — starts empty; `window/tabs/refresh_documents_menu`
    /// fills it (in visual strip order) once the window's first tab is registered.
    pub documents_menu: Menu,
    /// `Format ▸`'s insert section (Link = 0, Image = 1, Table = 2), relabeled
    /// Insert↔Edit by `update_format_menu_labels`.
    pub format_insert_menu: Menu,
}

/// Build one flat command menu from a `Cmd` slice, inserting a section separator
/// before every `section_start` command. Shared by the File and Edit menus.
fn build_command_menu(cmds: &[Cmd]) -> Menu {
    let menu = Menu::new();
    let mut section = Menu::new();
    for cmd in cmds {
        if cmd.section_start && section.n_items() > 0 {
            menu.append_section(None, &section);
            section = Menu::new();
        }
        let item = MenuItem::new(Some(&mnem(cmd.label)), Some(cmd.action));
        set_accel(&item, cmd.accel);
        section.append_item(&item);
    }
    if section.n_items() > 0 {
        menu.append_section(None, &section);
    }
    menu
}

/// View menu — radio items targeting the win.view-mode stateful action.
/// GTK marks the active item automatically when the action state matches
/// the item's target value (D5). Returns the assembled View menu AND a handle
/// to its (initially empty) Documents submenu for `refresh_documents_menu`.
/// Reading Theme menu items for `action`, built from the installed themes — ONE model
/// builder for both surfaces, so they cannot list different themes. The action chosen
/// selects the presentation: the STATEFUL `app.preview-theme` (View menu) renders RADIO
/// items with a tick on the active theme, consistent with the other menubar menus; the
/// STATELESS `app.pick-preview-theme` (toolbar) renders PLAIN items, consistent with the
/// Heading picker. Both drive the same switch (the shim forwards), so they never diverge.
fn reading_theme_menu(action: &str) -> Menu {
    let menu = Menu::new();
    for (id, name, symbol) in crate::theme::themes().chooser_list() {
        let label = crate::theme::Themes::chooser_label(&name, symbol.as_deref());
        let item = MenuItem::new(Some(&mnem(&label)), None);
        item.set_action_and_target_value(Some(action), Some(&id.to_variant()));
        menu.append_item(&item);
    }
    menu
}

/// The View ▸ Reading Theme submenu — RADIO items (stateful `app.preview-theme`).
pub(crate) fn build_reading_theme_menu() -> Menu {
    reading_theme_menu("app.preview-theme")
}

/// The toolbar Reading Theme picker menu — PLAIN items (stateless `app.pick-preview-theme`).
pub(crate) fn build_reading_theme_toolbar_menu() -> Menu {
    reading_theme_menu("app.pick-preview-theme")
}

fn build_view_menu() -> (Menu, Menu) {
    // Back / Forward through this window's document-visit history (TDD §23), in
    // their own leading section — the browser's own placement, above the view
    // modes, so the two navigation commands read as a pair rather than as members
    // of the mode group. One `win.nav-*` action each, shared with the toolbar
    // buttons, the two accelerators and the mouse thumb buttons; GTK greys each
    // item whenever its action is disabled, which is the whole of the "insensitive
    // when it leads nowhere" contract (TDD 23.5, POLICY's single-`GAction` rule).
    let nav_section = Menu::new();
    for (label, action) in [("Back", "win.nav-back"), ("Forward", "win.nav-forward")] {
        let item = MenuItem::new(Some(&mnem(label)), Some(action));
        set_inline_accel(&item, action);
        nav_section.append_item(&item);
    }

    let section = Menu::new();
    for cmd in &VIEW_CMDS {
        let item = MenuItem::new(Some(&mnem(cmd.label)), None);
        item.set_action_and_target_value(
            Some("win.view-mode"),
            Some(&cmd.action_target.to_variant()),
        );
        set_accel(&item, cmd.accel);
        section.append_item(&item);
    }
    // Window-management commands that don't touch files (operator decision)
    // — New Window and Move Tab to New Window —
    // live in View, not File. QA round-2 N1 correction: Close Tab is
    // a document/tab-closing command, not a window-management one, so
    // it stays in File (below, near `menubar.append_submenu`) — this
    // comment previously (and an earlier reconstruction of it)
    // wrongly grouped it here. These View-menu items are ad-hoc
    // MenuItems (not driven by a Cmd-table array), so neither picks up a
    // toolbar button automatically
    // the way a FILE_CMDS/VIEW_CMDS table entry would: New Window
    // stays menu/keyboard-only, while Move Tab to New Window gets an
    // explicit toolbar-button exception (`toolbar.rs`, operator decision).
    let tabs_section = Menu::new();
    let new_window_item = MenuItem::new(Some(&mnem("New Window")), Some("win.new-window"));
    set_inline_accel(&new_window_item, "win.new-window");
    tabs_section.append_item(&new_window_item);
    let move_tab_item = MenuItem::new(
        Some(&mnem("Move Tab to New Window")),
        Some("win.move-tab-new-window"),
    );
    set_inline_accel(&move_tab_item, "win.move-tab-new-window");
    tabs_section.append_item(&move_tab_item);
    // Tab navigation + the Documents submenu (the fast-switch list of THIS
    // window's open tabs, one `win.select-tab::<id>` radio item each). The
    // submenu starts empty — its content is filled/refreshed per window by
    // `window/tabs/refresh_documents_menu` (deferred to idle: mutating a
    // GMenu bound to a live menubar mid-activation is unsafe, GTK4Rs/AP-76). It groups naturally beside Previous/Next Tab.
    let tab_nav_section = Menu::new();
    tab_nav_section.append_item(&MenuItem::new(
        Some(&mnem("Previous Tab")),
        Some("win.previous-tab"),
    ));
    tab_nav_section.append_item(&MenuItem::new(
        Some(&mnem("Next Tab")),
        Some("win.next-tab"),
    ));
    let documents_menu = Menu::new();
    tab_nav_section.append_submenu(Some(&mnem("Documents")), &documents_menu);

    // Outline sidebar toggle — a boolean win.outline action renders as a
    // checkbox item; its own section separates it from the mode radios.
    // Go To Line joins it here — both are "jump within the
    // document" navigation commands. One `win.go-to-line` action drives
    // this item, the toolbar button (toolbar.rs), and the accelerator
    // below (single source of truth); it's editor-only, so it's disabled
    // in preview mode and off editor focus (editoractions.rs +
    // editbar.rs's setup_editor_focus_gate).
    let outline_section = Menu::new();
    let outline_item = MenuItem::new(Some(&mnem("Outline")), Some("win.outline"));
    set_inline_accel(&outline_item, "win.outline");
    outline_section.append_item(&outline_item);
    // Annotations viewer toggle — a boolean win.annotations action, the sibling of
    // win.outline, sharing the same section (both toggle a sidebar pane). F8, mirroring
    // Outline's F9.
    let annotations_item = MenuItem::new(Some(&mnem("Annotations")), Some("win.annotations"));
    set_inline_accel(&annotations_item, "win.annotations");
    outline_section.append_item(&annotations_item);
    let go_to_line_item = MenuItem::new(Some(&mnem("Go To Line…")), Some("win.go-to-line"));
    set_inline_accel(&go_to_line_item, "win.go-to-line");
    outline_section.append_item(&go_to_line_item);

    // View-chrome visibility toggles — boolean `win.show-*` actions render as
    // checkbox items; their own section keeps them apart from the panel toggle.
    let chrome_section = Menu::new();
    // Toolbar is a SUBMENU (the per-section `show-tbtn-<id>` toggles):
    //   View ▸ Toolbar ▸ ┌ Show            (win.show-toolbar — the whole bar)
    //                    ├──────────────
    //                    │ File / Edit / Format / View / Split / Zoom
    //                    └ (win.show-tbtn-<id>, canonical order)
    // The first section is the whole-bar "Show" toggle (the pre-existing
    // behaviour); the second holds the six per-section checkbox items. GTK
    // greys the six whenever "Show" is off — their actions are disabled by
    // `reconcile_toolbar_chrome` (I3), which leaves their ticks intact (I4),
    // so re-showing the bar restores the exact per-section configuration.
    let toolbar_menu = Menu::new();
    let show_section = Menu::new();
    show_section.append_item(&MenuItem::new(
        Some(&mnem("Show")),
        Some("win.show-toolbar"),
    ));
    toolbar_menu.append_section(None, &show_section);
    let sections_section = Menu::new();
    for id in TBTN_SECTION_IDS {
        // Title-case the canonical ID for the label (drift-free — no second
        // list to keep in sync with TBTN_SECTION_IDS).
        let mut label = id.to_string();
        if let Some(head) = label.get_mut(0..1) {
            head.make_ascii_uppercase();
        }
        sections_section.append_item(&MenuItem::new(
            Some(&mnem(&label)),
            Some(&format!("win.show-tbtn-{id}")),
        ));
    }
    toolbar_menu.append_section(None, &sections_section);
    chrome_section.append_submenu(Some(&mnem("Toolbar")), &toolbar_menu);

    // Reading Theme — one RADIO item per installed theme, targeting the stateful
    // app.preview-theme action so GTK ticks the active theme, consistent with the other
    // menubar radio menus (view-mode etc.). The toolbar picker shows the SAME themes as
    // PLAIN items via the stateless shim; both drive one switch, so they can't diverge.
    //
    // Built from the theme registry, never a hardcoded list: adding a theme is a
    // block in themes.toml, and it must appear here without a code change (TDD 18.14).
    chrome_section.append_submenu(Some(&mnem("Reading Theme")), &build_reading_theme_menu());
    chrome_section.append_item(&MenuItem::new(
        Some(&mnem("Status Bar")),
        Some("win.show-statusbar"),
    ));

    // Zoom controls — three one-shot `win.zoom-*` actions; their
    // enabled state encodes both mode and ladder position (see
    // update_zoom_action_state in window.rs). Separated so the GTK
    // menubar draws a rule above them, keeping the View menu tidy.
    // accel comes from the SSOT INLINE_ACCEL_CMDS table via set_inline_accel (M-4).
    let make_zoom_item = |label: &str, action: &str| -> MenuItem {
        let item = MenuItem::new(Some(&mnem(label)), Some(action));
        set_inline_accel(&item, action);
        item
    };
    let zoom_section = Menu::new();
    zoom_section.append_item(&make_zoom_item("Zoom In", "win.zoom-in"));
    zoom_section.append_item(&make_zoom_item("Zoom Out", "win.zoom-out"));
    zoom_section.append_item(&make_zoom_item("Reset Zoom", "win.zoom-reset"));

    // Content safety: opt-in toggle to load remote images and images
    // outside the document folder.  Its own section keeps it visually
    // separate from the chrome visibility toggles above.
    let unsafe_images_section = Menu::new();
    unsafe_images_section.append_item(&MenuItem::new(
        Some(&mnem("Show Unsafe Images")),
        Some("win.show-unsafe-images"),
    ));

    // Split-pane arrangement — only enabled when in split mode (the actions
    // are disabled/greyed in preview/edit by apply_mode_action_state).
    let split_section = Menu::new();
    split_section.append_item(&MenuItem::new(
        Some(&mnem("Swap Panes")),
        Some("win.split-swap"),
    ));
    split_section.append_item(&MenuItem::new(
        Some(&mnem("Vertical Split")),
        Some("win.split-orientation"),
    ));

    let outer = Menu::new();
    outer.append_section(None, &nav_section);
    outer.append_section(None, &section);
    outer.append_section(None, &tabs_section);
    outer.append_section(None, &tab_nav_section);
    outer.append_section(None, &outline_section);
    outer.append_section(None, &chrome_section);
    outer.append_section(None, &unsafe_images_section);
    outer.append_section(None, &zoom_section);
    outer.append_section(None, &split_section);
    (outer, documents_menu)
}

/// Edit menu: the EDIT_CMDS commands plus an editor section mirroring the
/// GtkSourceView context menu — Insert Emoji and a Change Case submenu.
/// These are menu-only (not in EDIT_CMDS, so no toolbar buttons).
fn build_edit_menu() -> Menu {
    let edit_menu = build_command_menu(&EDIT_CMDS);
    let editor_section = Menu::new();
    // Annotate — an inline-accel command (Ctrl+Alt+M), menu-only here
    // (its toolbar/overlay button lives in the shared Format row); the accel hint is set
    // from the SSOT inline table, like Go To Line / Outline.
    let annotate_item = MenuItem::new(Some(&mnem("Annotate")), Some("win.annotate"));
    set_inline_accel(&annotate_item, "win.annotate");
    editor_section.append_item(&annotate_item);
    // The annotation walk, beside the command that creates one. Menu items are how a
    // reader FINDS a keyboard command — a shortcut nobody can discover is reachable
    // only by the people who already knew — and the Action CAM owes every command a
    // menu-bar surface in any case.
    for (label, action) in [
        ("Next Annotation", "win.next-annotation"),
        ("Previous Annotation", "win.prev-annotation"),
    ] {
        let item = MenuItem::new(Some(&mnem(label)), Some(action));
        set_inline_accel(&item, action);
        editor_section.append_item(&item);
    }
    editor_section.append(Some(&mnem("Insert Emoji")), Some("win.insert-emoji"));
    let change_case = Menu::new();
    change_case.append(Some(&mnem("UPPER CASE")), Some("win.change-case::upper"));
    change_case.append(Some(&mnem("lower case")), Some("win.change-case::lower"));
    change_case.append(Some(&mnem("Title Case")), Some("win.change-case::title"));
    change_case.append(Some(&mnem("tOGGLE cASE")), Some("win.change-case::toggle"));
    editor_section.append_submenu(Some(&mnem("Change Case")), &change_case);
    edit_menu.append_section(None, &editor_section);
    edit_menu
}

/// Format menu — all items target the parameterised win.format action.
/// Order: Bold, Italic, Heading ▸ (1–6 submenu), Strikethrough, Code Span,
/// Superscript, Subscript, Code Block, Quote, Bulleted List, Numbered List,
/// Horizontal Bar — then, after a separator (a second section), the Tier-2
/// insertions Link / Image / Table.
/// Heading is a submenu here (a combo box in the toolbar); both drive
/// win.format::h{1..6}. Returns the Format menu AND its insert-section handle
/// (per-window now, for the Insert↔Edit relabel — GTK4Rs/AP-76).
fn build_format_menu() -> (Menu, Menu) {
    // QA round-1 L6: a bare `.unwrap()` here panicked with no context
    // beyond "called `Option::unwrap()` on a `None` value" on a pure
    // typo in one of this closure's literal `t` strings vs.
    // `FORMAT_CMDS`'s `target` fields — two independently-edited
    // string lists with nothing tying them together. The diagnostic
    // message at least names the offending target directly.
    let by_target =
        |t: &str| {
            FORMAT_CMDS.iter().find(|c| c.target == t).unwrap_or_else(|| {
            panic!("build_format_menu: no FORMAT_CMDS entry with target {t:?} — check for a typo")
        })
        };
    let append = |menu: &Menu, t: &str| {
        let c = by_target(t);
        menu.append_item(&make_format_item(c.label, c.target, c.accel));
    };

    let inline = Menu::new();
    append(&inline, "bold");
    append(&inline, "italic");

    let heading = Menu::new();
    for n in 1..=6u8 {
        // The `_` before the digit is the access key; `mnem` (inside
        // make_format_item) is a no-op for this already-marked literal.
        heading.append_item(&make_format_item(
            &format!("Heading _{n}"),
            &format!("h{n}"),
            &format!("<Shift>F{n}"),
        ));
    }
    // mnem("Heading") → the _H access key (QA M-3: this site alone shipped the bare
    // label, so the Format ▸ Heading submenu lost its runtime mnemonic even though
    // the uniqueness test reserved H for it — a false pass).
    inline.append_submenu(Some(&mnem("Heading")), &heading);

    for t in [
        "strike",
        "highlight",
        "code-span",
        "sup",
        "sub",
        "code-block",
        "quote",
        "bulleted-list",
        "numbered-list",
        "task-list",
        "hr",
    ] {
        append(&inline, t);
    }

    // Insertions go in their own section so GTK draws a separator above them.
    let insert = Menu::new();
    for t in ["link", "image", "table"] {
        append(&insert, t);
    }

    let outer = Menu::new();
    outer.append_section(None, &inline);
    outer.append_section(None, &insert);
    (outer, insert)
}

/// File menu: the FILE_CMDS commands plus an ad-hoc Close Tab section inserted
/// BEFORE the trailing Exit section (an operator decision) —
/// menu/keyboard-only (Ctrl+W), never a FILE_CMDS table entry
/// (which would also generate an unwanted toolbar button).
fn build_file_menu() -> Menu {
    let file_menu = build_command_menu(&FILE_CMDS);
    let close_tab_section = Menu::new();
    // Rename sits with Close Tab and, like it, is built ad-hoc rather than as a
    // `FILE_CMDS` row: a row auto-generates a toolbar button, and Rename has none
    // (a granted CAM deviation — see CAM.md § Granted CAM exceptions).
    let rename_item = MenuItem::new(Some(&mnem("Rename…")), Some("win.rename"));
    set_inline_accel(&rename_item, "win.rename");
    close_tab_section.append_item(&rename_item);
    let close_tab_item = MenuItem::new(Some(&mnem("Close Tab")), Some("win.close-tab"));
    set_inline_accel(&close_tab_item, "win.close-tab");
    close_tab_section.append_item(&close_tab_item);
    file_menu.insert_section((file_menu.n_items() - 1).max(0), None, &close_tab_section);
    file_menu
}

/// Build one window's menubar model and wrap it in a `GtkPopoverMenuBar`. Called
/// once per window by `window::build_chrome`. `win.*` items resolve against the
/// window's OWN action muxer once the bar is packed into that window's widget
/// tree (the bar walks up to the window — researcher-confirmed against
/// gtkpopovermenubar.c), so no explicit action-group insertion is needed and
/// action state stays per-window. Keyboard accelerators are still app-wide
/// (`setup_app`'s `set_accels_for_action`) and independent of this model.
///
/// `GtkPopoverMenuBar` silently ignores `g_menu_item_set_icon()` — see
/// GTK4Rs/AP-11; icons are not set here.
pub(crate) fn build_menubar() -> BuiltMenubar {
    let help_menu = Menu::new();
    // Keyboard Shortcuts opens the GtkShortcutsWindow via the per-window
    // win.show-help-overlay action that `set_help_overlay` installs (TDD 16.1).
    let shortcuts_item = MenuItem::new(
        Some(&mnem("Keyboard Shortcuts")),
        Some("win.show-help-overlay"),
    );
    set_inline_accel(&shortcuts_item, "win.show-help-overlay");
    help_menu.append_item(&shortcuts_item);
    // Markdown Reference — opens the CommonMark syntax reference in the browser
    // (app.markdown-help). CommonMark because that's the syntax the app renders.
    help_menu.append(Some(&mnem("Markdown Reference")), Some("app.markdown-help"));
    // About sits alone in a trailing section (HIG: separated from the help items).
    let about_section = Menu::new();
    about_section.append(Some(&mnem("About")), Some("app.about"));
    help_menu.append_section(None, &about_section);

    let file_menu = build_file_menu();
    let edit_menu = build_edit_menu();
    let (view_menu, documents_menu) = build_view_menu();
    let (format_menu, format_insert_menu) = build_format_menu();

    let menubar = Menu::new();
    // Top-level bar titles carry the Alt+<letter> mnemonics directly (Alt+F/E/R/V/H).
    menubar.append_submenu(Some("_File"), &file_menu);
    menubar.append_submenu(Some("_Edit"), &edit_menu);
    menubar.append_submenu(Some("Fo_rmat"), &format_menu);
    menubar.append_submenu(Some("_View"), &view_menu);
    menubar.append_submenu(Some("_Help"), &help_menu);

    // macOS renders this model in the SYSTEM menu bar instead
    // (`platform::mac::menubar::track_active_window`), so no in-window bar is
    // built there at all — the two must never both exist, which is the defect
    // this returns `None` to prevent (TDD 9.35).
    let bar = if cfg!(target_os = "macos") {
        None
    } else {
        Some(gtk::PopoverMenuBar::from_model(Some(&menubar)))
    };
    BuiltMenubar {
        bar,
        model: menubar,
        documents_menu,
        format_insert_menu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recursively collect every SUBMENU title (the label on an item that carries a
    /// `submenu` link) from a built menu model, descending through sections.
    fn collect_submenu_titles(model: &gtk::gio::MenuModel, out: &mut Vec<String>) {
        for i in 0..model.n_items() {
            if let Some(sub) = model.item_link(i, "submenu") {
                if let Some(label) = model
                    .item_attribute_value(i, "label", None)
                    .and_then(|v| v.str().map(str::to_string))
                {
                    out.push(label);
                }
                collect_submenu_titles(&sub, out);
            }
            if let Some(sec) = model.item_link(i, "section") {
                collect_submenu_titles(&sec, out);
            }
        }
    }

    /// QA M-3: exercise the REAL menu-model builders (not the hardcoded mirror in
    /// `mnemonics.rs`, which reserved `H` for Heading yet never checked the runtime
    /// bound it), so a submenu title that forgets `mnem()` can no longer pass
    /// silently — `Format ▸ Heading` did exactly that (it shipped the bare
    /// "Heading"). These builders are pure `gio::Menu` models (no widgets), so the
    /// test runs headlessly.
    #[test]
    fn every_runtime_submenu_title_carries_a_mnemonic() {
        let (format_menu, _) = build_format_menu();
        let (view_menu, _) = build_view_menu();
        let mut titles = Vec::new();
        for model in [
            build_file_menu().upcast::<gtk::gio::MenuModel>(),
            build_edit_menu().upcast(),
            format_menu.upcast(),
            view_menu.upcast(),
        ] {
            collect_submenu_titles(&model, &mut titles);
        }
        assert!(!titles.is_empty(), "expected at least one nested submenu");
        // Every runtime submenu title must carry a GTK mnemonic marker ('_').
        for title in &titles {
            assert!(
                title.contains('_'),
                "runtime submenu title {title:?} has no mnemonic marker — missing mnem()?"
            );
        }
        // And specifically the title that regressed must use mnem("Heading").
        assert!(
            titles.contains(&mnem("Heading")),
            "Format ▸ Heading must use mnem(\"Heading\") = {:?}; got {titles:?}",
            mnem("Heading")
        );
    }
}
