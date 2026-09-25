//! **The task checkbox as a drawn shape** — its outline path, where a themed tick sits
//! inside it, and a raster of the whole box for a sink that cannot draw it in place.
//!
//! Display-free (Cairo and Pango only), because three renderings draw this one box: the
//! preview's gutter (`codeview::gutter`), the PDF sink (which lays a marker picture in
//! its own gutter), and — through CSS rather than this module — the HTML sink. The first
//! two used to share only the outline path, which lived in `codeview::gutter`; the PDF
//! sink could not reach it from there without depending on the widget layer, so the
//! box's geometry moved here once a second drawer needed it.
//!
//! The tick is the part a theme can change (`list_task_tick_glyph`, TDD 18.63): a glyph
//! drawn INSIDE the box in place of the checkmark, the box itself staying drawn in the
//! task marker's ink. That is a different substitution from `list_task_checked_glyph`,
//! which replaces the whole box.

use gtk::cairo;

/// The drawn box's side at zoom 1, in pixels. The preview scales it by zoom; the PDF
/// raster scales it to the page. The stroke and corner radius are stated at this side
/// so every drawer derives the same proportions from one place.
pub(crate) const DESIGN_SIDE: f64 = 13.0;
/// The resting outline's stroke width at [`DESIGN_SIDE`].
pub(crate) const DESIGN_STROKE: f64 = 1.5;
/// The outline's corner radius at [`DESIGN_SIDE`].
pub(crate) const DESIGN_RADIUS: f64 = 3.0;
/// The share of the box's side a themed tick's INK fills along its longer axis. Below 1
/// so the tick sits inside the stroke rather than over it.
pub(crate) const TICK_FILL: f64 = 0.82;

/// Trace a rounded-rectangle path (four quarter-circle corners) on `cr`.
pub(crate) fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::{FRAC_PI_2, PI};
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

/// A square in whichever unit its drawer works in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Square {
    pub x: f64,
    pub y: f64,
    pub side: f64,
}

/// A glyph's INK rectangle relative to its layout origin, as Pango reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ink {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Where to draw a tick layout so its ink is centred in `bx`: the scale to apply and
/// the layout origin in the box's own coordinates (the origin is where the SCALED
/// layout's top-left lands).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TickFit {
    pub scale: f64,
    pub x: f64,
    pub y: f64,
}

/// Fit a tick glyph into the box. Centred on its INK, not its logical rectangle: a
/// glyph's logical box carries the font's ascent and descent, so centring that would
/// sit most glyphs visibly high. `None` for an ink-less glyph (a space) or a degenerate
/// box — the caller then draws the default checkmark, so the checked state never goes
/// blank.
pub(crate) fn tick_fit(ink: Ink, bx: Square) -> Option<TickFit> {
    let longer = ink.w.max(ink.h);
    if longer <= 0.0 || bx.side <= 0.0 {
        return None;
    }
    let scale = bx.side * TICK_FILL / longer;
    let (cx, cy) = (bx.x + bx.side / 2.0, bx.y + bx.side / 2.0);
    Some(TickFit {
        scale,
        x: cx - (ink.x + ink.w / 2.0) * scale,
        y: cy - (ink.y + ink.h / 2.0) * scale,
    })
}

/// The share of a raster's side the box occupies. The PDF sink draws a marker picture as
/// a square at the row's full height, where the preview's box is smaller than its row;
/// the margin keeps the two the same size relative to the text.
const RASTER_BOX_SHARE: f64 = 0.62;
/// The raster's side in device pixels — enough that the box stays crisp on a printed
/// page at the sizes a list row reaches.
const RASTER_SIDE: i32 = 96;

/// The whole task box as a picture: the outline in `ink`, and `tick` centred inside it
/// when there is one. For a sink that draws markers as images (the PDF gutter). `None`
/// only if Cairo cannot allocate the surface.
pub(crate) fn raster(tick: Option<&str>, ink: &gtk::gdk::RGBA) -> Option<crate::sprite::Raster> {
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, RASTER_SIDE, RASTER_SIDE).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        let n = f64::from(RASTER_SIDE);
        let side = n * RASTER_BOX_SHARE;
        let bx = Square {
            x: (n - side) / 2.0,
            y: (n - side) / 2.0,
            side,
        };
        let k = side / DESIGN_SIDE;
        cr.set_source_rgba(
            f64::from(ink.red()),
            f64::from(ink.green()),
            f64::from(ink.blue()),
            f64::from(ink.alpha()),
        );
        cr.set_line_width(DESIGN_STROKE * k);
        rounded_rect(&cr, bx.x, bx.y, side, side, DESIGN_RADIUS * k);
        cr.stroke().ok()?;
        if let Some(text) = tick {
            let layout = pangocairo::functions::create_layout(&cr);
            layout.set_text(text);
            let (ink_rect, _) = layout.pixel_extents();
            let ink_box = Ink {
                x: f64::from(ink_rect.x()),
                y: f64::from(ink_rect.y()),
                w: f64::from(ink_rect.width()),
                h: f64::from(ink_rect.height()),
            };
            if let Some(fit) = tick_fit(ink_box, bx) {
                cr.translate(fit.x, fit.y);
                cr.scale(fit.scale, fit.scale);
                pangocairo::functions::show_layout(&cr, &layout);
            }
        }
    }
    surface.flush();
    let n = f64::from(RASTER_SIDE);
    Some((surface, n, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: Square = Square {
        x: 10.0,
        y: 20.0,
        side: 13.0,
    };

    #[test]
    fn a_tick_is_scaled_so_its_longer_side_fills_the_stated_share() {
        let fit = tick_fit(
            Ink {
                x: 0.0,
                y: 0.0,
                w: 20.0,
                h: 10.0,
            },
            BOX,
        )
        .unwrap();
        assert!((fit.scale * 20.0 - 13.0 * TICK_FILL).abs() < 1e-9);
    }

    #[test]
    fn a_tick_is_centred_on_its_ink_not_its_origin() {
        // Ink offset inside the layout, as a glyph with a left bearing and a high
        // ascent reports it.
        let ink = Ink {
            x: 3.0,
            y: 5.0,
            w: 10.0,
            h: 10.0,
        };
        let fit = tick_fit(ink, BOX).unwrap();
        let ink_cx = fit.x + (ink.x + ink.w / 2.0) * fit.scale;
        let ink_cy = fit.y + (ink.y + ink.h / 2.0) * fit.scale;
        assert!((ink_cx - (BOX.x + BOX.side / 2.0)).abs() < 1e-9);
        assert!((ink_cy - (BOX.y + BOX.side / 2.0)).abs() < 1e-9);
    }

    #[test]
    fn an_inkless_tick_or_an_empty_box_fits_nothing() {
        let blank = Ink {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
        assert!(tick_fit(blank, BOX).is_none());
        let ink = Ink {
            x: 0.0,
            y: 0.0,
            w: 4.0,
            h: 4.0,
        };
        assert!(tick_fit(ink, Square { side: 0.0, ..BOX }).is_none());
    }

    #[test]
    fn the_raster_draws_the_box_and_a_tick_inside_it() {
        let ink = gtk::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0);
        let (mut empty, _, _) = raster(None, &ink).unwrap();
        let (mut ticked, _, _) = raster(Some("X"), &ink).unwrap();
        let centre = |s: &mut cairo::ImageSurface| {
            let stride = s.stride() as usize;
            let mid = (RASTER_SIDE / 2) as usize;
            let data = s.data().unwrap();
            // ARGB32 is premultiplied, native-endian; the alpha byte is enough here.
            let px = &data[mid * stride + mid * 4..mid * stride + mid * 4 + 4];
            u32::from_ne_bytes([px[0], px[1], px[2], px[3]]) >> 24
        };
        assert_eq!(centre(&mut empty), 0, "an empty box has a clear centre");
        assert!(
            centre(&mut ticked) > 0,
            "the tick is drawn at the box's centre"
        );
    }
}
