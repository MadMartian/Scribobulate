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
//! than solving it. [`FoldState::clear`] is what a document change calls.

use std::collections::HashSet;

/// A disclosure block's identity: the source byte offset at which its opening
/// raw-HTML block begins.
///
/// A newtype rather than a bare `usize` because this codebase carries several
/// different offset spaces (original source bytes, cleaned source bytes, buffer
/// characters) and mixing them is a defect class it has paid for before — an offset is
/// only as trustworthy as the space it was measured in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct FoldKey(pub usize);

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

    /// Forget every toggle — called when the document's text changes, since the keys
    /// are source offsets and a changed source moves them.
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
}
