use super::super::super::*;
use gtk::prelude::Cast;
use std::path::Path;

/// The toolbar's open-documents combo box must (a) share the ONE per-window
/// `documents_menu` GMenu with the menubar's View ▸ Documents submenu — so the
/// two surfaces can never list different documents — and (b) show the ACTIVE
/// document's name as its label, tracking it across a tab switch. Both are the
/// point of the feature: a second surface for the same fast-switch command, not
/// a parallel one that can drift.
#[gtktest::test]
fn documents_combo_shares_the_menu_model_and_labels_the_active_document() {
    let app = gtk::Application::new(
        Some("com.extollit.scribobulate.integrationtest.docscombo"),
        gtk::gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(gtk::gio::Cancellable::NONE)
        .expect("register (emits startup) before building any window");

    // A window whose sole tab has a backing path, so the combo shows a real
    // filename rather than "Untitled". `new_window` does not read the file (the
    // Markdown is passed in), so a non-existent path is fine for this test.
    let window = crate::window::new_window(
        &app,
        "IT",
        "# Alpha",
        Some(Path::new("/tmp/scrib-it/alpha.md")),
    );
    let chrome = winstate::chrome(&window).expect("chrome registered");

    // (a) ONE model, two surfaces: the combo's popup binds the very same GMenu
    // object the menubar submenu does — identity, not just an equal copy.
    let bound = chrome
        .documents_btn
        .menu_model()
        .expect("the combo's menu-model is bound after build_window");
    assert_eq!(
        bound.as_ptr() as *const (),
        chrome
            .documents_menu
            .upcast_ref::<gtk::gio::MenuModel>()
            .as_ptr() as *const (),
        "the combo must reuse the window's documents_menu, not a separate model"
    );

    // (b) The label reflects the active document. The initial rebuild is
    // deferred to idle (GTK4Rs/AP-76), so drive the same refresh synchronously here —
    // this is exactly what the idle would call.
    super::refresh_documents_button(&window);
    assert_eq!(
        chrome.documents_btn.label().map(|s| s.to_string()),
        Some("alpha.md".to_string()),
        "the combo labels the sole (active) document"
    );

    // Add a second document and switch to it (defer = false → switch-page fires
    // synchronously → `refresh_tab_surfaces` updates the label). The combo must
    // now name the newly-active document.
    crate::window::create_tab_in_window(
        &window,
        "# Beta",
        Some(Path::new("/tmp/scrib-it/beta.md")),
        false,
        false,
    )
    .expect("second tab created");
    assert_eq!(
        chrome.documents_btn.label().map(|s| s.to_string()),
        Some("beta.md".to_string()),
        "switching to the new tab retargets the combo label"
    );

    // Switch back to the first document: the label follows the active tab.
    let first = winstate::tabs_for_window(&window)
        .into_iter()
        .find(|t| {
            t.path
                .borrow()
                .as_deref()
                .and_then(Path::file_name)
                .is_some_and(|n| n == "alpha.md")
        })
        .expect("first tab still present");
    chrome.tabs.focus_page(&first.content_box);
    assert_eq!(
        chrome.documents_btn.label().map(|s| s.to_string()),
        Some("alpha.md".to_string()),
        "switching back retargets the combo label to the first document"
    );

    window.destroy();
}

/// **Opening a file into a reused blank tab must relabel every derived-view
/// surface, not just the window title (TDD 1.5, TDD 15.18).**
///
/// Drives the REAL reuse path: a `gtk::Application` built exactly as production
/// builds it (`HANDLES_OPEN`, the real `setup_app` wiring) with one window
/// holding a single blank/untouched tab (`File ▸ New Document`'s state) is fed a
/// real file through `app.open(&[file], "interactive")` — the same entry point
/// `File ▸ Open`'s dialog response calls. That reaches `openbatch::build_opened_batch`'s
/// `find_reusable_blank_tab` branch, which loads the file into the blank tab
/// **in place** rather than opening a new tab or window.
///
/// Three surfaces are asserted, and none is implied by another: the tab-strip
/// label (`TabView::tab_label_text`), the Documents combo's button label, and the
/// View ▸ Documents menu's sole item label. All three must name the opened file,
/// not "Untitled" — the label `load_source_into_window` used to leave behind by
/// retitling the window with a bare `set_title` instead of going through
/// `update_window_title` (the one entry point that also relabels the strip and
/// schedules the Documents-menu rebuild).
#[gtktest::test]
fn opening_a_file_into_a_reused_blank_tab_relabels_every_surface() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("reopened.md");
    std::fs::write(&file, "# Reopened\n").unwrap();

    // Built the way production builds it — HANDLES_OPEN plus the real
    // `setup_app` wiring — so this drives the actual `open` handler
    // (`app::openbatch::on_open`) rather than calling it directly, which its
    // `pub(super)` visibility does not allow from here in any case.
    let app = gtk::Application::new(
        Some("com.extollit.scribobulate.integrationtest.reuseblanktab"),
        gtk::gio::ApplicationFlags::HANDLES_OPEN | gtk::gio::ApplicationFlags::NON_UNIQUE,
    );
    crate::app::setup_app(&app);
    app.register(gtk::gio::Cancellable::NONE)
        .expect("register before building a window");

    // File ▸ New Document's state: one window, one blank/untouched tab, no
    // backing path.
    let window = crate::window::new_window(&app, "Scribobulate", crate::app::WELCOME, None);
    crate::testpump::until(
        crate::testpump::Clock::Idle,
        "the new window becomes active",
        || app.active_window().is_some(),
    );
    // Let the coalesced Documents-menu rebuild `new_window`'s own construction
    // schedules (GTK4Rs/AP-76) actually dispatch and settle on "Untitled" before
    // `File ▸ Open` runs — a real user never clicks Open in the same main-loop
    // turn as New Document, and skipping this drain lets that STILL-PENDING
    // construction-time idle mask the defect under test: it would fire only
    // during the wait below, by which point the reuse has already landed, so it
    // would rebuild against the CORRECT state and hide a `load_source_into_window`
    // that never re-schedules it.
    crate::testpump::drain_for(
        crate::testpump::Clock::Idle,
        std::time::Duration::from_millis(300),
    );
    let tabs_before = winstate::tabs_for_window(&window);
    assert_eq!(tabs_before.len(), 1, "precondition: exactly one tab");
    assert!(
        tabs_before[0].path.borrow().is_none(),
        "precondition: the sole tab is blank/pathless"
    );

    // The real entry point `File ▸ Open`'s dialog response calls
    // (`appactions::add_open_action`) — the "interactive" hint is what selects
    // the blank-tab-reuse branch in `build_opened_batch`.
    let _cold = crate::app::coldstart::force_for_test(false); // This models a launch into a RUNNING instance: a window already exists, so it is
    app.open(&[gtk::gio::File::for_path(&file)], "interactive");

    // The read runs off the main thread (`docio::read_document`); wait for the
    // reuse to land rather than assuming it has by the time `open` returns.
    assert!(
        crate::docio::settle(|| {
            winstate::tabs_for_window(&window).len() == 1
                && state(&window)
                    .and_then(|st| st.path.borrow().clone())
                    .as_deref()
                    == Some(file.as_path())
        }),
        "the file must load into the existing blank tab in place — no new tab, \
             no new window"
    );
    // Flush the coalesced Documents-menu rebuild idle (GTK4Rs/AP-76) — `open`'s
    // own async pass never yields back to the main loop once the read is in
    // hand, but the deferred idle it schedules needs one further turn.
    crate::testpump::drain_for(
        crate::testpump::Clock::Idle,
        std::time::Duration::from_millis(300),
    );

    let chrome = winstate::chrome(&window).expect("chrome registered");
    let tab = state(&window).expect("the reused tab is still active");

    assert_eq!(
        chrome.tabs.tab_label_text(&tab.content_box),
        Some("reopened.md".to_string()),
        "the tab strip must show the opened file's name, not the blank tab's \
             stale label"
    );
    assert_eq!(
        chrome.documents_btn.label().map(|s| s.to_string()),
        Some("reopened.md".to_string()),
        "the toolbar Documents combo must name the opened file"
    );
    assert_eq!(chrome.documents_menu.n_items(), 1, "still a single tab");
    assert_eq!(
        chrome
            .documents_menu
            .item_attribute_value(0, "label", None)
            .and_then(|v| v.str().map(str::to_string)),
        Some("reopened.md".to_string()),
        "the View ▸ Documents menu must name the opened file, not \"Untitled\""
    );

    window.destroy();
}

/// `GTK_TEXT_VIEW_PRIORITY_VALIDATE` (`gtktextview.h`: `GDK_PRIORITY_REDRAW + 5`) — the
/// priority of a text view's background layout, a source that stays ready until the whole
/// buffer is laid out.
const TEXT_VIEW_VALIDATE_PRIORITY: i32 = 125;

/// **The Documents list relabels a reused tab while the new document is still being laid
/// out (TDD 15.18).** A rebuild queued at the default idle priority (200) waits behind a
/// text view's validation source (125) until the whole document is laid out — close to a
/// second of "Untitled" for a large file. A source held ready at that priority stands in
/// for the layout, so the check is deterministic rather than a race against file size: the
/// list must still name the file while it is armed.
#[gtktest::test]
fn the_documents_list_relabels_a_reused_tab_while_layout_is_still_running() {
    let dir = tempfile::tempdir().unwrap();
    let alpha = dir.path().join("alpha.md");
    let beta = dir.path().join("beta.md");
    std::fs::write(&alpha, "# Alpha\n").unwrap();
    std::fs::write(&beta, "# Beta\n").unwrap();

    let app = gtk::Application::new(
        Some("com.extollit.scribobulate.integrationtest.reusenewtab"),
        gtk::gio::ApplicationFlags::HANDLES_OPEN | gtk::gio::ApplicationFlags::NON_UNIQUE,
    );
    crate::app::setup_app(&app);
    app.register(gtk::gio::Cancellable::NONE)
        .expect("register before building a window");

    let window = crate::window::new_window(&app, "Scribobulate", crate::app::WELCOME, None);
    crate::testpump::until(crate::testpump::Clock::Idle, "window active", || {
        app.active_window().is_some()
    });
    crate::testpump::drain_for(
        crate::testpump::Clock::Idle,
        std::time::Duration::from_millis(300),
    );
    let _cold = crate::app::coldstart::force_for_test(false); // This models a launch into a RUNNING instance: a window already exists, so it is
    app.open(&[gtk::gio::File::for_path(&alpha)], "interactive");
    assert!(crate::docio::settle(|| state(&window)
        .and_then(|st| st.path.borrow().clone())
        .as_deref()
        == Some(alpha.as_path())));
    crate::testpump::drain_for(
        crate::testpump::Clock::Idle,
        std::time::Duration::from_millis(300),
    );

    crate::window::add_new_document_tab(&window);
    crate::testpump::drain_for(
        crate::testpump::Clock::Idle,
        std::time::Duration::from_millis(300),
    );
    let labels = |w: &ApplicationWindow| {
        let chrome = winstate::chrome(w).unwrap();
        (0..chrome.documents_menu.n_items())
            .map(|i| {
                chrome
                    .documents_menu
                    .item_attribute_value(i, "label", None)
                    .and_then(|v| v.str().map(str::to_string))
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(&window), ["alpha.md", "Untitled"], "precondition");
    // The blank tab in the BACKGROUND: reuse must still relabel it (TDD 1.5).
    if let Some(named) = winstate::tabs_for_window(&window)
        .into_iter()
        .find(|t| t.has_path())
    {
        winstate::chrome(&window)
            .unwrap()
            .tabs
            .focus_page(&named.content_box);
    }
    crate::testpump::drain_for(
        crate::testpump::Clock::Idle,
        std::time::Duration::from_millis(300),
    );

    // Stand-in for the opened document's layout, disarmed on every exit path so it
    // cannot starve the tests that run after this one.
    struct Layout(std::rc::Rc<std::cell::Cell<bool>>);
    impl Drop for Layout {
        fn drop(&mut self) {
            self.0.set(false);
        }
    }
    let layout = Layout(std::rc::Rc::new(std::cell::Cell::new(true)));
    let running = layout.0.clone();
    gtk::glib::idle_add_local_full(
        gtk::glib::Priority::from(TEXT_VIEW_VALIDATE_PRIORITY),
        move || {
            if running.get() {
                gtk::glib::ControlFlow::Continue
            } else {
                gtk::glib::ControlFlow::Break
            }
        },
    );

    let _cold = crate::app::coldstart::force_for_test(false); // This models a launch into a RUNNING instance: a window already exists, so it is
    app.open(&[gtk::gio::File::for_path(&beta)], "interactive");
    assert!(
        crate::docio::settle(|| labels(&window) == ["alpha.md", "beta.md"]),
        "View ▸ Documents must name the opened file while its layout is still running, \
         got {:?}",
        labels(&window)
    );
    assert_eq!(
        winstate::chrome(&window)
            .unwrap()
            .documents_btn
            .label()
            .map(|s| s.to_string()),
        Some("beta.md".to_string()),
        "the toolbar Documents combo must name the opened file too"
    );
    drop(layout);

    window.destroy();
}

/// **The window title names the ACTIVE document and counts the others (TDD
/// 15.7).** Three claims, none implied by the others:
///
/// 1. A lone tab is named with no parenthetical at all.
/// 2. Opening a second document adds `(+1 document)` — singular — to the title.
/// 3. A plain tab SWITCH re-aims the title at the newly active document. This is
///    the one the old count-only title had no way to get wrong, and the reason
///    `refresh_tab_surfaces` now retitles: nothing else fires on a switch, so
///    without that call the title would name whichever document happened to be
///    active when the tab SET last changed and then quietly stop tracking.
///
/// Asserted against `window.title()` — the property the desktop actually reads —
/// and against literal strings rather than a second call to the formula: a test
/// that recomputes the formula agrees with any change to it, including a wrong
/// one (GTK4Rs/AP-160's "makes the test agree with itself"). The one-shared-formula
/// claim is `save.rs`'s to make; this test's job is the *content*.
#[gtktest::test]
fn window_title_names_the_active_document_and_counts_the_others() {
    let app = gtk::Application::new(
        Some("com.extollit.scribobulate.integrationtest.wintitle"),
        gtk::gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(gtk::gio::Cancellable::NONE)
        .expect("register (emits startup) before building any window");

    let window = crate::window::new_window(
        &app,
        "IT",
        "# Alpha",
        Some(Path::new("/tmp/scrib-it/alpha.md")),
    );
    assert_eq!(
        window.title().map(|s| s.to_string()),
        Some("alpha.md — Scribobulate".to_string()),
        "a lone document is named on its own — no sibling count"
    );

    // A second document, switched to on creation (defer = false → switch-page
    // fires synchronously). Singular: one other document, not "(+1 documents)".
    crate::window::create_tab_in_window(
        &window,
        "# Beta",
        Some(Path::new("/tmp/scrib-it/beta.md")),
        false,
        false,
    )
    .expect("second tab created");
    assert_eq!(
        window.title().map(|s| s.to_string()),
        Some("beta.md (+1 document) — Scribobulate".to_string()),
        "the title names the newly active document and counts the other one"
    );

    // A third, then switch back to the first: the title must follow the ACTIVE
    // tab, and the count must follow the tab SET independently of it.
    crate::window::create_tab_in_window(
        &window,
        "# Gamma",
        Some(Path::new("/tmp/scrib-it/gamma.md")),
        false,
        false,
    )
    .expect("third tab created");
    assert_eq!(
        window.title().map(|s| s.to_string()),
        Some("gamma.md (+2 documents) — Scribobulate".to_string()),
        "two other documents pluralise the count"
    );

    let chrome = winstate::chrome(&window).expect("chrome registered");
    let first = winstate::tabs_for_window(&window)
        .into_iter()
        .find(|t| {
            t.path
                .borrow()
                .as_deref()
                .and_then(Path::file_name)
                .is_some_and(|n| n == "alpha.md")
        })
        .expect("first tab still present");
    chrome.tabs.focus_page(&first.content_box);
    assert_eq!(
        window.title().map(|s| s.to_string()),
        Some("alpha.md (+2 documents) — Scribobulate".to_string()),
        "a plain tab switch re-aims the title at the now-active document, \
             keeping the same sibling count"
    );

    window.destroy();
}

/// **One invocation naming a file twice opens it ONCE.**
///
/// The dedup was against already-OPEN tabs, which is a different question, so a batch
/// that repeated a path built a second tab on the same document — each with its own
/// baseline, file monitor and swapfile, so saving in one raises an external-change
/// decision in the other. Not a contrived input: the handler's own doc comment gives
/// overlapping globs as the ordinary case, and a shell expands them without
/// deduplicating.
///
/// Two SPELLINGS of the one path, not the same string twice, because string equality
/// would have deduplicated the easy case and left the real one open.
#[gtktest::test]
fn one_invocation_naming_a_file_twice_opens_it_once() {
    let app = gtk::Application::new(
        Some("com.extollit.scribobulate.integrationtest.opendedup"),
        gtk::gio::ApplicationFlags::HANDLES_OPEN | gtk::gio::ApplicationFlags::NON_UNIQUE,
    );
    crate::app::setup_app(&app);
    app.register(gtk::gio::Cancellable::NONE)
        .expect("register before building a window");
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("notes.md");
    std::fs::write(&file, "# once\n").expect("fixture");
    // `dir/./notes.md` — the same file, spelled differently.
    let indirect = dir.path().join(".").join("notes.md");

    // A window must exist before `open`, or this is a cold start rather than a
    // launch into a running instance.
    let _window = crate::window::new_window(&app, "IT", "", None);
    let _cold = crate::app::coldstart::force_for_test(false);
    app.open(
        &[
            gtk::gio::File::for_path(&file),
            gtk::gio::File::for_path(&indirect),
        ],
        "interactive",
    );

    // The claim is about DUPLICATION, not about blank-tab reuse: exactly one tab in
    // the whole application backs this file. Counting tabs-with-this-path rather than
    // total tabs keeps the assertion to the thing under test.
    let backing_this_file = || {
        app.windows()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::ApplicationWindow>().ok())
            .flat_map(|w| winstate::tabs_for_window(&w))
            .filter(|t| {
                t.path
                    .borrow()
                    .as_deref()
                    .is_some_and(|p| p == file.as_path())
            })
            .count()
    };
    assert!(
        crate::docio::settle(|| backing_this_file() == 1),
        "two spellings of one path in one invocation must produce ONE tab backing it, \
         got {}",
        backing_this_file()
    );
    // Non-vacuous: the file really was opened, rather than refused by both limbs.
    assert_eq!(backing_this_file(), 1);
}

/// Two `open` invocations naming the same file, the second arriving while the first is
/// still reading: still ONE tab. Both pass the pre-read "already open?" check, because
/// nothing is built yet; the build re-asks per document, so whichever batch builds
/// second focuses the first one's tab.
///
/// Mutation: remove the re-check at the top of `build_opened_batch`'s loop and this
/// counts two tabs on the one file.
#[gtktest::test]
fn two_overlapping_invocations_naming_one_file_open_it_once() {
    let app = gtk::Application::new(
        Some("com.extollit.scribobulate.integrationtest.openoverlap"),
        gtk::gio::ApplicationFlags::HANDLES_OPEN | gtk::gio::ApplicationFlags::NON_UNIQUE,
    );
    crate::app::setup_app(&app);
    app.register(gtk::gio::Cancellable::NONE)
        .expect("register before building a window");
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("notes.md");
    std::fs::write(&file, "# once\n").expect("fixture");
    let _window = crate::window::new_window(&app, "IT", "", None);
    let _cold = crate::app::coldstart::force_for_test(false);
    // Back to back, with no main-loop turn between: the first batch's reads cannot
    // have completed when the second invocation runs its pre-read check.
    let built_before = crate::app::BATCHES_BUILT.with(std::cell::Cell::get);
    app.open(&[gtk::gio::File::for_path(&file)], "interactive");
    app.open(&[gtk::gio::File::for_path(&file)], "interactive");

    let backing_this_file = || {
        app.windows()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::ApplicationWindow>().ok())
            .flat_map(|w| winstate::tabs_for_window(&w))
            .filter(|t| {
                t.path
                    .borrow()
                    .as_deref()
                    .is_some_and(|p| p == file.as_path())
            })
            .count()
    };
    // Both batches must have FINISHED building before counting — the event that ends
    // the race, not a time a slow second batch could outlast.
    assert!(
        crate::docio::settle(
            || crate::app::BATCHES_BUILT.with(std::cell::Cell::get) >= built_before + 2
        ),
        "precondition: both open batches ran to the end"
    );
    assert_eq!(
        backing_this_file(),
        1,
        "two overlapping invocations naming one file must produce ONE tab backing it"
    );
}

/// **Opening a file that carries a NUL leaves no dirty tab, and the file on disk is
/// byte-identical afterwards, even after a Save (TDD 1.4a; manual 4.2-c).**
///
/// The defect, MEASURED on Linux and Windows: the loader admitted the NUL (it is valid
/// UTF-8), the editor's `set_text` dropped the whole document on GTK's
/// `g_utf8_validate` assertion, and the tab opened dirty against its full baseline —
/// so a Save with nothing typed wrote 0 bytes over the file and said "File saved.".
/// Driven through the real `open` handler (`app.open`, which File ▸ Open, the CLI and a
/// file-association launch all reach), then a Save is attempted the way the user
/// attempted it, and the bytes are compared. Pre-fix this dies on the fatal
/// `Gtk-CRITICAL` the insert raises.
#[gtktest::test]
fn opening_a_file_with_a_nul_leaves_no_dirty_tab_and_the_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("nul.md");
    let bytes = b"# Title\n\nbefore\x00after\n\nlast line\n";
    std::fs::write(&file, bytes).unwrap();

    crate::session::with_state_home_for_test(dir.path(), || {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.nulopen"),
            gtk::gio::ApplicationFlags::HANDLES_OPEN | gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        crate::app::setup_app(&app);
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");
        let window = crate::window::new_window(&app, "Scribobulate", crate::app::WELCOME, None);
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the new window becomes active",
            || app.active_window().is_some(),
        );

        let _cold = crate::app::coldstart::force_for_test(false);
        let built_before = crate::app::BATCHES_BUILT.with(std::cell::Cell::get);
        app.open(&[gtk::gio::File::for_path(&file)], "interactive");
        assert!(
            crate::docio::settle(
                || crate::app::BATCHES_BUILT.with(std::cell::Cell::get) > built_before
            ),
            "the open batch must finish building"
        );

        // Read the tab's state BEFORE the save, then press Save, then assert — the
        // data loss first, so a regression reports the overwrite rather than stopping
        // at the first symptom that precedes it.
        let st = state(&window).expect("the opened tab is active");
        let source = st.source().clone();
        let has_path = st.path.borrow().is_some();
        let dirty = winstate::tabs_for_window(&window)
            .iter()
            .any(|tab| tab.is_dirty());
        let save_enabled = window.lookup_action("save").is_some_and(|a| a.is_enabled());

        // The user's gesture, whatever its enablement: it must not reach the file.
        gtk::prelude::ActionGroupExt::activate_action(&window, "save", None);
        crate::testpump::drain_for(
            crate::testpump::Clock::Worker,
            std::time::Duration::from_millis(500),
        );
        assert_eq!(
            std::fs::read(&file).unwrap(),
            bytes,
            "the file on disk is byte-identical after open and Save"
        );
        assert!(!dirty, "no tab may open dirty");
        assert!(!save_enabled, "Save is disabled on the refused tab");
        assert!(
            !has_path,
            "a refused document has no path a save could write over"
        );
        assert!(
            source.contains("Refused to open file") && source.contains("NUL"),
            "the tab shows a refusal naming the NUL, got: {source}"
        );
        window.destroy();
    });
}
