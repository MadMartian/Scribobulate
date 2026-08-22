//! Ink: turning measured fragments into marks on a cairo surface.
//!
//! The counterpart to [`super::measure`]-style work — this module owns the *drawing*
//! half of the sink and nothing else. It decides nothing: what page a line lands on came
//! from [`super::super::paginate`], how wide a column is came from
//! [`super::geometry`], and what a construct is came from [`super::decide`]. What is
//! left is cairo.
//!
//! # Every glyph goes through `show_layout_line`
//!
//! **Never** a per-run `show_glyph_string` loop. That hands cairo positioned glyphs with
//! no UTF-8 and no cluster information, which silently destroys the PDF's text layer:
//! the page still looks correct and nothing in it can be searched, selected or copied
//! (TDD 25.18). It is the kind of regression that passes every visual check.

use super::super::pdftable;
use super::geometry::{pango_to_pt, MIN_PRINTABLE_PT};
use super::{Laid, LineKind, PageDrawn, TableCell, RULE_THICKNESS_PT};
use crate::palette::Palette;
use crate::theme::Theme;
use gtk::cairo;

/// Set cairo's source to an RGBA's colour.
///
/// The three-line `set_source_rgb(f64::from(c.red()), …)` incantation was written out ten
/// times in this file, four of them restoring a colour nothing subsequently drew with.
/// One name, so a reader can see WHICH colour is being set rather than decode that it is
/// being set at all.
fn set_ink(cr: &cairo::Context, colour: gtk::gdk::RGBA) {
    cr.set_source_rgb(
        f64::from(colour.red()),
        f64::from(colour.green()),
        f64::from(colour.blue()),
    );
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
    let bar_ink = theme.blockquote_bar.unwrap_or(palette.blockquote_bar);
    let rule_ink = theme.rule.unwrap_or(palette.rule);
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
        // The quote bar, at the metric the theme states.
        if line.quote_depth > 0 {
            cr.save().ok();
            set_ink(cr, bar_ink);
            let w = f64::from(theme.metrics.blockquote_bar_width);
            cr.rectangle(margin_pt + line.indent - w * 2.0, y, w, line.height);
            cr.fill().ok();
            cr.restore().ok();
            set_ink(cr, fg);
        }
        match &line.kind {
            LineKind::Rule => {
                cr.save().ok();
                set_ink(cr, rule_ink);
                // Span the printable column this rule sits in, at the theme's own
                // thickness. It used to be `400.0, 0.75` — two literals in a file whose
                // POLICY forbids them, which over- or under-ran the margin depending on
                // page setup and nesting depth rather than tracking either.
                let width = (laid.printable_width_pt - line.indent).max(MIN_PRINTABLE_PT);
                let thickness = RULE_THICKNESS_PT;
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
                if let Some(pl) = layout.line_readonly(*index) {
                    let (_ink, logical) = pl.extents();
                    cr.move_to(margin_pt + line.indent, y - pango_to_pt(logical.y()));
                    // `show_layout_line`, never a per-run glyph loop — the text layer is
                    // the difference between a searchable PDF and a picture of one.
                    pangocairo::functions::show_layout_line(cr, &pl);
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
        y += line.height;
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
    let border_rgba = theme.table_border.unwrap_or(palette.table_border);
    let head_bg = theme.table_head_bg.unwrap_or(palette.table_head_bg);
    let fg = palette.body_fg;

    cr.save().ok();
    cr.translate(left, top);
    if row.scale > 0.0 && (row.scale - 1.0).abs() > f64::EPSILON {
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

    set_ink(cr, fg);
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
