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
                | Inline::Claim { inner: v, .. } => collect_segs(v, out),
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
        | Inline::Claim { inner: v, .. } => v.iter().any(contains_image),
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
pub(crate) fn list_marker_markup(
    task: Option<bool>,
    start: Option<u64>,
    n: usize,
    depth: u32,
    glyphs: &crate::theme::ListGlyphs,
    ink: Option<gtk::gdk::RGBA>,
) -> String {
    // A theme may stand its own glyph in for any of the four (TDD 18.24), which is the
    // same substitution — and the same per-state independence for the task marker — the
    // drawn gutter makes.
    //
    // The return is **Pango MARKUP**, which is why the name says so: the glyph goes
    // through `MarkerGlyph`'s Pango projection HERE rather than being escaped by the
    // caller, because that projection is the one place that knows this grammar. The
    // default markers below carry no metacharacter, so they are literal by inspection.
    // Escaping it twice would render `&amp;` on the page; escaping it never would fail
    // `pango_parse_markup` and render the whole run EMPTY, with no warning (ScrAP-163).
    // The HTML sink takes the same key through its OWN projection — one key, one
    // validation, a different escape per grammar.
    //
    // The kind is normalised ONCE, at this sink's boundary, and the depth tier is
    // applied inside the engine — so the "task arms come first" rule and the
    // bullet-only depth variation are stated in one place rather than in four tables
    // inside this file alone.
    let kind = crate::theme::MarkerKind::from_task_and_start(task, start);
    // This projection asks only about GLYPHS. A sprite marker is a picture and cannot
    // ride inside a text run, so the sink resolves it separately
    // (`list_marker_sprite`) and suppresses this markup entirely when one applies.
    let themed = crate::theme::marker_glyph(kind, depth as usize, glyphs);
    let glyph = match themed {
        Some(g) => g.escaped_for_pango_markup(),
        None => match kind {
            crate::theme::MarkerKind::TaskChecked => "\u{2611}".to_string(),
            crate::theme::MarkerKind::Task => "\u{2610}".to_string(),
            // `saturating_add`, never `+`: `start` is a `u64` straight out of the
            // parser and `n` is the item's index, so `u64::MAX` plus a second item
            // overflows — a debug-build panic inside an export walk.
            crate::theme::MarkerKind::Ordered => {
                format!("{}.", start.unwrap_or(1).saturating_add(n as u64))
            }
            crate::theme::MarkerKind::Bullet => "\u{2022}".to_string(),
        },
    };
    // The marker's own INK, which this sink carried for nothing before: the marker was
    // prepended to the item's first line and inherited that line's body colour, so a
    // theme's `list_marker` reached the drawn gutter and the HTML sink and stopped at
    // the page (TDD 18.26's last clause). Unset ⇒ no span at all, which is the body ink
    // and therefore byte-identical to what this sink emitted before (TDD 18.2).
    //
    // The span wraps the GLYPH only, never the trailing no-break space: colouring a
    // space is invisible, and keeping it outside means the coloured run on the page is
    // exactly the marker a reader sees.
    match ink {
        None => format!("{glyph}\u{00a0}"),
        Some(c) => format!(
            "<span foreground=\"{}\">{glyph}</span>\u{00a0}",
            crate::palette::to_hex_rgba(c)
        ),
    }
}

/// The list-marker SPRITE a theme states for this item's kind, if any — resolved by
/// exactly the precedence the drawn gutter uses, from the one function that owns it, so
/// the artefact and the preview cannot disagree about which key wins.
pub(crate) fn list_marker_sprite(
    task: Option<bool>,
    start: Option<u64>,
    depth: u32,
    sprites: &crate::theme::Sprites,
) -> Option<&crate::sprite::SpriteRef> {
    // Only the SPRITE half; the glyph half is `list_marker_markup`'s, and the two are
    // chosen by the same table.
    crate::theme::marker_sprite(
        crate::theme::MarkerKind::from_task_and_start(task, start),
        depth as usize,
        sprites,
    )
}

/// The ink one list marker is drawn in: the BULLET's depth-tiered colour where the theme
/// states one (TDD 18.26), the shared `list_marker` otherwise.
///
/// **A thin normalisation over the engine's own answer, not a twin of it.** This used
/// to be a four-line hand-rolled copy, argued for on the grounds that "this sink's
/// marker kind is a `(task, start)` pair where the gutter's is a `ListMarkerKind`, and
/// the module that owns the gutter's version draws with GTK". The first half was true
/// and is now handled by normalising the pair at this boundary
/// (`MarkerKind::from_task_and_start`); the second was an argument for moving the
/// dispatch OUT of the gutter, which is where it went (`crate::theme::decor`).
pub(crate) fn list_marker_ink(
    task: Option<bool>,
    start: Option<u64>,
    depth: u32,
    theme: &crate::theme::Theme,
) -> Option<gtk::gdk::RGBA> {
    theme.marker_ink(
        crate::theme::MarkerKind::from_task_and_start(task, start),
        depth as usize,
    )
}

/// Which entry of the theme's heading scale a heading of `level` reads.
///
/// **The clamp is the whole function**, and it is no longer this function's own: the
/// h6→h5 fold is `crate::theme::heading_slot`, so this sink cannot disagree with the
/// preview or the outline about it. It used to hardcode `.min(4)` while the HTML sink
/// derived the same bound from `HEADING_LEVELS` — one constant change away from moving
/// one surface and not the other. `heading_scale` has five entries and Markdown has
/// six heading levels, so an unclamped `level - 1` indexes out of bounds for an `h6`,
/// which is a panic on a document a reader can trivially write.
pub(crate) fn heading_scale_index(level: u8) -> usize {
    crate::theme::heading_slot(level)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A theme that states no marker glyph — the shape every assertion below about the
    /// DEFAULT markers is written against.
    fn plain() -> crate::theme::ListGlyphs {
        crate::theme::ListGlyphs::default()
    }

    /// TDD 18.24 — a themed glyph stands in for each of the four markers, and the task
    /// states resolve independently (a theme may state one alone).
    #[test]
    fn a_themed_glyph_stands_in_for_each_default_marker() {
        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test(
            "[themes.marks]\nlist_bullet_glyph = \"b\"\nlist_ordered_glyph = \"o\"\n\
             list_task_checked_glyph = \"c\"\n",
        );
        let g = themes.resolve("marks").list_glyphs;
        assert_eq!(list_marker_markup(None, None, 0, 1, &g, None), "b\u{00a0}");
        assert_eq!(
            list_marker_markup(None, Some(1), 4, 1, &g, None),
            "o\u{00a0}"
        );
        assert_eq!(
            list_marker_markup(Some(true), None, 0, 1, &g, None),
            "c\u{00a0}"
        );
        // Unstated: the drawn default's own glyph, unchanged.
        assert_eq!(
            list_marker_markup(Some(false), None, 0, 1, &g, None),
            "\u{2610}\u{00a0}"
        );
        // The NO-BREAK space is part of the contract for a themed marker too — a glyph
        // that wrapped away from its item text would be the same defect in new clothes.
        assert!(list_marker_markup(None, None, 0, 1, &g, None).ends_with('\u{00a0}'));
    }

    /// TDD 18.26's last clause — the PDF sink coloured NO marker, of any kind, at any
    /// depth: the marker was prepended to the item's first line and inherited that
    /// line's body ink, so a theme's `list_marker` reached the drawn gutter and the HTML
    /// sink and stopped at the page.
    ///
    /// Unset ⇒ no span at all, so the markup is byte-identical to what this sink emitted
    /// before the key could reach it (TDD 18.2) — asserted, because "no colour stated"
    /// and "the colour happens to be the body ink" produce the same PAGE and very
    /// different markup, and only one of them is this sink leaving the default alone.
    #[test]
    fn the_marker_carries_the_themes_ink_and_nothing_when_it_states_none() {
        let ink = gtk::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0);
        // Every kind, not just the bullet: the gap was total.
        for (task, start) in [
            (None, None),
            (None, Some(1)),
            (Some(true), None),
            (Some(false), None),
        ] {
            let bare = list_marker_markup(task, start, 0, 1, &plain(), None);
            assert!(!bare.contains("<span"), "unset must add nothing: {bare}");
            let inked = list_marker_markup(task, start, 0, 1, &plain(), Some(ink));
            assert!(
                inked.starts_with("<span foreground=\"#ff0000\">"),
                "task={task:?} start={start:?}: {inked}"
            );
            // The no-break space stays OUTSIDE the span — see `list_marker_markup`.
            assert!(inked.ends_with("</span>\u{00a0}"), "{inked}");
            // …and it is still there, so Pango cannot wrap the marker away from its
            // item text. Colouring the marker must not cost that.
            assert!(inked.ends_with('\u{00a0}'));
        }
    }

    /// A themed GLYPH is escaped once and once only, then coloured — the two operations
    /// compose rather than one undoing the other. `&` escaped twice renders `&amp;` on
    /// the page; escaped never fails `pango_parse_markup` and renders the whole run
    /// EMPTY, with no warning (ScrAP-163).
    #[test]
    fn a_themed_glyph_is_escaped_once_inside_its_colour_span() {
        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test("[themes.amp]\nlist_bullet_glyph = \"&\"\n");
        let g = themes.resolve("amp").list_glyphs;
        let out = list_marker_markup(None, None, 0, 1, &g, Some(gtk::gdk::RGBA::BLACK));
        assert!(out.contains(">&amp;<"), "{out}");
        assert!(!out.contains("&amp;amp;"), "double-escaped: {out}");
        gtk::pango::parse_markup(&out, '\0').expect("the marker markup must parse");
    }

    /// TDD 18.26 — the bullet's glyph, sprite and ink all follow the nesting depth this
    /// sink now threads, through the SAME tier map the drawn gutter uses; ordered and
    /// task markers are untouched by it at every depth.
    #[test]
    fn a_bullets_marker_follows_its_nesting_depth_and_no_other_kinds_does() {
        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test(
            "[themes.tiered]\nlist_marker_color = \"#111111\"\nlist_marker_color_2 = \"#222222\"\n\
             list_bullet_glyph = \"1\"\nlist_bullet_glyph_2 = \"2\"\n\
             list_ordered_glyph = \"o\"\n",
        );
        let t = themes.resolve("tiered");
        let bullet = |depth: u32| {
            list_marker_markup(
                None,
                None,
                0,
                depth,
                &t.list_glyphs,
                list_marker_ink(None, None, depth, &t),
            )
        };
        assert!(bullet(1).contains(">1<"), "{}", bullet(1));
        assert!(bullet(1).contains("#111111"), "{}", bullet(1));
        assert!(bullet(2).contains(">2<"), "{}", bullet(2));
        assert!(bullet(2).contains("#222222"), "{}", bullet(2));
        // Depth 3 inherited depth 2 in both properties.
        assert!(bullet(3).contains(">2<"), "{}", bullet(3));
        assert!(bullet(3).contains("#222222"), "{}", bullet(3));

        // A nested ORDERED item keeps its own glyph and the shared ink at any depth.
        let ordered = list_marker_markup(
            None,
            Some(1),
            0,
            3,
            &t.list_glyphs,
            list_marker_ink(None, Some(1), 3, &t),
        );
        assert!(ordered.contains(">o<"), "{ordered}");
        assert!(ordered.contains("#111111"), "{ordered}");
    }

    /// The bullet's SPRITE reads the tier too, and only the bullet's arm does.
    #[test]
    fn a_bullet_sprite_is_selected_by_depth() {
        let mut sprites = crate::theme::Sprites::default();
        let at_path = |n: &str| crate::sprite::SpriteRef::File(std::path::PathBuf::from(n));
        sprites.list_bullet[0] = Some(at_path("/x/1.png"));
        sprites.list_bullet[1] = Some(at_path("/x/2.png"));
        sprites.list_bullet[2] = Some(at_path("/x/3.png"));
        sprites.list_ordered = Some(at_path("/x/o.png"));
        let at = |d: u32| {
            list_marker_sprite(None, None, d, &sprites)
                .expect("a bullet sprite at every tier")
                .clone()
        };
        assert_eq!(at(1), at_path("/x/1.png"));
        assert_eq!(at(2), at_path("/x/2.png"));
        assert_eq!(at(9), at_path("/x/3.png"));
        // The ordered arm ignores depth entirely.
        for d in [1u32, 2, 9] {
            assert_eq!(
                list_marker_sprite(None, Some(1), d, &sprites),
                Some(&at_path("/x/o.png"))
            );
        }
    }

    /// TDD 18.27 — the PDF sink colours the task checkbox from its own key while the
    /// bullet and the numeral in the same document keep `list_marker`, and both task
    /// states share the one colour.
    #[test]
    fn the_task_marker_carries_its_own_ink_in_the_artefact() {
        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test(
            "[themes.split]\nlist_marker_color = \"#111111\"\nlist_task_marker_color = \"#ff00ff\"\n",
        );
        let t = themes.resolve("split");
        let markup = |task: Option<bool>, start: Option<u64>| {
            list_marker_markup(
                task,
                start,
                0,
                1,
                &t.list_glyphs,
                list_marker_ink(task, start, 1, &t),
            )
        };
        assert!(markup(Some(false), None).contains("#ff00ff"));
        assert!(markup(Some(true), None).contains("#ff00ff"));
        // A task item inside an ORDERED list is still a checkbox — the task arm comes
        // first, the same ordering `list_marker_markup` relies on for its glyph.
        assert!(markup(Some(true), Some(1)).contains("#ff00ff"));
        assert!(markup(None, None).contains("#111111"));
        assert!(markup(None, Some(1)).contains("#111111"));
    }

    /// The sprite precedence is resolved HERE for the PDF, from the same per-kind
    /// mapping the drawn gutter uses — so the artefact cannot answer a different key
    /// than the screen does.
    #[test]
    fn a_sprite_is_selected_per_marker_kind() {
        let mut sprites = crate::theme::Sprites::default();
        assert!(list_marker_sprite(None, None, 1, &sprites).is_none());
        let bullet = crate::sprite::SpriteRef::File(std::path::PathBuf::from("/x/b.png"));
        sprites.list_bullet[0] = Some(bullet.clone());
        assert_eq!(list_marker_sprite(None, None, 1, &sprites), Some(&bullet));
        // …and only that kind.
        assert!(list_marker_sprite(None, Some(1), 1, &sprites).is_none());
        assert!(list_marker_sprite(Some(true), None, 1, &sprites).is_none());
    }

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
                list_marker_markup(task, start, n, 1, &plain(), None),
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
            let marker = list_marker_markup(task, start, 0, 1, &plain(), None);
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
    ///
    /// **The SECOND item is the test.** This passed `n = 0`, so `start + n` never
    /// exercised the addition the name is about — the arithmetic that overflows is
    /// `u64::MAX + 1`, which is a debug-build panic *inside an export*, and
    /// `start.rs:333-337` calls a panic on a render walk the worst outcome available.
    #[test]
    fn an_absurd_ordered_start_does_not_panic() {
        let first = list_marker_markup(None, Some(u64::MAX), 0, 1, &plain(), None);
        assert!(first.starts_with(&u64::MAX.to_string()));
        // The item after it: `u64::MAX + 1`, saturating rather than wrapping or
        // panicking. Saturating is right for a numeral — the list is already absurd,
        // and the reader gets a number rather than a dead window.
        let second = list_marker_markup(None, Some(u64::MAX), 1, 1, &plain(), None);
        assert!(
            second.starts_with(&u64::MAX.to_string()),
            "the second item of a list starting at u64::MAX must saturate: {second}"
        );
        // …and a far-from-the-edge list still counts normally, so the saturation is
        // not simply pinning every numeral to one value.
        let ordinary = list_marker_markup(None, Some(7), 2, 1, &plain(), None);
        assert!(ordinary.starts_with('9'), "7 + 2 must be 9: {ordinary}");
    }

    /// Every Markdown heading level indexes inside the theme's five-entry scale.
    #[test]
    fn every_heading_level_indexes_inside_the_scale() {
        // Derived from the theme's own constant, never a local literal: a hardcoded
        // `5` here could not have caught the disagreement this seam was extracted to
        // end, because it would have agreed with whichever site also hardcoded one.
        for level in 1..=6_u8 {
            let i = heading_scale_index(level);
            assert!(
                i < crate::theme::HEADING_LEVELS,
                "h{level} indexed {i}, outside the scale"
            );
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

// ── what the page's surface decorations become ───────────────────────────────
//
// The sink's own module doc says `ink` "decides nothing … what is left is cairo". It
// decided five things: theme-key-else-palette for the quote bar and the rule, and
// sprite-vs-flat precedence for the bar, the band and the rule (F-INKSEAM-001). Each of
// those is a decision with a definite answer no measurement can change, which is exactly
// this module's remit — and `ink`'s versions were unreachable from a test without a
// cairo surface and a page.

/// How one surface is painted, once its precedence is settled.
///
/// Generic over the sprite payload so the DECISION can be exercised with a stand-in —
/// `Wash<&str>` in a unit test, `Wash<cairo::ImageSurface>` in the sink. That is the
/// injection point the ink half never had: reaching the sprite-vs-flat branch used to
/// require decoding a real image.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Wash<S> {
    /// A sprite, tiled at natural size. **Outranks the flat colour, and the flat colour
    /// is not painted underneath it** — a transparent tile would let it bleed through,
    /// which is a bug only the tiles nobody tested would show.
    Tile(S),
    /// The theme's flat colour, or the palette's where the theme states none.
    Flat(gtk::gdk::RGBA),
    /// Two stops. Only a heading band can be one; a bar or a rule has no gradient key.
    Gradient {
        from: gtk::gdk::RGBA,
        to: gtk::gdk::RGBA,
    },
    /// Nothing to paint at all — a band the theme did not state.
    None,
}

/// Settle a two-way decoration: its sprite where the theme names one AND `load` can
/// produce it, else the theme's flat colour, else the caller's own default.
///
/// The `load` failure is the load-bearing case and it is why this takes a loader rather
/// than an already-loaded `Option`: a sprite that passes every admission check and then
/// fails to produce a surface must fall through to the flat colour, not erase the
/// decoration — the same degrade-don't-erase rule every marker follows.
pub(crate) fn wash_of<S>(
    decor: &crate::theme::Fill<'_>,
    fallback: gtk::gdk::RGBA,
    load: impl Fn(&crate::sprite::SpriteRef) -> Option<S>,
) -> Wash<S> {
    match decor.sprite.and_then(load) {
        Some(tile) => Wash::Tile(tile),
        None => Wash::Flat(decor.flat_or(fallback)),
    }
}

/// Settle a heading band's THREE-way decoration: sprite, else gradient, else flat, else
/// nothing at all.
///
/// Unlike [`wash_of`] there is no fallback colour — a band the theme did not state paints
/// nothing, which is what keeps an unbanded heading byte-identical to before the
/// decoration existed (TDD 18.2).
pub(crate) fn band_wash<S>(
    decor: &crate::theme::Band<'_>,
    load: impl Fn(&crate::sprite::SpriteRef) -> Option<S>,
) -> Wash<S> {
    if let Some(tile) = decor.sprite.and_then(load) {
        return Wash::Tile(tile);
    }
    match decor.without_sprite() {
        Some(crate::theme::BandPaint::Gradient { from, to }) => Wash::Gradient { from, to },
        Some(crate::theme::BandPaint::Flat(fill)) => Wash::Flat(fill),
        None => Wash::None,
    }
}

#[cfg(test)]
mod wash_tests {
    use super::*;
    use gtk::gdk::RGBA;

    const RED: RGBA = RGBA::new(1.0, 0.0, 0.0, 1.0);
    const BLUE: RGBA = RGBA::new(0.0, 0.0, 1.0, 1.0);
    const GREEN: RGBA = RGBA::new(0.0, 1.0, 0.0, 1.0);

    fn named() -> crate::sprite::SpriteRef {
        crate::sprite::SpriteRef::Compiled("sprites/copper-plate.png")
    }

    /// The whole precedence ladder for a two-way decoration, including the rung that
    /// matters most: a sprite the loader cannot produce falls through to the COLOUR
    /// rather than erasing the decoration.
    #[test]
    fn a_sprite_outranks_the_colour_and_a_failed_load_falls_through_to_it() {
        let sprite = named();
        let stated = crate::theme::Fill {
            sprite: Some(&sprite),
            flat: Some(RED),
        };
        assert_eq!(
            wash_of(&stated, BLUE, |_| Some("tile")),
            Wash::Tile("tile"),
            "a sprite that loads must outrank the theme's own colour"
        );
        assert_eq!(
            wash_of(&stated, BLUE, |_| None::<&str>),
            Wash::Flat(RED),
            "a sprite that will not load must degrade to the colour, never erase it"
        );

        // No sprite: the theme's colour, else the caller's palette-derived default.
        let coloured = crate::theme::Fill {
            sprite: None,
            flat: Some(RED),
        };
        assert_eq!(wash_of(&coloured, BLUE, |_| Some("tile")), Wash::Flat(RED));
        let bare = crate::theme::Fill {
            sprite: None,
            flat: None,
        };
        assert_eq!(
            wash_of(&bare, BLUE, |_| Some("tile")),
            Wash::Flat(BLUE),
            "a theme stating neither key leaves the caller's own default standing — \
             which is what keeps this surface byte-identical (TDD 18.2)"
        );
    }

    /// The band's extra rung, and the one asymmetry with [`wash_of`]: a band the theme
    /// did not state paints NOTHING rather than falling back to a colour.
    #[test]
    fn a_bands_gradient_sits_between_its_sprite_and_its_flat_fill() {
        let sprite = named();
        let full = crate::theme::Band {
            sprite: Some(&sprite),
            gradient: Some((RED, GREEN)),
            flat: Some(BLUE),
        };
        assert_eq!(band_wash(&full, |_| Some("tile")), Wash::Tile("tile"));
        assert_eq!(
            band_wash(&full, |_| None::<&str>),
            Wash::Gradient {
                from: RED,
                to: GREEN
            },
            "a sprite that will not load must degrade to the gradient, not past it"
        );

        let gradientless = crate::theme::Band {
            sprite: None,
            gradient: None,
            flat: Some(BLUE),
        };
        assert_eq!(band_wash(&gradientless, |_| Some("t")), Wash::Flat(BLUE));

        let unstated = crate::theme::Band {
            sprite: None,
            gradient: None,
            flat: None,
        };
        assert_eq!(
            band_wash(&unstated, |_| Some("t")),
            Wash::None::<&str>,
            "an unstated band paints nothing — there is no palette fallback for it"
        );
    }
}
