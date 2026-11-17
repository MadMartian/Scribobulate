//! Per-window menubar construction. The menubar MODEL is built per window
//! (GTK4Rs/AP-76) — because `View ▸ Documents` lists THAT window's tabs and
//! `Format ▸`'s insert section is relabeled per that window's selection — so the
//! whole `GtkPopoverMenuBar` is assembled here per window rather than once on the
//! GApplication. Only what differs by window is built per window; the menus no window
//! changes are built once and shared ([`SharedMenus`]).
//!
//! Built from `FILE_CMDS` / `EDIT_CMDS` / `VIEW_CMDS` / `FORMAT_CMDS` so labels stay in
//! sync with the context menu. Accelerator HINTS are not set here at all — GTK derives
//! each one from `set_accels_for_action`; see [`item`] for why the `accel` attribute
//! this file used to write was removed rather than tidied.

use super::commands::{
    toolbar_toggles, Cmd, FormatGroup, ToolbarToggleGroup, EDIT_CMDS, FILE_CMDS, FORMAT_CMDS,
    VIEW_CMDS,
};
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
/// **At `HIGH_IDLE`, not the default idle priority.** What makes the deferral safe is
/// running at main-loop top level, and any idle priority gives that. The default one (200)
/// also ranks below a `GtkTextView`'s background layout (125), which stays ready until the
/// whole buffer is laid out — so opening a large document left the Documents list and combo
/// naming the previous tab for close to a second (gtk4-rs skill T-5). A deferral that can
/// wait behind other work is not a brief one (CAM Derived-view, row 4).
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
/// (GTK4Rs/AP-63 / GTK4Rs/AP-128).
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
    glib::idle_add_local_full(
        glib::Priority::HIGH_IDLE,
        glib::clone!(
            #[weak(rename_to = w)]
            window,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move || {
                if let Some(chrome) = crate::winstate::chrome(&w) {
                    scheduled(&chrome).set(false);
                    mutate(&w, &chrome);
                }
                glib::ControlFlow::Break
            }
        ),
    );
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

/// The View ▸ Toolbar submenu: `Show`, a separator, then the six section toggles —
/// built by walking [`super::commands::toolbar_toggles`], the same list the toolbar's
/// right-click menu (`window::toolbarmenu`) walks, so the two surfaces offer the same
/// toggles bound to the same `app.` actions (TDD 9.40).
fn build_toolbar_layout_menu() -> Menu {
    let toolbar_menu = Menu::new();
    let mut section: Option<(ToolbarToggleGroup, Menu)> = None;
    for toggle in toolbar_toggles() {
        if section.as_ref().map(|(group, _)| *group) != Some(toggle.group) {
            if let Some((_, done)) = section.take() {
                toolbar_menu.append_section(None, &done);
            }
            section = Some((toggle.group, Menu::new()));
        }
        if let Some((_, menu)) = &section {
            menu.append_item(&item(&toggle.label, &toggle.detailed_action()));
        }
    }
    if let Some((_, done)) = section {
        toolbar_menu.append_section(None, &done);
    }
    toolbar_menu
}

/// The View menu's sections that hold nothing of one window's: built once per application
/// (see [`SharedMenus`]), the head above the tab-navigation section and the tail below it.
struct ViewSections {
    head: Vec<Menu>,
    tail: Vec<Menu>,
}

fn build_view_sections(themes: &crate::theme::Themes) -> ViewSections {
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
    // (Tab navigation + the Documents submenu sit between `tabs_section` and the outline
    // section; they are per-window and built by [`assemble_view_menu`].)

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
    // The two sidebar filters, beside the panes they filter (Shift+F9 / Shift+F8).
    outline_section.append_item(&item("Jump to Heading…", "win.filter-outline"));
    outline_section.append_item(&item("Jump to Comment…", "win.filter-annotations"));
    outline_section.append_item(&item("Go To Line…", "win.go-to-line"));

    // View-chrome visibility toggles — boolean `win.show-*` actions render as
    // checkbox items; their own section keeps them apart from the panel toggle.
    let chrome_section = Menu::new();
    // Toolbar is a SUBMENU (the per-section `show-tbtn-<id>` toggles):
    //   View ▸ Toolbar ▸ ┌ Show            (app.show-toolbar — the whole bar)
    //                    ├──────────────
    //                    │ File / Edit / Format / View / Split / Zoom
    //                    └ (app.show-tbtn-<id>, canonical order)
    // The first section is the whole-bar "Show" toggle; the second holds the six
    // per-section checkbox items. GTK greys the six whenever "Show" is off — their
    // actions are disabled by `window::toolbarchrome` (I3), which leaves their
    // ticks intact (I4), so re-showing the bar restores the exact per-section
    // configuration.
    //
    // `app.`, not `win.`: the toolbar layout is one app-wide preference (a window
    // is ephemeral, so a layout scoped to one never feels saved), and nothing about
    // it is per-window — not even sensitivity, which is the only reason the split
    // arrangement below still keeps `win.` forwarders.
    let toolbar_menu = build_toolbar_layout_menu();
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
    // separate from the chrome visibility toggles above. Play Animations joins
    // it (operator decision, TDD 27.5) — a second opt-out sharing the same axis.
    let unsafe_images_section = Menu::new();
    unsafe_images_section.append_item(&item("Show Unsafe Images", "win.show-unsafe-images"));
    // Play Animations — process-wide (`app.`, not `win.`, so every window's tick
    // agrees; `app::appactions::add_play_animations_action`), beside Show Unsafe
    // Images per the operator's design (both are content-safety-adjacent opt-outs).
    // Deliberately no toolbar button and no accelerator (TDD 27.5) — not wired
    // through a `Cmd`/`FmtCmd` descriptor, which is what would otherwise draw a
    // toolbar button and register a shortcut for it.
    unsafe_images_section.append_item(&item("Play Animations", "app.play-animations"));

    // Split-pane arrangement — only enabled when in split mode (the actions
    // are disabled/greyed in preview/edit by apply_mode_action_state).
    let split_section = Menu::new();
    split_section.append_item(&item("Swap Panes", "win.split-swap"));
    split_section.append_item(&item("Vertical Split", "win.split-orientation"));

    ViewSections {
        head: vec![nav_section, section, tabs_section],
        tail: vec![
            outline_section,
            chrome_section,
            unsafe_images_section,
            zoom_section,
            split_section,
        ],
    }
}

/// One window's View menu: the shared sections around this window's own tab-navigation
/// section. Returns the menu AND a handle to its (initially empty) Documents submenu.
///
/// Tab navigation + the Documents submenu (the fast-switch list of THIS window's open
/// tabs, one `win.select-tab::<id>` radio item each). The submenu starts empty — its
/// content is filled/refreshed per window by `window/tabs/refresh_documents_menu`
/// (deferred to idle: mutating a GMenu bound to a live menubar mid-activation is unsafe,
/// GTK4Rs/AP-76). It groups naturally beside Previous/Next Tab.
fn assemble_view_menu(parts: &ViewSections) -> (Menu, Menu) {
    let tab_nav_section = Menu::new();
    tab_nav_section.append_item(&item("Previous Tab", "win.previous-tab"));
    tab_nav_section.append_item(&item("Next Tab", "win.next-tab"));
    let documents_menu = Menu::new();
    tab_nav_section.append_submenu(Some(&mnem("Documents")), &documents_menu);

    let outer = Menu::new();
    for section in &parts.head {
        outer.append_section(None, section);
    }
    outer.append_section(None, &tab_nav_section);
    for section in &parts.tail {
        outer.append_section(None, section);
    }
    (outer, documents_menu)
}

/// Edit menu: the EDIT_CMDS commands and an editor section mirroring the GtkSourceView
/// context menu — Insert Emoji and a Change Case submenu. All menu-only (not in
/// EDIT_CMDS, so no toolbar buttons). The find bar's option toggles have no menu item:
/// they qualify a search that exists only while the bar is open, so the bar is their
/// one surface (operator, 2026-10-24).
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
    // A submenu row has no action of its own, so GTK draws it enabled whatever its items
    // say. Naming `win.change-case-available` (the no-parameter twin of `win.change-case`,
    // which cannot back a row that has no string target) makes the tracker grey the row
    // with its items, on the menu bar and on macOS's exported menu alike. No `hidden-when`:
    // the row greys like its siblings and is never removed.
    let change_case_row = MenuItem::new_submenu(Some(&mnem("Change Case")), &change_case);
    change_case_row.set_attribute_value("action", Some(&"win.change-case-available".to_variant()));
    editor_section.append_item(&change_case_row);
    edit_menu.append_section(None, &editor_section);
    edit_menu
}

/// Format menu — all items target the parameterised win.format action.
/// Order: Bold, Italic, Heading ▸ (1–6 submenu), Strikethrough, Code Span,
/// Superscript, Subscript, Code Block, Quote, Bulleted List, Numbered List,
/// Horizontal Bar — then, after a separator (a second section), the Tier-2
/// insertions Link / Image / Table.
/// Heading is a submenu here (a combo box in the toolbar); both drive
/// win.format::h{1..6}. This is the part of the Format menu that is the same in every
/// window; the insert section below it is per-window (see [`assemble_format_menu`]).
fn build_format_inline_section() -> Menu {
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

    // The rest of the text styles, then the block commands, in table order, from each
    // command's own `group` (the Keyboard Shortcuts window reads the same field).
    for c in FORMAT_CMDS
        .iter()
        .filter(|c| c.group != FormatGroup::Insert && !matches!(c.target, "bold" | "italic"))
    {
        append(&inline, c.target);
    }

    inline
}

/// One window's Format menu: the shared styles/blocks section above this window's own
/// insert section, which `update_format_menu_labels` relabels Insert↔Edit.
fn assemble_format_menu(inline: &Menu) -> (Menu, Menu) {
    // Insertions go in their own section so GTK draws a separator above them.
    let insert = Menu::new();
    for c in FORMAT_CMDS
        .iter()
        .filter(|c| c.group == FormatGroup::Insert)
    {
        insert.append_item(&make_format_item(c.label, c.target));
    }

    let outer = Menu::new();
    outer.append_section(None, inline);
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

/// The menus, and parts of menus, that no window changes: built ONCE per application and
/// shared by every window's model, then frozen so a mutation through any handle is
/// refused rather than seen by all windows.
///
/// **Why they are shared.** GTK 4.6.9's `GtkPopoverMenuBar` takes a transfer-full
/// reference to every submenu model it links and never drops it (`gtkpopovermenubar.c`
/// 441-442; GTK4Rs/AP-387), so a submenu model built per window is never freed once the
/// window closes — about 50 KB per window, for every File/Edit/View/… menu and everything
/// they link. That reference cannot be released from here (emptying the models at destroy
/// is the mutation of a live-bound `GMenu` GTK4Rs/AP-76 forbids). Sharing makes the
/// leaked reference pin ONE model for the process instead of one more per window. What a
/// window does change — `View ▸ Documents`, `Format ▸`'s insert section — stays per-window
/// and small (their parents are built per window too), so each window still leaks only
/// those few items.
///
/// Sharing is sound because nothing here varies with the window or with time: the
/// themes are loaded once per process (`crate::theme::themes`), every item binds an
/// action by name, and the accelerator hints come from the application's registered
/// accelerators, not from the model.
struct SharedMenus {
    file: Menu,
    edit: Menu,
    help: Menu,
    view: ViewSections,
    format_inline: Menu,
}

impl SharedMenus {
    fn build(themes: &crate::theme::Themes) -> Self {
        let shared = Self {
            file: build_file_menu(),
            edit: build_edit_menu(),
            help: build_help_menu(),
            view: build_view_sections(themes),
            format_inline: build_format_inline_section(),
        };
        let every_menu = [
            &shared.file,
            &shared.edit,
            &shared.help,
            &shared.format_inline,
        ]
        .into_iter()
        .chain(shared.view.head.iter().chain(shared.view.tail.iter()));
        for menu in every_menu {
            menu.freeze();
        }
        shared
    }

    /// One window's top-level menus: the shared ones, and this window's own View and
    /// Format around their per-window sections.
    fn assemble(&self) -> TopLevelMenus {
        let (view_menu, documents_menu) = assemble_view_menu(&self.view);
        let (format_menu, format_insert_menu) = assemble_format_menu(&self.format_inline);
        TopLevelMenus {
            menus: vec![
                ("_File", self.file.clone()),
                ("_Edit", self.edit.clone()),
                ("Fo_rmat", format_menu),
                ("_View", view_menu),
                ("_Help", self.help.clone()),
            ],
            documents_menu,
            format_insert_menu,
        }
    }
}

thread_local! {
    /// The application's [`SharedMenus`], built from the installed themes on first use.
    static SHARED_MENUS: std::cell::OnceCell<SharedMenus> = const { std::cell::OnceCell::new() };
}

/// Build the top-level menus. This is the SINGLE enumeration of the menubar's
/// structure: [`build_menubar`] assembles the shipped model from the same
/// [`SharedMenus::assemble`], and the mnemonics guards in [`super::mnemonics`] derive
/// their per-popover access-key namespaces from it. Deriving matters — the guard used to
/// compare a hand-maintained mirror of these menus, which silently stopped matching and
/// left eight live entries (one of them half of a real access-key collision)
/// checked by nothing.
///
/// Pure `gio::Menu` models, no widgets, so a caller can build them headlessly. The
/// application-wide menus are shared between calls (see [`SharedMenus`]).
pub(crate) fn build_top_level_menus() -> TopLevelMenus {
    SHARED_MENUS.with(|shared| {
        shared
            .get_or_init(|| SharedMenus::build(&crate::theme::themes()))
            .assemble()
    })
}

/// As [`build_top_level_menus`], with the theme set supplied rather than read from disk,
/// and nothing shared: a fresh model each call.
///
/// The guard seam. `mnemonics` derives its access-key namespaces from these models, and a
/// guard whose INPUT varies with the host's installed themes cannot state what it checked:
/// the same source passes on one machine and fails on another, and the difference is a
/// config directory. Pass `Themes::builtin()` there and the assertion is about the program.
#[cfg(test)]
pub(crate) fn build_top_level_menus_with(themes: &crate::theme::Themes) -> TopLevelMenus {
    SharedMenus::build(themes).assemble()
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
        // GEP-1 guard-against-the-guard: a walk that descends wrongly scans nothing
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

    /// The shipped Edit menu offers Cut, Copy and Paste (TDD 9.37) — walked off the
    /// model itself, so a Paste that leaves the menu by any route fails here.
    #[test]
    fn the_menu_bar_offers_cut_copy_and_paste() {
        let present: BTreeSet<String> = menu_actions().into_iter().collect();
        for action in ["win.cut", "win.copy", "win.paste"] {
            assert!(present.contains(action), "no menu-bar item drives {action}");
        }
    }

    /// TDD 6.13: two windows' menu models share every submenu no window changes, and own
    /// the ones a window does. GTK 4.6.9's menu bar never drops its reference to a submenu
    /// model (GTK4Rs/AP-387), so what is built per window is kept for good: this is what
    /// makes that leaked reference pin one shared model instead of one per window.
    /// Mutation: build the shared menus per call and the identity assertions fail; share a
    /// per-window menu and the inequality ones do. A `GMenu` is plain data, so no display.
    #[test]
    fn windows_share_the_menus_none_of_them_changes_and_own_the_rest() {
        let (first, second) = (build_top_level_menus(), build_top_level_menus());
        let menu = |menus: &TopLevelMenus, title: &str| -> Menu {
            menus
                .menus
                .iter()
                .find_map(|(t, m)| (*t == title).then(|| m.clone()))
                .unwrap_or_else(|| panic!("no {title} menu"))
        };
        for title in ["_File", "_Edit", "_Help"] {
            assert_eq!(
                menu(&first, title),
                menu(&second, title),
                "{title} is built per window"
            );
            assert!(
                !menu(&first, title).is_mutable(),
                "the shared {title} menu can be mutated through one window's handle"
            );
        }
        for title in ["Fo_rmat", "_View"] {
            assert_ne!(
                menu(&first, title),
                menu(&second, title),
                "{title} holds a per-window section and cannot be shared whole"
            );
        }
        assert_ne!(first.documents_menu, second.documents_menu);
        assert_ne!(first.format_insert_menu, second.format_insert_menu);
        assert!(first.documents_menu.is_mutable() && first.format_insert_menu.is_mutable());

        // Inside the two per-window menus, everything but the per-window section is shared.
        let section = |menu: &Menu, i: i32| menu.item_link(i, "section").expect("a section");
        assert_eq!(
            section(&menu(&first, "Fo_rmat"), 0),
            section(&menu(&second, "Fo_rmat"), 0),
            "Format's styles and blocks are built per window"
        );
        assert_eq!(
            section(&menu(&first, "_View"), 0),
            section(&menu(&second, "_View"), 0),
            "View's leading sections are built per window"
        );
    }

    /// The find bar's option toggles are reached only from the bar (TDD 11.13): a menu
    /// item offered them while the bar was closed, where they mean nothing. Mutation:
    /// put any one of them back in a menu and this fails.
    #[test]
    fn no_menu_item_drives_a_find_option() {
        let present: BTreeSet<String> = menu_actions().into_iter().collect();
        for action in [
            "win.find-match-case",
            "win.find-whole-word",
            "win.find-regex",
            "win.find-in-selection",
        ] {
            assert!(!present.contains(action), "a menu-bar item drives {action}");
        }
    }

    /// TDD 27.5: Play Animations sits beside Show Unsafe Images (same section) and
    /// targets `app.play-animations` — walked off the model itself, not a mirror of
    /// it, for the same reason [`menu_actions`] is (a guard whose input is a copy of
    /// its subject reports on the copy). No `#[gtktest::test]` needed: a `GMenu`
    /// model is a plain data structure, not a live display object — the same
    /// reasoning `every_menu_command_with_a_shortcut_is_registered` and
    /// `no_menu_item_declares_its_own_accel_attribute` above already rely on.
    #[test]
    fn play_animations_shares_the_show_unsafe_images_section() {
        let view_menu = build_top_level_menus_with(&crate::theme::Themes::builtin())
            .menus
            .into_iter()
            .find_map(|(title, menu)| (title == "_View").then_some(menu))
            .expect("the View menu");

        let label_at = |section: &Menu, i: i32| -> Option<String> {
            section
                .item_attribute_value(i, "label", None)
                .and_then(|v| v.str().map(str::to_string))
        };

        // Find the section holding "Show Unsafe Images" (mnemonic-marked, as the
        // real menu renders it).
        let mut target_section: Option<Menu> = None;
        for i in 0..view_menu.n_items() {
            let Some(section) = view_menu.item_link(i, "section") else {
                continue;
            };
            let section: Menu = section
                .downcast::<Menu>()
                .expect("a section link is a Menu");
            if (0..section.n_items())
                .any(|j| label_at(&section, j).as_deref() == Some("Show _Unsafe Images"))
            {
                target_section = Some(section);
                break;
            }
        }
        let section = target_section.expect("Show Unsafe Images section not found in View menu");

        let play_index = (0..section.n_items())
            .find(|&j| label_at(&section, j).as_deref() == Some("Pla_y Animations"))
            .expect("Play Animations must be in the SAME section as Show Unsafe Images");

        assert_eq!(
            section
                .item_attribute_value(play_index, "action", None)
                .and_then(|v| v.str().map(str::to_string))
                .as_deref(),
            Some("app.play-animations"),
            "Play Animations must target the app-wide app.play-animations action"
        );
        assert!(
            section.item_link(play_index, "submenu").is_none(),
            "Play Animations is a direct item, not a nested submenu"
        );
    }

    /// The Change Case submenu row names an action of its own, so it can grey.
    ///
    /// A `GMenu` submenu row has no action, and GTK draws such a row enabled whatever its
    /// items say, so Edit ▸ Change Case sat enabled over four greyed items in the preview.
    /// The row now carries `win.change-case-available`, a no-parameter twin of
    /// `win.change-case` (which cannot back the row: it takes a string target the row has
    /// none of, so GTK would read it as never activatable). Walked off the shipped model,
    /// never a mirror of it. `hidden-when` is deliberately absent: the row greys like its
    /// siblings and is never removed.
    #[test]
    fn the_change_case_row_names_a_no_parameter_action_and_never_hides() {
        let edit = build_top_level_menus_with(&crate::theme::Themes::builtin())
            .menus
            .into_iter()
            .find_map(|(title, menu)| (title == "_Edit").then_some(menu))
            .expect("the Edit menu");
        let mut row = None;
        for i in 0..edit.n_items() {
            let Some(section) = edit.item_link(i, "section") else {
                continue;
            };
            let section: Menu = section.downcast().expect("a section link is a Menu");
            for j in 0..section.n_items() {
                if section.item_link(j, "submenu").is_some()
                    && section
                        .item_attribute_value(j, "label", None)
                        .and_then(|v| v.str().map(str::to_string))
                        .as_deref()
                        == Some("Chan_ge Case")
                {
                    row = Some((section.clone(), j));
                }
            }
        }
        let (section, j) = row.expect("Edit has a Change Case submenu row");
        let attr = |name: &str| {
            section
                .item_attribute_value(j, name, None)
                .and_then(|v| v.str().map(str::to_string))
        };
        assert_eq!(
            attr("action").as_deref(),
            Some("win.change-case-available"),
            "the Change Case row must name an action, or GTK draws it enabled over greyed items"
        );
        assert_eq!(
            attr("hidden-when"),
            None,
            "the row greys; it is never hidden"
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

    /// Every `modelbutton` under `root`: `(its text, whether it is sensitive)`.
    fn model_buttons(root: &gtk::Widget) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(w) = stack.pop() {
            if w.css_name() == "modelbutton" {
                let text: Option<String> = w.property("text");
                out.push((text.unwrap_or_default(), w.is_sensitive()));
            }
            let mut c = w.first_child();
            while let Some(child) = c {
                c = child.next_sibling();
                stack.push(child);
            }
        }
        out
    }

    /// **Edit ▸ Change Case draws as unavailable exactly when its four items are.**
    ///
    /// A widget-level check on the shipped Edit model, opened as a `GtkPopoverMenu`
    /// on a real window so the row resolves `win.` actions through the window's own
    /// action muxer. (The bar's own popovers are built lazily per item, so the model is
    /// opened directly; the row's sensitivity is the tracker's, which is the same code on
    /// the bar and on macOS's exported menu.) Preview: no selection, row and items both
    /// unavailable. Edit with a selection: both available. The first half is the bug.
    ///
    /// Mutation check: dropping the row's `action` attribute leaves the preview row
    /// sensitive and fails the first assertion.
    #[gtktest::test]
    fn the_change_case_row_follows_its_items() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = crate::window::testkit::test_app(
                "com.extollit.scribobulate.integrationtest.changecaserow",
            );
            let window = crate::window::new_window(&app, "IT", "# doc\n\nsome text\n", None);
            window.present();
            let edit = build_top_level_menus_with(&crate::theme::Themes::builtin())
                .menus
                .into_iter()
                .find_map(|(title, menu)| (title == "_Edit").then_some(menu))
                .expect("the Edit menu");
            // Anchored on the window's own content, never swapped in for it: the tabs'
            // selection handlers resolve their window through that tree.
            let host = window.child().expect("the window has content");
            let popover = gtk::PopoverMenu::from_model_full(&edit, gtk::PopoverMenuFlags::NESTED);
            popover.set_parent(&host);
            popover.popup();
            let ctx = glib::MainContext::default();
            let settle = || {
                for _ in 0..50 {
                    while ctx.iteration(false) {}
                }
            };
            settle();
            let row_and_items = || {
                let buttons = model_buttons(popover.upcast_ref());
                let find = |needle: &str| {
                    buttons
                        .iter()
                        .find(|(t, _)| t.contains(needle))
                        .unwrap_or_else(|| panic!("no menu row {needle:?} in {buttons:?}"))
                        .1
                };
                (find("Change Case"), find("UPPER CASE"))
            };

            let (row, item) = row_and_items();
            assert!(
                !item,
                "precondition: UPPER CASE is unavailable in the preview"
            );
            assert!(
                !row,
                "Change Case draws enabled while all four of its items are unavailable"
            );
            let sensitive = |name: &str| {
                window
                    .lookup_action(name)
                    .unwrap_or_else(|| panic!("no action {name}"))
                    .is_enabled()
            };
            assert_eq!(
                sensitive("change-case-available"),
                sensitive("change-case"),
                "the row's action must carry the items' own enabled state"
            );

            crate::window::change_action_state(&window, "view-mode", &"edit".to_variant());
            let st = crate::winstate::state(&window).expect("state");
            st.editor_buf.select_range(
                &st.editor_buf.iter_at_offset(2),
                &st.editor_buf.iter_at_offset(5),
            );
            settle();
            let (row, item) = row_and_items();
            assert!(
                item,
                "precondition: UPPER CASE is available with a selection"
            );
            assert!(row, "Change Case must be available when its items are");
            assert_eq!(sensitive("change-case-available"), sensitive("change-case"));

            popover.popdown();
            settle();
            popover.unparent();
            window.destroy();
        });
    }
}
