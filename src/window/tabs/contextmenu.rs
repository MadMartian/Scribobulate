//! Per-tab close button (N1) and right-click context menu (N2) wiring — the
//! window-domain half of the tab handles `widgets/tab`'s `TabBar`
//! builds but has no domain knowledge to act on. Split out of the former
//! monolithic `window/tabs.rs`.

use super::super::*;
use super::*;
use crate::widgets::tab::TabView;

/// Wire the per-tab `×` close button (N1) and the per-tab
/// right-click context menu (N2) that `widgets/tab`'s `TabBar` builds
/// into every tab handle, but has no domain knowledge to act on itself.
pub(crate) fn wire_tab_close_and_menu(window: &ApplicationWindow, tab_view: &TabView) {
    tab_view.connect_tab_close_requested(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_tv, content| {
            if let Some(tab) = winstate::tab_by_content_box(content) {
                close_specific_tab(&w, tab);
            }
        }
    ));
    tab_view.connect_tab_context_menu(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |tv, content, x, y| {
            let Some(tab) = winstate::tab_by_content_box(content) else {
                return;
            };
            show_tab_context_menu(&w, tv, &tab, x, y);
        }
    ));
}

/// The tab context menu's items, in order — **the** enumeration of them.
///
/// It exists because the menubar's mnemonic guard was taught (in this same merge) to
/// derive from the menu it checks rather than from a hand-maintained mirror, and this
/// menu, which has the identical structure one level down, was left with THREE copies of
/// its label list: the marked label was spelled twice at each call site (once to build
/// the button, once to register its access key) and a third time as literals inside the
/// guard. The guard's copy was already stale — it listed five items against the six that
/// ship, so `Re_name…`'s access key was checked for collision against nothing at all.
///
/// A guard whose input set is a second copy of the thing it checks reports on the copy.
///
/// One manual step survives and is worth naming rather than pretending away: a new
/// variant must be added to [`Self::ALL`] as well. `label` is an exhaustive match, so
/// the variant cannot be added without deciding its text; `ALL` is what the guard walks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TabMenuItem {
    Save,
    SaveAs,
    Close,
    CloseOthers,
    MoveToNewWindow,
    CopyFullPath,
    CopyDocument,
    Reload,
    Rename,
}

impl TabMenuItem {
    /// Every item this menu ships, in the order it presents them.
    pub(crate) const ALL: [Self; 9] = [
        Self::Save,
        Self::SaveAs,
        Self::Close,
        Self::CloseOthers,
        Self::MoveToNewWindow,
        Self::CopyFullPath,
        Self::CopyDocument,
        Self::Reload,
        Self::Rename,
    ];

    /// Whether a separator follows this item, so the menu's grouping is data on the
    /// enumeration rather than an `append` buried between two call sites.
    pub(crate) fn separator_after(self) -> bool {
        matches!(
            self,
            Self::SaveAs | Self::CloseOthers | Self::MoveToNewWindow
        )
    }

    /// The `_`-marked label: the button's text and its access key, one string.
    ///
    /// "Save", "Save As…", "Close Tab", "Move to New Window" and "Reload" reuse their
    /// menu-bar marks so the letters match; "Copy Full Path" uses `F` to match the
    /// `win.copy-path` accelerator; `S`/`A`/`C`/`O`/`M`/`F`/`R` being taken is why
    /// Rename's is `n`. Copy Document's Edit-menu letter `m` belongs to Move to New
    /// Window here, so it takes `D`.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Save => "_Save",
            Self::SaveAs => "Save _As…",
            Self::Close => "_Close Tab",
            Self::CloseOthers => "Close _Other Tabs",
            Self::MoveToNewWindow => "_Move to New Window",
            Self::CopyFullPath => "Copy _Full Path",
            Self::CopyDocument => "Copy _Document",
            Self::Reload => "_Reload",
            Self::Rename => "Re_name…",
        }
    }
}

/// N2's `GMenu`/`PopoverMenu` was rejected in favor of the codebase's own
/// established context-menu idiom (a plain `GtkPopover` + `GtkButton` column,
/// `window/contextmenu.rs`) — a spurious-scrollbar issue with
/// `GtkPopoverMenu` on this GTK version is why that convention exists here in
/// the first place; a NEW context menu should not reintroduce it.
fn show_tab_context_menu(
    window: &ApplicationWindow,
    tab_view: &TabView,
    tab: &Rc<TabState>,
    x: f64,
    y: f64,
) {
    let menu = crate::window::contextmenu::ContextMenu::new();
    let popover = menu.popover.clone();

    // "Close Tab" reuses the File-menu mark so its `C` matches; the other two are
    // context-only but keep the menu-bar letters (Close Other = O, Move = M). One flat
    // page, so every access key has an always-true gate.
    //
    // Building the button and registering its access key are ONE call, taking the item
    // rather than a string. Two things follow, and both were live defects: the label is
    // spelled once instead of twice per item, and — because `add_key` was a silent no-op
    // when `access_markup` found no `_` — a label edited to drop its marker can no longer
    // lose its access key with neither a warning nor a failing test. The registration
    // itself is `ContextMenu::add_access_key`, shared with the other two menus.
    let make_btn = |item: TabMenuItem| -> gtk::Button {
        let lbl = gtk::Label::new(None);
        lbl.set_halign(gtk::Align::Start);
        let btn = gtk::Button::new();
        btn.set_child(Some(&lbl));
        btn.add_css_class("flat");
        btn.set_halign(gtk::Align::Fill);
        lbl.set_markup(&menu.add_access_key(&btn, item.label(), || true));
        btn
    };

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // THE MENU IS BUILT BY WALKING `TabMenuItem::ALL`, in its order. That is what makes
    // the enumeration load-bearing rather than decorative: an item missing from `ALL`
    // does not merely go unchecked by the mnemonics guard — it does not RENDER, which is
    // the one kind of drift that reports itself. Identity, label, access key and
    // grouping are shared; sensitivity and the click handler stay per-item.
    //
    // Every row holds the tab WEAKLY. A tab reaches its window's chrome, and the chrome
    // the tab bar this menu hangs off, so a strong capture is the cycle
    // `bar → menu → row → closure → tab → chrome → bar`. A menu closed normally is
    // unparented and breaks it; a window closed with the menu open never emits `closed`
    // on X11, so the bar, the menu and the whole chrome outlived the window (MEASURED,
    // `closing_the_window_with_the_tab_menu_open_releases_the_menu`; GTK4Rs/AP-63).
    let has_others = winstate::tabs_for_window(window)
        .iter()
        .any(|t| t.id != tab.id);
    let content = tab.content_box.clone();
    for item in TabMenuItem::ALL {
        let btn = make_btn(item);
        match item {
            // Save / Save As are window-scoped actions (`win.save` / `win.save-as`)
            // that always act on the ACTIVE tab — same two requirements as
            // Copy Full Path / Reload / Rename below: read sensitivity from the
            // CLICKED tab's own state, and `focus_page` it before driving the
            // action so a right-click on an inactive tab saves THAT tab.
            TabMenuItem::Save => {
                btn.set_sensitive(save_enabled(
                    tab.is_dirty(),
                    tab.backing_loss.get().is_some(),
                ));
                btn.connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = w)]
                    window,
                    #[weak]
                    tab,
                    move |_| {
                        dismiss_context_popover(&po);
                        save_for_tab(&w, &tab);
                    }
                ));
            }
            TabMenuItem::SaveAs => {
                // win.save-as carries no dirty/backing gate — it is always
                // reachable for the active tab, so this button is too.
                btn.connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = w)]
                    window,
                    #[weak]
                    tab,
                    move |_| {
                        dismiss_context_popover(&po);
                        save_as_for_tab(&w, &tab);
                    }
                ));
            }
            TabMenuItem::Close => {
                btn.connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = w)]
                    window,
                    #[weak]
                    tab,
                    move |_| {
                        dismiss_context_popover(&po);
                        close_specific_tab(&w, tab.clone());
                    }
                ));
            }
            TabMenuItem::CloseOthers => {
                btn.set_sensitive(has_others);
                btn.connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = w)]
                    window,
                    #[weak]
                    tab,
                    move |_| {
                        dismiss_context_popover(&po);
                        // Close the OTHER tabs, keeping this one. Clean tabs close at
                        // once; dirty tabs prompt sequentially, not N dialogs at once.
                        close_other_tabs(&w, tab.clone());
                    }
                ));
            }
            TabMenuItem::MoveToNewWindow => {
                // Single source of truth (POLICY): "Move to New Window" is one command
                // with three surfaces (menu bar, toolbar, this tab context menu), all
                // driven by the `win.move-tab-new-window` GAction. Read the action's own
                // enabled state rather than recomputing the `tab_count > 1` precondition
                // here — a future change to the action's gate (e.g. also disabling
                // mid-drag) then can't skip this surface. (M4)
                btn.set_sensitive(
                    simple_action(window, "move-tab-new-window")
                        .map(|a| a.is_enabled())
                        .unwrap_or(false),
                );
                btn.connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = w)]
                    window,
                    #[strong]
                    content,
                    move |_| {
                        dismiss_context_popover(&po);
                        let Some(chrome) = winstate::chrome(&w) else {
                            return;
                        };
                        // Reuse the active-tab path (this menu's tab need not be active).
                        chrome.tabs.focus_page(&content);
                        move_tab_to_new_window(&w);
                    }
                ));
            }
            // Copy Full Path / Reload / Rename are window-scoped actions
            // (`win.copy-path` / `win.reload` / `win.rename`) that always act on the
            // ACTIVE tab — but this menu fires for the RIGHT-CLICKED tab, which need not
            // be active. Two requirements follow:
            //
            // 1. Sensitivity must be read from the CLICKED tab's own predicate, the same
            //    one that feeds the actions on every switch (`window/tabs/switch.rs`) —
            //    NOT from `action.is_enabled()`. Unlike `MoveToNewWindow` above (M4,
            //    correct there because that action's gate is genuinely window-scoped),
            //    these are per-tab: the action's current `is_enabled()` reflects
            //    whichever tab is active RIGHT NOW, which is the wrong tab for a
            //    right-click on an inactive one.
            // 2. Activation must not just invoke the action — that would silently act on
            //    the active tab. Follow the Move-to-New-Window precedent: `focus_page`
            //    the clicked tab first (synchronously resyncing the actions' enabled
            //    state to match), then drive the action. Reload and Rename also prompt,
            //    so focusing first means the prompt names the document the user actually
            //    clicked (TDD 24.6/24.11).
            TabMenuItem::CopyFullPath => {
                btn.set_sensitive(tab.has_path());
                btn.connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = w)]
                    window,
                    #[weak]
                    tab,
                    move |_| {
                        dismiss_context_popover(&po);
                        copy_full_path_for_tab(&w, &tab);
                    }
                ));
            }
            // `win.copy-document` has no enable gate — every tab holds a document — so
            // the row is always sensitive; like Copy Full Path it focuses the clicked tab
            // first, because the action copies the active one.
            TabMenuItem::CopyDocument => {
                btn.connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = w)]
                    window,
                    #[weak]
                    tab,
                    move |_| {
                        dismiss_context_popover(&po);
                        copy_document_for_tab(&w, &tab);
                    }
                ));
            }
            TabMenuItem::Reload => {
                btn.set_sensitive(tab.has_path());
                btn.connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = w)]
                    window,
                    #[weak]
                    tab,
                    move |_| {
                        dismiss_context_popover(&po);
                        reload_for_tab(&w, &tab);
                    }
                ));
            }
            TabMenuItem::Rename => {
                btn.set_sensitive(rename_enabled_for(tab));
                btn.connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = w)]
                    window,
                    #[weak]
                    tab,
                    move |_| {
                        dismiss_context_popover(&po);
                        rename_for_tab(&w, &tab);
                    }
                ));
            }
        }
        box_.append(&btn);
        if item.separator_after() {
            box_.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
    }

    // Scrolls rather than self-dismissing where the screen cannot fit it (TDD 9.39).
    menu.open(&tab_view.bar_widget(), (x, y), &box_, |_| {});
}

/// Drive `win.save` for `tab` — same focus-first requirement as
/// [`copy_full_path_for_tab`] below: `win.save` always acts on the active tab,
/// so make `tab` active first.
fn save_for_tab(window: &ApplicationWindow, tab: &Rc<TabState>) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    chrome.tabs.focus_page(&tab.content_box);
    if let Some(action) = simple_action(window, "save") {
        action.activate(None);
    }
}

/// Drive `win.save-as` for `tab` — same focus-first requirement as
/// [`save_for_tab`]. Focusing first also means the Save As dialog's suggested
/// name/location is drawn from the document the user actually clicked.
fn save_as_for_tab(window: &ApplicationWindow, tab: &Rc<TabState>) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    chrome.tabs.focus_page(&tab.content_box);
    if let Some(action) = simple_action(window, "save-as") {
        action.activate(None);
    }
}

/// Drive `win.copy-path` for `tab`, which need not be the window's currently
/// active tab — the action always acts on whichever tab is
/// active, so make `tab` active first. `focus_page` resyncs the action's
/// enabled state synchronously (`window/tabs/switch.rs`) to `tab`'s own
/// `has_path()`, matching the button's own sensitivity gate above.
fn copy_full_path_for_tab(window: &ApplicationWindow, tab: &Rc<TabState>) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    chrome.tabs.focus_page(&tab.content_box);
    if let Some(action) = simple_action(window, "copy-path") {
        action.activate(None);
    }
}

/// Drive `win.copy-document` for `tab` — same focus-first requirement as
/// [`copy_full_path_for_tab`]: the action copies the active tab's source.
fn copy_document_for_tab(window: &ApplicationWindow, tab: &Rc<TabState>) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    chrome.tabs.focus_page(&tab.content_box);
    if let Some(action) = simple_action(window, "copy-document") {
        action.activate(None);
    }
}

/// Drive `win.reload` for `tab` — same focus-first requirement as
/// [`copy_full_path_for_tab`]. Focusing first is also correct UX here: Reload
/// prompts on a dirty buffer, so the user sees the document the prompt names.
fn reload_for_tab(window: &ApplicationWindow, tab: &Rc<TabState>) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    chrome.tabs.focus_page(&tab.content_box);
    if let Some(action) = simple_action(window, "reload") {
        action.activate(None);
    }
}

/// GTK4Rs/AP-30: don't `popdown()` synchronously from inside a descendant button's
/// own `clicked` handler — see `window/contextmenu.rs`'s identical helper for
/// the full "why" (Broken-accounting-of-active-state + spurious
/// `g_object_unref` criticals). Deliberately duplicated rather than shared:
/// the two context menus (document text vs. tab strip) are unrelated widget
/// trees, and importing across `window/contextmenu.rs` for four lines isn't
/// worth the coupling.
fn dismiss_context_popover(po: &gtk::Popover) {
    let po = po.clone();
    glib::idle_add_local_once(move || po.popdown());
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use std::io::Write;

    /// The required rubric: right-clicking an INACTIVE tab and choosing Reload
    /// must reload THAT tab, not whichever tab happens to be
    /// active. `win.reload` always acts on the active tab, so the fix
    /// (`reload_for_tab`) must `focus_page` the clicked tab first. This drives
    /// `reload_for_tab` directly (bypassing the popover button's own click
    /// plumbing, which only marshals the same call — the same shortcut
    /// `dnd.rs`'s `move_tab_to_new_window` integration test takes).
    ///
    /// Mutation-checked: reverting `reload_for_tab` to skip the `focus_page`
    /// call (i.e. just `simple_action(window, "reload").activate(None)`)
    /// makes this test FAIL — it reloads tab A's file (or no-ops, if A has no
    /// path) instead of tab B's, and the active tab stays A.
    #[gtktest::test]
    fn reload_for_tab_acts_on_the_clicked_tab_not_the_active_one() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.tabcontextmenu.reload"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        let mut file_a = tempfile::NamedTempFile::new().expect("temp file A");
        write!(file_a, "# A original").unwrap();
        let mut file_b = tempfile::NamedTempFile::new().expect("temp file B");
        write!(file_b, "# B original").unwrap();

        let window = crate::window::new_window(&app, "IT", "# A original", Some(file_a.path()));
        let tab_a = state(&window).expect("state registered after new_window");

        let tab_b_id = crate::window::create_tab_in_window(
            &window,
            "# B original",
            Some(file_b.path()),
            false,
            false,
        )
        .expect("create_tab_in_window returns the new tab's id");
        let tab_b = winstate::tab_by_id(tab_b_id).expect("tab B registered");
        assert_ne!(tab_a.id, tab_b.id, "sanity: two distinct tabs");

        // Switch back to tab A — tab B is now the INACTIVE tab this test
        // simulates a right-click on.
        let chrome = winstate::chrome(&window).expect("chrome registered");
        chrome.tabs.focus_page(&tab_a.content_box);
        assert_eq!(
            state(&window).map(|s| s.id),
            Some(tab_a.id),
            "sanity: tab A is active before the simulated right-click"
        );

        // Simulate an external edit to tab B's file while tab B sits in the
        // background — exactly what "Reload" is for.
        write!(file_b, " — EDITED").unwrap();
        file_b.flush().unwrap();
        let edited_b = std::fs::read_to_string(file_b.path()).unwrap();

        reload_for_tab(&window, &tab_b);
        assert!(
            crate::docio::settle(|| *tab_b.source() == edited_b),
            "the reload must land: it reads the file off the main thread now"
        );

        assert_eq!(
            state(&window).map(|s| s.id),
            Some(tab_b.id),
            "reload_for_tab must focus the CLICKED tab (B), not leave A active"
        );
        assert_eq!(
            *tab_b.source(),
            edited_b,
            "tab B's own file content must be reloaded"
        );
        assert_eq!(
            *tab_a.source(),
            "# A original",
            "tab A must be untouched by a reload driven for tab B"
        );

        window.destroy();
    }

    /// Same rubric, for Copy Full Path: copying the path of an
    /// INACTIVE tab must put THAT tab's path on the clipboard, not the active
    /// tab's. `win.copy-path` always acts on the active tab, so
    /// `copy_full_path_for_tab` must focus the clicked tab first.
    ///
    /// Mutation-checked the same way as the reload test above.
    #[gtktest::test]
    fn copy_full_path_for_tab_acts_on_the_clicked_tab_not_the_active_one() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.tabcontextmenu.copypath"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        let file_a = tempfile::NamedTempFile::new().expect("temp file A");
        let file_b = tempfile::NamedTempFile::new().expect("temp file B");

        let window = crate::window::new_window(&app, "IT", "# A", Some(file_a.path()));
        let tab_a = state(&window).expect("state registered after new_window");

        let tab_b_id =
            crate::window::create_tab_in_window(&window, "# B", Some(file_b.path()), false, false)
                .expect("create_tab_in_window returns the new tab's id");
        let tab_b = winstate::tab_by_id(tab_b_id).expect("tab B registered");

        // Tab A active, tab B in the background — the state this test
        // simulates a right-click-on-B from.
        let chrome = winstate::chrome(&window).expect("chrome registered");
        chrome.tabs.focus_page(&tab_a.content_box);
        assert_eq!(state(&window).map(|s| s.id), Some(tab_a.id));

        copy_full_path_for_tab(&window, &tab_b);

        assert_eq!(
            state(&window).map(|s| s.id),
            Some(tab_b.id),
            "copy_full_path_for_tab must focus the CLICKED tab (B)"
        );

        // Read the clipboard back asynchronously; pump the default main
        // context until the callback lands (same pattern as `dnd.rs`'s
        // integration test pumping a deferred idle callback).
        let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        {
            let result = result.clone();
            window
                .clipboard()
                .read_text_async(gtk::gio::Cancellable::NONE, move |text| {
                    *result.borrow_mut() = text.ok().flatten().map(|s| s.to_string());
                });
        }
        let ctx = glib::MainContext::default();
        for _ in 0..200 {
            if result.borrow().is_some() || !ctx.iteration(false) {
                break;
            }
        }

        assert_eq!(
            result.borrow().as_deref(),
            Some(file_b.path().to_string_lossy()).as_deref(),
            "clipboard must hold tab B's path, not tab A's"
        );

        window.destroy();
    }

    /// Same rubric, for Copy Document (TDD 7.12): the menu offers it, and choosing it
    /// on an INACTIVE tab copies THAT tab's source, not the active tab's.
    #[gtktest::test]
    fn copy_document_for_tab_acts_on_the_clicked_tab_not_the_active_one() {
        assert!(
            TabMenuItem::ALL.contains(&TabMenuItem::CopyDocument),
            "the tab context menu offers Copy Document"
        );
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.tabcontextmenu.copydocument"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        let window = crate::window::new_window(&app, "IT", "# A", None);
        let tab_a = state(&window).expect("state registered after new_window");
        let tab_b_id = crate::window::create_tab_in_window(&window, "# B", None, false, false)
            .expect("create_tab_in_window returns the new tab's id");
        let tab_b = winstate::tab_by_id(tab_b_id).expect("tab B registered");

        let chrome = winstate::chrome(&window).expect("chrome registered");
        chrome.tabs.focus_page(&tab_a.content_box);
        assert_eq!(state(&window).map(|s| s.id), Some(tab_a.id));

        copy_document_for_tab(&window, &tab_b);

        assert_eq!(
            state(&window).map(|s| s.id),
            Some(tab_b.id),
            "copy_document_for_tab must focus the CLICKED tab (B)"
        );
        let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        {
            let result = result.clone();
            window
                .clipboard()
                .read_text_async(gtk::gio::Cancellable::NONE, move |text| {
                    *result.borrow_mut() = text.ok().flatten().map(|s| s.to_string());
                });
        }
        let ctx = glib::MainContext::default();
        for _ in 0..200 {
            if result.borrow().is_some() || !ctx.iteration(false) {
                break;
            }
        }
        assert_eq!(
            result.borrow().as_deref(),
            Some("# B"),
            "clipboard must hold tab B's source, not tab A's"
        );

        window.destroy();
    }

    /// Same rubric, for Save: writing an INACTIVE tab must save THAT tab's
    /// content, not the active tab's. `win.save` always acts on the active
    /// tab, so `save_for_tab` must focus the clicked tab first (TDD 15.19).
    ///
    /// Mutation-checked the same way as the reload/copy-path tests above.
    #[gtktest::test]
    fn save_for_tab_acts_on_the_clicked_tab_not_the_active_one() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let a_path = dir.path().join("a.md");
            let b_path = dir.path().join("b.md");
            std::fs::write(&a_path, "a0\n").unwrap();
            std::fs::write(&b_path, "b0\n").unwrap();

            let app = gtk::Application::new(
                Some("com.extollit.scribobulate.integrationtest.tabcontextmenu.save"),
                gtk::gio::ApplicationFlags::NON_UNIQUE,
            );
            app.register(gtk::gio::Cancellable::NONE)
                .expect("register (emits startup) before building any window");

            let window = crate::window::new_window(&app, "IT", "a0\n", Some(&a_path));
            let tab_a = state(&window).expect("state registered after new_window");
            let tab_b_id =
                crate::window::create_tab_in_window(&window, "b0\n", Some(&b_path), false, false)
                    .expect("create_tab_in_window returns the new tab's id");
            let tab_b = winstate::tab_by_id(tab_b_id).expect("tab B registered");

            // Dirty tab B only, then switch back to A — B is the INACTIVE tab
            // this test simulates a right-click-Save on.
            tab_b.editor_buf.set_text("b1\n");
            let chrome = winstate::chrome(&window).expect("chrome registered");
            chrome.tabs.focus_page(&tab_a.content_box);
            assert_eq!(
                state(&window).map(|s| s.id),
                Some(tab_a.id),
                "sanity: tab A is active before the simulated right-click"
            );

            save_for_tab(&window, &tab_b);
            assert!(
                crate::docio::settle(|| !tab_b.is_dirty()),
                "the save must land: it writes off the main thread now"
            );

            assert_eq!(
                state(&window).map(|s| s.id),
                Some(tab_b.id),
                "save_for_tab must focus the CLICKED tab (B), not leave A active"
            );
            assert_eq!(std::fs::read_to_string(&b_path).unwrap(), "b1\n");
            assert_eq!(
                std::fs::read_to_string(&a_path).unwrap(),
                "a0\n",
                "tab A's file must be untouched by a save driven for tab B"
            );

            window.destroy();
        });
    }

    /// TDD 9.39: the tab menu takes the same scrolling body as the document menu, so
    /// its minimum height stops being its full height and GTK ≥ 4.14 cannot close it
    /// on the frame it opens wherever the screen cannot fit it. The effect itself is
    /// tested once, on `install_menu_body` (which `ContextMenu::open` installs); this
    /// pins the tab menu to it.
    #[gtktest::test]
    fn the_tab_menu_scrolls_rather_than_closing_where_the_screen_cannot_fit_it() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.tabcontextmenu.fit"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let window = crate::window::new_window(&app, "IT", "# A", None);
        let tab = state(&window).expect("state registered after new_window");
        let chrome = winstate::chrome(&window).expect("chrome registered");

        show_tab_context_menu(&window, &chrome.tabs, &tab, 10.0, 10.0);

        let bar = chrome.tabs.bar_widget();
        let popover = std::iter::successors(bar.first_child(), |w| w.next_sibling())
            .find_map(|w| w.downcast::<gtk::Popover>().ok())
            .expect("the tab menu is parented to the tab bar");
        assert!(
            popover
                .child()
                .and_downcast::<gtk::ScrolledWindow>()
                .is_some(),
            "the tab menu's body is not in a scroller, so a screen shorter than the \
             menu closes it the frame it opens"
        );
        popover.popdown();
        window.destroy();
    }

    /// **Closing the window with the tab menu open releases the menu** — the guard the
    /// three menus share (`contextmenu::assert_closing_the_window_releases`). This menu
    /// is released only because `TabBar`'s `dispose` unparents every child, and that
    /// dispose runs only if nothing in the menu holds the bar alive. Written, it found
    /// exactly that: every row strong-captured the tab, which reaches the bar.
    ///
    /// Mutation checks: emptying the tab bar's `dispose` sweep fails the release
    /// assertion, and so does restoring a `#[strong] tab` capture on any one row.
    #[gtktest::test]
    fn closing_the_window_with_the_tab_menu_open_releases_the_menu() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.tabcontextmenu.close"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let window = crate::window::new_window(&app, "IT", "# A", None);
        window.present();
        // Scoped: the tab and the chrome hold the tab bar strongly, and a reference the
        // test keeps would hold the menu's host alive past the close.
        let popover = {
            let tab = state(&window).expect("state registered after new_window");
            let chrome = winstate::chrome(&window).expect("chrome registered");
            show_tab_context_menu(&window, &chrome.tabs, &tab, 10.0, 10.0);
            let bar = chrome.tabs.bar_widget();
            std::iter::successors(bar.first_child(), |w| w.next_sibling())
                .find_map(|w| w.downcast::<gtk::Popover>().ok())
                .expect("the tab menu is parented to the tab bar")
        };
        crate::window::contextmenu::assert_closing_the_window_releases(
            window,
            popover,
            "the tab menu",
        );
    }

    /// The tab menu is released after its `closed` emission has unwound, never inside
    /// it: on GDK-Win32 `closed` can fire inside GTK's own unmap of the popover, where an
    /// unparent unrealizes the surface GDK is still hiding (GTK4Rs/AP-30). A STATE guard,
    /// as for the toolbar menu, because X11 queues that emission and cannot stage the
    /// fault. The menu is presented and shown first, so the close is a real one.
    ///
    /// Mutation check: an `on_closed` of `|p| p.unparent()` fails the first assertion.
    #[gtktest::test]
    fn the_closed_tab_menu_is_unparented_after_its_close_not_during_it() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.tabcontextmenu.release"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let window = crate::window::new_window(&app, "IT", "# A", None);
        window.present();
        let tab = state(&window).expect("state registered after new_window");
        let chrome = winstate::chrome(&window).expect("chrome registered");
        show_tab_context_menu(&window, &chrome.tabs, &tab, 10.0, 10.0);
        let bar = chrome.tabs.bar_widget();
        let popover = std::iter::successors(bar.first_child(), |w| w.next_sibling())
            .find_map(|w| w.downcast::<gtk::Popover>().ok())
            .expect("the tab menu is parented to the tab bar");
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            crate::testpump::TOOLTIP_HOVER_DRAIN,
        );
        assert!(popover.is_visible(), "precondition: the menu is open");

        let parented_in_closed = std::rc::Rc::new(std::cell::Cell::new(None));
        popover.connect_closed({
            let seen = std::rc::Rc::clone(&parented_in_closed);
            // Connected after the menu's own handler, so it runs after it.
            move |p| seen.set(Some(p.parent().is_some()))
        });
        popover.popdown();
        assert_eq!(
            parented_in_closed.get(),
            Some(true),
            "the tab menu's `closed` handler must not unparent it inside the emission"
        );
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            crate::widgets::TOOLTIP_HOVER_DRAIN + std::time::Duration::from_millis(200),
        );
        assert!(
            popover.parent().is_none(),
            "the closed menu is released afterwards"
        );
        window.destroy();
    }
}
