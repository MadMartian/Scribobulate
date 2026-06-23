//! Selection → Markdown source mapping (display-free, unit-tested).
//!
//! The render loop records a sparse `(buffer_char_offset, source_byte_offset)`
//! waypoint per event; these helpers pick the right source anchor per event,
//! finalize the forward map, and build the inverse index the split scroll-sync
//! binary-searches.

use pulldown_cmark::Event;
use std::ops::Range;

/// Source-offset waypoint to record for an event spanning `range`.
///
/// `Start`/`Text`/inline events anchor at the element's source *start* — the
/// glyphs they emit begin there.  `End` events must anchor at the source *end*:
/// their `range` spans the whole element (e.g. `0..10` for a heading), so
/// recording `range.start` points the waypoint *backwards* to the block start.
/// That makes the (buf_offset, src_offset) map non-monotonic, and a selection
/// confined to one block collapses to an empty source slice — the
/// copy-a-lone-heading regression.
pub(super) fn waypoint_src_offset(ev: &Event, range: &Range<usize>) -> usize {
    if matches!(ev, Event::End(_)) {
        range.end
    } else {
        range.start
    }
}

/// Build the source→buffer inverse index from a `(buffer_char, source_byte)` map:
/// swap the columns and sort by source byte, so a source offset can be
/// binary-searched to its buffer offset (the split scroll-sync).
pub(crate) fn invert_source_map(source_map: &[(i32, usize)]) -> Vec<(usize, i32)> {
    let mut inv: Vec<(usize, i32)> = source_map.iter().map(|&(bc, sb)| (sb, bc)).collect();
    inv.sort_unstable_by_key(|&(sb, _)| sb);
    inv
}

/// Append the end sentinel and collapse to one waypoint per buffer offset.
pub(super) fn finalize_source_map(
    mut map: Vec<(i32, usize)>,
    end_buf: i32,
    md_len: usize,
) -> Vec<(i32, usize)> {
    map.push((end_buf, md_len)); // sentinel at end
    map.dedup_by_key(|e| e.0); // one entry per buf offset
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::TagEnd;

    #[test]
    fn waypoint_anchors_end_events_at_the_source_end() {
        // An End event spans the whole element; anchoring at `range.start` would
        // point the waypoint backwards to the block start (the lone-heading copy
        // regression), so it must record `range.end`.
        let end = Event::End(TagEnd::Heading(pulldown_cmark::HeadingLevel::H1));
        assert_eq!(waypoint_src_offset(&end, &(0..10)), 10);
        // Every non-End event anchors at the source start (where its glyphs begin).
        assert_eq!(waypoint_src_offset(&Event::Text("x".into()), &(3..7)), 3);
        assert_eq!(waypoint_src_offset(&Event::SoftBreak, &(5..6)), 5);
    }

    #[test]
    fn invert_swaps_columns_and_sorts_by_source_byte() {
        // Forward map is sorted by buffer offset; the source column is only
        // near-monotonic, so the inverse must sort by source byte for binary search.
        let fwd = vec![(0, 10), (5, 2), (9, 20), (12, 2)];
        let inv = invert_source_map(&fwd);
        assert_eq!(inv, vec![(2, 5), (2, 12), (10, 0), (20, 9)]);
        // Sorted ascending by the source-byte key.
        assert!(inv.windows(2).all(|w| w[0].0 <= w[1].0));
    }

    #[test]
    fn invert_of_empty_is_empty() {
        assert!(invert_source_map(&[]).is_empty());
    }

    #[test]
    fn finalize_appends_sentinel_and_dedups_by_buffer_offset() {
        // Two waypoints at the same buffer offset collapse to the first; the end
        // sentinel `(end_buf, md_len)` is appended.
        let map = vec![(0, 0), (0, 3), (4, 5)];
        assert_eq!(
            finalize_source_map(map, 10, 42),
            vec![(0, 0), (4, 5), (10, 42)]
        );
    }

    #[test]
    fn finalize_on_empty_map_is_just_the_sentinel() {
        assert_eq!(finalize_source_map(Vec::new(), 7, 99), vec![(7, 99)]);
    }
}
