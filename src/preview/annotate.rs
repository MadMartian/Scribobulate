//! Part C — the preview-side **create** overlay. When the reader
//! selects text in the preview, a small non-autohide **GtkPopover** offers **Annotate**;
//! clicking it dismisses the popover and reveals an in-surface comment-entry card (an
//! GtkOverlay child) at the same anchor, whose Save wraps the selected source range in
//! CriticMarkup (or, for a cross-block selection, drops a point comment at the selection
//! end) via the view's annotation sink.
//!
//! The selection→source mapping ([`create_from_selection`]) and the entry-card placement
//! geometry ([`bar_placement`], and the shared `saferizer::ViewportRect` anchor guard) are
//! pure functions unit-tested without
//! a display; the GTK wiring around them lives in the [`overlay`] submodule, which is
//! excluded from the coverage ratchet (scripts/coverage.sh / POLICY.md) because it cannot
//! be exercised headlessly — same treatment as `window/editbar/` and `codeview.rs`.

use crate::codeview::CreateAnnotation;
use crate::span::CleanedByteOffset;

mod overlay;
pub(crate) use overlay::{position_card, wire_annotation_overlay};
/// The cell-press controller's event filter and coordinate transform, display-free.
mod press;

/// Capture a preview selection `[buf_a, buf_b)` as a target the comment card can act
/// on **later** — the moment the reader clicks Annotate, not the moment they press Save.
///
/// # Why the crossing into source space happens HERE
///
/// The card stays open across an unbounded number of main-loop turns: the reader is
/// typing a comment, and meanwhile a fold toggle, an external reload or a theme
/// re-render can rebuild the preview's offset maps wholesale (Document-Reference CAM
/// row 8). Holding the two BUFFER offsets and mapping them at Save resolved them against
/// a `copymap` describing a different render — the longest-held reference in the
/// application, resolved against the most replaceable map in it, and the failure WRITES
/// into the document rather than mis-positioning a view.
///
/// A preview selection already carries everything needed to cross into source space at
/// the moment it is made, so the crossing is simply moved earlier. What comes back is a
/// [`PendingTarget`]: the construct's own text, re-findable in a document that has
/// changed (ScrAP-187's mechanism, applied to the CREATE path).
///
/// The wrap span comes from the render's **copymap** ([`crate::copymap::wrap_span`]),
/// which returns the OUTER, balanced source range — any inline construct the selection
/// touches (emphasis/strong/link/code) is included WHOLE, so the `{==…==}` delimiters can
/// never split a `**`/`` ` ``/`[]()` (the inline-construct-split bug).
///
/// `None` for an empty or unmappable selection.
pub(crate) fn capture_selection(
    copymap: &crate::copymap::CopyTree,
    shifts: &[(usize, usize)],
    cleaned: &str,
    original: &str,
    buf_a: i32,
    buf_b: i32,
) -> Option<PendingTarget> {
    PendingTarget::capture(
        original,
        selection_target(copymap, shifts, cleaned, buf_a, buf_b)?,
    )
}

/// The editor-side sibling of [`capture_selection`]: the editor buffer holds the raw
/// Markdown verbatim, so there is no copymap indirection — only char→byte and the same
/// balancing.
pub(crate) fn capture_editor_selection(
    source: &str,
    char_a: i32,
    char_b: i32,
) -> Option<PendingTarget> {
    PendingTarget::capture(source, editor_selection_target(source, char_a, char_b)?)
}

/// A selection the reader has aimed the comment card at, held in SOURCE space and
/// re-resolvable against a document that has moved since.
///
/// The card's Save resolves it once, at the one choke point the mutation is applied
/// (`window::annotate::apply_annotation_edit`), so the annotation lands on the text the
/// reader selected or on nothing at all — never on another occurrence of the same
/// words, because the target is held with its neighbours (`docref::capture_in_context`).
#[derive(Clone, Debug)]
pub(crate) struct PendingTarget {
    span: crate::docref::AnchoredSpan,
    /// A selection crossing a block boundary becomes a point comment at the span's END
    /// rather than a highlight over it.
    point: bool,
}

impl PendingTarget {
    fn capture(source: &str, target: SelectionTarget) -> Option<Self> {
        let (span, point) = match target {
            SelectionTarget::Highlight(range) => (range, false),
            SelectionTarget::Point(range) => (range, true),
        };
        Some(Self {
            // **In context**: an annotation target is prose the reader chose, which a
            // document repeats (`the`, `flat`), so the selected text alone cannot name
            // one place. Its neighbours make it distinctive; `Nearest` then only breaks
            // a tie between occurrences whose surroundings also match. If the edit
            // touched the neighbours, the target does not resolve and the card's commit
            // reports that rather than writing onto another occurrence.
            span: crate::docref::AnchoredSpan::capture_in_context(
                source,
                span,
                crate::docref::Ambiguity::Nearest,
            )?,
            point,
        })
    }

    /// Where this target lives in `source` now, or `None` if its text is gone.
    ///
    /// Used by the card to show the comments a commit would MERGE, so the preview of
    /// the merge and the merge itself resolve the same reference the same way.
    pub(crate) fn resolve(&self, source: &str) -> Option<std::ops::Range<usize>> {
        self.span.resolve(source)
    }

    /// True when this target merges nothing: a point comment inserts a new construct
    /// rather than replacing any.
    pub(crate) fn is_point(&self) -> bool {
        self.point
    }

    /// Pair the target with the comment the reader typed. `None` for an empty comment.
    pub(crate) fn with_comment(self, comment: &str) -> Option<CreateAnnotation> {
        let comment = comment.trim().to_string();
        if comment.is_empty() {
            return None;
        }
        Some(if self.point {
            CreateAnnotation::Point {
                target: self.span,
                comment,
            }
        } else {
            CreateAnnotation::Highlight {
                target: self.span,
                comment,
            }
        })
    }
}

/// Where in the ORIGINAL source a preview selection lands, independent of any comment.
/// See [`selection_target`].
pub(crate) enum SelectionTarget {
    /// An intra-block selection, wrapped as a highlight over this source byte range.
    Highlight(std::ops::Range<usize>),
    /// A selection crossing a block boundary — a point comment at this range's END
    /// instead (the cross-block fallback). The whole range is carried rather than the
    /// end alone because it is the range that has an IDENTITY: an offset cannot be
    /// re-found in a document that moved, and the text at the selection can.
    Point(std::ops::Range<usize>),
}

/// Resolve a preview buffer selection to its target in the ORIGINAL source.
///
/// The shared first half of [`create_from_selection`], split out because the
/// comment card must pre-populate from the annotations the commit is *going* to merge, so
/// both must resolve the selection to the same source range. Re-deriving the mapping at the
/// card would be a second implementation free to drift, and any drift shows up as the card
/// displaying comments that are not the ones destroyed — worse than no pre-population.
///
/// Returns `None` for an empty/degenerate selection.
pub(crate) fn selection_target(
    copymap: &crate::copymap::CopyTree,
    shifts: &[(usize, usize)],
    cleaned: &str,
    buf_a: i32,
    buf_b: i32,
) -> Option<SelectionTarget> {
    let (lo, hi) = if buf_a <= buf_b {
        (buf_a, buf_b)
    } else {
        (buf_b, buf_a)
    };
    let span = crate::copymap::wrap_span(copymap, cleaned, lo, hi)?;
    if span.start >= span.end {
        return None;
    }
    // `span` is a range in CLEANED bytes (from `wrap_span` over `cleaned`); the
    // SelectionTarget holds ORIGINAL bytes — the N1-typed cleaned→original crossing.
    // `.get()`, not a raw slice (QA round 3, P-3). `span` comes back from
    // `wrap_span` over a copymap built from a DIFFERENT (cleaned) string than the
    // one indexed here, and its ends are byte arithmetic — nothing has proved
    // they land on char boundaries. A raw slice panics on that, and a panic on a
    // selection handler is a process abort. Treating an unusable span as
    // "does not cross a block" is the conservative answer: it produces a
    // Highlight rather than a Point, which the caller can still act on.
    // The block break is extraction's own (`annotate::holds_block_break`): a highlight
    // across one would be written and then read back as literal markup.
    if cleaned
        .get(span.clone())
        .is_some_and(crate::annotate::holds_block_break)
    {
        Some(SelectionTarget::Point(
            crate::annotate::cleaned_to_original(shifts, CleanedByteOffset::new(span.start)).raw()
                ..crate::annotate::cleaned_to_original(shifts, CleanedByteOffset::new(span.end))
                    .raw(),
        ))
    } else {
        Some(SelectionTarget::Highlight(
            crate::annotate::cleaned_to_original(shifts, CleanedByteOffset::new(span.start)).raw()
                ..crate::annotate::cleaned_to_original(shifts, CleanedByteOffset::new(span.end))
                    .raw(),
        ))
    }
}

/// Resolve an EDITOR selection to its target in the original source, independent of any
/// comment — the editor-side sibling of [`selection_target`].
///
/// The shared first half of [`create_from_editor_selection`], split out for the same reason
/// the preview's was: the comment card must pre-populate from the annotations the
/// commit is *going* to merge, so both must resolve the selection to the same source range.
/// Re-deriving the char→byte + balance mapping at the card would be a second implementation
/// free to drift, and any drift shows up as the card displaying comments that are not the
/// ones destroyed — worse than no pre-population at all.
///
/// Returns `None` for an empty/degenerate selection.
pub(crate) fn editor_selection_target(
    source: &str,
    char_a: i32,
    char_b: i32,
) -> Option<SelectionTarget> {
    let (lo, hi) = if char_a <= char_b {
        (char_a, char_b)
    } else {
        (char_b, char_a)
    };
    if lo < 0 || hi <= lo {
        return None;
    }
    let start = char_to_byte(source, lo as usize)?;
    let end = char_to_byte(source, hi as usize)?;
    if start >= end {
        return None;
    }
    // Balance BEFORE the block-crossing test: swallowing a construct can only widen
    // the span, and the widened span is what actually gets wrapped — so it is the one
    // that must be tested for the cross-block point-comment fallback. The caller
    // normalises (ScrAP-75, `NormalizedMd`'s doc): the substitution is length- and
    // position-preserving, so the returned range still indexes `source` unchanged.
    let normalized = crate::renderer::NormalizedMd::new(source);
    let span = crate::copymap::balance_source_span(&normalized, start..end);
    // Same reasoning as the cleaned-side sibling above (QA round 3, P-3):
    // `balance_source_span` returns byte arithmetic over the source, not a
    // proven char boundary.
    if source
        .get(span.clone())
        .is_some_and(crate::annotate::holds_block_break)
    {
        Some(SelectionTarget::Point(span))
    } else {
        Some(SelectionTarget::Highlight(span))
    }
}

/// Byte offset of the `char_off`-th character boundary in `s` (its length when
/// `char_off` equals the character count), or `None` when `char_off` is past the end.
fn char_to_byte(s: &str, char_off: usize) -> Option<usize> {
    s.char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(s.len()))
        .nth(char_off)
}

/// Where the Annotate entry card goes (unit-tested without a display). Given the anchor
/// in overlay-local coords (`ax` = selection-midpoint x, `ay` = selection first-line top),
/// its line height, the card's natural size (`bw`×`bh`), and the overlay's visible size
/// (`ow`×`oh`), return the clamped top-left `(margin_start, margin_top)`.
///
/// It is [`floating_card::place`](crate::widgets::floating_card::place)'s rule, called
/// rather than restated: the entry card replaces the Annotate card in the same spot, so
/// the two must agree on the gap, the flip below when there is no room above, and the
/// clamp inside the overlay (with `clip-overlay` set, GTK would clip a card off an edge).
/// A second copy of that arithmetic had already drifted to a different gap.
pub(crate) fn bar_placement(
    ax: i32,
    ay: i32,
    line_h: i32,
    bw: i32,
    bh: i32,
    ow: i32,
    oh: i32,
) -> (i32, i32) {
    use crate::widgets::floating_card::{place, Placement, Rect, Side};
    let anchor = Rect {
        x: ax,
        y: ay,
        w: 0,
        h: line_h,
    };
    let Placement { x, y, .. } = place(anchor, bw, bh, ow, oh, Side::Top);
    (x, y)
}

#[cfg(test)]
mod tests;
