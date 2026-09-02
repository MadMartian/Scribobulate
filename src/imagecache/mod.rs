//! Process-wide, URL-keyed cache for decoded remote-image textures.
//!
//! ## Why this exists
//!
//! `renderer::start::load_remote_texture` fetches and decodes a remote image
//! synchronously on the GTK main thread (accepted for the opt-in "Show Unsafe
//! Images" path, ScrAP-34a). A disclosure fold-toggle re-renders its document
//! into a scratch buffer to rebuild its offset maps, which walks every image tag
//! again — so without a cache, toggling a fold would re-fetch every remote image
//! in the document on every toggle, freezing the UI each time. This module is
//! the fix: a hit returns the already-decoded texture with no network call at
//! all, and a cached failure returns `None` immediately instead of re-issuing a
//! doomed request. The eviction/TTL policy itself is pure and lives in
//! [`policy`], display-free and unit-tested there; this module is the thin GTK
//! wiring over it — the process-wide singleton, decoded-byte accounting, and the
//! entry point [`get_or_fetch`] that `renderer::start` calls.
//!
//! ## Shape
//!
//! One [`policy::Cache`] instance, keyed on the image URL, holds either a
//! decoded `gtk::gdk::Texture` (a hit — returned with no network access) or a
//! short-lived negative marker (a cached failure — see [`NEGATIVE_CACHE_TTL`]).
//! It is **process-wide**, not per-tab or per-window: two tabs, or two windows,
//! showing the same remote image fetch it once between them. GTK is
//! single-threaded (POLICY § Architecture rules "All GTK access on the main
//! thread"), so a `thread_local!` holds it without a lock, the same pattern
//! `theme.rs`'s active-theme cell uses for the same reason.
//!
//! Successful entries live for the session — there is no positive TTL and
//! nothing ever proactively expires one; the only way a decoded texture leaves
//! the cache is LRU eviction under [`IMAGE_CACHE_BUDGET_BYTES`].

mod policy;

use gtk::prelude::TextureExt;
use policy::Cache;
use std::cell::RefCell;
use std::time::{Duration, Instant};

/// Total decoded-pixel bytes the cache may hold before evicting the
/// least-recently-used entry. This bounds **system RAM under the Cairo software
/// renderer this project forces** (POLICY § Architecture rules) — a decoded
/// `GdkTexture` never touches the GPU, so it does not count against the 50 MiB
/// VRAM ceiling (TDD §6), but it is exactly the same "measure it, don't guess
/// it" discipline that ceiling is held to, so the number is chosen the same way:
/// `limits::MAX_REMOTE_IMAGE_BYTES`'s own doc comment measured a real 1280px
/// photographic JPEG at 583 KiB compressed; decoded to ARGB32 pixels that is
/// roughly 1280 × 853 × 4 ≈ 4.4 MiB (see [`decoded_byte_size`]). This budget
/// (32 MiB) therefore holds on the order of seven such images at once —
/// comfortably a whole document's worth of remote images — while staying well
/// under half of the ~80 MiB RAM TECH.md measures for a single document with
/// full-fidelity rendering, so a document with unusually many or large remote
/// images cannot double the application's own baseline footprint. Re-measure
/// rather than re-reason if this ever needs raising, per the same rule
/// `limits.rs` states for its own constant.
const IMAGE_CACHE_BUDGET_BYTES: usize = 32 * 1024 * 1024;

/// How long a failed fetch is remembered before the next disclosure toggle (or
/// any other re-render) is allowed to retry it. This is the number that stands
/// between the two failure modes negative caching exists to balance: with **no**
/// negative TTL, a dead URL is re-fetched synchronously on every re-render —
/// the exact freeze this cache exists to remove, since a fold toggle is exactly
/// such a re-render. With a **long** (or session-lasting) TTL, a transient
/// failure — a flaky host, a momentary DNS blip — reads as permanently broken
/// for the rest of the session, with no way for the reader to make it retry
/// short of restarting the app. ~60 seconds comfortably absorbs a burst of fold
/// toggles against the same document (each one re-renders synchronously, so a
/// human operating the UI cannot toggle faster than roughly once a second in
/// practice) while remaining short enough that a reader who waits a minute and
/// tries again sees the image reappear on its own. Operator-ratified value;
/// re-measure against real toggle cadence rather than re-guess if this ever
/// needs changing.
const NEGATIVE_CACHE_TTL: Duration = Duration::from_secs(60);

thread_local! {
    static CACHE: RefCell<Cache<gtk::gdk::Texture>> =
        RefCell::new(Cache::new(IMAGE_CACHE_BUDGET_BYTES, NEGATIVE_CACHE_TTL));
}

/// The decoded byte cost of a texture of `width` × `height` pixels: one ARGB32
/// word (4 bytes) per pixel, the format a `GdkTexture` decodes into under the
/// Cairo software renderer this project forces. Not exact for every source
/// pixel format `GdkTexture::from_bytes` might choose internally, but it is the
/// same order of magnitude for all of them, and it is the decoded size the byte
/// budget prunes against — never the compressed transfer size `imagefetch`'s own
/// cap already bounds separately. A pure `i32 -> usize` function (rather than
/// taking `&gtk::gdk::Texture` directly) so the arithmetic is unit-tested with
/// no GTK type at all.
fn decoded_byte_size(width: i32, height: i32) -> usize {
    (width.max(0) as u64 * height.max(0) as u64 * 4) as usize
}

/// Look up `uri` in the process-wide cache; call `fetch` only on an outright
/// miss (never on a hit, never during a live negative-cache window), and record
/// its outcome. `fetch` should perform the actual network GET + decode
/// (`renderer::start::load_remote_texture` supplies it) — this function owns
/// only whether that ever needs to happen.
pub(crate) fn get_or_fetch(
    uri: &str,
    fetch: impl FnOnce() -> Option<gtk::gdk::Texture>,
) -> Option<gtk::gdk::Texture> {
    // The borrow discipline lives in `policy::get_or_fetch` — it takes the `RefCell`
    // precisely so no borrow is held across `fetch`. See its doc comment.
    CACHE.with(|cell| {
        policy::get_or_fetch(cell, uri, Instant::now(), || {
            fetch().map(|texture| {
                let bytes = decoded_byte_size(texture.width(), texture.height());
                (texture, bytes)
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::decoded_byte_size;

    #[test]
    fn decoded_byte_size_is_four_bytes_per_pixel() {
        assert_eq!(decoded_byte_size(1280, 853), 1280 * 853 * 4);
    }

    #[test]
    fn decoded_byte_size_never_underflows_on_a_degenerate_dimension() {
        assert_eq!(decoded_byte_size(0, 100), 0);
        assert_eq!(decoded_byte_size(-1, 100), 0);
    }
}
