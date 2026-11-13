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
use super::geometry::{pango_to_pt, MIN_PRINTABLE_PT, PT_PER_PX};
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

/// Where a tiled decoration's pattern grid starts, given the rect it fills and the tile's
/// size **in points**.
///
/// Pure, and split out from the painting so the rule is testable without a surface: the
/// two axes take different anchors and the asymmetry is easy to "tidy" into a symmetry
/// that reintroduces a defect. `paint_wash` explains why each axis is what it is;
/// `widgets::tile_texture` is the screen's copy of the same rule and the two must agree,
/// because holding the page beside the screen is how a divergence is found.
fn tile_origin(x: f64, y: f64, tile_w: f64, tile_h: f64) -> (f64, f64) {
    // Horizontal: the rect's own left edge, so a decoration N tiles wide prints N WHOLE
    // tiles. `tile_w` is unused by design and named for the caller's sake.
    let _ = tile_w;
    // Vertical: the document-origin grid line at or above the rect.
    let oy = if tile_h > 0.0 {
        (y / tile_h).floor() * tile_h
    } else {
        y
    };
    (x, oy)
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
            // ⚠️ **The two axes take DIFFERENT anchors, and that asymmetry is the rule
            // rather than an oversight.** `widgets::tile_texture` is the authority and it
            // has always done this: vertically it takes the document-origin grid line at
            // or above the rect (congruent to 0 modulo the tile), horizontally it takes
            // `rect.x()` — the decoration's own left edge.
            //
            // The VERTICAL anchor is the page's because this medium draws a decoration
            // LINE BY LINE. A quote panel is one rect per quoted line, abutting, so a
            // per-rect phase cuts the pattern at every line boundary — invisible for a
            // tile whose rows all look alike, and obvious for any diagonal or
            // large-featured one. The accent bar and the heading band abut the same way.
            //
            // The HORIZONTAL anchor is the rect's because a decoration is a fixed number
            // of tiles WIDE, and a page-anchored lattice phase-shifts it: an accent bar
            // exactly one tile wide, sitting where the lattice boundary falls inside it,
            // prints the right half of one plate beside the left half of the next —
            // MANUAL-TEST 18.28's "column of half-rivets sliced down the left", which it
            // names as the independent horizontal half that a fix for the vertical one
            // reintroduces. That is exactly what this site did after it was generalised
            // to both axes.
            //
            // ⚠️ **It bites only where the rect's x is NOT a whole multiple of the tile
            // width, and that is why it survived a measurement.** MEASURED here by
            // rasterising this function with a half-black tile: a 12pt tile under a bar at
            // x=72 prints identically either way, because 72 is already on the 12pt
            // lattice — so the shipped geometry looked correct and proved nothing. Move
            // the same bar to x=78 and the page-anchored spelling opens with the plate's
            // WHITE half and closes with the next plate's black; the rect-anchored one
            // opens flush with a whole plate. Any bar width, indent or nesting depth that
            // lands off the lattice reaches it.
            //
            // Rounded to whole tiles vertically, so the anchor never introduces a
            // fractional offset the pattern would have to resample across.
            //
            // ⚠️ A tile's dimensions are PIXELS and this page is measured in POINTS, so
            // both the lattice below and the pattern itself are converted. Laying the
            // surface down unconverted prints it at 4/3 the size the preview draws it —
            // coherent per decoration, so a tile checked on its own looks deliberate,
            // and the error is only visible by holding the page beside the screen. The
            // pattern matrix maps user space to pattern space, hence the reciprocal:
            // one pixel is to occupy `PT_PER_PX` points.
            let scale = 1.0 / PT_PER_PX;
            pattern.set_matrix(cairo::Matrix::new(scale, 0.0, 0.0, scale, 0.0, 0.0));
            let (tw, th) = (
                f64::from(surface.width()) * PT_PER_PX,
                f64::from(surface.height()) * PT_PER_PX,
            );
            let (ox, oy) = tile_origin(x, y, tw, th);
            cr.translate(ox, oy);
            if cr.set_source(&pattern).is_ok() {
                cr.rectangle(x - ox, y - oy, width, height);
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

/// Draw a curated scene once inside a rect and clipped to it — the cairo half of
/// `widgets::draw_scene_into` and `widgets::draw_scene_corner`, and the same two
/// renderings the HTML sink declares as `right center / auto 100%` and a bare corner
/// position.
///
/// `anchor` decides both the position and the SIZE RULE, exactly as it does on screen
/// (`theme::SceneAnchor`):
///
/// * `None` — fitted by HEIGHT and hung on the RIGHT edge, for the reason the key
///   states: a header row's height is fixed by its text while its width tracks the
///   column, so the right edge is the only stable place to hang a picture and the left
///   is where the labels are.
/// * a corner — drawn at its NATURAL size (one image px to one point, this sink's
///   scale) and pinned there, so a cluster keeps the spread it was drawn with instead
///   of being rescaled by the row's height.
///
/// A scene that cannot be decoded draws NOTHING and leaves the fill beneath it intact —
/// degrade, never erase, the rule every decoration in this vocabulary follows.
fn paint_scene(
    cr: &cairo::Context,
    scene: &crate::sprite::SpriteRef,
    anchor: Option<crate::theme::SceneAnchor>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) {
    let Some((surface, nat_w, nat_h)) = crate::sprite::surface(scene) else {
        return;
    };
    if nat_w <= 0.0 || nat_h <= 0.0 || width <= 0.0 || height <= 0.0 {
        return;
    }
    // The fit takes the height; a corner keeps the source's own size.
    //
    // ⚠️ The two arms are in different unit regimes and only one of them needs a
    // conversion, which is why this reads as inconsistent and is not. `nat_w`/`nat_h`
    // are PIXELS. The fit divides a point-space height by a pixel height, so the units
    // cancel and the ratio is already correct. The corner keeps the source's own size,
    // and "its own size" on paper is its pixel size expressed in points — unconverted
    // it prints 4/3 oversize, and a bar tile sized to `blockquote_bar_width` is clipped
    // a quarter of the way down its right edge on every page, silently.
    let scale = match anchor {
        Some(_) => PT_PER_PX,
        None => height / nat_h,
    };
    let (drawn_w, drawn_h) = (nat_w * scale, nat_h * scale);
    // Placement is `SceneAnchor::offset`'s, the same function the GTK painter calls, so
    // the page and the screen cannot put one theme's scene in different corners.
    // Unanchored keeps its right edge and its own top, which the fit has already made
    // the full height.
    let (free_w, free_h) = (width - drawn_w, height - drawn_h);
    let (dx, dy) = match anchor {
        Some(corner) => corner.offset(free_w, free_h),
        None => (free_w, 0.0),
    };
    cr.save().ok();
    cr.rectangle(x, y, width, height);
    cr.clip();
    // A scene wider or taller than the cell overflows the edge its anchor is NOT on
    // and the clip above takes it — negative free space is the same multiplication.
    cr.translate(x + dx, y + dy);
    cr.scale(scale, scale);
    if cr.set_source_surface(&surface, 0.0, 0.0).is_ok() {
        cr.paint().ok();
    }
    cr.restore().ok();
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
    // The panel is the one decoration here that may be ABSENT, so it takes
    // `optional_wash` rather than `wash_of`: no fallback colour, and a theme stating
    // neither key paints nothing (TDD 18.2). A theme may tile it (TDD 18.59), decoded
    // once for the page like the bar's and the rule's.
    let quote_wash = super::decide::optional_wash(&theme.blockquote_panel_decor(), |r| {
        crate::sprite::surface(r).map(|(surface, _, _)| surface)
    });
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
            // Compared on ROOT, not on `id`: two adjacent lines at different depths of
            // one quote tree are the same quote to a reader, so comparing `id` here
            // opens a seam in the outer bar and the panel at every nesting boundary.
            // Two genuinely separate quotes one blank line apart still differ by root,
            // which is the case this comparison exists for.
            let previous_in_same_quote = i > 0
                && idx
                    .checked_sub(1)
                    .and_then(|prev| laid.lines.get(prev))
                    .and_then(|prev| prev.quote)
                    .is_some_and(|prev| prev.root == quote.root);
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
            let step = w + gap;
            // The OUTERMOST level's indent, reached by stepping back out of every
            // enclosing level. Both the panel and the leftmost bar hang off this.
            let root_indent = quote.indent - step * f64::from(quote.depth - 1);
            // The panel goes down FIRST, behind every bar and the text. It spans the
            // OUTERMOST quote's column — from that indent to the printable edge — which
            // is this medium's reading of the content column the preview fills.
            //
            // Once per line, from the root, NOT once per level: the background does not
            // nest (TDD 2.11b, operator 2026-08-28). Drawing it per level would step the
            // fill right at each depth and, for the translucent `blockquote_bg` two
            // shipped themes state, composite it with itself so the inner region read
            // darker for a reason no theme key asked for.
            {
                let width = (laid.printable_width_pt - root_indent).max(MIN_PRINTABLE_PT);
                paint_wash(cr, &quote_wash, margin_pt + root_indent, top, width, height);
            }
            // ONE BAR PER LEVEL, on every line inside it (TDD 2.11b). A line reports only
            // its innermost quote, so without this loop the enclosing levels' bars would
            // simply stop wherever a nested quote interrupted them — the outer quote
            // would read as two quotes with a hole in the middle. Every level steps by
            // the same `step`, so level `k` out from this one sits exactly `k * step`
            // left of it and no ancestry list has to be carried on the line.
            //
            // A theme may tile a sprite down the bar instead of filling it (TDD 18.28),
            // at natural size, the same picture the preview tiles. An `else` rather than
            // a paint-over for the reason the drawn bar states: an opaque tile hides the
            // difference and a transparent one lets the flat colour bleed through.
            for level in 0..quote.depth {
                let x = margin_pt + quote.indent - step * f64::from(level) - w - gap;
                paint_wash(cr, &bar_wash, x, top, w, height);
            }
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
                    let baseline = y - pango_to_pt(logical.y());
                    cr.move_to(margin_pt + line.indent, baseline);
                    // `show_layout_line`, never a per-run glyph loop — the text layer is
                    // the difference between a searchable PDF and a picture of one.
                    pangocairo::functions::show_layout_line(cr, &pl);
                    // The level's marker, immediately after this line's TEXT. The x comes
                    // from the line's own logical width, which this sink may read freely:
                    // it built the layout itself, so there is no `GtkTextLayout` and none
                    // of the display-cache hazard that rules the same question out on the
                    // preview's paint path.
                    //
                    // Drawn with its BOTTOM on the baseline, which is what aligns it with
                    // the capitals — `show_layout_line` draws from the baseline, so this
                    // is the same rule the preview gets from Pango placing a shape there,
                    // arrived at independently rather than by copying a number.
                    if let Some(mk) = &line.end_marker {
                        let (nat_w, nat_h) = mk.natural;
                        // A quarter of the marker's own height as the gap to the text,
                        // derived from the thing being drawn rather than stated — the
                        // same discipline the gutter marker's gap above follows, and it
                        // stands in for the single space the preview inserts.
                        let gap = mk.height / 4.0;
                        let w = mk.height * nat_w / nat_h;
                        cr.save().ok();
                        cr.translate(
                            margin_pt + line.indent + pango_to_pt(logical.width()) + gap,
                            baseline - mk.height,
                        );
                        cr.scale(w / nat_w, mk.height / nat_h);
                        if cr.set_source_surface(&mk.surface, 0.0, 0.0).is_ok() {
                            cr.paint().ok();
                        }
                        cr.restore().ok();
                        set_ink(cr, fg);
                    }
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
                opens,
                closes,
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
                        // A piece of a cut row closes its box at a page edge as well as
                        // at the row's own ends, so a continuation is never open-ended.
                        top_edge: *opens || i == 0,
                        bottom_edge: *closes || idx + 1 == range.end,
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
    /// Whether to rule the box's top / bottom edge. Only a piece of a row cut across
    /// pages ever omits one — where it joins a sibling piece on the same page.
    top_edge: bool,
    bottom_edge: bool,
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
    //
    // ONE BAND PER HEADER CELL (TDD 18.57), matching the preview's own extent and the
    // `<th>` the HTML sink styles: a scene appears once per column heading, and a tile
    // starts from each cell's own origin. The row-wide alternative is expressible here
    // and on screen but NOT in HTML, where the unit is the cell — so painting the row
    // would have made this sink disagree with the artefact by construction.
    if row.is_head {
        let decor = theme.table_head_decor();
        // `sprite::surface` memoises per reference, so this decodes once for the
        // document however many tables (or repeated header rows) reach it.
        let mut wash = super::decide::band_wash(&decor, |r| {
            crate::sprite::surface(r).map(|(surface, _, _)| surface)
        });
        // A table header always HAS a fill — derived off the page where the theme
        // states none — unlike a heading band, which is absent unless asked for. So the
        // one rung `band_wash` can return as `None` is filled in here rather than left
        // unpainted, which is what keeps a theme that states nothing byte-identical to
        // before this band existed (TDD 18.2).
        if matches!(wash, super::decide::Wash::None) {
            wash = super::decide::Wash::Flat(palette.table_head_bg);
        }
        for column in row.columns {
            paint_wash(cr, &wash, column.x, 0.0, column.box_width, row.box_height);
            // The SCENE composites over whichever of those painted, exactly as it does
            // on screen (`widgets::paint_band_into`) and in the HTML sink's `th` rule.
            //
            // This is the only scene that reaches paper, and the reason is geometry, not
            // policy: `heading_band_scene` and `blockquote_scene` are `not_on_paper`
            // because this sink draws those decorations LINE BY LINE, so a wrapped one
            // has no single right edge to anchor a picture to. A table's header cell is
            // drawn as one unit at a known `box_height`, so it does.
            if let Some(scene) = decor.scene {
                paint_scene(
                    cr,
                    scene,
                    decor.scene_anchor,
                    column.x,
                    0.0,
                    column.box_width,
                    row.box_height,
                );
            }
        }
    }

    // One stroked box per cell. Adjacent cells share an edge, so a reader sees a
    // continuous rule rather than a double line — the `border-collapse` the HTML sink
    // asks for, expressed in the only way a page has.
    //
    // An omitted edge is stroked OUTSIDE the box and clipped away, rather than the box
    // being drawn as separate segments: the sides then run flush into the sibling
    // piece's, with the same joins a whole box has, and a row that fits — both edges —
    // draws exactly the rectangle it always did.
    if row.chrome.border > 0.0 {
        cr.save().ok();
        set_ink(cr, border_rgba);
        cr.set_line_width(row.chrome.border);
        let inset = row.chrome.border / 2.0;
        let above = if row.top_edge {
            inset
        } else {
            -row.chrome.border
        };
        let below = if row.bottom_edge {
            row.box_height - inset
        } else {
            row.box_height + row.chrome.border
        };
        if !(row.top_edge && row.bottom_edge) {
            let width = row.columns.last().map_or(0.0, |c| c.x + c.box_width);
            cr.rectangle(0.0, 0.0, width, row.box_height);
            cr.clip();
        }
        for column in row.columns {
            cr.rectangle(
                column.x + inset,
                above,
                (column.box_width - row.chrome.border).max(0.0),
                (below - above).max(0.0),
            );
        }
        cr.stroke().ok();
        cr.restore().ok();
    }

    set_ink(cr, head_fg);
    for cell in row.cells {
        let Some(column) = row.columns.get(cell.column) else {
            continue;
        };
        let left = column.x + row.chrome.border + row.chrome.padding_h;
        // Line by line, and only the lines this row (or this piece of a row cut across
        // pages) owns: a whole-layout `show_layout` would draw a piece's neighbours too,
        // off the page's foot, and put their words into the text layer once per page.
        // `show_layout_line` hands cairo UTF-8 with clusters exactly as `show_layout`
        // does, so a wrapped cell stays searchable and selectable (TDD 25.18).
        for ink in &cell.lines {
            cr.move_to(left + ink.x, ink.baseline);
            pangocairo::functions::show_layout_line(cr, &ink.line);
        }
    }
    cr.restore().ok();
}

#[cfg(test)]
mod tile_origin_tests {
    /// The two axes anchor differently, and a symmetry here is a defect in one of them.
    ///
    /// ⚠️ **The x under test must NOT be a whole multiple of the tile width.** The
    /// geometry this was found on — a 12pt tile under a bar at x=72 — is on the lattice,
    /// so both spellings agree there and an assertion built from it passes with the fix
    /// removed. That is the trap this test exists downstream of: the shipped numbers
    /// looked correct and proved nothing, and the defect was only visible once the bar
    /// was moved off the lattice. MEASURED by rasterising `paint_wash` with a half-black
    /// tile: at x=78 the page-anchored spelling opens on the plate's white half, the
    /// rect-anchored one opens flush with a whole plate.
    ///
    /// Asserting the origin rather than the pixels because that is the decision; what a
    /// reader sees on paper belongs to MANUAL-TEST 25.9a and 18.28.
    #[test]
    fn the_horizontal_origin_is_the_rect_and_the_vertical_is_the_page_grid() {
        let (tile_w, tile_h) = (12.0, 12.0);

        // x=78 against a 12pt tile: OFF the lattice, which the boundary at 72 would
        // otherwise hide. This is the case that discriminates.
        let (ox, oy) = super::tile_origin(78.0, 597.508, tile_w, tile_h);
        assert_eq!(
            ox, 78.0,
            "the grid must start at the decoration's own left edge, not at the page \
             lattice line below it (72.0) — a bar anchored there opens on half a plate"
        );
        assert_eq!(
            oy, 588.0,
            "vertically the grid stays the PAGE's — 597.508 sits between the 12pt grid \
             lines at 588 and 600, so the anchor is the one at or above it, and abutting \
             per-line rects do not re-phase the pattern at every line boundary"
        );

        // A rect already on a vertical boundary keeps it, and one whose x is fractional
        // is not rounded — the horizontal axis never quantises, which is the whole point.
        let (ox, oy) = super::tile_origin(91.5, 600.0, 18.0, 18.0);
        assert_eq!(ox, 91.5);
        assert_eq!(oy, 594.0);
    }

    /// A degenerate tile must not divide by zero or move the rect.
    #[test]
    fn a_zero_sized_tile_leaves_the_origin_alone() {
        assert_eq!(super::tile_origin(10.0, 20.0, 0.0, 0.0), (10.0, 20.0));
    }
}
