//! The one document coordinate the editor pane and the preview pane both agree on,
//! and the conversions each pane needs to reach it.
//!
//! # Why this exists
//!
//! The two panes hold *different text*. The editor holds the document's ORIGINAL
//! source; the preview holds a rendered buffer built from the CLEANED source
//! (CriticMarkup extracted), whose characters correspond to source bytes only
//! through the render's waypoint map. So "where is the reader?" has no shared
//! answer in either pane's own coordinates, and the two obvious substitutes are
//! both wrong:
//!
//! * A **scroll fraction** is a ratio of two view-specific content heights. The
//!   panes never have the same height, and the preview is rebuilt on every entry
//!   into a preview-visible mode, so a fraction loses precision at both ends of
//!   every conversion. Carried across repeated view-mode round trips it does not
//!   merely round — it *accumulates*, walking the reader through the document in
//!   one direction until it clamps at the end (measured: four preview↔split trips
//!   moved a 40-section fixture's top line 79 → 110 → 152 → 158, terminating at
//!   the document's end).
//! * A **buffer line or char offset** is meaningful in exactly one of the two
//!   panes and silently wrong in the other.
//!
//! [`DocPosition`] is the shared answer: a byte offset into the original source.
//! Every pane can convert to it and back, the conversions are lossless up to the
//! render map's own waypoint granularity, and — the property the fraction lacks —
//! converting *out* and back *in* is idempotent, so a round trip cannot drift.
//!
//! # Who uses it
//!
//! Both position hand-offs in the app, deliberately through this one module rather
//! than each doing its own arithmetic (POLICY: prefer extending an existing path):
//!
//! * `window::scrollsync::project_scroll` — the per-frame split-pane projection,
//!   where one pane drives and the other follows.
//! * `window::scrollsync`'s view-mode capture/apply — the reading position carried
//!   across a mode switch, where the destination pane may not exist yet at capture
//!   time.
//!
//! # What is NOT here
//!
//! Nothing that touches GTK. This module is pure arithmetic over `&str` and the two
//! waypoint maps, which is what lets it be unit-tested without a display — the
//! live-widget half (reading a viewport's top iter, writing an adjustment) stays in
//! `window::scrollsync`.

use crate::span::{CleanedByteOffset, OriginalByteOffset};

/// A reading position, as a byte offset into the document's ORIGINAL source.
///
/// The original source is the coordinate space chosen because the editor buffer
/// *is* it — so the pane a user most often reads from converts for free, and only
/// the preview pays for the translation. It also survives a preview rebuild, which
/// a preview buffer offset does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DocPosition(OriginalByteOffset);

impl DocPosition {
    /// The start of the document — the position a load baseline puts the caret at,
    /// and the fallback whenever a pane cannot answer where the reader is.
    pub(crate) fn start() -> Self {
        Self(OriginalByteOffset::new(0))
    }

    pub(crate) fn new(offset: OriginalByteOffset) -> Self {
        Self(offset)
    }

    pub(crate) fn original(self) -> OriginalByteOffset {
        self.0
    }
}

/// Byte offset in `text` of the char at `char_off` (clamped to the end).
pub(crate) fn char_to_byte(text: &str, char_off: usize) -> usize {
    text.char_indices()
        .nth(char_off)
        .map_or(text.len(), |(b, _)| b)
}

/// Char offset in `text` of the char containing `byte_off`.
///
/// Delegates to the shared seam (QA round 3, P-1). This used to slice raw, which
/// PANICKED — i.e. aborted the process, from a tick callback on a C trampoline —
/// on a byte offset that landed inside a multi-byte character. Byte offsets here
/// come from the shift table, which does arithmetic and proves nothing about
/// character boundaries.
pub(crate) fn byte_to_char(text: &str, byte_off: usize) -> i32 {
    crate::saferizer::buffer_text::char_offset_at_byte(text, byte_off)
}

/// Forward map: preview buffer char offset -> cleaned source byte offset. Largest
/// waypoint whose buffer offset is <= `buf_char` (the map is sorted by buffer
/// offset).
pub(crate) fn buf_char_to_src_byte(map: &[(i32, usize)], buf_char: i32) -> usize {
    let i = map.partition_point(|&(bc, _)| bc <= buf_char);
    i.checked_sub(1).map_or(0, |j| {
        let (_buf_char, src_byte) = map[j];
        src_byte
    })
}

/// Inverse map: cleaned source byte offset -> preview buffer char offset.
/// Binary-searches the render's `source_map_inv` (sorted by source byte) for the
/// waypoint with the largest source byte offset <= `src_byte`.
pub(crate) fn src_byte_to_buf_char(inv: &[(usize, i32)], src_byte: usize) -> i32 {
    let i = inv.partition_point(|&(sb, _)| sb <= src_byte);
    i.checked_sub(1).map_or(0, |j| {
        let (_src_byte, buf_char) = inv[j];
        buf_char
    })
}

/// Editor char offset -> [`DocPosition`]. The editor buffer holds the original
/// source, so this is a plain char->byte conversion with no map involved.
pub(crate) fn from_editor_char(original: &str, char_off: i32) -> DocPosition {
    let byte = char_to_byte(original, char_off.max(0) as usize);
    DocPosition::new(OriginalByteOffset::new(byte))
}

/// [`DocPosition`] -> editor char offset.
pub(crate) fn to_editor_char(original: &str, pos: DocPosition) -> i32 {
    byte_to_char(original, pos.original().raw())
}

/// Preview buffer char offset -> [`DocPosition`].
///
/// Two hops, and both are required: the render's waypoint map answers in CLEANED
/// bytes (CriticMarkup extracted), and the shift table carries cleaned back to
/// original. Both are the identity when the document carries no annotations, so an
/// un-annotated document costs the same as a direct conversion.
pub(crate) fn from_preview_char(
    source_map: &[(i32, usize)],
    shifts: &[(usize, usize)],
    buf_char: i32,
) -> DocPosition {
    let cleaned = buf_char_to_src_byte(source_map, buf_char);
    let original = crate::annotate::cleaned_to_original(shifts, CleanedByteOffset::new(cleaned));
    DocPosition::new(original)
}

/// [`DocPosition`] -> preview buffer char offset. The inverse of
/// [`from_preview_char`], through the same two hops in the other order.
pub(crate) fn to_preview_char(
    source_map_inv: &[(usize, i32)],
    shifts: &[(usize, usize)],
    pos: DocPosition,
) -> i32 {
    let cleaned = crate::annotate::original_to_cleaned(shifts, pos.original()).raw();
    src_byte_to_buf_char(source_map_inv, cleaned)
}

/// The start of an editor LINE -> preview buffer char, for the split pane's
/// editor-driven projection: the first waypoint STRICTLY inside the line (after its
/// first byte), otherwise [`to_preview_char`]'s at-or-before.
///
/// **Why the at-or-before lookup is wrong here.** A block's content waypoint sits where
/// its CONTENT begins, after any markup: a `### Heading` line's sits past the `### `.
/// The editor's top-of-viewport position is the line's first byte, and what the map
/// holds AT or before that byte describes the space before the block, never the block:
///
/// * after a paragraph, nothing at the line start — the at-or-before lookup lands on
///   the paragraph's last waypoint, and the preview shows that paragraph's line above
///   the heading (MEASURED on a reader parked on a heading in Split: 72 of the 88 px
///   the preview settled above the heading; the other 16 px was the adjustment margin,
///   `ViewportTopIter::value_for_top_y`);
/// * after a list, a waypoint exactly AT the line start whose buffer char is the end of
///   the list's last line, before the block separator (MEASURED on `sdd/TDD.md`'s
///   `### 7.5`: line start → the last bullet of 7.4; `+4` bytes → the heading). So
///   "the first waypoint in the line" is not enough; it has to be past the first byte.
///
/// A line whose only waypoint is at its first byte, or that holds none at all (a blank
/// line, a fence), keeps the at-or-before answer. For a blank line that is the block
/// before it — accepted: the reader is between blocks there.
///
/// Cost: one scan from `line_start` to the next newline, plus a binary search — bounded
/// by the line, not the document (Hot-path CAM row 1).
pub(crate) fn line_to_preview_char(
    source_map_inv: &[(usize, i32)],
    shifts: &[(usize, usize)],
    original: &str,
    line_start: DocPosition,
) -> i32 {
    let at_or_before = to_preview_char(source_map_inv, shifts, line_start);
    let start = line_start.original().raw();
    let end = original
        .get(start..)
        .and_then(|rest| rest.find('\n'))
        .map_or(original.len(), |n| start + n);
    let clean = |byte: usize| {
        crate::annotate::original_to_cleaned(shifts, OriginalByteOffset::new(byte)).raw()
    };
    let (clean_start, clean_end) = (clean(start), clean(end));
    let i = source_map_inv.partition_point(|&(sb, _)| sb <= clean_start);
    match source_map_inv.get(i) {
        Some(&(src_byte, buf_char)) if src_byte < clean_end => buf_char,
        _ => at_or_before,
    }
}

/// A held reading position -> preview buffer char, for a pane opened onto a position
/// captured from EITHER pane (a view-mode switch): [`line_to_preview_char`] when `pos` is
/// the first byte of a source line, otherwise [`to_preview_char`].
///
/// A position the editor's viewport produced is a line start whenever its top line is
/// the first display line of a source line, which is exactly where a reader parked on a
/// heading sits; the at-or-before lookup then lands on the block above (MEASURED: an Edit
/// reader on a heading switched to Preview opened two lines up, on the end of the
/// paragraph or list before it). A position the preview produced comes from a waypoint,
/// and a heading's waypoint is past its `#`s, so it is not a line start and keeps the
/// exact inverse. A preview waypoint that IS a line start (a paragraph's first byte)
/// gets the first waypoint further along the same source line, which lies on the same
/// preview buffer line.
pub(crate) fn reading_to_preview_char(
    source_map_inv: &[(usize, i32)],
    shifts: &[(usize, usize)],
    original: &str,
    pos: DocPosition,
) -> i32 {
    let byte = pos.original().raw();
    let line_start = byte == 0 || original.as_bytes().get(byte - 1) == Some(&b'\n');
    if line_start && byte <= original.len() {
        line_to_preview_char(source_map_inv, shifts, original, pos)
    } else {
        to_preview_char(source_map_inv, shifts, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Local mirror of the render's own inversion (`preview::sourcemap`), so these
    /// tests exercise this module's arithmetic without reaching into a private
    /// module of another. Sorted by source byte, which is what
    /// [`src_byte_to_buf_char`] binary-searches.
    fn invert(map: &[(i32, usize)]) -> Vec<(usize, i32)> {
        let mut inv: Vec<(usize, i32)> = map.iter().map(|&(bc, sb)| (sb, bc)).collect();
        inv.sort_unstable();
        inv
    }

    /// The pre-fix call sliced raw and aborted the process on an offset landing
    /// inside a multi-byte char. Pinned at this seam as well as in the seam's own
    /// tests, because what matters here is that this module DELEGATES — an inlined
    /// re-implementation would pass the seam's tests while re-arming the abort.
    #[test]
    fn a_byte_offset_inside_a_multibyte_char_floors_instead_of_panicking() {
        let text = "aé→😀b";
        assert_eq!(byte_to_char(text, 2), 1, "inside 'é'");
        assert_eq!(byte_to_char(text, 5), 2, "inside '→'");
        assert_eq!(byte_to_char(text, 8), 3, "inside '😀'");
        assert_eq!(byte_to_char(text, 9_999), 5, "past the end clamps");
    }

    /// The paired direction was already total (`nth` yields `None` past the end);
    /// pinned so the pair keeps agreeing at the boundaries.
    #[test]
    fn char_to_byte_round_trips_with_byte_to_char_on_boundaries() {
        let text = "aé→😀b";
        for c in 0..=5 {
            let b = char_to_byte(text, c);
            assert_eq!(byte_to_char(text, b), c as i32, "round trip at char {c}");
        }
    }

    /// **The property the scroll fraction lacked**, and the reason this type
    /// exists: converting a position OUT of a pane and back IN returns the same
    /// position, so repeating the trip cannot accumulate.
    ///
    /// Asserted over many trips rather than one. A single round trip is satisfied
    /// by any conversion accurate to within its own rounding — it is the REPEAT
    /// that separates a lossless mapping from a lossy one, which is exactly how
    /// the fraction hand-off passed inspection while walking the reader to the end
    /// of the document over four trips.
    #[test]
    fn an_editor_round_trip_is_idempotent_over_many_repeats() {
        let original = "# Heading\n\nSome prose with é and 😀 in it.\n\n## Next\n\nMore.\n";
        for start_char in 0..original.chars().count() as i32 {
            let mut pos = from_editor_char(original, start_char);
            let first = pos;
            for trip in 0..16 {
                let ch = to_editor_char(original, pos);
                pos = from_editor_char(original, ch);
                assert_eq!(
                    pos, first,
                    "trip {trip} moved the position from char {start_char}"
                );
            }
        }
    }

    /// The same idempotence across the PANE BOUNDARY — editor to preview and back
    /// — which is the conversion a view-mode round trip actually performs.
    ///
    /// The positions asserted on are waypoint positions, because that is the
    /// granularity the map can represent: a position between two waypoints
    /// resolves to the earlier one, and the guarantee is that it then STAYS there
    /// rather than continuing to slide, which the loop below is what proves.
    #[test]
    fn a_cross_pane_round_trip_settles_and_then_does_not_move() {
        // Waypoints: (preview buffer char, cleaned source byte).
        let source_map: Vec<(i32, usize)> = vec![(0, 0), (10, 12), (25, 40), (60, 90)];
        let source_map_inv = invert(&source_map);
        let shifts: Vec<(usize, usize)> = vec![(0, 0)];

        for buf_char in [0, 3, 10, 17, 25, 44, 60, 120] {
            let pos = from_preview_char(&source_map, &shifts, buf_char);
            // Settle once: a position between waypoints lands on the earlier one.
            let settled = to_preview_char(&source_map_inv, &shifts, pos);
            let mut here = settled;
            for trip in 0..16 {
                let p = from_preview_char(&source_map, &shifts, here);
                here = to_preview_char(&source_map_inv, &shifts, p);
                assert_eq!(
                    here, settled,
                    "trip {trip} moved a settled position (from buf_char {buf_char})"
                );
            }
        }
    }

    /// A document with no annotations must cost nothing: the shift table is the
    /// identity, so the cleaned and original coordinates coincide and the preview
    /// conversion reduces to the map lookup alone.
    #[test]
    fn an_unannotated_document_maps_cleaned_and_original_alike() {
        let source_map: Vec<(i32, usize)> = vec![(0, 0), (10, 10), (25, 25)];
        let shifts: Vec<(usize, usize)> = vec![(0, 0)];
        let pos = from_preview_char(&source_map, &shifts, 12);
        assert_eq!(
            pos.original().raw(),
            10,
            "identity shifts leave the waypoint's byte untouched"
        );
    }

    /// The split projection's line lookup, on the map shape a heading actually has:
    /// its first waypoint sits after the `### `, so the line's start byte holds none.
    #[test]
    fn a_line_whose_markup_precedes_its_content_maps_to_its_own_first_waypoint() {
        let original = "para one\n\n### Heading\n\nmore\n";
        let heading_line = original.find("###").expect("fixture");
        let content = original.find("Heading").expect("fixture");
        let blank_line = original.find("\n\n").expect("fixture") + 1;
        let more = original.find("more").expect("fixture");
        // (preview buffer char, cleaned source byte)
        let source_map: Vec<(i32, usize)> = vec![(0, 0), (4, 5), (10, content), (18, more)];
        let inv = invert(&source_map);
        let shifts: Vec<(usize, usize)> = vec![(0, 0)];
        let at = |byte| DocPosition::new(OriginalByteOffset::new(byte));

        assert_eq!(
            to_preview_char(&inv, &shifts, at(heading_line)),
            4,
            "control: the at-or-before lookup lands on the paragraph above"
        );
        assert_eq!(
            line_to_preview_char(&inv, &shifts, original, at(heading_line)),
            10,
            "the heading line maps to the heading"
        );
        assert_eq!(
            line_to_preview_char(&inv, &shifts, original, at(0)),
            4,
            "a paragraph line takes its first waypoint past the start, which is on the \
             paragraph's own preview line"
        );
        assert_eq!(
            line_to_preview_char(&inv, &shifts, "x\n", at(0)),
            0,
            "a line whose only waypoint is at its start keeps the at-or-before answer"
        );
        assert_eq!(
            line_to_preview_char(&inv, &shifts, original, at(blank_line)),
            4,
            "a blank line has no waypoint of its own and keeps the block before it"
        );
        assert_eq!(
            line_to_preview_char(&inv, &shifts, original, at(more)),
            18,
            "the last line"
        );
    }

    /// The other shape `sdd/TDD.md` showed: after a LIST the map holds a waypoint AT
    /// the heading line's first byte, pointing at the end of the list's last line. The
    /// lookup must look past it to the heading's own waypoint.
    #[test]
    fn a_waypoint_at_the_line_start_does_not_stop_the_lookup() {
        let original = "- item\n\n### Heading\n";
        let heading_line = original.find("###").expect("fixture");
        let content = original.find("Heading").expect("fixture");
        // (preview buffer char, cleaned source byte): the item, the end of its line
        // (keyed at the heading line's start), the heading text two separators on.
        let source_map: Vec<(i32, usize)> = vec![(0, 2), (4, heading_line), (6, content)];
        let inv = invert(&source_map);
        let shifts: Vec<(usize, usize)> = vec![(0, 0)];
        let pos = DocPosition::new(OriginalByteOffset::new(heading_line));
        assert_eq!(
            to_preview_char(&inv, &shifts, pos),
            4,
            "control: the at-or-before lookup takes the waypoint at the line start"
        );
        assert_eq!(line_to_preview_char(&inv, &shifts, original, pos), 6);
    }

    /// The line's bounds cross into the map's CLEANED coordinates through the shift
    /// table. Comparing original bytes with cleaned waypoints directly is right only
    /// for an un-annotated document.
    #[test]
    fn the_line_lookup_compares_in_cleaned_coordinates() {
        // `{>>c<<}` (7 bytes) is removed from the cleaned source, so every later
        // original byte is 7 ahead of its cleaned byte.
        let original = "{>>c<<}x\n\n## H\n";
        let heading_line = original.find("##").expect("fixture");
        let content_cleaned = original.find('H').expect("fixture") - 7;
        let source_map: Vec<(i32, usize)> = vec![(0, 0), (3, content_cleaned)];
        let inv = invert(&source_map);
        let shifts: Vec<(usize, usize)> = vec![(0, 0), (0, 7)];
        let pos = DocPosition::new(OriginalByteOffset::new(heading_line));
        assert_eq!(
            crate::annotate::original_to_cleaned(&shifts, pos.original()).raw(),
            heading_line - 7,
            "precondition: the shift table carries original to cleaned as assumed"
        );
        assert_eq!(line_to_preview_char(&inv, &shifts, original, pos), 3);
    }

    /// The start of the document is representable and converts to the start of
    /// each pane — the fallback every capture falls back to.
    #[test]
    fn the_start_of_the_document_converts_to_the_start_of_each_pane() {
        let original = "# Heading\n\nprose\n";
        let source_map: Vec<(i32, usize)> = vec![(0, 0), (10, 12)];
        let inv = invert(&source_map);
        let shifts: Vec<(usize, usize)> = vec![(0, 0)];
        assert_eq!(to_editor_char(original, DocPosition::start()), 0);
        assert_eq!(to_preview_char(&inv, &shifts, DocPosition::start()), 0);
    }

    /// A view-mode switch hands the incoming preview a position from either pane: a line
    /// start takes the line lookup (the heading, not the list above it), a mid-line
    /// position keeps the exact inverse.
    #[test]
    fn a_held_position_takes_the_line_lookup_only_at_a_line_start() {
        let original = "- item\n\n### Heading\n";
        let heading_line = original.find("###").expect("fixture");
        let content = original.find("Heading").expect("fixture");
        let source_map: Vec<(i32, usize)> = vec![(0, 2), (4, heading_line), (6, content)];
        let inv = invert(&source_map);
        let shifts: Vec<(usize, usize)> = vec![(0, 0)];
        let at = |b: usize| DocPosition::new(OriginalByteOffset::new(b));
        assert_eq!(
            reading_to_preview_char(&inv, &shifts, original, at(heading_line)),
            6
        );
        assert_eq!(
            reading_to_preview_char(&inv, &shifts, original, at(content)),
            6,
            "a position the preview produced (the heading's own waypoint) maps exactly"
        );
        assert_eq!(
            reading_to_preview_char(&inv, &shifts, original, at(3)),
            to_preview_char(&inv, &shifts, at(3)),
            "a mid-line position keeps the at-or-before lookup"
        );
    }

    /// The case the line-start test exists for: a mid-line position on a line that holds a
    /// LATER waypoint. The line lookup would jump forward to that waypoint; the held
    /// position must keep the one at or before it. The fixture above has no waypoint
    /// after its mid-line byte, so there both lookups agree and cannot tell them apart.
    #[test]
    fn a_mid_line_position_does_not_jump_to_a_later_waypoint_on_its_line() {
        let original = "alpha beta gamma\n";
        let beta = original.find("beta").expect("fixture");
        let gamma = original.find("gamma").expect("fixture");
        let source_map: Vec<(i32, usize)> = vec![(0, 0), (10, beta), (20, gamma)];
        let inv = invert(&source_map);
        let shifts: Vec<(usize, usize)> = vec![(0, 0)];
        let inside_beta = DocPosition::new(OriginalByteOffset::new(beta + 1));
        assert_eq!(
            line_to_preview_char(&inv, &shifts, original, inside_beta),
            20,
            "control: the line lookup from here takes the later waypoint"
        );
        assert_eq!(
            reading_to_preview_char(&inv, &shifts, original, inside_beta),
            10,
            "a mid-line position keeps the waypoint at or before it"
        );
    }
}
