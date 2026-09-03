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
    Reload,
    Rename,
}

impl TabMenuItem {
    /// Every item this menu ships, in the order it presents them.
    pub(crate) const ALL: [Self; 8] = [
        Self::Save,
        Self::SaveAs,
        Self::Close,
        Self::CloseOthers,
        Self::MoveToNewWindow,
        Self::CopyFullPath,
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
    /// Rename's is `n`.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Save => "_Save",
            Self::SaveAs => "Save _As…",
            Self::Close => "_Close Tab",
            Self::CloseOthers => "Close _Other Tabs",
            Self::MoveToNewWindow => "_Move to New Window",
            Self::CopyFullPath => "Copy _Full Path",
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
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.set_parent(&tab_view.bar_widget());
    popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));

    // `marked` is a `_`-marked label; `access_markup` renders it with the access
    // char underlined (a plain popover never gets mnemonics-visible — ScrAP-70). "Close Tab" reuses the File-menu mark so its `C` matches; the other two
    // are context-only but keep the menu-bar letters (Close Other = O, Move = M).
    // Bare-letter access keys (single flat page, so an always-true gate) — the same
    // Capture/Local ShortcutController recipe as the pane context menu (ScrAP-70).
    let key_controller = gtk::ShortcutController::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    key_controller.set_scope(gtk::ShortcutScope::Local);

    // Building the button and registering its access key are ONE call, taking the item
    // rather than a string. Two things follow, and both were live defects: the label is
    // spelled once instead of twice per item, and — because `add_key` was a silent no-op
    // when `access_markup` found no `_` — a label edited to drop its marker can no longer
    // lose its access key with neither a warning nor a failing test.
    let make_btn = {
        let ctrl = key_controller.clone();
        move |item: TabMenuItem| -> gtk::Button {
            let marked = item.label();
            let lbl = gtk::Label::new(None);
            lbl.set_markup(&crate::app::access_markup(marked).1);
            lbl.set_halign(gtk::Align::Start);
            let btn = gtk::Button::new();
            btn.set_child(Some(&lbl));
            btn.add_css_class("flat");
            btn.set_halign(gtk::Align::Fill);
            if let Some(ch) = crate::app::access_markup(marked).0 {
                if let Some(sc) = crate::app::access_shortcut(&btn, ch, || true) {
                    ctrl.add_shortcut(sc);
                }
            }
            btn
        }
    };

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // THE MENU IS BUILT BY WALKING `TabMenuItem::ALL`, in its order. That is what makes
    // the enumeration load-bearing rather than decorative: an item missing from `ALL`
    // does not merely go unchecked by the mnemonics guard — it does not RENDER, which is
    // the one kind of drift that reports itself. Identity, label, access key and
    // grouping are shared; sensitivity and the click handler stay per-item.
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
                btn.set_sensitive(save_enabled(tab.is_dirty(), tab.backing_missing.get()));
                btn.connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = w)]
                    window,
                    #[strong]
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
                    #[strong]
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
                    #[strong]
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
                    #[strong]
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
                    #[strong]
                    tab,
                    move |_| {
                        dismiss_context_popover(&po);
                        copy_full_path_for_tab(&w, &tab);
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
                    #[strong]
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
                    #[strong]
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

    popover.set_child(Some(&box_));
    popover.add_controller(key_controller);
    popover.connect_closed(|p| p.unparent());
    popover.popup();
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
}
