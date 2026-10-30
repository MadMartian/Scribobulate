//! [`AnchoredSpan`] — a byte range into a document that survives the document
//! changing underneath it. The one held reference this application has.
//!
//! # Why this exists
//!
//! A control is built at one instant and acted on at another. Between those two
//! instants the document can change: the user types, an undo lands, a save flushes
//! the editor into the source, or the split-mode live re-render re-scans. A plain
//! `Range<usize>` captured at build time does not survive any of that — and applying
//! a stale one is not a near-miss, it is *destructive*, because string surgery at the
//! wrong offsets deletes whatever happens to be there and leaves the markup half-open.
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
//!    answer — the overwhelmingly common case, and it costs one comparison. The fast
//!    path is not merely an optimisation: it also disambiguates, so several identical
//!    constructs in an unmoved document each resolve to themselves.
//! 2. Otherwise the [`Ambiguity`] policy the capture declared decides, and it is the
//!    ONLY thing that varies between the constructs using this type.
//! 3. If the text is gone entirely, there is no answer: `None`.
//!
//! # `None` obliges the caller to RE-DERIVE, never to do nothing
//!
//! This is the contract that makes the type safe to reuse, and it was learnt from the
//! one construct that got it wrong. A held reference that gives up whenever the
//! document moved makes the feature useless in exactly the session where it is most
//! used — and giving up *silently* leaves a live control on screen that does nothing,
//! with no cause visible where it was caused (a save) or where it was felt (a click).
//! So a caller that cannot resolve rebuilds the view the reference was minted by; the
//! reader's next gesture then lands on a control that names the current document. The
//! cost of the wrong answer is a wasted rebuild, never a dead feature.
//!
//! Pure and display-free by construction: it holds a `String`, a `Range` and a
//! one-byte policy, so every rule above is exhaustively testable with plain
//! `cargo test`.

use std::ops::Range;

/// What to do when the captured text is no longer at the captured offset — the one
/// per-construct decision this type takes, because the STRENGTH of an identity varies
/// by what it identifies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ambiguity {
    /// Take the occurrence **nearest** the captured offset.
    ///
    /// Right for an identity a document does not repeat — an annotation carries
    /// user-authored comment text, so the nearest occurrence is the construct that
    /// moved. Ties break toward the earlier match, so the choice is deterministic.
    Nearest,
    /// Resolve only while the identity names **exactly one** place in the document,
    /// and refuse otherwise.
    ///
    /// Right for an identity a document repeats: a disclosure's identity is its
    /// opening delimiter (`<details><summary>Example</summary>`), and two identical
    /// siblings plus an edit above them larger than half their separation resolves
    /// "nearest" to the WRONG block — the one outcome a fold control promises never
    /// happens. Proximity cannot disambiguate an identity that is not distinctive, so
    /// it is not consulted for one; the caller re-derives instead (see the module
    /// docs).
    Unique,
}

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
    /// See [`Ambiguity`]. Carried by the span rather than passed at resolution so the
    /// policy is a property of the CONSTRUCT, decided once where the identity is
    /// known — a resolve-time argument would let two call sites judge one reference
    /// by two different rules.
    on_ambiguity: Ambiguity,
    /// The text immediately either side of the capture, when the construct asked for
    /// it ([`capture_in_context`](Self::capture_in_context)). An occurrence of `text`
    /// is then only a candidate if its neighbours match too, on the fast path as well.
    context: Option<Context>,
}

/// Up to [`CONTEXT_BYTES`] of the source either side of a capture.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Context {
    before: String,
    after: String,
}

/// How much neighbouring text a contextual capture keeps on each side. Enough to tell
/// apart two occurrences of a common word, or two identical blocks, by what surrounds
/// them; short enough that an edit a few words away does not disturb it.
const CONTEXT_BYTES: usize = 24;

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
    ///
    /// Test-only: every production construct names a place a document can repeat, so
    /// each captures [`in context`](Self::capture_in_context).
    #[cfg(test)]
    pub(crate) fn capture(source: &str, at: Range<usize>) -> Option<Self> {
        Self::capture_with(source, at, Ambiguity::Nearest)
    }

    /// Capture `at` from `source`, stating what an ambiguous re-resolution means for
    /// this construct — see [`Ambiguity`], and [`capture`](Self::capture) for the rest.
    pub(crate) fn capture_with(
        source: &str,
        at: Range<usize>,
        on_ambiguity: Ambiguity,
    ) -> Option<Self> {
        if at.start >= at.end {
            return None;
        }
        let text = source.get(at.clone())?.to_string();
        Some(Self {
            at,
            text,
            on_ambiguity,
            context: None,
        })
    }

    /// Capture `at` with the text either side of it as part of its identity.
    ///
    /// **For an identity a document repeats**, where the captured text alone cannot
    /// name one place: prose the reader selected (`the`, `flat`), or a delimiter a
    /// document carries more than once. Without the neighbours, an edit above two
    /// occurrences resolved `Nearest` to the wrong one, and the fast path could not tell
    /// "nothing moved" from "an identical copy moved into the old offset" — so a comment
    /// was written onto a different word, or a toggle folded a different block.
    ///
    /// The cost is the other direction, and it is the safe one: an edit touching the
    /// neighbours stops the span resolving, and `None` obliges the caller to re-derive
    /// (see the module docs) rather than to guess.
    pub(crate) fn capture_in_context(
        source: &str,
        at: Range<usize>,
        on_ambiguity: Ambiguity,
    ) -> Option<Self> {
        let mut span = Self::capture_with(source, at.clone(), on_ambiguity)?;
        let from =
            crate::strbound::floor_char_boundary(source, at.start.saturating_sub(CONTEXT_BYTES));
        let to = crate::strbound::ceil_char_boundary(source, at.end + CONTEXT_BYTES);
        span.context = Some(Context {
            before: source[from..at.start].to_string(),
            after: source[at.end..to].to_string(),
        });
        Some(span)
    }

    /// Whether the captured text at `start` in `source` also has the captured
    /// neighbours, when this span carries any.
    fn neighbours_match(&self, source: &str, start: usize) -> bool {
        let Some(context) = &self.context else {
            return true;
        };
        let end = start + self.text.len();
        let before_ok = start
            .checked_sub(context.before.len())
            .and_then(|from| source.get(from..start))
            == Some(context.before.as_str());
        let after_ok = source.get(end..end + context.after.len()) == Some(context.after.as_str());
        before_ok && after_ok
    }

    /// Where the captured text lives in `source` **now**, or `None` if it is no
    /// longer present.
    ///
    /// See the module docs for the rule, and [`Ambiguity`] for what a miss on the
    /// fast path means for this construct. Note the fast path is not merely an
    /// optimisation — it also disambiguates: when a document holds several
    /// identical constructs and none of them moved, each resolves to its own
    /// offset rather than all collapsing onto the first.
    ///
    /// **`None` is an instruction, not a verdict**: re-derive the view this
    /// reference was minted by. See the module docs.
    pub(crate) fn resolve(&self, source: &str) -> Option<Range<usize>> {
        if source.get(self.at.clone()) == Some(self.text.as_str())
            && self.neighbours_match(source, self.at.start)
        {
            return Some(self.at.clone());
        }
        let mut found = source
            .match_indices(self.text.as_str())
            .map(|(i, _)| i)
            // `Unique` does not consult the neighbours here: its identity is the text
            // (TDD 2.26n — "the identity is the delimiter"), so two identical copies
            // that have moved are ambiguous and decline, as that rubric requires. The
            // neighbours guard only its fast path above, where an identical copy
            // shifted into the old offset must not answer for the block that was there.
            .filter(|&i| {
                self.on_ambiguity == Ambiguity::Unique || self.neighbours_match(source, i)
            });
        let start = match self.on_ambiguity {
            // Nearest occurrence to where it used to be. `match_indices` yields
            // non-overlapping matches in ascending order, so `min_by_key` with a tie
            // broken toward the earlier match is deterministic.
            Ambiguity::Nearest => {
                let want = self.at.start as i128;
                found.min_by_key(|&i| (i as i128 - want).abs())?
            }
            // Exactly one, or none: a second occurrence makes the identity unable to
            // name a place, and proximity is not consulted for an identity that is
            // not distinctive.
            Ambiguity::Unique => {
                let only = found.next()?;
                if found.next().is_some() {
                    return None;
                }
                only
            }
        };
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

    /// Whether `other`, captured from a later source, names the same construct as this
    /// span: the same text with the same neighbours.
    ///
    /// For matching one scan's constructs to the next scan's when neither source is to
    /// hand (an annotation card re-keyed onto a new render's markers). It asks what
    /// [`resolve`](Self::resolve) asks of a candidate place, so an edit touching the
    /// neighbours stops the match, as it stops the resolution. The ambiguity policy is not
    /// compared: it belongs to the kind of construct, which both captures share.
    pub(crate) fn names_same_construct(&self, other: &Self) -> bool {
        self.text == other.text && self.context == other.context
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

    /// A contextual capture of the construct in `src`.
    fn construct_in(src: &str) -> AnchoredSpan {
        let at = src.find("{==").unwrap()..src.find("<<}").unwrap() + 3;
        AnchoredSpan::capture_in_context(src, at, Ambiguity::Nearest).unwrap()
    }

    #[test]
    fn a_construct_moved_by_an_edit_far_above_it_is_the_same_construct() {
        let pad = "x".repeat(2 * CONTEXT_BYTES);
        let before = format!("{pad} {SRC}");
        let after = format!("an inserted paragraph\n\n{before}");
        let (a, b) = (construct_in(&before), construct_in(&after));
        assert_ne!(a.captured_at(), b.captured_at(), "precondition: it moved");
        assert!(a.names_same_construct(&b));
    }

    #[test]
    fn a_construct_with_other_text_or_other_neighbours_is_not_the_same() {
        let pad = "x".repeat(2 * CONTEXT_BYTES);
        let a = construct_in(&format!("{pad} {SRC}"));
        let reworded = construct_in(&format!("{pad} {}", SRC.replace("note", "memo")));
        let renamed_neighbour = construct_in(&format!("{pad} {}", SRC.replace("alpha", "omega")));
        assert!(!a.names_same_construct(&reworded), "the comment changed");
        assert!(
            !a.names_same_construct(&renamed_neighbour),
            "a neighbour changed"
        );
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

    /// The ambiguity policy, both arms, on the shape that produced it: two identical
    /// constructs and an edit above them big enough that proximity picks the wrong one.
    #[test]
    fn a_repetitive_identity_refuses_rather_than_choosing_between_two_of_itself() {
        // `<details><summary>Example</summary>` twice, 40 bytes apart. `Nearest` is
        // free to answer with either; `Unique` must answer with neither.
        let id = "<details><summary>Example</summary>";
        let src = format!("{id}\n\nbody one\n\n{id}\n\nbody two\n");
        let second = src.rfind(id).unwrap();
        let at = second..second + id.len();

        let nearest = AnchoredSpan::capture_with(&src, at.clone(), Ambiguity::Nearest).unwrap();
        let unique = AnchoredSpan::capture_with(&src, at.clone(), Ambiguity::Unique).unwrap();

        // Unmoved: the fast path answers for both, and it answers correctly — which is
        // why an ambiguous identity is safe as long as nothing moved.
        assert_eq!(nearest.resolve(&src), Some(at.clone()));
        assert_eq!(unique.resolve(&src), Some(at.clone()));

        // An edit ABOVE both, longer than half the distance between them: the second
        // construct's new position is now nearer the FIRST one's old offset.
        let pad = "x".repeat(at.len() + 30);
        let live = format!("{pad}\n\n{src}");
        let moved = live.rfind(id).unwrap();
        assert_ne!(
            nearest.resolve(&live),
            Some(moved..moved + id.len()),
            "precondition: proximity picks the wrong one of the two, which is the \
             failure the Unique policy exists to refuse"
        );
        assert_eq!(
            unique.resolve(&live),
            None,
            "a repetitive identity names no place once the fast path misses"
        );
    }

    #[test]
    fn a_unique_identity_still_follows_its_construct_across_an_edit() {
        // The other half: refusing on ambiguity must not degrade into refusing always.
        let id = "<details><summary>Only one</summary>";
        let src = format!("lead\n\n{id}\n\nbody\n");
        let at = src.find(id).unwrap()..src.find(id).unwrap() + id.len();
        let a = AnchoredSpan::capture_with(&src, at, Ambiguity::Unique).unwrap();
        let live = format!("a new paragraph above it\n\n{src}");
        let got = a.resolve(&live).expect("still exactly one occurrence");
        assert_eq!(&live[got], id);
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

    /// A selected common word, an edit above it, and a second occurrence that the
    /// edit moved nearer the old offset: held in context, the span still resolves to
    /// the word the reader selected, where `Nearest` on the bare text took the other.
    #[test]
    fn a_word_held_in_context_survives_an_edit_that_makes_another_copy_nearer() {
        let src = "the cat. the dog.";
        let at = 9..12;
        assert_eq!(&src[at.clone()], "the");
        let bare = AnchoredSpan::capture(src, at.clone()).unwrap();
        let held = AnchoredSpan::capture_in_context(src, at, Ambiguity::Nearest).unwrap();
        let live = format!("0123456789{src}");
        assert_eq!(
            bare.resolve(&live),
            Some(10..13),
            "the defect: the bare word picks the first copy"
        );
        assert_eq!(
            held.resolve(&live),
            Some(19..22),
            "in context it is still the second `the`"
        );
    }

    /// Two identical blocks, and an edit that shifts the first into the second's old
    /// offset (a duplicate pasted above them): the fast path on bare text answered with
    /// the first block. In context the neighbours differ, so the fast path declines, and
    /// the search then finds the identical copies ambiguous, so `Unique` names neither —
    /// never the wrong block (TDD 2.26n).
    #[test]
    fn an_identical_copy_shifted_into_the_old_offset_does_not_answer_for_it() {
        let block = "<details><summary>X</summary>";
        let src = format!("intro\n{block}\nA\n{block}\nB\n");
        let second = src.rfind(block).unwrap();
        let at = second..second + block.len();
        let held = AnchoredSpan::capture_in_context(&src, at.clone(), Ambiguity::Unique).unwrap();
        // Insert exactly the separation between the two blocks at the first one, so the
        // first now sits where the second was.
        let first = src.find(block).unwrap();
        let shift = &src[first..second];
        let live = format!("{}{shift}{}", &src[..first], &src[first..]);
        assert_eq!(
            &live[at.clone()],
            block,
            "precondition: a copy now sits at the old offset"
        );
        let bare = AnchoredSpan::capture_with(&src, at.clone(), Ambiguity::Unique).unwrap();
        assert_eq!(
            bare.resolve(&live),
            Some(at),
            "the defect: the bare fast path answers"
        );
        assert_eq!(
            held.resolve(&live),
            None,
            "in context it declines rather than guessing"
        );
    }

    /// An edit inside the neighbours of a target held in context makes it resolve to
    /// NOTHING, never to another copy of the same words: the comment card then reports
    /// the target gone instead of writing into the document somewhere else. The bare
    /// capture beside it shows what this guards: it answers with the remaining copy.
    ///
    /// Mutation: fall back to the unfiltered search when no neighbour-matching
    /// occurrence is left, and the held span answers too.
    #[test]
    fn a_target_whose_neighbours_were_edited_resolves_to_nothing() {
        let src = "the cat. the dog.";
        let at = 9..12;
        let held = AnchoredSpan::capture_in_context(src, at.clone(), Ambiguity::Nearest).unwrap();
        let bare = AnchoredSpan::capture(src, at).unwrap();
        let live = "the cat. the cow.";
        assert!(
            bare.resolve(live).is_some(),
            "the defect: the bare word still answers"
        );
        assert_eq!(held.resolve(live), None, "in context it must decline");
    }
}
