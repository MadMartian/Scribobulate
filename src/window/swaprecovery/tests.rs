use super::*;
use crate::swapfile::{DocId, SwapHeader};
use crate::window::new_window;

/// Write a swap file into the (test-redirected) state directory, exactly as a
/// pre-crash run would have left it.
///
/// Built through the production encoder rather than a hand-written string: a fixture
/// that fabricates the on-disk shape independently would keep passing after the
/// format changed, which is the failure mode a recovery test can least afford.
pub(super) fn seed_swap(state_home: &std::path::Path, header: &SwapHeader, body: &str) {
    let dir = state_home.join("scribobulate").join("swap");
    std::fs::create_dir_all(&dir).expect("swap dir");
    let name = crate::swapfile::swap_file_name(
        header.path.as_deref().map(std::path::Path::new),
        &header.doc_id,
    );
    std::fs::write(
        dir.join(name),
        crate::swapfile::encode(header, body).unwrap(),
    )
    .expect("seed the snapshot");
}

pub(super) fn header(doc_id: DocId, path: Option<&std::path::Path>, baseline: &[u8]) -> SwapHeader {
    SwapHeader {
        doc_id,
        path: path.and_then(|p| p.to_str()).map(str::to_string),
        untitled: path.is_none(),
        baseline_digest: crate::swapfile::content_digest(baseline),
        written_at: 1_754_000_000,
        // A pid that is not ours and is not a live Scribobulate, so the liveness
        // guard resolves to "recover" — which is what a real post-crash scan sees.
        owner_pid: 999_999,
        app_version: "0.1.0".to_string(),
    }
}

/// TDD 22.1, end to end bar the actual crash: content snapshotted before an unclean
/// exit comes back into the tab that was restored for it, still dirty.
#[gtktest::test]
fn a_snapshot_is_recovered_into_the_tab_that_was_restored_for_it() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec1");
        let win = new_window(&app, "IT", "on disk", None);
        let tab = winstate::state(&win).expect("a tab");

        // Stand in for "the session restored this tab with this identity".
        let doc_id = DocId::generate();
        tab.adopt_doc_id(doc_id.clone());
        seed_swap(
            dir.path(),
            &header(doc_id, None, b"on disk"),
            "on disk, plus work that was never saved",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        assert_eq!(
            tab.editor_text(),
            "on disk, plus work that was never saved",
            "the pre-crash buffer content is back"
        );
        assert!(
            tab.is_dirty(),
            "and it comes back DIRTY — the pre-crash state, not merely the layout"
        );
    });
}

/// **QA M01** — recovery repairs the SOURCE, not only the buffer.
///
/// `lineendings`' module doc records that repairing the buffer alone was the first
/// attempt at the lone-CR defect, that it looked convincing, and that every derived
/// view stayed broken because the preview, the outline and the annotations list render
/// from `tab.source` and never from the editor buffer. Swap recovery was a fourth
/// ingress door with exactly that shape: the buffer is repaired by the hook armed at
/// its birth, and `source` was assigned the decoded body verbatim.
///
/// This asserts BOTH halves, because asserting only `editor_text()` is what let the
/// original defect survive its own test suite.
#[gtktest::test]
fn recovery_repairs_the_derived_source_and_not_only_the_editor_buffer() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.reccr");
        let win = new_window(&app, "IT", "on disk", None);
        let tab = winstate::state(&win).expect("a tab");

        let doc_id = DocId::generate();
        tab.adopt_doc_id(doc_id.clone());
        // A lone CR, which no buffer in this process may hold and which the swap
        // codec returns byte-identical on purpose.
        seed_swap(
            dir.path(),
            &header(doc_id, None, b"on disk"),
            "alpha\rbeta\r\ngamma",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        assert_eq!(
            tab.editor_text(),
            "alpha\nbeta\r\ngamma",
            "the buffer's lone CR is repaired and the CRLF is left alone"
        );
        assert_eq!(
            *tab.source(),
            "alpha\nbeta\r\ngamma",
            "and so is the source every derived view renders from — this is the half \
                 that used to keep the whole suite green while the preview was broken"
        );
    });
}

/// The `.swap` files filed under `stem` in the (test-redirected) swap directory.
///
/// Filtered by stem as a second line of defence: `with_state_home_for_test` discards
/// and closes every window a test opened before lifting its redirect.
fn swaps_left(state_home: &std::path::Path, stem: &str) -> Vec<std::path::PathBuf> {
    let dir = state_home.join("scribobulate").join("swap");
    let prefix = format!("{stem}-");
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "swap"))
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(&prefix))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// **TDD 22.19: a snapshot identical to the file holds nothing to recover, and is
/// removed rather than carried into every later launch.**
///
/// The field failure: a snapshot equal to the file was applied, which left the tab
/// clean, and a clean tab is never saved, discarded or edited back — the three routes
/// that delete a snapshot. It was re-applied on every launch across ordinary quits, and
/// once the file was changed by another program it put the old text back over the new
/// as "recovered unsaved changes".
///
/// Both arrival routes, because they fail differently: the restored tab kept the
/// snapshot, and the reopen route also opened a tab for a document nobody had asked for.
#[gtktest::test]
fn a_snapshot_identical_to_the_file_is_removed_not_recovered() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec13");
        let restored_doc = dir.path().join("restored.md");
        let reopened_doc = dir.path().join("elsewhere.md");
        std::fs::write(&restored_doc, "on disk").unwrap();
        std::fs::write(&reopened_doc, "also on disk").unwrap();

        let win = new_window(&app, "IT", "on disk", Some(&restored_doc));
        let tab = winstate::state(&win).expect("a tab");
        let restored_id = DocId::generate();
        tab.adopt_doc_id(restored_id.clone());
        let before = winstate::tabs_for_window(&win).len();

        seed_swap(
            dir.path(),
            &header(restored_id, Some(&restored_doc), b"on disk"),
            "on disk",
        );
        seed_swap(
            dir.path(),
            &header(DocId::generate(), Some(&reopened_doc), b"also on disk"),
            "also on disk",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        for stem in ["restored", "elsewhere"] {
            assert_eq!(
                swaps_left(dir.path(), stem),
                Vec::<std::path::PathBuf>::new(),
                "nothing is left to be recovered again on the next launch"
            );
        }
        assert_eq!(
            winstate::tabs_for_window(&win).len(),
            before,
            "no tab is opened for a document with nothing to recover"
        );
        assert!(!tab.is_dirty(), "the restored tab is still clean");
        assert!(
            !tab.chrome()
                .status
                .borrow()
                .label_text()
                .contains("Recovered"),
            "and nothing is announced as recovered"
        );

        // The next launch, after another program changed the file: nothing may come
        // back over it.
        std::fs::write(&restored_doc, "changed elsewhere").unwrap();
        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));
        assert_eq!(tab.editor_text(), "on disk", "no stale content was applied");
        assert!(
            !tab.pending_external.get(),
            "and no external-change conflict was raised by a recovery"
        );
    });
}

/// TDD 22.19's second half: a snapshot that only becomes equal to the file once it is
/// APPLIED — here a lone CR the buffer repairs into the file's newline — leaves the tab
/// clean, and that tab's snapshot must go too.
///
/// Raw byte comparison cannot catch this one, so it is what pins the ordering inside
/// the apply: the tab has to know a snapshot sits on disk under its name BEFORE the
/// invariant runs, or the invariant's delete arm believes there is nothing to remove.
#[gtktest::test]
fn a_recovery_that_leaves_the_tab_clean_removes_its_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec14");
        let doc = dir.path().join("notes.md");
        std::fs::write(&doc, "alpha\nbeta").unwrap();
        let win = new_window(&app, "IT", "alpha\nbeta", Some(&doc));
        let tab = winstate::state(&win).expect("a tab");
        let doc_id = DocId::generate();
        tab.adopt_doc_id(doc_id.clone());
        seed_swap(
            dir.path(),
            &header(doc_id, Some(&doc), b"alpha\nbeta"),
            "alpha\rbeta",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        assert!(
            !tab.is_dirty(),
            "precondition: the applied content is the file's"
        );
        assert_eq!(
            swaps_left(dir.path(), "notes"),
            Vec::<std::path::PathBuf>::new(),
            "a clean tab keeps no snapshot"
        );
        assert!(
            !tab.chrome()
                .status
                .borrow()
                .label_text()
                .contains("Recovered"),
            "and a recovery that brought nothing unsaved back is not announced"
        );
    });
}

/// The same clean recovery on a STALE baseline (the file changed after the snapshot)
/// raises no external-change conflict: nothing unsaved came back, so there is nothing
/// to reconcile.
///
/// Mutation: move the `came_back` return in `apply_recovered_content` below the stale branch
/// and the conflict flag is raised for a clean tab.
#[gtktest::test]
fn a_clean_recovery_on_a_stale_baseline_raises_no_conflict() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec14s");
        let doc = dir.path().join("notes.md");
        std::fs::write(&doc, "alpha\nbeta").unwrap();
        let win = new_window(&app, "IT", "alpha\nbeta", Some(&doc));
        let tab = winstate::state(&win).expect("a tab");
        let doc_id = DocId::generate();
        tab.adopt_doc_id(doc_id.clone());
        seed_swap(
            dir.path(),
            &header(doc_id, Some(&doc), b"an older version of the file"),
            "alpha\rbeta",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        assert!(
            !tab.is_dirty(),
            "precondition: the applied content is the file's"
        );
        assert!(
            !tab.pending_external.get(),
            "a recovery that left the tab clean must not raise a conflict prompt"
        );
    });
}

/// A recovery is written as a load, not an edit: one Ctrl+Z cannot revert the recovered
/// work to the file (the way back is Discard recovery), and the caret starts at the top
/// like every other load.
///
/// Mutation: write the buffer with a raw `set_text` again and `can_undo` turns true.
#[gtktest::test]
fn a_recovery_is_a_load_that_undo_cannot_revert() {
    use gtk::prelude::TextBufferExt;
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.recundo");
        let doc = dir.path().join("notes.md");
        std::fs::write(&doc, "on disk").unwrap();
        let win = new_window(&app, "IT", "on disk", Some(&doc));
        let tab = winstate::state(&win).expect("a tab");
        let doc_id = DocId::generate();
        tab.adopt_doc_id(doc_id.clone());
        seed_swap(
            dir.path(),
            &header(doc_id, Some(&doc), b"on disk"),
            "recovered work\nline two",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        assert_eq!(
            tab.editor_text(),
            "recovered work\nline two",
            "precondition: recovered"
        );
        assert!(
            !tab.editor_buf.can_undo(),
            "Undo must not revert a recovery to the file"
        );
        let caret = tab.editor_buf.iter_at_mark(&tab.editor_buf.get_insert());
        assert_eq!(
            caret.offset(),
            0,
            "the caret starts at the top, as on every load"
        );
    });
}

/// TDD 22.6: a snapshot the session never restored is recovered anyway.
///
/// The rubric that makes the header authoritative rather than advisory. Reversing the
/// two — session first, header as confirmation — would silently discard exactly this
/// document, and nothing else in the suite would notice.
#[gtktest::test]
fn a_snapshot_no_restored_tab_claims_is_still_recovered() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec2");
        let win = new_window(&app, "IT", "unrelated", None);
        let before = winstate::tabs_for_window(&win).len();

        // A document with an identity no tab in the session carries.
        seed_swap(
            dir.path(),
            &header(DocId::generate(), None, b""),
            "orphaned but unsaved",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        let tabs = winstate::tabs_for_window(&win);
        assert_eq!(tabs.len(), before + 1, "a tab was opened for it");
        assert!(
            tabs.iter()
                .any(|t| t.editor_text() == "orphaned but unsaved"),
            "its content came back: {:?}",
            tabs.iter().map(|t| t.editor_text()).collect::<Vec<_>>()
        );
    });
}

/// **TDD 22.17: reopening the crashed document by name recovers into THAT tab, not a
/// second one.**
///
/// The scenario is the ordinary post-crash reopen and not a contrived one: the user
/// double-clicks the file in Explorer, or types `scribobulate notes.md`. That path
/// mints a **fresh** `DocId`, so the snapshot — filed under the id the crashed tab
/// had — correlates with nothing the session restored, and recovery used to open a
/// second tab for the same file: one clean, one carrying the work, with no way for the
/// user to tell which was which beyond clicking both.
///
/// The tab count is the assertion that matters. Content coming back was never the
/// broken half — it came back into the *wrong* tab, which is why every existing test
/// here passes against the defect.
///
/// Mutation: passing `None` for `disposition`'s `tab_at_same_path` restores the two
/// tabs and fails the first assertion.
#[gtktest::test]
fn reopening_the_crashed_document_by_path_recovers_into_the_tab_already_showing_it() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec11");
        let doc = dir.path().join("notes.md");
        std::fs::write(&doc, "on disk").unwrap();

        // The tab the user got by opening the file again. `new_window` mints its own
        // identity, exactly as the `open` handler does — which is the whole defect.
        let win = new_window(&app, "IT", "on disk", Some(&doc));
        let tab = winstate::state(&win).expect("a tab");
        let opened_as = tab.doc_id();
        let before = winstate::tabs_for_window(&win).len();

        // The snapshot the crashed run left behind, under the id THAT tab had.
        let crashed_as = DocId::generate();
        assert_ne!(
            crashed_as, opened_as,
            "precondition: the reopened tab carries a different identity, which is \
                 what makes identity alone unable to correlate them"
        );
        seed_swap(
            dir.path(),
            &header(crashed_as.clone(), Some(&doc), b"on disk"),
            "on disk, plus work that was never saved",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        assert_eq!(
            winstate::tabs_for_window(&win).len(),
            before,
            "one document, one tab: {:?}",
            winstate::tabs_for_window(&win)
                .iter()
                .map(|t| t.path.borrow().clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            tab.editor_text(),
            "on disk, plus work that was never saved",
            "and the work came back into the tab the user is looking at"
        );
        assert!(tab.is_dirty(), "still dirty against what is on disk");
        assert_eq!(
            tab.doc_id(),
            crashed_as,
            "the tab takes on the identity the document has been filed under, so the \
                 re-armed snapshot supersedes the recovered file instead of orphaning it"
        );
    });
}

/// **TDD 22.16: two snapshots for one path are two documents — the second must not
/// steal the first's tab.**
///
/// Reachable through two `--new-instance` processes both holding one file dirty. The
/// duplicate-tab fix must not become a data-loss fix in the other direction: applying
/// the second snapshot over the first would silently destroy recovered work, which is
/// strictly worse than the extra tab it was introduced to remove.
#[gtktest::test]
fn a_second_snapshot_for_one_path_gets_its_own_tab_rather_than_overwriting_the_first() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec12");
        let doc = dir.path().join("contended.md");
        std::fs::write(&doc, "on disk").unwrap();

        let win = new_window(&app, "IT", "on disk", Some(&doc));
        let before = winstate::tabs_for_window(&win).len();

        seed_swap(
            dir.path(),
            &header(DocId::generate(), Some(&doc), b"on disk"),
            "work from instance one",
        );
        seed_swap(
            dir.path(),
            &header(DocId::generate(), Some(&doc), b"on disk"),
            "work from instance two",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        let texts: Vec<String> = winstate::tabs_for_window(&win)
            .iter()
            .map(|t| t.editor_text())
            .collect();
        assert_eq!(
            winstate::tabs_for_window(&win).len(),
            before + 1,
            "the first snapshot adopts the open tab, the second opens its own: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "work from instance one"),
            "neither buffer may be lost: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "work from instance two"),
            "neither buffer may be lost: {texts:?}"
        );
    });
}

/// **TDD 22.16 after a bare relaunch**: the session restored the shared file's tab under
/// ONE of the two snapshots' identities, and each snapshot still gets a tab of its own.
///
/// Distinct from the test above, where the open tab matched neither snapshot by identity.
/// Here one snapshot matches by identity and the other only by path, and the scan visits
/// them in whatever order the directory yields. Run with the restored identity on each
/// side in turn so both visiting orders are exercised whatever that order is.
fn each_snapshot_gets_its_own_tab_when_the_session_restored_one_of_them(
    app_id: &str,
    restored_is_first: bool,
) {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app = super::super::gtk_integration_tests::test_app(app_id);
        let doc = dir.path().join("shared.md");
        std::fs::write(&doc, "on disk").unwrap();

        let win = new_window(&app, "IT", "on disk", Some(&doc));
        let tab = winstate::state(&win).expect("a tab");
        let before = winstate::tabs_for_window(&win).len();

        let one = DocId::generate();
        let two = DocId::generate();
        tab.adopt_doc_id(if restored_is_first { &one } else { &two }.clone());
        seed_swap(
            dir.path(),
            &header(one, Some(&doc), b"on disk"),
            "work from instance one",
        );
        seed_swap(
            dir.path(),
            &header(two, Some(&doc), b"on disk"),
            "work from instance two",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        let texts: Vec<String> = winstate::tabs_for_window(&win)
            .iter()
            .map(|t| t.editor_text())
            .collect();
        assert_eq!(
            winstate::tabs_for_window(&win).len(),
            before + 1,
            "one snapshot goes into the restored tab, the other opens its own: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "work from instance one"),
            "neither buffer may be hidden: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "work from instance two"),
            "neither buffer may be hidden: {texts:?}"
        );
    });
}

#[gtktest::test]
fn both_snapshots_get_tabs_when_the_session_restored_the_first_ones_identity() {
    each_snapshot_gets_its_own_tab_when_the_session_restored_one_of_them(
        "com.extollit.scribobulate.it.rec15a",
        true,
    );
}

#[gtktest::test]
fn both_snapshots_get_tabs_when_the_session_restored_the_second_ones_identity() {
    each_snapshot_gets_its_own_tab_when_the_session_restored_one_of_them(
        "com.extollit.scribobulate.it.rec15b",
        false,
    );
}

/// **TDD 22.16 on a bare relaunch with no session at all**: nothing restored the shared
/// file, so neither snapshot matches a tab by identity, and each must still open a tab of
/// its own.
///
/// The case the two tests above do not reach. There the file already has a tab when the
/// pass starts; here the only tab is an unrelated untitled one (the welcome tab a
/// session-less launch shows), so the FIRST snapshot opens the file's tab itself. The
/// path fallback then found that brand-new tab for the SECOND snapshot, which was never
/// recorded as claimed, and applied the second over the first: one tab showing whichever
/// edit landed last, the other edit gone, and the status bar still counting two
/// recoveries. Scan order does not matter — either snapshot going first loses the other.
///
/// Mutation: drop the claim in `recover_after_restore`'s reopen arm and the tab count
/// comes back one short, with only one of the two texts present.
#[gtktest::test]
fn each_snapshot_opens_its_own_tab_when_no_session_restored_the_file() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec15c");
        let doc = dir.path().join("shared.md");
        std::fs::write(&doc, "on disk").unwrap();

        let win = new_window(&app, "IT", "welcome", None);
        let before = winstate::tabs_for_window(&win).len();

        seed_swap(
            dir.path(),
            &header(DocId::generate(), Some(&doc), b"on disk"),
            "work from instance one",
        );
        seed_swap(
            dir.path(),
            &header(DocId::generate(), Some(&doc), b"on disk"),
            "work from instance two",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        let texts: Vec<String> = winstate::tabs_for_window(&win)
            .iter()
            .map(|t| t.editor_text())
            .collect();
        assert_eq!(
            winstate::tabs_for_window(&win).len(),
            before + 2,
            "each snapshot opens a tab of its own: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "work from instance one"),
            "neither buffer may be overwritten: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "work from instance two"),
            "neither buffer may be overwritten: {texts:?}"
        );
    });
}

/// TDD 22.10: an unrelated file in the recovery location is neither parsed nor
/// removed.
///
/// The state directory is shared, so the scan must never become a file shredder.
/// Asserted on the file's *survival* as well as on the absence of a recovery, because
/// those are different failures and only one of them is destructive.
#[gtktest::test]
fn a_foreign_file_is_left_exactly_as_it_was_found() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec3");
        let win = new_window(&app, "IT", "unrelated", None);
        let before = winstate::tabs_for_window(&win).len();

        let swap_dir = dir.path().join("scribobulate").join("swap");
        std::fs::create_dir_all(&swap_dir).unwrap();
        let foreign = swap_dir.join("somebody-elses.swap");
        std::fs::write(&foreign, b"# not ours at all\n").unwrap();

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        assert!(foreign.exists(), "the foreign file must not be deleted");
        assert_eq!(
            std::fs::read(&foreign).unwrap(),
            b"# not ours at all\n",
            "nor modified"
        );
        assert_eq!(
            winstate::tabs_for_window(&win).len(),
            before,
            "and it must not be recovered into a tab"
        );
    });
}

/// TDD 22.14: a snapshot a confirmed-live instance owns is left alone.
#[gtktest::test]
fn a_snapshot_left_by_our_own_pid_is_still_recovered() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec4");
        let win = new_window(&app, "IT", "unrelated", None);
        let before = winstate::tabs_for_window(&win).len();

        let mut h = header(DocId::generate(), None, b"");
        // The only pid this test can assert liveness for portably is its own — and
        // `owner_is_live` treats it as NOT live, because a snapshot this very process
        // wrote is one it should reclaim rather than skip. The live-SIBLING case is
        // covered as pure data in `swapfile::recovery`; what is worth pinning HERE is
        // that self-owned data is still recovered, since the opposite reading would
        // silently disable recovery for every ordinary single-instance crash.
        h.owner_pid = std::process::id();
        seed_swap(dir.path(), &h, "self-owned work");

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        assert_eq!(
            winstate::tabs_for_window(&win).len(),
            before + 1,
            "a snapshot left by this pid is still the user's work and must come back"
        );
    });
}

/// TDD 22.9: the recovery is applied first and reversible second.
///
/// Pins the operator's stated shape of the discard action — *revert from disk, and
/// let the invariant remove the recovery data* — rather than a bespoke deletion. The
/// assertion that matters is the second one: reverting alone must be sufficient, so a
/// future refactor that adds an explicit delete here is adding a second deletion path
/// (GTK4Rs/AP-108/GEP-25) and this test would keep passing while that rot set in — so
/// it deliberately never calls a delete itself.
#[gtktest::test]
fn discarding_a_recovery_reverts_the_tab_and_clears_its_recovery_data() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let file = dir.path().join("notes.md");
        std::fs::write(&file, "on disk").unwrap();

        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec5");
        let win = new_window(&app, "IT", "on disk", Some(&file));
        let tab = winstate::state(&win).expect("a tab");
        crate::app::attach_file_backing(&win, &tab, file.clone());

        let doc_id = DocId::generate();
        tab.adopt_doc_id(doc_id.clone());
        seed_swap(
            dir.path(),
            &header(doc_id, Some(&file), b"on disk"),
            "on disk, plus unsaved work",
        );
        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));
        assert!(
            tab.is_dirty(),
            "precondition: the recovery landed and is dirty"
        );
        assert!(
            tab.recovered_at.get().is_some(),
            "precondition: the tab carries an outstanding recovery notice"
        );

        // Exactly what "Discard recovery" does, and nothing else.
        super::reload::reload_from_disk(&win);
        assert!(
            crate::docio::settle(|| !tab.is_dirty()),
            "the reload must land: it reads the file off the main thread now"
        );

        assert_eq!(tab.editor_text(), "on disk", "the tab reverted to the file");
        assert!(!tab.is_dirty(), "and is clean again");
        // THIS document's snapshot, not "the swap directory is empty".
        //
        // The directory-wide form was over-broad and only ever passed by accident of
        // scheduling. The claim under test is about the tab that was reverted, but
        // the assertion was about global state this test does not own — and it held
        // only while nothing else could run in between. Once the revert began
        // pumping the main loop (the reload's read is off-thread now), other tests'
        // still-armed snapshot timers got their chance to fire, and they write into
        // whichever state home `with_state_home_for_test` currently has installed —
        // this one. Measured on Windows: `untitled-<uuid>.swap` files, plus
        // `.swap.swap.tmp` siblings from writes still in flight, none of them this
        // document's. Order-dependent, so it passed alone and failed in the suite.
        //
        // Naming the file makes the assertion say what the test means and stops it
        // reporting somebody else's litter as this feature being broken.
        let ours = crate::swapfile::swap_path(Some(&file), &tab.doc_id())
            .expect("a swap path resolves under the test state home");
        assert!(
            !ours.exists(),
            "reverting alone must clear THIS document's recovery data, with no \
                 bespoke deletion — the invariant is the mechanism: {ours:?} survived"
        );
    });
}

/// **Derived-view CAM row 8, column B** — saving a recovered tab retires its notice.
///
/// The defect this pins is not cosmetic. The notice's action is "Discard recovery",
/// which reverts the tab to what is on disk; left standing after a save it would
/// throw away work the user had just committed, while its label went on describing a
/// recovery that no longer bears on what they are looking at. Found by walking the
/// CAM's persistence column, not by any failing happy-path test — which is the whole
/// argument for the matrix.
#[gtktest::test]
fn saving_a_recovered_tab_retires_its_recovery_notice() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let file = dir.path().join("notes.md");
        std::fs::write(&file, "on disk").unwrap();

        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec6");
        let win = new_window(&app, "IT", "on disk", Some(&file));
        let tab = winstate::state(&win).expect("a tab");
        crate::app::attach_file_backing(&win, &tab, file.clone());

        let doc_id = DocId::generate();
        tab.adopt_doc_id(doc_id.clone());
        seed_swap(
            dir.path(),
            &header(doc_id, Some(&file), b"on disk"),
            "on disk, plus unsaved work",
        );
        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));
        assert!(
            tab.recovered_at.get().is_some() && tab.chrome().recovery_toast.is_visible(),
            "precondition: the notice is up"
        );

        // Save it — the ordinary thing a user does with recovered work.
        tab.saved_baseline.replace(tab.editor_text());
        refresh_dirty_status(&win);

        assert!(
            tab.recovered_at.get().is_none(),
            "a saved document is no longer 'recovered but unsaved'"
        );
        assert!(
            !tab.chrome().recovery_toast.is_visible(),
            "and its notice — whose action would now revert the saved work — is gone"
        );
    });
}

/// **Derived-view CAM row 8, column D** — the notice follows its tab across a switch.
///
/// The widget is window-shared while the fact it reports is per document, so the two
/// only stay in step if every host change re-derives it from whichever tab is now
/// active. A recovered tab sitting in the background must not leak its notice onto an
/// unrecovered one, and must get it back when the user returns to it.
#[gtktest::test]
fn the_recovery_notice_follows_the_active_tab() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec7");
        let win = new_window(&app, "IT", "first", None);
        let recovered = winstate::state(&win).expect("a tab");
        let doc_id = DocId::generate();
        recovered.adopt_doc_id(doc_id.clone());
        seed_swap(dir.path(), &header(doc_id, None, b""), "recovered work");
        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));
        assert!(
            recovered.chrome().recovery_toast.is_visible(),
            "precondition: the notice is up on the recovered tab"
        );

        // A second, ordinary tab — switching to it must take the notice away.
        let other = create_tab_in_window(&win, "unrelated", None, false, false)
            .and_then(winstate::tab_by_id)
            .expect("a second tab");
        assert!(
            !other.chrome().recovery_toast.is_visible(),
            "an unrecovered tab must not inherit another tab's recovery notice"
        );

        // …and switching back must bring it back, not leave the user with no way to
        // answer it ("it corrects itself later" is a CAM fail).
        if let Some(chrome) = winstate::chrome(&win) {
            chrome.tabs.focus_page(&recovered.content_box);
        }
        assert!(
            recovered.chrome().recovery_toast.is_visible(),
            "returning to the recovered tab restores its notice"
        );
    });
}

/// **The preview must show the recovered text, not the pre-crash file.**
///
/// This asserts `source` — the text every *derived* view renders from — and not
/// `editor_text()`, which is the editor buffer. That distinction is the entire
/// point of the test: the bug it pins shipped through 856 green tests precisely
/// because every existing assertion read the editor buffer, which was correct,
/// while the preview rendered stale on-disk content. A user working in Preview
/// mode — this application's *default* mode — would have seen the recovery
/// silently do nothing.
///
/// Found on a live display, not headlessly, and the reason is worth
/// keeping: a headless suite can only catch what its assertions point at, so
/// aiming them all at one surface makes the suite's greenness evidence about that
/// surface alone (GTK4Rs/AP-78). Mutation-tested.
#[gtktest::test]
fn a_recovery_reaches_the_derived_views_not_only_the_editor_buffer() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec8");
        let win = new_window(&app, "IT", "on disk", None);
        let tab = winstate::state(&win).expect("a tab");
        let doc_id = DocId::generate();
        tab.adopt_doc_id(doc_id.clone());
        assert!(
            tab.heading_index.borrow().is_empty(),
            "precondition: the document on disk has no headings"
        );
        seed_swap(
            dir.path(),
            &header(doc_id, None, b"on disk"),
            "on disk\n\n# Recovered heading\n",
        );

        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        assert_eq!(
            *tab.source(),
            "on disk\n\n# Recovered heading\n",
            "the preview/outline/annotations all render from `source`; leaving it \
                 stale makes every projection of the document disagree with the editor"
        );
        assert_eq!(
            *tab.source(),
            tab.editor_text(),
            "and the two must not be allowed to drift apart in the first place"
        );
        assert_eq!(
            tab.heading_index.borrow().len(),
            1,
            "the outline of the active tab is rebuilt from the recovered text, not left \
                 describing the file on disk"
        );
    });
}
/// **The startup sweep clears our own stray temps — and nothing else.**
///
/// A `<name>.swap.tmp` that outlived its process is an incomplete write by
/// definition, with no way to tell a truncated one from a whole one, so it is deleted
/// outright. The rest of the assertion is the important half: the sweep is the *only*
/// deletion the scan performs, and it must not generalise. A foreign `.tmp` belonging
/// to another tool, a foreign `.swap`, and a damaged-but-ours `.swap` all survive —
/// the last because it may be the only remaining copy of the user's work.
#[gtktest::test]
fn the_sweep_removes_our_stray_temps_and_leaves_everything_else_alone() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let swap_dir = dir.path().join("scribobulate").join("swap");
        std::fs::create_dir_all(&swap_dir).unwrap();

        let ours_stray = swap_dir.join("notes-aaaa.swap.tmp");
        let someone_elses_tmp = swap_dir.join("unrelated-tool.tmp");
        let foreign_swap = swap_dir.join("notmine.swap");
        let damaged_ours = swap_dir.join("torn-bbbb.swap");
        std::fs::write(&ours_stray, b"half a snapshot").unwrap();
        std::fs::write(&someone_elses_tmp, b"not ours").unwrap();
        std::fs::write(&foreign_swap, b"# not ours either\n").unwrap();
        std::fs::write(&damaged_ours, b"+++scribobulate-swap 1\nno closing fence").unwrap();

        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.rec9");
        let _win = new_window(&app, "IT", "unrelated", None);
        gtk::glib::MainContext::default().block_on(recover_after_restore(&app));

        assert!(
            !ours_stray.exists(),
            "an incomplete temp of ours is swept — it can never be anything but garbage"
        );
        assert!(
            someone_elses_tmp.exists(),
            "a `.tmp` that is NOT ours must be untouched — the state directory is shared \
                 and this must never become a general file shredder"
        );
        assert!(foreign_swap.exists(), "a foreign .swap is never deleted");
        assert!(
            damaged_ours.exists(),
            "a DAMAGED snapshot of ours is KEPT, not swept — it may be the only \
                 surviving copy of the user's work, which is exactly why the temp case \
                 has to be recognised precisely rather than by a loose pattern"
        );
    });
}

/// **A launch carrying a file argument must recover too** — the route that shipped
/// broken.
///
/// `recover_after_restore` originally had one call site, in the bare-launch
/// (`activate`) handler. A launch with a file path dispatches to `open` instead, so
/// `scribobulate notes.md`, an Explorer double-click, a `.desktop` association and
/// `xdg-open` all silently skipped the recovery offer — i.e. the ordinary ways a user
/// reopens the document they just lost. Found by the Windows seat, in shared code, not
/// in the port.
///
/// This asserts the *effect* through the same entry point a real file launch takes,
/// rather than that a particular function was called: a future refactor is free to
/// move the call, and must not be free to drop it.
#[gtktest::test]
fn a_launch_with_a_file_argument_still_recovers() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let file = dir.path().join("opened.md");
        std::fs::write(&file, "on disk").unwrap();
        // A snapshot left by a previous, crashed run — belonging to a document the
        // incoming launch knows nothing about.
        seed_swap(
            dir.path(),
            &header(DocId::generate(), None, b""),
            "work from before the crash",
        );

        // Built the way production builds it — HANDLES_OPEN, and the real
        // `setup_app` wiring — so this exercises the actual `open` handler rather
        // than a stand-in. That is the whole point: the bug was not in the recovery
        // pass, it was in which entry points reach it.
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.it.recopen"),
            gtk::gio::ApplicationFlags::HANDLES_OPEN | gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        crate::app::setup_app(&app);
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before opening");
        assert!(
            app.windows().is_empty(),
            "precondition: a cold start, which is what gates recovery"
        );
        // The real `open` entry point, with a file argument — not `activate`.
        // `open` now reads its file off the main thread and builds the window
        // when that comes back, so the windows do not exist the instant it
        // returns.
        let _cold = crate::app::coldstart::force_for_test(true); // This launch models a COLD start — an empty process reached by a file argument,
        app.open(&[gtk::gio::File::for_path(&file)], "");
        assert!(
            crate::docio::settle(|| !app.windows().is_empty()),
            "the file-argument launch must build its window"
        );

        let recovered: Vec<String> = app
            .windows()
            .iter()
            .filter_map(|w| w.clone().downcast::<ApplicationWindow>().ok())
            .flat_map(|w| winstate::tabs_for_window(&w))
            .map(|t| t.editor_text())
            .collect();
        assert!(
            recovered.iter().any(|t| t == "work from before the crash"),
            "a file-argument launch must still offer the unsaved work back: {recovered:?}"
        );
    });
}

/// **The failure notice fires on the TRANSITION and retracts on recovery** — the two
/// halves a naive implementation gets wrong in opposite directions.
///
/// The user-visible half of TDD 22.15 (does a toast physically appear) was verified
/// once, by hand, against a real full filesystem — it needs a display and a disk that
/// will not fill on demand in CI. But the *logic* underneath it does not, and it is
/// the part most likely to rot silently: a persistent failure re-notifying every few
/// seconds trains the user to dismiss it unread, and a notice that never retracts
/// leaves them believing they are unprotected long after they are not.
///
/// Asserted on the handle rather than on the status text, because the handle IS the
/// mechanism: one outstanding notice means one push, and an unchanged handle across a
/// second failure is precisely "this was a retry, not a transition".
#[gtktest::test]
fn a_snapshot_failure_notifies_once_and_retracts_on_recovery() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.swapfail");
        let win = new_window(&app, "IT", "content", None);
        let tab = winstate::state(&win).expect("a tab");
        assert!(
            tab.swap_fail_status.get().is_none(),
            "precondition: nothing reported yet"
        );

        crate::window::report_snapshot_failure_for_test(&tab, "simulated ENOSPC");
        let first = tab.swap_fail_status.get();
        assert!(first.is_some(), "the transition into failure is reported");

        // A retry while the condition still holds must NOT notify again.
        crate::window::report_snapshot_failure_for_test(&tab, "simulated ENOSPC again");
        assert_eq!(
            tab.swap_fail_status.get(),
            first,
            "a persistent failure must report the TRANSITION, not every retry — an \
                 unchanged handle means no second notice was pushed"
        );

        crate::window::clear_snapshot_failure_for_test(&tab);
        assert!(
            tab.swap_fail_status.get().is_none(),
            "the notice retracts on the first success — leaving it up tells the user \
                 they are unprotected long after they are not"
        );

        // And it can report again after a genuine recovery-then-failure cycle.
        crate::window::report_snapshot_failure_for_test(&tab, "failed again later");
        assert!(
            tab.swap_fail_status.get().is_some(),
            "a NEW transition after a recovery is a new notice, not suppressed"
        );
    });
}

/// **A failure notice must not outlive the document it is about.**
///
/// Two ways it could, both found by inspection after the live check passed — which
/// is the point: the happy path (fail, recover, retract) was verified end-to-end on a
/// real full filesystem and neither of these is on it.
///
/// 1. **Closing a tab mid-failure.** The tab is destroyed, so nothing can ever call
///    the retraction on it, and the window reports "not being backed up" forever for
///    a document that no longer exists.
/// 2. **Moving a tab between windows.** The handle belongs to the ORIGIN window's
///    status stack; popping it against the destination's matches nothing and leaves
///    the origin's notice up permanently, with no error anywhere — the exact failure
///    `StatusCtx`'s own doc comment describes, which its newtype cannot prevent
///    because the hazard is the wrong *stack*, not the wrong *id type*.
#[gtktest::test]
fn a_failure_notice_does_not_outlive_its_document() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app =
            super::super::gtk_integration_tests::test_app("com.extollit.scribobulate.it.failleak");

        // (1) closed while failing
        let win = new_window(&app, "IT", "content", None);
        let tab = winstate::state(&win).expect("a tab");
        crate::window::report_snapshot_failure_for_test(&tab, "ENOSPC");
        assert!(
            tab.swap_fail_status.get().is_some(),
            "precondition: reported"
        );
        crate::window::discard_tab_swap(&tab);
        assert!(
            tab.swap_fail_status.get().is_none(),
            "a tab going away must retract its notice — nothing can retract it \
                 afterwards, so the window would report it forever"
        );

        // (2) moved between windows while failing
        let win_b = new_window(&app, "IT-B", "other", None);
        let tab_b = winstate::state(&win_b).expect("a tab");
        crate::window::report_snapshot_failure_for_test(&tab_b, "ENOSPC");
        assert!(
            tab_b.swap_fail_status.get().is_some(),
            "precondition: reported"
        );
        let dest = winstate::chrome(&win).expect("destination chrome");
        tab_b.set_chrome(dest);
        assert!(
            tab_b.swap_fail_status.get().is_none(),
            "re-homing must retract against the window being LEFT — its handle is \
                 meaningless in the destination's stack, so a later pop would silently \
                 no-op and strand the notice in the origin"
        );
    });
}
