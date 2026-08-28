//! **The per-item loop that places the drawn list gutter** — which items are on
//! screen, where each one's marker column sits, and the task checkboxes' hit-boxes.
//!
//! Lifted out of `snapshot_layer` whole. It is a separate module from
//! [`super::gutter`] rather than more lines in it because the two answer different
//! questions and only one of them needs a view: `gutter` owns the SHAPES and the pure
//! column arithmetic (unit-tested headlessly, at any metric and any zoom), this owns
//! the live geometry read and the paint. Merging them would also have pushed that file
//! well past the 500-line soft limit it already sits near.

use super::gutter::{
    draw_list_marker, first_display_line, list_content_margin_px, marker_gap_px, MarkerPaint,
};
use super::paint::PaintCtx;
use crate::decorplan::{offset_on_screen, row_on_screen};
use gtk::graphene;
use gtk::prelude::*;

/// Paint every visible list item's marker, recording the task checkboxes' hit-boxes.
pub(super) fn draw(snapshot: &gtk::Snapshot, ctx: &PaintCtx) {
    let view = ctx.view;
    let buffer = &ctx.buffer;
    let list_markers = ctx.imp.list_markers.borrow();
    let (vis_start, vis_end, vtop, vbot) = (ctx.vis_start, ctx.vis_end, ctx.vtop, ctx.vbot);
    let lm = ctx.lm;
    // A bullet dot / right-aligned number / static checkbox per item, drawn in
    // the band LEFT of the item's content margin, aligned to the item's FIRST
    // line. Paint VISIBLE first-lines only, so
    // the y read (`line_yrange`, cache-free — never `iter_location`, GTK4Rs/AP-22/
    // ScrAP-105; research §4) is on a validated line; x derives from `depth`
    // (`list_content_margin_px` == the `li-{depth}` content margin), never
    // from GTK geometry. Buffer-space, so scroll-correct for free.
    if !list_markers.is_empty() {
        let zoom = ctx.imp.gutter_zoom.get();
        // The container text margins a list item's own indent accumulates
        // ONTO, mirroring `tags.rs` exactly: the view's configured
        // `left_margin` for a body item, and the `blockquote` tag's
        // `left_margin + px(bar_width + text_gap)` for a quoted one.
        // Both are SET properties (same read the bq bar above does), not
        // lazily-validated layout — no GTK4Rs/AP-22 exposure. Without the quoted
        // base, a quoted list's markers land left of the quote's accent bar
        // (POLICY Document Rendering CAM row 2).
        let body_base = lm;
        // The SAME two theme keys `tags.rs` builds the `blockquote` tag's
        // margin from, so a themed bar/gap can never leave a quoted list's
        // markers beside the quote instead of inside it (GTK4Rs/AP-96).
        let qm = crate::theme::active();
        let quoted_base = lm
            + crate::theme::px(
                qm.metrics.blockquote_bar_width + qm.metrics.blockquote_text_gap,
                zoom,
            ) as f32;
        // Marker glyph ink: the active theme's `list_marker` if it sets one,
        // else the widget foreground (the pre-theming default — keeps System
        // byte-identical). Colours the bullet/numeral/checkbox only; the item
        // text is buffer content and unaffected.
        // The pre-theming default every marker falls back to. The THEMED
        // ink is resolved per item below, because a bullet's colour varies
        // by nesting depth (TDD 18.26) and this loop is where the depth is.
        let default_fg = view.style_context().color();
        // Accent colour for a hovered checkbox's border — the desktop's
        // accent, distinct from the resting foreground so the hover border
        // reads as a change. Through `palette`, which owns the name chain and
        // its floor; this site used to open-code both and was the one probe
        // `F-PROBE-001` could not reach from inside that module.
        let hover_fg = crate::palette::desktop_accent();
        let hovered = ctx.imp.hovered_checkbox.get();
        // Rebuild the checkbox hit-boxes for the visible task items this paint
        // (same clear+repopulate discipline as `marker_hitboxes`).
        let mut cbhits: Vec<(graphene::Rect, usize)> = Vec::new();
        // `line_yrange` returns the height of the WHOLE logical line, which for
        // a soft-wrapped item spans EVERY display row — centering a marker on
        // that whole span floats it to the MIDDLE row instead of the first
        // (operator report 2026-07-22, most visible at higher zoom, which grows
        // the rows and provokes the wrap). Clamp each item's height to its first
        // display row via `first_display_line`. `single_line_h` is one row's
        // text height in the view's OWN CSS-zoomed font — a fresh Pango layout
        // (cache-free, never `iter_location`: GTK4Rs/AP-22), the same font the ordered
        // numeral is drawn in, so it tracks zoom automatically. `marker_gap` is
        // the item's `pixels_above_lines`, the band the text sits below.
        let (_, single_line_h) = view.create_pango_layout(Some("0")).pixel_size();
        let single_line_h = single_line_h as f32;
        let marker_gap = marker_gap_px(&qm.metrics, zoom);
        for (idx, m) in list_markers.iter().enumerate() {
            // The item's first line must be within the on-screen span —
            // its `line_yrange` is 0/stale on an unvalidated line (research
            // §4). Offset gate mirrors the code-block/blockquote loops.
            if !offset_on_screen(m.first_line, vis_start, vis_end) {
                continue;
            }
            let (y, h) = view.line_yrange(&buffer.iter_at_offset(m.first_line));
            // Clamp the whole-logical-line height to the item's FIRST display
            // row so the marker stays top-aligned when the item soft-wraps
            // (see the `single_line_h` note above). A single-row item is
            // unchanged. Both the drawn marker and the checkbox hit-column below
            // derive from this clamped `(y, h)`, so they stay in lock-step.
            let (y, h) = first_display_line((y as f32, h as f32), single_line_h, marker_gap);
            // Straddle clamp: skip a first-line whose refined y left the
            // viewport (same guard the comment chips use).
            if !row_on_screen(y, h, vtop, vbot) {
                continue;
            }
            let base = if m.quoted { quoted_base } else { body_base };
            let content = list_content_margin_px(base, m.depth, zoom, &qm.metrics);
            // Only TASK checkboxes are interactive: record a generous hit-box for
            // the checkbox in BUFFER coordinates — the exact space the marker is
            // drawn in. `is_over_checkbox` converts the incoming widget-space
            // click back to buffer coords with GTK's own `window_to_buffer_coords`
            // (the precise inverse of the draw transform), so there is NO
            // hand-rolled scroll/margin math to drift — the earlier `- vtop`
            // version silently displaced the zone by the margin on real
            // compositors (invisible under Xvfb). The hit-box is the whole marker
            // COLUMN for this item — from the parent depth's content margin across
            // to this item's, and the full first-line height — so the small drawn
            // box is not a pixel-hunt: clicking anywhere in the checkbox's gutter
            // cell toggles it. Clamped to the item's own line so adjacent items'
            // columns never overlap.
            if matches!(m.kind, crate::renderer::ListMarkerKind::Task { .. }) {
                let col_left =
                    list_content_margin_px(base, m.depth.saturating_sub(1), zoom, &qm.metrics);
                cbhits.push((
                    graphene::Rect::new(col_left, y, (content - col_left).max(0.0), h),
                    idx,
                ));
            }
            // Resolved per item, from the tier this item's depth reads —
            // a pure step over the array `Theme::resolve` already folded,
            // so nothing here re-derives the shallower-tier fallback.
            let fg = qm
                .marker_ink(m.kind.theme_kind(), m.depth)
                .unwrap_or(default_fg);
            draw_list_marker(
                snapshot,
                view.upcast_ref::<gtk::TextView>(),
                &m.kind,
                content,
                (y, h),
                zoom,
                &MarkerPaint {
                    fg: &fg,
                    hover: (hovered == Some(idx)).then_some(&hover_fg),
                    metrics: &qm.metrics,
                    depth: m.depth,
                    glyphs: &qm.list_glyphs,
                    sprites: &qm.sprites,
                },
            );
        }
        *ctx.imp.checkbox_hitboxes.borrow_mut() = cbhits;
    }
}
