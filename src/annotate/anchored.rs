//! [`AnchoredSpan`] — a source byte range that survives the document changing
//! underneath it.
//!
//! # Why this exists
//!
//! An annotation card is built at one instant and acted on at another. Between
//! those two instants the document can change: the user types, an undo lands, or
//! the split-mode live re-render re-scans the source. A plain `Range<usize>`
//! captured at build time does not survive any of that — and applying a stale one
//! is not a near-miss, it is *destructive*, because string surgery at the wrong
//! offsets deletes whatever happens to be there and leaves the markup half-open.
//! Out of bounds it is worse still: a raw slice **panics**, and a panic inside a
//! GTK signal handler aborts the process.
//!
//! So a range is never carried across time on its own. It is carried together
//! with **the text that occupied it**, which is what makes it re-findable: the
//! text is the identity, the offset is only a hint about where to look.
//!
//! # The resolution rule
//!
//! 1. If the captured text is still exactly at the captured offset, that is the
//!    answer — the overwhelmingly common case, and it costs one comparison.
//! 2. Otherwise, find every occurrence of the captured text and take the one
//!    **nearest the captured offset**. An edit elsewhere shifts a construct by the
//!    edit's size, so the nearest occurrence is the one that moved; ties break
//!    toward the earlier, so the choice is deterministic.
//! 3. If the text is gone entirely, there is no answer: `None`. The caller must
//!    treat that as "do nothing", never as "guess".
//!
//! The nearest-occurrence rule is what keeps the feature *working* rather than
//! merely safe. Validation alone would reject every post-edit removal, turning a
//! corruption bug into a "Remove silently does nothing" bug.
//!
//! Pure and display-free by construction: it holds a `String` and a `Range`, so
//! every rule above is exhaustively testable with plain `cargo test`.

use std::ops::Range;

/// A byte range captured from a source string at one instant, together with the
/// text that occupied it, so it can be re-located in a source that has since
/// changed.
///
/// Construct with [`capture`](Self::capture) and read back with
/// [`resolve`](Self::resolve). The captured range is available as
/// [`captured_at`](Self::captured_at) for use as a stable identity key, but it
/// must never be used as an index into a source that may have moved on — that is
/// the entire hazard this type exists to remove.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnchoredSpan {
    at: Range<usize>,
    text: String,
}

impl AnchoredSpan {
    /// Capture `at` from `source`.
    ///
    /// `None` when `at` is not a valid slice of `source` — out of bounds, or not
    /// on `char` boundaries. Returning `None` rather than panicking matters: the
    /// ranges reaching here come from a scan of a source that may already have
    /// been superseded, so an invalid one is a reachable state, not a bug to
    /// assert on.
    ///
    /// An **empty** range is also refused. Empty text matches everywhere and
    /// therefore anchors nothing; a construct always has delimiters, so an empty
    /// capture means the caller's range was wrong.
    pub(crate) fn capture(source: &str, at: Range<usize>) -> Option<Self> {
        if at.start >= at.end {
            return None;
        }
        let text = source.get(at.clone())?.to_string();
        Some(Self { at, text })
    }

    /// Where the captured text lives in `source` **now**, or `None` if it is no
    /// longer present.
    ///
    /// See the module docs for the rule. Note the fast path is not merely an
    /// optimisation — it also disambiguates: when a document holds several
    /// identical annotations and none of them moved, each resolves to its own
    /// offset rather than all collapsing onto the first.
    pub(crate) fn resolve(&self, source: &str) -> Option<Range<usize>> {
        if source.get(self.at.clone()) == Some(self.text.as_str()) {
            return Some(self.at.clone());
        }
        // Nearest occurrence to where it used to be. `match_indices` yields
        // non-overlapping matches in ascending order, so `min_by_key` with a tie
        // broken toward the earlier match is deterministic.
        let want = self.at.start as i128;
        let start = source
            .match_indices(self.text.as_str())
            .map(|(i, _)| i)
            .min_by_key(|&i| (i as i128 - want).abs())?;
        Some(start..start + self.text.len())
    }

    /// The range this span was captured from.
    ///
    /// For identity and diagnostics — **not** for indexing. Use
    /// [`resolve`](Self::resolve) to get a range that is valid against a
    /// particular source.
    pub(crate) fn captured_at(&self) -> Range<usize> {
        self.at.clone()
    }

    /// Re-express an absolute sub-range of the captured text as an offset
    /// relative to its start, so it can be carried alongside this span and
    /// re-absolutised after resolution.
    ///
    /// `None` unless the sub-range lies wholly within the captured range: a
    /// sub-range that escapes its construct is a caller bug, and silently
    /// clamping it would reintroduce exactly the class of quiet mis-splice this
    /// type exists to prevent.
    pub(crate) fn relative(&self, abs: Range<usize>) -> Option<Range<usize>> {
        if abs.start < self.at.start || abs.end > self.at.end || abs.start > abs.end {
            return None;
        }
        Some(abs.start - self.at.start..abs.end - self.at.start)
    }

    /// Map a relative sub-range (from [`relative`](Self::relative)) back onto a
    /// resolved location.
    pub(crate) fn absolute(resolved: &Range<usize>, rel: &Range<usize>) -> Range<usize> {
        resolved.start + rel.start..resolved.start + rel.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "alpha {==beta==}{>>note<<} gamma";

    fn construct() -> AnchoredSpan {
        let at = SRC.find("{==").unwrap()..SRC.find("<<}").unwrap() + 3;
        AnchoredSpan::capture(SRC, at).unwrap()
    }

    #[test]
    fn an_unchanged_source_resolves_to_the_captured_range() {
        let a = construct();
        assert_eq!(a.resolve(SRC), Some(a.captured_at()));
    }

    #[test]
    fn an_insertion_before_the_span_shifts_the_resolution_by_its_length() {
        let a = construct();
        let live = format!("EDITED {SRC}");
        let got = a.resolve(&live).expect("the construct is still present");
        assert_eq!(&live[got.clone()], "{==beta==}{>>note<<}");
        assert_eq!(got.start, a.captured_at().start + "EDITED ".len());
    }

    #[test]
    fn a_deletion_before_the_span_shifts_the_resolution_back() {
        let a = construct();
        let live = SRC.replace("alpha ", "");
        let got = a.resolve(&live).expect("the construct is still present");
        assert_eq!(&live[got], "{==beta==}{>>note<<}");
    }

    #[test]
    fn a_vanished_construct_resolves_to_nothing_rather_than_a_guess() {
        let a = construct();
        assert_eq!(a.resolve("alpha beta gamma"), None);
    }

    #[test]
    fn a_source_shorter_than_the_captured_offset_resolves_to_nothing() {
        // The bare-offset version of this panicked: "start byte index 26 is out
        // of bounds". Resolution must be total.
        let a = construct();
        assert_eq!(a.resolve("alpha"), None);
        assert_eq!(a.resolve(""), None);
    }

    #[test]
    fn identical_constructs_each_resolve_to_themselves_while_nothing_moves() {
        let src = "x {==a==}{>>n<<} y {==a==}{>>n<<} z";
        let first = src.find("{==").unwrap();
        let second = src.rfind("{==").unwrap();
        let len = "{==a==}{>>n<<}".len();
        let a1 = AnchoredSpan::capture(src, first..first + len).unwrap();
        let a2 = AnchoredSpan::capture(src, second..second + len).unwrap();
        assert_eq!(a1.resolve(src), Some(first..first + len));
        assert_eq!(a2.resolve(src), Some(second..second + len));
    }

    #[test]
    fn among_identical_constructs_the_nearest_to_the_old_offset_wins() {
        let src = "x {==a==}{>>n<<} y {==a==}{>>n<<} z";
        let second = src.rfind("{==").unwrap();
        let len = "{==a==}{>>n<<}".len();
        let a2 = AnchoredSpan::capture(src, second..second + len).unwrap();
        // Two chars inserted at the very front: both copies shift by 2, and the
        // second must still resolve to the second.
        let live = format!("qq{src}");
        assert_eq!(a2.resolve(&live), Some(second + 2..second + 2 + len));
    }

    #[test]
    fn capture_refuses_a_range_that_is_not_a_valid_slice() {
        assert_eq!(AnchoredSpan::capture(SRC, 0..SRC.len() + 1), None);
        assert_eq!(AnchoredSpan::capture(SRC, 5..5), None); // empty anchors nothing
                                                            // Not on a char boundary: "é" is two bytes.
        assert_eq!(AnchoredSpan::capture("é", 0..1), None);
    }

    #[test]
    fn a_multibyte_document_resolves_on_char_boundaries() {
        let src = "Åπ {==claim==}{>>note<<} tail";
        let at = src.find("{==").unwrap()..src.find("<<}").unwrap() + 3;
        let a = AnchoredSpan::capture(src, at).unwrap();
        let live = format!("ÅÅÅ{src}");
        let got = a.resolve(&live).expect("present");
        assert_eq!(&live[got], "{==claim==}{>>note<<}");
    }

    #[test]
    fn relative_and_absolute_round_trip_a_sub_range() {
        let a = construct();
        let inner = SRC.find("beta").unwrap()..SRC.find("beta").unwrap() + 4;
        let rel = a.relative(inner.clone()).unwrap();
        assert_eq!(AnchoredSpan::absolute(&a.captured_at(), &rel), inner);
        // And onto a moved resolution.
        let live = format!("EDITED {SRC}");
        let moved = a.resolve(&live).unwrap();
        assert_eq!(&live[AnchoredSpan::absolute(&moved, &rel)], "beta");
    }

    #[test]
    fn relative_refuses_a_sub_range_that_escapes_the_construct() {
        let a = construct();
        let at = a.captured_at();
        assert_eq!(a.relative(at.start - 1..at.end), None);
        assert_eq!(a.relative(at.start..at.end + 1), None);
    }
}
