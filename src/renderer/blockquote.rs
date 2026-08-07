//! The per-line content split for margin-tag application — pure and unit-tested. The
//! GTK side (`apply_tag_per_line`, in [`super::emit`]) applies a margin tag (the
//! `blockquote` indent, or a `li-{depth}` list hanging-indent) over each range this
//! returns.

/// Char-offset ranges of each logical line's CONTENT within `[start, end)`, EXCLUDING
/// the terminating `\n` of every line. `text` is the buffer slice for `[start, end)`
/// (`GtkTextBuffer::slice`, so each anchored child counts as one `U+FFFC` char, keeping
/// char offsets aligned — ScrAP-74); returned ranges are absolute buffer char
/// offsets. Pure (no GTK) so it is unit-tested; the newline-excluding split is what
/// gives every tagged line its own tag toggle (GTK4Rs/AP-72).
pub(super) fn logical_line_ranges(text: &str, start: i32, end: i32) -> Vec<(i32, i32)> {
    let mut ranges = Vec::new();
    let mut line_start = start;
    for (i, ch) in text.chars().enumerate() {
        if ch == '\n' {
            let off = start + i as i32;
            if off > line_start {
                ranges.push((line_start, off));
            }
            line_start = off + 1; // skip the '\n' — leave it untagged
        }
    }
    if end > line_start {
        ranges.push((line_start, end));
    }
    ranges
}

#[cfg(test)]
mod logical_line_ranges_tests {
    use super::logical_line_ranges;

    #[test]
    fn splits_each_line_excluding_newlines() {
        // "abc\nde\nf" over [10..18]: content runs are 10..13, 14..16, 17..18;
        // the two '\n's (offsets 13 and 16) are left out — that is what creates a
        // per-line tag toggle (GTK4Rs/AP-72).
        assert_eq!(
            logical_line_ranges("abc\nde\nf", 10, 18),
            vec![(10, 13), (14, 16), (17, 18)]
        );
    }

    #[test]
    fn single_line_is_one_range() {
        assert_eq!(logical_line_ranges("hello", 0, 5), vec![(0, 5)]);
    }

    #[test]
    fn empty_lines_produce_no_range() {
        // A blank quoted line ("a\n\nb"): the empty middle line yields no range (no
        // content to tag), leaving both '\n's untagged.
        assert_eq!(logical_line_ranges("a\n\nb", 0, 4), vec![(0, 1), (3, 4)]);
    }

    #[test]
    fn multibyte_chars_count_as_one_offset_each() {
        // "é" and "🚀" are single chars (multi-byte) — offsets are CHAR counts, not
        // bytes, matching GtkTextBuffer char offsets.
        assert_eq!(logical_line_ranges("é🚀\nx", 0, 4), vec![(0, 2), (3, 4)]);
    }
}
