//! Per-window menubar construction. The menubar MODEL is built per window
//! (GTK4Rs/AP-76) — because `View ▸ Documents` lists THAT window's tabs and
//! `Format ▸`'s insert section is relabeled per that window's selection — so the
//! whole `GtkPopoverMenuBar` is assembled here per window rather than once on the
//! GApplication.
//!
//! Built from `FILE_CMDS` / `EDIT_CMDS` / `VIEW_CMDS` / `FORMAT_CMDS` so labels stay in
//! sync with the context menu. Accelerator HINTS are not set here at all — GTK derives
//! each one from `set_accels_for_action`; see [`item`] for why the `accel` attribute
//! this file used to write was removed rather than tidied.

use super::commands::{Cmd, EDIT_CMDS, FILE_CMDS, FORMAT_CMDS, TBTN_SECTION_IDS, VIEW_CMDS};
use super::mnemonics::mnem;
use crate::export::ExportTarget;
use crate::winstate::FmtInsertKind;
use gtk::gio::{Menu, MenuItem};
use gtk::prelude::*;

/// A menu item: mnemonic-marked label plus the action it drives. **No accelerator
/// hint is set here, deliberately**, and that is the whole of this file's accel story.
///
/// GTK derives the hint from the accelerators the application registered. The chain is
/// `gtk_menu_tracker_item_get_accel` → (no `accel` attribute) →
/// `gtk_action_muxer_get_primary_accel` → `accels[0]` as passed to
/// `gtk_application_set_accels_for_action`. It is public documented behaviour
/// (`gtkpopovermenu.c`'s class docs name `set_accels_for_action` as the usual source),
/// identical at GTK 4.6.9 / 4.12.0 / 4.22.4, and **backend-independent — every file in
/// that chain is core `gtk/`**. The macOS system menu bar, which
/// `platform::mac::menubar` feeds via `set_menubar`, is a different renderer but calls
/// the *same* accessor (`gtkapplication-quartz-menu.c`'s `didChangeAccel`), with a real
/// `GtkActionMuxer` as its observable, so the fallback is live there too and `<Meta>`
/// lands on `NSEventModifierFlagCommand`.
///
/// **This file used to set the attribute anyway, and that was the defect worth removing.**
/// [`crate::app::setup::accelerator_bindings_for`] already re-spells every accelerator
/// for the host *before* `set_accels_for_action`, so the attribute could only ever
/// restate what GTK was about to say — a second source of truth for the same string,
/// with nothing comparing the two. And where they disagree the attribute WINS silently:
/// MEASURED, a build that set `<Primary><Alt>F12` on Zoom In rendered exactly that while
/// `Ctrl++` went on working. So the mechanism could not add a hint, and could only ever
/// subtract correctness — POLICY's "a second copy is how the first one silently stops
/// matching", with the copy losing.
///
/// A review (M36/M-4) read the two items that had *never* set the attribute — Previous
/// Tab and Next Tab — as a live defect showing no key. They were not: two release
/// binaries driven identically on GTK 4.6.9/X11 both showed `Ctrl+Page Up` /
/// `Ctrl+Page Down`, and the macOS seat read `⌘PageUp`/`⌘PageDown` off the live system
/// menu bar through the Accessibility API. Those two items were right and the other
/// sixteen were carrying redundant weight.
///
/// What replaces the mechanism is `every_menu_command_with_a_shortcut_is_registered`:
/// the fallback fires only for actions the muxer can resolve, so making
/// `set_accels_for_action` the sole source makes *registration* the thing worth
/// asserting.
fn item(label: &str, action: &str) -> MenuItem {
    MenuItem::new(Some(&mnem(label)), Some(action))
}

/// Build one `win.format::<target>` menu item. Shared by the menu construction and the
/// live Insert↔Edit relabel so they cannot drift. The label is routed through `mnem` so
/// Format items carry their access keys too (including across the Insert↔Edit relabel —
/// both forms are in `MENU_MNEMONICS`). The accel hint comes from the registered
/// binding for `win.format::<target>`, as for every item here — see [`item`].
fn make_format_item(label: &str, target: &str) -> MenuItem {
    let menu_item = MenuItem::new(Some(&mnem(label)), None);
    menu_item.set_action_and_target_value(Some("win.format"), Some(&target.to_variant()));
    menu_item
}

/// Run `mutate` against `window`'s chrome on the next main-loop idle, coalescing
/// repeat requests into one run.
///
/// **The choke point for mutating a `GMenu` that is bound to a live
/// `GtkPopoverMenuBar`** (GTK4Rs/AP-76). Such a mutation is synchronous and unsafe from
/// inside a signal dispatch — worst of all from a menu item's own activation, where GTK
/// frees the `GtkModelButton` mid-`clicked` because `gtkmenusectionbox.c` refs only the
/// popover. Deferring to idle puts the mutation outside any dispatch.
///
/// It exists as a shared function because the rule had been applied to one of the two
/// live-bound submenus and not the other. `View ▸ Documents` had the idle machinery and
/// an emphatic "never call it directly from a signal handler"; `Format ▸`'s insert
/// section, an equally live-bound child of the same model, was relabelled straight from
/// three signal handlers. A mitigation that each site has to remember is one the next
/// site will not, so the two `GMenu` handles are now reached only through here.
///
/// `scheduled` selects that menu's own coalescing flag, so the two submenus queue
/// independently. The closure re-resolves the chrome rather than capturing it: a window
/// can be gone by the time the idle fires, and a strong capture would keep it alive
/// (ScrAP-60 / GTK4Rs/AP-128).
pub(crate) fn defer_live_menu_mutation(
    window: &gtk::ApplicationWindow,
    scheduled: fn(&crate::winstate::WindowChrome) -> &std::cell::Cell<bool>,
    mutate: impl Fn(&gtk::ApplicationWindow, &crate::winstate::WindowChrome) + 'static,
) {
    let Some(chrome) = crate::winstate::chrome(window) else {
        return;
    };
    if scheduled(&chrome).replace(true) {
        return; // a run is already queued for this window's menu
    }
    glib::idle_add_local_once(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move || {
            let Some(chrome) = crate::winstate::chrome(&w) else {
                return;
            };
            scheduled(&chrome).set(false);
            mutate(&w, &chrome);
        }
    ));
}

/// Relabel `window`'s OWN Format menu Link/Image items Insert↔Edit for its editor
/// selection `kind` (`None` = neither). Driven per-window by
/// `window::update_format_edit_surfaces`. Per-window since the menubar migration
/// (GTK4Rs/AP-76): each window's Format menu now reflects its OWN selection rather than
/// the last-focused window's leaking across every menubar.
///
/// **The mutation is deferred to idle** through [`defer_live_menu_mutation`], because
/// this menu is a live-bound child of the window's `GtkPopoverMenuBar` exactly as
/// `View ▸ Documents` is. All three of this function's callers are signal handlers, and
/// the `win.format` `notify::enabled` one fires precisely when focus moves into a menu
/// popover — the mid-activation window the rule is about.
///
/// Requested kind and displayed kind are tracked separately so coalescing is correct:
/// the last request in a turn wins, and a request that returns to what is already shown
/// costs no mutation at all.
pub(crate) fn update_format_menu_labels(
    window: &gtk::ApplicationWindow,
    kind: Option<FmtInsertKind>,
) {
    let Some(chrome) = crate::winstate::chrome(window) else {
        return;
    };
    // Record the request synchronously — cheap, and it is what makes the last writer in
    // a turn the one the idle honours.
    chrome.format_menu_pending.set(kind);
    if chrome.format_menu_kind.get() == kind {
        return; // the menu already shows this; nothing to schedule
    }
    defer_live_menu_mutation(
        window,
        |chrome| &chrome.format_menu_refresh_scheduled,
        |_, chrome| {
            let kind = chrome.format_menu_pending.get();
            // Re-check against what is DISPLAYED: an A→B→A toggle inside one turn
            // resolves to no mutation at all.
            if chrome.format_menu_kind.replace(kind) == kind {
                return;
            }
            let menu = &chrome.format_insert_menu;
            for (idx, k) in [(0_i32, FmtInsertKind::Link), (1, FmtInsertKind::Image)] {
                let relabelled = make_format_item(k.label(kind == Some(k)), k.target());
                menu.remove(idx);
                menu.insert_item(idx, &relabelled);
            }
        },
    );
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
        section.append_item(&item(cmd.label, cmd.action));
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
///
/// Takes its themes as an ARGUMENT rather than reading the installed set. This function is
/// reached by `build_top_level_menus`, which the mnemonics guards derive their access-key
/// namespaces from — so with an implicit `crate::theme::themes()` the guard's input set
/// varied with whatever theme files happened to be on the machine running it, and a
/// collision could appear or vanish with a `~/.config` edit that touched no source. On
/// macOS this menu model is also handed to the SYSTEM menu bar, so a filesystem-dependent
/// menu is user-visible behaviour and not only a test-hygiene problem.
fn reading_theme_menu(action: &str, themes: &crate::theme::Themes) -> Menu {
    let menu = Menu::new();
    for entry in themes.chooser_list() {
        // A theme name comes from a user-editable `themes.toml`, so this is DYNAMIC text
        // in a mnemonic context: escape it, and do not route it through `mnem()`, whose
        // table is keyed on static command labels and would inject a marker into any theme
        // sharing one of them.
        let label = crate::theme::Themes::chooser_label(&entry.label, entry.symbol.as_deref());
        let item = MenuItem::new(Some(&crate::app::escape_mnemonic(&label)), None);
        item.set_action_and_target_value(Some(action), Some(&entry.id.to_variant()));
        menu.append_item(&item);
    }
    menu
}

/// The toolbar Reading Theme picker menu — PLAIN items (stateless `app.pick-preview-theme`).
pub(crate) fn build_reading_theme_toolbar_menu() -> Menu {
    reading_theme_menu("app.pick-preview-theme", &crate::theme::themes())
}

fn build_view_menu(themes: &crate::theme::Themes) -> (Menu, Menu) {
    // Back / Forward through this window's document-visit history (TDD §23), in
    // their own leading section — the browser's own placement, above the view
    // modes, so the two navigation commands read as a pair rather than as members
    // of the mode group. One `win.nav-*` action each, shared with the toolbar
    // buttons, the two accelerators and the mouse thumb buttons; GTK greys each
    // item whenever its action is disabled, which is the whole of the "insensitive
    // when it leads nowhere" contract (TDD 23.5, POLICY's single-`GAction` rule).
    let nav_section = Menu::new();
    for (label, action) in [("Back", "win.nav-back"), ("Forward", "win.nav-forward")] {
        nav_section.append_item(&item(label, action));
    }

    let section = Menu::new();
    for cmd in &VIEW_CMDS {
        let mode_item = MenuItem::new(Some(&mnem(cmd.label)), None);
        mode_item.set_action_and_target_value(
            Some("win.view-mode"),
            Some(&cmd.action_target.to_variant()),
        );
        section.append_item(&mode_item);
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
    tabs_section.append_item(&item("New Window", "win.new-window"));
    tabs_section.append_item(&item("Move Tab to New Window", "win.move-tab-new-window"));
    // Tab navigation + the Documents submenu (the fast-switch list of THIS
    // window's open tabs, one `win.select-tab::<id>` radio item each). The
    // submenu starts empty — its content is filled/refreshed per window by
    // `window/tabs/refresh_documents_menu` (deferred to idle: mutating a
    // GMenu bound to a live menubar mid-activation is unsafe, GTK4Rs/AP-76). It groups naturally beside Previous/Next Tab.
    let tab_nav_section = Menu::new();
    tab_nav_section.append_item(&item("Previous Tab", "win.previous-tab"));
    tab_nav_section.append_item(&item("Next Tab", "win.next-tab"));
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
    outline_section.append_item(&item("Outline", "win.outline"));
    // Annotations viewer toggle — a boolean win.annotations action, the sibling of
    // win.outline, sharing the same section (both toggle a sidebar pane). F8, mirroring
    // Outline's F9.
    outline_section.append_item(&item("Annotations", "win.annotations"));
    outline_section.append_item(&item("Go To Line…", "win.go-to-line"));

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
    show_section.append_item(&item("Show", "win.show-toolbar"));
    toolbar_menu.append_section(None, &show_section);
    let sections_section = Menu::new();
    for id in TBTN_SECTION_IDS {
        // Title-case the canonical ID for the label (drift-free — no second
        // list to keep in sync with TBTN_SECTION_IDS).
        let mut label = id.to_string();
        if let Some(head) = label.get_mut(0..1) {
            head.make_ascii_uppercase();
        }
        sections_section.append_item(&item(&label, &format!("win.show-tbtn-{id}")));
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
    chrome_section.append_submenu(
        Some(&mnem("Reading Theme")),
        &reading_theme_menu("app.preview-theme", themes),
    );
    chrome_section.append_item(&item("Status Bar", "win.show-statusbar"));

    // Zoom controls — three one-shot `win.zoom-*` actions; their
    // enabled state encodes both mode and ladder position (see
    // update_zoom_action_state in window.rs). Separated so the GTK
    // menubar draws a rule above them, keeping the View menu tidy.
    let zoom_section = Menu::new();
    zoom_section.append_item(&item("Zoom In", "win.zoom-in"));
    zoom_section.append_item(&item("Zoom Out", "win.zoom-out"));
    zoom_section.append_item(&item("Reset Zoom", "win.zoom-reset"));

    // Content safety: opt-in toggle to load remote images and images
    // outside the document folder.  Its own section keeps it visually
    // separate from the chrome visibility toggles above.
    let unsafe_images_section = Menu::new();
    unsafe_images_section.append_item(&item("Show Unsafe Images", "win.show-unsafe-images"));

    // Split-pane arrangement — only enabled when in split mode (the actions
    // are disabled/greyed in preview/edit by apply_mode_action_state).
    let split_section = Menu::new();
    split_section.append_item(&item("Swap Panes", "win.split-swap"));
    split_section.append_item(&item("Vertical Split", "win.split-orientation"));

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
    editor_section.append_item(&item("Annotate", "win.annotate"));
    // The annotation walk, beside the command that creates one. Menu items are how a
    // reader FINDS a keyboard command — a shortcut nobody can discover is reachable
    // only by the people who already knew — and the Action CAM owes every command a
    // menu-bar surface in any case.
    for (label, action) in [
        ("Next Annotation", "win.next-annotation"),
        ("Previous Annotation", "win.prev-annotation"),
    ] {
        editor_section.append_item(&item(label, action));
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
        menu.append_item(&make_format_item(c.label, c.target));
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
    // Save All — an "Uncommon command" (CAM.md), in its own section right after
    // the primary Save/Save As group. Built ad-hoc rather than as a `FILE_CMDS`
    // row for the same reason Export is: a row auto-generates a toolbar button,
    // and this one sat confusingly close to Save/Save As there (operator,
    // 2026-09-02). One `win.save-all` action drives this item and the
    // accelerator (`INLINE_ACCEL_CMDS`), so they cannot drift.
    let save_all_section = Menu::new();
    save_all_section.append_item(&item("Save All", "win.save-all"));
    file_menu.insert_section(1, None, &save_all_section);
    // File ▸ Export ▸ { PDF, HTML }. Built ad-hoc rather than as a `FILE_CMDS` row for
    // the same reason Rename is: a row auto-generates a toolbar button, and Export has
    // none. That is a decision, not an omission — the file toolbar's button section is
    // already crowded, and export is peripheral to this application's primary audience,
    // developers who review agent-written prose here and act on it in their own tools.
    // One `win.export` action drives both items, so adding a surface later is a
    // menu-model change rather than new plumbing.
    let export_menu = Menu::new();
    for target in [ExportTarget::Pdf, ExportTarget::Html] {
        let item = MenuItem::new(Some(&mnem(target.label())), None);
        item.set_action_and_target_value(Some("win.export"), Some(&target.target().to_variant()));
        export_menu.append_item(&item);
    }
    let export_section = Menu::new();
    export_section.append_submenu(Some(&mnem("Export")), &export_menu);
    // Before the trailing Exit section, beside Rename and Close Tab.
    file_menu.insert_section((file_menu.n_items() - 1).max(0), None, &export_section);
    let close_tab_section = Menu::new();
    // Rename sits with Close Tab and, like it, is built ad-hoc rather than as a
    // `FILE_CMDS` row: a row auto-generates a toolbar button, and Rename has none
    // (a granted CAM deviation — see CAM.md § Granted CAM exceptions).
    close_tab_section.append_item(&item("Rename…", "win.rename"));
    close_tab_section.append_item(&item("Close Tab", "win.close-tab"));
    file_menu.insert_section((file_menu.n_items() - 1).max(0), None, &close_tab_section);
    file_menu
}

/// Help menu — Keyboard Shortcuts, Markdown Reference, and About in its own
/// trailing section.
fn build_help_menu() -> Menu {
    let help_menu = Menu::new();
    // Keyboard Shortcuts opens the GtkShortcutsWindow via the per-window
    // win.show-help-overlay action that `set_help_overlay` installs (TDD 16.1).
    help_menu.append_item(&item("Keyboard Shortcuts", "win.show-help-overlay"));
    // Markdown Reference — opens the CommonMark syntax reference in the browser
    // (app.markdown-help). CommonMark because that's the syntax the app renders.
    help_menu.append(Some(&mnem("Markdown Reference")), Some("app.markdown-help"));
    // About sits alone in a trailing section (HIG: separated from the help items).
    let about_section = Menu::new();
    about_section.append(Some(&mnem("About")), Some("app.about"));
    help_menu.append_section(None, &about_section);
    help_menu
}

/// The bar's top-level menus, plus the two submenu handles whose CONTENT is
/// mutated at runtime.
pub(crate) struct TopLevelMenus {
    /// Each top-level menu paired with its `_`-marked bar title, in bar order.
    /// Titles carry the Alt+&lt;letter&gt; mnemonics directly (Alt+F/E/R/V/H).
    pub menus: Vec<(&'static str, Menu)>,
    pub documents_menu: Menu,
    pub format_insert_menu: Menu,
}

/// Build the top-level menus. This is the SINGLE enumeration of the menubar's
/// structure: [`build_menubar`] assembles the shipped model from it, and the
/// mnemonics guards in [`super::mnemonics`] derive their per-popover access-key
/// namespaces from it. Deriving matters — the guard used to compare a
/// hand-maintained mirror of these menus, which silently stopped matching and
/// left eight live entries (one of them half of a real access-key collision)
/// checked by nothing.
///
/// Pure `gio::Menu` models, no widgets, so a caller can build them headlessly.
pub(crate) fn build_top_level_menus() -> TopLevelMenus {
    build_top_level_menus_with(&crate::theme::themes())
}

/// As [`build_top_level_menus`], with the theme set supplied rather than read from disk.
///
/// The guard seam. `mnemonics` derives its access-key namespaces from these models, and a
/// guard whose INPUT varies with the host's installed themes cannot state what it checked:
/// the same source passes on one machine and fails on another, and the difference is a
/// config directory. Pass `Themes::builtin()` there and the assertion is about the program.
pub(crate) fn build_top_level_menus_with(themes: &crate::theme::Themes) -> TopLevelMenus {
    let (view_menu, documents_menu) = build_view_menu(themes);
    let (format_menu, format_insert_menu) = build_format_menu();
    TopLevelMenus {
        menus: vec![
            ("_File", build_file_menu()),
            ("_Edit", build_edit_menu()),
            ("Fo_rmat", format_menu),
            ("_View", view_menu),
            ("_Help", build_help_menu()),
        ],
        documents_menu,
        format_insert_menu,
    }
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
    let TopLevelMenus {
        menus,
        documents_menu,
        format_insert_menu,
    } = build_top_level_menus();

    let menubar = Menu::new();
    for (title, menu) in &menus {
        menubar.append_submenu(Some(title), menu);
    }

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
    use crate::app::commands::INLINE_ACCEL_CMDS;
    use crate::app::setup::accelerator_bindings_for;
    use std::collections::BTreeSet;

    /// Every `action` a shipped menubar item drives, sections and submenus flattened.
    /// Derived from [`build_top_level_menus_with`] — the model `build_menubar` actually
    /// ships — never from a mirror of it, for the reason
    /// `mnemonics::menu_access_keys_unique_per_popover` records: a guard whose input is
    /// a second copy of its subject reports on the copy.
    ///
    /// Builtin themes, not the installed set, so the walk does not vary with the host's
    /// `~/.config`. Items driving a parameterised action carry it as
    /// `action` + `target`, which is spelled `action::target` everywhere else in the
    /// toolchain (`set_accels_for_action`, the muxer's key), so it is re-joined here.
    fn menu_actions() -> Vec<String> {
        let menus = build_top_level_menus_with(&crate::theme::Themes::builtin()).menus;
        let mut out = Vec::new();
        let mut stack: Vec<gtk::gio::MenuModel> = menus
            .iter()
            .map(|(_, m)| m.clone().upcast::<gtk::gio::MenuModel>())
            .collect();
        while let Some(m) = stack.pop() {
            for i in 0..m.n_items() {
                for link in ["section", "submenu"] {
                    if let Some(child) = m.item_link(i, link) {
                        stack.push(child);
                    }
                }
                let Some(action) = m
                    .item_attribute_value(i, "action", None)
                    .and_then(|v| v.str().map(str::to_string))
                else {
                    continue;
                };
                let target = m
                    .item_attribute_value(i, "target", None)
                    .and_then(|v| v.str().map(str::to_string));
                out.push(match target {
                    Some(t) => format!("{action}::{t}"),
                    None => action,
                });
            }
        }
        out
    }

    /// Every menu item whose command has a declared accelerator has that accelerator
    /// **registered**, as the first one for its action.
    ///
    /// This is the assertion the menubar earns by NOT setting an `accel` attribute. GTK
    /// draws each hint from `gtk_action_muxer_get_primary_accel`, which is `accels[0]`
    /// as handed to `set_accels_for_action` — so registration is now the sole source of
    /// every hint on every platform, and an action the muxer cannot resolve is the one
    /// state that produces a genuinely hintless item. Nothing else in the toolchain asks
    /// this question: the binding works whether or not the command has a menu entry, and
    /// the menu entry works whether or not the command has a binding.
    ///
    /// `accelerator_bindings_for(Other)` rather than the host's, so the check is about
    /// the program and not about the machine running it.
    #[test]
    fn every_menu_command_with_a_shortcut_is_registered() {
        let bindings = accelerator_bindings_for(crate::accel::Platform::Other);
        // First-registered wins: `register_accelerators` groups by action preserving
        // first-seen order, and GTK displays `accels[0]`.
        let first_accel = |action: &str| -> Option<String> {
            bindings
                .iter()
                .find(|(a, _)| a == action)
                .map(|(_, accel)| accel.clone())
        };
        // What each command DECLARES, from the same tables the bindings are built from.
        let mut declared: Vec<(String, &str)> = Vec::new();
        for cmd in FILE_CMDS.iter().chain(EDIT_CMDS.iter()) {
            if !cmd.accel.is_empty() {
                declared.push((cmd.action.to_string(), cmd.accel));
            }
        }
        for cmd in VIEW_CMDS.iter().filter(|c| !c.accel.is_empty()) {
            declared.push((format!("win.view-mode::{}", cmd.action_target), cmd.accel));
        }
        for cmd in FORMAT_CMDS.iter().filter(|c| !c.accel.is_empty()) {
            declared.push((format!("win.format::{}", cmd.target), cmd.accel));
        }
        for cmd in INLINE_ACCEL_CMDS {
            declared.push((cmd.action.to_string(), cmd.accels[0]));
        }

        let in_menu: BTreeSet<String> = menu_actions().into_iter().collect();
        // ScrAP-132 guard-against-the-guard: a walk that descends wrongly scans nothing
        // and passes forever. Pin one item per construction route — a flat Cmd-table
        // row, a parameterised radio item, a parameterised Format item behind a submenu
        // link, and an inline command — before trusting the sweep.
        for action in [
            "win.save",
            "win.view-mode::preview",
            "win.format::bold",
            "win.previous-tab",
        ] {
            assert!(
                in_menu.contains(action),
                "the walk did not reach {action:?} — it found {} actions",
                in_menu.len()
            );
        }

        let mut problems: Vec<String> = Vec::new();
        for (action, accel) in &declared {
            if !in_menu.contains(action) {
                continue;
            }
            match first_accel(action) {
                Some(registered) if registered == *accel => {}
                Some(registered) => problems.push(format!(
                    "{action}: menu hint will show {registered:?} (first registered), \
                     but the command declares {accel:?}"
                )),
                None => problems.push(format!(
                    "{action}: declares {accel:?} but nothing registers it — its menu \
                     item will show NO key hint, on every platform"
                )),
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    /// No menubar item carries an `accel` attribute.
    ///
    /// The attribute WINS over the registered accelerator where both exist, silently, so
    /// one re-introduced here would be a hint that can contradict the binding with
    /// nothing comparing them — the drift this file removed the mechanism to end. Stated
    /// as an assertion rather than a comment because the attribute is one method call
    /// away and reads like an improvement.
    #[test]
    fn no_menu_item_declares_its_own_accel_attribute() {
        let menus = build_top_level_menus_with(&crate::theme::Themes::builtin()).menus;
        let mut offenders: Vec<String> = Vec::new();
        let mut stack: Vec<gtk::gio::MenuModel> = menus
            .iter()
            .map(|(_, m)| m.clone().upcast::<gtk::gio::MenuModel>())
            .collect();
        while let Some(m) = stack.pop() {
            for i in 0..m.n_items() {
                for link in ["section", "submenu"] {
                    if let Some(child) = m.item_link(i, link) {
                        stack.push(child);
                    }
                }
                if let Some(accel) = m
                    .item_attribute_value(i, "accel", None)
                    .and_then(|v| v.str().map(str::to_string))
                {
                    let label = m
                        .item_attribute_value(i, "label", None)
                        .and_then(|v| v.str().map(str::to_string))
                        .unwrap_or_default();
                    offenders.push(format!("{label:?} sets accel={accel:?}"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "menubar items must take their hint from `set_accels_for_action`, not a \
             second copy of it: {}",
            offenders.join("; ")
        );
    }

    /// Every inline command reaches the menu bar at all — a keyboard shortcut with no
    /// menu entry is one nobody discovers, and the Action CAM owes every command a
    /// menu-bar surface.
    #[test]
    fn every_inline_command_has_a_menu_item() {
        let present: BTreeSet<String> = menu_actions().into_iter().collect();
        let missing: Vec<&str> = INLINE_ACCEL_CMDS
            .iter()
            .map(|c| c.action)
            .filter(|a| !present.contains(*a))
            .collect();
        assert!(
            missing.is_empty(),
            "inline commands with no menu-bar item: {missing:?}"
        );
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod live_menu_tests {
    use super::*;

    /// The Format insert-section relabel is DEFERRED, not applied in the caller's turn.
    ///
    /// The property GTK4Rs/AP-76 is about, and the one this file had for
    /// `View ▸ Documents` and not for its sibling: a `GMenu` bound to a live
    /// `GtkPopoverMenuBar` must not be mutated from inside a signal dispatch, and all
    /// three callers of `update_format_menu_labels` are signal handlers. Asserted as a
    /// TIMING property — the menu is unchanged immediately after the call and changed
    /// after the main loop runs — because "it eventually shows the right label" is true
    /// of the unsafe synchronous version too and cannot tell the two apart.
    #[gtktest::test]
    fn the_format_relabel_lands_on_idle_and_not_in_the_callers_turn() {
        let app =
            crate::window::testkit::test_app("com.extollit.scribobulate.integrationtest.menudefer");
        let window = crate::window::new_window(&app, "IT", "# doc\n", None);
        let chrome = crate::winstate::chrome(&window).expect("chrome");
        let label_of = |idx: i32| {
            chrome
                .format_insert_menu
                .item_attribute_value(idx, "label", None)
                .and_then(|v| v.str().map(str::to_string))
                .unwrap_or_default()
        };
        let ctx = glib::MainContext::default();
        // Settle whatever the window's own construction queued, so the assertion below
        // is about THIS call and not about a rebuild already in flight.
        for _ in 0..50 {
            ctx.iteration(false);
        }
        let before = label_of(0);
        assert!(
            before.contains("Insert"),
            "expected the Link item to start in Insert form, got {before:?}"
        );

        update_format_menu_labels(&window, Some(FmtInsertKind::Link));
        assert_eq!(
            label_of(0),
            before,
            "the menu was mutated inside the caller's turn — this is the synchronous \
             mutation of a live-bound GMenu that GTK4Rs/AP-76 forbids"
        );

        for _ in 0..50 {
            ctx.iteration(false);
        }
        assert!(
            label_of(0).contains("Edit"),
            "the deferred relabel never landed; got {:?}",
            label_of(0)
        );
    }

    /// A request that returns to the displayed value inside one turn costs no mutation.
    ///
    /// Coalescing correctness, which is why requested and displayed kind are separate
    /// fields. Folding them into one — the shape before this change — makes an A→B→A
    /// toggle record B as applied while the menu still shows A.
    #[gtktest::test]
    fn a_there_and_back_relabel_in_one_turn_settles_on_what_is_shown() {
        let app = crate::window::testkit::test_app(
            "com.extollit.scribobulate.integrationtest.menucoalesce",
        );
        let window = crate::window::new_window(&app, "IT", "# doc\n", None);
        let chrome = crate::winstate::chrome(&window).expect("chrome");
        let ctx = glib::MainContext::default();
        for _ in 0..50 {
            ctx.iteration(false);
        }
        let shown = chrome
            .format_insert_menu
            .item_attribute_value(0, "label", None)
            .and_then(|v| v.str().map(str::to_string))
            .unwrap_or_default();

        update_format_menu_labels(&window, Some(FmtInsertKind::Link));
        update_format_menu_labels(&window, None);
        for _ in 0..50 {
            ctx.iteration(false);
        }

        let after = chrome
            .format_insert_menu
            .item_attribute_value(0, "label", None)
            .and_then(|v| v.str().map(str::to_string))
            .unwrap_or_default();
        assert_eq!(after, shown, "a there-and-back toggle changed the menu");
        assert_eq!(
            chrome.format_menu_kind.get(),
            None,
            "the DISPLAYED kind must record what the menu shows, not the last request"
        );
    }
}
