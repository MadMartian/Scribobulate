//! The toolbar's right-click menu (TDD 9.40): the View ▸ Toolbar toggles, offered on
//! the bar they change.
//!
//! **One list, two surfaces.** Both this menu and the View ▸ Toolbar submenu are built
//! by walking [`crate::app::toolbar_toggles`], and every row binds the toggle's own
//! `app.` action by name (POLICY "One action per command"). Nothing here keeps state,
//! sets sensitivity or handles a click: a row's tick is the action's state, its greyed
//! state is the action's `enabled` (invariant I3 in [`super::toolbarchrome`]), and a
//! click is the action's own `change_state` — the same path the menu item takes, so the
//! last-section veto (I8) holds here without being restated.
//!
//! **Plain popover of check buttons, not a `GtkPopoverMenu`** — the convention every
//! context menu here follows (`window/contextmenu.rs`, `window/tabs/contextmenu.rs`).
//! A `GtkCheckButton` bound to a boolean stateful action shows the tick and reads as a
//! checkbox to assistive technology. Bare-letter access keys use the menu bar's letters
//! (`mnem`), drawn underlined and served by a local shortcut controller (GTK4Rs/T-1).
//!
//! The menu stays open across toggles so several sections can be changed in one visit;
//! it closes on Escape or a click outside, or by itself when a toggle hides the bar it
//! is anchored to.

use super::*;
use crate::app::{mnem, toolbar_toggles, ToolbarToggle};
use crate::widgets::wrapbox::ToolbarWrapBox;

/// Attach the right-click menu to the window's button toolbar.
///
/// Bubble phase: no toolbar control claims a secondary press, so a right-click on a
/// button reaches the bar as well, and the menu opens wherever on the bar the reader
/// right-clicks.
pub(super) fn attach_toolbar_menu(toolbar: &ToolbarWrapBox) {
    let gesture = GestureClick::new();
    gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
    gesture.connect_pressed(glib::clone!(
        #[weak]
        toolbar,
        move |gesture, _, x, y| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            open_toolbar_menu(&toolbar, x, y);
        }
    ));
    toolbar.add_controller(gesture);
}

/// Build and open the menu at `(x, y)` in the toolbar's coordinates. Returns the
/// popover so a test can inspect it.
pub(super) fn open_toolbar_menu(toolbar: &ToolbarWrapBox, x: f64, y: f64) -> gtk::Popover {
    let menu = super::contextmenu::ContextMenu::new();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let mut previous_group = None;
    for toggle in toolbar_toggles() {
        if previous_group.is_some_and(|group| group != toggle.group) {
            body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        previous_group = Some(toggle.group);
        body.append(&toggle_row(&menu, &toggle));
    }
    menu.open(toolbar, (x, y), &body, |_| {})
}

/// One row: a check button bound to the toggle's action, its label carrying the menu
/// bar's access key underlined, and that key registered on `menu`.
fn toggle_row(menu: &super::contextmenu::ContextMenu, toggle: &ToolbarToggle) -> gtk::CheckButton {
    let row = gtk::CheckButton::with_label(&toggle.label);
    let markup = menu.add_access_key(&row, &mnem(&toggle.label), || true);
    row.set_action_name(Some(&toggle.detailed_action()));
    // GTK 4.6 has no `CheckButton::set_child` (4.8), and a plain popover never makes
    // mnemonics visible (GTK4Rs/T-1), so the underline is drawn on the button's own
    // label. If a GTK release ever builds the button without a label child, the row
    // keeps its plain label and its access key still works.
    if let Some(label) = row.last_child().and_downcast::<gtk::Label>() {
        label.set_markup(&markup);
    }
    row
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::window::testkit::*;

    /// Every check button in the open menu, in order.
    fn rows(popover: &gtk::Popover) -> Vec<gtk::CheckButton> {
        fn walk(widget: &gtk::Widget, out: &mut Vec<gtk::CheckButton>) {
            if let Some(check) = widget.downcast_ref::<gtk::CheckButton>() {
                out.push(check.clone());
                return;
            }
            let mut next = widget.first_child();
            while let Some(child) = next {
                walk(&child, out);
                next = child.next_sibling();
            }
        }
        let mut out = Vec::new();
        walk(popover.upcast_ref(), &mut out);
        out
    }

    /// The actions the shipped View ▸ Toolbar submenu binds, in order — read from the
    /// menu bar model itself, not from the list both surfaces are built from, so this
    /// compares the two surfaces rather than one list with itself.
    fn view_toolbar_submenu_actions() -> Vec<String> {
        use gtk::gio::MenuModel;
        fn find_toolbar(model: &MenuModel) -> Option<MenuModel> {
            for i in 0..model.n_items() {
                let label = model
                    .item_attribute_value(i, "label", None)
                    .and_then(|v| v.get::<String>());
                if let Some(sub) = model.item_link(i, "submenu") {
                    if label.as_deref() == Some("_Toolbar") {
                        return Some(sub);
                    }
                    if let Some(found) = find_toolbar(&sub) {
                        return Some(found);
                    }
                }
                if let Some(section) = model.item_link(i, "section") {
                    if let Some(found) = find_toolbar(&section) {
                        return Some(found);
                    }
                }
            }
            None
        }
        fn actions(model: &MenuModel, out: &mut Vec<String>) {
            for i in 0..model.n_items() {
                if let Some(section) = model.item_link(i, "section") {
                    actions(&section, out);
                } else if let Some(action) = model
                    .item_attribute_value(i, "action", None)
                    .and_then(|v| v.get::<String>())
                {
                    out.push(action);
                }
            }
        }
        let menus = crate::app::build_menubar().model;
        let toolbar = find_toolbar(menus.upcast_ref()).expect("View ▸ Toolbar submenu");
        let mut out = Vec::new();
        actions(&toolbar, &mut out);
        out
    }

    fn open(win: &ApplicationWindow) -> (ToolbarWrapBox, gtk::Popover) {
        let toolbar = winstate::chrome(win).expect("chrome").toolbar.clone();
        let popover = open_toolbar_menu(&toolbar, 10.0, 10.0);
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(200),
        );
        (toolbar, popover)
    }

    /// A state home of the test's own, so the layout the window seeds from is not the
    /// developer's saved session.
    fn with_fresh_session<T>(f: impl FnOnce() -> T) -> T {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), f)
    }

    fn close(popover: &gtk::Popover) {
        popover.popdown();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(600),
        );
    }

    fn bool_state(app: &gtk::Application, action: &str) -> bool {
        app.action_state(action)
            .and_then(|v| v.get::<bool>())
            .unwrap_or_else(|| panic!("{action} is a boolean stateful action"))
    }

    fn row_for(popover: &gtk::Popover, action: &str) -> gtk::CheckButton {
        rows(popover)
            .into_iter()
            .find(|r| r.action_name().as_deref() == Some(format!("app.{action}").as_str()))
            .unwrap_or_else(|| panic!("a row for {action}"))
    }

    /// Press and release the primary button in the middle of `row`, through the row's
    /// own click gesture — the path a pointer click takes.
    fn click(row: &gtk::CheckButton) {
        let gesture = row
            .observe_controllers()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
            .find_map(|c| c.downcast::<GestureClick>().ok())
            .expect("a check button has a click gesture");
        let (x, y) = (f64::from(row.width()) / 2.0, f64::from(row.height()) / 2.0);
        gesture.emit_by_name::<()>("pressed", &[&1i32, &x, &y]);
        gesture.emit_by_name::<()>("released", &[&1i32, &x, &y]);
    }

    /// Run the shortcut the open menu binds to bare `key`, as a key press would.
    fn press_access_key(popover: &gtk::Popover, key: char) -> bool {
        popover
            .observe_controllers()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
            .filter_map(|c| c.downcast::<gtk::ShortcutController>().ok())
            .flat_map(|ctl| {
                ctl.iter::<glib::Object>()
                    .filter_map(Result::ok)
                    .filter_map(|s| s.downcast::<gtk::Shortcut>().ok())
                    .collect::<Vec<_>>()
            })
            .find(|s| {
                s.trigger()
                    .and_downcast::<gtk::KeyvalTrigger>()
                    .is_some_and(|t| t.keyval().to_unicode() == Some(key))
            })
            .and_then(|s| s.action())
            .is_some_and(|a| a.activate(gtk::ShortcutActionFlags::empty(), popover, None))
    }

    fn settle(ms: u64) {
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(ms),
        );
    }

    /// The right-click menu offers exactly the View ▸ Toolbar toggles, in the same
    /// order, each bound to the same `app.` action, ticked to match its state, with the
    /// menu bar's access key drawn underlined.
    #[gtktest::test]
    fn the_toolbar_menu_offers_the_view_toolbar_toggles() {
        with_fresh_session(|| {
            let app = test_app("com.extollit.scribobulate.integrationtest.toolbarmenu");
            let win = new_window(&app, "IT-toolbarmenu", "# H\n\ntext", None);
            win.present();
            let (_, popover) = open(&win);
            assert!(popover.is_visible(), "the menu opened");

            let rows = rows(&popover);
            let bound: Vec<String> = rows
                .iter()
                .map(|r| r.action_name().map(|n| n.to_string()).unwrap_or_default())
                .collect();
            let submenu = view_toolbar_submenu_actions();
            assert_eq!(submenu.len(), 7, "precondition: Show plus six sections");
            assert_eq!(
                bound, submenu,
                "the two surfaces must offer the same toggles"
            );

            for row in &rows {
                let name = row.action_name().expect("bound").to_string();
                let action = name.strip_prefix("app.").expect("app-scoped");
                assert_eq!(
                    row.is_active(),
                    bool_state(&app, action),
                    "{name}'s tick matches its state"
                );
                let label = row
                    .last_child()
                    .and_downcast::<gtk::Label>()
                    .expect("a check button draws its label in a child label");
                assert!(
                    label.label().contains("<u>"),
                    "{name}'s access key is drawn underlined"
                );
            }
            close(&popover);
        });
    }

    /// A click and an access key both drive the shared action, the tick follows, and the
    /// section on the bar follows.
    #[gtktest::test]
    fn a_row_toggles_the_shared_action_and_the_section_follows() {
        with_fresh_session(|| {
            let app = test_app("com.extollit.scribobulate.integrationtest.toolbarmenutoggle");
            let win = new_window(&app, "IT-toolbarmenu-toggle", "# H\n\ntext", None);
            for id in crate::app::TBTN_SECTION_IDS {
                app.change_action_state(&toolbarchrome::section_action(id), &true.to_variant());
            }
            win.present();
            let zoom = toolbarchrome::section_action("zoom");
            let zoom_items = winstate::chrome(&win).expect("chrome").toolbar_sections[5].clone();
            let (_, popover) = open(&win);
            let zoom_row = row_for(&popover, &zoom);

            click(&zoom_row);
            settle(100);
            assert!(
                !bool_state(&app, &zoom),
                "clicking the row hides the Zoom section"
            );
            assert!(!zoom_row.is_active(), "the row's tick follows the action");
            assert!(
                zoom_items.iter().all(|w| !w.is_visible()),
                "the Zoom section's widgets leave the bar"
            );

            // The menu bar's View ▸ Toolbar letter for Zoom.
            let key = crate::app::access_markup(&mnem("Zoom"))
                .0
                .expect("Zoom has an access key");
            assert!(press_access_key(&popover, key), "the access key is bound");
            settle(100);
            assert!(
                bool_state(&app, &zoom),
                "the access key shows the Zoom section again"
            );
            assert!(zoom_row.is_active());
            assert!(zoom_items.iter().all(|w| w.is_visible()));
            close(&popover);
        });
    }

    /// Unticking the last visible section from this menu hides the whole bar and keeps
    /// the section's tick — the View ▸ Toolbar rule, reached through the same action.
    #[gtktest::test]
    fn unticking_the_last_section_here_hides_the_bar_as_the_menu_does() {
        with_fresh_session(|| {
            let app = test_app("com.extollit.scribobulate.integrationtest.toolbarmenulast");
            let win = new_window(&app, "IT-toolbarmenu-last", "# H\n\ntext", None);
            for id in crate::app::TBTN_SECTION_IDS {
                let on = id == "zoom";
                app.change_action_state(&toolbarchrome::section_action(id), &on.to_variant());
            }
            win.present();
            let zoom = toolbarchrome::section_action("zoom");
            let (toolbar, popover) = open(&win);
            click(&row_for(&popover, &zoom));
            settle(600);
            assert!(
                !bool_state(&app, toolbarchrome::SHOW),
                "the whole bar hides"
            );
            assert!(!toolbar.is_visible());
            assert!(bool_state(&app, &zoom), "the last section keeps its tick");
            assert!(
                !popover.is_visible(),
                "the menu leaves with the bar it is on"
            );
        });
    }

    /// The `closed` handler leaves the popover parented until the emission has unwound.
    ///
    /// A STATE guard, because the failure it prevents cannot be staged here: the unparent
    /// is only harmful when `closed` is emitted inside GTK's own unmap of the popover, and
    /// that happens where the surface's `mapped` notify is synchronous (GDK-Win32), not on
    /// X11, where it is queued to an idle. Unparenting inside the emission fails this test
    /// on every platform.
    #[gtktest::test]
    fn the_closed_menu_is_unparented_after_its_close_not_during_it() {
        with_fresh_session(|| {
            let app = test_app("com.extollit.scribobulate.integrationtest.toolbarmenurelease");
            let win = new_window(&app, "IT-toolbarmenu-release", "# H\n\ntext", None);
            win.present();
            let (toolbar, popover) = open(&win);
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
                "the menu's `closed` handler must not unparent it inside the emission"
            );
            assert_eq!(popover.parent().as_ref(), Some(toolbar.upcast_ref()));
            settle(crate::widgets::TOOLTIP_HOVER_DRAIN.as_millis() as u64 + 200);
            assert!(
                popover.parent().is_none(),
                "the closed menu is released afterwards"
            );
        });
    }

    /// Closing the window with the menu open releases the menu, and raises nothing.
    ///
    /// The STATE half is the guard the three menus share
    /// ([`assert_closing_the_window_releases`](super::super::contextmenu::assert_closing_the_window_releases)):
    /// this menu is released only because `ToolbarWrapBox::dispose` unparents every child,
    /// and narrowing that sweep fails it on every platform. The teardown half fails only
    /// by a critical (fatal under the suite) or a crash: on GDK-Win32 the open popover's
    /// `closed` fires inside the window's unrealize of it, which is where an unparent in
    /// `closed` unrealized it a second time, so there it is a reproduction. On X11 that
    /// emission comes later and the fault cannot be staged; the STATE guard for it is
    /// `the_closed_menu_is_unparented_after_its_close_not_during_it`.
    ///
    /// Mutation check: emptying `ToolbarWrapBox::dispose` fails the release assertion.
    #[gtktest::test]
    fn closing_the_window_with_the_menu_open_releases_the_menu() {
        with_fresh_session(|| {
            let app = test_app("com.extollit.scribobulate.integrationtest.toolbarmenudestroy");
            let win = new_window(&app, "IT-toolbarmenu-destroy", "# H\n\ntext", None);
            win.present();
            let popover = {
                let (_, popover) = open(&win);
                popover
            };
            super::super::contextmenu::assert_closing_the_window_releases(
                win,
                popover,
                "the toolbar menu",
            );
        });
    }

    /// A right-click on the toolbar opens this menu: the toolbar's own secondary-button
    /// gesture is pressed, as a pointer press would, and the menu must then be showing
    /// on the toolbar.
    #[gtktest::test]
    fn the_toolbar_answers_a_right_click() {
        with_fresh_session(|| {
            let app = test_app("com.extollit.scribobulate.integrationtest.toolbarmenuclick");
            let win = new_window(&app, "IT-toolbarmenu-click", "# H\n\ntext", None);
            win.present();
            let toolbar = winstate::chrome(&win).expect("chrome").toolbar.clone();
            let menu_on_toolbar = || {
                std::iter::successors(toolbar.first_child(), |w| w.next_sibling())
                    .find_map(|w| w.downcast::<gtk::Popover>().ok())
            };
            assert!(
                menu_on_toolbar().is_none(),
                "precondition: no menu before the press"
            );
            let secondary = toolbar
                .observe_controllers()
                .iter::<glib::Object>()
                .filter_map(Result::ok)
                .filter_map(|c| c.downcast::<GestureClick>().ok())
                .find(|g| g.button() == gtk::gdk::BUTTON_SECONDARY)
                .expect("a secondary-button click gesture is on the toolbar");
            secondary.emit_by_name::<()>("pressed", &[&1i32, &10.0f64, &10.0f64]);
            // The press builds the menu synchronously, so this returns at once; it is a
            // bound, not a settle, should that ever become deferred.
            crate::testpump::until(
                crate::testpump::Clock::Frame,
                "the press to open a menu on the toolbar",
                || menu_on_toolbar().is_some(),
            );
            let popover = menu_on_toolbar().expect("the press opened no menu on the toolbar");
            assert!(
                popover.is_visible(),
                "the menu the press opened is not showing"
            );
            assert!(
                !rows(&popover).is_empty(),
                "the popover opened is not the toolbar menu"
            );
            close(&popover);
        });
    }
}
