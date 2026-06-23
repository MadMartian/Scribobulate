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
    let make_btn = |marked: &str| -> gtk::Button {
        let lbl = gtk::Label::new(None);
        lbl.set_markup(&crate::app::access_markup(marked).1);
        lbl.set_halign(gtk::Align::Start);
        let btn = gtk::Button::new();
        btn.set_child(Some(&lbl));
        btn.add_css_class("flat");
        btn.set_halign(gtk::Align::Fill);
        btn
    };

    // Bare-letter access keys (single flat page, so an always-true gate) — the same
    // Capture/Local ShortcutController recipe as the pane context menu (ScrAP-70).
    let key_controller = gtk::ShortcutController::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    key_controller.set_scope(gtk::ShortcutScope::Local);
    let add_key = {
        let ctrl = key_controller.clone();
        move |btn: &gtk::Button, marked: &str| {
            if let Some(ch) = crate::app::access_markup(marked).0 {
                if let Some(sc) = crate::app::access_shortcut(btn, ch, || true) {
                    ctrl.add_shortcut(sc);
                }
            }
        }
    };

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let close_btn = make_btn("_Close Tab");
    add_key(&close_btn, "_Close Tab");
    close_btn.connect_clicked(glib::clone!(
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
    box_.append(&close_btn);

    let has_others = winstate::tabs_for_window(window)
        .iter()
        .any(|t| t.id != tab.id);
    let close_others_btn = make_btn("Close _Other Tabs");
    add_key(&close_others_btn, "Close _Other Tabs");
    close_others_btn.set_sensitive(has_others);
    close_others_btn.connect_clicked(glib::clone!(
        #[weak(rename_to = po)]
        popover,
        #[weak(rename_to = w)]
        window,
        #[strong]
        tab,
        move |_| {
            dismiss_context_popover(&po);
            // Close the OTHER tabs, keeping this one. Clean tabs close at once;
            // dirty tabs prompt sequentially, not N dialogs at once.
            close_other_tabs(&w, tab.clone());
        }
    ));
    box_.append(&close_others_btn);

    box_.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    let move_btn = make_btn("_Move to New Window");
    add_key(&move_btn, "_Move to New Window");
    // Single source of truth (POLICY): "Move to New Window" is one command with
    // three surfaces (menu bar, toolbar, this tab context menu), all driven by
    // the `win.move-tab-new-window` GAction. Read the action's own enabled state
    // rather than recomputing the `tab_count > 1` precondition here — a future
    // change to the action's gate (e.g. also disabling mid-drag) then can't skip
    // this surface. (M4)
    move_btn.set_sensitive(
        simple_action(window, "move-tab-new-window")
            .map(|a| a.is_enabled())
            .unwrap_or(false),
    );
    let content = tab.content_box.clone();
    move_btn.connect_clicked(glib::clone!(
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
    box_.append(&move_btn);

    box_.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // Copy Full Path / Reload are window-scoped actions (`win.copy-path` /
    // `win.reload`) that always act on the ACTIVE tab — but this menu fires for
    // the RIGHT-CLICKED tab, which need not be active. Two requirements follow:
    //
    // 1. Sensitivity must be read from the CLICKED tab's own `has_path()`, the
    //    same predicate that feeds the actions on every switch
    //    (`window/tabs/switch.rs`) — NOT from `action.is_enabled()`. Unlike
    //    `move_btn` above (M4, correct there because `move-tab-new-window`'s
    //    gate is genuinely window-scoped), these two actions are per-tab: the
    //    action's current `is_enabled()` reflects whichever tab is active RIGHT
    //    NOW, which is the wrong tab for a right-click on an inactive one.
    // 2. Activation must not just invoke the action — that would silently act
    //    on the active tab. Follow the Move-to-New-Window precedent below:
    //    `focus_page` the clicked tab first (synchronously resyncing the
    //    actions' enabled state to match), then drive the action. Reload also
    //    prompts on a dirty buffer, so focusing first means the prompt names
    //    the document the user actually clicked.
    let copy_path_btn = make_btn("Copy _Full Path");
    add_key(&copy_path_btn, "Copy _Full Path");
    copy_path_btn.set_sensitive(tab.has_path());
    copy_path_btn.connect_clicked(glib::clone!(
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
    box_.append(&copy_path_btn);

    let reload_btn = make_btn("_Reload");
    add_key(&reload_btn, "_Reload");
    reload_btn.set_sensitive(tab.has_path());
    reload_btn.connect_clicked(glib::clone!(
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
    box_.append(&reload_btn);

    popover.set_child(Some(&box_));
    popover.add_controller(key_controller);
    popover.connect_closed(|p| p.unparent());
    popover.popup();
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

/// ScrAP-30: don't `popdown()` synchronously from inside a descendant button's
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
            crate::docio::settle(|| *tab_b.source.borrow() == edited_b),
            "the reload must land: it reads the file off the main thread now"
        );

        assert_eq!(
            state(&window).map(|s| s.id),
            Some(tab_b.id),
            "reload_for_tab must focus the CLICKED tab (B), not leave A active"
        );
        assert_eq!(
            *tab_b.source.borrow(),
            edited_b,
            "tab B's own file content must be reloaded"
        );
        assert_eq!(
            *tab_a.source.borrow(),
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
}
