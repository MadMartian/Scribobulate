//! The find bar's test helpers, shared by `bartests` and `splittests`.
//!
//! One copy, because the two modules each had their own and the fix for the delayed
//! search below went into one of them only: the other kept a fixed settle after typing
//! and so kept the race the fix had measured away.

use crate::winstate::state;
use gtk::prelude::*;
use gtk::ApplicationWindow;

/// Open the find bar and put `query` in it, for the pane the window is currently
/// showing, and return once the search it asks for has actually run.
///
/// **Returning before the search runs makes the NEXT step race it.** `GtkSearchEntry`
/// delivers `search-changed` after its own delay, not on `set_text`, and a caller that
/// selects a passage straight away was relying on doing so inside that window. When it
/// ran late the delayed search arrived after the selection and took it away — in the
/// preview by re-selecting the current match, in the editor through a loss of the X11
/// PRIMARY selection — so Find in Selection was unavailable, its toggle was ignored,
/// and the count covered the whole document. Measured: 2 to 6 failures in every 192
/// loaded, instrumented runs of `bartests` before, none in 192 after. The handler
/// records the query on the tab, so waiting for that record is waiting for the search,
/// not for a guessed delay.
pub(super) fn search(win: &ApplicationWindow, query: &str) {
    crate::window::actions::simple_action(win, "find")
        .expect("win.find is registered")
        .activate(None);
    type_query(win, query);
}

/// Put `query` in the already-open find field and return once its delayed search has
/// run — [`search`] without opening the bar, for a caller that opened it some other
/// way (Find and Replace).
pub(super) fn type_query(win: &ApplicationWindow, query: &str) {
    let st = state(win).expect("the window has an active tab");
    st.chrome().find_entry.set_text(query);
    crate::testpump::until(
        crate::testpump::Clock::Idle,
        "the find field's delayed search to run",
        || *st.find_query.borrow() == query,
    );
}

/// The readout's text as it stands now, with no wait — for a caller that polls it
/// against the value it expects.
pub(super) fn readout_now(win: &ApplicationWindow) -> String {
    state(win)
        .expect("the window has an active tab")
        .chrome()
        .match_count_label
        .text()
        .to_string()
}

/// Wait until the readout reads `expected`, naming `what` if it never does. The wait
/// IS the assertion: a readout that a retarget or a delayed search rewrites later than
/// the test looked is the race a fixed settle loses on a loaded host.
pub(super) fn until_readout(win: &ApplicationWindow, expected: &str, what: &str) {
    crate::testpump::until(
        crate::testpump::Clock::Idle,
        &format!("{what} (the readout to read {expected:?})"),
        || readout_now(win) == expected,
    );
}

/// The readout's text once the search it describes has finished scanning.
///
/// **It does not wait for the search to START.** `GtkSearchEntry` debounces
/// `search-changed`, so a readout read straight after a raw `set_text` is the PREVIOUS
/// query's answer — and that answer is not `…`, so the wait below passes at once and
/// the caller measures the wrong search. That wait belongs to whoever typed: put text in
/// the field with [`type_query`] or [`search`], which return once the search has run.
/// This then waits only for the editor's engine, which answers its `-1` sentinel (`…`)
/// until it has swept the buffer; the preview's path has no such delay and passes
/// straight through.
///
/// It once also drained a fixed 400 ms "past the debounce", which was pure delay for
/// every caller that had already waited on the search, and a race for every caller that
/// had not.
pub(super) fn readout(win: &ApplicationWindow) -> String {
    let st = state(win).expect("the window has an active tab");
    let label = st.chrome().match_count_label.clone();
    crate::testpump::until(
        crate::testpump::Clock::Idle,
        "the find readout to stop reporting a scan in progress",
        || label.text() != "…",
    );
    label.text().to_string()
}

/// [`readout`] after giving the main loop a fixed span to run, for an ABSENCE assertion:
/// "this step changed nothing", "the bar stayed dark". An absence has no observable to
/// wait for, so a deferred effect is given the span to appear in before the caller
/// checks it did not. Never use this to wait for a search: that is [`type_query`].
pub(super) fn readout_after_quiet(win: &ApplicationWindow) -> String {
    crate::testpump::drain_for(
        crate::testpump::Clock::Idle,
        std::time::Duration::from_millis(400),
    );
    readout(win)
}
