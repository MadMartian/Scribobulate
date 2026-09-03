//! Which disclosure blocks are collapsed — the display-free fold model.
//!
//! Kept free of any GTK type so it can be unit-tested under POLICY's no-live-display
//! coverage gate, like `outline/` and `copymap.rs`. The renderer reads it as a render
//! INPUT; nothing here knows what a buffer or a widget is.
//!
//! # Why a render input rather than hidden text
//!
//! A collapsed body is simply **not rendered**. The alternative — leaving it in the
//! buffer under a `GtkTextTag:invisible` — was rejected on measurement, not taste: it
//! corrupts GTK's own AT-SPI text interface on the vintage this project ships. MEASURED
//! at the bus on GTK 4.6.9 with a fixture of `AAAA` + invisible `XXXX` + `BBBB`:
//! `CharacterCount` reports 12 while `GetText(0, 12)` returns 8 characters, and
//! `GetCharacterAtOffset` hands back `U+0000` at every hidden offset. It would also make
//! `line_yrange`'s zero height permanently ambiguous — indistinguishable from "not yet
//! validated" — which is a trap for every future author of geometry-polling code.
//!
//! Not rendering the body deletes all of that by construction, at the cost of a
//! re-render per toggle rather than a family of hazards that fail silently and later.
//! That cost is no longer paid in full: a toggle SPLICES its own region rather than
//! rebuilding the document (`preview::splice`), so the reader keeps their place.
//!
//! # Scope, and where to stop looking
//!
//! Collapsed state lives for the session and is deliberately NOT persisted across one.
//! It is keyed on the source byte offset of a block's opening raw-HTML, which is stable
//! across zoom, theme, view-mode and live-preview re-renders and is cleared when the
//! document changes — matching HTML, where a disclosure's state is the `open` attribute
//! and therefore a property of the document rather than of the reader.
//!
//! **There is no GTK4 prior art for this.** Nothing in gtk4-demo, nothing in gtk4-rs,
//! and GtkSourceView has no code folding at all; every GTK Markdown viewer renders its
//! preview through a web engine and so delegates `<details>` to a UA. The one known
//! implementation of the idea in GNOME is GTK3's `TeplFoldRegion` (libgedit-tepl), from
//! which two design details carry over — line-snapped bounds, and marks rather than
//! character offsets for anything that must survive an edit. Its anonymous-tag-per-fold
//! detail does not, because this design uses no tags; and it never met the anchored-child
//! problem, because it folds source code, which has no child anchors. Recorded so the
//! next author searches for an hour less than the first one did.
//!
//! # What a fold is keyed on, and why that key is honest about its lifetime
//!
//! A [`FoldKey`] is the **source byte offset** of the disclosure's opening raw-HTML
//! block. It is stable across every re-render that does not change the source — zoom,
//! theme switch, view-mode switch, live-preview re-render — which is exactly the set of
//! events a reader expects their folds to survive.
//!
//! It is deliberately NOT stable across an edit or a reload, and that matches the
//! behaviour HTML itself specifies: in a browser, a disclosure's state is the `open`
//! *attribute*, a property of the document rather than of the session. So a reload
//! honours `<details open>` afresh and forgets what the reader toggled — which removes
//! the "key per-fold state to something that survives arbitrary edits" problem rather
//! than solving it.
//!
//! **Two paths move the source, and both must clear.** `TabState::set_source` is the
//! obvious one; the other is a live edit in split mode, where the editor buffer is
//! authoritative and the preview re-renders straight from it without `set_source` ever
//! running. `TabState::note_source_offsets_moved` is the single method both call, and it
//! exists because the second path called nothing at all — so a key that no longer named
//! the block it was minted for either reverted a collapsed block mid-typing or, when it
//! collided with a different block's new start offset, collapsed the wrong one.

use std::collections::HashSet;

/// A disclosure block's identity: the source byte offset at which its opening
/// raw-HTML block begins.
///
/// A newtype rather than a bare `usize` because this codebase carries several
/// different offset spaces (original source bytes, cleaned source bytes, buffer
/// characters) and mixing them is a defect class it has paid for before — an offset is
/// only as trustworthy as the space it was measured in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct FoldKey(usize);

impl FoldKey {
    /// Mint a key from a SOURCE BYTE offset — the only space this type accepts.
    ///
    /// The field is private and this is its only constructor, so the doc comment above
    /// states a property of the type rather than a convention its callers observe. It
    /// was `pub` while claiming exactly this, which made the claim a request.
    pub(crate) fn from_source_offset(offset: usize) -> Self {
        Self(offset)
    }

    /// The source byte offset this key was minted from.
    pub(crate) fn source_offset(self) -> usize {
        self.0
    }
}

/// The set of disclosures the reader has collapsed, plus the ones the document asked
/// to be collapsed.
///
/// Empty means "everything renders as the document says", which is why
/// [`Default`] is the right thing for every caller that has no reader state — the
/// export sink, and every test that is not about folding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FoldState {
    /// Blocks the reader has explicitly toggled, and which way.
    ///
    /// Explicit rather than a bare "collapsed" set because the DOCUMENT supplies a
    /// default per block (`<details open>`), and a reader who expands a collapsed block
    /// and one who has never touched an expanded block are different states that must
    /// render the same way and behave differently on the next toggle.
    toggled: HashSet<FoldKey>,
}

impl FoldState {
    /// Is the block at `key` rendered collapsed, given the `open` attribute the
    /// document states for it?
    ///
    /// The document supplies the default and the reader overrides it. Expressed as a
    /// single function so no call site re-derives the precedence — the rule is one
    /// XOR, and two sites spelling it out is how they come to disagree.
    pub(crate) fn is_collapsed(&self, key: FoldKey, open_in_source: bool) -> bool {
        let collapsed_by_default = !open_in_source;
        collapsed_by_default != self.toggled.contains(&key)
    }

    /// Flip the block at `key`.
    pub(crate) fn toggle(&mut self, key: FoldKey) {
        if !self.toggled.insert(key) {
            self.toggled.remove(&key);
        }
    }

    /// Put the block at `key` into a STATED render state, whatever it is in now.
    ///
    /// The companion to [`Self::toggle`], and the distinction is not cosmetic. A caller
    /// that means "expand this" and spells it `toggle` is correct only while the block
    /// really is collapsed, and nothing in the type enforces that: hand it an already
    /// expanded block and it collapses one the reader asked to see. `reveal_folds`
    /// carried exactly that latent inversion, resting on an invariant held by its
    /// caller's caller.
    ///
    /// `open_in_source` is the document's own `<details open>`, the same input
    /// [`Self::is_collapsed`] takes — the precedence is one rule and this is the same
    /// rule solved for the toggle bit rather than for the answer.
    pub(crate) fn set_collapsed(&mut self, key: FoldKey, open_in_source: bool, collapsed: bool) {
        // Spelled from `is_collapsed`'s own terms rather than minimised, so the two
        // read as one rule solved two ways: `is_collapsed` is
        // `collapsed_by_default != toggled`, and this solves that for `toggled` given
        // the answer the caller wants.
        let collapsed_by_default = !open_in_source;
        let want_toggled = collapsed != collapsed_by_default;
        if want_toggled {
            self.toggled.insert(key);
        } else {
            self.toggled.remove(&key);
        }
    }

    /// Forget every toggle — called when the document's text changes, since the keys
    /// are source offsets and a changed source moves them.
    /// Expand every key in `chain` against the document's own `<details open>`
    /// attributes, and report the ones that named no disclosure.
    ///
    /// **`set_collapsed(.., false)`, never `toggle`.** The caller's NAME is the
    /// postcondition — every key in `chain` is expanded when this returns — and a
    /// toggle only delivers that while every key really is collapsed, an invariant held
    /// by the caller's caller and enforced by nothing. Hand it an already-expanded block
    /// and a toggle CLOSES one the reader asked to see.
    ///
    /// A key naming no span is flipped and RETURNED: the fold map and the document have
    /// diverged, so there is no `open` attribute to reason from, and the flip is the old
    /// behaviour kept rather than a guess invented. The caller logs what comes back.
    ///
    /// **Here rather than in `window::foldreveal`** (F-TEST-B-003). It was a loop of
    /// pure decisions inside a coverage-excluded file with no test of any kind; moving
    /// it to the type that owns `FoldState` puts it inside the gate, which is the
    /// mechanism POLICY's scope rule names for exactly this.
    pub(crate) fn expand_chain(
        &mut self,
        spans: &[crate::renderer::disclosure::DisclosureSpan],
        chain: &[FoldKey],
    ) -> Vec<FoldKey> {
        let mut diverged = Vec::new();
        for key in chain {
            match spans.iter().find(|s| s.fold_key() == *key) {
                Some(span) => self.set_collapsed(*key, span.open, false),
                None => {
                    diverged.push(*key);
                    self.toggle(*key);
                }
            }
        }
        diverged
    }

    pub(crate) fn clear(&mut self) {
        self.toggled.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: FoldKey = FoldKey(80);
    const B: FoldKey = FoldKey(200);

    #[test]
    fn a_plain_details_starts_collapsed() {
        // `<details>` renders collapsed unless marked open — the HTML default, and the
        // normal condition for every document this feature exists to serve.
        let folds = FoldState::default();
        assert!(folds.is_collapsed(A, false));
    }

    #[test]
    fn details_open_starts_expanded() {
        // Rubric 2.26b: the document's own attribute decides the initial state.
        let folds = FoldState::default();
        assert!(!folds.is_collapsed(A, true));
    }

    #[test]
    fn toggling_inverts_whichever_default_the_document_stated() {
        // The reader's override composes with the document's default rather than
        // replacing it, so both starting points behave the same way under a toggle.
        let mut folds = FoldState::default();

        folds.toggle(A);
        assert!(!folds.is_collapsed(A, false), "a collapsed block opens");

        folds.toggle(B);
        assert!(
            folds.is_collapsed(B, true),
            "an <details open> block closes"
        );
    }

    #[test]
    fn toggling_twice_returns_to_the_documents_state() {
        // The apply path works on its own; the reverse is the half that silently
        // diverges, so it gets its own assertion.
        let mut folds = FoldState::default();
        folds.toggle(A);
        folds.toggle(A);
        assert!(folds.is_collapsed(A, false));
    }

    #[test]
    fn siblings_toggle_independently() {
        // Rubric 2.26e: one disclosure's state must not reach another's.
        let mut folds = FoldState::default();
        folds.toggle(A);
        assert!(!folds.is_collapsed(A, false));
        assert!(
            folds.is_collapsed(B, false),
            "toggling one block must not disturb its sibling"
        );
    }

    #[test]
    fn a_nested_block_keeps_its_own_state_while_its_parent_closes() {
        // Rubric 2.26e's second half: re-expanding an outer disclosure restores the
        // inner one's OWN prior state rather than resetting it. Keys are independent,
        // so an outer block's collapse cannot erase an inner block's entry — the model
        // has no parent/child relation at all, which is what makes this true by
        // construction rather than by care.
        let mut folds = FoldState::default();
        let (outer, inner) = (FoldKey(10), FoldKey(40));
        folds.toggle(inner); // reader opens the inner block
        folds.toggle(outer); // reader opens, then closes, the outer one
        folds.toggle(outer);

        assert!(
            folds.is_collapsed(outer, false),
            "the outer block is closed again"
        );
        assert!(
            !folds.is_collapsed(inner, false),
            "the inner block kept the reader's expansion"
        );
    }

    #[test]
    fn a_document_change_forgets_every_toggle() {
        // The keys are source offsets, so an edit moves them. Honouring the source's
        // own `open` attributes afresh is both correct and what a browser does.
        let mut folds = FoldState::default();
        folds.toggle(A);
        folds.clear();
        assert!(folds.is_collapsed(A, false));
    }

    #[test]
    fn set_collapsed_reaches_the_stated_state_from_either_side() {
        // The property `toggle` cannot offer: the ANSWER is the argument, so calling it
        // twice is calling it once, and a block already in the wanted state stays there.
        for open_in_source in [false, true] {
            for want in [false, true] {
                let mut folds = FoldState::default();
                // ...from the document's own default...
                folds.set_collapsed(A, open_in_source, want);
                assert_eq!(folds.is_collapsed(A, open_in_source), want);
                // ...and from the opposite state the reader put it in.
                let mut flipped = FoldState::default();
                flipped.toggle(A);
                flipped.set_collapsed(A, open_in_source, want);
                assert_eq!(flipped.is_collapsed(A, open_in_source), want);
                // Idempotent, which is the whole difference from `toggle`.
                flipped.set_collapsed(A, open_in_source, want);
                assert_eq!(flipped.is_collapsed(A, open_in_source), want);
            }
        }
    }

    #[test]
    fn set_collapsed_leaves_every_other_block_alone() {
        let mut folds = FoldState::default();
        folds.toggle(B);
        let b_before = folds.is_collapsed(B, true);
        folds.set_collapsed(A, false, false);
        assert_eq!(
            folds.is_collapsed(B, true),
            b_before,
            "expanding A must not disturb B"
        );
    }

    /// **F-TEST-B-003.** `reveal_folds`' loop was pure logic in a coverage-excluded
    /// file with no test of any kind. Three cases pay for the extraction immediately,
    /// and each is a different way the old shape could have been wrong.
    #[test]
    fn expand_chain_expands_against_the_documents_own_open_attribute() {
        use crate::renderer::disclosure::DisclosureSpan;

        let span = |start: usize, open: bool| DisclosureSpan {
            start,
            at: 0,
            open,
            body: Some(start..start + 10),
        };
        let key = |start: usize| FoldKey::from_source_offset(start);

        // (1) An ALREADY-EXPANDED key stays expanded. This is the postcondition the
        // function's name promises, and the one a `toggle` cannot deliver: hand a
        // toggle an expanded block and it closes the thing the reader asked to see.
        let spans = [span(0, false)];
        let mut folds = FoldState::default();
        folds.toggle(key(0)); // the reader opened a block the document says is closed
        assert!(!folds.is_collapsed(key(0), false), "precondition: expanded");
        assert!(folds.expand_chain(&spans, &[key(0)]).is_empty());
        assert!(
            !folds.is_collapsed(key(0), false),
            "an already-expanded key is left expanded, not flipped shut"
        );

        // (2) A block the DOCUMENT marks `open`, which the reader has collapsed,
        // resolves to expanded rather than to a flip — the two answers differ, because
        // `set_collapsed` is stated against the document's own attribute.
        let spans = [span(0, true)];
        let mut folds = FoldState::default();
        folds.toggle(key(0)); // the reader closed an `<details open>`
        assert!(folds.is_collapsed(key(0), true), "precondition: collapsed");
        assert!(folds.expand_chain(&spans, &[key(0)]).is_empty());
        assert!(!folds.is_collapsed(key(0), true), "and now expanded");

        // (3) A key naming NO span is returned as diverged *and* still flipped — the
        // old behaviour kept deliberately, because with no `open` attribute there is
        // nothing to reason from. Returning it is what lets the caller say so.
        let spans = [span(0, false)];
        let mut folds = FoldState::default();
        let stray = key(999);
        assert_eq!(
            folds.expand_chain(&spans, &[stray]),
            vec![stray],
            "a key with no span is reported"
        );
        assert!(
            !folds.is_collapsed(stray, false),
            "and flipped, which is the fallback rather than a guess"
        );
    }
}
