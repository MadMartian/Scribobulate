//! Dimension-only probing: learn an image's declared size without decoding a pixel
//! buffer, so the pixel cap can refuse a decompression bomb before any canvas is
//! allocated.

/// How much of a file the dimension probe feeds the GTK loader before giving up.
/// Every format GTK's own loaders recognise carries its dimensions in the first few
/// dozen bytes; this is generous enough for a JPEG whose SOF marker sits behind a
/// large EXIF block, and small enough that the probe is not a second full read.
const PROBE_CHUNK: usize = 64 * 1024;

use super::svgencoding::{loader_input, LoaderInput};

/// The declared pixel dimensions of `bytes`, choosing the probe that can actually
/// answer for this content.
///
/// **richimg-recognised bytes (WebP/GIF/APNG) go to `richimg::probe`, never to the GTK
/// loader below.** The GTK-loader probe depends on a HOST-INSTALLED gdk-pixbuf loader
/// for the format — exactly the dependency this project's own memory gates measure as
/// inconsistently present for WebP (`memgate::gtk`'s `SKIPPED […]: this host has no
/// decoder for the animated WebP fixture`) — while `richimg` is pure Rust and decodes
/// identically on every host. Probing through the GTK loader for a format `richimg`
/// owns would therefore refuse-as-unreadable on a host that cannot decode it via
/// gdk-pixbuf, even though `richimg` decodes it perfectly well; probing through
/// `richimg` first makes the admission check as host-independent as the decode it
/// gates.
///
/// `richimg::probe` does not itself enforce a pixel cap (see its own doc comment), so
/// which `Limits` value is passed here is immaterial — only `Info::width`/`Info::height`
/// are read.
pub(crate) fn probe_dimensions(bytes: &[u8]) -> Option<(i32, i32)> {
    if richimg::sniff(bytes).is_some() {
        let limits = richimg::Limits::default();
        return richimg::probe(bytes, &limits)
            .ok()
            .map(|info| (info.width as i32, info.height as i32));
    }
    probe_pixel_size(bytes)
}

/// The natural pixel dimensions a GTK-decodable image's header declares, without
/// decoding it. Moved here from `sprite.rs` — the same probe now serves the
/// document/remote-image path as well as the theme-sprite path, so the two cannot
/// disagree about what an image's header says (GTK4Rs/AP-311).
///
/// Feeds a `GdkPixbufLoader` in chunks only until `size-prepared` fires, then **aborts
/// the load from inside that handler with `set_size(0, 0)`**. Returns `None` for bytes
/// no installed loader recognises.
///
/// **`0, 0` is load-bearing and no other value substitutes for it.** A non-zero
/// `set_size` asks the loader to *scale*, which several image modules — the PNG one
/// among them — honour only after allocating the pixel buffer at the file's declared
/// size; zero is the sentinel `gdk_pixbuf_get_file_info` itself uses to make the
/// module bail out before allocating anything. MEASURED on this project's Linux
/// reference host against a 20000×20000 PNG: peak RSS 1163 MB with `set_size(1, 1)`
/// and 20 MB with `set_size(0, 0)`, against an 18 MB do-nothing baseline. The
/// intuitive spelling reports the right dimensions, refuses the image, passes the
/// test — and allocates the bomb anyway, which is the whole thing this gate exists to
/// prevent.
///
/// Deliberately a loader rather than `gdk_pixbuf_get_file_info`, which is equivalent
/// (18 MB, same measurement) but takes a **path**: a caller may hold only bytes
/// (a compiled-in sprite, a remote fetch), and re-opening a file the caller has
/// already read and validated would reintroduce a check-then-use seam.
///
/// Screened first ([`super::screen`]): bytes the XInclude screen refuses answer `None`
/// without reaching the loader.
pub(crate) fn probe_pixel_size(raw: &[u8]) -> Option<(i32, i32)> {
    let input = loader_input(super::screen::screen_quietly(raw)?, None)?;
    probe_input_size(&input)
}

/// [`probe_pixel_size`] for bytes the caller has already screened and prepared, so a
/// decode does not screen (or inflate) the same buffer twice.
pub(super) fn probe_input_size(input: &LoaderInput<'_>) -> Option<(i32, i32)> {
    probe_header(input).size
}

#[cfg(test)]
thread_local! {
    /// How many times [`probe_header`] has fed bytes to a `PixbufLoader` on this thread.
    ///
    /// **Test-only, and it exists because the thing worth asserting is not observable
    /// from a return value.** `probe_vector_dimensions` answers `None` for a
    /// richimg-owned format whether or not its `richimg::sniff` guard is present — an
    /// animated WebP is not scalable, so the loader would refuse it one step later and
    /// the test would stay green with the guard deleted. What the guard actually buys is
    /// that those bytes never REACH the loader, because reaching it is what costs
    /// ~2.3 MB per call (ScrAP-351). So the test counts arrivals instead of reading
    /// answers. Thread-local, so parallel tests do not see each other's counts.
    static HEADER_PROBES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn header_probes() -> usize {
    HEADER_PROBES.with(|n| n.get())
}

/// What a header probe learned: the declared size `size-prepared` reported, and the
/// format gdk-pixbuf identified the CONTENT as.
struct Header {
    size: Option<(i32, i32)>,
    /// Whether that format is scalable (a vector image).
    scalable: bool,
}

/// **The one copy of the `size-prepared` → `set_size(0, 0)` bomb guard**, shared by
/// [`probe_pixel_size`] and [`probe_vector`].
///
/// It is factored out rather than written twice because the two copies it replaces had
/// already drifted by a line, and this is not a guard that tolerates drift: it is the
/// difference between a 20 MB probe and a measured 1163 MB allocation (GTK4Rs/AP-311,
/// GTK4Rs/AP-311). A duplicated guard whose doc comment claims to "share" the original is
/// worse than an obvious copy — the reader is told the correction they make here will
/// reach both call sites, and it will not.
///
/// Feeds `bytes` in chunks only until the size is known, then stops: the loader is
/// deliberately left partway through the file, so `close` reports a premature end of
/// file and its error is discarded. Closing anyway is required — an unclosed loader
/// warns at finalize. The loader is the one [`LoaderInput::loader`] picks, so a UTF-16
/// or gzip SVG, which no sniff recognises, is probed by the `svg` module.
///
/// **`format` is read AFTER the close, never before.** gdk-pixbuf picks a module only
/// once it has buffered enough bytes to sniff, or at `close`, so for a file shorter
/// than that buffer the format is not known until the close has run. MEASURED on
/// gdk-pixbuf 2.42.8: a 115-byte SVG reports no format before `close` and `svg` after
/// it, so reading it first answered "not scalable" for every small SVG and none of them
/// was re-rendered at zoom.
fn probe_header(input: &LoaderInput<'_>) -> Header {
    use gtk::gdk_pixbuf::prelude::PixbufLoaderExt;
    use std::cell::Cell;
    use std::rc::Rc;

    #[cfg(test)]
    HEADER_PROBES.with(|n| n.set(n.get() + 1));

    // On Linux an SVG never reaches gdk-pixbuf (`super::rsvg`'s module doc).
    #[cfg(target_os = "linux")]
    if input.svg_only() {
        return Header {
            size: super::svgencoding::svg_size(input),
            scalable: true,
        };
    }

    let bytes = input.bytes();
    let seen: Rc<Cell<Option<(i32, i32)>>> = Rc::new(Cell::new(None));
    let Some(loader) = input.loader() else {
        return Header {
            size: None,
            scalable: false,
        };
    };
    loader.connect_size_prepared({
        let seen = Rc::clone(&seen);
        move |l, w, h| {
            seen.set(Some((w, h)));
            l.set_size(0, 0);
        }
    });
    for chunk in bytes.chunks(PROBE_CHUNK) {
        if seen.get().is_some() {
            break;
        }
        if loader.write(chunk).is_err() {
            break;
        }
    }
    let _ = loader.close();
    let scalable = loader.format().is_some_and(|f| f.is_scalable());
    Header {
        size: seen.get(),
        scalable,
    }
}

/// Whether admitted local `bytes` are a genuine VECTOR (scalable) image, decided from
/// CONTENT alone — never from the caller's file name (Finding 1 / TDD 2.23b: the vector
/// path used to be chosen by `path.extension()`, ahead of admission, so a FIFO named
/// `.svg` blocked the main thread forever and a `richimg`-owned format wearing a `.svg`
/// name reached the gdk-pixbuf loader chain this project's decode choke point exists to
/// make unreachable).
///
/// **Callers MUST check [`richimg::sniff`] first and never call this on richimg-owned
/// bytes.** A WebP/GIF/APNG file is never scalable, but finding that out HERE would mean
/// feeding it to a raw `PixbufLoader` — reaching the same leaking header-probe path this
/// project measured at ~2.3 MB/call on an animated WebP (`Pixbuf::file_info`; see this
/// crate's module doc comment, ScrAP-351) — so richimg-owned content must stay
/// unreachable here whatever it would have answered. [`super::probe_dimensions`]'s own
/// doc comment states the same precondition for the same reason.
///
/// Returns the declared pixel size only when gdk-pixbuf's OWN loader identifies the
/// content itself — via its installed signature table, not this file's name — as a
/// [`gtk::gdk_pixbuf::PixbufFormat::is_scalable`] format. This is the "content sniffing"
/// half of the admit-then-sniff-then-split fix: gdk-pixbuf already sniffs every format it
/// loads by content (a `.png`-named WebP still loads as WebP, GTK4Rs/AP-66), so reusing
/// its own format detection is a smaller, more robust discriminator than hand-rolling an
/// `<svg`/XML prefix scan, and it is exercised by every format this project might ever
/// see wearing a `.svg` name — not just SVG-shaped text.
///
/// Shares [`probe_pixel_size`]'s `size-prepared` → `set_size(0, 0)` sentinel
/// (GTK4Rs/AP-311, GTK4Rs/AP-311) — genuinely shares it, through the single
/// [`probe_header`] both call: sniffing the format can still run a module far enough to
/// reach that signal, and this function must never allocate a decompression bomb just to
/// answer "is this scalable".
pub(crate) fn probe_vector_dimensions(bytes: &[u8]) -> Option<(i32, i32)> {
    if richimg::sniff(bytes).is_some() {
        return None;
    }
    probe_vector(bytes)
}

/// The gdk-pixbuf-identified format and declared size of `bytes`, or `None` if no
/// installed loader recognises it or the recognised format is not scalable. Private:
/// [`probe_vector_dimensions`] is the only sanctioned entry, because it is the one that
/// enforces the richimg-first precondition above.
fn probe_vector(bytes: &[u8]) -> Option<(i32, i32)> {
    let input = loader_input(super::screen::screen_quietly(bytes)?, None)?;
    let Header { size, scalable } = probe_header(&input);
    let (w, h) = size?;
    if !scalable {
        return None;
    }
    Some((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_pixel_size_reads_the_header_and_decodes_nothing() {
        // A minimal valid 1x1 white PNG.
        const PNG: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0xDC, 0xCC, 0x59,
            0xE7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        assert_eq!(probe_pixel_size(PNG), Some((1, 1)));
        assert_eq!(probe_pixel_size(b"not an image at all"), None);
        assert_eq!(probe_pixel_size(&[]), None);
    }

    /// A WebP's dimensions must be readable even when nothing is registered to feed
    /// `probe_pixel_size` — the whole reason `probe_dimensions` checks `richimg::sniff`
    /// first. `anim.webp` is a real fixture (480×270).
    #[test]
    fn probe_dimensions_reads_webp_via_richimg_not_the_gtk_loader() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/anim.webp"
        ));
        assert_eq!(probe_dimensions(bytes), Some((480, 270)));
    }

    #[test]
    fn probe_dimensions_falls_back_to_the_gtk_loader_for_a_still_png() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/logo.png"
        ));
        assert_eq!(probe_dimensions(bytes), Some((220, 130)));
    }

    #[test]
    fn probe_dimensions_is_none_for_garbage() {
        assert_eq!(probe_dimensions(b"not an image at all"), None);
    }

    /// TDD 13.11 / Finding 1: a real SVG is recognised as scalable from its CONTENT,
    /// with the correct declared size — the positive control for the vector branch's
    /// admit-then-sniff-then-split rewrite. Regresses if the SVG loader is absent from
    /// the host, which every CI image and dev box for this project carries.
    #[test]
    fn probe_vector_dimensions_recognises_a_real_svg_by_content() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/sdd/system-overview.svg"
        ));
        assert_eq!(probe_vector_dimensions(bytes), Some((1000, 1190)));
    }

    /// Finding 1: a richimg-owned format is never scalable, decided WITHOUT ever
    /// handing these bytes to a gdk-pixbuf loader (`richimg::sniff` short-circuits
    /// first) — the leak hazard this function's own doc comment names.
    ///
    /// **Asserts on ARRIVAL at the loader, not on the answer, and that is the whole
    /// point of the test.** An earlier version compared the return value to `None` and
    /// passed with the `richimg::sniff` guard deleted: an animated WebP is not scalable,
    /// so `probe_vector` refuses it one step later and answers `None` too. The guard's
    /// value is that the bytes never reach the loader at all — reaching it is what costs
    /// ~2.3 MB a call — so that is what [`HEADER_PROBES`] observes. The SVG control runs
    /// first and must ADVANCE the counter, or a dead instrument would make the real
    /// assertion below vacuous.
    #[test]
    fn probe_vector_dimensions_refuses_richimg_owned_content() {
        let svg = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/sdd/system-overview.svg"
        ));
        let before = header_probes();
        assert!(
            probe_vector_dimensions(svg).is_some(),
            "control: a real SVG must be recognised, or this host has no SVG loader"
        );
        assert_eq!(
            header_probes(),
            before + 1,
            "control: non-richimg content MUST reach the loader — if it does not, the \
             counter is dead and the assertion below proves nothing"
        );

        let webp = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/anim.webp"
        ));
        let before = header_probes();
        assert_eq!(probe_vector_dimensions(webp), None);
        assert_eq!(
            header_probes(),
            before,
            "richimg-owned bytes must be refused BEFORE the PixbufLoader — delete the \
             `richimg::sniff` short-circuit and this is the assertion that goes red"
        );
    }

    /// An ordinary raster format is not scalable either, whatever name a caller might
    /// have found it under — `PixbufFormat::is_scalable()` says no for PNG.
    #[test]
    fn probe_vector_dimensions_is_none_for_an_ordinary_raster_format() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/logo.png"
        ));
        assert_eq!(probe_vector_dimensions(bytes), None);
    }

    #[test]
    fn probe_vector_dimensions_is_none_for_garbage() {
        assert_eq!(probe_vector_dimensions(b"not an image at all"), None);
    }
}
