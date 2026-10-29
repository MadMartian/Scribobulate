//! The application's ONE choke point for turning encoded image bytes into a texture.
//!
//! Every decode site in this crate reads bytes and hands them to [`decode`] (or, for a
//! local file, [`read_local`] first): a Markdown image and a raw-HTML `<picture>`
//! (`renderer::start`), a remote image fetched over HTTP (same file), a theme sprite
//! (`sprite.rs`), and a PDF-export embed (`export::pdf`). Content is sniffed by
//! [`richimg::sniff`], never by file extension or MIME claim — a WebP saved with a
//! `.png` name still routes to `richimg`, and a `.webp`-named file that is really a
//! still PNG still routes to GTK — routing is by content, through one choke point.
//!
//! WebP, GIF and APNG go to [`richimg`], which has no GTK in it and cannot enter the
//! leaking gdk-pixbuf incremental-WebP path this module exists to make unreachable
//! (ScrAP-351 / ScrAP-352). Everything else — a still PNG, JPEG, BMP, … — goes to GTK's own
//! decoder, [`gtk::gdk::Texture::from_bytes`]. The SVG path (`imagecache::loader`'s
//! vector branch, `rasterize_vector`) is a different question — a **re-render at a
//! target size**, not a format this module routes — and stays on GTK/gdk-pixbuf
//! (ScrAP-343). It is admitted through [`read_local`] exactly like every other local
//! image before anything asks whether it is scalable, and that question is answered
//! from CONTENT by [`probe_vector_dimensions`] — never from the caller's file
//! extension, which is what let a FIFO block the main thread and a richimg-owned
//! format reach the gdk-pixbuf loader chain by wearing a `.svg` name (Finding 1 /
//! TDD 2.23b).
//!
//! **Every hand-off to gdk-pixbuf's loader chain is screened first** ([`screen`],
//! [`xinclude`]): the loader picks librsvg by content, and a librsvg with
//! CVE-2026-96889 corrupts memory on an SVG that uses XInclude, so on every platform
//! but macOS (whose bundle carries the fixed librsvg) such content is refused before
//! a loader sees a byte. The loader calls here take a `svgencoding::LoaderInput`,
//! which only a `screen::Screened` can become, and which only the screen hands out
//! (TDD 2.23c). [`svgencoding`] is also where a UTF-16 or gzip SVG, which no sniff
//! recognises, is handed to the SVG loader by name (TDD 2.23j).
//!
//! `clippy.toml`'s `disallowed-methods` bans SEVEN GTK/gdk-pixbuf entry points outside
//! this module — `Texture::from_bytes`/`from_file`/`from_filename`,
//! `Pixbuf::from_stream`/`from_file_at_scale`/`from_stream_at_scale` and
//! `PixbufLoader::new` — so a new decode site elsewhere fails to compile rather than
//! silently re-opening the leak.
//!
//! **There are now NO exemptions, and the two that used to be here are worth knowing
//! about because of how they read.** The vector branch kept
//! `Pixbuf::from_file_at_scale` on the argument that a scalable source must be decoded
//! AT A TARGET SIZE, and only the path-taking entry point accepts one. The first half
//! is true and still is; the second half was simply wrong — `from_stream_at_scale`
//! takes the identical size arguments from a byte stream. The cost of not checking was
//! a check-then-use seam: `imagecache::loader` admitted and content-sniffed the file,
//! then this re-opened it BY NAME, so the bytes rendered were never the bytes checked
//! (QA round 2, F-R2-3). The decode now lives here as [`rasterize_vector_bytes`] and
//! takes the admitted buffer. (The other exemption, `PixbufAnimation::from_file` in the
//! old `local_dimensions` probe, went away with that function.)
//!
//! The lesson generalises past this module: **an exemption justified by a capability is
//! only as good as the search for another way to get that capability**, and "only X can
//! do this" is a claim about the API surface that ages badly and is cheap to re-check.

mod admission;
mod decode;
// Test-only decode counter (Finding 2 / TDD 6.8), gated identically to the ONE call
// site that increments it (inside `decode::decode`) and to `memgate::gtk`, its only
// reader — see this module's own doc comment for why that keeps it out of every other
// build.
#[cfg(all(test, feature = "memory-gates"))]
pub(crate) mod decode_probe;
// The XInclude screen judged by the decoder it protects (TDD 2.23d–2.23i).
#[cfg(test)]
mod differential;
mod probe;
mod screen;
mod svgencoding;
mod xinclude;

pub(crate) use admission::read_local;
pub(crate) use decode::{
    decode, decode_pixbuf, memory_texture_from_frame, rasterize_vector_bytes, resample_nearest,
    richimg_limits, FramePixels,
};
pub(crate) use probe::{probe_dimensions, probe_pixel_size, probe_vector_dimensions};
#[cfg(test)]
pub(crate) use screen::{forget_for_test, told_for_test};
pub(crate) use screen::{
    on_refusal, report_shown_refusal, screen_refuses, svg_refused, take_unreported_refusals,
    RefusalTarget,
};
#[cfg(test)]
pub(crate) use xinclude::enforced as screens_xinclude;

// `Refusal` and `AnimationSource`/`DecodedImage` are named at every current call site
// only implicitly (`Err(refusal) => …`, `.map(|decoded| decoded.texture)`), so nothing
// in this tree yet spells `imagedecode::Refusal` or `imagedecode::AnimationSource` —
// but they are still part of this module's declared contract, and
// `AnimationSource` in particular is exactly what the playback core needs to
// import by name. Re-exported with an explicit
// allow rather than silently dropped, so the contract stays readable from one place.
#[allow(unused_imports)] // part of the contract; not yet named outside this module
pub(crate) use admission::Refusal;
#[allow(unused_imports)]
// part of the contract; the playback core is AnimationSource's first caller
pub(crate) use decode::{AnimationSource, DecodedImage};
