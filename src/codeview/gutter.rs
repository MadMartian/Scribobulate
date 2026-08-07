//! Drawn list-marker gutter for [`CodePreviewView`]: static
//! bullet dots, right-aligned ordered numbers, and static task checkboxes painted in
//! the band immediately LEFT of each list item's content margin, aligned to the item's
//! FIRST line — the word-processor presentation from the plan's Research findings.
//!
//! Two rules keep this correct and crash-free:
//!  * **x derives PURELY from the container base + nesting depth**
//!    ([`list_content_margin_px`], mirroring the `li-{depth}` tag's *accumulative*
//!    `left_margin = px(depth*list_step)` in `tags.rs`), NEVER from laid-out GTK
//!    geometry — so the marker column lines up with the content margin at any zoom.
//!    The base is the container's own text margin (the view's configured `left_margin`,
//!    plus the blockquote indent for a quoted item), which is a *set property*, not a
//!    lazily-validated layout read — safe in a snapshot (GTK4Rs/AP-22 applies to the y, not x).
//!  * **y comes from the caller's cache-free `line_yrange` read** (research §4 —
//!    never `iter_location`, which validates/caches a line display mid-snapshot →
//!    GTK4Rs/AP-22 blank view / ScrAP-105 btree abort). The caller paints VISIBLE lines only, so
//!    `line_yrange` is on a validated line (its 0/stale hazard doesn't bite here).
//!
//! Shapes draw via Cairo (`snapshot.append_cairo`): `Snapshot::append_stroke` /
//! `append_fill` are GTK 4.14+, above the 4.6–4.12 target, so an arbitrary rounded box
//! outline + checkmark path needs Cairo. The number is a Pango layout sized by the
//! view's own (CSS-zoomed) font, so it matches the item text automatically. Colour is
//! the widget foreground (research §7 — deliberately libadwaita-free, no accent tint).

use crate::renderer::ListMarkerKind;
use crate::theme::Metrics;
use gtk::prelude::*;
use gtk::{cairo, gdk, graphene};

/// The per-level step / inter-item gap for one paint, as floats.
///
/// Both come from the ACTIVE THEME's `Metrics` — the SAME two keys `tags.rs` builds the
/// `li-{depth}` tags from — so the tag and this gutter cannot drift apart (POLICY "One
/// theme key, every application path"; they were once duplicated bare literals behind
/// "KEEP IN SYNC" comments, F-DRY-003). A themed `list_step` that reached the tag but
/// not this gutter would strand every marker off its text: GTK4Rs/AP-96's exact failure mode.
/// Taken as a parameter rather than read from the thread-local here, so every function
/// below stays pure and headlessly unit-testable at any metric.
fn step_f(m: &Metrics) -> f32 {
    m.list_step as f32
}
fn gap_f(m: &Metrics) -> f32 {
    m.list_item_gap as f32
}

/// Content margin (buffer x, px) for a list item at `depth` inside a container whose own
/// text margin is `base` px, EXACTLY mirroring how GTK resolves the item's line:
/// `base` (the highest-priority NON-accumulative margin — the view default, or the
/// `blockquote` tag's) + the `li-{depth}` tag's ACCUMULATIVE `px(depth*list_step)`
/// (`px(n) = round(n*zoom)`, so the two roundings compose exactly as `tags.rs` applies
/// them — do NOT fold this into one `round(base + depth*STEP*zoom)`). The marker draws in
/// the band immediately LEFT of the returned margin.
///
/// Taking `base` as an argument (rather than assuming the body margin) is what keeps a
/// list nested inside a blockquote correct: its markers sit in the quote's indented
/// column instead of left of the quote's accent bar (POLICY Document Rendering CAM row 2).
/// Pure — unit-tested without a display; GTK stays at the edges.
pub(crate) fn list_content_margin_px(base: f32, depth: usize, zoom: f64, m: &Metrics) -> f32 {
    base + ((depth as i32 * m.list_step) as f64 * zoom).round() as f32
}

/// The checkbox square (buffer coordinates) for a `Task` marker at `content_margin`,
/// aligned to the item's first line `line = (top_y, height)`, at `zoom`. Pure — the
/// hit-test recording (`snapshot_layer` in `mod.rs`) and the paint (`draw_list_marker`'s
/// Task branch) BOTH derive the box from this one function, so the hover band can never
/// drift from the drawn checkbox. Unit-tested without a display; GTK stays at the edges.
pub(crate) fn checkbox_rect(
    content_margin: f32,
    line: (f32, f32),
    zoom: f64,
    m: &Metrics,
) -> graphene::Rect {
    let (y, h) = line;
    let z = zoom as f32;
    let gap = marker_gap_px(m, zoom);
    let text_top = y + gap;
    let text_h = (h - gap).max(0.0);
    let text_cy = text_top + text_h / 2.0;
    let col_cx = content_margin - (step_f(m) / 2.0) * z;
    let s = (13.0 * z).round().max(9.0);
    graphene::Rect::new(col_cx - s / 2.0, text_cy - s / 2.0, s, s)
}

/// Clamp a logical line's `(top_y, full_height)` — as returned by the cache-free
/// `GtkTextView::line_yrange`, which spans EVERY soft-wrapped display row of the
/// paragraph — down to just its FIRST display row.
///
/// `single_line_h` is that row's text height (a fresh Pango layout in the view's own
/// CSS-zoomed font — cache-free, never `iter_location`: GTK4Rs/AP-22) and `gap` is the item's
/// `pixels_above_lines` (= `px(list_item_gap)`, the inter-item gap the text sits below).
/// Markers centre on the text midline (`draw_list_marker` / `checkbox_rect`), so without
/// this clamp a soft-wrapped item's `text_h` is `N · single_line_h` and its marker floats
/// to the MIDDLE display row instead of staying top-aligned on the first row (operator
/// report 2026-07-22 — worse at higher zoom, which grows the rows and provokes the wrap).
/// A single-row item is unchanged: its full height already equals `gap + single_line_h`
/// exactly (`pixels_above_lines` and the Pango row height are the same integers GTK laid
/// the line out with, and `pixels_inside_wrap = 0`), so `min` is a no-op there. Pure —
/// unit-tested without a display.
pub(crate) fn first_display_line(line: (f32, f32), single_line_h: f32, gap: f32) -> (f32, f32) {
    let (y, h) = line;
    (y, h.min(gap + single_line_h))
}

/// The item's first-line `gap` in device px (`pixels_above_lines`, = `px(list_item_gap)`).
/// One definition so [`first_display_line`]'s clamp and [`draw_list_marker`]'s `text_top`
/// use the identical value (they must agree, or the clamp cuts the wrong band).
pub(crate) fn marker_gap_px(m: &Metrics, zoom: f64) -> f32 {
    (gap_f(m) * zoom as f32).round()
}

/// Everything about HOW to paint one marker, other than where: the resting `fg`,
/// plus `hover` = `Some(accent)` when this marker's `Task` checkbox is under the
/// pointer (its border then strokes thicker in the accent colour) and `None` at
/// rest, plus the active theme's `metrics` (the step/gap this marker's column and
/// vertical centring derive from). Bundled so [`draw_list_marker`] stays within the
/// argument-count lint. Bullets/numbers ignore `hover` — they aren't interactive.
pub(crate) struct MarkerPaint<'a> {
    pub fg: &'a gdk::RGBA,
    pub hover: Option<&'a gdk::RGBA>,
    pub metrics: &'a Metrics,
}

/// Draw one list item's marker in the gutter left of `content_margin`, aligned to the
/// item's first line `line = (top_y, height)` (from the caller's cache-free
/// `line_yrange`, in the buffer coordinates `snapshot_layer` paints in). `paint.fg` is
/// the widget foreground colour; `paint.hover` (an accent) applies ONLY to a hovered
/// `Task` checkbox.
pub(crate) fn draw_list_marker(
    snapshot: &gtk::Snapshot,
    view: &gtk::TextView,
    kind: &ListMarkerKind,
    content_margin: f32,
    line: (f32, f32),
    zoom: f64,
    paint: &MarkerPaint,
) {
    let fg = paint.fg;
    let m = paint.metrics;
    let (y, h) = line;
    let z = zoom as f32;
    // The item text sits below the first-line gap; center markers on the text, not the
    // full line box, so bullets/checkboxes read on the text's midline. `line` is the
    // item's FIRST display row only (the caller clamps the logical-line height via
    // `first_display_line`) — otherwise a soft-wrapped item centres over every row.
    let gap = marker_gap_px(m, zoom);
    let text_top = y + gap;
    let text_h = (h - gap).max(0.0);
    let text_cy = text_top + text_h / 2.0;
    // Bullets/checkboxes center in the band half a step left of the content margin
    // (Word's hanging-indent gutter is half the per-level step).
    let col_cx = content_margin - (step_f(m) / 2.0) * z;

    match kind {
        ListMarkerKind::Bullet => {
            let r = (2.5 * z).max(1.5);
            let bounds = graphene::Rect::new(
                (col_cx - r - 1.0).max(0.0),
                text_cy - r - 1.0,
                r * 2.0 + 2.0,
                r * 2.0 + 2.0,
            );
            let cr = snapshot.append_cairo(&bounds);
            set_source(&cr, fg);
            cr.arc(
                col_cx as f64,
                text_cy as f64,
                r as f64,
                0.0,
                std::f64::consts::TAU,
            );
            let _ = cr.fill();
        }
        ListMarkerKind::Ordered(n) => {
            // Right-align "n." so the period sits just-left of the content margin, so
            // `9.` and `10.` line up on their periods (research A, "DDDLU"). The layout
            // uses the view's own font (CSS-zoomed), so it matches the item text size.
            let layout = view.create_pango_layout(Some(&format!("{n}.")));
            let (lw, lh) = layout.pixel_size();
            let period_gap = (6.0 * z).round();
            let x = content_margin - period_gap - lw as f32;
            // Same font as the item text → matching centers give matching baselines.
            let top = text_top + (text_h - lh as f32) / 2.0;
            snapshot.save();
            snapshot.translate(&graphene::Point::new(x, top));
            snapshot.append_layout(&layout, fg);
            snapshot.restore();
        }
        ListMarkerKind::Task { checked, .. } => {
            // A static box outline (+ checkmark when checked) — custom-drawn, no live
            // GtkCheckButton and no icon-theme dependency (research §7; GTK4Rs/AP-91). The
            // box geometry comes from the shared pure `checkbox_rect` so the hover
            // hit-band (recorded in mod.rs) matches the painted box exactly.
            let rect = checkbox_rect(content_margin, line, zoom, m);
            let (bx, by, s) = (rect.x(), rect.y(), rect.width());
            // On hover: a thicker, accent-coloured border signals clickability; resting
            // keeps the plain foreground outline. Widen the Cairo bounds by half the
            // (larger) line width so the thicker stroke is never clipped.
            let stroke = paint.hover.unwrap_or(fg);
            let lw = if paint.hover.is_some() {
                (2.4 * z).max(1.8)
            } else {
                (1.5 * z).max(1.0)
            };
            let hp = (lw / 2.0 + 1.0).max(2.0);
            let bounds = graphene::Rect::new(bx - hp, by - hp, s + 2.0 * hp, s + 2.0 * hp);
            let cr = snapshot.append_cairo(&bounds);
            set_source(&cr, stroke);
            cr.set_line_width(lw as f64);
            rounded_rect(
                &cr,
                bx as f64,
                by as f64,
                s as f64,
                s as f64,
                (3.0 * z).min(s / 3.0) as f64,
            );
            let _ = cr.stroke();
            if *checked {
                // Keep the checkmark in the foreground colour (legible content ink)
                // regardless of hover — only the box border adopts the accent.
                set_source(&cr, fg);
                cr.set_line_width((2.0 * z).max(1.2) as f64);
                cr.set_line_cap(cairo::LineCap::Round);
                cr.set_line_join(cairo::LineJoin::Round);
                let (bxf, byf, sf) = (bx as f64, by as f64, s as f64);
                cr.move_to(bxf + sf * 0.24, byf + sf * 0.52);
                cr.line_to(bxf + sf * 0.42, byf + sf * 0.70);
                cr.line_to(bxf + sf * 0.76, byf + sf * 0.30);
                let _ = cr.stroke();
            }
        }
    }
}

fn set_source(cr: &cairo::Context, fg: &gdk::RGBA) {
    cr.set_source_rgba(
        fg.red() as f64,
        fg.green() as f64,
        fg.blue() as f64,
        fg.alpha() as f64,
    );
}

/// Trace a rounded-rectangle path (four quarter-circle corners) on `cr`.
fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::{FRAC_PI_2, PI};
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

#[cfg(test)]
mod tests {
    use super::{checkbox_rect, first_display_line, list_content_margin_px, marker_gap_px};
    use crate::theme::{Metrics, Themes};

    /// The shipped system theme's metrics — so these assertions are pinned to the
    /// real data file, not to a copy of it that could drift.
    fn sys() -> Metrics {
        Themes::builtin().resolve(crate::theme::SYSTEM_ID).metrics
    }

    #[test]
    fn checkbox_rect_centers_half_a_step_left_of_the_content_margin() {
        // depth-1 content margin at zoom 1.0 = 28; first line top 100, height 24.
        let r = checkbox_rect(28.0, (100.0, 24.0), 1.0, &sys());
        // Box size = round(13*zoom) (floor 9).
        assert_eq!(r.width(), 13.0);
        assert_eq!(r.height(), 13.0);
        // Center x sits half a per-level step (14 px @1.0) left of the content margin.
        let cx = r.x() + r.width() / 2.0;
        assert!((cx - (28.0 - 14.0)).abs() < 1e-4, "cx={cx}");
        // Center y is on the TEXT midline: gap=round(8)=8 → text_top=108, text_h=16 →
        // text_cy = 108 + 8 = 116.
        let cy = r.y() + r.height() / 2.0;
        assert!((cy - 116.0).abs() < 1e-4, "cy={cy}");
    }

    #[test]
    fn checkbox_rect_scales_with_zoom() {
        // At zoom 2.0, depth-2 margin 56: size round(26)=26; center x = 56 - 28.
        let r = checkbox_rect(56.0, (0.0, 40.0), 2.0, &sys());
        assert_eq!(r.width(), 26.0);
        let cx = r.x() + r.width() / 2.0;
        assert!((cx - (56.0 - 28.0)).abs() < 1e-4, "cx={cx}");
        // gap=round(list_item_gap*2)=round(16)=16 → text_top=16, text_h=24 → text_cy = 28.
        let cy = r.y() + r.height() / 2.0;
        assert!((cy - 28.0).abs() < 1e-4, "cy={cy}");
    }

    #[test]
    fn content_margin_matches_li_tag_px_at_various_zooms() {
        // Must equal `base + px(depth*list_step)` where `px(n) = round(n*zoom)` — the sum
        // GTK resolves for a line carrying the ACCUMULATIVE `li-{depth}` tag. `base` here
        // is the view's own `left_margin` (config default px(20) @1.0).
        let m = sys();
        // depth 1/2 at 1.0 → 20+28 / 20+56.
        assert_eq!(list_content_margin_px(20.0, 1, 1.0, &m), 48.0);
        assert_eq!(list_content_margin_px(20.0, 2, 1.0, &m), 76.0);
        // Zoom scales the indent: depth 3 @1.5 → base + round(84*1.5) = 30 + 126.
        assert_eq!(list_content_margin_px(30.0, 3, 1.5, &m), 156.0);
        // Rounding matches px(): depth 1 @1.25 → 25 + round(35) = 60.
        assert_eq!(list_content_margin_px(25.0, 1, 1.25, &m), 60.0);
        // The two roundings must compose SEPARATELY, exactly as tags.rs applies them:
        // the base is already px()'d by its own site, then the indent is px()'d here.
        assert_eq!(list_content_margin_px(22.0, 1, 1.1, &m), 53.0);
    }

    #[test]
    fn content_margin_nests_a_quoted_list_inside_its_blockquote() {
        // POLICY Document Rendering CAM row 2: a list inside a blockquote must sit INSIDE
        // the quote. The quoted base is the `blockquote` tag's margin
        // (view_lm + px(bar_width + text_gap) = 20 + 13 = 33 @1.0); the item's own
        // accumulative indent adds on top, so depth 1 lands at 33 + 28 = 61 — verified
        // against live GTK. The body-based margin (48) would put the item, and its marker
        // column half a step left of it, on the wrong side of the quote's accent bar.
        let m = sys();
        let quoted = list_content_margin_px(33.0, 1, 1.0, &m);
        assert_eq!(quoted, 61.0);
        // The marker column sits half a step (14 @1.0) left of the content margin — still
        // right of the quote's bar+gap (33), i.e. inside the quote, not beside it.
        assert!(quoted - 14.0 > 33.0, "marker column escaped the blockquote");
        // A quoted item is indented strictly further than the same item at body level.
        assert!(quoted > list_content_margin_px(20.0, 1, 1.0, &m));
    }

    /// TDD 18.10 / GTK4Rs/AP-96 — a THEMED `list_step` must move the marker column and the
    /// content margin TOGETHER. This is the drift the one-key rule exists to prevent:
    /// the gutter and `tags.rs` read the same key, so a theme cannot strand a marker.
    #[test]
    fn a_themed_list_step_moves_the_marker_and_its_text_together() {
        let mut themes = Themes::builtin();
        themes.merge_over_for_test("[themes.wide]\nlist_step = 40\n");
        let m = themes.resolve("wide").metrics;
        assert_eq!(m.list_step, 40);
        // The content margin follows the themed step…
        let margin = list_content_margin_px(20.0, 1, 1.0, &m);
        assert_eq!(margin, 60.0);
        // …and the marker column stays exactly half a themed step left of it, rather
        // than half a hardcoded 28 (which would leave it 6px adrift of its own text).
        let r = checkbox_rect(margin, (0.0, 24.0), 1.0, &m);
        let cx = r.x() + r.width() / 2.0;
        assert!((cx - (margin - 20.0)).abs() < 1e-4, "cx={cx}");
    }

    /// A themed inter-item gap must move the marker's vertical centring too — the
    /// marker centres on the item's TEXT, which starts below the gap.
    #[test]
    fn a_themed_item_gap_recentres_the_marker_on_its_text() {
        let mut themes = Themes::builtin();
        themes.merge_over_for_test("[themes.airy]\nlist_item_gap = 16\n");
        let m = themes.resolve("airy").metrics;
        // gap=16 → text_top=16, text_h=24-16=8 → text_cy = 16 + 4 = 20.
        let r = checkbox_rect(28.0, (0.0, 24.0), 1.0, &m);
        let cy = r.y() + r.height() / 2.0;
        assert!((cy - 20.0).abs() < 1e-4, "cy={cy}");
    }

    /// The soft-wrap fix (operator report 2026-07-22): `line_yrange` returns the WHOLE
    /// logical line, so a 3-row wrapped item arrives here as `(y, gap + 3·single_line_h)`.
    /// `first_display_line` must clamp that to the FIRST row (`gap + single_line_h`) so the
    /// marker centres on row 1, not the middle row. A single-row item is a no-op.
    #[test]
    fn first_display_line_clamps_a_wrapped_item_to_its_first_row() {
        let gap = 8.0;
        let single = 20.0;
        // Single row: full height already == gap + single → unchanged (byte-identical).
        assert_eq!(
            first_display_line((100.0, gap + single), single, gap),
            (100.0, 28.0)
        );
        // Three wrapped rows (pixels_inside_wrap = 0): gap + 3·single = 68 → clamped to 28.
        assert_eq!(
            first_display_line((100.0, gap + 3.0 * single), single, gap),
            (100.0, 28.0)
        );
        // The clamp keeps the marker on the FIRST row's midline: for the wrapped item the
        // centre is text_top + single/2 = 108 + 10 = 118, identical to the single-row item —
        // NOT the whole-block midline (100 + 68/2 = 134) it drew at before the fix.
        let (y, h) = first_display_line((100.0, gap + 3.0 * single), single, gap);
        let cy = (y + gap) + (h - gap) / 2.0;
        assert!((cy - 118.0).abs() < 1e-4, "cy={cy}");
    }

    /// `marker_gap_px` is the single definition of the first-line gap the clamp and the
    /// paint both rely on: `round(list_item_gap · zoom)`, matching `tags.rs`'s `px()`.
    #[test]
    fn marker_gap_px_matches_px_of_the_item_gap() {
        let m = sys(); // shipped list_item_gap
        assert_eq!(marker_gap_px(&m, 1.0), (m.list_item_gap as f32).round());
        assert_eq!(
            marker_gap_px(&m, 2.0),
            (m.list_item_gap as f32 * 2.0).round()
        );
    }
}
