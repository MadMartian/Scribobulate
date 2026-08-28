//! **The right-margin annotation chips**, and the hit-boxes a click maps through.
//!
//! Lifted out of `snapshot_layer` whole. Drawn in the ABOVE-TEXT pass for a reason
//! that is not about ordering against another decoration: a cell annotation's chip
//! sits at the cell's buffer-Y, INSIDE an anchored table's vertical span, and the
//! below-text pass would put it behind that opaque widget — the "cell markers don't
//! show" defect. Its x column starts two pixels past the content column's right edge,
//! so it overlaps none of the block decorations in either direction.

use super::geometry::{chip_rect, marker_row_y_h};
use super::markers::group_by_line;
use super::paint::PaintCtx;
use crate::decorplan::{offset_on_screen, row_on_screen};
use gtk::graphene;
use gtk::prelude::*;

/// Paint every visible annotation chip, recording this frame's hit-boxes.
pub(super) fn draw(snapshot: &gtk::Snapshot, ctx: &PaintCtx) {
    let view = ctx.view;
    let buffer = &ctx.buffer;
    let markers = ctx.imp.markers.borrow();
    let (vis_start, vis_end, vtop, vbot) = (ctx.vis_start, ctx.vis_end, ctx.vtop, ctx.vbot);
    let rm = ctx.rm;
    // CriticMarkup comment markers: a small amber chip in the reserved right
    // margin at each annotated line. When several annotations share one visual
    // line they collapse to a single chip showing a count. Visible-only
    // measurement, viewport-anchored — never reads an off-screen (unvalidated)
    // iter (GTK4Rs/AP-22), the discipline every decoration here shares.
    let mut hitboxes: Vec<(graphene::Rect, Vec<usize>)> = Vec::new();
    if !markers.is_empty() {
        /// One on-screen annotation marker: which marker it is, and the
        /// buffer-space row it sits on. Named rather than a bare
        /// `(usize, f32, f32)` so the three readers below cannot transpose
        /// `y` and `h` — the project convention is to destructure by name.
        struct VisMarker {
            index: usize,
            y: f32,
            h: f32,
        }
        let mut vis_markers: Vec<VisMarker> = Vec::new();
        for (i, m) in markers.iter().enumerate() {
            let is_cell = m.cell_widget.is_some();
            // Body markers: cheap anchor-offset visibility gate. A CELL
            // marker's anchor is the table U+FFFC, which can scroll off-screen
            // while the cell's own row is still visible (tall tables), so defer
            // its culling to the refined-Y check below.
            if !is_cell && !offset_on_screen(m.anchor, vis_start, vis_end) {
                continue;
            }
            // Base Y (anchor line), refined to the exact table row for a CELL
            // marker, via the SHARED `marker_row_y_h` — the same formula the
            // annotation card anchors itself with, so the chip and the card
            // that points at it can never drift apart (GTK4Rs/AP-78/GTK4Rs/AP-127).
            // Recomputed EVERY frame: both halves are cheap and scroll-stable,
            // so there's nothing to cache against (no flicker, no stale cache).
            let (y, h) = marker_row_y_h(view, buffer, m);
            // Skip chips whose refined Y is fully outside the viewport
            // (tall tables: a lower-row cell can leave the view while the
            // table anchor line is still "visible" by buffer offset).
            if !row_on_screen(y, h, vtop, vbot) {
                continue;
            }
            vis_markers.push(VisMarker { index: i, y, h });
        }
        let ys: Vec<i32> = vis_markers.iter().map(|m| m.y as i32).collect();
        let width = view.width() as f32;
        // Themed, TDD 18.19: `None` on either key ⇒ the exact hardcoded
        // amber/white this chip has always used (TDD 18.2). A sprite, if
        // the theme names one AND it decodes, replaces the flat fill only —
        // the count numeral still draws in `accent`/chip-fg ink on top,
        // same as the flat-fill case.
        let chip_th = crate::theme::active();
        // The engine decides which of the chip's two FILL appearances
        // applies (`theme::Fill`); its INK is a separate key and paints on
        // top either way, which is the one respect the chip differs from
        // the bar and the rule.
        let chip_decor = chip_th.annotation_chip_decor();
        let accent = chip_decor.flat_or(crate::palette::ANNOTATION_CHIP_FLOOR);
        let ink = chip_th
            .annotation_chip_fg
            .unwrap_or(crate::palette::ANNOTATION_CHIP_INK_FLOOR);
        for (_gy, local) in group_by_line(&ys) {
            let VisMarker { y, h, .. } = vis_markers[local[0]];
            // SHARED chip arithmetic (`chip_rect`) — the annotation card
            // re-derives its anchor with the very same call, so the drawn chip
            // and the card pointing at it cannot disagree (GTK4Rs/AP-78/GTK4Rs/AP-127).
            let (raw_x, raw_y, raw_w, raw_h) = chip_rect(width, rm, y, h);
            // ROUNDED ONCE, here, and then used by all four of the chip's
            // faces — the sprite, the flat fill, the count numeral and the
            // hit-box. The rounding is this site's own decision (the shared
            // `chip_rect` stays fractional for the card's anchor): the rect
            // comes from a text row, so a sprite drawn on a half-pixel
            // boundary is resampled a second time by the compositor. Rounding
            // only the sprite would make what is DRAWN and what is CLICKABLE
            // two different rectangles, which is an interaction defect rather
            // than a cosmetic one (GTK4Rs/AP-78).
            let (chip_x, cy) = (raw_x.round(), raw_y.round());
            let (marker_w, chip_h) = (raw_w.round().max(1.0), raw_h.round().max(1.0));
            let rect = graphene::Rect::new(chip_x, cy, marker_w, chip_h);
            // Through the SHARED resample-and-draw seam (the twin of
            // `tile_texture`).
            let sprite_drawn = chip_decor
                .sprite
                .is_some_and(|sprite| crate::widgets::draw_sprite_into(snapshot, &rect, sprite));
            // A sprite that will not decode falls back to the flat fill —
            // degrading, not erasing.
            if !sprite_drawn {
                snapshot.append_color(&accent, &rect);
            }
            if local.len() > 1 {
                let layout = view.create_pango_layout(Some(&local.len().to_string()));
                snapshot.save();
                snapshot.translate(&graphene::Point::new(chip_x + 2.0, cy - 1.0));
                snapshot.append_layout(&layout, &ink);
                snapshot.restore();
            }
            // Hit-box in WIDGET coords. Convert the chip's buffer-y to widget-y
            // with GTK's own transform (chip_x is already widget-space). Using
            // `cy - visible_rect().y()` drifts by the top margin on compositors
            // where the two disagree — the same bug that displaced the task
            // checkbox hit zone (operator, 2026-07-16); GTK4Rs/AP-80-safe (pure math).
            let (_, chip_wy) =
                view.buffer_to_window_coords(gtk::TextWindowType::Widget, 0, cy as i32);
            let ann_idxs: Vec<usize> = local.iter().map(|&li| vis_markers[li].index).collect();
            hitboxes.push((
                graphene::Rect::new(chip_x, chip_wy as f32, marker_w, chip_h),
                ann_idxs,
            ));
        }
    }
    *ctx.imp.marker_hitboxes.borrow_mut() = hitboxes;
}
