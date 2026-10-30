//! SVG in an encoding gdk-pixbuf's format sniff cannot see: UTF-16 text, or a gzip
//! (`.svgz`) layer. librsvg reads both, but gdk-pixbuf picks a loader by matching the
//! first bytes against each module's signature table, and the SVG module's signatures
//! (`<svg`, `<!DOCTYPE svg`) are ASCII. MEASURED on gdk-pixbuf 2.42.8 / librsvg 2.52.5:
//! an untyped loader fed a 40×40 SVG as UTF-16 (either byte order, with or without a
//! BOM) or gzip never picks a module ("Unrecognized image file format") and never emits
//! `size-prepared`; the same bytes fed to a loader created for the `svg` type emit
//! `size-prepared` with 40×40, honour the `set_size(0, 0)` sentinel, and decode. So it
//! is the sniff that fails, never librsvg.
//!
//! [`loader_input`] decides how a screened buffer is handed to gdk-pixbuf, and is the
//! only way to get a [`LoaderInput`], so every loader call in `imagedecode` takes one:
//! - **gzip** is inflated HERE, never by librsvg, to at most
//!   [`crate::limits::MAX_LOCAL_IMAGE_BYTES`] — an `.svgz` may expand to no more than an
//!   uncompressed SVG may weigh by default (a decode does not know whether its bytes came
//!   from a file, so the configured file limit is not consulted). Nothing this project
//!   sets bounds librsvg's own inflate, and on a macOS bundle the XInclude screen (whose
//!   inflate is bounded) stands down, so handing librsvg the compressed bytes would put
//!   nothing in front of a decompression bomb. The inflated text is refused if it is gzip once more (librsvg would inflate that layer
//!   itself), and is screened again before any loader sees it. That second screen is
//!   redundant today — the screen of the compressed bytes already inflated and read the
//!   same text, and deleting it leaves every test green — and is kept so that what
//!   librsvg reads is what was screened, without depending on two inflates agreeing.
//! - **UTF-16**, as libxml2 detects it (a BOM, or `<?` in either byte order: XML 1.0
//!   Appendix F), goes to the `svg` loader as it is.
//! - Everything else goes to an ordinary sniffing loader, exactly as before.
//!
//! Content routed here is only ever treated as SVG: a gzip-wrapped PNG is not a format
//! anything claims, and stays refused.

use super::screen::{self, Screened};
#[cfg(not(target_os = "linux"))]
use gtk::gdk_pixbuf::prelude::PixbufLoaderExt;
use gtk::gdk_pixbuf::{Pixbuf, PixbufLoader};
use std::borrow::Cow;

const GZIP_MAGIC: &[u8] = &[0x1f, 0x8b];

/// The first bytes by which libxml2 recognises UTF-16 (`xmlDetectCharEncoding`): a BOM
/// in either byte order, or `<?` written in either byte order (XML 1.0 Appendix F).
/// UTF-16 with neither is not detectable by the parser, so librsvg could not read it
/// either and it is left to the ordinary route, which refuses it.
const UTF16_SIGNATURES: &[&[u8]] = &[
    &[0xFF, 0xFE],
    &[0xFE, 0xFF],
    &[0x3C, 0x00, 0x3F, 0x00],
    &[0x00, 0x3C, 0x00, 0x3F],
];

/// UTF-32LE's BOM, which begins with UTF-16LE's. libxml2 reads it as UCS-4, so it is
/// not UTF-16 and is not routed here.
const UTF32LE_BOM: &[u8] = &[0xFF, 0xFE, 0x00, 0x00];

/// The gdk-pixbuf module name librsvg's loader registers under, on every platform.
const SVG_MODULE: &str = "svg";

/// Screened bytes as a loader must receive them, and which loader that is. Only
/// [`loader_input`] makes one, so holding one means the bytes in it were screened.
pub(super) struct LoaderInput<'a> {
    bytes: Cow<'a, [u8]>,
    svg_only: bool,
}

impl LoaderInput<'_> {
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether these bytes reach gdk-pixbuf only through the `svg` module. Nothing that
    /// sniffs recognises them — GTK's `Texture::from_bytes` and gdk-pixbuf's stream
    /// decoders included — so they must be decoded by [`decode_svg`].
    pub(super) fn svg_only(&self) -> bool {
        self.svg_only
    }

    /// A fresh loader for these bytes: the `svg` module's for [`Self::svg_only`] input,
    /// a sniffing one otherwise. `None` only where no `svg` module is installed, which
    /// refuses the image exactly as an unrecognised format is refused.
    pub(super) fn loader(&self) -> Option<PixbufLoader> {
        if self.svg_only {
            #[expect(clippy::disallowed_methods)] // this module IS the sanctioned route
            return PixbufLoader::with_type(SVG_MODULE).ok();
        }
        #[expect(clippy::disallowed_methods)] // this module IS the sanctioned route
        Some(PixbufLoader::new())
    }
}

/// How `screened` is handed to gdk-pixbuf (see the module doc). `None` when a gzip
/// layer does not inflate within the bound, inflates to gzip again, or inflates to text
/// the screen refuses. `origin` names the image for the log; a header probe passes
/// `None` and logs at debug only, because the decode that follows reports it.
pub(super) fn loader_input<'a>(
    screened: Screened<'a>,
    origin: Option<&str>,
) -> Option<LoaderInput<'a>> {
    let bytes = screened.bytes();
    if !bytes.starts_with(GZIP_MAGIC) {
        return Some(LoaderInput {
            bytes: Cow::Borrowed(bytes),
            svg_only: is_utf16(bytes) || (cfg!(target_os = "linux") && is_plain_svg(bytes)),
        });
    }
    let inflated = match inflate(bytes) {
        Ok(inflated) => inflated,
        Err(why) => {
            match origin {
                Some(origin) => log::warn!("image {origin} not loaded: its gzip layer {why}"),
                None => log::debug!("image header probe refused: its gzip layer {why}"),
            }
            return None;
        }
    };
    match origin {
        Some(origin) => screen::screen(&inflated, origin)?,
        None => screen::screen_quietly(&inflated)?,
    };
    Some(LoaderInput {
        bytes: Cow::Owned(inflated),
        svg_only: true,
    })
}

/// Whether `bytes` are SVG by content, as gdk-pixbuf 2.42 decided it: the shared MIME
/// database's guess from the bytes alone. On Linux every SVG goes to [`decode_svg`], so
/// gdk-pixbuf never decodes one (`super::rsvg`'s module doc says why).
fn is_plain_svg(bytes: &[u8]) -> bool {
    gtk::gio::content_type_guess(None::<&str>, bytes).0 == "image/svg+xml"
}

fn is_utf16(bytes: &[u8]) -> bool {
    !bytes.starts_with(UTF32LE_BOM) && UTF16_SIGNATURES.iter().any(|s| bytes.starts_with(s))
}

/// Why a gzip layer was not inflated, worded to follow "its gzip layer".
#[derive(Debug, PartialEq, Eq)]
enum Inflate {
    Undecodable,
    TooLarge,
    Nested,
}

impl std::fmt::Display for Inflate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let why = match self {
            Self::Undecodable => "does not decode",
            Self::TooLarge => "inflates past the image size limit",
            Self::Nested => "holds another gzip layer",
        };
        f.write_str(why)
    }
}

/// Inflate the gzip member `bytes` starts with, reading at most one byte past
/// [`crate::limits::MAX_LOCAL_IMAGE_BYTES`] of output whatever the member claims, so a
/// bomb costs the bound and no more. Only the first member is read; librsvg, which
/// receives the output instead of the gzip, never sees anything after it.
fn inflate(bytes: &[u8]) -> Result<Vec<u8>, Inflate> {
    use std::io::Read;

    let cap = crate::limits::MAX_LOCAL_IMAGE_BYTES;
    let mut out = Vec::new();
    flate2::bufread::GzDecoder::new(bytes)
        .take(cap + 1)
        .read_to_end(&mut out)
        .map_err(|_| Inflate::Undecodable)?;
    if out.len() as u64 > cap {
        return Err(Inflate::TooLarge);
    }
    if out.starts_with(GZIP_MAGIC) {
        return Err(Inflate::Nested);
    }
    Ok(out)
}

/// Decode [`LoaderInput::svg_only`] input with the `svg` module: at its natural size,
/// or `target_w` pixels wide with its aspect ratio kept and the height rounded as
/// gdk-pixbuf's own `*_at_scale` decoders round it (truncated, at least 1), so a zoomed
/// render is the size the sniffing route gives an ordinary SVG.
///
/// Refuses in `size-prepared`, with the `set_size(0, 0)` sentinel (GTK4Rs/AP-311), an
/// output over [`crate::limits::MAX_IMAGE_PIXELS`]: the caller has already probed and
/// capped the declared size, and this holds the cap on the decode itself rather than
/// trusting that a second parse of the same bytes declares the same size.
#[cfg(target_os = "linux")]
pub(super) fn decode_svg(input: &LoaderInput<'_>, target_w: Option<i32>) -> Result<Pixbuf, String> {
    if !input.svg_only {
        return Err("it is not SVG".to_owned());
    }
    let document = super::rsvg::Document::parse(input.bytes())?;
    let (w, h) = document.size();
    let (out_w, out_h) = output_size(w, h, target_w);
    if out_w <= 0 || out_h <= 0 {
        return Err(format!("it has no size ({w}×{h})"));
    }
    if !crate::limits::image_pixels_within_cap(out_w, out_h) {
        return Err(format!(
            "it renders past the {}-pixel cap",
            crate::limits::MAX_IMAGE_PIXELS
        ));
    }
    document.render(out_w, out_h)
}

/// The natural size of [`LoaderInput::svg_only`] input, read by librsvg on Linux.
#[cfg(target_os = "linux")]
pub(super) fn svg_size(input: &LoaderInput<'_>) -> Option<(i32, i32)> {
    let size = super::rsvg::Document::parse(input.bytes()).ok()?.size();
    (size.0 > 0 && size.1 > 0).then_some(size)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn decode_svg(input: &LoaderInput<'_>, target_w: Option<i32>) -> Result<Pixbuf, String> {
    use std::cell::Cell;
    use std::rc::Rc;

    let loader = input
        .loader()
        .filter(|_| input.svg_only)
        .ok_or("no SVG loader is installed")?;
    let refused = Rc::new(Cell::new(false));
    loader.connect_size_prepared({
        let refused = Rc::clone(&refused);
        move |l, w, h| {
            let (out_w, out_h) = output_size(w, h, target_w);
            if refused.get() || !crate::limits::image_pixels_within_cap(out_w, out_h) {
                refused.set(true);
                l.set_size(0, 0);
            } else if (out_w, out_h) != (w, h) {
                l.set_size(out_w, out_h);
            }
        }
    });
    let written = loader.write(input.bytes());
    let closed = loader.close();
    if refused.get() {
        return Err(format!(
            "it renders past the {}-pixel cap",
            crate::limits::MAX_IMAGE_PIXELS
        ));
    }
    written.and(closed).map_err(|err| err.to_string())?;
    loader
        .pixbuf()
        .ok_or_else(|| "the SVG loader produced no image".to_owned())
}

/// The size a `w`×`h` SVG is rendered at: natural, or `target_w` wide with the height
/// scaled as gdk-pixbuf's `at_scale_size_prepared_cb` scales it.
fn output_size(w: i32, h: i32, target_w: Option<i32>) -> (i32, i32) {
    match target_w {
        Some(target_w) if w > 0 && h > 0 => {
            let scaled = (f64::from(h) * f64::from(target_w) / f64::from(w)) as i32;
            (target_w.max(1), scaled.max(1))
        }
        _ => (w, h),
    }
}

#[cfg(test)]
mod tests {
    use crate::imagedecode::{
        decode, decode_pixbuf, probe_dimensions, probe_vector_dimensions, rasterize_vector_bytes,
    };
    use gtk::prelude::TextureExt;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/svgencodings")
                .join(name),
        )
        .expect("svgencodings fixture")
    }

    const ENCODED: [&str; 2] = ["green-utf16le.svg", "green.svgz"];

    /// The control: the same 40×40 square as plain UTF-8 passes every entry point, so a
    /// failure below is about the encoding and not about the square.
    #[test]
    fn the_plain_square_is_read_and_decoded() {
        let bytes = fixture("green.svg");
        assert_eq!(probe_dimensions(&bytes), Some((40, 40)));
        assert_eq!(probe_vector_dimensions(&bytes), Some((40, 40)));
        let tex = decode(&bytes, "test:svgenc:green.svg")
            .expect("decodes")
            .texture;
        assert_eq!((tex.width(), tex.height()), (40, 40));
    }

    /// TDD 2.23j: a benign UTF-16 or gzip SVG has its size read, by both probes.
    #[test]
    fn an_encoded_square_has_its_size_read() {
        for name in ENCODED {
            let bytes = fixture(name);
            assert_eq!(probe_dimensions(&bytes), Some((40, 40)), "{name}");
            assert_eq!(probe_vector_dimensions(&bytes), Some((40, 40)), "{name}");
        }
    }

    /// TDD 2.23j: … and decodes through every decode entry point, at its natural size and
    /// re-rendered at a zoomed width.
    #[test]
    fn an_encoded_square_decodes_through_every_entry_point() {
        for name in ENCODED {
            let bytes = fixture(name);
            let origin = format!("test:svgenc:{name}");
            let tex = decode(&bytes, &origin).expect(name).texture;
            assert_eq!((tex.width(), tex.height()), (40, 40), "{name}");
            let pixbuf = decode_pixbuf(&bytes, &origin).expect(name);
            assert_eq!((pixbuf.width(), pixbuf.height()), (40, 40), "{name}");
            let zoomed = rasterize_vector_bytes(&bytes, 80, &origin).expect(name);
            assert_eq!((zoomed.width(), zoomed.height()), (80, 80), "{name}");
        }
    }

    use super::{decode_svg, inflate, is_utf16, loader_input, output_size, Inflate};

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(bytes).expect("in memory");
        encoder.finish().expect("in memory")
    }

    fn utf16le(text: &str) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    }

    /// The square's PIXELS, not only its size: a decode that produced a blank 40×40
    /// would pass every size check above.
    #[test]
    fn an_encoded_square_is_green() {
        for name in ENCODED {
            let pixbuf =
                decode_pixbuf(&fixture(name), &format!("test:svgenc:px:{name}")).expect(name);
            let pixels = pixbuf.read_pixel_bytes();
            let at = 20 * pixbuf.rowstride() as usize + 20 * pixbuf.n_channels() as usize;
            assert_eq!(&pixels[at..at + 3], &[0x00, 0xA0, 0x00], "{name}");
        }
    }

    /// The pixel cap holds on this route: a UTF-16 SVG and an `.svgz` declaring
    /// 100000×100000 have that size read, and are refused as over the cap — not as
    /// unreadable, and not decoded.
    #[test]
    fn an_encoded_svg_over_the_pixel_cap_is_refused() {
        let plain = String::from_utf8(fixture("green.svg")).expect("UTF-8");
        let huge = plain.replacen(
            r#"width="40" height="40""#,
            r#"width="100000" height="100000""#,
            1,
        );
        for (name, bytes) in [("utf16", utf16le(&huge)), ("svgz", gzip(huge.as_bytes()))] {
            assert_eq!(probe_dimensions(&bytes), Some((100_000, 100_000)), "{name}");
            let cap = crate::testlog::capture();
            assert!(decode(&bytes, &format!("test:svgenc:huge:{name}")).is_none());
            assert!(
                cap.logged(log::Level::Warn, "100000×100000 pixels"),
                "{name}: refused as over the cap: {:?}",
                cap.records()
            );
        }
    }

    /// The cap holds INSIDE the `svg`-module decode, not only in front of it: asked for
    /// a raster past the cap (9000 wide is 81 M pixels for a square), it refuses in
    /// `size-prepared` and says why, rather than allocating the raster.
    #[test]
    fn the_svg_module_decode_refuses_an_output_over_the_pixel_cap() {
        for name in ENCODED {
            let bytes = fixture(name);
            let screened = crate::imagedecode::screen::screen_quietly(&bytes).expect(name);
            let input = loader_input(screened, None).expect(name);
            assert!(input.svg_only(), "{name}");
            let err = decode_svg(&input, Some(9000)).expect_err(name);
            assert!(err.contains("pixel cap"), "{name}: {err}");
            assert!(
                decode_svg(&input, Some(80)).is_ok(),
                "control: {name} at 80 wide"
            );
        }
    }

    /// A zoomed render on this route is the size the sniffing route gives the same
    /// picture, judged by that route itself (an odd 7×3 so the height truncates).
    #[test]
    fn a_zoomed_render_is_the_size_the_sniffing_route_gives() {
        let plain = r#"<svg xmlns="http://www.w3.org/2000/svg" width="7" height="3"/>"#;
        let encoded = utf16le(plain);
        for target in [1, 10, 11, 13, 100, 333] {
            let sniffed = rasterize_vector_bytes(plain.as_bytes(), target, "test:svgenc:7x3")
                .expect("the sniffing route renders");
            let routed = rasterize_vector_bytes(&encoded, target, "test:svgenc:7x3:utf16")
                .expect("the svg route renders");
            assert_eq!(
                (routed.width(), routed.height()),
                (sniffed.width(), sniffed.height()),
                "at {target} wide"
            );
            assert_eq!(
                output_size(7, 3, Some(target)),
                (routed.width(), routed.height())
            );
        }
        assert_eq!(output_size(7, 3, None), (7, 3));
    }

    /// A gzip layer is inflated to the bound and no further: one byte past it is
    /// refused, the bound itself is not.
    #[test]
    fn a_gzip_layer_inflating_past_the_bound_is_refused() {
        let cap = crate::limits::MAX_LOCAL_IMAGE_BYTES as usize;
        assert_eq!(inflate(&gzip(&vec![b' '; cap + 1])), Err(Inflate::TooLarge));
        assert_eq!(
            inflate(&gzip(&vec![b' '; cap])).map(|out| out.len()),
            Ok(cap)
        );
        assert_eq!(inflate(&[0x1f, 0x8b, 0, 0]), Err(Inflate::Undecodable));
    }

    /// gzip inside gzip is refused here: handed on, librsvg would inflate the inner
    /// layer itself, with no bound. The screen allows several gzip levels, so this
    /// refusal is this module's alone.
    #[test]
    fn a_gzip_layer_holding_another_is_refused() {
        let twice = gzip(&fixture("green.svgz"));
        assert_eq!(inflate(&twice), Err(Inflate::Nested));
        assert_eq!(probe_dimensions(&twice), None);
        assert!(decode(&twice, "test:svgenc:twice.svgz").is_none());
    }

    /// Content routed to the `svg` module is only ever SVG: a gzip-wrapped PNG, which no
    /// format claims, stays refused.
    #[test]
    fn a_gzip_wrapped_raster_is_not_decoded() {
        let png = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/logo.png"),
        )
        .expect("logo.png");
        assert!(decode(&gzip(&png), "test:svgenc:logo.png.gz").is_none());
    }

    #[test]
    fn utf16_is_recognised_as_libxml2_recognises_it() {
        assert!(is_utf16(b"\xFF\xFE<\x00"));
        assert!(is_utf16(b"\xFE\xFF\x00<"));
        assert!(is_utf16(b"<\x00?\x00x\x00"));
        assert!(is_utf16(b"\x00<\x00?\x00x"));
        assert!(!is_utf16(b"\xFF\xFE\x00\x00<\x00\x00\x00"), "UTF-32LE");
        assert!(!is_utf16(b"<svg"));
        assert!(
            !is_utf16(b"<\x00s\x00"),
            "no BOM and no <? is not detectable"
        );
        assert!(!is_utf16(b""));
    }

    /// TDD 2.23c: the hostile UTF-16 and gzip fixtures are still refused by every entry
    /// point now that their encodings have a route to librsvg.
    #[test]
    fn the_hostile_encoded_fixtures_are_still_refused() {
        if !crate::imagedecode::xinclude::enforced() {
            println!("SKIPPED [2.23c]: this platform does not screen SVG XInclude");
            return;
        }
        for name in ["utf16.svg", "compressed.svgz"] {
            let bytes = std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/xinclude")
                    .join(name),
            )
            .expect(name);
            let origin = format!("test:svgenc:hostile:{name}");
            assert_eq!(probe_dimensions(&bytes), None, "{name}");
            assert!(decode(&bytes, &origin).is_none(), "{name}");
            assert!(crate::imagedecode::svg_refused(&origin), "{name}");
            assert!(decode_pixbuf(&bytes, &origin).is_none(), "{name}");
            assert!(
                rasterize_vector_bytes(&bytes, 80, &origin).is_none(),
                "{name}"
            );
            crate::imagedecode::forget_for_test(&origin);
        }
    }
}
