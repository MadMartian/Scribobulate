//! PROBE — can an SVG be rasterised at an arbitrary size, and is the result a VECTOR
//! re-render or a resample?
//!
//! The question behind it: the preview's zoom must enlarge a document image, and an SVG
//! blown up from its natural-size raster is blurry exactly where it matters (the text
//! drawn inside a diagram). `GdkTexture::from_file` gives no size control, so the
//! candidate route is gdk-pixbuf's own scaled loaders — but "the loader accepted a size"
//! and "the loader re-rendered at that size" look identical from the outside, and only
//! the second is worth building on.
//!
//! **The discriminator is the anti-aliased fringe.** A vector re-render at 3x draws a
//! ~1px-wide soft edge along each shape's perimeter. A bilinear upscale of a 1x raster
//! stretches that 1px fringe to ~3px, so it carries roughly three times as many
//! intermediate-luminance pixels for the same geometry. Counting them separates the two
//! without a human looking at anything.
//!
//! Run: `cargo run` in this directory. Prints a verdict per question.

use gtk4::gdk;
use gtk4::gdk_pixbuf::{Colorspace, InterpType, Pixbuf, PixbufLoader};
use gtk4::prelude::*;
use std::io::Write;

/// An SVG that declares `width`/`height` AND a `viewBox`, with text in it — the shape
/// `sdd/system-overview.svg` has.
const SIZED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100" width="200" height="100">
<rect width="200" height="100" fill="#ffffff"/>
<text x="10" y="60" font-family="sans-serif" font-size="40" fill="#000000">Wg</text>
</svg>
"##;

/// The same drawing with NO width/height — only a viewBox. What a rasteriser does with
/// this is the case that decides whether "natural size" is even a well-defined term.
const VIEWBOX_ONLY: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
<rect width="200" height="100" fill="#ffffff"/>
<text x="10" y="60" font-family="sans-serif" font-size="40" fill="#000000">Wg</text>
</svg>
"##;

fn write_temp(name: &str, body: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(name);
    let mut f = std::fs::File::create(&path).expect("temp svg");
    f.write_all(body.as_bytes()).expect("write temp svg");
    path
}

/// The share of pixels sitting between clearly-ink and clearly-paper: the anti-aliased
/// fringe. Reported per thousand so the two numbers are comparable at a glance.
fn fringe_per_mille(pb: &Pixbuf) -> f64 {
    assert_eq!(pb.colorspace(), Colorspace::Rgb);
    let n_channels = pb.n_channels() as usize;
    let rowstride = pb.rowstride() as usize;
    let (w, h) = (pb.width() as usize, pb.height() as usize);
    let bytes = pb.read_pixel_bytes();
    let mut fringe = 0usize;
    for y in 0..h {
        for x in 0..w {
            let i = y * rowstride + x * n_channels;
            let lum = (u32::from(bytes[i]) + u32::from(bytes[i + 1]) + u32::from(bytes[i + 2])) / 3;
            if (52..=203).contains(&lum) {
                fringe += 1;
            }
        }
    }
    1000.0 * fringe as f64 / (w * h) as f64
}

fn main() {
    let sized = write_temp("probe-sized.svg", SIZED);
    let viewbox = write_temp("probe-viewbox-only.svg", VIEWBOX_ONLY);

    println!("== Q1/Q3: what the header probe reports ==");
    for (label, path) in [("width/height + viewBox", &sized), ("viewBox only", &viewbox)] {
        match Pixbuf::file_info(path) {
            Some((format, w, h)) => println!(
                "  {label}: format={} scalable={} natural={w}x{h}",
                format.name().unwrap_or_default(),
                format.is_scalable()
            ),
            None => println!("  {label}: NO FORMAT — the SVG loader is not installed"),
        }
    }

    println!("== Q1: what GdkTexture::from_file produces (no size control) ==");
    for (label, path) in [("width/height + viewBox", &sized), ("viewBox only", &viewbox)] {
        match gdk::Texture::from_file(&gtk4::gio::File::for_path(path)) {
            Ok(t) => println!("  {label}: {}x{}", t.width(), t.height()),
            Err(e) => println!("  {label}: FAILED ({e})"),
        }
    }

    println!("== Q2: from_file_at_scale — vector re-render, or a resample? ==");
    let natural = Pixbuf::from_file(&sized).expect("natural raster");
    let target_w = natural.width() * 3;
    let target_h = natural.height() * 3;
    let at_scale =
        Pixbuf::from_file_at_scale(&sized, target_w, target_h, true).expect("scaled raster");
    let upscaled = natural
        .scale_simple(target_w, target_h, InterpType::Bilinear)
        .expect("bilinear upscale");
    let f_scale = fringe_per_mille(&at_scale);
    let f_upscale = fringe_per_mille(&upscaled);
    println!(
        "  at_scale {}x{} fringe={f_scale:.1}‰ | bilinear upscale fringe={f_upscale:.1}‰",
        at_scale.width(),
        at_scale.height()
    );
    println!(
        "  VERDICT: {}",
        if f_upscale > f_scale * 1.5 {
            "from_file_at_scale RE-RENDERS (its fringe is far thinner than an upscale's)"
        } else {
            "from_file_at_scale looks like a RESAMPLE — do not build on it"
        }
    );

    println!("== Q5: the in-memory bytes path (remote images) ==");
    let loader = PixbufLoader::with_type("svg").expect("svg loader type");
    loader.set_size(target_w, target_h);
    loader.write(SIZED.as_bytes()).expect("write svg bytes");
    loader.close().expect("close loader");
    match loader.pixbuf() {
        Some(pb) => {
            let f_bytes = fringe_per_mille(&pb);
            println!(
                "  PixbufLoader::set_size -> {}x{} fringe={f_bytes:.1}‰",
                pb.width(),
                pb.height()
            );
            println!(
                "  VERDICT: {}",
                if pb.width() == target_w && f_upscale > f_bytes * 1.5 {
                    "set_size RE-RENDERS at the requested size"
                } else {
                    "set_size did NOT give a re-rendered raster at the requested size"
                }
            );
        }
        None => println!("  no pixbuf from the loader"),
    }

    println!("== Both axes vs one: does an exact-looking pair letterbox? ==");
    // MEASURED ON THREE HOSTS, AND THE ANSWER IS NO — recorded because this comment used
    // to claim the opposite. With `preserve_aspect_ratio` TRUE the loader FITS inside the
    // box and returns the aspect-correct size, so a square request against a 4:1 document
    // comes back 512x128 rather than padded: identical output from (512,128), (512,512)
    // and (512,-1) on Linux/librsvg 2.52.5, macOS/Homebrew 4.22.4 and Windows/gvsbuild.
    // The real letterbox hazard is `preserve_aspect_ratio = FALSE`, where librsvg still
    // honours the document's own `preserveAspectRatio` and paints the art inside the
    // canvas you asked for with transparent padding — that one is measured and stands.
    // One axis plus -1 is still what the application passes, for the smaller reason that
    // no rounding of ours then decides which axis binds.
    let wide = write_temp(
        "probe-wide.svg",
        &SIZED.replace("viewBox=\"0 0 200 100\" width=\"200\" height=\"100\"", "viewBox=\"0 0 800 200\" width=\"800\" height=\"200\""),
    );
    for (label, w, h) in [("both axes, exact", 512, 128), ("both axes, square", 512, 512), ("one axis (w,-1)", 512, -1)] {
        match Pixbuf::from_file_at_scale(&wide, w, h, true) {
            Ok(pb) => {
                // Count columns that carry any ink: letterboxing leaves transparent or
                // background-coloured margins the requested canvas does not fill.
                let (pw, ph) = (pb.width() as usize, pb.height() as usize);
                let nch = pb.n_channels() as usize;
                let rs = pb.rowstride() as usize;
                let bytes = pb.read_pixel_bytes();
                let mut inked = 0usize;
                for x in 0..pw {
                    let mut any = false;
                    for y in 0..ph {
                        let i = y * rs + x * nch;
                        let opaque = nch < 4 || bytes[i + 3] > 8;
                        if opaque && (u32::from(bytes[i]) + u32::from(bytes[i + 1]) + u32::from(bytes[i + 2])) < 720 {
                            any = true;
                            break;
                        }
                    }
                    if any { inked += 1; }
                }
                println!("  {label}: got {pw}x{ph}, {inked}/{pw} columns carry ink");
            }
            Err(e) => println!("  {label}: FAILED ({e})"),
        }
    }

    println!("== How long does a real diagram cost to re-render? ==");
    // Resolved against this crate's own directory, never the caller's: run from the repo
    // root the relative forms silently print "absent" and the whole timing section
    // disappears, which reads as "nothing to report" rather than "I looked in the wrong
    // place" (macOS seat, 2026-09-03).
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("probes/<crate>/ sits two levels under the repository root")
        .to_path_buf();
    for rel in ["sdd/system-overview.svg", "tests/fixtures/diagram.svg"] {
        let owned = repo.join(rel);
        let p = owned.as_path();
        let path = p.display();
        if !p.exists() {
            println!("  {path}: absent");
            continue;
        }
        let t0 = std::time::Instant::now();
        let info = Pixbuf::file_info(p);
        let probe_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let (nw, nh) = info.map(|(_, w, h)| (w, h)).unwrap_or((0, 0));
        let t1 = std::time::Instant::now();
        let natural = Pixbuf::from_file(p).ok();
        let nat_ms = t1.elapsed().as_secs_f64() * 1000.0;
        let t2 = std::time::Instant::now();
        let scaled = Pixbuf::from_file_at_scale(p, nw * 3, -1, true).ok();
        let scaled_ms = t2.elapsed().as_secs_f64() * 1000.0;
        println!(
            "  {path}: natural {nw}x{nh} | file_info {probe_ms:.1} ms | from_file {nat_ms:.1} ms | at_scale x3 -> {} {scaled_ms:.1} ms",
            scaled.map(|p| format!("{}x{}", p.width(), p.height())).unwrap_or_else(|| "FAILED".into())
        );
        let _ = natural;
    }

    println!("== Is the SVG loader even installed? (the per-platform question) ==");
    let svg = Pixbuf::formats()
        .into_iter()
        .find(|f| f.mime_types().iter().any(|m| m == "image/svg+xml"));
    match svg {
        Some(f) => println!(
            "  present: {} (disabled={})",
            f.name().unwrap_or_default(),
            f.is_disabled()
        ),
        None => println!("  ABSENT — no loader claims image/svg+xml on this host"),
    }

    println!("== Q3 control: a RASTER source must report scalable=false ==");
    let png = std::env::temp_dir().join("probe-raster.png");
    Pixbuf::new(Colorspace::Rgb, false, 8, 64, 64)
        .expect("blank pixbuf")
        .savev(&png, "png", &[])
        .expect("save png");
    match Pixbuf::file_info(&png) {
        Some((format, w, h)) => println!(
            "  png: format={} scalable={} natural={w}x{h}",
            format.name().unwrap_or_default(),
            format.is_scalable()
        ),
        None => println!("  png: no format"),
    }
}
