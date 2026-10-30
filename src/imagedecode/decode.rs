//! The two decode entry points: [`decode`] (bytes → a `gdk::Texture`, the shape most
//! callers need) and [`decode_pixbuf`] (bytes → a `gdk_pixbuf::Pixbuf`, for
//! `sprite.rs`'s nearest-neighbour resample, which needs pixel-level access
//! `GdkTexture` has no equivalent for at this project's GTK floor).
//!
//! Both sniff by content first ([`richimg::sniff`]) and route WebP/GIF/APNG to
//! `richimg`; everything else goes to GTK's own decoder. Neither falls back to GTK
//! when `richimg` recognises the format but fails to decode it (a truncated WebP, or
//! GIF/APNG while their codecs are still `Unsupported` stubs) — a format `richimg`
//! owns is never retried through the leaking route this module exists to close off.

use super::probe::probe_input_size;
use super::screen::screen;
use super::svgencoding::{decode_svg, loader_input, LoaderInput};
use std::sync::Arc;

/// The result of decoding one image: the texture to show now (always the FIRST
/// frame), plus — for an animated `richimg`-owned format — the encoded bytes and
/// metadata a later playback feature needs to build a `richimg::Animation`.
pub(crate) struct DecodedImage {
    pub(crate) texture: gtk::gdk::Texture,
    /// `None` for a still image, or one decoded through the GTK route (which never
    /// animates). `Some` only for an animated `richimg`-owned format.
    pub(crate) animation: Option<AnimationSource>,
}

/// What a later playback feature needs to build a `richimg::Animation`: the whole
/// ENCODED file (not the decoded pixels — those are `DecodedImage::texture`'s first
/// frame only) plus the metadata `richimg::probe` already learned.
pub(crate) struct AnimationSource {
    pub(crate) bytes: Arc<[u8]>,
    #[allow(
        dead_code,
        reason = "read by playback, which lands later; wired and tested here first"
    )]
    pub(crate) info: richimg::Info,
}

/// The `richimg::Limits` this application decodes under: the pixel cap every local and
/// remote image is already held to ([`crate::limits::MAX_IMAGE_PIXELS`]), and the
/// operator-configured short-frame-delay substitute (`config.toml`'s
/// `[images].short_frame_delay_ms`).
pub(crate) fn richimg_limits() -> richimg::Limits {
    richimg::Limits {
        max_pixels: crate::limits::MAX_IMAGE_PIXELS as u64,
        short_delay_substitute: std::time::Duration::from_millis(
            crate::config::config().images.short_frame_delay_ms,
        ),
    }
}

/// Decode already-read `bytes` into a texture.
///
/// `origin` is a caller-chosen description of where the bytes came from — a path, a
/// URL, a sprite reference — folded into every log line so a decode failure can be
/// traced back to what failed to load. **Never used to route the decode**: routing is
/// by content alone, through this one choke point.
pub(crate) fn decode(bytes: &[u8], origin: &str) -> Option<DecodedImage> {
    // Finding 2 / TDD 6.8: the ONE increment for the test-only decode counter, so a
    // memory-gate test can assert "the second load was a cache HIT" by counting real
    // decodes instead of measuring byte growth a small decode can satisfy without
    // being skipped at all. Gated identically to the counter itself
    // (`imagedecode::decode_probe`'s doc comment) so it compiles to nothing here too.
    #[cfg(all(test, feature = "memory-gates"))]
    super::decode_probe::note_decode();
    if richimg::sniff(bytes).is_some() {
        return decode_richimg(bytes, origin);
    }
    decode_gtk(bytes, origin)
}

fn decode_richimg(bytes: &[u8], origin: &str) -> Option<DecodedImage> {
    let limits = richimg_limits();
    let info = match richimg::probe(bytes, &limits) {
        Ok(info) => info,
        Err(err) => {
            log::warn!("image {origin} not decoded (richimg probe: {err})");
            return None;
        }
    };
    // `first_frame` re-probes and applies the pixel cap BEFORE any canvas is
    // allocated (`richimg::Animation::new`'s own doc comment; `first_frame` shares
    // the same `check_pixel_cap` call) — the cheap re-probe costs nothing next to the
    // guarantee that a decode never happens over the cap.
    let frame = match richimg::first_frame(bytes, &limits) {
        Ok(frame) => frame,
        Err(err) => {
            log::warn!("image {origin} not decoded (richimg: {err})");
            return None;
        }
    };
    let texture = memory_texture_from_frame(frame, origin)?;
    let animation = info.animated.then(|| AnimationSource {
        bytes: Arc::from(bytes),
        info,
    });
    Some(DecodedImage { texture, animation })
}

/// Build a `gdk::MemoryTexture` over a richimg frame's owned, straight-alpha RGBA8
/// bytes. **Never `Texture::from_bytes`** — that would hand the ALREADY-DECODED pixels
/// back through an encoded-image loader, which is exactly the leaking route this
/// module exists to avoid.
pub(crate) fn memory_texture_from_frame(
    frame: richimg::Frame,
    origin: &str,
) -> Option<gtk::gdk::Texture> {
    FramePixels::of(frame, origin).map(|pixels| pixels.texture())
}

/// One decoded frame's straight-alpha RGBA8 pixels, held as shared `glib::Bytes`, so
/// the texture and every resample read the same buffer rather than copies of it.
///
/// What an animated theme sprite keeps per frame (TDD 27.9): a sprite drawn into a box
/// the layout chose is resampled from PIXELS, and a `GdkTexture` only gives them back
/// as a premultiplied download in the host's byte order.
pub(crate) struct FramePixels {
    bytes: gtk::glib::Bytes,
    width: i32,
    height: i32,
}

impl FramePixels {
    /// Take ownership of `frame`'s pixels. `None` — logged — for an empty frame.
    pub(crate) fn of(frame: richimg::Frame, origin: &str) -> Option<Self> {
        let (Ok(width), Ok(height)) = (i32::try_from(frame.width), i32::try_from(frame.height))
        else {
            log::warn!(
                "image {origin} decoded to a frame too large to address ({}x{}) — not loaded",
                frame.width,
                frame.height
            );
            return None;
        };
        if width == 0 || height == 0 {
            log::warn!("image {origin} decoded to an empty frame ({width}x{height}) — not loaded");
            return None;
        }
        Some(Self {
            bytes: gtk::glib::Bytes::from_owned(frame.rgba),
            width,
            height,
        })
    }

    #[cfg(test)]
    fn from_rgba(rgba: Vec<u8>, width: i32, height: i32) -> Self {
        Self {
            bytes: gtk::glib::Bytes::from_owned(rgba),
            width,
            height,
        }
    }

    pub(crate) fn width(&self) -> i32 {
        self.width
    }

    pub(crate) fn height(&self) -> i32 {
        self.height
    }

    /// The frame as a texture. **Never `Texture::from_bytes`** — that would hand the
    /// ALREADY-DECODED pixels back through an encoded-image loader.
    pub(crate) fn texture(&self) -> gtk::gdk::Texture {
        use gtk::prelude::Cast;
        gtk::gdk::MemoryTexture::new(
            self.width,
            self.height,
            gtk::gdk::MemoryFormat::R8g8b8a8,
            &self.bytes,
            self.width as usize * 4,
        )
        .upcast()
    }

    /// The frame resampled to exactly `w × h`, nearest-neighbour — the same resample
    /// `sprite::scaled` applies to a still sprite.
    pub(crate) fn resampled(&self, w: i32, h: i32) -> Option<gtk::gdk::Texture> {
        let pb = gtk::gdk_pixbuf::Pixbuf::from_bytes(
            &self.bytes,
            gtk::gdk_pixbuf::Colorspace::Rgb,
            true,
            8,
            self.width,
            self.height,
            self.width * 4,
        );
        resample_nearest(&pb, w, h)
    }
}

/// `pb` resampled to exactly `w × h` with nearest-neighbour filtering, as a texture.
/// `None` for a size that is not one, or a resample gdk-pixbuf refuses.
///
/// Nearest because GSK 4.6's `append_texture` filters linearly with no choice
/// (GTK4Rs/AP-114), so pre-resampling is the only way pixel art stays crisp at any zoom.
/// One definition, so a still sprite (`sprite::scaled`) and an animated sprite's frame
/// (`FramePixels::resampled`) cannot resample differently.
pub(crate) fn resample_nearest(
    pb: &gtk::gdk_pixbuf::Pixbuf,
    w: i32,
    h: i32,
) -> Option<gtk::gdk::Texture> {
    if w <= 0 || h <= 0 {
        return None;
    }
    pb.scale_simple(w, h, gtk::gdk_pixbuf::InterpType::Nearest)
        .map(|resampled| gtk::gdk::Texture::for_pixbuf(&resampled))
}

#[cfg(test)]
mod frame_pixels_tests {
    use super::FramePixels;
    use gtk::gdk::prelude::{TextureExt, TextureExtManual};

    /// Opaque red | opaque blue, 2×1.
    fn two_pixels() -> FramePixels {
        FramePixels::from_rgba(vec![255, 0, 0, 255, 0, 0, 255, 255], 2, 1)
    }

    /// TDD 27.9: an animated sprite's frame resamples to the requested box by repeating
    /// source pixels, never by blending them. Compared across columns rather than
    /// against channel values, so the download's byte order does not matter.
    #[test]
    fn a_frame_resamples_nearest_neighbour_to_the_requested_size() {
        let tex = two_pixels().resampled(4, 2).expect("a real size resamples");
        assert_eq!((tex.width(), tex.height()), (4, 2));
        let mut buf = vec![0u8; 4 * 2 * 4];
        tex.download(&mut buf, 4 * 4);
        let px: Vec<&[u8]> = buf.chunks_exact(4).collect();
        assert_eq!(
            px[0], px[1],
            "the left source pixel is repeated, not blended"
        );
        assert_eq!(
            px[2], px[3],
            "the right source pixel is repeated, not blended"
        );
        assert_ne!(px[1], px[2], "the two source pixels stay distinct");
        assert_eq!(
            &buf[..16],
            &buf[16..],
            "both rows sample the one source row"
        );
    }

    #[test]
    fn a_non_positive_size_is_not_a_size() {
        assert!(two_pixels().resampled(0, 3).is_none());
        assert!(two_pixels().resampled(3, -1).is_none());
    }

    /// The texture is the frame at its own size.
    #[test]
    fn the_texture_is_the_frame_at_its_natural_size() {
        let tex = two_pixels().texture();
        assert_eq!((tex.width(), tex.height()), (2, 1));
    }
}

/// Screen `bytes`, prepare them for a loader, and read their declared size, refusing
/// (with the reason logged) content the screen refuses, content whose size cannot be
/// read, and content over the pixel cap. The shared front half of [`decode_gtk`] and
/// [`decode_pixbuf`]: TDD 2.23c screens before the probe, which is itself a loader.
fn within_cap<'a>(bytes: &'a [u8], origin: &str) -> Option<LoaderInput<'a>> {
    let input = loader_input(screen(bytes, origin)?, Some(origin))?;
    let Some((w, h)) = probe_input_size(&input) else {
        log::warn!(
            "image {origin} not loaded: its dimensions could not be read, so the \
             {}-pixel cap cannot be enforced and the decode is refused",
            crate::limits::MAX_IMAGE_PIXELS
        );
        return None;
    };
    if !crate::limits::image_pixels_within_cap(w, h) {
        log::warn!(
            "image {origin} decodes to {w}×{h} pixels (cap {}) — not loaded",
            crate::limits::MAX_IMAGE_PIXELS
        );
        return None;
    }
    Some(input)
}

fn decode_gtk(bytes: &[u8], origin: &str) -> Option<DecodedImage> {
    // FAILS CLOSED. A probe that cannot answer refuses the decode rather than waving it
    // through (QA round 2, F-R2-7).
    //
    // The cap is a decompression-bomb defence, and it used to be skipped entirely
    // whenever `probe_pixel_size` returned `None` — while the decode below does NOT
    // depend on the probe succeeding, because GTK 4.6 decodes PNG/JPEG/TIFF with its own
    // loaders and only falls through to gdk-pixbuf afterwards. So on a host where the
    // probe's gdk-pixbuf loader is missing but GTK's built-in one is not, a bomb was
    // decoded with no cap applied at all: a security gate failing open on a
    // HOST-DEPENDENT input, in the one module whose entire premise is that host loader
    // availability cannot be depended on.
    //
    // The cost is refusing content GTK could have decoded but gdk-pixbuf cannot even
    // identify. That set is believed empty in practice (gdk-pixbuf ships loaders for all
    // three formats GTK handles natively, and WebP/GIF/APNG never reach here — they go
    // to richimg), but it is not provably empty, so the refusal is logged distinctly
    // from a cap breach: if a real host ever hits it, the log says which branch refused.
    let input = within_cap(bytes, origin)?;
    if input.svg_only() {
        return match decode_svg(&input, None) {
            Ok(pixbuf) => Some(DecodedImage {
                texture: gtk::gdk::Texture::for_pixbuf(&pixbuf),
                animation: None,
            }),
            Err(err) => {
                log::warn!("image {origin} not decoded: {err}");
                None
            }
        };
    }
    #[expect(clippy::disallowed_methods)] // this module IS the sanctioned route
    let result =
        gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_owned(input.bytes().to_vec()));
    match result {
        Ok(texture) => Some(DecodedImage {
            texture,
            animation: None,
        }),
        Err(err) => {
            log::warn!("image {origin} not decoded: {err}");
            None
        }
    }
}

/// Decode already-read `bytes` into a `gdk_pixbuf::Pixbuf` rather than a
/// `gdk::Texture` — the shape `sprite.rs`'s nearest-neighbour resample needs
/// (`Pixbuf::scale_simple`), which `GdkTexture` has no equivalent for at this
/// project's GTK floor (v4_6; the filter-choosing `append_scaled_texture` is 4.10 —
/// see `sprite.rs`'s module doc comment).
///
/// Same content-based routing as [`decode`]: a richimg-decoded frame's RGBA8 bytes are
/// wrapped in a `Pixbuf` with [`gtk::gdk_pixbuf::Pixbuf::from_mut_slice`] — raw pixel
/// memory, never encoded bytes, so this is not one of the banned decode entry points —
/// so a WebP/GIF/APNG sprite never reaches the leaking `Pixbuf::from_stream` chain
/// either.
pub(crate) fn decode_pixbuf(bytes: &[u8], origin: &str) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    if richimg::sniff(bytes).is_some() {
        let limits = richimg_limits();
        return match richimg::first_frame(bytes, &limits) {
            Ok(frame) => pixbuf_from_frame(frame, origin),
            Err(err) => {
                log::warn!("image {origin} not decoded (richimg: {err})");
                None
            }
        };
    }
    // Fails closed for the same reason [`decode_gtk`] does (F-R2-7) — and here the
    // refusal costs nothing at all: this route decodes through gdk-pixbuf itself, so
    // content its probe cannot identify is content `Pixbuf::from_stream` was going to
    // reject one step later anyway.
    let input = within_cap(bytes, origin)?;
    if input.svg_only() {
        return decode_svg(&input, None)
            .map_err(|err| log::warn!("image {origin} not decoded: {err}"))
            .ok();
    }
    let stream = gtk::gio::MemoryInputStream::from_bytes(&gtk::glib::Bytes::from_owned(
        input.bytes().to_vec(),
    ));
    #[expect(clippy::disallowed_methods)] // this module IS the sanctioned route
    match gtk::gdk_pixbuf::Pixbuf::from_stream(&stream, gtk::gio::Cancellable::NONE) {
        Ok(pixbuf) => Some(pixbuf),
        Err(err) => {
            log::warn!("image {origin} not decoded: {err}");
            None
        }
    }
}

/// Re-render already-admitted VECTOR `bytes` at `target_w` pixels wide, preserving the
/// document's own aspect ratio. `None` on any loader failure, so a caller can fall back
/// to the ordinary natural-size decode.
///
/// **Takes BYTES, not a path, and that is the entire point of it existing here.** Its
/// predecessor lived in `imagecache::loader` and called
/// `Pixbuf::from_file_at_scale(path, …)`, which re-opened the file BY NAME after the
/// caller had already read, admitted and content-sniffed it — a check-then-use seam
/// (TOCTOU) that discarded every guarantee admission had just established. The bytes
/// decoded were not the bytes sniffed, so the byte cap bounded nothing about the second
/// read, and the content proven to be a scalable vector was merely the content that had
/// been at that name a moment earlier. Feeding the admitted buffer closes it: there is
/// exactly one read, and the thing decoded is the thing that was checked.
///
/// `height` is passed as `-1` with `preserve_aspect_ratio = true`, exactly as the
/// path-based call did — the loader derives the second axis itself, so this function's
/// own rounding never decides which axis binds. `from_stream_at_scale` is the direct
/// byte-stream twin of `from_file_at_scale`; nothing about the RENDER changes, only
/// where the bytes come from.
///
/// The caller is responsible for bounding `target_w` (`imagecache::loader` applies
/// `cap_raster` to the target before calling): a vector's natural size bounds nothing,
/// since a `viewBox="0 0 24 24"` document can be asked for a raster of any size at all.
pub(crate) fn rasterize_vector_bytes(
    bytes: &[u8],
    target_w: i32,
    origin: &str,
) -> Option<gtk::gdk::Texture> {
    // TDD 2.23c. Refused content returns `None`, so the caller's natural-size fallback
    // runs — and is refused by the same screen, which is what reports it.
    let input = loader_input(screen(bytes, origin)?, Some(origin))?;
    if input.svg_only() {
        return match decode_svg(&input, Some(target_w)) {
            Ok(pixbuf) => Some(gtk::gdk::Texture::for_pixbuf(&pixbuf)),
            Err(err) => {
                log::debug!(
                    "vector image {origin} not re-rendered at {target_w}px wide ({err}) — \
                     using its natural size"
                );
                None
            }
        };
    }
    let stream = gtk::gio::MemoryInputStream::from_bytes(&gtk::glib::Bytes::from_owned(
        input.bytes().to_vec(),
    ));
    #[expect(clippy::disallowed_methods)] // this module IS the sanctioned route
    match gtk::gdk_pixbuf::Pixbuf::from_stream_at_scale(
        &stream,
        target_w,
        -1,
        true,
        gtk::gio::Cancellable::NONE,
    ) {
        Ok(pixbuf) => Some(gtk::gdk::Texture::for_pixbuf(&pixbuf)),
        Err(err) => {
            // Not a user-visible failure: the caller falls back to the natural-size
            // decode, so the image still appears — just not re-rendered for zoom.
            log::debug!(
                "vector image {origin} not re-rendered at {target_w}px wide ({err}) — \
                 using its natural size"
            );
            None
        }
    }
}

fn pixbuf_from_frame(frame: richimg::Frame, origin: &str) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    if frame.width == 0 || frame.height == 0 {
        log::warn!(
            "image {origin} decoded to an empty frame ({}x{}) — not loaded",
            frame.width,
            frame.height
        );
        return None;
    }
    let stride = frame.width as i32 * 4;
    Some(gtk::gdk_pixbuf::Pixbuf::from_mut_slice(
        frame.rgba,
        gtk::gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        frame.width as i32,
        frame.height as i32,
        stride,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::prelude::TextureExt;

    fn webp_fixture() -> &'static [u8] {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/anim.webp"
        ))
    }

    fn png_fixture() -> &'static [u8] {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/wide.png"
        ))
    }

    /// The host's own pixbuf loaders never enter it: a WebP decodes through
    /// `richimg`, whatever gdk-pixbuf loaders this host does or does not have
    /// installed (the whole reason `richimg` exists — memgate/gtk.rs's own WebP gates
    /// used to `SKIPPED […]` on a host with no WebP pixbuf loader; this must not).
    #[test]
    fn a_webp_decodes_via_richimg_regardless_of_host_pixbuf_loaders() {
        let decoded = decode(webp_fixture(), "test:anim.webp").expect("richimg decodes it");
        assert_eq!(
            (decoded.texture.width(), decoded.texture.height()),
            (480, 270)
        );
        assert!(
            decoded.animation.is_some(),
            "anim.webp is animated; AnimationSource must be populated"
        );
    }

    /// Content sniffing, not extension: the same WebP bytes under a `.png`-shaped
    /// origin string still route to richimg and still decode.
    #[test]
    fn a_webp_saved_with_a_png_name_still_decodes_via_richimg() {
        let decoded =
            decode(webp_fixture(), "test:pretend.png").expect("content sniffing ignores the name");
        assert_eq!(
            (decoded.texture.width(), decoded.texture.height()),
            (480, 270)
        );
    }

    /// The `AnimationSource`, when present, carries the ENCODED bytes (for a later
    /// `richimg::Animation::new`), not the decoded pixels.
    #[test]
    fn the_animation_source_carries_the_encoded_bytes() {
        let decoded = decode(webp_fixture(), "test:anim.webp").expect("decodes");
        let anim = decoded.animation.expect("animated");
        assert_eq!(&*anim.bytes, webp_fixture());
        assert!(anim.info.animated);
    }

    /// A still PNG must still go through GTK's own decoder, unchanged. `richimg::sniff`
    /// must not claim it (`probe.rs`'s own tests pin that directly); the field this
    /// module can observe is that no `AnimationSource` is ever produced for it.
    #[test]
    fn a_still_png_decodes_via_the_gtk_route_with_no_animation_source() {
        let decoded = decode(png_fixture(), "test:wide.png").expect("GTK decodes a plain PNG");
        assert_eq!(decoded.texture.width(), 1600);
        assert!(decoded.animation.is_none());
    }

    /// TDD 2.23c: an SVG naming the XInclude namespace reaches no loader through any of
    /// the three decode entry points, and is remembered as refused; the same SVG
    /// without it still decodes, so the refusal is the screen's and not the loader's.
    #[test]
    fn an_svg_using_xinclude_is_refused_by_every_entry_point() {
        if !crate::imagedecode::xinclude::enforced() {
            println!("SKIPPED [2.23c]: this platform does not screen SVG XInclude");
            return;
        }
        const PLAIN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"/></svg>"#;
        let hostile = PLAIN.replace(
            "<svg ",
            r#"<svg xmlns:xi="http://www.w3.org/2001/XInclude" "#,
        );
        let origin = "test:decode:xinclude.svg";
        assert!(decode(hostile.as_bytes(), origin).is_none());
        assert!(crate::imagedecode::svg_refused(origin));
        assert!(decode_pixbuf(hostile.as_bytes(), origin).is_none());
        assert!(rasterize_vector_bytes(hostile.as_bytes(), 8, origin).is_none());
        crate::imagedecode::forget_for_test(origin);
        let plain = decode(PLAIN.as_bytes(), "test:decode:plain.svg").expect("librsvg decodes it");
        assert_eq!(plain.texture.width(), 4);
    }

    /// Truncated/garbage bytes degrade to `None` (the placeholder), never a panic —
    /// same fixture shape as `preview::build`'s
    /// `undecodable_webp_degrades_to_one_anchored_child`.
    #[test]
    fn truncated_webp_bytes_degrade_to_none() {
        assert!(decode(b"RIFF\x00\x00\x00\x00WEBPVP8 ", "test:truncated.webp").is_none());
    }

    #[test]
    fn garbage_bytes_degrade_to_none() {
        assert!(decode(b"this is not an image at all", "test:garbage").is_none());
    }

    /// Over the pixel cap, refused before any pixel buffer is allocated. Reuses
    /// richimg's own `oversized_canvas.webp` fixture (a declared 16384x16384 canvas,
    /// proven by richimg's `no_oversized_alloc.rs` to return `Error::TooLarge` from
    /// BOTH `Animation::new` and `first_frame` without ever allocating a
    /// canvas-sized buffer) rather than hand-crafting new WebP bytes here.
    #[test]
    fn a_webp_over_the_pixel_cap_is_refused_before_any_pixel_buffer() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("richimg/tests/fixtures/oversized_canvas.webp"),
        )
        .expect("richimg's oversized_canvas.webp fixture");
        let cap = crate::testlog::capture();
        assert!(
            decode(&bytes, "test:oversized_canvas.webp").is_none(),
            "a declared canvas over MAX_IMAGE_PIXELS must be refused"
        );
        assert!(
            cap.logged(log::Level::Warn, "exceeds the configured pixel cap"),
            "the refusal must be diagnosed as the pixel cap, not some other failure: {:?}",
            cap.records()
        );
    }

    /// GIF/APNG may still be `Unsupported` stub codecs in this tree — routing
    /// must degrade to the placeholder, never fall back to
    /// GTK and never panic, once content sniffing has claimed the format.
    #[test]
    fn a_gif_that_richimg_cannot_yet_decode_degrades_to_none_not_a_gtk_fallback() {
        assert!(decode(b"GIF89a not a real gif body", "test:stub.gif").is_none());
    }

    #[test]
    fn decode_pixbuf_routes_webp_via_richimg_too() {
        let pixbuf = decode_pixbuf(webp_fixture(), "test:anim.webp").expect("richimg decodes it");
        assert_eq!((pixbuf.width(), pixbuf.height()), (480, 270));
    }

    #[test]
    fn decode_pixbuf_routes_a_still_png_via_gtk() {
        let pixbuf = decode_pixbuf(png_fixture(), "test:wide.png").expect("GTK decodes it");
        assert_eq!(pixbuf.width(), 1600);
    }

    /// The no-fallback rule: a GIF whose header is valid (so `richimg::sniff` claims it)
    /// but whose image data stops early, which `richimg` refuses. **The assertion that
    /// holds on every host is the arrival count**: `decode` must refuse without the bytes
    /// ever reaching a gdk-pixbuf loader, and a fallback reaches one in its first step
    /// (`decode_gtk` → `within_cap` → the header probe), whatever that loader would then
    /// answer. Mutation run: returning `decode_gtk` from `decode_richimg`'s refusal fails
    /// it on Ubuntu 22.04 and 26.04 alike.
    ///
    /// Where gdk-pixbuf's own GIF decoder is lenient, the fallback would also have
    /// SUCCEEDED, and the test says so as a second, visible witness. That half is
    /// host-dependent: glycin (gdk-pixbuf 2.44.5 on Ubuntu 26.04) refuses these bytes as
    /// richimg does, so there it cannot be observed and only the arrival count speaks.
    ///
    /// **The vehicle must be GIF and cannot be APNG**, because whether gdk-pixbuf will
    /// so much as CLAIM an APNG is a per-host answer. MEASURED on the same bytes:
    /// decoded by 2.42.8 (Ubuntu 22.04) and by 2.44.7 (Homebrew), and refused outright
    /// by 2.42.10 (Ubuntu 24.04) with "Unrecognized image file format". So it is not a
    /// version floor to raise and wait out — the APNG fixture that held this role made
    /// the oracle's premise false on one live platform while the rule it guards was
    /// never in doubt. GIF is claimed by all three.
    #[test]
    fn a_format_richimg_claims_never_falls_back_to_gtk() {
        let bytes = gtk_decodable_gif_richimg_refuses();
        assert_eq!(
            richimg::sniff(&bytes),
            Some(richimg::Format::Gif),
            "precondition: the fixture is content-sniffed as GIF"
        );
        assert!(
            richimg::first_frame(&bytes, &richimg_limits()).is_err(),
            "precondition: richimg refuses these bytes"
        );
        let before = super::super::probe::header_probes();
        assert!(
            decode(&bytes, "test:disagreeing.png").is_none(),
            "a format richimg claims must never fall back to GTK on richimg's own refusal"
        );
        assert_eq!(
            super::super::probe::header_probes(),
            before,
            "richimg's refusal must not reach a gdk-pixbuf loader at all"
        );
        if decode_gtk(&bytes, "test:control.png").is_none() {
            eprintln!(
                "NOTE: this host's gdk-pixbuf refuses the fixture too, so a fallback would \
                 not have decoded it; the arrival count above is the only witness here"
            );
        }
    }

    /// A GIF89a that is structurally complete — logical screen descriptor, global
    /// colour table, one frame, block terminator, trailer — whose first frame's LZW
    /// stream sends End-of-Information after four of its sixteen pixels. gdk-pixbuf's
    /// GIF loader leaves the rest of the frame at the background colour and reports
    /// success; the `gif` crate (`richimg`'s GIF codec) insists the frame buffer be
    /// filled and returns `Malformed`. The two decoders deliberately disagree about
    /// these bytes, and that disagreement is what makes the no-fallback assertion
    /// above able to fail at all.
    ///
    /// MEASURED against gdk-pixbuf 2.42.8, 2.42.10, 2.44.6 and 2.44.7 — all four decode
    /// it — and `gif` 0.14.2, which refuses it. gdk-pixbuf 2.44.5 built with glycin
    /// (Ubuntu 26.04) refuses it as well, which is why the test's visible witness is
    /// host-dependent and its arrival count is not.
    ///
    /// Malformed-but-tolerated, never truncated: a file that simply ENDS early is
    /// refused by gdk-pixbuf too ("Not all frames of the GIF image were loaded"), which
    /// would take the visible witness away on every host.
    fn gtk_decodable_gif_richimg_refuses() -> Vec<u8> {
        #[rustfmt::skip]
        const BYTES: [u8; 45] = [
            // "GIF89a"
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61,
            // Logical screen descriptor: 4x4, global colour table of two entries.
            0x04, 0x00, 0x04, 0x00, 0x80, 0x00, 0x00,
            // The global colour table: black, white.
            0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF,
            // Graphic control extension: 100 ms, no transparent index.
            0x21, 0xF9, 0x04, 0x00, 0x0A, 0x00, 0x00, 0x00,
            // Image descriptor: at 0,0, 4x4, no local colour table.
            0x2C, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x04, 0x00, 0x00,
            // LZW minimum code size, then one 4-byte sub-block holding
            // CLEAR, index 0, CLEAR, index 0, CLEAR, index 0, CLEAR, index 0,
            // CLEAR, EOI -- four pixels for a sixteen-pixel frame.
            0x02, 0x04, 0x04, 0x41, 0x10, 0x2C,
            // Block terminator, then the trailer.
            0x00, 0x3B,
        ];
        BYTES.to_vec()
    }

    /// F-R2-7: the pixel cap must FAIL CLOSED. Content whose dimensions cannot be read
    /// is refused, rather than decoded with no cap applied.
    ///
    /// Uses bytes no loader can identify, which is the only reachable way to make the
    /// probe answer `None` without uninstalling a host loader. That makes this a weaker
    /// test than the hazard deserves — it cannot reproduce the real shape, where the
    /// probe fails but `Texture::from_bytes` SUCCEEDS, because on this host both
    /// recognise the same formats. It still pins the branch: restore the old
    /// `if let Some(..) = probe` shape and this goes red, because the unreadable bytes
    /// then fall through to a decode attempt and the refusal is reported as a decode
    /// failure instead of a cap refusal.
    #[test]
    fn unreadable_dimensions_refuse_the_decode_rather_than_skipping_the_cap() {
        let cap = crate::testlog::capture();
        assert!(
            decode(b"not an image at all", "test:unreadable").is_none(),
            "content whose size cannot be read must not be decoded"
        );
        assert!(
            cap.logged(log::Level::Warn, "the decode is refused"),
            "the refusal must be diagnosed as the unenforceable cap, not as a generic \
             decode failure: {:?}",
            cap.records()
        );
    }
}
