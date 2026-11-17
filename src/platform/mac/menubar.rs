//! macOS only. Puts the application's menus where this desktop keeps them — the
//! **system** menu bar at the top of the screen — instead of inside the window.
//!
//! ## What the platform lacks, and what this supplies
//!
//! Nothing here is application behaviour: the menu model is the same one
//! `app::build_menubar` assembles for every platform, and GTK already knows how
//! to render a `GMenuModel` as a native `NSMenu`. What is missing is the
//! *plumbing* — GTK's Quartz backend takes exactly one application-wide menubar
//! model, while this app's menus carry per-window content (`View ▸ Documents`
//! lists that window's tabs; `Format ▸`'s Link/Image items relabel Insert↔Edit
//! for that window's selection), so something has to keep the exported model
//! pointed at whichever window is active. That is this module's whole job.
//!
//! ## Why not calling `set_menubar` was the bug
//!
//! `app::setup` deliberately never called `gtk_application_set_menubar`, on the
//! reasoning that doing so would *export the model to a global-menu shell and
//! duplicate the in-window bar* (GTK4Rs/AP-76). That reasoning was drawn from
//! Linux's opt-in global-menu shells, and on Quartz it is exactly inverted:
//! `gtk_application_impl_quartz_startup` installs a native `NSMenu`
//! **unconditionally**, and when `gtk_application_get_menubar` returns NULL it
//! substitutes a built-in fallback — the `default-menu` resource in
//! `gtk/ui/gtkapplication-quartz.ui`, an app menu plus a stub `Edit` and
//! `Window` (GTK 4.22.4, `gtkapplication-quartz.c:320-333`). MEASURED before the
//! fix: the app's native bar read `scribobulate | Edit | Window`, that fallback
//! verbatim, while the real File/Edit/Format/View/Help bar sat inside the window.
//! So *withholding* the model is what produced two menu bars; supplying it
//! replaces the stub with the real thing and leaves exactly one.
//!
//! The in-window bar is not hidden here — it is never built (`BuiltMenubar::bar`
//! is `None` on this platform), so there is no second surface to keep in step.
//!
//! ## Why the export is re-pointed on every activation
//!
//! `gtk_application_impl_quartz_set_menubar` replaces one section of a `GMenu`
//! the native menu's item tracker observes, so a later `set_menubar` updates the
//! live `NSMenu` rather than needing it rebuilt. Action *state* needs no help at
//! all: `gtk_application_impl_quartz_active_window_changed` re-points its muxer's
//! `win` group at the newly active window (`gtkapplication-quartz.c:369-395`), so
//! `win.*` items in the exported model resolve against, and grey out for, the
//! window the user is actually looking at. Only model *content* had to be
//! followed, which is what this does.

use gtk::prelude::*;

/// The ONE model the system menu bar is given, for the life of the process, and the two
/// submenus inside it whose content follows the active window.
struct Exported {
    root: gtk::gio::Menu,
    /// `View ▸ Documents`: a copy of the active window's open-tab list.
    documents: gtk::gio::Menu,
    /// `Format ▸`'s insert section: a copy of the active window's Link/Image/Table items.
    insert: gtk::gio::Menu,
}

thread_local! {
    static EXPORTED: std::cell::OnceCell<Exported> = const { std::cell::OnceCell::new() };
}

/// Set on a window once its own menus are being watched for change, so a second
/// activation does not connect the watchers again.
const WATCHED: crate::saferizer::qdata_key::QdataKey<bool> =
    crate::saferizer::qdata_key::QdataKey::new("scrib-mac-menubar-watched");

fn with_exported<R>(f: impl FnOnce(&Exported) -> R) -> R {
    EXPORTED.with(|cell| {
        f(cell.get_or_init(|| {
            let crate::app::TopLevelMenus {
                menus,
                documents_menu,
                format_insert_menu,
            } = crate::app::build_top_level_menus();
            let root = gtk::gio::Menu::new();
            for (title, menu) in &menus {
                root.append_submenu(Some(title), menu);
            }
            Exported {
                root,
                documents: documents_menu,
                insert: format_insert_menu,
            }
        }))
    })
}

/// Whether `a` and `b` list the same items (label, action and target of each).
fn same_items(a: &gtk::gio::Menu, b: &gtk::gio::Menu) -> bool {
    if a.n_items() != b.n_items() {
        return false;
    }
    (0..a.n_items()).all(|i| {
        ["label", "action", "target"].into_iter().all(|attr| {
            a.item_attribute_value(i, attr, None) == b.item_attribute_value(i, attr, None)
        })
    })
}

/// Make `into` list what `from` lists, touching nothing when it already does.
///
/// Touching nothing matters: every change to the exported model makes GTK's Quartz menu
/// tracker build `NSMenuItem`s for the changed items, and MEASURED (GTK 4.24.1, macOS) it
/// never frees the ones it builds.
fn mirror(from: &gtk::gio::Menu, into: &gtk::gio::Menu) {
    if same_items(from, into) {
        return;
    }
    into.remove_all();
    for i in 0..from.n_items() {
        into.append_item(&gtk::gio::MenuItem::from_model(from, i));
    }
}

/// Copy `window`'s per-window menu content into the exported model.
fn follow(window: &gtk::ApplicationWindow) {
    let Some(chrome) = crate::winstate::chrome(window) else {
        return;
    };
    with_exported(|ex| {
        mirror(&chrome.documents_menu, &ex.documents);
        mirror(&chrome.format_insert_menu, &ex.insert);
    });
}

/// Keep the system menu bar showing the active window's own tab list and insert labels.
///
/// Call once, from `startup`, before any window exists. The menu bar is given ONE model, once,
/// and never another. The old design pointed it at each window's own model on every activation
/// (`set_menubar`), and every such call made GTK's Quartz backend build a whole new `NSMenu`
/// tree (~100 `GNSMenu` and ~470 `GNSMenuItem`, each with its `GtkMenuTrackerItem`) that was
/// never freed: MEASURED with `heap` across 8 Cmd+N/Cmd+W cycles, 1.66 MB of `MALLOC_SMALL`
/// per cycle, two activations to the cycle (MANUAL-TEST 8.6). What differs by window is only
/// the two small submenus above, so those are copied in on activation and nothing else moves.
///
/// A window whose chrome is not registered yet, or an activation with no window at all (the
/// last one closed), leaves the current content in place rather than blanking the bar.
pub(crate) fn track_active_window(app: &gtk::Application) {
    with_exported(|ex| app.set_menubar(Some(&ex.root)));
    app.connect_active_window_notify(|app| {
        let Some(window) = app.active_window() else {
            return;
        };
        let Some(window) = window.downcast_ref::<gtk::ApplicationWindow>() else {
            return;
        };
        let Some(chrome) = crate::winstate::chrome(window) else {
            return;
        };
        if WATCHED.get(window).is_none() {
            WATCHED.set(window, true);
            // While this window is the active one, a change to its tab list or insert
            // labels (an idle-deferred rebuild) reaches the bar too.
            for menu in [&chrome.documents_menu, &chrome.format_insert_menu] {
                menu.connect_items_changed(gtk::glib::clone!(
                    #[weak]
                    window,
                    move |_, _, _, _| {
                        let is_active = window
                            .application()
                            .and_then(|app| app.active_window())
                            .is_some_and(|active| active == window);
                        if is_active {
                            follow(&window);
                        }
                    }
                ));
            }
        }
        // Deferred: the activation is mid-signal, and the menu bar is live.
        let window = window.clone();
        gtk::glib::idle_add_local_once(move || follow(&window));
    });
}

/// The exported `View ▸ Documents` and `Format ▸` insert submenus, for tests.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn exported_dynamic_menus() -> (gtk::gio::Menu, gtk::gio::Menu) {
    with_exported(|ex| (ex.documents.clone(), ex.insert.clone()))
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    fn labels(menu: &gtk::gio::Menu) -> Vec<String> {
        (0..menu.n_items())
            .map(|i| {
                menu.item_attribute_value(i, "label", None)
                    .and_then(|v| v.str().map(str::to_owned))
                    .unwrap_or_default()
            })
            .collect()
    }

    /// **The system menu bar's `View ▸ Documents` follows the active window.**
    ///
    /// The bar has ONE model for the process, so what follows a window is a copy of its two
    /// per-window submenus. Mutation: not copying on activation leaves the first window's
    /// list in the bar for the second window; copying only on activation leaves a tab opened
    /// later in the active window out of it.
    #[gtktest::test]
    fn the_exported_documents_list_follows_the_active_window() {
        let app = crate::window::testkit::test_app(
            "com.extollit.scribobulate.integrationtest.macmenubar",
        );
        track_active_window(&app);
        let first = crate::window::new_window(&app, "IT", "# one\n", None);
        first.present();
        let (documents, _) = exported_dynamic_menus();
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the first window to be active",
            || app.active_window().as_ref() == Some(first.upcast_ref()),
        );
        // The tab list is rebuilt on an idle; the bar copies it as it changes.
        crate::testpump::until(crate::testpump::Clock::Idle, "one document listed", || {
            documents.n_items() == 1
        });

        let second =
            crate::window::new_window_from_source(&app, "IT", "# two\n", None, Some(&first));
        crate::window::create_tab_in_window(&second, "# three\n", None, false, false)
            .expect("a second tab");
        second.present();
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the second window to be active",
            || app.active_window().as_ref() == Some(second.upcast_ref()),
        );
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the bar to list the second window's two documents",
            || documents.n_items() == 2,
        );
        let second_list = labels(&documents);

        first.present();
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the first window to be active again",
            || app.active_window().as_ref() == Some(first.upcast_ref()),
        );
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the bar to list the first window's one document again",
            || documents.n_items() == 1,
        );
        assert_ne!(
            labels(&documents),
            second_list,
            "the list is the first window's own"
        );
        first.destroy();
        second.destroy();
    }
}
