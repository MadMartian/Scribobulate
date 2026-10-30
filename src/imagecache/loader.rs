//! Resolve an [`ImageResolution`] into a decoded texture, through this crate's one
//! image cache ([`super::get_or_fetch`]) and its one decode choke point
//! ([`crate::imagedecode`]).
//!
//! Relocated here from `renderer::start` because it
//! is fundamentally cache-client code — every function here exists to decide what to
//! hand [`super::get_or_fetch`] and what to do with what comes back — and because that
//! file was already well past POLICY's soft file-size limit; this module is a better
//! home for logic that grows with the cache's own contract. `renderer::start`
//! re-exports [`load_texture`] and [`LoadedImage`] at their old path so nothing
//! outside this crate's image-loading code needed to change.
//!
//! **Local images** go through [`load_local`]: a path+mtime+length+size cache so a
//! re-render does not decode again. A local file is always admitted first
//! ([`crate::imagedecode::read_local`] — a regular file, within the configured byte
//! cap) and its CONTENT, never its name, decides whether it takes the vector branch
//! (dimensions from [`crate::imagedecode::probe_vector_dimensions`]) or the ordinary
//! raster one (below) — see [`load_local`]'s own doc comment for why (Finding 1 /
//! TDD 2.23b). Raster decode is [`crate::imagedecode`], the application's one choke
//! point: it sniffs by content and routes WebP/GIF/APNG
//! to `richimg` — never `Texture::from_file`, which leaked ~12 MB/call on a valid
//! animated WebP and SIGSEGVed on a truncated one (`Pixbuf::from_file` errored
//! outright and was never a fallback — ScrAP-351 / ScrAP-352, now superseded by
//! this route for every format it claims).
//!
//! **Remote images** are fetched by [`crate::imagefetch`], not by GIO — a
//! `gio::File::for_uri("https://…")` needs a GVfs http backend that claims the scheme,
//! which exists on the Linux desktop and nowhere else, so that route rendered
//! nothing at all on macOS (GTK4Rs/AP-292). The bytes then go through the same
//! [`crate::imagedecode`] choke point as a local file. `Refused`/`Missing` never
//! load. Remote fetches block the main thread for the request (accepted for the
//! opt-in "Show Unsafe Images" path, GTK4Rs/AP-44, its 34a half).
//!
//! ## Animation on a cache hit (TDD 27.1)
//!
//! A cache MISS always resolves its own animation bytes directly (see
//! [`decode_local_raster`] and [`load_remote_texture`]). A cache HIT answers from
//! [`super::CachedTexture::animation`]'s hint: [`super::AnimationHint::Remote`]
//! already carries the retained bytes, no further work needed; a local
//! [`super::AnimationHint::Local`] carries none (see `imagecache`'s module doc), so
//! [`recover_local_animation`] runs — free when a live picture still holds the
//! bytes, a bounded re-read otherwise.
use super::keys::{file_stamp, local_animation_key, local_cache_key, FileStamp};
use crate::animation::source;
use crate::imagedecode;
use crate::links::ImageResolution;
use crate::renderer::image::{cap_raster, zoomed_extent, Extent};
use gtk::prelude::TextureExt;
use std::cell::RefCell;
use std::sync::Arc;

pub(crate) fn load_texture(resolution: &ImageResolution, zoom: f64) -> Option<LoadedImage> {
    match resolution {
        ImageResolution::Local(path) => load_local(path, zoom),
        ImageResolution::Remote(uri) => load_remote_texture(uri),
        ImageResolution::Refused | ImageResolution::NetworkShare | ImageResolution::Missing => None,
    }
}

/// Decode a contained local image, reusing a cached texture when the file and
/// the size it is drawn at have not changed (TDD 6.8).
///
/// **The vector branch is chosen by CONTENT, never by `path.extension()` alone**
/// (Finding 1 / TDD 2.23b). Before this, `pixbuf_format_for_path` decided the branch
/// from the file name ahead of any admission check, so a FIFO named `.svg` blocked the
/// main thread forever, and a richimg-owned format (WebP/GIF/APNG) wearing a `.svg`
/// name reached the gdk-pixbuf loader chain [`crate::imagedecode`] exists to make
/// unreachable. The extension test below is kept, but demoted to a pure PERFORMANCE
/// prefilter: it decides only whether admitting and content-sniffing the file is
/// worth paying for, never whether the vector branch runs. A wrong guess there costs
/// at most a missed sharpen-at-zoom (a real SVG under an unusual name) or one
/// avoidable admission read (a non-SVG file wearing `.svg`) — never a hang or a leak,
/// because [`admit_local_bytes`] and [`imagedecode::probe_vector_dimensions`] gate
/// every actual decision from here on.
fn load_local(path: &std::path::Path, zoom: f64) -> Option<LoadedImage> {
    let stamp = file_stamp(path);
    // Identity zoom needs no dimension probe, vector or otherwise — the natural-size
    // raster decode below already IS the answer at zoom 1.0.
    let zoom_changes_size = zoomed_extent(8, 8, zoom).w != 8;
    if zoom_changes_size && pixbuf_format_for_path(path).is_some_and(|f| f.is_scalable()) {
        return match admit_local_bytes(path) {
            // Refused before a single content byte was decoded (FIFO, directory,
            // oversized) — never decodable either way, so there is nothing to fall
            // back to (the ordinary raster path would hit the identical refusal).
            None => None,
            Some(bytes) => match imagedecode::probe_vector_dimensions(&bytes) {
                Some((w, h)) => load_local_vector(path, &bytes, w, h, zoom, stamp)
                    .or_else(|| load_local_raster(path, stamp, Some(bytes))),
                // richimg-owned, or content gdk-pixbuf itself does not call scalable —
                // decode via the ordinary route, reusing the bytes already admitted
                // above rather than reading the file a second time.
                None => load_local_raster(path, stamp, Some(bytes)),
            },
        };
    }
    load_local_raster(path, stamp, None)
}

/// Admit a local file exactly as the ordinary raster path does
/// ([`imagedecode::read_local`]: a regular file, within the configured byte cap) —
/// so the vector branch can never reach a gdk-pixbuf call before this succeeds.
/// Logs and returns `None` on refusal in the same shape [`decode_local_raster`]
/// uses for the same refusal, so a rejected `.svg`-named file is reported once.
fn admit_local_bytes(path: &std::path::Path) -> Option<Arc<[u8]>> {
    match imagedecode::read_local(path) {
        Ok(bytes) => Some(bytes),
        Err(refusal) => {
            log::warn!("image not loaded: {} ({refusal})", path.display());
            None
        }
    }
}

/// Re-render an already content-confirmed vector source at `zoom`. `w`/`h` are the
/// declared size [`imagedecode::probe_vector_dimensions`] read from CONTENT, not from
/// this file's name — by the time this runs, admission and content-sniffing have
/// already proved the source is neither richimg-owned nor otherwise a non-scalable
/// raster (Finding 1).
///
/// `bytes` are those same ADMITTED bytes, carried in rather than re-read: the decode
/// below must be of the content that was checked, not of whatever the path resolves to
/// by the time it runs (see [`imagedecode::rasterize_vector_bytes`]).
fn load_local_vector(
    path: &std::path::Path,
    bytes: &[u8],
    w: i32,
    h: i32,
    zoom: f64,
    stamp: FileStamp,
) -> Option<LoadedImage> {
    if !crate::limits::image_pixels_within_cap(w, h) {
        log::warn!(
            "image {} decodes to {w}×{h} pixels (cap {}) — not loaded",
            path.display(),
            crate::limits::MAX_IMAGE_PIXELS
        );
        return None;
    }
    let intrinsic = Extent {
        w: w.max(1),
        h: h.max(1),
    };
    let zoomed = zoomed_extent(intrinsic.w, intrinsic.h, zoom);
    if zoomed == intrinsic {
        return None;
    }
    let target = cap_raster(zoomed);
    let key = local_cache_key(path, stamp, &format!("{}x{}", target.w, target.h));
    let cached = super::get_or_fetch(&key, || {
        rasterize_vector(path, bytes, target).map(|t| (t, super::AnimationHint::Still))
    })?;
    // A vector source never sniffs as a richimg-owned format, so it never animates.
    Some(LoadedImage {
        texture: cached.texture,
        intrinsic,
        animation: None,
    })
}

/// The ordinary raster cache path: reuses a decode across renders keyed by the file's
/// stamp (mtime+length) so a HIT costs nothing beyond a `stat()` and a map lookup
/// (TDD 6.8) — unaffected by Finding 1's fix for the overwhelming majority of images,
/// whose extension never matches [`pixbuf_format_for_path`]'s scalable check and so
/// never reach [`load_local`]'s admission step at all.
///
/// `admitted` carries bytes the vector-candidate check upstream already read through
/// [`imagedecode::read_local`] — threaded through here so a `.svg`-named file that
/// content-sniffing turned away from the vector branch (richimg-owned, or simply not
/// a scalable format) decodes from those SAME bytes instead of reading the file a
/// second time. `None` for the ordinary case (an unzoomed image, or one whose
/// extension gave [`load_local`] no reason to try the vector branch) — there
/// [`decode_local_raster`] reads on a genuine cache MISS only, exactly as before this
/// fix.
fn load_local_raster(
    path: &std::path::Path,
    stamp: FileStamp,
    admitted: Option<Arc<[u8]>>,
) -> Option<LoadedImage> {
    let key = local_cache_key(path, stamp, "natural");
    // Populated by the decode closure only on a genuine MISS — see
    // `decode_local_raster_bytes`'s doc comment. On a HIT it stays `None`, and the
    // hint carried back in `cached.animation` decides whether to recover the bytes
    // below.
    let fresh: RefCell<Option<Arc<[u8]>>> = RefCell::new(None);
    let cached = super::get_or_fetch(&key, || match &admitted {
        Some(bytes) => decode_local_raster_bytes(Arc::clone(bytes), path, stamp, &fresh),
        None => decode_local_raster(path, stamp, &fresh),
    })?;
    let animation = match fresh.into_inner() {
        Some(bytes) => Some(bytes),
        None => match cached.animation {
            super::AnimationHint::Local => recover_local_animation(path, stamp),
            super::AnimationHint::Still | super::AnimationHint::Remote(_) => None,
        },
    };
    Some(LoadedImage::at_natural_size(cached.texture, animation))
}

/// Raster decode, through the application's one decode choke point
/// ([`crate::imagedecode`]): admission ([`crate::imagedecode::read_local`]) — a
/// regular file, within the configured byte cap — then content-sniffed decode
/// ([`crate::imagedecode::decode`]), which routes WebP/GIF/APNG to `richimg` and
/// everything else to GTK. This is what makes the leaking gdk-pixbuf WebP route
/// (`Texture::from_file`, ~12 MB/call on an animated WebP) unreachable; the cache in
/// [`load_local_raster`] is what stops a re-render from paying for a decode twice
/// either way. Reads the file itself — [`decode_local_raster_bytes`] is the version
/// for a caller that already has admitted bytes in hand.
fn decode_local_raster(
    path: &std::path::Path,
    stamp: FileStamp,
    fresh: &RefCell<Option<Arc<[u8]>>>,
) -> Option<(gtk::gdk::Texture, super::AnimationHint)> {
    let bytes = match imagedecode::read_local(path) {
        Ok(bytes) => bytes,
        Err(refusal) => {
            log::warn!("image not loaded: {} ({refusal})", path.display());
            return None;
        }
    };
    decode_local_raster_bytes(bytes, path, stamp, fresh)
}

/// The content-sniffed decode half of [`decode_local_raster`], taking already-admitted
/// `bytes` rather than reading `path` itself — the shape [`load_local`]'s
/// vector-candidate check needs so a `.svg`-named file that turns out to be richimg-owned
/// (or simply not scalable) is decoded from the SAME read, not a second one.
///
/// `fresh` is a side channel, not a return value, because [`super::get_or_fetch`]'s
/// closure signature is fixed at `Option<(gtk::gdk::Texture, super::AnimationHint)>`
/// — the cache stores only the hint, deliberately: a playing animation is not a cache
/// entry — animation state is per picture, bounded by what is on screen (TDD 27.3).
/// This function runs ONLY on a cache MISS, so it is the one place that
/// ever needs to actually resolve a local animated file's bytes from scratch; a cache
/// HIT recovers them separately, in [`recover_local_animation`].
fn decode_local_raster_bytes(
    bytes: Arc<[u8]>,
    path: &std::path::Path,
    stamp: FileStamp,
    fresh: &RefCell<Option<Arc<[u8]>>>,
) -> Option<(gtk::gdk::Texture, super::AnimationHint)> {
    let origin = path.display().to_string();
    let decoded = imagedecode::decode(&bytes, &origin)?;
    let hint = match decoded.animation {
        Some(anim_source) => {
            let key = local_animation_key(path, stamp);
            // Deduped against any other picture currently showing this same file
            // (`animation::source::shared`) before the paintable opens its own
            // decoder — decoder state itself is never shared (TDD "Animation
            // state"). Kept alive via `fresh` for the rest of THIS call; the cache
            // entry itself stores only `AnimationHint::Local`, no bytes.
            fresh.replace(Some(source::shared(&key, anim_source.bytes)));
            super::AnimationHint::Local
        }
        None => super::AnimationHint::Still,
    };
    Some((decoded.texture, hint))
}

/// Recover a LOCAL animated image's encoded bytes after a cache HIT (TDD 27.1: an
/// animation plays by itself, not only the first time it is rendered).
/// [`decode_local_raster`] never re-runs on a hit, so its own registration is
/// unreachable here — this is the only other route back to the same bytes.
///
/// Cheap when another picture showing this exact file is still alive
/// ([`crate::animation::source::shared_or_else`] finds it with no I/O at all);
/// otherwise a bounded re-read through the same admission
/// ([`crate::imagedecode::read_local`]) every local image goes through — far
/// cheaper than the decode a genuine miss pays, and it keeps the plan's residency
/// bound: the recovered bytes are handed straight to this render, never written
/// back into the texture cache.
fn recover_local_animation(path: &std::path::Path, stamp: FileStamp) -> Option<Arc<[u8]>> {
    let key = local_animation_key(path, stamp);
    source::shared_or_else(&key, || match imagedecode::read_local(path) {
        Ok(bytes) => Some(bytes),
        Err(refusal) => {
            log::warn!(
                "animation bytes not re-read on a cache hit: {} ({refusal})",
                path.display()
            );
            None
        }
    })
}

fn pixbuf_format_for_path(path: &std::path::Path) -> Option<gtk::gdk_pixbuf::PixbufFormat> {
    let ext = path.extension()?.to_str()?;
    gtk::gdk_pixbuf::Pixbuf::formats()
        .into_iter()
        .find(|f| f.extensions().iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

/// A loaded image, plus the size it occupies **at zoom 1.0**.
///
/// The second member exists because those two facts stopped being the same one. For a
/// raster source the texture's own dimensions are the design-time size, as they always
/// were; for a vector source re-rendered for zoom the texture is already `zoom×` larger,
/// so reading the size back off it and scaling again would compound the factor — a
/// 3× render laid out at 9×. The renderer needs the size at zoom 1.0 and the loader is
/// the only party that still knows it.
pub(crate) struct LoadedImage {
    pub(crate) texture: gtk::gdk::Texture,
    pub(crate) intrinsic: Extent,
    /// `Some` only for an animated richimg-owned format — the shared,
    /// already-deduped encoded bytes
    /// [`crate::animation::paintable::AnimatedPaintable::new`] needs, resolved the
    /// same way whether this render was a cache miss or a hit (TDD 27.1).
    /// `None` for a still image, unconditionally.
    pub(crate) animation: Option<Arc<[u8]>>,
}

impl LoadedImage {
    /// For every source whose texture IS its design-time size — i.e. everything except a
    /// re-rendered vector (which never animates, so it always passes `None` here).
    fn at_natural_size(texture: gtk::gdk::Texture, animation: Option<Arc<[u8]>>) -> Self {
        let intrinsic = Extent {
            w: texture.width().max(1),
            h: texture.height().max(1),
        };
        Self {
            texture,
            intrinsic,
            animation,
        }
    }
}

/// Rasterise a vector image at `zoom`, or `None` to fall back to the ordinary path.
///
/// Decodes the ADMITTED `bytes`, never the path — `path` is carried only to name the
/// image in a log line. The decode itself is [`imagedecode::rasterize_vector_bytes`],
/// inside the one sanctioned decode module; this function owns the SIZING decision
/// (which is the part `imagecache` is responsible for) and nothing else.
///
/// **MEASURED, not assumed** (`probes/svg-rasterise-rs`, librsvg 2.52.5 / gdk-pixbuf
/// 2.42.8): the `…_at_scale` entry point hands the requested size to librsvg's
/// pixbuf loader, which RE-RENDERS the document at it rather than resampling a
/// natural-size raster — the anti-aliased fringe of a 3× render measures 4.7‰ of its
/// pixels against 19.2‰ for a bilinear upscale of the same drawing, which is the ~3×
/// ratio a stretched 1px edge predicts.
///
/// **Why `Texture::from_file` cannot do this.** GTK 4.6 decodes only PNG/JPEG/TIFF
/// itself and falls through to `gdk_pixbuf_new_from_stream` for everything else, which
/// passes the loader a **no-op size callback** — so a scalable source is asked for its
/// natural size and the caller is given no way to say otherwise (researcher-sourced,
/// gdk-pixbuf-io.c). The size has to be an input to the decode, and this is the entry
/// point that takes one.
///
/// And a correctly-sized texture is the only route to a sharp result at this floor: GSK
/// 4.6 sets no cairo filter at all on a texture node, so an enlargement gets cairo's
/// default `FILTER_GOOD` (bilinear quality) whatever the renderer, and
/// `gtk_snapshot_append_scaled_texture` does not exist until 4.10 (cf. `sprite.rs`,
/// which pre-resamples for the same reason).
///
/// **The target size is where the pixel bound has to be applied for a vector source,
/// and this is a NEW exposure the re-render introduces.** A `viewBox="0 0 24 24"`
/// document probes as 576 pixels and passes any cap trivially, then gets asked for a
/// raster nine times larger than the layout — natural size bounds nothing here.
/// [`cap_raster`] is therefore applied to the TARGET, before the decode.
///
/// One axis is passed and the other left `-1`: with `preserve_aspect_ratio` the loader
/// derives the second itself, which removes any chance of this function's own rounding
/// deciding which axis binds. That is the whole reason — passing both axes is NOT a
/// letterbox hazard here, measured identically on three hosts by three seats; the
/// letterboxing librsvg really does perform belongs to `preserve_aspect_ratio = false`,
/// which this never passes.
///
/// `target` is the size to render AT, already capped by the caller — which is also the
/// caller that decided a re-render is worth doing at all (`zoomed == intrinsic` means
/// zoom 1.0, where a second decode buys nothing over the natural-size one). This
/// function used to re-derive both and re-test that condition, so the guard here was
/// unreachable and the sizing existed twice; one derivation feeds both the cache key and
/// the decode now, so they cannot disagree (QA round 2, F-DRY2-16).
///
/// Returns `None` on any loader failure, so an SVG the
/// scaled loader cannot handle still renders by the ordinary route rather than becoming
/// a broken-image placeholder. **A host with no SVG loader at all is that same path**:
/// `from_file` then fails too and the reader gets the usual placeholder, exactly as it
/// would have before this existed.
fn rasterize_vector(
    path: &std::path::Path,
    bytes: &[u8],
    target: Extent,
) -> Option<gtk::gdk::Texture> {
    #[cfg(all(test, feature = "memory-gates"))]
    crate::imagedecode::decode_probe::note_vector_rasterize();
    imagedecode::rasterize_vector_bytes(bytes, target.w, &path.display().to_string())
}

/// Fetch and decode a remote image, logging why it did not appear.
///
/// Split from [`load_texture`] because it has two distinct failure stages — the
/// fetch and the decode — and collapsing them into one `.ok()` is what made the
/// GVfs gap above invisible for as long as it was: the placeholder tooltip said
/// "Could not load image", which reads as *the bytes were not an image* when in
/// fact no request had been made (GTK4Rs/AP-292).
///
/// **Routed through [`super`].** A disclosure fold-toggle re-renders its document
/// into a scratch buffer to rebuild its offset maps, which walks every image tag
/// again — without a cache every toggle would re-run the fetch below for every
/// remote image in the document, freezing the UI each time.
/// [`super::get_or_fetch`] calls this closure only on an outright cache miss; a
/// hit or a live cached failure returns with no network access at all.
///
/// **A remote VECTOR image is deliberately NOT re-rendered for zoom**, though a local one
/// is ([`rasterize_vector`]), so it softens as it grows where a local SVG stays sharp.
/// The asymmetry is the cache's doing rather than an oversight: it is keyed by URL and
/// stores decoded textures, so making the target size part of the key turns every zoom
/// step into a cache miss — and a miss here is a synchronous network fetch on the main
/// thread. Trading a soft image for a per-zoom-step network round trip, on a path the
/// reader has to opt into via "Show Unsafe Images", is the worse bargain. Revisit only
/// with a size-aware cache that still fetches once (TDD 13.11).
///
/// **A remote animated image's bytes are retained by the cache entry itself**
/// (`super::AnimationHint::Remote`), unlike a local one: the only alternative on a
/// HIT would be a second network fetch, which is worse than the bytes staying
/// resident (bounded by `limits::MAX_REMOTE_IMAGE_BYTES` per entry, and counted in
/// the cache's own byte budget — see `imagecache`'s module doc). So this function
/// resolves `animation` identically whether the call was a hit or a miss, unlike
/// [`load_local`], which only recovers local bytes on the hit branch.
fn load_remote_texture(uri: &str) -> Option<LoadedImage> {
    let cached = super::get_or_fetch(uri, || {
        let bytes = match crate::imagefetch::fetch_image_bytes(uri) {
            Ok(bytes) => bytes,
            Err(err) => {
                log::warn!("remote image not fetched: {uri} ({err})");
                return None;
            }
        };
        // Through the same choke point local images go through: the dimension probe
        // runs BEFORE any decode (`imagefetch`'s byte cap bounds the transfer and says
        // nothing about what it expands to, F-SEC-206), and content sniffing routes
        // WebP/GIF/APNG to `richimg` — a remote animated image animates like a local
        // one.
        let decoded = imagedecode::decode(&bytes, uri)?;
        let hint = match decoded.animation {
            Some(anim_source) => {
                let key = format!("anim:remote:{uri}");
                super::AnimationHint::Remote(source::shared(&key, anim_source.bytes))
            }
            None => super::AnimationHint::Still,
        };
        Some((decoded.texture, hint))
    })?;
    let animation = match cached.animation {
        super::AnimationHint::Remote(bytes) => Some(bytes),
        super::AnimationHint::Still | super::AnimationHint::Local => None,
    };
    Some(LoadedImage::at_natural_size(cached.texture, animation))
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    /// **TDD 27.1, the red-before-fix demonstration.** On the pre-fix code,
    /// `decode_local_raster` only ever resolves animation bytes on a MISS — a HIT
    /// returns `LoadedImage::animation == None` unconditionally, so a re-render
    /// (theme switch, zoom, a fold toggle) freezes the animation. Every picture from
    /// the first render is dropped before the second load, so nothing keeps the
    /// bytes alive by accident — the second load's own hit-recovery is the only
    /// thing that can make this pass.
    #[gtktest::test]
    fn a_local_animation_survives_a_cache_hit() {
        crate::imagecache::reset_for_test();
        crate::animation::source::reset_for_test();
        let path = fixture("anim.webp");

        let first = load_texture(&ImageResolution::Local(path.clone()), 1.0)
            .expect("richimg decodes anim.webp on every host");
        assert!(
            first.animation.is_some(),
            "sanity: a fresh decode of an animated file must carry animation bytes"
        );
        drop(first); // every picture from the first render is gone

        let second = load_texture(&ImageResolution::Local(path), 1.0)
            .expect("a cache HIT must still return the texture");
        assert!(
            second.animation.is_some(),
            "TDD 27.1: a re-render (cache HIT) must still animate, not freeze to a \
             still image just because nothing decoded again"
        );
    }

    /// The other half of TDD 27.1: when a picture from the FIRST render is still
    /// alive, the second (hit) render must share its exact `Arc`, never re-read the
    /// file — proving the cheap path is actually taken, not merely "some bytes came
    /// back".
    #[gtktest::test]
    fn a_live_picture_s_animation_bytes_are_shared_on_a_hit() {
        crate::imagecache::reset_for_test();
        crate::animation::source::reset_for_test();
        let path = fixture("anim.webp");

        let first = load_texture(&ImageResolution::Local(path.clone()), 1.0)
            .expect("richimg decodes anim.webp on every host");
        let first_bytes = first.animation.clone().expect("animated fixture");
        // `first` is deliberately kept alive across the second load.

        let second = load_texture(&ImageResolution::Local(path), 1.0).expect("cache hit");
        let second_bytes = second.animation.expect("TDD 27.1: still animated on a hit");
        assert!(
            Arc::ptr_eq(&first_bytes, &second_bytes),
            "a live picture's bytes must be SHARED, not re-read from disk a second time"
        );
        drop(first);
    }

    /// A vector (SVG) image never animates, hit or miss — `load_local_vector`'s
    /// `AnimationHint::Still` must round-trip as `None`.
    #[gtktest::test]
    fn a_vector_image_never_carries_animation_bytes() {
        crate::imagecache::reset_for_test();
        let path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sdd/system-overview.svg");
        let loaded = load_texture(&ImageResolution::Local(path), 2.0).expect("SVG decode");
        assert!(loaded.animation.is_none());
    }

    /// TDD 13.11 / Finding 1's positive control: a real SVG must still re-render at
    /// the ZOOMED size, not merely decode without crashing (which
    /// `a_vector_image_never_carries_animation_bytes` above cannot tell apart from
    /// "fell back to the natural-size raster" — `animation.is_none()` is true either
    /// way). Proves content-sniffing still lets a genuine vector through to
    /// `rasterize_vector` after the admit-then-sniff-then-split rewrite.
    #[gtktest::test]
    fn a_real_svg_still_rerenders_sharply_at_zoom() {
        crate::imagecache::reset_for_test();
        let path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sdd/system-overview.svg");
        // This fixture's declared viewBox/width/height (sdd/system-overview.svg).
        let natural = Extent { w: 1000, h: 1190 };
        let loaded = load_texture(&ImageResolution::Local(path), 2.0).expect("SVG decode");
        let expected = cap_raster(zoomed_extent(natural.w, natural.h, 2.0));
        assert_eq!(
            loaded.texture.width(),
            expected.w,
            "TDD 13.11: a vector source must be RE-RASTERISED at the zoomed size, \
             not upscaled from a natural-size decode"
        );
        assert_eq!(loaded.texture.height(), expected.h);
    }

    /// TDD 13.11 and 2.23j: a small SVG, a UTF-16 one and an `.svgz` re-render at the
    /// zoomed size too. Each failed before, two ways: the vector probe read the format
    /// before closing its loader, which for any SVG shorter than gdk-pixbuf's sniff
    /// buffer was not yet known, so it answered "not scalable"; and a UTF-16 or gzip SVG
    /// had no loader at all.
    #[gtktest::test]
    fn small_and_encoded_svgs_rerender_at_zoom() {
        crate::imagecache::reset_for_test();
        let expected = cap_raster(zoomed_extent(40, 40, 2.0));
        for name in ["green.svg", "green-utf16le.svg", "green.svgz"] {
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/svgencodings")
                .join(name);
            let loaded = load_texture(&ImageResolution::Local(path), 2.0).expect(name);
            assert_eq!(
                (loaded.texture.width(), loaded.texture.height()),
                (expected.w, expected.h),
                "{name}: re-rendered at the zoomed size"
            );
        }
    }

    /// Finding 1 / TDD 2.23b: an animated WebP wearing a `.svg` name must still
    /// decode through `richimg` — proven the same way
    /// `a_vector_image_never_carries_animation_bytes` proves the opposite (a real
    /// vector never animates): only a richimg-owned decode carries animation bytes,
    /// so `animation.is_some()` here is proof the gdk-pixbuf vector loader chain was
    /// never reached, whatever the file was named.
    #[gtktest::test]
    fn a_richimg_format_wearing_a_scalable_extension_decodes_through_richimg() {
        crate::imagecache::reset_for_test();
        crate::animation::source::reset_for_test();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disguised.svg");
        std::fs::copy(fixture("anim.webp"), &path).expect("copy the WebP fixture");
        let loaded = load_texture(&ImageResolution::Local(path), 2.0)
            .expect("richimg must still decode a WebP wearing a .svg name");
        assert!(
            loaded.animation.is_some(),
            "TDD 2.23b: a richimg-owned format wearing a .svg name must decode via \
             richimg (which carries animation bytes), never via the gdk-pixbuf \
             vector loader chain (which never does)"
        );
    }

    /// TDD 2.23b, portable: mirrors
    /// `imagedecode::admission::tests::a_directory_named_like_an_image_is_refused_on_every_platform`
    /// — a directory is the one non-regular-file case every platform (Windows
    /// included, which has no FIFO) can produce, so it is the cross-platform proof
    /// that the vector branch admits before it decides anything from content.
    #[gtktest::test]
    fn a_directory_named_svg_is_refused_on_every_platform() {
        crate::imagecache::reset_for_test();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notregular.svg");
        std::fs::create_dir(&path).expect("make the directory");
        assert!(
            load_texture(&ImageResolution::Local(path), 2.0).is_none(),
            "TDD 2.23b: a directory named .svg must be refused, not decoded"
        );
    }

    /// TDD 2.23b: an oversized file named `.svg` must be refused before the read
    /// completes — `imagedecode::read_local`'s admission bounds the read to `cap + 1`
    /// bytes regardless of the file's on-disk length (a sparse file here, so this
    /// costs no real I/O either way).
    #[gtktest::test]
    fn an_oversized_file_named_svg_is_refused_before_the_read_completes() {
        crate::imagecache::reset_for_test();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bomb.svg");
        const BYTES_PER_MIB: u64 = 1024 * 1024;
        let cap = crate::config::config().images.local_file_limit_mib * BYTES_PER_MIB;
        let f = std::fs::File::create(&path).expect("create the sparse file");
        f.set_len(cap + 1).expect("extend it past the cap");
        drop(f);
        assert!(
            load_texture(&ImageResolution::Local(path), 2.0).is_none(),
            "TDD 2.23b: an oversized file named .svg must be refused"
        );
    }

    /// TDD 2.23b, the hang-guard demonstration: a FIFO named `.svg` must be refused
    /// before any vector/gdk-pixbuf call — never reached by asking the FILE NAME
    /// whether it looks like a vector (Finding 1's root cause). Opening a FIFO for
    /// read blocks forever with no writer, so this test is WATCHDOG-BOUNDED: a
    /// second, GTK-untouched thread (GTK is single-threaded — the `load_texture` call
    /// itself stays on this test's own thread, which `#[gtktest::test]` already
    /// serializes onto the one GTK-safe worker) just times the call and aborts the
    /// whole process — rather than letting the run hang — if it does not return.
    /// Under the FIXED code this never fires: admission refuses a FIFO from `stat`
    /// metadata alone, without ever calling `open()` on it.
    ///
    /// Never `#[cfg(unix)]` on the function itself — a cfg'd-out test is deleted, not
    /// skipped (POLICY § Unit tests) — so this prints a runtime `SKIPPED [TDD 2.23b]:`
    /// on a platform with no FIFO form (mirrors
    /// `imagedecode::admission::tests::a_fifo_named_gif_is_refused_and_does_not_block`).
    #[gtktest::test]
    fn a_fifo_named_svg_is_refused_and_does_not_hang() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            use std::sync::atomic::{AtomicBool, Ordering};

            crate::imagecache::reset_for_test();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("pipe.svg");
            let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
                .expect("path has no interior NUL");
            // SAFETY: `c_path` is a valid NUL-terminated string that outlives the
            // call; `mkfifo` only reads it.
            let rc = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
            assert_eq!(rc, 0, "mkfifo failed: {}", std::io::Error::last_os_error());
            assert!(
                std::fs::metadata(&path).unwrap().file_type().is_fifo(),
                "precondition: it is a FIFO"
            );

            let watchdog_armed = Arc::new(AtomicBool::new(false));
            let watchdog_armed_for_thread = Arc::clone(&watchdog_armed);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(5));
                if !watchdog_armed_for_thread.load(Ordering::SeqCst) {
                    eprintln!(
                        "TDD 2.23b WATCHDOG: loading a FIFO named .svg did not return \
                         within 5s — the vector branch is blocking on a FIFO open, i.e. \
                         it decided from the FILE NAME again instead of admitting \
                         content first. Aborting rather than hanging the run."
                    );
                    std::process::abort();
                }
            });

            // The call itself stays on THIS thread (the one `#[gtktest::test]`
            // already serializes GTK access onto) — the watchdog above never touches
            // GTK, it only measures elapsed time.
            let loaded = load_texture(&ImageResolution::Local(path), 2.0);
            watchdog_armed.store(true, Ordering::SeqCst);
            assert!(
                loaded.is_none(),
                "TDD 2.23b: a FIFO named .svg must be refused, not decoded"
            );
        }
        #[cfg(not(unix))]
        crate::testsymlink::skipped(
            "TDD 2.23b",
            "FIFO admission is unix-only: Windows named pipes are not filesystem \
             entries in the sense `std::fs::metadata` reports one",
        );
    }
}
