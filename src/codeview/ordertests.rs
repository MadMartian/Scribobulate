//! **Compositing-order guards for the preview's drawn decorations.**
//!
//! One test per pair of decorations that can genuinely overlap, each pinning which
//! of the two lands on top. Together they are the net under any change to
//! `snapshot_layer` — the property most at risk when that function is restructured,
//! and the one the rest of the pixel suite asserts least: every existing paint test
//! drives ONE decoration on a document that contains nothing else, so a build that
//! composited the whole vocabulary in the wrong order passes all of them.
//!
//! **The pair list is derived, not guessed.** [`super::DRAWN_VECTORS`] enumerates the
//! five vectors `snapshot_layer` draws from; a pair can overlap only if (a) the two
//! constructs can nest in Markdown, so their buffer spans coincide, and (b) their
//! drawn rectangles intersect in x. Both halves were MEASURED rather than reasoned:
//! the nestings against the real renderer (`preview::build`'s products for
//! `> # heading`, `> ```code```, `> - item`, `- # heading`, and a list item whose
//! first line is a fence — every one of them produced coinciding spans), and the
//! x-ranges from the arithmetic the painters share:
//!
//! | region | x | drawn by |
//! |---|---|---|
//! | content column | `[lm, lm + card_w]` | quote panel, heading band, code card |
//! | accent gutter | `[lm, lm + bar_w]` | blockquote bar |
//! | marker gutter | `[base, base + depth·step]`, `base ≥ lm` | list markers |
//! | right margin | `[width − rm + 2, …]` | annotation chip |
//! | card top-right | inside the card | copy button |
//!
//! **The disclosure summary band is in the content-column row of that table**, so it
//! overlaps exactly what a heading band does and takes the same answers: it lands ON
//! the quote panel a disclosure can sit inside (TDD 2.26c), and UNDER the accent bar
//! and the gutter markers that cross its row. Pair 10 below drives the containing
//! case in pixels; the other two are the same two constraints the heading band's
//! pairs 5 and 7 already prove about a rectangle at the same column in the same layer,
//! and `decorplan::tests` states all three as intent. Against the heading band and the
//! code card it is not an overlap at all: a summary line is text this renderer writes
//! for a raw-HTML block, and no heading or fence can be laid on it, so their spans are
//! always disjoint.
//!
//! **Three pairs are therefore NOT overlaps, and they are stated rather than
//! omitted** — an absent row and a ruled-out one look identical:
//!
//! * *annotation chip × anything in the content column* — `chip_rect` starts at
//!   `width − rm + 2`, two pixels PAST the content column's right edge, so the chip
//!   never covers and is never covered. It is why the chip can be in the ABOVE-TEXT
//!   layer for an unrelated reason (an anchored table would hide it) without that
//!   choice implying anything about ordering.
//! * *blockquote bar × list markers* — a quoted item's marker column starts at
//!   `lm + px(bar_width + text_gap)`, which is exactly the bar's right edge. The
//!   markers are placed clear of the bar deliberately (POLICY Document Rendering CAM
//!   row 2), so there is nothing to order.
//! * *heading band × code card* — a heading is a leaf block and a fenced block is
//!   not a container, so no document nests one in the other and their spans are
//!   always disjoint.
//!
//! **The oracle, and why it is shaped this way.** Each test paints a fixture in which
//! the upper decoration's rectangle lies wholly inside the lower one's, then asserts
//! the upper decoration's colour is in the framebuffer *and* the lower one's is too.
//! Swap the two draws and the upper vanishes completely — there is nowhere left for it
//! to show. The second assertion is not decoration: without it a fixture that silently
//! stopped producing the lower decoration would pass forever, having nothing to cover
//! anything (ScrAP-209's shape). Where the rectangles merely intersect — a bar runs a
//! whole quote while a band covers only its heading's rows — presence survives the swap
//! on the rows outside the intersection, so those two use the row-scoped form instead.
//!
//! Every assertion here was mutation-checked by transposing the two draws in
//! `snapshot_layer` and confirming the test went red; the pairs and their results are
//! recorded in the commit that introduced this module.

use super::painttest::{contains_rgb, framebuffer_of, present_for_paint_sized, rows_with};
use super::CodePreviewView;
use crate::renderer::{HeadingSpan, ListMarker, ListMarkerKind};
use crate::span::{BufferSpan, QuoteSpan};
use gtk::gdk;
use gtk::prelude::*;

const W: usize = 400;
const H: usize = 300;
/// Wide enough that the view's own margins leave a real content column, narrow
/// enough that everything stays on one screen without scrolling.
const VIEW_MARGIN: i32 = 12;

/// Colours no other part of a render can produce, so finding one is evidence about
/// the decoration that was asked for it and not about the page.
const PANEL: (u8, u8, u8) = (0x0a, 0x18, 0x30);
const BAND: (u8, u8, u8) = (0x33, 0x66, 0x99);
const CARD: (u8, u8, u8) = (0xcc, 0x33, 0x00);
const BAR: (u8, u8, u8) = (0x00, 0xff, 0x00);
const MARKER: (u8, u8, u8) = (0xff, 0x00, 0xff);
const SUMMARY_BAND: (u8, u8, u8) = (0x33, 0x99, 0x66);

/// A theme stating `extra` over a plain white page, activated for this test only.
fn themed(extra: &str) -> crate::theme::ActiveThemeGuard {
    let mut themes = crate::theme::themes();
    themes.merge_over_for_test(&format!(
        "[themes.ordering]\nbackground = \"#ffffff\"\nforeground = \"#000000\"\n{extra}"
    ));
    crate::theme::activate_for_test(themes.resolve("ordering"))
}

/// A view holding `text`, with margins set and the text indented clear of the
/// decorations drawn in the gutter — so a glyph landing on a bar or a marker cannot
/// shrink a measured extent and read as a missing decoration.
fn view_with(text: &str) -> CodePreviewView {
    let view = CodePreviewView::new();
    view.set_left_margin(VIEW_MARGIN);
    view.set_right_margin(VIEW_MARGIN);
    let buffer = view.buffer();
    buffer.set_text(text);
    if let Some(indent) = buffer.create_tag(None, &[("left-margin", &96)]) {
        buffer.apply_tag(&indent, &buffer.start_iter(), &buffer.end_iter());
    }
    view
}

/// Char count of `text`, the unit every `BufferSpan` in this module is measured in.
fn chars(text: &str) -> i32 {
    text.chars().count() as i32
}

/// Paint `view` and hand back its framebuffer, tearing the window down afterwards.
fn painted(view: &CodePreviewView) -> Vec<u8> {
    let window = present_for_paint_sized(view, W as i32, H as i32);
    let data = framebuffer_of(view, W as f64, H as f64);
    window.destroy();
    data
}

/// The full ordering assertion for a pair whose upper rectangle is inside the lower's.
///
/// Both halves are load-bearing and they fail for different reasons: the first is the
/// order itself, the second is the fixture keeping its end of the bargain.
#[track_caller]
fn assert_on_top(
    data: &[u8],
    upper: (u8, u8, u8),
    upper_name: &str,
    lower: (u8, u8, u8),
    lower_name: &str,
) {
    assert!(
        contains_rgb(data, lower),
        "the fixture never painted the {lower_name} at all, so this run says nothing \
         about whether the {upper_name} lands on top of it"
    );
    assert!(
        contains_rgb(data, upper),
        "the {upper_name} is nowhere in the frame — its rectangle is inside the \
         {lower_name}'s, so the only way for it to vanish is for the {lower_name} to \
         be painted after it. Check the order the two are drawn in."
    );
}

/// The row-scoped form, for a pair whose rectangles intersect without containment.
#[track_caller]
fn assert_on_top_per_row(
    data: &[u8],
    upper: (u8, u8, u8),
    upper_name: &str,
    lower: (u8, u8, u8),
    lower_name: &str,
) {
    let upper_rows = rows_with(data, upper, W);
    let lower_rows = rows_with(data, lower, W);
    assert!(
        lower_rows.iter().any(|r| *r),
        "the fixture never painted the {lower_name} at all"
    );
    assert!(
        upper_rows.iter().any(|r| *r),
        "the fixture never painted the {upper_name} at all"
    );
    for (y, (has_lower, has_upper)) in lower_rows.iter().zip(&upper_rows).enumerate() {
        if *has_lower {
            assert!(
                *has_upper,
                "row {y}: the {lower_name} is painted here but the {upper_name} is \
                 not — the {upper_name} runs through this row and must land ON the \
                 {lower_name}, so its absence means the {lower_name} was drawn last"
            );
        }
    }
}

/// **Pair 1 — a heading band inside a blockquote lands ON the quote panel.**
///
/// The quote is the outermost container of the two, so its fill is the ground every
/// per-heading decoration inside it is drawn against.
#[gtktest::test]
fn a_heading_band_inside_a_quote_lands_on_the_quote_panel() {
    let _theme = themed(
        "blockquote_bg = \"#0a1830\"\nblockquote_bar_color = \"#00ff00\"\n\
         blockquote_bar_width = 8\nheading_band_color_h1 = \"#336699\"\n",
    );
    let heading = "Quoted heading";
    let text = format!("{heading}\n\nquote body\n");
    let view = view_with(&text);
    view.set_blockquotes(
        vec![QuoteSpan {
            span: BufferSpan::new(0, chars(&text)),
            depth: 1,
        }],
        gdk::RGBA::new(0.0, 1.0, 0.0, 1.0),
    );
    view.set_heading_spans(vec![HeadingSpan {
        span: BufferSpan::new(0, chars(heading)),
        level_index: 0,
    }]);

    let data = painted(&view);
    assert_on_top(&data, BAND, "heading band", PANEL, "quote panel");
}

/// **Pair 10 — a disclosure's summary band inside a blockquote lands ON the quote
/// panel.**
///
/// The new decoration's own instance of pair 1: a `<details>` nests in a quote
/// (TDD 2.26c) and its band fills the same content column a heading band does, so the
/// panel is its ground too. Driven rather than reasoned from the heading band's result
/// because the step is a separate entry in `PAINT_ORDER` and could be moved on its own.
#[gtktest::test]
fn a_disclosure_summary_band_inside_a_quote_lands_on_the_quote_panel() {
    let _theme = themed(
        "blockquote_bg = \"#0a1830\"\nblockquote_bar_color = \"#00ff00\"\n\
         blockquote_bar_width = 8\ndisclosure_band_color = \"#339966\"\n",
    );
    let summary = "Quoted summary";
    let text = format!("{summary}\n\nquoted body\n");
    let view = view_with(&text);
    view.set_blockquotes(
        vec![QuoteSpan {
            span: BufferSpan::new(0, chars(&text)),
            depth: 1,
        }],
        gdk::RGBA::new(0.0, 1.0, 0.0, 1.0),
    );
    view.set_disclosure_bands(vec![BufferSpan::new(0, chars(summary))]);

    let data = painted(&view);
    assert_on_top(
        &data,
        SUMMARY_BAND,
        "disclosure summary band",
        PANEL,
        "quote panel",
    );
}

/// **Pair 2 — a code block inside a blockquote lands ON the quote panel.**
#[gtktest::test]
fn a_code_card_inside_a_quote_lands_on_the_quote_panel() {
    let _theme = themed(
        "blockquote_bg = \"#0a1830\"\nblockquote_bar_color = \"#00ff00\"\n\
         blockquote_bar_width = 8\n",
    );
    let intro = "Quote intro\n\n";
    let text = format!("{intro}code line\n");
    let view = view_with(&text);
    view.set_blockquotes(
        vec![QuoteSpan {
            span: BufferSpan::new(0, chars(&text)),
            depth: 1,
        }],
        gdk::RGBA::new(0.0, 1.0, 0.0, 1.0),
    );
    view.set_code_blocks(
        vec![BufferSpan::new(chars(intro), chars(&text) - 1)],
        gdk::RGBA::new(0.8, 0.2, 0.0, 1.0),
    );

    let data = painted(&view);
    assert_on_top(&data, CARD, "code-block card", PANEL, "quote panel");
}

/// **Pair 3 — the accent bar lands ON the quote panel it shares an extent with.**
///
/// The one pair drawn from a single vector, and the one separated by the widest span
/// of code: the panel opens the below-text pass and the bar is drawn after the heading
/// band and the code card. Nothing about the two being one decoration's two halves
/// keeps them adjacent in the source, which is exactly why the order needs a guard.
#[gtktest::test]
fn the_accent_bar_lands_on_the_quote_panel() {
    let _theme = themed(
        "blockquote_bg = \"#0a1830\"\nblockquote_bar_color = \"#00ff00\"\n\
         blockquote_bar_width = 8\n",
    );
    let text = "Quoted paragraph\n\nand another\n";
    let view = view_with(text);
    view.set_blockquotes(
        vec![QuoteSpan {
            span: BufferSpan::new(0, chars(text)),
            depth: 1,
        }],
        gdk::RGBA::new(0.0, 1.0, 0.0, 1.0),
    );

    let data = painted(&view);
    assert_on_top(&data, BAR, "accent bar", PANEL, "quote panel");
}

/// **Pair 4 — a quoted list's gutter markers land ON the quote panel.**
#[gtktest::test]
fn quoted_list_markers_land_on_the_quote_panel() {
    let _theme = themed(
        "blockquote_bg = \"#0a1830\"\nblockquote_bar_color = \"#00ff00\"\n\
         blockquote_bar_width = 8\nlist_marker_color = \"#ff00ff\"\n",
    );
    let first = "item one\n";
    let text = format!("{first}item two\n");
    let view = view_with(&text);
    view.set_blockquotes(
        vec![QuoteSpan {
            span: BufferSpan::new(0, chars(&text)),
            depth: 1,
        }],
        gdk::RGBA::new(0.0, 1.0, 0.0, 1.0),
    );
    view.set_list_markers(
        vec![
            ListMarker {
                depth: 1,
                kind: ListMarkerKind::Bullet,
                first_line: 0,
                quoted: true,
            },
            ListMarker {
                depth: 1,
                kind: ListMarkerKind::Bullet,
                first_line: chars(first),
                quoted: true,
            },
        ],
        1.0,
    );

    let data = painted(&view);
    assert_on_top(&data, MARKER, "list gutter marker", PANEL, "quote panel");
}

/// **Pair 5 — the accent bar lands ON a heading band inside the quote.**
///
/// Row-scoped: the bar runs the whole quote while the band covers only the heading's
/// rows, so covering the bar there leaves it visible everywhere else and a presence
/// check cannot see the swap.
#[gtktest::test]
fn the_accent_bar_lands_on_a_quoted_heading_band() {
    let _theme = themed(
        "blockquote_bar_color = \"#00ff00\"\nblockquote_bar_width = 8\n\
         heading_band_color_h1 = \"#336699\"\n",
    );
    let heading = "Quoted heading";
    let text = format!("{heading}\n\nquote body\n");
    let view = view_with(&text);
    view.set_blockquotes(
        vec![QuoteSpan {
            span: BufferSpan::new(0, chars(&text)),
            depth: 1,
        }],
        gdk::RGBA::new(0.0, 1.0, 0.0, 1.0),
    );
    view.set_heading_spans(vec![HeadingSpan {
        span: BufferSpan::new(0, chars(heading)),
        level_index: 0,
    }]);

    let data = painted(&view);
    assert_on_top_per_row(&data, BAR, "accent bar", BAND, "heading band");
}

/// **Pair 6 — the accent bar lands ON a code card inside the quote.**
#[gtktest::test]
fn the_accent_bar_lands_on_a_quoted_code_card() {
    let _theme = themed("blockquote_bar_color = \"#00ff00\"\nblockquote_bar_width = 8\n");
    let intro = "Quote intro\n\n";
    let text = format!("{intro}code line\n");
    let view = view_with(&text);
    view.set_blockquotes(
        vec![QuoteSpan {
            span: BufferSpan::new(0, chars(&text)),
            depth: 1,
        }],
        gdk::RGBA::new(0.0, 1.0, 0.0, 1.0),
    );
    view.set_code_blocks(
        vec![BufferSpan::new(chars(intro), chars(&text) - 1)],
        gdk::RGBA::new(0.8, 0.2, 0.0, 1.0),
    );

    let data = painted(&view);
    assert_on_top_per_row(&data, BAR, "accent bar", CARD, "code-block card");
}

/// **Pair 7 — a list item's gutter marker lands ON a heading band on the same row.**
///
/// `- # Heading` is one line carrying both, MEASURED against the renderer: the item's
/// `first_line` and the heading's span both start at offset 0.
#[gtktest::test]
fn a_list_marker_lands_on_a_heading_band_sharing_its_row() {
    let _theme = themed("heading_band_color_h1 = \"#336699\"\nlist_marker_color = \"#ff00ff\"\n");
    let heading = "Heading in item";
    let text = format!("{heading}\nplain second item\n");
    let view = view_with(&text);
    view.set_heading_spans(vec![HeadingSpan {
        span: BufferSpan::new(0, chars(heading)),
        level_index: 0,
    }]);
    view.set_list_markers(
        vec![ListMarker {
            depth: 1,
            kind: ListMarkerKind::Bullet,
            first_line: 0,
            quoted: false,
        }],
        1.0,
    );

    let data = painted(&view);
    assert_on_top(&data, MARKER, "list gutter marker", BAND, "heading band");
}

/// **Pair 8 — a list item's gutter marker lands ON a code card sharing its row.**
///
/// An item whose first line is the opening fence, MEASURED against the renderer: the
/// block's span and the item's `first_line` both start at offset 0.
#[gtktest::test]
fn a_list_marker_lands_on_a_code_card_sharing_its_row() {
    let _theme = themed("list_marker_color = \"#ff00ff\"\n");
    let fenced = "code line one\ncode line two\n";
    let text = format!("{fenced}plain second item\n");
    let view = view_with(&text);
    view.set_code_blocks(
        vec![BufferSpan::new(0, chars(fenced) - 1)],
        gdk::RGBA::new(0.8, 0.2, 0.0, 1.0),
    );
    view.set_list_markers(
        vec![ListMarker {
            depth: 1,
            kind: ListMarkerKind::Bullet,
            first_line: 0,
            quoted: false,
        }],
        1.0,
    );

    let data = painted(&view);
    assert_on_top(&data, MARKER, "list gutter marker", CARD, "code-block card");
}

/// **Pair 9 — a code block's copy button lands ON its own card.**
///
/// The only pair whose order is held by the LAYER split rather than by statement
/// order: the card paints in `BelowText` and the button in `AboveText`, so moving the
/// button's draw into the below-text pass is the mutation this guards, and no
/// rearrangement WITHIN a pass can reach it.
///
/// The oracle cannot be a colour, because the button's fill IS the card's own (it
/// masks the code text it covers) and its ink is the widget's CSS foreground, which
/// every glyph on the page also carries. So it counts pixels in the card's top-right
/// corner that are NOT the card's fill: the button's outline and glyph put dozens
/// there, and a card painted over the button leaves a flat fill with none.
#[gtktest::test]
fn a_copy_button_lands_on_its_own_code_card() {
    let _theme = themed("");
    let text = "short\ncode\nlines\n";
    let view = view_with(text);
    view.set_code_blocks(
        vec![BufferSpan::new(0, chars(text) - 1)],
        gdk::RGBA::new(0.8, 0.2, 0.0, 1.0),
    );
    // Reveal the button: it is drawn only for the block under the pointer or the one
    // whose copy has just been confirmed.
    view.set_hovered_code_block(Some(0), None);

    let data = painted(&view);

    // The card's own rows, and the right end of the content column the button sits in.
    let card_rows = rows_with(&data, CARD, W);
    assert!(
        card_rows.iter().any(|r| *r),
        "the fixture never painted the code card at all"
    );
    let x0 = W - VIEW_MARGIN as usize - 48;
    let x1 = W - VIEW_MARGIN as usize;
    let off_card = data
        .chunks_exact(W * 4)
        .enumerate()
        .filter(|(y, _)| card_rows[*y])
        .flat_map(|(_, row)| row.chunks_exact(4).skip(x0).take(x1 - x0))
        .filter(|px| (px[2], px[1], px[0]) != CARD)
        .count();
    assert!(
        off_card > 20,
        "the card's top-right corner is a flat fill ({off_card} pixels differ from it) \
         — the copy button is drawn there and is being painted over, which happens \
         when it moves out of the ABOVE-TEXT pass into the same one as the card"
    );
}
