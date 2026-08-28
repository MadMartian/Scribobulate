//! Ink: turning measured fragments into marks on a cairo surface.
//!
//! The counterpart to [`super::measure`]-style work — this module owns the *drawing*
//! half of the sink and nothing else. It decides nothing: what page a line lands on came
//! from [`super::super::paginate`], how wide a column is came from
//! [`super::geometry`], and what a construct is came from [`super::decide`]. What is
//! left is cairo.
//!
//! **That claim was FALSE for a while and this note is what makes it checkable.** Five
//! decisions had leaked in: theme-key-else-palette for the quote bar and the rule, and
//! sprite-vs-flat precedence for the bar, the band and the rule — each of them a
//! definite answer no measurement can change, and each unreachable from a test without a
//! cairo surface and a page (F-INKSEAM-001). They are `decide::wash_of` and
//! `decide::band_wash` now, both generic over the sprite payload so the PRECEDENCE can
//! be exercised with a stand-in and no image at all. What is left here is
//! [`paint_wash`]: given a settled `Wash`, fill a rectangle.
//!
//! # Every glyph goes through `show_layout_line`
//!
//! **Never** a per-run `show_glyph_string` loop. That hands cairo positioned glyphs with
//! no UTF-8 and no cluster information, which silently destroys the PDF's text layer:
//! the page still looks correct and nothing in it can be searched, selected or copied
//! (TDD 25.18). It is the kind of regression that passes every visual check.

use super::super::pdftable;
use super::geometry::px_to_pt;
use super::geometry::{pango_to_pt, MIN_PRINTABLE_PT};
use super::{Laid, LineKind, PageDrawn, TableCell};
use crate::palette::Palette;
use crate::theme::Theme;
use gtk::cairo;

/// Set cairo's source to an RGBA's colour.
///
/// The three-line `set_source_rgb(f64::from(c.red()), …)` incantation was written out ten
/// times in this file, four of them restoring a colour nothing subsequently drew with.
/// One name, so a reader can see WHICH colour is being set rather than decode that it is
/// being set at all.
///
/// **`set_source_rgba`, four channels.** It was three, which discarded the alpha of
/// every colour a theme stated — while the gradient arm four lines away passed
/// `f64::from(c.alpha())` to `add_color_stop_rgba` and kept it. Two arms of one `match`,
/// disagreeing about whether alpha exists. Every colour key in this vocabulary parses
/// `#RRGGBBAA`, two shipped defaults are translucent, and `blockquote_bg` — "a panel
/// behind quoted text" — is the key an author would most naturally make a wash: it
/// rendered translucent on screen and as a solid block on the page.
fn set_ink(cr: &cairo::Context, colour: gtk::gdk::RGBA) {
    cr.set_source_rgba(
        f64::from(colour.red()),
        f64::from(colour.green()),
        f64::from(colour.blue()),
        f64::from(colour.alpha()),
    );
}

/// Fill `rect` with a settled [`Wash`] — the cairo half, and nothing but.
///
/// A tile repeats at its natural size from the rect's own origin (`translate` first, so
/// the pattern's phase is the decoration's rather than the page's). A flat colour and a
/// gradient fill the same rect. `None` paints nothing, which is how an unstated band
/// leaves a heading byte-identical.
fn paint_wash(
    cr: &cairo::Context,
    wash: &super::decide::Wash<cairo::ImageSurface>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) {
    match wash {
        super::decide::Wash::Tile(surface) => {
            let pattern = cairo::SurfacePattern::create(surface);
            pattern.set_extend(cairo::Extend::Repeat);
            cr.save().ok();
            cr.translate(x, y);
            if cr.set_source(&pattern).is_ok() {
                cr.rectangle(0.0, 0.0, width, height);
                cr.fill().ok();
            }
            cr.restore().ok();
        }
        super::decide::Wash::Gradient { from, to } => {
            let g = cairo::LinearGradient::new(x, y, x, y + height);
            for (offset, c) in [(0.0, from), (1.0, to)] {
                g.add_color_stop_rgba(
                    offset,
                    f64::from(c.red()),
                    f64::from(c.green()),
                    f64::from(c.blue()),
                    f64::from(c.alpha()),
                );
            }
            cr.save().ok();
            // The rectangle goes INSIDE the guard, matching the tile arm above.
            // Appended before it, a failing `set_source` leaves it on the path —
            // and cairo's save/restore does not save the path, so the next
            // `show_layout_line` would fill it in the TEXT colour: a solid block
            // over the heading rather than a missing gradient.
            if cr.set_source(&g).is_ok() {
                cr.rectangle(x, y, width, height);
                cr.fill().ok();
            }
            cr.restore().ok();
        }
        super::decide::Wash::Flat(fill) => {
            set_ink(cr, *fill);
            cr.rectangle(x, y, width, height);
            cr.fill().ok();
        }
        super::decide::Wash::None => {}
    }
}

/// Draw one page's fragments onto `cr`, in points.
pub(crate) fn draw_page(
    cr: &cairo::Context,
    laid: &Laid,
    range: std::ops::Range<usize>,
    palette: &Palette,
    theme: &Theme,
    margin_pt: f64,
) -> PageDrawn {
    let fg = palette.body_fg;
    // Resolved ONCE for the page, not per line. Neither depends on the line, so both
    // used to be recomputed inside the loop below — the quote bar's on every quoted
    // line, the rule's on every horizontal rule. Hoisting also puts the whole page's
    // "theme key, else palette" resolution in one readable place, which is where the
    // POLICY § One theme key rule wants it: a reader checking that a surface is themed
    // consistently should not have to find every draw site to be sure.
    // Decoded ONCE for the page rather than per quoted line: the surface is cheap to
    // hold and re-reading the file for every line of a long quote is not.
    // Sprite-vs-flat precedence and theme-key-else-palette are BOTH `decide`'s, so what
    // is left below is cairo — which is what this module's own doc has always claimed
    // (F-INKSEAM-001). `surface` is the injection point: `wash_of` takes the loader, so
    // the decision is exercisable with a stand-in payload and no image at all.
    let bar_wash =
        super::decide::wash_of(&theme.blockquote_bar_decor(), palette.blockquote_bar, |r| {
            crate::sprite::surface(r).map(|(surface, _, _)| surface)
        });
    // The quote panel and its ink (TDD 18.29), resolved with the same hoist and the same
    // "theme key, else absent" rule: each is `None` unless the theme states it, and an
    // unstated one leaves quoted text on the page in the body ink, exactly as before.
    let quote_bg = theme.blockquote_bg;
    let quote_fg = theme.blockquote_fg;
    // The rule's tile (TDD 18.31), decoded ONCE for the page beside the quote bar's, for
    // the same reason: a document of rules would otherwise re-read the same picture per
    // rule. `measure` has already given each rule line room for a whole tile.
    let rule_wash = super::decide::wash_of(&theme.rule_decor(), palette.rule, |r| {
        crate::sprite::surface(r).map(|(surface, _, _)| surface)
    });
    set_ink(cr, fg);
    let mut y = margin_pt;
    for (i, idx) in range.clone().enumerate() {
        // Both `.get()`, though `push_line` now makes them the same length by
        // construction: this used to guard `lines` and then INDEX `fragments` three lines
        // later, so a guard returning None was followed by a panic on the same index.
        let (Some(line), Some(frag)) = (laid.lines.get(idx), laid.fragments.get(idx)) else {
            continue;
        };
        if i > 0 {
            y += frag.space_before;
        }
        // The quote's own decoration — its panel (TDD 18.29) and its accent bar — both
        // drawn from the QUOTE's column rather than this line's, and both extended up
        // over the block gap above when the line above belongs to the same quote.
        //
        // Those two corrections are what make a quote holding an intro paragraph, a
        // nested list and a closing paragraph draw as ONE object. Per line at the line's
        // own indent, the list's rows stepped `INDENT_PT` to the right of the paragraphs
        // around them and every `space_before` between blocks showed the paper through —
        // the same defect the preview carried, arriving here by a different route (there,
        // GTK's per-paragraph `paragraph_background_rgba`; here, arithmetic that only
        // ever saw one line at a time).
        //
        // `gap_above` is the space THIS iteration just added to `y`, given back. It is
        // zero for the first line on a page, where none was added — so a quote split
        // across a page break starts flush at the top margin rather than reaching above
        // it, which is the right answer for a page and needs no case of its own.
        if let Some(quote) = line.quote {
            // Compared by IDENTITY, never by indent: two quotes one blank line apart
            // share every metric and must still draw as two panels.
            let previous_in_same_quote = i > 0
                && idx
                    .checked_sub(1)
                    .and_then(|prev| laid.lines.get(prev))
                    .and_then(|prev| prev.quote)
                    .is_some_and(|prev| prev.id == quote.id);
            let gap_above = if previous_in_same_quote {
                frag.space_before
            } else {
                0.0
            };
            let top = y - gap_above;
            let height = line.height + gap_above;
            cr.save().ok();
            // The bar sits its own width plus the themed gap left of the quoted text
            // — the geometry `measure` stepped the quote in by, read back. It used to
            // be `w * 2.0`, which made the bar-to-text gap silently equal to the bar's
            // own width and left `blockquote_text_gap` expressing nothing on the page.
            let w = px_to_pt(theme.metrics.blockquote_bar_width);
            let gap = px_to_pt(theme.metrics.blockquote_text_gap);
            let x = margin_pt + quote.indent - w - gap;
            // The panel goes down FIRST, behind both the bar and the text. It spans the
            // quote's own column — from the quote's indent to the printable edge — which
            // is this medium's reading of the content column the preview fills.
            if let Some(bg) = quote_bg {
                set_ink(cr, bg);
                let width = (laid.printable_width_pt - quote.indent).max(MIN_PRINTABLE_PT);
                cr.rectangle(margin_pt + quote.indent, top, width, height);
                cr.fill().ok();
            }
            // A theme may tile a sprite down the bar instead of filling it (TDD 18.28),
            // at natural size, the same picture the preview tiles. An `else` rather than
            // a paint-over for the reason the drawn bar states: an opaque tile hides the
            // difference and a transparent one lets the flat colour bleed through.
            paint_wash(cr, &bar_wash, x, top, w, height);
            cr.restore().ok();
            set_ink(cr, fg);
        }
        // The heading band (TDD 18.25), FIRST so the heading's own glyphs land on top of
        // it. Spans the printable column — the same extent the preview draws it at, and
        // the widest thing this medium offers without restructuring the page.
        if let Some(band) = &line.fill {
            // Back OUT by the padding the line was laid out inside: the text moved in,
            // the band did not (TDD 18.25's padding fix), so the band keeps the exact
            // printable column the preview and the HTML sink match against.
            let left = (line.indent - band.padding).max(0.0);
            let width = (laid.printable_width_pt - left).max(MIN_PRINTABLE_PT);
            let x = margin_pt + left;
            cr.save().ok();
            // The band's three-way precedence is `decide`'s (`BlockFill::wash`), settled
            // at measure time; what happens here is the fill.
            paint_wash(cr, &band.wash, x, y, width, line.height);
            cr.restore().ok();
            set_ink(cr, fg);
        }
        // A themed list-marker SPRITE, in the gutter LEFT of this line's own indent —
        // out of the text run, exactly as the preview's drawn gutter puts it, which is
        // also why the text carries no marker prefix when one applies (TDD 18.24/25.3).
        if let Some(mk) = &line.marker {
            let (nat_w, nat_h) = mk.natural;
            cr.save().ok();
            // Half the marker's own side as the gap to the text: derived from the thing
            // being drawn rather than stated, so it tracks the row height at any page
            // size and adds no literal to a file POLICY forbids them in.
            let gap = mk.size / 2.0;
            cr.translate(margin_pt + line.indent - mk.size - gap, y);
            if nat_w > 0.0 && nat_h > 0.0 {
                cr.scale(mk.size / nat_w, mk.size / nat_h);
            }
            if cr.set_source_surface(&mk.surface, 0.0, 0.0).is_ok() {
                cr.paint().ok();
            }
            cr.restore().ok();
            set_ink(cr, fg);
        }
        match &line.kind {
            LineKind::Rule => {
                cr.save().ok();
                let width = (laid.printable_width_pt - line.indent).max(MIN_PRINTABLE_PT);
                // A sprite OUTRANKS the flat colour, stated as a branch for the reason
                // every other sprite-vs-flat pair in this vocabulary states it: an opaque
                // tile hides the difference, and a transparent one lets the colour bleed
                // through — a bug only the tiles nobody tested would show.
                if matches!(rule_wash, super::decide::Wash::Tile(_)) {
                    paint_wash(
                        cr,
                        &rule_wash,
                        margin_pt + line.indent,
                        y,
                        width,
                        line.height,
                    );
                    cr.restore().ok();
                    set_ink(cr, fg);
                    y += line.height + frag.space_after;
                    continue;
                }
                // The flat rung of the same `Wash` — a hairline rather than a filled
                // band, which is why it is not `paint_wash`: the rule's flat form is a
                // LINE and its tiled form fills the reserved height.
                if let super::decide::Wash::Flat(ink) = rule_wash {
                    set_ink(cr, ink);
                }
                // Span the printable column this rule sits in, at the theme's own
                // thickness. It used to be `400.0, 0.75` — two literals in a file whose
                // POLICY forbids them, which over- or under-ran the margin depending on
                // page setup and nesting depth rather than tracking either.
                let thickness = super::geometry::px_to_pt(theme.metrics.rule_thickness);
                cr.rectangle(
                    margin_pt + line.indent,
                    y + line.height / 2.0,
                    width,
                    thickness,
                );
                cr.fill().ok();
                cr.restore().ok();
                set_ink(cr, fg);
            }
            LineKind::Image {
                surface,
                natural,
                drawn,
            } => {
                // Scaled from device pixels to the points it was laid out at. Inside a
                // save/restore so the transform cannot leak into the next line's text.
                let (nat_w, nat_h) = *natural;
                let (w, h) = *drawn;
                cr.save().ok();
                cr.translate(margin_pt + line.indent, y);
                if nat_w > 0.0 && nat_h > 0.0 {
                    cr.scale(w / nat_w, h / nat_h);
                }
                if cr.set_source_surface(surface, 0.0, 0.0).is_ok() {
                    cr.paint().ok();
                }
                cr.restore().ok();
                set_ink(cr, fg);
            }
            LineKind::Text { layout, index } => {
                // Quoted body text takes the panel's ink where the theme states one
                // (TDD 18.29). Set on the CONTEXT, not into the markup, so a `<span
                // foreground=…>` the markup already carries — a link, a heading colour, a
                // `==mark==` — still wins: the same ladder the preview gets from
                // `TagName::BlockquoteInk` being the lowest-priority ink tag.
                let quoted_ink = line.quote.is_some().then_some(quote_fg).flatten();
                if let Some(c) = quoted_ink {
                    set_ink(cr, c);
                }
                if let Some(pl) = layout.line_readonly(*index) {
                    let (_ink, logical) = pl.extents();
                    cr.move_to(margin_pt + line.indent, y - pango_to_pt(logical.y()));
                    // `show_layout_line`, never a per-run glyph loop — the text layer is
                    // the difference between a searchable PDF and a picture of one.
                    pangocairo::functions::show_layout_line(cr, &pl);
                }
                // Put the body pen back, the same duty every branch above discharges:
                // this is the one branch that used never to change it, so a quote's ink
                // would otherwise have leaked into the prose after it.
                if quoted_ink.is_some() {
                    set_ink(cr, fg);
                }
            }
            LineKind::TableRow {
                cells,
                columns,
                chrome,
                scale,
                box_height,
                is_head,
            } => {
                draw_table_row(
                    cr,
                    TableRowInk {
                        cells,
                        columns,
                        chrome,
                        scale: *scale,
                        box_height: *box_height,
                        is_head: *is_head,
                        head_fg: theme.table_head_fg,
                    },
                    margin_pt + line.indent,
                    y,
                    palette,
                    theme,
                );
                // The row drew its own colours; put the body pen back for whatever
                // follows, or the next line of prose inherits a border colour.
                set_ink(cr, fg);
            }
        }
        // The gap BELOW this block, where the theme asked for one. Unlike
        // `space_before` it is not dropped at a page boundary: it belongs to the block
        // above it rather than to the join, so a heading whose page ends right after it
        // keeps the rhythm it asked for. The paginator budgets the same quantity, so a
        // page's contents and its measurement agree.
        y += line.height + frag.space_after;
    }
    // ONE status check, at the one place that owns the page's outcome.
    //
    // Every cairo call in this function ends `.ok()`, and that is not laziness: cairo is a
    // latching state machine, so the FIRST error puts the context into a permanent error
    // state and every later call becomes a no-op returning the same error. Checking each
    // call would report the same fault a dozen times and still not tell you which one was
    // first. Checking once, here, asks the question that matters — did this page reach the
    // surface intact — and a failure is logged rather than swallowed, because the promote
    // gate upstream decides what to do about a short page and cannot see a cairo status.
    if let Err(e) = cr.status() {
        log::error!("PDF page draw ended in a cairo error state: {e}; the page may be incomplete");
    }
    PageDrawn(())
}

/// Everything the ink pass needs about one table row, gathered so the drawing
/// function takes a subject rather than eight positional arguments.
struct TableRowInk<'a> {
    cells: &'a [TableCell],
    columns: &'a [pdftable::Column],
    chrome: &'a pdftable::Chrome,
    scale: f64,
    box_height: f64,
    is_head: bool,
    /// The theme's resolved header ink, or `None` where neither `table_head_fg` nor
    /// `heading_color` is stated.
    head_fg: Option<gtk::gdk::RGBA>,
}

/// Draw one table row with its cell borders, header fill and text.
///
/// `left`/`top` are the row's top-left corner on the page, in points. Everything after
/// the transform is in **unscaled table coordinates**, so the geometry drawn here is
/// exactly the geometry [`pdftable::fit`] decided — the scale is applied once, to the
/// whole row, and nothing downstream has to know about it (TDD 25.17).
fn draw_table_row(
    cr: &cairo::Context,
    row: TableRowInk<'_>,
    left: f64,
    top: f64,
    palette: &Palette,
    theme: &Theme,
) {
    let border_rgba = theme.table_border_color.unwrap_or(palette.table_border);
    let head_bg = theme.table_head_bg.unwrap_or(palette.table_head_bg);
    let fg = palette.body_fg;
    // The header row's ink (TDD 18.30), already folded with `heading_color` by
    // `Theme::resolve` — one resolved value, the same one the preview's `.cell-head` rule
    // and the HTML sink's `th` rule read. Unstated by both keys it stays `fg`, which is
    // what this sink drew for every row before the key existed; a theme that colours its
    // headings now gets that colour here too, which closes a gap on the way past (this
    // sink coloured no header ink at all, of any kind — the same shape as the marker gap
    // TDD 18.26 closed).
    let head_fg = if row.is_head {
        row.head_fg.unwrap_or(fg)
    } else {
        fg
    };

    cr.save().ok();
    cr.translate(left, top);
    // Says what it means: skip an IDENTITY transform. Written as a tolerance
    // (`(scale - 1.0).abs() > f64::EPSILON`) it read as an approximate comparison
    // while being an exact one — `f64::EPSILON` is the ULP at 1.0, so it admits
    // nothing a plain `!=` does not, and a scale arrived at by a different
    // derivation would take the wrong branch for reasons the code did not state.
    if row.scale > 0.0 && row.scale != 1.0 {
        cr.scale(row.scale, row.scale);
    }

    // The header's fill goes down first, so the borders and text sit on top of it.
    if row.is_head {
        set_ink(cr, head_bg);
        for column in row.columns {
            cr.rectangle(column.x, 0.0, column.box_width, row.box_height);
        }
        cr.fill().ok();
    }

    // One stroked box per cell. Adjacent cells share an edge, so a reader sees a
    // continuous rule rather than a double line — the `border-collapse` the HTML sink
    // asks for, expressed in the only way a page has.
    if row.chrome.border > 0.0 {
        set_ink(cr, border_rgba);
        cr.set_line_width(row.chrome.border);
        let inset = row.chrome.border / 2.0;
        for column in row.columns {
            cr.rectangle(
                column.x + inset,
                inset,
                (column.box_width - row.chrome.border).max(0.0),
                (row.box_height - row.chrome.border).max(0.0),
            );
        }
        cr.stroke().ok();
    }

    set_ink(cr, head_fg);
    for cell in row.cells {
        let Some(column) = row.columns.get(cell.column) else {
            continue;
        };
        cr.move_to(
            column.x + row.chrome.border + row.chrome.padding_h,
            row.chrome.padding_v,
        );
        // `show_layout` walks the layout's own lines and hands cairo UTF-8 with
        // clusters, exactly as `show_layout_line` does for body text — a wrapped cell
        // must stay searchable and selectable like everything else (TDD 25.18).
        pangocairo::functions::show_layout(cr, &cell.layout);
    }
    cr.restore().ok();
}
