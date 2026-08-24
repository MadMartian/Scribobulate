//! What a construct BECOMES, decided before any toolkit is involved — **pure, and
//! deliberately free of Pango and cairo**.
//!
//! Every function here answers a question with a definite answer that no measurement can
//! change: how many columns a table has, whether a paragraph contains an image and where
//! it splits, and what glyph introduces a list item. None of it needs a `pango::Context`
//! — and the list marker in particular had never been directly tested, because the only
//! way to reach it was to build a document, build a context, and run the whole
//! measurement pass to see what came out the other end.
//!
//! The module doc next door says "if this file grows a decision, logic has leaked into
//! it". It had. This is where those decisions live now, where a unit test can ask them
//! directly.

use crate::export::{ImageRef, Inline};

/// How many columns a table has: the delimiter row's count, which the header carries,
/// falling back to the widest body row for a table whose header is empty.
pub(crate) fn table_column_count(head: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) -> usize {
    head.len().max(rows.iter().map(Vec::len).max().unwrap_or(0))
}
/// One run of a paragraph: prose, or an image that interrupts it.
pub(crate) enum Seg {
    Text(Vec<Inline>),
    Image(ImageRef),
}

/// Split a paragraph's inlines into prose runs and the images between them.
///
/// Recurses into containers, so `[![badge](b.png)](https://…)` — a link wrapping an
/// image, which is how every status badge in a README is written — yields the image
/// rather than a note. A container that holds **both** an image and text loses that
/// container's emphasis on the text either side; that is a deliberate, bounded
/// degradation, and the alternative is re-wrapping split runs, which buys typography
/// nobody writes at the cost of real complexity.
pub(crate) fn split_on_images(inlines: &[Inline]) -> Vec<Seg> {
    let mut out: Vec<Seg> = Vec::new();
    collect_segs(inlines, &mut out);
    out
}

fn collect_segs(inlines: &[Inline], out: &mut Vec<Seg>) {
    for inline in inlines {
        match inline {
            Inline::Image(img) => out.push(Seg::Image(img.clone())),
            _ if contains_image(inline) => match inline {
                Inline::Emphasis(v)
                | Inline::Strong(v)
                | Inline::Strikethrough(v)
                | Inline::Superscript(v)
                | Inline::Subscript(v)
                | Inline::Highlight(v)
                | Inline::Claim(_, v) => collect_segs(v, out),
                Inline::Link { inner, .. } => collect_segs(inner, out),
                other => push_text(out, other.clone()),
            },
            other => push_text(out, other.clone()),
        }
    }
}

fn push_text(out: &mut Vec<Seg>, inline: Inline) {
    match out.last_mut() {
        Some(Seg::Text(run)) => run.push(inline),
        _ => out.push(Seg::Text(vec![inline])),
    }
}

/// Whether an inline holds an image anywhere inside it.
pub(crate) fn contains_image(inline: &Inline) -> bool {
    match inline {
        Inline::Image(_) => true,
        Inline::Emphasis(v)
        | Inline::Strong(v)
        | Inline::Strikethrough(v)
        | Inline::Superscript(v)
        | Inline::Subscript(v)
        | Inline::Highlight(v)
        | Inline::Claim(_, v) => v.iter().any(contains_image),
        Inline::Link { inner, .. } => inner.iter().any(contains_image),
        _ => false,
    }
}

/// The glyph that introduces one list item — the checkbox, the number, or the bullet.
///
/// `n` is the item's zero-based position, `start` the ordered list's first number
/// (`None` for an unordered list), and `task` the checkbox state (`None` when the item
/// is not a task).
///
/// **A checkbox is its Unicode glyph, not a widget.** An exported artefact is a record,
/// and a control the reader could press would imply an edit that goes nowhere.
///
/// The trailing `\u{00a0}` is a NO-BREAK space in every arm, and that is load-bearing
/// rather than decorative: the marker is prepended to the item's first line and handed
/// to Pango as one markup run, so an ordinary space would let the wrap algorithm break
/// between the bullet and the word it introduces, stranding a lone `•` at the end of a
/// line. It is easy to "tidy" and the damage only shows on a narrow column.
pub(crate) fn list_marker(task: Option<bool>, start: Option<u64>, n: usize) -> String {
    match (task, start) {
        (Some(true), _) => "\u{2611}\u{00a0}".to_string(),
        (Some(false), _) => "\u{2610}\u{00a0}".to_string(),
        // A task item keeps its checkbox even inside an ordered list, which is why the
        // task arms come first: `1. [x] done` is a checkbox, not a number.
        (None, Some(s)) => format!("{}.\u{00a0}", s + n as u64),
        (None, None) => "\u{2022}\u{00a0}".to_string(),
    }
}

/// Which entry of the theme's heading scale a heading of `level` reads.
///
/// **The clamp is the whole function.** `heading_scale` has five entries and Markdown
/// has six heading levels, so `level - 1` indexes out of bounds for an `h6` — a panic on
/// a document a reader can trivially write. The bound was previously an inline
/// `.min(4)` inside the measurement pass, where reaching it meant exporting a document
/// with an `h6` in it.
pub(crate) fn heading_scale_index(level: u8) -> usize {
    (level as usize).saturating_sub(1).min(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every list marker shape, including the ones that only differ by an argument the
    /// other arms ignore. Table-driven because the function IS a table.
    #[test]
    fn a_list_item_gets_the_marker_its_kind_calls_for() {
        let cases: &[(Option<bool>, Option<u64>, usize, &str)] = &[
            (None, None, 0, "\u{2022}\u{00a0}"),
            (None, None, 7, "\u{2022}\u{00a0}"),
            (None, Some(1), 0, "1.\u{00a0}"),
            (None, Some(1), 4, "5.\u{00a0}"),
            // An ordered list need not start at 1 — `5.` in the source is honoured.
            (None, Some(5), 0, "5.\u{00a0}"),
            (None, Some(5), 2, "7.\u{00a0}"),
            (Some(false), None, 0, "\u{2610}\u{00a0}"),
            (Some(true), None, 0, "\u{2611}\u{00a0}"),
            // A checkbox wins over the number: `1. [x] done` is a task item.
            (Some(true), Some(1), 3, "\u{2611}\u{00a0}"),
        ];
        for &(task, start, n, want) in cases {
            assert_eq!(
                list_marker(task, start, n),
                want,
                "task={task:?} start={start:?} n={n}"
            );
        }
    }

    /// Every marker ends in a NO-BREAK space, so Pango cannot wrap between the marker
    /// and the word it introduces. Asserted as a property over all arms rather than
    /// spelled into each expectation above, where it reads as incidental.
    #[test]
    fn every_marker_binds_to_its_text_with_a_no_break_space() {
        for (task, start) in [
            (None, None),
            (None, Some(1)),
            (Some(true), None),
            (Some(false), Some(3)),
        ] {
            let marker = list_marker(task, start, 0);
            assert!(
                marker.ends_with('\u{00a0}'),
                "task={task:?} start={start:?} produced {marker:?}, which Pango may \
                 wrap away from its item text"
            );
            assert!(
                !marker.ends_with(' '),
                "task={task:?} start={start:?} used an ordinary space"
            );
        }
    }

    /// An ordered list whose numbering would overflow does not panic. `start` is a
    /// `u64` straight out of the parser, so this is reachable from a document.
    #[test]
    fn an_absurd_ordered_start_does_not_panic() {
        let marker = list_marker(None, Some(u64::MAX), 0);
        assert!(marker.starts_with(&u64::MAX.to_string()));
    }

    /// Every Markdown heading level indexes inside the theme's five-entry scale.
    #[test]
    fn every_heading_level_indexes_inside_the_scale() {
        const SCALE_LEN: usize = 5;
        for level in 1..=6_u8 {
            let i = heading_scale_index(level);
            assert!(i < SCALE_LEN, "h{level} indexed {i}, outside the scale");
        }
        assert_eq!(heading_scale_index(1), 0);
        assert_eq!(heading_scale_index(5), 4);
        // h6 shares h5's scale rather than panicking — the clamp's whole purpose.
        assert_eq!(heading_scale_index(6), 4);
    }

    /// A level of 0 cannot arise from the parser, but `saturating_sub` means it cannot
    /// underflow into a huge index if it ever did.
    #[test]
    fn a_zero_heading_level_cannot_underflow_into_a_wild_index() {
        assert_eq!(heading_scale_index(0), 0);
    }

    fn text(s: &str) -> Inline {
        Inline::Text {
            text: s.to_string(),
            span: (0, s.chars().count() as i32),
        }
    }

    fn image(alt: &str) -> Inline {
        Inline::Image(ImageRef {
            alt: alt.to_string(),
            title: None,
            source: crate::export::ImageSource::Remote("https://example.invalid/b.png".into()),
        })
    }

    /// A paragraph with no image is one run, not a run plus two empty ones.
    #[test]
    fn prose_with_no_image_is_a_single_run() {
        let segs = split_on_images(&[text("just prose"), text(" and more")]);
        assert_eq!(segs.len(), 1);
        assert!(matches!(&segs[0], Seg::Text(v) if v.len() == 2));
    }

    /// The README badge shape: a link WRAPPING an image. This is the case the doc
    /// comment singles out, and the reason the walk recurses into containers.
    #[test]
    fn an_image_inside_a_link_is_found_rather_than_noted() {
        let badge = Inline::Link {
            href: "https://example.invalid".to_string(),
            title: None,
            inner: vec![image("badge")],
        };
        assert!(contains_image(&badge), "a wrapped image must be reachable");

        let segs = split_on_images(&[badge]);
        assert!(
            segs.iter().any(|s| matches!(s, Seg::Image(_))),
            "the badge yielded no image segment: it would render as an italic note"
        );
    }

    /// An image between prose splits the paragraph either side of it, in order.
    #[test]
    fn an_image_splits_the_prose_around_it_in_order() {
        let segs = split_on_images(&[text("before "), image("mid"), text(" after")]);
        let shapes: Vec<&str> = segs
            .iter()
            .map(|s| match s {
                Seg::Text(_) => "text",
                Seg::Image(_) => "image",
            })
            .collect();
        assert_eq!(shapes, ["text", "image", "text"]);
    }

    /// The column count comes from the header, which carries the delimiter row's count.
    #[test]
    fn a_tables_column_count_comes_from_its_header() {
        let head = vec![vec![text("a")], vec![text("b")], vec![text("c")]];
        let rows = vec![vec![vec![text("1")], vec![text("2")], vec![text("3")]]];
        assert_eq!(table_column_count(&head, &rows), 3);
    }

    /// A header-less table falls back to the widest body row, so no cell is dropped.
    #[test]
    fn a_headerless_table_falls_back_to_its_widest_row() {
        let rows = vec![
            vec![vec![text("1")]],
            vec![vec![text("1")], vec![text("2")], vec![text("3")]],
            vec![vec![text("1")], vec![text("2")]],
        ];
        assert_eq!(table_column_count(&[], &rows), 3);
    }

    /// A table with nothing in it has no columns, and says so rather than panicking on
    /// an empty `max()`.
    #[test]
    fn an_empty_table_has_no_columns() {
        assert_eq!(table_column_count(&[], &[]), 0);
    }
}
