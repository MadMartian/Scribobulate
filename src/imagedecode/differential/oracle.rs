//! The judge: the decoder this platform really loads, asked by rendering.
//!
//! An input is decoded through the same gdk-pixbuf loader chain the app uses, with no
//! screen in front of it, and the verdict is whether the sentinel colour appears — which
//! only the core's fallback paints, and only once librsvg has processed the include
//! (`super::grammar`'s module doc). No model of librsvg decides anything here.
//!
//! **What the witness rests on**, MEASURED on librsvg 2.52.5 before this was built:
//! librsvg itself, not libxml2, processes `xi:include` (its own include and `parse`
//! handling is in the library; an include in any other namespace draws nothing), and it
//! resolves no relative or missing `href` from a stream with no base URL, so the
//! `xi:fallback` runs. It reads `data:` URIs with the `data-url` crate it vendors, which
//! took `; base64`, `;BASE64`, `;charset=x;base64` and a trailing space as base64, and
//! `%3Bbase64`, `;base64%20` and `;base64;x=y` as not. Another platform's librsvg may
//! differ; the run's control (`super::control`) is what proves the witness there.
//! gdk-pixbuf recognised no top-level gzip or UTF-16 SVG from a stream on that host, so
//! those layers are live only nested inside another document.

use super::grammar::SENTINEL;
use super::safety::Admitted;

/// The raster the oracle asks for. Small, so a mutated size attribute costs nothing, and
/// every generated document scales to fill it.
const RASTER_SIDE: i32 = 32;

/// How far a channel may sit from the sentinel and still count: resampling a nested
/// image moves an interior pixel by a unit or two, and no blend of the documents' other
/// paint (black, or nothing) with anything reaches the sentinel from elsewhere.
const CHANNEL_TOLERANCE: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    /// The decoder processed an include: the sentinel was drawn.
    Acted,
    /// It drew no sentinel, or did not decode the input at all.
    Inert,
}

/// The decoder's verdict on `input`, and the loader's error when it decoded nothing.
pub(super) fn judge(input: &Admitted) -> (Verdict, Option<String>) {
    let stream = gtk::gio::MemoryInputStream::from_bytes(&gtk::glib::Bytes::from(input.bytes()));
    // The oracle IS the unscreened loader chain; `Admitted` is what keeps an entity out.
    #[expect(clippy::disallowed_methods)] // the differential test's unscreened oracle
    let decoded = gtk::gdk_pixbuf::Pixbuf::from_stream_at_scale(
        &stream,
        RASTER_SIDE,
        RASTER_SIDE,
        true,
        gtk::gio::Cancellable::NONE,
    );
    match decoded {
        Ok(pixbuf) if shows_sentinel(&pixbuf) => (Verdict::Acted, None),
        Ok(_) => (Verdict::Inert, None),
        Err(err) => (Verdict::Inert, Some(err.to_string())),
    }
}

fn shows_sentinel(pixbuf: &gtk::gdk_pixbuf::Pixbuf) -> bool {
    let bytes = pixbuf.read_pixel_bytes();
    let channels = pixbuf.n_channels() as usize;
    let stride = pixbuf.rowstride() as usize;
    let width = pixbuf.width() as usize;
    let alpha = pixbuf.has_alpha();
    let near = |a: u8, b: u8| a.abs_diff(b) <= CHANNEL_TOLERANCE;
    bytes.chunks(stride).any(|row| {
        row.chunks_exact(channels).take(width).any(|px| {
            near(px[0], SENTINEL[0])
                && near(px[1], SENTINEL[1])
                && near(px[2], SENTINEL[2])
                && (!alpha || px[3] == u8::MAX)
        })
    })
}

/// The decoder that judged, for every failure message: the librsvg this process has
/// loaded, by the version it exports.
pub(super) fn decoder() -> String {
    crate::platform::loaded_svg_decoder_version().map_or_else(
        || "an SVG loader whose librsvg version could not be read".to_owned(),
        |v| format!("librsvg {v}"),
    )
}
