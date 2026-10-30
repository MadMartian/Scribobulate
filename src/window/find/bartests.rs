//! The find bar driven as a reader drives it, for the match options.
//!
//! `parity` proves the two ENGINES agree over raw text. This file proves the two PANES
//! agree in a running window — which is a different claim, because between a reader
//! ticking a toggle and a number appearing sit the action, the tab's own state, the
//! editor's `SearchSettings`, the preview's hit cache and the readout. Every one of
//! those is a place where an option can reach one pane and not the other, and the shape
//! that failure takes on screen is a count that changes when you switch view mode.
//!
//! Its own module rather than more cases inside `window/find.rs`'s test block: these
//! drive the BAR, not the search, and the file they would otherwise join is already the
//! largest in the tree.

use super::super::testkit::test_app;
use super::testkit::{readout, readout_after_quiet, search, type_query};
use crate::winstate::state;
use gtk::prelude::*;
use gtk::ApplicationWindow;

/// Four occurrences of the term in body text, three in a table cell and three inside a
/// collapsed disclosure — the preview's three separate search paths, each carrying the
/// same mixture of cases and one `notebook` that only a whole-word search rejects.
///
/// The counts the tests assert are derived from this fixture and are written out rather
/// than computed, because a count computed by the same matcher under test would agree
/// with it however wrong both were.
const MD: &str = "\
note Note NOTE notebook

| Head |
|---|
| note Note notebook |

<details>
<summary>Folded</summary>

note Note notebook

</details>
";

/// Every occurrence of `note`, case-insensitively and not whole-word:
/// 4 in the body, 3 in the cell, 3 in the collapsed body.
const ALL: i32 = 10;
/// Case-sensitive: the lowercase `note` and the `note` inside `notebook`, per source.
const CASE_EXACT: i32 = 6;
/// Whole word: everything except the three `notebook`s.
const WHOLE: i32 = 7;

/// The readout's count, whatever wording it is wrapped in. `None` when the readout is
/// not reporting a count at all.
fn count(win: &ApplicationWindow) -> Option<i32> {
    let text = readout(win);
    if text == "No matches" {
        return Some(0);
    }
    text.split_whitespace()
        .next_back()
        .and_then(|n| n.parse().ok())
        .or_else(|| text.split_whitespace().next().and_then(|n| n.parse().ok()))
}

fn set_option(win: &ApplicationWindow, name: &str, on: bool) {
    crate::window::actions::change_action_state(win, name, &on.to_variant());
}

fn set_mode(win: &ApplicationWindow, mode: &str) {
    win.change_action_state("view-mode", &mode.to_variant());
    crate::testpump::drain_for(
        crate::testpump::Clock::Frame,
        std::time::Duration::from_millis(200),
    );
}

/// **The claim, end to end: an option means the same thing in whichever pane is
/// visible** (TDD 11.13).
///
/// The fixture puts matches in all three of the preview's search paths — body text, a
/// table cell's `GtkLabel`, and a collapsed disclosure that this render did not draw —
/// because those used to be three hard-wired matchers. An option reaching one of them
/// and not the others is a count that changes when the reader switches view mode, and
/// it is invisible on a fixture whose matches are all in one place.
///
/// Mutation check: drop the `set_case_sensitive` line from
/// `findbar::push_options_to_engine` and the editor legs fail while every preview leg
/// stays green — which is exactly the half-wired shape this exists to catch.
#[gtktest::test]
fn a_match_option_counts_the_same_in_either_pane() {
    let app = test_app("com.extollit.scribobulate.integrationtest.findoptions");
    let win = crate::window::new_window(&app, "IT-findoptions", MD, None);

    for mode in ["edit", "preview"] {
        set_mode(&win, mode);
        search(&win, "note");
        assert_eq!(
            count(&win),
            Some(ALL),
            "{mode}: the default is the case-insensitive literal the bar has always done"
        );

        set_option(&win, "find-match-case", true);
        assert_eq!(
            count(&win),
            Some(CASE_EXACT),
            "{mode}: match case must drop the capitalised occurrences"
        );
        set_option(&win, "find-match-case", false);

        set_option(&win, "find-whole-word", true);
        assert_eq!(
            count(&win),
            Some(WHOLE),
            "{mode}: whole word must reject the occurrences inside `notebook`"
        );
        set_option(&win, "find-whole-word", false);

        assert_eq!(
            count(&win),
            Some(ALL),
            "{mode}: unticking an option must restore the count it changed"
        );
    }
    win.destroy();
}

/// A regular expression reaches both panes, and reaches the two preview paths that are
/// not the buffer (TDD 11.14).
///
/// `^note` is chosen deliberately: an anchor is where a compile-flag difference between
/// the engines becomes visible, and it cannot match inside `notebook` either, so the
/// count says something about the pattern rather than only about the option being read.
#[gtktest::test]
fn a_regular_expression_reaches_both_panes() {
    let app = test_app("com.extollit.scribobulate.integrationtest.findregex");
    let win = crate::window::new_window(&app, "IT-findregex", MD, None);

    let mut counts = Vec::new();
    for mode in ["edit", "preview"] {
        set_mode(&win, mode);
        search(&win, "note");
        set_option(&win, "find-regex", true);
        type_query(&win, r"note\w*");
        let wildcard = count(&win);
        type_query(&win, "notebook|NOTE");
        let alternation = count(&win);
        counts.push((wildcard, alternation));
        set_option(&win, "find-regex", false);
    }
    let editor = counts[0];
    let preview = counts[1];
    assert_eq!(
        editor, preview,
        "the same pattern counted differently in the two panes ({editor:?} vs {preview:?}) \
         — they must compile it with the same engine"
    );
    assert_eq!(
        editor.0,
        Some(ALL),
        "`note\\w*` must match every occurrence, `notebook` included"
    );
    assert!(
        editor.1.is_some_and(|n| n > 0),
        "an alternation must match something; it counted {:?}",
        editor.1
    );
    win.destroy();
}

/// **A pattern that does not compile is reported, not answered** (TDD 11.15).
///
/// Both panes, because both have their own way of failing to compile — the editor's
/// engine reports a `regex-error` and the preview's matcher returns one — and a readout
/// that only knows about one of them is silently wrong in the other pane.
///
/// Next/Prev doing nothing is asserted with the rest: there is no list to step, and
/// stepping the previous pattern's list would move the reader through matches that are
/// not what the readout describes.
#[gtktest::test]
fn a_malformed_pattern_is_reported_rather_than_counted_as_zero() {
    let app = test_app("com.extollit.scribobulate.integrationtest.findbadregex");
    let win = crate::window::new_window(&app, "IT-findbadregex", MD, None);

    for mode in ["edit", "preview"] {
        set_mode(&win, mode);
        search(&win, "note");
        set_option(&win, "find-regex", true);
        let st = state(&win).expect("a tab");
        type_query(&win, "(unclosed");
        assert_eq!(
            readout(&win),
            "Invalid pattern",
            "{mode}: a malformed pattern must say so — `No matches` reports on the \
             document when what happened is that nobody was able to ask"
        );
        assert!(
            st.chrome().match_count_label.tooltip_text().is_some(),
            "{mode}: the engine's own diagnostic must be reachable — a fixed \
             `Invalid pattern` with nothing behind it leaves the reader with no way to \
             find out WHICH construct it objected to"
        );

        st.find_cursor.set(crate::window::FindCursor::None);
        // What the Next chevron, Enter in the field and Edit ▸ Find Next all call.
        super::find_step(&win, &st.search_context, super::SearchDir::Forward);
        assert_eq!(
            readout_after_quiet(&win),
            "Invalid pattern",
            "{mode}: Next must do nothing while the pattern does not compile"
        );
        assert_eq!(
            st.find_cursor.get(),
            crate::window::FindCursor::None,
            "{mode}: Next must not land on a match of the PREVIOUS pattern"
        );

        // Finishing the pattern recovers in place — no closing and reopening the bar.
        type_query(&win, "(unclosed)?note");
        assert_eq!(
            count(&win),
            Some(ALL),
            "{mode}: completing the pattern must recover without reopening the bar"
        );
        set_option(&win, "find-regex", false);
    }
    win.destroy();
}

/// A tab's options travel with it, and every control that shows them follows (TDD
/// 15.12).
///
/// The two halves are asserted together on purpose: the behaviour reverting while the
/// toggles stay put, and the toggles reverting while the behaviour stays put, are both
/// real and each looks fine from the other's vantage point. This is the
/// `show-unsafe-images` lying-mirror defect's shape, in a second place.
#[gtktest::test]
fn a_tab_keeps_its_own_match_options() {
    let app = test_app("com.extollit.scribobulate.integrationtest.findtabopts");
    let win = crate::window::new_window(&app, "IT-findtabopts", MD, None);
    set_mode(&win, "preview");
    search(&win, "note");
    set_option(&win, "find-match-case", true);
    let first = state(&win).expect("a tab").id;
    // Read the count here for its WAIT, not for its value: `GtkSearchEntry` debounces
    // `search-changed`, and that handler is what records the query against this tab. A
    // tab switched away from before it has fired keeps an empty query and comes back
    // with nothing to count, which reads as "the options were lost" and is not.
    assert_eq!(count(&win), Some(CASE_EXACT));

    let second = crate::window::create_tab_in_window(&win, MD, None, false, false)
        .expect("the second tab is created");
    // The query is per tab too (§15.12), so the new tab starts with an empty field and
    // has to be given the same term before its COUNT says anything about its options.
    search(&win, "note");
    assert_eq!(
        count(&win),
        Some(ALL),
        "a fresh tab starts at the default options, not the previous tab's"
    );
    assert!(
        !option_state(&win, "find-match-case"),
        "the toggle must follow the newly active tab, not keep the other tab's tick"
    );

    crate::window::actions::change_action_state(
        &win,
        "select-tab",
        &first.to_string().to_variant(),
    );
    crate::testpump::drain_for(
        crate::testpump::Clock::Frame,
        std::time::Duration::from_millis(200),
    );
    assert_eq!(
        count(&win),
        Some(CASE_EXACT),
        "switching back must restore the tab's own options, not only its query"
    );
    assert!(
        option_state(&win, "find-match-case"),
        "…and the control that shows them must say so too"
    );
    let _ = second;
    win.destroy();
}

/// The current state of one option `GAction`, read through the action map the toggle
/// button and the Edit-menu item both read.
fn option_state(window: &ApplicationWindow, name: &str) -> bool {
    window
        .lookup_action(name)
        .and_then(|a| a.state())
        .and_then(|v| v.get::<bool>())
        .expect("the option action is registered and stateful")
}

/// Select `[start, end)` characters of whichever pane the window is showing.
fn select_range(win: &ApplicationWindow, start: i32, end: i32) {
    let st = state(win).expect("a tab");
    let buf: gtk::TextBuffer = match super::find_target(win) {
        super::FindTarget::Preview(view) => view.buffer(),
        _ => st.editor_buf.clone().upcast(),
    };
    buf.select_range(&buf.iter_at_offset(start), &buf.iter_at_offset(end));
    crate::testpump::drain_for(
        crate::testpump::Clock::Idle,
        std::time::Duration::from_millis(100),
    );
}

fn action_enabled(window: &ApplicationWindow, name: &str) -> bool {
    window
        .lookup_action(name)
        .expect("the action is registered")
        .is_enabled()
}

/// **Search in selection confines the search in the editor, and Replace All with it**
/// (TDD 11.16, 11.17).
///
/// The scope is a pair of marks precisely so it survives the replacements: the
/// assertion that it still covers the same passage AFTERWARDS is the one a pair of
/// offsets fails, and it fails silently — the first Replace All looks right and the
/// second one reaches text outside the selection.
#[gtktest::test]
fn search_in_selection_confines_the_editor_search_and_its_replacements() {
    const DOC: &str = "one target\ntwo target\nthree target\nfour target\n";
    let app = test_app("com.extollit.scribobulate.integrationtest.findscope");
    let win = crate::window::new_window(&app, "IT-findscope", DOC, None);
    set_mode(&win, "edit");
    search(&win, "target");
    assert_eq!(count(&win), Some(4), "unscoped, every occurrence counts");

    // The first two lines only: "one target\ntwo target\n" is characters 0..22.
    let bound_end = DOC.find("three").expect("the fixture has a third line") as i32;
    select_range(&win, 0, bound_end);
    assert!(
        action_enabled(&win, super::super::findbar::FIND_IN_SELECTION),
        "a live selection must make the control usable"
    );
    set_option(&win, super::super::findbar::FIND_IN_SELECTION, true);
    assert_eq!(
        count(&win),
        Some(2),
        "the count must describe only the occurrences inside the selected passage"
    );

    // Stepping wraps WITHIN the passage rather than around the document.
    let st = state(&win).expect("a tab");
    for expected in [1, 2, 1, 2] {
        super::find_step(&win, &st.search_context, super::SearchDir::Forward);
        assert_eq!(
            st.find_cursor.get(),
            crate::window::FindCursor::Editor(expected),
            "stepping must wrap inside the passage, not out of it"
        );
    }

    super::replace_all_matches(&win, &st, "TOKEN");
    let after = crate::saferizer::BufferText::of(&st.editor_buf).into_string();
    assert_eq!(
        after, "one TOKEN\ntwo TOKEN\nthree target\nfour target\n",
        "Replace All must reach only the occurrences the SEARCH covered — not because \
         Replace All is scoped, but because the search it acts on is"
    );
    // Reported in the FOOTER, not in the find bar's readout — and asserted through the
    // status stack's own composed line rather than through a widget read taken the
    // instant this returns. The readout version of this assertion was green on a
    // message no reader could see: the replacement fires the buffer's `changed`, the
    // engine re-scans, and ~60 ms later its notify repaints the readout (MEASURED by
    // the Windows seat). An in-process assertion made before that re-search cannot tell
    // a message that survives from one that is clobbered a frame later.
    let shown = st.chrome().status.borrow().label_text();
    assert!(
        shown.contains("2 replacements made"),
        "Replace All reports how many it made, not how many are left — got {shown:?}"
    );

    // The passage still covers the same text even though the replacements changed its
    // length — the marks moved with it. Searching the new word finds both, and only
    // both.
    type_query(&win, "TOKEN");
    assert_eq!(
        count(&win),
        Some(2),
        "the scope must still cover the same passage after its length changed"
    );
    win.destroy();
}

/// Every character the editor lights while a passage confines the search, and nothing
/// else. An empty answer means the scoped tag is not on the buffer at all.
fn highlighted_offsets(st: &std::rc::Rc<crate::winstate::TabState>) -> Vec<i32> {
    let buf: gtk::TextBuffer = st.editor_buf.clone().upcast();
    let Some(tag) = buf.tag_table().lookup(super::EDITOR_SCOPE_HL_TAG) else {
        return Vec::new();
    };
    (0..buf.char_count())
        .filter(|o| buf.iter_at_offset(*o).has_tag(&tag))
        .collect()
}

/// **The editor lights what it counts, and nothing outside the passage** (TDD 11.16).
///
/// Reported independently by the Windows and macOS seats against the first delivery of
/// the scope: the count said 2 while every occurrence in the document stayed lit,
/// because `GtkSourceSearchContext`'s `highlight` property is whole-buffer and has no
/// bounded form. The preview pane was already correct — measured, 16 matches down to 7
/// with the table cells and the collapsed body going dark — so this was one pane not
/// implementing the rubric rather than a limitation both shared, and a reader seeing
/// the two disagree reads the NUMBER as the broken half.
///
/// Asserted by character rather than by eye: the engine's own highlight and this one
/// are deliberately the same colour (they are the same matches, fewer of them), so a
/// screenshot cannot tell a confined highlight from an unconfined one.
#[gtktest::test]
fn the_editor_lights_only_the_matches_inside_the_passage() {
    const DOC: &str = "one target\ntwo target\nthree target\nfour target\n";
    let app = test_app("com.extollit.scribobulate.integrationtest.findscopehl");
    let win = crate::window::new_window(&app, "IT-findscopehl", DOC, None);
    set_mode(&win, "edit");
    search(&win, "target");
    let st = state(&win).expect("a tab");
    assert!(
        st.search_context.is_highlight(),
        "unscoped, the engine paints its own whole-buffer highlight"
    );
    assert!(
        highlighted_offsets(&st).is_empty(),
        "unscoped, this application paints nothing of its own — two highlights over \
         the same matches is a second thing to keep true"
    );

    let bound_end = DOC.find("three").expect("the fixture has a third line") as i32;
    select_range(&win, 0, bound_end);
    set_option(&win, super::super::findbar::FIND_IN_SELECTION, true);
    // Drained deliberately: the engine's `occurrences-count` settles asynchronously and
    // its notification is the one that used to overwrite a scoped readout with the
    // unscoped count. An assertion taken before it lands cannot see that.
    assert_eq!(count(&win), Some(2), "the count describes the passage");

    assert!(
        !st.search_context.is_highlight(),
        "the engine's highlight cannot be bounded, so it must be OFF while a passage \
         confines the search"
    );
    // "one target\ntwo target\n" — the two occurrences inside the passage, written out
    // rather than searched for, so the expectation does not come from the code under
    // test.
    let expected: Vec<i32> = (4..10).chain(15..21).collect();
    assert_eq!(
        highlighted_offsets(&st),
        expected,
        "exactly the in-passage occurrences are lit — not the two on lines three and \
         four, which the readout already says are not matches"
    );

    set_option(&win, super::super::findbar::FIND_IN_SELECTION, false);
    assert_eq!(count(&win), Some(4), "unticking restores the whole buffer");
    assert!(
        highlighted_offsets(&st).is_empty(),
        "the scoped highlight is taken OFF the buffer, not merely stopped being added \
         to — it is a tag, and a tag outlives the search that applied it"
    );
    assert!(
        st.search_context.is_highlight(),
        "and the engine takes its own highlight back"
    );
    win.destroy();
}

/// Replace acts on the match the reader is LOOKING at (TDD 11.17; ScrAP-27).
///
/// The fixture puts the caret past a match on purpose: re-finding forward from the
/// caret and replacing what it lands on is right only while the caret happens to sit on
/// the highlighted match, and the whole point is the case where it does not.
#[gtktest::test]
fn replace_acts_on_the_current_match_and_then_advances() {
    const DOC: &str = "aaa target bbb target ccc target\n";
    let app = test_app("com.extollit.scribobulate.integrationtest.findreplacecur");
    let win = crate::window::new_window(&app, "IT-findreplacecur", DOC, None);
    set_mode(&win, "edit");
    search(&win, "target");
    // Settle first: the editor's engine answers its scanning sentinel until it has
    // swept the buffer, and a step taken before then lands on the right match while
    // honestly reporting no position for it (§11.3). This test is about WHICH match is
    // replaced, so it waits rather than measuring through that state.
    assert_eq!(count(&win), Some(3));
    let st = state(&win).expect("a tab");

    // Step onto the SECOND match, so "the current match" and "the first match after the
    // start of the document" are different answers.
    super::find_step(&win, &st.search_context, super::SearchDir::Forward);
    super::find_step(&win, &st.search_context, super::SearchDir::Forward);
    assert_eq!(st.find_cursor.get(), crate::window::FindCursor::Editor(2));

    super::replace_current_match(&win, &st, "DONE");
    let after = crate::saferizer::BufferText::of(&st.editor_buf).into_string();
    assert_eq!(
        after, "aaa target bbb DONE ccc target\n",
        "the SECOND occurrence — the one that was highlighted — must be the one replaced"
    );
    // …and the selection advanced to the next one rather than staying put.
    let (sel_start, sel_end) = st
        .editor_buf
        .selection_bounds()
        .expect("Replace advances to the next match and selects it");
    assert_eq!(
        crate::saferizer::BufferText::of_range(&st.editor_buf, &sel_start, &sel_end).as_str(),
        "target",
        "Replace must advance to the next match"
    );
    assert!(
        sel_start.offset() > "aaa target bbb DONE".len() as i32,
        "…the next one FORWARD, not back to the first"
    );
    win.destroy();
}

/// **A preview scope that no longer resolves makes the holder re-derive** (TDD 11.16).
///
/// The toggle turns itself off and the search covers the whole pane. Reinterpreting the
/// range against the new render would confine the search to whatever now happens to sit
/// at those offsets, which is a confident answer about a passage the reader never chose
/// — the arm the retired `fold_epoch` got wrong and `PreviewFindCache` got right.
#[gtktest::test]
fn a_preview_scope_that_no_longer_resolves_turns_itself_off() {
    let app = test_app("com.extollit.scribobulate.integrationtest.findscopepreview");
    let win = crate::window::new_window(&app, "IT-findscopepreview", MD, None);
    set_mode(&win, "preview");
    search(&win, "note");
    assert_eq!(count(&win), Some(ALL));

    // The first line of the rendered preview only.
    select_range(&win, 0, 23);
    set_option(&win, super::super::findbar::FIND_IN_SELECTION, true);
    let confined = count(&win);
    assert!(
        confined.is_some_and(|n| n > 0 && n < ALL),
        "the selection must confine the count to part of the pane, got {confined:?}"
    );
    assert!(option_state(&win, super::super::findbar::FIND_IN_SELECTION));

    // A theme switch re-renders the preview beneath the bound.
    crate::app::re_render_all_windows(&app);
    crate::testpump::drain_for(
        crate::testpump::Clock::Frame,
        std::time::Duration::from_millis(400),
    );
    assert_eq!(
        count(&win),
        Some(ALL),
        "an unresolvable bound must make the search cover the whole pane again"
    );
    assert!(
        !option_state(&win, super::super::findbar::FIND_IN_SELECTION),
        "…and the toggle must say so, rather than claiming a confinement that is gone"
    );
    assert!(
        state(&win).expect("a tab").find_scope.borrow().is_none(),
        "the bound itself must be released, not merely ignored"
    );
    win.destroy();
}

/// A collapsed disclosure ABOVE a passage, so expanding it splices text in above the
/// captured bound — the arm where a held offset and a `GtkTextTag` range behave
/// differently.
const FOLD_MD: &str = "note before the fold\n\n<details>\n<summary>Hidden section</summary>\n\nnote hidden one\n\nnote hidden two\n\n</details>\n\nnote in the passage\n\nnote also in the passage\n\nnote at the very end\n";

/// **A fold splice is a boundary too, and the find state has to be told** (TDD 11.16).
///
/// Reported by the macOS seat against 11.19's fold-splice leg, and the report is worth
/// keeping because it was right about the observation and wrong about the diagnosis in
/// a way no amount of looking at the pane could settle. What they saw was a toggle that
/// stayed ticked over a count that stayed correct, and they reasonably asked whether the
/// rubric was over-strict — a bound that survives honestly should not be discarded.
///
/// It had not survived. **Nothing had asked it.** An in-place splice bumps the render
/// generation through `build::install_content`, so the bound would have failed to
/// resolve the moment anything looked; but `refresh_preview_find_highlight` had three
/// callers and the splice path was not one of them. The highlight went on looking right
/// because a `GtkTextTag` range moves with an insertion of its own accord while a held
/// offset does not — so the tags tracked the splice, the bound did not, and the two
/// agreed by coincidence. The state was not stale but INCONSISTENT: touching the search
/// at all would have released the bound and jumped the count with no reader action
/// between.
///
/// **This is why the assertion is that the count changes with NO search interaction.**
/// A check that searched again afterwards would have passed on the broken build — the
/// search itself is what repaired it.
#[gtktest::test]
fn expanding_a_fold_above_a_passage_releases_the_passage() {
    let app = test_app("com.extollit.scribobulate.integrationtest.findscopesplice");
    let win = crate::window::new_window(&app, "IT-findscopesplice", FOLD_MD, None);
    set_mode(&win, "preview");
    search(&win, "note");
    let whole_pane = count(&win).expect("the preview counts the query");

    // The two passage paragraphs, which sit BELOW the fold.
    let view = match super::find_target(&win) {
        super::FindTarget::Preview(v) => v,
        _ => panic!("preview mode must resolve a preview view"),
    };
    let body = crate::saferizer::BufferText::of_range(
        &view.buffer(),
        &view.buffer().start_iter(),
        &view.buffer().end_iter(),
    )
    .into_string();
    let from = body
        .find("note in the passage")
        .expect("the fixture's passage must be rendered") as i32;
    let to = body
        .find("note at the very end")
        .expect("the fixture's tail must be rendered") as i32;
    select_range(&win, from, to);
    set_option(&win, super::super::findbar::FIND_IN_SELECTION, true);
    let confined = count(&win);
    assert!(
        confined.is_some_and(|n| n > 0 && n < whole_pane),
        "precondition: the passage must confine the count, got {confined:?} of \
         {whole_pane}"
    );

    // Expand the disclosure ABOVE the passage — the production path, through the
    // control rather than through the splice, because a toggle whose handler never
    // reaches the splice changes nothing and looks identical from here.
    let toggle = crate::preview::scrib_render_data(&view)
        .expect("the preview carries render data")
        .borrow()
        .disclosure_lines[0]
        .1
        .clone();
    toggle.set_active(!toggle.is_active());
    crate::testpump::drain_for(
        crate::testpump::Clock::Frame,
        std::time::Duration::from_millis(600),
    );

    assert!(
        state(&win).expect("a tab").find_scope.borrow().is_none(),
        "the bound indexed the previous render and must be released by the splice \
         itself — not by whatever the reader happens to do next"
    );
    assert!(
        !option_state(&win, super::super::findbar::FIND_IN_SELECTION),
        "…and the toggle must stop claiming a confinement that is no longer in force"
    );
    assert_ne!(
        count(&win),
        confined,
        "the readout must have recounted for the whole pane without the reader \
         touching the search — a count that only corrects itself on the next \
         keystroke is the inconsistency this guards"
    );
    win.destroy();
}

/// With nothing selected and nothing captured, the control is unavailable and says why
/// (TDD 11.16).
#[gtktest::test]
fn the_in_selection_control_is_unavailable_with_nothing_to_confine() {
    let app = test_app("com.extollit.scribobulate.integrationtest.findscopesens");
    let win = crate::window::new_window(&app, "IT-findscopesens", MD, None);
    set_mode(&win, "edit");
    search(&win, "note");
    let name = super::super::findbar::FIND_IN_SELECTION;
    assert!(
        !action_enabled(&win, name),
        "nothing is selected, so there is nothing to confine the search to"
    );
    select_range(&win, 0, 10);
    assert!(action_enabled(&win, name), "a selection makes it usable");
    set_option(&win, name, true);
    // Collapsing the selection must NOT take the control away: a captured bound has to
    // stay releasable, or the reader is stuck inside it.
    select_range(&win, 5, 5);
    assert!(
        action_enabled(&win, name),
        "a captured bound must stay releasable after the selection that made it is gone"
    );
    win.destroy();
}

/// **The find bar's boundary refresh describes the pane in front of the reader, not
/// only the preview** (TDD 11.13).
///
/// `refresh_preview_find_highlight` is what every boundary that rebuilds a pane calls —
/// a mode switch, a theme re-render, an external reload. It used to act on the Preview
/// arm alone, on the reasoning that only a rebuilt preview loses anything. True of the
/// highlights; false of the READOUT, which was left showing the count of the pane the
/// reader had just left. The two panes legitimately count differently — the preview
/// searches three texts, so an anchored pattern matches inside each — so the stale
/// number was true of neither the query nor the pane.
///
/// Reported by the macOS seat, ratifying batch A.
///
/// **The assertion is made against the refresh ITSELF, not against a live mode
/// switch**, and the difference is the whole reliability of this guard. A mode switch
/// has side effects that also happen to recount — the editor takes focus, the caret
/// moves, and its engine re-emits `occurrences-count` — so a test that switches modes
/// and then reads the label is measuring whichever of several paths got there first,
/// and may pass on a build where this refresh does nothing at all. Poisoning the label
/// and invoking the boundary refresh asks the one question that matters: does the call
/// every pane-rebuilding boundary makes answer for the pane the reader can see?
///
/// Mutation check: narrow `findbar::refresh_preview_find_highlight` back to its
/// `if let FindTarget::Preview(..)` arm → the poison survives in edit mode
/// (`"999 matches"` against an owed `"2 matches"`). VERIFIED, and worth saying how it
/// was nearly not: the first attempt at this check edited a pattern that did not exist
/// in the file, so it mutated nothing and the test "passed" — a green mutation run is
/// indistinguishable from a mutation that never happened unless the edit asserts it
/// landed.
#[gtktest::test]
fn the_boundary_refresh_recounts_for_whichever_pane_is_visible() {
    let app = test_app("com.extollit.scribobulate.integrationtest.findmodereadout");
    let win = crate::window::new_window(&app, "IT-findmodereadout", MD, None);

    set_mode(&win, "edit");
    search(&win, "note");
    set_option(&win, "find-regex", true);
    let st = state(&win).expect("a tab");
    type_query(&win, "^note");
    let in_editor = count(&win).expect("the editor counts the anchored pattern");

    set_mode(&win, "preview");
    let in_preview = count(&win).expect("the preview counts it too");
    assert_ne!(
        in_editor, in_preview,
        "precondition: the fixture must make the two panes disagree, or a stale readout \
         is indistinguishable from a fresh one"
    );

    let label = st.chrome().match_count_label.clone();
    for (mode, owed) in [("preview", in_preview), ("edit", in_editor)] {
        set_mode(&win, mode);
        // Whatever the switch itself did, put a number on screen that is true of
        // neither pane, then ask the boundary refresh to answer.
        label.set_text("999 matches");
        crate::window::refresh_preview_find_highlight(&win);
        assert_eq!(
            label.text().as_str(),
            format!("{owed} matches"),
            "in {mode} the boundary refresh must recount FOR {mode} — it is the one \
             call every pane-rebuilding boundary makes, so an arm it does not cover is \
             a readout nothing corrects"
        );
    }
    win.destroy();
}

/// **Whole word bounds an alternation as one group, in the editor too** (TDD 11.14).
///
/// `notebook` starts with one branch and ends with the other, so it matches an
/// ungrouped `\b…\b` wrapper and not a grouped one. GtkSourceView's own wrapper is
/// ungrouped, so this passes only because the application wraps the pattern before
/// handing it over — which is the whole of `matcher::editor_pattern`.
///
/// Driven through the find bar rather than through two engines directly, because the
/// wrapping happens in `findbar::refresh_find` and a probe that set the raw query on a
/// `SearchSettings` of its own would measure an engine the application never drives.
#[gtktest::test]
fn whole_word_bounds_an_alternation_the_same_way_in_both_panes() {
    const SPANNING: &str = "note notebook book\n";
    let app = test_app("com.extollit.scribobulate.integrationtest.findwordalt");
    let win = crate::window::new_window(&app, "IT-findwordalt", SPANNING, None);
    for mode in ["edit", "preview"] {
        set_mode(&win, mode);
        search(&win, "note|book");
        set_option(&win, "find-regex", true);
        set_option(&win, "find-whole-word", true);
        assert_eq!(
            count(&win),
            Some(2),
            "{mode}: `notebook` is neither whole word, so only the two standalone \
             words match — an ungrouped wrapper counts it as 4 here"
        );
        set_option(&win, "find-whole-word", false);
        set_option(&win, "find-regex", false);
    }
    win.destroy();
}

/// **A passage captured in one pane is released when the search moves to the other**
/// (TDD 11.16).
///
/// The count was always truthful here — a preview scope confines nothing once the
/// editor is the target — so the defect was purely that the toggle went on claiming a
/// confinement that was not applied. That is the shape nobody notices until they have
/// trusted it once, which is why it is asserted on the CONTROL and not only on the
/// count. Found by the macOS seat, Preview → Side by Side.
#[gtktest::test]
fn a_scope_is_released_when_the_search_moves_to_the_other_pane() {
    let name = super::super::findbar::FIND_IN_SELECTION;
    let app = test_app("com.extollit.scribobulate.integrationtest.findscopepane");
    let win = crate::window::new_window(&app, "IT-findscopepane", MD, None);
    set_mode(&win, "preview");
    search(&win, "note");
    select_range(&win, 0, 23);
    set_option(&win, name, true);
    let confined = count(&win).expect("a confined count");
    assert!(
        confined < ALL,
        "precondition: the passage confines the search"
    );

    // Split makes the EDITOR the find target, and a preview range is not a position in
    // the source buffer.
    set_mode(&win, "split");
    assert_eq!(
        count(&win),
        Some(ALL),
        "the search must cover the whole pane it has moved to"
    );
    assert!(
        !option_state(&win, name),
        "…and the toggle must say so. A truthful count under a ticked toggle is worse \
         than a wrong one: nothing on screen contradicts it"
    );
    assert!(
        state(&win).expect("a tab").find_scope.borrow().is_none(),
        "the bound itself must be released, not merely unused"
    );
    win.destroy();
}

/// The entries a field's drop-down would offer, in order, read through the menu model
/// the popover is actually built from.
fn history_rows(win: &ApplicationWindow, btn: &gtk::MenuButton) -> Vec<String> {
    // `popup()` is what the press does, and it is what runs the `create_popup_func`
    // that builds the model from the ACTIVE tab. Reading the model rather than the
    // popover's widgets keeps this about the CONTENT — and it works on a bare Xvfb,
    // where a popover surface may never map (GTK4Rs/AP-175).
    btn.popup();
    crate::testpump::drain_for(
        crate::testpump::Clock::Idle,
        std::time::Duration::from_millis(150),
    );
    let Some(model) = btn.menu_model() else {
        return Vec::new();
    };
    let _ = win;
    let rows: Vec<String> = (0..model.n_items())
        .filter_map(|i| {
            model
                .item_attribute_value(i, gtk::gio::MENU_ATTRIBUTE_LABEL, None)
                .and_then(|v| v.get::<String>())
        })
        .collect();
    btn.popdown();
    rows
}

/// **Each field offers that tab's own committed entries, most recent first** (TDD
/// 11.18).
///
/// The half that fails quietly is the commit rule: the find field searches as you type,
/// so a history fed from `search-changed` records every PREFIX of every query. That
/// looks fine on a fixture with one search and is useless on a real one, which is why
/// the "typed but never committed" leg is asserted explicitly rather than inferred from
/// the committed ones being present.
#[gtktest::test]
fn each_field_offers_that_tabs_own_recent_entries() {
    let app = test_app("com.extollit.scribobulate.integrationtest.findhistory");
    let win = crate::window::new_window(&app, "IT-findhistory", MD, None);
    set_mode(&win, "edit");
    let st = state(&win).expect("a tab");
    let find_btn = st.chrome().find_history_btn.clone();

    search(&win, "note");
    assert!(
        !find_btn.is_sensitive(),
        "a drop-down with nothing to offer must be unavailable — an empty menu is \
         worse than a control that says it has nothing"
    );
    assert!(
        history_rows(&win, &find_btn).is_empty(),
        "typing is not committing: the field searches as you type, so a history fed \
         from that records every prefix of every query"
    );

    // Commit three terms, the middle one twice.
    for term in ["note", "Note", "note"] {
        type_query(&win, term);
        let _ = count(&win);
        super::super::findbar::record_committed_query(&st);
    }
    assert_eq!(
        history_rows(&win, &find_btn),
        vec!["note".to_string(), "Note".to_string()],
        "most recent first, de-duplicated — re-committing a term MOVES it rather than \
         adding a second copy"
    );
    assert!(find_btn.is_sensitive(), "…and the control is now usable");

    // Choosing an entry fills the field AND searches for it.
    type_query(&win, "");
    let _ = count(&win);
    crate::window::actions::simple_action(&win, super::super::findbar::PICK_HISTORY)
        .expect("the pick action is registered")
        .activate(Some(&"fNote".to_variant()));
    assert_eq!(
        st.chrome().find_entry.text().as_str(),
        "Note",
        "choosing a row fills the field"
    );
    assert!(
        count(&win).is_some_and(|n| n > 0),
        "…and searches for it immediately, rather than leaving the reader to press Enter"
    );

    // A second tab has its OWN history, not the first tab's.
    let second = crate::window::create_tab_in_window(&win, MD, None, false, false)
        .expect("the second tab is created");
    let st2 = crate::winstate::tab_by_id(second).expect("the new tab is registered");
    assert!(
        history_rows(&win, &st2.chrome().find_history_btn).is_empty(),
        "a fresh tab's drop-down offers its own history, never the previous tab's"
    );
    assert!(!st2.chrome().find_history_btn.is_sensitive());
    win.destroy();
}

/// **The find field is selected BEFORE it is focused, never after** (TDD 11.1).
///
/// Not a style preference — it is a toolkit invariant. `gtk_text_focus_changed`
/// arms the cursor-blink tick whenever the entry takes focus with no selection, and
/// `gtk_text_set_selection_bounds` never calls `gtk_text_check_cursor_blink` to disarm
/// it again (source-read at the 4.6.9 floor). Focus first and the entry spends the next
/// frame blinking a cursor over a selection, which GTK notices about itself and reports
/// as `GtkText - unexpected blinking selection. Removing`.
///
/// **Why it asserts at focus-in rather than afterwards**: both orders leave the same
/// selection behind once `win.find` returns, so a check on the settled state passes on
/// either build. The only moment the two differ is the instant focus arrives, so that is
/// where the observation is taken. Swap the two lines in `findbar`'s open closure and
/// this fails while every other find test stays green.
///
/// The precondition matters as much as the assertion: a term already in the field with
/// the caret collapsed at its end is the state a REOPEN finds, and it is the only state
/// that can reach the defect — which is why five isolated legs, each opening the bar
/// over an empty field, all came back clean.
#[gtktest::test]
fn the_find_field_is_selected_before_it_takes_focus() {
    use std::cell::Cell;
    use std::rc::Rc;

    let app = test_app("com.extollit.scribobulate.integrationtest.findblink");
    let win = crate::window::new_window(&app, "IT-findblink", MD, None);
    let entry = state(&win)
        .expect("the window has an active tab")
        .chrome()
        .find_entry
        .clone();

    entry.set_text("note");
    assert!(
        entry.selection_bounds().is_none(),
        "precondition: a field filled programmatically holds no selection, so this \
         test can only pass by the open path making one"
    );

    let at_focus_in: Rc<Cell<Option<(i32, i32)>>> = Rc::new(Cell::new(None));
    let focus = gtk::EventControllerFocus::new();
    {
        let at_focus_in = Rc::clone(&at_focus_in);
        let entry = entry.clone();
        focus.connect_enter(move |_| at_focus_in.set(entry.selection_bounds()));
    }
    entry.add_controller(focus.clone());

    crate::window::actions::simple_action(&win, "find")
        .expect("win.find is registered")
        .activate(None);
    crate::testpump::drain_for(
        crate::testpump::Clock::Frame,
        std::time::Duration::from_millis(200),
    );

    assert_eq!(
        at_focus_in.get(),
        Some((0, 4)),
        "the whole term is selected at the instant the field takes focus"
    );
    // Asked of the CONTROLLER, not of the widget: the focus lands on the `GtkText`
    // inside the `GtkSearchEntry`, so the entry itself never reports `has-focus`.
    assert!(
        focus.contains_focus(),
        "…and the field is the one focused, which is what made the moment observable"
    );
    win.destroy();
}

/// The passage a capture holds, as the two marks themselves.
fn scope_marks(st: &crate::winstate::TabState) -> (gtk::TextMark, gtk::TextMark) {
    match st.find_scope.borrow().as_ref() {
        Some(super::FindScope::Editor { start, end }) => (start.mark().clone(), end.mark().clone()),
        _ => panic!("an editor passage is captured"),
    }
}

/// Letting a captured passage go deletes its two marks from the editor buffer. An
/// anonymous mark belongs to the buffer until `delete_mark` runs, so dropping the Rust
/// handles alone left two marks behind per capture for the life of the tab.
///
/// Mutation: make `OwnedMark::drop` a no-op and both marks stay live.
#[gtktest::test]
fn releasing_a_captured_passage_deletes_its_marks() {
    const DOC: &str = "one target\ntwo target\nthree target\n";
    let app = test_app("com.extollit.scribobulate.integrationtest.findscopemarks");
    let win = crate::window::new_window(&app, "IT-findscopemarks", DOC, None);
    set_mode(&win, "edit");
    search(&win, "target");
    let st = state(&win).expect("a tab");
    for _ in 0..3 {
        select_range(&win, 0, 10);
        set_option(&win, super::super::findbar::FIND_IN_SELECTION, true);
        let (start, end) = scope_marks(&st);
        assert!(
            !start.is_deleted() && !end.is_deleted(),
            "a held passage is live"
        );
        set_option(&win, super::super::findbar::FIND_IN_SELECTION, false);
        assert!(
            start.is_deleted() && end.is_deleted(),
            "releasing the passage must delete both marks from the buffer"
        );
    }
    win.destroy();
}

/// A closed find bar leaves the editor undecorated, whatever reaches the find code
/// while it is closed: a menu toggle of a match option re-runs the search, and an edit
/// inside a held passage fires the engine's count notification. Both used to repaint.
///
/// Mutation: drop the `open` term from `refresh_find`'s `set_highlight` and the first
/// assertion fails; drop the `find_bar_open` gate in `update_editor_readout` and the
/// second does.
#[gtktest::test]
fn a_closed_find_bar_paints_no_match_highlight() {
    const DOC: &str = "one target\ntwo target\nthree target\nfour target\n";
    let app = test_app("com.extollit.scribobulate.integrationtest.findclosedhl");
    let win = crate::window::new_window(&app, "IT-findclosedhl", DOC, None);
    set_mode(&win, "edit");
    search(&win, "target");
    assert_eq!(count(&win), Some(4));
    let st = state(&win).expect("a tab");
    st.chrome()
        .find_entry
        .emit_by_name::<()>("stop-search", &[]);
    set_option(&win, "find-match-case", true);
    readout_after_quiet(&win);
    assert!(
        !st.search_context.is_highlight(),
        "a match option toggled from the menu must not light the document with the bar closed"
    );
    set_option(&win, "find-match-case", false);

    // A held passage, then the bar closed, then a matching word typed inside it.
    search(&win, "target");
    let bound_end = DOC.find("three").expect("a third line") as i32;
    select_range(&win, 0, bound_end);
    set_option(&win, super::super::findbar::FIND_IN_SELECTION, true);
    assert_eq!(count(&win), Some(2));
    assert!(
        !highlighted_offsets(&st).is_empty(),
        "the open bar lights the passage"
    );
    st.chrome()
        .find_entry
        .emit_by_name::<()>("stop-search", &[]);
    assert!(
        highlighted_offsets(&st).is_empty(),
        "closing takes the scoped highlight off"
    );
    st.editor_buf
        .insert(&mut st.editor_buf.iter_at_offset(4), "target ");
    readout_after_quiet(&win);
    assert!(
        highlighted_offsets(&st).is_empty(),
        "an edit inside the passage must not repaint it while the bar is closed"
    );
    win.destroy();
}

/// A Replace All confined to a passage is one undo step, like the unconfined one, so
/// the checkbox does not change how many Undos the command takes.
///
/// Mutation: remove the `UndoGroup` around the scoped loop and one Undo restores only
/// the last replacement.
#[gtktest::test]
fn a_scoped_replace_all_is_one_undo_step() {
    const DOC: &str = "one target\ntwo target\nthree target\nfour target\n";
    let app = test_app("com.extollit.scribobulate.integrationtest.findscopeundo");
    let win = crate::window::new_window(&app, "IT-findscopeundo", DOC, None);
    set_mode(&win, "edit");
    search(&win, "target");
    let bound_end = DOC.find("four").expect("a fourth line") as i32;
    select_range(&win, 0, bound_end);
    set_option(&win, super::super::findbar::FIND_IN_SELECTION, true);
    assert_eq!(count(&win), Some(3));
    let st = state(&win).expect("a tab");
    super::replace_all_matches(&win, &st, "TOKEN");
    let replaced = crate::saferizer::BufferText::of(&st.editor_buf).into_string();
    assert_eq!(replaced, "one TOKEN\ntwo TOKEN\nthree TOKEN\nfour target\n");
    st.editor_buf.undo();
    let undone = crate::saferizer::BufferText::of(&st.editor_buf).into_string();
    assert_eq!(
        undone, DOC,
        "one Undo must restore every replacement the command made"
    );
    win.destroy();
}
