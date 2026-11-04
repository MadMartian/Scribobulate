//! The find bar's **per-field history** — what the reader searched for and what they
//! replaced it with, most recent first.
//!
//! One type for both fields, because they are the same thing twice: a short, ordered,
//! de-duplicated list of strings a reader can get back to. The only thing that differs
//! is which field feeds it, and that is the call site's business.
//!
//! # What counts as an entry
//!
//! A term enters when it is **committed** — Enter, Next/Prev, Replace, Replace All —
//! never per keystroke. That rule lives at the call sites (there is one `record` here
//! and several places that call it), but the reason belongs with the type: the find
//! field searches as you type, so every prefix of every query is a "search" that
//! happened. A history fed per keystroke is `n`, `no`, `not`, `note` — which is the
//! reader's last query four times over, and it pushes everything they actually wanted
//! off the end of a capped list.
//!
//! # Why most-recent-first, de-duplicated
//!
//! A reader reaching for this wants the term they used a minute ago, and the one thing
//! they know about it is that it was recent. Re-committing a term already in the list
//! MOVES it to the front rather than adding a second copy: without that, searching the
//! same two terms alternately fills the whole list with those two.

/// How many entries a field's history keeps.
///
/// Long enough to hold a session's working set, short enough that the drop-down is
/// still something a reader scans rather than reads. One number, owned here — the
/// popover, the persistence and the tests all read it rather than writing their own.
pub(crate) const HISTORY_CAP: usize = 12;

/// How much of an entry is shown in the drop-down.
///
/// An entry is a string the READER typed, so its length is unbounded and a regular
/// expression is routinely long. The popover is not in the find bar's allocation, so
/// this is legibility rather than a window-width obligation (TDD 9.38) — but a menu
/// wider than the window is still a menu nobody can read.
pub(crate) const HISTORY_LABEL_CHARS: usize = 48;

/// One field's committed entries, most recent first.
///
/// `#[serde(default)]` and a plain `Vec<String>` on the wire: this is persisted in the
/// session file, and a history is the one piece of find state whose loss costs the
/// reader nothing but convenience — so it deserializes leniently and is repaired on
/// load rather than failing a session that contains a malformed one.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct FindHistory {
    entries: Vec<String>,
}

impl FindHistory {
    /// Record a committed `entry`, moving it to the front if it is already known.
    ///
    /// An empty entry is not recorded: it is not something the reader can want back,
    /// and an empty row in the drop-down does nothing when chosen.
    pub(crate) fn record(&mut self, entry: &str) {
        if entry.is_empty() {
            return;
        }
        self.entries.retain(|e| e != entry);
        self.entries.insert(0, entry.to_string());
        self.entries.truncate(HISTORY_CAP);
    }

    /// The entries, most recent first.
    pub(crate) fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Whether there is anything to offer. The drop-down button is insensitive when
    /// there is not — a control that opens an empty list is worse than one that says
    /// it has nothing.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Repair a history read from a session file.
    ///
    /// **A persisted list is untrusted input**, and the three things that can be wrong
    /// with it are the three invariants `record` maintains: an over-long list, an empty
    /// entry, and a duplicate. A hand-edited or partially-written session file can
    /// carry any of them, and none is worth failing the whole session load over (which
    /// would cost the reader every window and tab to protect a convenience) — the same
    /// judgement `TabSession::doc_id` already records.
    pub(crate) fn sanitised(self) -> Self {
        let mut out = Self::default();
        // Re-recorded from the OLDEST first, so the order the file claims survives:
        // `record` pushes to the front, so replaying front-to-back would reverse it.
        for entry in self.entries.iter().rev() {
            out.record(entry);
        }
        out
    }
}

/// The label for a drop-down row: the entry, shortened if it is long, with whitespace
/// made visible enough that two entries differing only in a trailing space are not one
/// row twice.
///
/// Returns an owned `String` rather than borrowing: most entries pass through unchanged
/// and a `Cow` would buy one allocation on a list of at most [`HISTORY_CAP`] rows, at
/// the cost of a lifetime in every caller.
pub(crate) fn row_label(entry: &str) -> String {
    let shown: String = entry
        .chars()
        .take(HISTORY_LABEL_CHARS)
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    if entry.chars().count() > HISTORY_LABEL_CHARS {
        format!("{shown}…")
    } else {
        shown
    }
}

/// TDD 11.18.
#[cfg(test)]
mod tests {
    use super::{row_label, FindHistory, HISTORY_CAP, HISTORY_LABEL_CHARS};

    /// TOML has no bare-array document, so the round trip needs one key to hang the
    /// value off — the same shape `TabSession` gives it in the real file.
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Wrapped {
        h: FindHistory,
    }

    fn recorded(entries: &[&str]) -> FindHistory {
        let mut h = FindHistory::default();
        for e in entries {
            h.record(e);
        }
        h
    }

    /// The order a reader expects: the last thing they committed is the first thing
    /// they are offered.
    #[test]
    fn entries_come_back_most_recent_first() {
        let h = recorded(&["one", "two", "three"]);
        assert_eq!(h.entries(), ["three", "two", "one"]);
    }

    /// Re-committing a known term MOVES it rather than adding a second copy. Without
    /// this, alternating between two terms fills the list with those two and pushes
    /// everything else off the end — which is the case a capped list makes visible.
    #[test]
    fn re_committing_a_term_moves_it_to_the_front() {
        let h = recorded(&["one", "two", "one"]);
        assert_eq!(h.entries(), ["one", "two"]);
        let h = recorded(&["a", "b", "a", "b", "a", "b"]);
        assert_eq!(h.entries(), ["b", "a"], "no duplicates accumulate");
    }

    /// The cap holds, and it drops the OLDEST — the end a reader is least likely to
    /// reach for.
    #[test]
    fn the_list_is_capped_and_drops_the_oldest() {
        let many: Vec<String> = (0..HISTORY_CAP + 5).map(|i| format!("q{i}")).collect();
        let mut h = FindHistory::default();
        for e in &many {
            h.record(e);
        }
        assert_eq!(h.entries().len(), HISTORY_CAP);
        assert_eq!(h.entries()[0], format!("q{}", HISTORY_CAP + 4));
        assert!(
            !h.entries().contains(&"q0".to_string()),
            "the oldest entries are what the cap drops"
        );
    }

    /// An empty commit is not an entry. The find field is empty every time the reader
    /// clears it, and a row that does nothing when chosen is not a history.
    #[test]
    fn an_empty_entry_is_not_recorded() {
        let h = recorded(&["one", "", "two"]);
        assert_eq!(h.entries(), ["two", "one"]);
        assert!(FindHistory::default().is_empty());
    }

    /// A persisted list is untrusted: it can be over-long, hold duplicates, or hold an
    /// empty entry, and none of those is worth failing a session load over. Repair
    /// preserves the file's ORDER, which replaying it front-to-back would reverse.
    #[test]
    fn a_persisted_history_is_repaired_rather_than_rejected() {
        let raw: FindHistory = toml::from_str::<Wrapped>(
            "h = [\"newest\", \"dup\", \"\", \"dup\", \"3\", \"4\", \"5\", \"6\", \"7\", \
             \"8\", \"9\", \"10\", \"11\", \"12\", \"13\"]",
        )
        .expect("a bare string array deserialises")
        .h;
        let fixed = raw.sanitised();
        assert_eq!(fixed.entries().len(), HISTORY_CAP);
        assert_eq!(
            fixed.entries()[0],
            "newest",
            "the file's own order is preserved, not reversed"
        );
        assert_eq!(
            fixed.entries().iter().filter(|e| *e == "dup").count(),
            1,
            "a duplicate is collapsed"
        );
        assert!(
            !fixed.entries().iter().any(|e| e.is_empty()),
            "an empty entry is dropped"
        );
    }

    /// A round trip through the session file's own format keeps the list intact. It is
    /// serialised as a bare array of strings, so a shape change here is a history that
    /// silently stops loading.
    #[test]
    fn a_history_round_trips_through_the_session_format() {
        let h = recorded(&["one", "two"]);
        let text = toml::to_string(&Wrapped { h: h.clone() }).expect("serialises");
        assert_eq!(text.trim(), r#"h = ["two", "one"]"#);
        assert_eq!(toml::from_str::<Wrapped>(&text).expect("deserialises").h, h);
    }

    /// A row label is bounded, because the entry is a string the reader typed and a
    /// regular expression is routinely long.
    #[test]
    fn a_long_entry_is_shortened_for_its_row() {
        let long = "x".repeat(HISTORY_LABEL_CHARS + 20);
        let label = row_label(&long);
        assert_eq!(label.chars().count(), HISTORY_LABEL_CHARS + 1);
        assert!(label.ends_with('…'));
        assert_eq!(row_label("short"), "short", "a short entry is untouched");
    }

    /// Whitespace is normalised for DISPLAY only, so a multi-line regular expression
    /// does not turn one row into three — and the entry itself is unchanged, because
    /// it is what gets searched for.
    #[test]
    fn a_rows_label_does_not_break_across_lines() {
        assert_eq!(row_label("a\nb\tc"), "a b c");
        let mut h = FindHistory::default();
        h.record("a\nb");
        assert_eq!(h.entries(), ["a\nb"], "the stored entry keeps its own text");
    }
}
