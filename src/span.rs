//! [`BufferSpan`] — a named half-open range of `GtkTextBuffer` CHARACTER offsets.
//!
//! The renderer records where each drawn block landed in the preview buffer (a code
//! block's extent, a blockquote's range) and threads those spans through
//! `preview::build`'s `RenderProducts` into `CodePreviewView`, which paints them in
//! its snapshot layer. Those spans used to travel as bare `(i32, i32)` tuples, read
//! back positionally: nothing in the type distinguished a
//! `(start, end)` span from the `(width, height)` and `(x, y)` pairs that also thread
//! through the same layout/draw code, and nothing stopped the two ends being swapped.
//!
//! Offsets are **buffer char offsets** (the `GtkTextIter::offset()` basis — the same
//! one `slice()` and the copymap use, where each anchored child counts as one
//! `U+FFFC` char), NOT byte offsets and NOT source offsets. The range is half-open:
//! `start` is the first char in the span, `end` the first char after it.

/// A half-open span of buffer character offsets, `[start, end)`.
///
/// Construct with [`BufferSpan::new`]; read the ends by NAME (`.start` / `.end`).
/// [`is_empty`](Self::is_empty) folds in the degenerate `end <= start` guard every
/// consumer needs before measuring geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSpan {
    /// First buffer char offset in the span.
    pub(crate) start: i32,
    /// First buffer char offset AFTER the span (half-open).
    pub(crate) end: i32,
}

impl BufferSpan {
    /// A span over `[start, end)`.
    pub(crate) fn new(start: i32, end: i32) -> Self {
        Self { start, end }
    }

    /// `true` when the span covers no character (`end <= start`) — the degenerate
    /// case every drawing consumer must skip before it measures line geometry.
    pub(crate) fn is_empty(self) -> bool {
        self.end <= self.start
    }

    /// The last char offset actually INSIDE the span (`end - 1`).
    ///
    /// Drawing a span's extent measures the last *content* line, not the half-open
    /// `end` (which is the first char of the NEXT line and would over-measure the
    /// block by one line). Only meaningful when the span is non-empty.
    pub(crate) fn last_content(self) -> i32 {
        self.end - 1
    }

    /// `true` when the span lies entirely outside the visible `[vis_start, vis_end]`
    /// char window — the visible-only clamp that keeps a draw from forcing an
    /// off-screen validating read (GTK4Rs/AP-22).
    pub(crate) fn is_outside(self, vis_start: i32, vis_end: i32) -> bool {
        self.last_content() < vis_start || self.start > vis_end
    }
}

/// A byte offset into the **original** Markdown source — the exact text the editor
/// buffer holds verbatim, CriticMarkup delimiters and all.
///
/// # Contract
///
/// This newtype names ONE of three distinct offset spaces the app threads through
/// otherwise-identical bare `usize`/`i32`, where passing one where another is wanted
/// is silent caret-placement corruption:
///
/// - **buffer CHAR offsets** — [`BufferSpan`]; the `GtkTextIter::offset()` basis
///   (each anchored child counts as one `U+FFFC`). NOT this type.
/// - **original-source BYTE offsets** — THIS type: an index into the raw document
///   the editor buffer holds unmodified. It anchors the outline
///   (`outline::Heading::src_offset`) and the annotations viewer
///   (`annotations::AnnotationEntry::src_span`), and every navigation consumer that
///   moves the editor caret to a source position.
/// - **cleaned-source BYTE offsets** — indices into the CriticMarkup-*stripped* text
///   pulldown-cmark parses, now typed as [`CleanedByteOffset`]. A cleaned offset is
///   translated to an original one by [`crate::annotate::cleaned_to_original`] — the
///   SOLE cleaned→original transition — before it ever reaches a consumer named by
///   this type.
///
/// # Byte → char
///
/// A GTK `TextBuffer` indexes by CHARACTER, so a consumer converts an
/// `OriginalByteOffset` to a buffer char offset by counting chars in the UTF-8
/// prefix (`text[..off].chars().count()`). The newtype guards only the PROVENANCE
/// of `off`; the conversion arithmetic is byte-for-byte the pre-newtype code —
/// wrapping never changes a computed caret position.
///
/// Read the value with [`raw`](Self::raw); construct with [`new`](Self::new). The
/// inner field is private, so callers cannot reach `.0` (project rule: no positional
/// tuple access in the public API).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct OriginalByteOffset(usize);

impl OriginalByteOffset {
    /// Name a byte offset as living in the ORIGINAL-source space.
    pub(crate) fn new(offset: usize) -> Self {
        Self(offset)
    }

    /// The underlying byte offset, for the one boundary where it must re-enter raw
    /// `usize` arithmetic: a UTF-8 prefix char-count for caret placement, a `str`
    /// index, or a `usize` a not-yet-migrated module (e.g. `codeview`'s
    /// `marker_index_for_src`) still expects.
    pub(crate) fn raw(self) -> usize {
        self.0
    }
}

/// A byte offset into the **cleaned** (CriticMarkup-stripped) source — the text
/// pulldown-cmark parses, the third of the three offset spaces named in
/// [`OriginalByteOffset`]'s contract. It is NOT interchangeable with an original
/// byte offset (they diverge by every stripped annotation's length) nor with a
/// buffer char offset; the SOLE crossings are
/// [`cleaned_to_original`](crate::annotate::cleaned_to_original)
/// (`CleanedByteOffset` → [`OriginalByteOffset`]) and its inverse
/// [`original_to_cleaned`](crate::annotate::original_to_cleaned)
/// ([`OriginalByteOffset`] → `CleanedByteOffset`). Typing this boundary is N1: a raw
/// `usize` cleaned offset handed to a consumer expecting an original offset was
/// silent caret/scroll displacement (the recurring bug class); now it will not
/// compile.
///
/// Read the value with [`raw`](Self::raw); construct with [`new`](Self::new). The
/// inner field is private (project rule: no positional `.0` in the API).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CleanedByteOffset(usize);

impl CleanedByteOffset {
    /// Name a byte offset as living in the CLEANED-source space.
    pub(crate) fn new(offset: usize) -> Self {
        Self(offset)
    }

    /// The underlying byte offset, for the one boundary where it must re-enter raw
    /// `usize` arithmetic (a `str` index into the cleaned text, or the shift-table
    /// search inside `cleaned_to_original`).
    pub(crate) fn raw(self) -> usize {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{BufferSpan, CleanedByteOffset, OriginalByteOffset};

    #[test]
    fn cleaned_byte_offset_round_trips_through_raw() {
        assert_eq!(CleanedByteOffset::new(42).raw(), 42);
        assert_eq!(CleanedByteOffset::default().raw(), 0);
        // Distinct space from OriginalByteOffset: the two newtypes are not
        // interchangeable even at equal raw values (the compiler enforces it; this
        // just pins that `raw()` is the only bridge).
        assert_eq!(
            CleanedByteOffset::new(7).raw(),
            OriginalByteOffset::new(7).raw()
        );
    }

    #[test]
    fn original_byte_offset_round_trips_through_raw() {
        assert_eq!(OriginalByteOffset::new(42).raw(), 42);
        assert_eq!(OriginalByteOffset::default().raw(), 0);
    }

    #[test]
    fn original_byte_offsets_order_and_compare_by_value() {
        assert!(OriginalByteOffset::new(3) < OriginalByteOffset::new(9));
        assert_eq!(OriginalByteOffset::new(5), OriginalByteOffset::new(5));
        let mut v = [
            OriginalByteOffset::new(9),
            OriginalByteOffset::new(1),
            OriginalByteOffset::new(4),
        ];
        v.sort();
        assert_eq!(v.map(|o| o.raw()), [1, 4, 9]);
    }

    #[test]
    fn ends_are_addressable_by_name() {
        let s = BufferSpan::new(3, 9);
        assert_eq!(s.start, 3);
        assert_eq!(s.end, 9);
    }

    #[test]
    fn is_empty_covers_both_degenerate_shapes() {
        assert!(!BufferSpan::new(3, 9).is_empty());
        assert!(BufferSpan::new(4, 4).is_empty(), "zero-width is empty");
        assert!(BufferSpan::new(9, 3).is_empty(), "inverted is empty");
    }

    #[test]
    fn last_content_is_the_final_char_inside_the_half_open_span() {
        // A 6-char span [3,9) holds chars 3..=8; measuring to 9 would over-reach.
        assert_eq!(BufferSpan::new(3, 9).last_content(), 8);
    }

    #[test]
    fn is_outside_brackets_the_visible_window() {
        let s = BufferSpan::new(10, 20);
        assert!(s.is_outside(30, 40), "entirely before the window");
        assert!(s.is_outside(0, 5), "entirely after the window");
        assert!(!s.is_outside(15, 40), "overlaps the window's start");
        assert!(!s.is_outside(0, 15), "overlaps the window's end");
        // Touching by a single char still counts as visible (inclusive bounds).
        assert!(!s.is_outside(19, 40), "last_content == vis_start");
        assert!(!s.is_outside(0, 10), "start == vis_end");
    }
}
