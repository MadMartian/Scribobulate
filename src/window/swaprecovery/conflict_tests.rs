//! TDD 22.7 — recovered work over a file that changed on disk after the snapshot.
//!
//! Split from `tests.rs` (the file-length limit), sharing its fixture builders. Three
//! surfaces of one condition: the save guard, the two prompts that share a corner, and
//! which tab the conflict prompt is raised for.

use super::tests::{header, seed_swap};
use super::*;
use crate::swapfile::DocId;
use crate::window::new_window;

/// What the file holds when the application comes back: somebody changed it after
/// the snapshot was taken.
const NEWER: &str = "changed on disk after the crash\n";
/// What the file held when the recovered edits were made.
const OLDER: &[u8] = b"as it was before the crash\n";
/// The snapshot: work on top of the OLDER file.
const RECOVERED: &str = "as it was before the crash\nplus work that was never saved\n";

/// The modal dialogs transient for `window` — the save guard's overwrite question is
/// one of these.
fn modal_transients_of(window: &ApplicationWindow) -> Vec<gtk::Window> {
    let parent: &gtk::Window = window.upcast_ref();
    let toplevels = gtk::Window::toplevels();
    (0..toplevels.n_items())
        .filter_map(|i| toplevels.item(i))
        .filter_map(|o| o.downcast::<gtk::Window>().ok())
        .filter(|w| w.is_modal() && w.transient_for().as_ref() == Some(parent))
        .collect()
}

/// A file-backed window whose one tab was restored for `file`, with a snapshot of
/// [`RECOVERED`] made against [`OLDER`] waiting in the swap directory, and the file
/// itself holding [`NEWER`]. Returns the window and its tab, recovery not yet run.
fn stale_recovery_fixture(
    state_home: &std::path::Path,
    app_suffix: &str,
) -> (
    Application,
    ApplicationWindow,
    Rc<TabState>,
    std::path::PathBuf,
) {
    let file = state_home.join("notes.md");
    std::fs::write(&file, NEWER).unwrap();
    let app = super::super::gtk_integration_tests::test_app(&format!(
        "com.extollit.scribobulate.it.{app_suffix}"
    ));
    let win = new_window(&app, "IT", NEWER, Some(&file));
    let tab = winstate::state(&win).expect("a tab");
    crate::app::attach_file_backing(&win, &tab, file.clone());
    let doc_id = DocId::generate();
    tab.adopt_doc_id(doc_id.clone());
    seed_swap(state_home, &header(doc_id, Some(&file), OLDER), RECOVERED);
    (app, win, tab, file)
}

/// **TDD 22.7 / 5.2 — Save does not write recovered work over a file that changed
/// since the crash without asking.**
///
/// The defect (macOS, 1.0.0 dry run): Cmd+S silently overwrote the newer file. The
/// guard compares the file against the tab's baseline, and recovery loads the tab from
/// the file as it is NOW — so the two agreed, the save was "safe", and the recovered
/// text went over somebody else's change with no word.
///
/// Mutation: drop the `recovered_basis` term from `winstate::save_is_safe` (or stop
/// recording it in `apply_recovered_content`) and the file reads back as the recovered
/// text with no prompt raised.
#[gtktest::test]
fn saving_recovered_work_over_a_file_changed_since_the_crash_asks_first() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let (app, win, tab, file) = stale_recovery_fixture(dir.path(), "rec22_7save");
        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));
        assert_eq!(
            tab.editor_text(),
            RECOVERED,
            "precondition: the work came back"
        );
        assert!(tab.is_dirty(), "precondition: and it came back dirty");

        let before = modal_transients_of(&win).len();
        super::super::save::save_with_guard(&win);
        // Settle on EITHER outcome so the failing arm reports promptly: the question
        // appears, or the save lands (the tab goes clean).
        crate::docio::settle(|| modal_transients_of(&win).len() > before || !tab.is_dirty());

        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            NEWER,
            "the change made on disk after the crash must not be overwritten unasked"
        );
        let prompts = modal_transients_of(&win);
        assert!(
            prompts.len() > before,
            "the save must ask before overwriting the newer file (TDD 5.2)"
        );
        assert!(
            tab.is_dirty(),
            "and the recovered work is still there to keep"
        );
        for p in prompts {
            p.destroy();
        }
        win.destroy();
    });
}

/// **TDD 22.7 — the conflict prompt is not hidden behind the recovery notice.**
///
/// Both are raised, both are bottom-end aligned in one overlay, and the recovery
/// notice was drawn second: the reader saw "Recovered unsaved changes" and nothing of
/// the file having changed until they pressed Keep (Linux 2/2, Windows). The conflict
/// prompt takes the corner while it is up; the notice comes back once it is answered.
///
/// Mutation: drop the `conflict_toast.is_visible()` yield from `sync_recovery_toast`
/// and both prompts report visible at once.
#[gtktest::test]
fn the_conflict_prompt_is_not_hidden_behind_the_recovery_notice() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let (app, win, tab, _file) = stale_recovery_fixture(dir.path(), "rec22_7toast");
        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));
        let chrome = tab.chrome();
        assert!(
            chrome.conflict_toast.is_visible(),
            "the file changed since the crash: the conflict prompt is raised"
        );
        assert!(
            !chrome.recovery_toast.is_visible(),
            "and nothing is drawn over it — the recovery notice waits its turn"
        );
        assert!(
            tab.recovered_at.get().is_some(),
            "waiting, not lost: the tab still carries the notice"
        );

        // Answer the conflict the way a reader does: its own Dismiss button.
        let dismiss =
            std::iter::successors(chrome.conflict_toast.first_child(), |w| w.next_sibling())
                .filter_map(|w| w.downcast::<gtk::Button>().ok())
                .find(|b| b.label().as_deref() == Some("Dismiss"))
                .expect("the conflict prompt has a Dismiss button");
        dismiss.emit_clicked();

        assert!(
            !chrome.conflict_toast.is_visible(),
            "the conflict is answered"
        );
        assert!(
            chrome.recovery_toast.is_visible(),
            "and the recovery notice is back, still offering Keep / Discard recovery"
        );
        win.destroy();
    });
}

/// **The conflict prompt is raised for the recovered tab, not over whichever is in
/// front.** The prompt is window-shared and its Reload acts on the ACTIVE tab, so
/// raising it for a background recovered tab offered to reload a different document.
/// A background tab is badged and gets its prompt when it is reached — the replay every
/// other background change takes.
///
/// Mutation: raise the prompt unconditionally again and the first assertion fails.
#[gtktest::test]
fn a_background_recovered_tab_gets_its_conflict_prompt_when_reached() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let (app, win, recovered, _file) = stale_recovery_fixture(dir.path(), "rec22_7bg");
        let other = create_tab_in_window(&win, "unrelated", None, false, false)
            .and_then(winstate::tab_by_id)
            .expect("a second tab");
        assert!(
            winstate::state(&win).is_some_and(|a| a.id == other.id),
            "precondition: the recovered tab is in the background"
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));
        assert!(recovered.is_dirty(), "precondition: the work came back");
        assert!(
            winstate::state(&win).is_some_and(|a| a.id == other.id),
            "precondition: the recovery left the other tab in front"
        );
        assert!(
            !other.chrome().conflict_toast.is_visible(),
            "no conflict prompt over a tab whose file did not change"
        );
        assert!(
            recovered.pending_external.get(),
            "the recovered tab keeps its change pending"
        );

        if let Some(chrome) = winstate::chrome(&win) {
            chrome.tabs.focus_page(&recovered.content_box);
        }
        assert!(
            crate::docio::settle(|| recovered.chrome().conflict_toast.is_visible()),
            "reaching the recovered tab raises its conflict prompt"
        );
        assert!(
            !recovered.chrome().recovery_toast.is_visible(),
            "with the recovery notice yielding to it there too"
        );
        win.destroy();
    });
}
