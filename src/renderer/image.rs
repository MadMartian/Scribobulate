//! The pure, unit-tested image decisions: the broken-image placeholder reason string,
//! and the display-extent arithmetic that decides how large an image is drawn. The GTK
//! sides — image anchoring in [`super::start`], the viewport clamp in
//! [`crate::codeview`] — apply what this module decides.

use crate::links::ImageResolution;

/// A drawn image's size in device pixels. A struct rather than a `(i32, i32)` because
/// both members are lengths of the same type in the same unit, which is the shape
/// that reads correctly at a call site and silently transposes at the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Extent {
    pub(crate) w: i32,
    pub(crate) h: i32,
}

/// The height that keeps `nat_w`×`nat_h`'s aspect ratio at a width of `w`.
///
/// `i64` throughout: the product overflows `i32` at around 46k×46k, which is inside
/// what `limits::MAX_IMAGE_PIXELS` admits for a wide-and-short image. Never zero — a
/// `GtkPicture` measured at height 0 short-circuits to a blank (ScrAP-32).
fn keep_aspect(nat_w: i32, nat_h: i32, w: i32) -> i32 {
    if nat_w <= 0 {
        return 1;
    }
    (i64::from(nat_h) * i64::from(w) / i64::from(nat_w)).max(1) as i32
}

/// An image's on-screen extent at `zoom`, from its intrinsic pixel size.
///
/// **Zoom does not reach an image through CSS.** The preview's zoom is a
/// `font-size: {zoom}em` rule on the text view (`window/zoom.rs`), which governs text
/// and nothing else; an anchored `GtkPicture`'s size is a widget property, so it is a
/// pixel metric and takes the same explicit `px(n, zoom)` scaling every themed metric
/// does (POLICY § "No hard-coded styling"). Without this the prose around a diagram
/// grows on zoom while the diagram — and the text drawn inside it — does not (TDD 13.11).
pub(crate) fn zoomed_extent(nat_w: i32, nat_h: i32, zoom: f64) -> Extent {
    let nat_w = nat_w.max(1);
    let nat_h = nat_h.max(1);
    let w = crate::theme::px(nat_w, zoom).max(1);
    Extent {
        w,
        h: keep_aspect(nat_w, nat_h, w),
    }
}

/// `extent` reduced, aspect preserved, until it fits inside
/// [`crate::limits::MAX_VECTOR_RASTER_PIXELS`]; unchanged when it already does.
///
/// This bounds what a vector re-render is allowed to ALLOCATE, and deliberately not what
/// the image is drawn at: the caller keeps drawing at the zoomed extent, so a diagram
/// past the budget goes soft rather than small (TDD 13.11). Returning a smaller *display*
/// size here would make an image shrink as the reader zoomed in, which is the original
/// complaint wearing a different face.
///
/// It is also the **pixel bound** for a scalable source, and the only one that means
/// anything there: a `viewBox="0 0 24 24"` document probes as 576 pixels and clears
/// `limits::image_pixels_within_cap` trivially, so the natural size the caller checked
/// says nothing about the raster it is about to ask for. Applied to the target, before
/// the decode.
pub(crate) fn cap_raster(extent: Extent) -> Extent {
    let budget = crate::limits::MAX_VECTOR_RASTER_PIXELS;
    let pixels = i64::from(extent.w.max(1)) * i64::from(extent.h.max(1));
    if pixels <= budget {
        return extent;
    }
    // Area scales with the square of a linear factor, so the linear one is the square
    // root of the area ratio. `floor` rather than `round`: a rounded factor can land a
    // pixel over the budget, and a budget that is exceeded by rounding is not a budget.
    let factor = (budget as f64 / pixels as f64).sqrt();
    let w = ((f64::from(extent.w) * factor).floor() as i32).max(1);
    Extent {
        w,
        h: keep_aspect(extent.w, extent.h, w),
    }
}

/// `extent` scaled down to fit `bound` px of width, aspect preserved; unchanged when it
/// already fits.
///
/// The `max-width: 100%` half of the display policy (TDD 2.21): an image never grows to
/// fill the column and never exceeds it. Exceeding it is not merely untidy — an anchored
/// child wider than the content column re-arms the Automatic h-scrollbar churn that
/// blanks the pane (ScrAP-23).
pub(crate) fn fit_within(extent: Extent, bound: i32) -> Extent {
    if bound <= 0 || extent.w <= bound {
        return extent;
    }
    Extent {
        w: bound.max(1),
        h: keep_aspect(extent.w, extent.h, bound.max(1)),
    }
}

/// The displayed extent of an image at `zoom`, with its fit to `bound` decided at zoom
/// 1.0 and then scaled — so zoom goes on enlarging an image that was ALREADY clamped.
///
/// `zoomed` is what [`zoomed_extent`] produced at the current zoom, `zoom` is that factor.
/// **This is the case the whole feature was reported for and the one a plain clamp does
/// not reach**: a diagram wider than the pane is already fitted at 100%, so scaling and
/// then re-clamping leaves it exactly where it was, and the reader who zooms in to read
/// its lettering gets nothing. Fitting first and scaling the fit gives zoom something to
/// act on.
///
/// **At zoom 1.0 and below this IS [`fit_within`]** — the default view is unchanged, and
/// only a reader who has deliberately zoomed in can produce an image wider than the
/// column.
///
/// **It produces an anchored child wider than the content column, which ScrAP-23 warns
/// about.** That was measured rather than assumed before it landed: the whole zoom ladder
/// plus an eight-width resize sweep at 300%, on the `image-test.md` corpus, left the pane
/// drawn at every step with no `Gtk-CRITICAL` and no blank (Xvfb, GTK 4.6.9 — which does
/// not clear the compositor/WM class of failure, so the platform seats re-run it). The
/// horizontal scrollbar it summons is the accepted cost, and is what TDD 2.21's zoom
/// clause now says.
pub(crate) fn fit_scaled(zoomed: Extent, zoom: f64, bound: i32) -> Extent {
    if zoom <= 1.0 {
        return fit_within(zoomed, bound);
    }
    let at_one_w = ((f64::from(zoomed.w) / zoom).round() as i32).max(1);
    let at_one = Extent {
        w: at_one_w,
        h: keep_aspect(zoomed.w, zoomed.h, at_one_w),
    };
    let fitted = fit_within(at_one, bound);
    let w = ((f64::from(fitted.w) * zoom).round() as i32).max(1);
    Extent {
        w,
        h: keep_aspect(fitted.w, fitted.h, w),
    }
}

/// The tooltip for the broken-image placeholder shown when an image cannot be
/// displayed, or `None` when it loaded and no placeholder is needed.
///
/// Every non-renderable image gets a VISIBLE placeholder (with this tooltip)
/// rather than silently degrading to bare alt text — so the reader always sees
/// that an image was expected AND why it isn't there. The three reasons are
/// distinguished (they were previously conflated: Refused showed an icon while
/// Missing / failed-to-decode silently fell back to alt text, so toggling
/// "Show Unsafe Images" on an image whose file was absent removed the icon and
/// left only the alt string, looking like the toggle did nothing).
///
/// `has_texture` is whether a `GdkTexture` actually loaded: a `Local`/`Remote`
/// resolution with no texture is a decode/read failure, distinct from a path
/// that could not be resolved at all (`Missing`).
pub(crate) fn image_placeholder_tooltip(
    resolution: &ImageResolution,
    has_texture: bool,
    src: &str,
) -> Option<String> {
    if has_texture {
        return None;
    }
    Some(match resolution {
        ImageResolution::Refused => {
            format!("Blocked image (enable Show Unsafe Images to load): {src}")
        }
        ImageResolution::Missing => format!("Image not found: {src}"),
        // Resolved to a path/URI, but the texture never loaded — the file exists
        // (or the URL was fetched) yet could not be decoded as an image.
        ImageResolution::Local(_) | ImageResolution::Remote(_) => {
            format!("Could not load image: {src}")
        }
    })
}

#[cfg(test)]
mod image_placeholder_tests {
    use super::image_placeholder_tooltip;
    use crate::links::ImageResolution;
    use std::path::PathBuf;

    #[test]
    fn loaded_image_has_no_placeholder() {
        // A texture loaded → the picture stands in, no placeholder tooltip.
        assert!(image_placeholder_tooltip(
            &ImageResolution::Local(PathBuf::from("/x/y.png")),
            true,
            "y.png"
        )
        .is_none());
        assert!(image_placeholder_tooltip(
            &ImageResolution::Remote("https://h/i.png".into()),
            true,
            "https://h/i.png"
        )
        .is_none());
    }

    #[test]
    fn refused_reports_the_policy_block() {
        let t =
            image_placeholder_tooltip(&ImageResolution::Refused, false, "../secret.svg").unwrap();
        assert!(t.contains("Show Unsafe Images"), "{t}");
        assert!(t.contains("../secret.svg"), "{t}");
    }

    #[test]
    fn missing_reports_not_found_with_the_src() {
        // The regression: a Missing image (file absent at render time) must now
        // show a "not found" placeholder rather than silently degrading to alt
        // text — so toggling Show Unsafe Images on an absent file no longer
        // leaves a bare alt string that looks like the toggle did nothing.
        let t =
            image_placeholder_tooltip(&ImageResolution::Missing, false, "./target-structure.svg")
                .unwrap();
        assert!(t.contains("not found"), "{t}");
        assert!(t.contains("./target-structure.svg"), "{t}");
    }

    #[test]
    fn resolved_but_undecodable_reports_load_failure() {
        // Path resolved / URL fetched, but no texture → decode failure, distinct
        // from Missing.
        let local = image_placeholder_tooltip(
            &ImageResolution::Local(PathBuf::from("/x/bad.png")),
            false,
            "bad.png",
        )
        .unwrap();
        assert!(local.contains("Could not load"), "{local}");
        assert!(local.contains("bad.png"), "{local}");
        let remote = image_placeholder_tooltip(
            &ImageResolution::Remote("https://h/bad.png".into()),
            false,
            "https://h/bad.png",
        )
        .unwrap();
        assert!(remote.contains("Could not load"), "{remote}");
    }
}

#[cfg(test)]
mod extent_tests {
    use super::{fit_within, zoomed_extent, Extent};

    #[test]
    fn zoom_one_is_the_intrinsic_size() {
        assert_eq!(zoomed_extent(800, 600, 1.0), Extent { w: 800, h: 600 });
    }

    #[test]
    fn the_extent_tracks_the_zoom_factor() {
        // The regression this whole feature exists for: at 200% an image was still
        // drawn at its intrinsic size while the prose around it doubled, so the text
        // inside a diagram stayed unreadable however far the reader zoomed (TDD 13.11).
        assert_eq!(zoomed_extent(800, 600, 2.0), Extent { w: 1600, h: 1200 });
        assert_eq!(zoomed_extent(800, 600, 0.5), Extent { w: 400, h: 300 });
        assert_eq!(zoomed_extent(1000, 1112, 3.0).w, 3000);
    }

    #[test]
    fn the_aspect_ratio_survives_every_ladder_step() {
        // Height is derived from the scaled width rather than scaled independently, so
        // the two roundings cannot disagree and leave the picture subtly stretched.
        for zoom in [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
            let e = zoomed_extent(1000, 1112, zoom);
            let ratio = f64::from(e.h) / f64::from(e.w);
            assert!(
                (ratio - 1.112).abs() < 0.002,
                "aspect drifted at zoom {zoom}: {e:?}"
            );
        }
    }

    #[test]
    fn a_degenerate_intrinsic_size_still_yields_a_drawable_extent() {
        // A zero dimension reaching a GtkPicture is a blank picture, not a small one
        // (ScrAP-32), and zoom-out is a second way to reach zero.
        assert_eq!(zoomed_extent(0, 0, 1.0), Extent { w: 1, h: 1 });
        let tiny = zoomed_extent(1, 1, 0.5);
        assert!(tiny.w >= 1 && tiny.h >= 1, "{tiny:?}");
    }

    #[test]
    fn an_extent_within_the_bound_is_left_alone() {
        let e = Extent { w: 400, h: 300 };
        assert_eq!(fit_within(e, 800), e, "never upscaled to fill the column");
        assert_eq!(fit_within(e, 400), e, "exactly fitting is fitting");
    }

    #[test]
    fn an_oversized_extent_is_scaled_down_with_its_aspect() {
        let fitted = fit_within(Extent { w: 1600, h: 1200 }, 800);
        assert_eq!(fitted, Extent { w: 800, h: 600 });
    }

    #[test]
    fn an_unknown_bound_does_not_collapse_the_image() {
        // `size_allocate` can ask before the view has a content column; answering 0
        // there would set a zero width request and blank the picture permanently.
        let e = Extent { w: 400, h: 300 };
        assert_eq!(fit_within(e, 0), e);
        assert_eq!(fit_within(e, -5), e);
    }

    #[test]
    fn zooming_can_bring_an_image_under_the_clamp_that_fitted_before() {
        // TDD 2.21's zoom clause: the fit is computed against the SCALED size, so an
        // image that fitted at 100% is clamped once zoom takes it past the column.
        let bound = 900;
        let at_100 = zoomed_extent(800, 600, 1.0);
        assert_eq!(fit_within(at_100, bound), at_100);
        let at_200 = zoomed_extent(800, 600, 2.0);
        assert_eq!(fit_within(at_200, bound).w, bound);
    }

    #[test]
    fn a_raster_within_the_budget_is_rendered_at_the_size_zoom_asked_for() {
        let e = crate::renderer::image::cap_raster(Extent { w: 1200, h: 900 });
        assert_eq!(e, Extent { w: 1200, h: 900 }, "1.08 M pixels is well under");
    }

    #[test]
    fn an_over_budget_raster_is_reduced_but_never_below_something_drawable() {
        // `sdd/system-overview.svg` at the top of the ladder: 3000x3336 is 10.0 M pixels
        // against a 6 M budget, so it is rendered at ~0.77 of that — still far sharper
        // than the 1x raster it replaces, and ~24 MB rather than ~40 MB.
        let capped = crate::renderer::image::cap_raster(Extent { w: 3000, h: 3336 });
        let pixels = i64::from(capped.w) * i64::from(capped.h);
        assert!(
            pixels <= crate::limits::MAX_VECTOR_RASTER_PIXELS,
            "over budget: {capped:?} = {pixels}"
        );
        assert!(
            capped.w > 1000,
            "still an enlargement of the 1000px original: {capped:?}"
        );
        let ratio = f64::from(capped.h) / f64::from(capped.w);
        assert!(
            (ratio - 3336.0 / 3000.0).abs() < 0.01,
            "aspect drifted: {capped:?}"
        );
    }

    #[test]
    fn the_budget_is_never_exceeded_by_rounding() {
        // A floor rather than a round on the linear factor: at these shapes a rounded
        // factor lands a few thousand pixels over a budget that is then not a budget.
        for (w, h) in [(4000, 4000), (9000, 700), (703, 9001), (2449, 2449)] {
            let capped = crate::renderer::image::cap_raster(Extent { w, h });
            let pixels = i64::from(capped.w) * i64::from(capped.h);
            assert!(
                pixels <= crate::limits::MAX_VECTOR_RASTER_PIXELS,
                "{w}x{h} capped to {capped:?} = {pixels}"
            );
        }
    }

    #[test]
    fn at_the_default_zoom_the_scaled_fit_is_exactly_the_clamp() {
        // The default view must be byte-identical to the behaviour that shipped before
        // an image could overflow at all — only a reader who deliberately zoomed in can
        // produce an over-wide anchored child (ScrAP-23's shape).
        use crate::renderer::image::{fit_scaled, fit_within};
        for (w, h) in [(220, 140), (1600, 900), (100, 3000)] {
            let e = Extent { w, h };
            for bound in [300, 900, 1275, 4000] {
                for zoom in [0.5, 0.75, 1.0] {
                    assert_eq!(
                        fit_scaled(e, zoom, bound),
                        fit_within(e, bound),
                        "{e:?} bound {bound} zoom {zoom}"
                    );
                }
            }
        }
    }

    #[test]
    fn an_already_clamped_image_still_answers_the_zoom_control() {
        // THE defect the whole feature exists for. A 1600px image in a 900px column is
        // fitted to 900 at 100%; under a plain clamp it measures 900 at every zoom step
        // too, so the reader zooming in to read its lettering sees nothing happen.
        use crate::renderer::image::{fit_scaled, fit_within, zoomed_extent};
        let bound = 900;
        let at_one = zoomed_extent(1600, 400, 1.0);
        assert_eq!(
            fit_within(at_one, bound).w,
            bound,
            "clamped at 100%, as before"
        );
        let at_two = zoomed_extent(1600, 400, 2.0);
        assert_eq!(
            fit_within(at_two, bound).w,
            bound,
            "a plain clamp is the thing that does not move"
        );
        assert_eq!(
            fit_scaled(at_two, 2.0, bound).w,
            2 * bound,
            "the fit is scaled, so 200% is twice the fitted size"
        );
    }

    #[test]
    fn an_image_that_fits_at_every_zoom_is_never_touched_by_the_bound() {
        // A small image is below the column at every step, so the fit is a no-op and the
        // extent is the zoomed one exactly — no rounding introduced by fitting and
        // re-scaling a value that was already correct.
        use crate::renderer::image::{fit_scaled, zoomed_extent};
        for zoom in [1.25, 1.5, 2.0, 3.0] {
            let zoomed = zoomed_extent(240, 120, zoom);
            assert_eq!(fit_scaled(zoomed, zoom, 4000), zoomed, "at zoom {zoom}");
        }
    }

    #[test]
    fn the_scaled_fit_keeps_the_aspect_ratio_of_the_original() {
        use crate::renderer::image::{fit_scaled, zoomed_extent};
        for zoom in [1.25, 1.75, 2.5, 3.0] {
            let e = fit_scaled(zoomed_extent(1600, 900, zoom), zoom, 900);
            let ratio = f64::from(e.h) / f64::from(e.w);
            assert!((ratio - 900.0 / 1600.0).abs() < 0.01, "at {zoom}: {e:?}");
        }
    }

    #[test]
    fn an_unknown_column_does_not_collapse_a_zoomed_image() {
        // `size_allocate` can ask before the view has a content column. Answering 0 there
        // would set a zero width request and blank the picture permanently (ScrAP-32).
        use crate::renderer::image::{fit_scaled, zoomed_extent};
        let zoomed = zoomed_extent(1600, 900, 2.0);
        assert_eq!(fit_scaled(zoomed, 2.0, 0), zoomed);
        assert_eq!(fit_scaled(zoomed, 2.0, -12), zoomed);
    }

    #[test]
    fn a_wide_and_short_image_does_not_overflow_the_aspect_arithmetic() {
        // The product nat_h * w overflows i32 well inside the pixel cap; i64 math is
        // what keeps this a size rather than a negative number.
        let e = zoomed_extent(60_000, 40_000, 3.0);
        assert!(e.w > 0 && e.h > 0, "{e:?}");
        assert_eq!(fit_within(e, 900), Extent { w: 900, h: 600 });
    }
}
