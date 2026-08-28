//! The fenced code block's **copy affordance** for [`CodePreviewView`] — the small
//! button revealed in a block's top-right corner while the pointer is over it, the
//! way GitHub's rendered Markdown and most IDEs present one.
//!
//! This module is the **drawing** half only. Where the button goes — and whether a
//! pointer is on it — is display-free arithmetic and lives in
//! [`crate::affordance`], settled by unit test; the view supplies the live inputs
//! (the card rectangle, the code padding, one text row's height) and owns the paint
//! and the hit-box recording.
//!
//! **Why the button is self-drawn rather than an anchored `GtkButton`.** A widget at
//! a `GtkTextChildAnchor` re-measures at minimum width and re-arms the horizontal
//! churn that blanks the view (GTK4Rs/AP-23), and an overlay child's minimum feeds
//! the view's own minimum with no opt-out (GTK4Rs/AP-189) — the same reasoning that
//! made the code card, the blockquote bar, the list gutter and the annotation chip
//! drawn rather than built. The accepted cost is the one those share: a drawn glyph
//! has no accessible object (`sdd/PLAN.accessibility.md`), which is why the button is
//! a *shortcut* for something already reachable — selecting the block and copying —
//! and never the only route to it.
//!
//! **Sizing derives, it does not declare** ([`crate::affordance::copy_button_rect`]),
//! so the button tracks zoom and the reading font with no metric of its own to keep in
//! step (POLICY "No hard-coded styling"). The fractions below are *shape* — what is
//! drawn, which the theme model deliberately does not reach — and the two colours are
//! the page's own ink and the card's own fill, taken from the caller.

use crate::decorplan::{any_copy_button_shown, copy_button_shown};
use gtk::prelude::*;
use gtk::{cairo, gdk, graphene};

/// Colours for one paint of the button. `fill` masks whatever code text the button
/// overlaps (it is the card's own fill, so the button reads as part of the card);
/// `fg` is the resting ink; `hover` carries the accent when the pointer is on the
/// button, in the same visual language the task checkbox uses for the same message.
pub(crate) struct CopyButtonPaint<'a> {
    pub fill: &'a gdk::RGBA,
    pub fg: &'a gdk::RGBA,
    pub hover: Option<&'a gdk::RGBA>,
    /// Draw the "copied" checkmark instead of the copy glyph.
    pub copied: bool,
}

/// Draw the copy button at `rect` (buffer coordinates), at `zoom`.
///
/// The resting state is a filled box with a foreground outline and the two-sheet copy
/// glyph; hovering thickens the outline in the accent colour (`paint.hover`), and the
/// moment after a copy the glyph becomes the same checkmark the task checkbox draws,
/// so "it worked" needs no toast and no second surface.
///
/// Cairo rather than `Snapshot::append_fill`/`append_stroke`, which are 4.14+ — above
/// this project's floor, where an above-floor wrapper compiles and fails at
/// link/runtime (GTK4Rs/AP-114).
pub(crate) fn draw_copy_button(
    snapshot: &gtk::Snapshot,
    rect: &graphene::Rect,
    zoom: f64,
    paint: &CopyButtonPaint,
) {
    let z = zoom as f32;
    let (x, y, s) = (rect.x(), rect.y(), rect.width());
    let stroke = paint.hover.unwrap_or(paint.fg);
    let lw = if paint.hover.is_some() {
        (2.0 * z).max(1.5)
    } else {
        (1.3 * z).max(1.0)
    };
    // Widen the Cairo bounds by half the (larger) line width so a thicker hover
    // stroke is never clipped — the same margin `draw_list_marker` leaves.
    let hp = (lw / 2.0 + 1.0).max(2.0);
    let bounds = graphene::Rect::new(x - hp, y - hp, s + 2.0 * hp, s + 2.0 * hp);
    let cr = snapshot.append_cairo(&bounds);
    let radius = (3.0 * z).min(s / 4.0) as f64;

    // The box: filled with the card's own colour so the button masks any first-line
    // code text it overlaps, then outlined.
    super::gutter::set_source(&cr, paint.fill);
    super::gutter::rounded_rect(&cr, x as f64, y as f64, s as f64, s as f64, radius);
    let _ = cr.fill();
    super::gutter::set_source(&cr, stroke);
    cr.set_line_width(lw as f64);
    super::gutter::rounded_rect(&cr, x as f64, y as f64, s as f64, s as f64, radius);
    let _ = cr.stroke();

    // The ink stays the foreground whatever the border does, so the glyph never
    // trades legibility for the hover cue (the checkbox makes the same split).
    super::gutter::set_source(&cr, paint.fg);
    let glyph_lw = (1.4 * z).max(1.0) as f64;
    cr.set_line_width(glyph_lw);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    if paint.copied {
        // The identical checkmark the task checkbox draws, from the one path both
        // call — a second copy would drift the moment either is tuned.
        super::gutter::checkmark_path(&cr, x as f64, y as f64, s as f64);
        cr.set_line_width((1.9 * z).max(1.2) as f64);
        let _ = cr.stroke();
        return;
    }
    // Two offset sheets — the copy glyph every toolkit draws. The back sheet is
    // stroked first, then the front sheet is filled with the card colour so it
    // occludes the back one's overlapping edges before being outlined itself.
    let sf = s as f64;
    let (xf, yf) = (x as f64, y as f64);
    let sheet = sf * 0.42;
    let sheet_r = (radius * 0.6).min(sheet / 3.0);
    let back = (xf + sf * 0.36, yf + sf * 0.22);
    let front = (xf + sf * 0.22, yf + sf * 0.36);
    let (bx, by) = back;
    super::gutter::rounded_rect(&cr, bx, by, sheet, sheet, sheet_r);
    let _ = cr.stroke();
    let (fx, fy) = front;
    super::gutter::set_source(&cr, paint.fill);
    super::gutter::rounded_rect(&cr, fx, fy, sheet, sheet, sheet_r);
    let _ = cr.fill();
    super::gutter::set_source(&cr, paint.fg);
    super::gutter::rounded_rect(&cr, fx, fy, sheet, sheet, sheet_r);
    let _ = cr.stroke();
}

/// **Paint the copy button of every code block that should be showing one**, from the
/// card rectangles the below-text pass recorded, and record this frame's hit-boxes.
///
/// Lifted out of `snapshot_layer` whole, and sited here beside the drawing it drives:
/// the module already owned *how* a button looks, and `crate::affordance` owns where
/// it goes — this is the loop that turns one into the other.
pub(super) fn draw_all(snapshot: &gtk::Snapshot, ctx: &super::paint::PaintCtx) {
    let view = ctx.view;
    let bg = *ctx.imp.bg.borrow();
    // The code-block copy button, drawn ABOVE the text because it sits in
    // the card's top-right corner and a long first line runs underneath it
    // (its fill is the card's own, so it masks what it covers). Revealed for
    // the block under the pointer, and kept for a moment after a copy so the
    // checkmark is seen — the two states the reader can be in.
    let mut copy_hits: Vec<(graphene::Rect, usize)> = Vec::new();
    let hovered_block = ctx.imp.hovered_code_block.get();
    let copied_block = ctx.imp.copied_block.get();
    if any_copy_button_shown(hovered_block, copied_block) {
        let zoom = ctx.imp.gutter_zoom.get();
        // The block's own inner padding, through the same `px()` the
        // `code-block` tag's margins take — one value, so the button's inset
        // and the text's inset cannot drift apart.
        let pad = crate::theme::px(crate::config::config().code.block_padding, zoom) as f32;
        // One text row in the view's own CSS-zoomed font: a fresh Pango
        // layout, which validates nothing (GTK4Rs/AP-22), exactly as the
        // gutter's soft-wrap clamp measures it.
        let (_, row_h) = view.create_pango_layout(Some("0")).pixel_size();
        let row_h = row_h as f32;
        let fg = view.style_context().color();
        let accent = view
            .style_context()
            .lookup_color("theme_selected_bg_color")
            .or_else(|| view.style_context().lookup_color("accent_bg_color"))
            .unwrap_or(fg);
        for &(card, bi) in ctx.imp.code_block_rects.borrow().iter() {
            if !copy_button_shown(bi, hovered_block, copied_block) {
                continue;
            }
            let Some(rect) = crate::affordance::copy_button_rect(&card, pad, row_h) else {
                continue;
            };
            draw_copy_button(
                snapshot,
                &rect,
                zoom,
                &CopyButtonPaint {
                    fill: &bg,
                    fg: &fg,
                    // Only the button the pointer is actually ON adopts the
                    // accent border; a revealed-but-not-pointed-at button
                    // stays at rest, the same split the checkbox draws.
                    hover: (ctx.imp.pointer_on_copy_button.get() == Some(bi)).then_some(&accent),
                    copied: copied_block == Some(bi),
                },
            );
            copy_hits.push((rect, bi));
        }
    }
    *ctx.imp.copy_button_hitboxes.borrow_mut() = copy_hits;
}
