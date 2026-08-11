//! `BufferText` — the sanctioned way to extract a `GtkTextBuffer`'s text.

use std::ops::Deref;

use gtk::prelude::*;

/// Text extracted from a `GtkTextBuffer` in a form whose character count and
/// offsets stay aligned with the buffer's own `char_count()`,
/// `selection_bounds()`, and `iter.offset()`.
///
/// The guarantee: every anchored child (image, widget) in the extracted range
/// is counted as exactly one `U+FFFC`, so a char offset taken against this
/// string addresses the same character the buffer's iters do. This is why the
/// only constructors extract via [`TextBufferExt::slice`] — GtkTextBuffer's
/// `text()` silently omits anchored children, drifting by one char per anchor
/// against everything that counts them.
///
/// [`of`](Self::of)/[`of_range`](Self::of_range) never fail; the resulting
/// string is empty only when the range is.
pub(crate) struct BufferText(String);

impl BufferText {
    /// The whole buffer's text, offsets aligned with `char_count()`.
    pub(crate) fn of(buffer: &impl IsA<gtk::TextBuffer>) -> Self {
        Self::of_range(buffer, &buffer.start_iter(), &buffer.end_iter())
    }

    /// The text of `[start, end)`, offsets aligned with the buffer's iters.
    pub(crate) fn of_range(
        buffer: &impl IsA<gtk::TextBuffer>,
        start: &gtk::TextIter,
        end: &gtk::TextIter,
    ) -> Self {
        // `slice` (not `text`) keeps the U+FFFC placeholder for every anchored
        // child, so offsets into the result match the buffer's char offsets.
        // `slice` is not the banned method — `text` is (see clippy.toml).
        Self(buffer.slice(start, end, true).to_string())
    }

    /// Borrow the extracted text.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the owned `String`.
    pub(crate) fn into_string(self) -> String {
        self.0
    }

    /// The `GtkTextBuffer` CHAR offset corresponding to `byte_off` in this text.
    /// See [`char_offset_at_byte`] for the contract; this is the same function
    /// with the text supplied by the seam.
    pub(crate) fn char_offset_at(&self, byte_off: usize) -> i32 {
        char_offset_at_byte(self.as_str(), byte_off)
    }

    /// The BYTE offset corresponding to `GtkTextBuffer` char offset `char_off` in this
    /// text. See [`byte_offset_at_char`] for the contract; this is the same function
    /// with the text supplied by the seam.
    pub(crate) fn byte_offset_at(&self, char_off: i32) -> usize {
        byte_offset_at_char(self.as_str(), char_off)
    }
}

/// Convert a **byte** offset into the `GtkTextBuffer` **char** offset the iter
/// API wants, for any byte offset at all — in range or not, on a character
/// boundary or not.
///
/// # Contract
///
/// Returns the char offset of the character *containing* `byte_off`, i.e.
/// `byte_off` floored to a character boundary and then counted. Past the end it
/// clamps to the character count. It cannot panic and has no failure value:
/// every input has a defined, nearby answer.
///
/// # Why this exists rather than three open-codings (QA round 3, P-1)
///
/// Buffer↔source byte offsets come from shift tables, source maps and copymap
/// arithmetic. **Nothing in any of those proves the result lands between
/// characters** — one non-ASCII character anywhere earlier is enough for the
/// arithmetic to address the middle of a multi-byte sequence. Three call sites
/// did this conversion by hand and split two ways, both wrong:
///
/// * `window/scrollsync.rs` sliced raw. A non-boundary offset **panicked**, in a
///   frame-clock tick callback, on a C trampoline, with no `catch_unwind` on the
///   app path — a process abort taking every unsaved buffer in every window.
/// * `window/outline_nav.rs` and `window/annotations_nav.rs` used
///   `text.get(..b).unwrap_or(0)`, which is total but answers **0** — the top of
///   the document. Silently jumping the caret or the paired pane to line 0 is
///   arguably the worse failure: a crash has a backtrace, and "it sometimes
///   scrolls to the beginning" is a bug report nobody can reproduce.
///
/// Flooring is total AND off by at most one character, so it strictly dominates
/// both. Neither sibling used the `0` as a sentinel — each computed it locally
/// and placed a cursor with it — so nothing depended on the old behaviour.
pub(crate) fn char_offset_at_byte(text: &str, byte_off: usize) -> i32 {
    let mut b = byte_off.min(text.len());
    // `str::floor_char_boundary` is unstable, so walk down by hand. Terminates:
    // byte 0 is always a boundary.
    while !text.is_char_boundary(b) {
        b -= 1;
    }
    text[..b].chars().count() as i32
}

/// Convert a `GtkTextBuffer` **char** offset into the **byte** offset that source-space
/// arithmetic wants — the exact inverse of [`char_offset_at_byte`], and total in the
/// same way.
///
/// # Contract
///
/// Returns the byte offset at which the `char_off`-th character begins. Past the end it
/// clamps to `text.len()`; a negative offset clamps to `0`. It cannot panic and has no
/// failure value.
///
/// # Why it belongs beside its inverse
///
/// The direction that already existed converts a stored annotation span into a caret
/// position. This one answers the other half of the same question — *where is the caret,
/// in the space the annotations are recorded in* — which is what a "go to the next
/// annotation from here" walk needs before it can compare anything. Written by hand at
/// the call site it would be the same one-line `chars().count()` slip
/// [`char_offset_at_byte`] documents, only inverted: comparing a raw char offset against
/// byte spans mis-orders every annotation that follows any multi-byte text, so the walk
/// silently skips or repeats one (TDD 20.14's hazard, arriving from the other side).
pub(crate) fn byte_offset_at_char(text: &str, char_off: i32) -> usize {
    let n = char_off.max(0) as usize;
    text.char_indices()
        .nth(n)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

impl Deref for BufferText {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for BufferText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// The contract with teeth: on a buffer that holds an anchored child,
    /// `BufferText` counts that child (one `U+FFFC`) exactly as `char_count`
    /// does — so offsets taken against the extracted string address the same
    /// characters the buffer's iters do.
    ///
    /// Mutation check (GTK4Rs/AP-78): swapping the `of` constructor's `slice`
    /// for the raw `text` makes this assertion FAIL — `text` drops the anchor,
    /// so `chars().count()` comes up one short of `char_count()`. Verified by
    /// hand, then reverted to `slice`. Without the anchor this test would be
    /// vacuous (the editor's own sourceview buffers have none), which is
    /// exactly why it inserts one.
    #[gtktest::test]
    fn buffer_text_counts_anchored_child_like_char_count() {
        let buf = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        buf.set_text("before after");
        // Anchor a child between the two words: char_count and slice both
        // count it as one U+FFFC; text() would omit it.
        let mut at = buf.iter_at_offset(6);
        buf.create_child_anchor(&mut at);

        let extracted = BufferText::of(&buf);
        assert_eq!(
            extracted.chars().count(),
            buf.char_count() as usize,
            "BufferText must count the anchored child like char_count()",
        );
    }
}

#[cfg(test)]
mod byte_offset_tests {
    use super::{byte_offset_at_char, char_offset_at_byte};

    /// The inverse direction, held to the same totality contract: every char offset —
    /// in range, past the end, negative — has a defined byte answer and none panics.
    #[test]
    fn every_char_offset_maps_to_the_byte_that_character_starts_at() {
        // 1 + 2 + 3 + 4 + 1 bytes: one of each UTF-8 width.
        let text = "aé→😀b";
        for (ch, byte) in [(0, 0), (1, 1), (2, 3), (3, 6), (4, 10)] {
            assert_eq!(byte_offset_at_char(text, ch), byte, "char {ch}");
        }
        // One past the last character is the end of the string, not a panic.
        assert_eq!(byte_offset_at_char(text, 5), text.len());
        assert_eq!(byte_offset_at_char(text, 9_999), text.len());
        // A negative offset clamps to the start.
        assert_eq!(byte_offset_at_char(text, -3), 0);
        // Pure ASCII is the identity, so the common path is untouched.
        assert_eq!(byte_offset_at_char("plain ascii", 6), 6);
        assert_eq!(byte_offset_at_char("", 0), 0);
    }

    /// The two directions are inverses on character boundaries — the property the
    /// annotation walk depends on, since it converts the caret one way and compares it
    /// against spans produced the other.
    #[test]
    fn the_two_conversions_round_trip_on_boundaries() {
        let text = "aé→😀b ends";
        for ch in 0..text.chars().count() as i32 {
            let byte = byte_offset_at_char(text, ch);
            assert_eq!(
                char_offset_at_byte(text, byte),
                ch,
                "round trip at char {ch}"
            );
        }
    }
}

#[cfg(test)]
mod char_offset_tests {
    use super::char_offset_at_byte;

    /// QA round 3, P-1. Every byte offset — boundary, interior, past the end —
    /// must yield the char offset of the character containing it, without
    /// panicking and without collapsing to 0.
    ///
    /// Mutation-tested against both pre-fix behaviours: a raw `text[..b]` panics
    /// on every interior case here, and a `get(..b).unwrap_or(0)` answers 0 for
    /// all of them.
    #[test]
    fn every_byte_offset_maps_to_the_character_containing_it() {
        // 1 + 2 + 3 + 4 + 1 bytes: one of each UTF-8 width.
        let text = "aé→😀b";
        assert_eq!(text.len(), 11);

        // Boundaries are exact.
        for (byte, ch) in [(0, 0), (1, 1), (3, 2), (6, 3), (10, 4), (11, 5)] {
            assert_eq!(char_offset_at_byte(text, byte), ch, "boundary byte {byte}");
        }
        // Interior bytes floor to their containing character — NOT to 0.
        for (byte, ch) in [(2, 1), (4, 2), (5, 2), (7, 3), (8, 3), (9, 3)] {
            assert_eq!(char_offset_at_byte(text, byte), ch, "interior byte {byte}");
        }
        // Past the end clamps to the character count.
        assert_eq!(char_offset_at_byte(text, 9_999), 5);
        // Pure ASCII is the identity, so the common path is untouched.
        assert_eq!(char_offset_at_byte("plain ascii", 6), 6);
        // The empty string has exactly one answer.
        assert_eq!(char_offset_at_byte("", 0), 0);
        assert_eq!(char_offset_at_byte("", 7), 0);
    }
}
