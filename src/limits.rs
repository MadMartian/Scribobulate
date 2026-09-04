//! Input limits — the project's single answer to "how big is too big?".
//!
//! A Markdown document is **untrusted content** (TDD 2.7). Every other input
//! gate in this crate reasons about what a document may *reference*
//! (`links::is_allowed_url`, `links::resolve_image`); this module reasons about
//! what a document may *cost*. Both are the same threat model seen from
//! different sides, and until this module existed only the first side had one.
//!
//! ## Where the rule lives, and why not here
//!
//! The **constraint** — every document read goes through
//! [`is_regular_file_within_limit`], never a comparison against these constants — is
//! stated in `sdd/POLICY.md` § Input limits, because it binds callers rather than this
//! module. The **reasoning** for why such a rule was needed at all is ScrAP-225.
//!
//! This comment used to argue that case at length, which duplicated both: a module
//! doc arguing for a policy is a second copy of the policy, and the copy is the one
//! that goes stale. What belongs here is only what a reader of *this file* needs.
//!
//! ## Every number here is measured, and says so
//!
//! This module is the single source of truth for the values, in the same sense
//! `scripts/coverage.sh` owns the coverage floor: they are not restated in POLICY,
//! because a second copy is how the first one silently stops matching.
//!
//! A limit chosen by intuition is indistinguishable from a limit chosen by
//! measurement once it is written down, and the intuitive one fails silently later.
//! Each constant below records how its value was arrived at, including the margin
//! over the threshold that was actually observed. When you change one, re-measure
//! rather than re-reason — the measurement procedure is in the doc comment precisely
//! so it can be repeated.

/// Maximum construct-nesting depth the copymap will build a char-precise tree
/// for. Beyond this a construct is recorded as one opaque node: selection and
/// copy still work over it (the whole construct's source is reproduced), they
/// are simply not character-precise *inside* it.
///
/// **The hazard.** `copymap::Builder::construct` recurses once per nesting
/// level. Nothing upstream bounds nesting: `pulldown_cmark` is iterative and
/// parses arbitrary depth happily (measured: 20 000 levels, 40 003 events, no
/// growth in stack use), so the parser hands us a depth no one has limited and
/// the recursion converts it directly into stack frames. A stack overflow is
/// **not** a catchable panic — the process aborts, taking every unsaved buffer
/// in every window with it (there is no `catch_unwind` on the app path, and
/// adding one would not help here anyway).
///
/// **Measured threshold** (Linux x86-64, debug, 2 MiB test-thread stack): a
/// document of `N` nested `>` characters builds successfully at N = 1050 and
/// overflows at N = 1100, i.e. ≈ 1.9 KiB of stack per level. That makes the
/// smallest document that can abort the process about **1.1 KiB** — a file so
/// small it arrives in a chat message. (An 8 MiB main-thread stack moves the
/// threshold to roughly 4× that; the cap is chosen against the *smallest* stack
/// the code can run on, not the most comfortable one.)
///
/// **Why 64.** It is ~16× under the worst-case measured threshold, and ~6× over
/// the deepest nesting any real document plausibly reaches — the renderer
/// already clamps list depth to [`crate::tags::MAX_LIST_DEPTH`] (6) and heading
/// level to 1..=6, so a document nested past 64 constructs is not a document
/// anyone wrote by hand.
///
/// **This one cap bounds six recursion sites, not one.** `wrap_span_node`,
/// `resolve_node`, `reconstruct`, `emits_leading_marker` and `debug_verify`'s
/// `walk` all recurse over the *built tree*, so a tree that cannot exceed this
/// depth cannot drive any of them past it either. Capping at construction is
/// what makes the consumers safe; capping in a consumer would not.
///
/// **And it does not bound the seventh** — say so, rather than let "six sites"
/// read as "all of them" (QA round 3, R3). `copymap::text_nodes` recursed over a
/// single Text event's *contents*, not over the built tree, so no construction
/// cap could reach it. It is no longer a recursion at all: the argument for the
/// cap here is that a depth driven by a document is a depth an attacker chooses,
/// and a depth driven by an upstream crate's tokenisation is one nobody here
/// owns — so that site peels in a loop instead. Its doc comment carries the
/// measurement. The rule to carry forward: when a cap is described as bounding
/// *N* sites, the enumeration is the claim, and a site left out of it needs a
/// sentence, not silence.
pub(crate) const MAX_NEST_DEPTH: usize = 64;

/// Maximum size of a document this application will load, in bytes.
///
/// Applies to the *file*, checked before the read, because the point is to
/// avoid the allocation rather than to notice it afterwards. A read that has
/// already succeeded has already done the damage.
///
/// **Why a cap exists at all.** The load path is synchronous and on the GTK main
/// thread, and the content is then held in three copies (the `String`, the
/// `GtkTextBuffer`, and the render products). So the memory cost is a multiple
/// of the file size and the UI is unresponsive for the whole of it.
///
/// **Why 64 MiB.** Comfortably beyond any Markdown document a person edits —
/// the largest plausible real input is a generated report in the low single-digit
/// megabytes — while still small enough that the three-copy cost stays bounded.
/// A limit that no legitimate document reaches is doing its job invisibly, which
/// is the intended state.
///
/// **A size cap is not the whole fix.** It does nothing about a *character
/// device* (`/dev/zero` reports no size and yields bytes forever) or a *FIFO*
/// (which blocks the reading thread indefinitely rather than growing). Those
/// need a file-*type* check, which is why [`is_regular_file_within_limit`] tests
/// both and callers must use it rather than comparing against this constant
/// themselves.
pub(crate) const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

/// Maximum number of bytes [`crate::imagefetch`] will read from a remote image
/// response before abandoning it.
///
/// **Why a cap exists at all.** A remote image is fetched only on the opt-in
/// "Show Unsafe Images" path, but the URL still comes out of an untrusted
/// document, and the fetch runs on the GTK main thread (ScrAP-34, its 34a half). Without a
/// cap, a `Content-Length`-less response that never ends is an unbounded
/// allocation *and* an unbounded freeze — the network's version of the character
/// device [`MAX_DOCUMENT_BYTES`] cannot see. The limit is enforced against the
/// bytes actually read, never against the advertised `Content-Length`, because
/// that header is written by the same party as the body.
///
/// **Why 16 MiB.** Measured against real inline document images rather than
/// guessed: a 1280 px photographic JPEG from Wikimedia is 583 KiB, and the badge
/// SVGs a README banner is usually made of are 0.8–5 KiB. 16 MiB is ~27× the
/// photographic case, so no image a document legitimately embeds comes near it,
/// while an endless response is cut off after a bounded allocation. Re-measure
/// rather than re-reason if this ever needs raising.
///
/// **What it does not bound.** Decoding, not transfer, is where an image bomb
/// pays off — a few MiB of PNG can decode to gigabytes of pixels, and that
/// exposure is identical for a local file, so it is not this constant's job and
/// is not addressed here (`GdkTexture` owns the decode either way).
pub(crate) const MAX_REMOTE_IMAGE_BYTES: usize = 16 * 1024 * 1024;

/// The largest DECODED raster a document image may expand to, in pixels.
///
/// **Decoding, not transfer, is where an image bomb pays off**, and the paragraph
/// above says so while bounding only the transfer — so a document carried the exposure
/// the project's own sprite path had already refused. MEASURED on this project's Linux
/// reference host, by `sprite`'s own measurement: a 20000×20000 single-colour PNG
/// compresses to 388 332 bytes — inside `MAX_REMOTE_IMAGE_BYTES` by a factor of forty
/// — and decoding it allocates ~1.2 GB, because neither `GdkTexture` nor `GdkPixbuf`
/// refuses on dimensions or offers a configurable limit. A local file has the identical
/// exposure and no byte cap at all in front of it.
///
/// **Why 8192×8192, and why it is not `MAX_SPRITE_PIXELS`.** A sprite is a tile or a
/// chip and its cap is sized to that vocabulary; a document image is a photograph or a
/// screenshot the author chose, and refusing one the reader can see in every other
/// viewer is a worse failure than the one this prevents. 67 M pixels is four times a
/// 40-megapixel camera's output and ~6× under the measured bomb, so no image a document
/// legitimately embeds comes near it. Re-measure rather than re-reason if this ever
/// needs raising.
pub(crate) const MAX_IMAGE_PIXELS: i64 = 8192 * 8192;

/// The largest raster the renderer will produce when it re-rasterises a VECTOR image
/// for zoom, in pixels.
///
/// **A different question from [`MAX_IMAGE_PIXELS`], which is why it is a different
/// number.** That cap refuses a hostile *input*; this one bounds an allocation the
/// application chooses to make on the user's behalf. An SVG rasterised at 3× costs nine
/// times the pixels of the same file at 1× — `sdd/system-overview.svg` (1000×1112) reaches
/// 10.0 M pixels ≈ 40 MB of ARGB32 at the top of the ladder, against a whole-application
/// baseline TECH.md measures at ~80 MiB for a document. Half the application's footprint
/// for one diagram is not a trade worth making silently.
///
/// **6 M pixels.** Chosen against the same yardstick as the image cache's own 32 MiB
/// budget, and one step below it: a single image may not cost more than the entire
/// remote-image cache is allowed to hold. It bounds SHARPNESS, never layout — an image
/// whose zoomed size exceeds this is still *drawn* at the size zoom asks for, from a
/// raster capped here, so the reader sees a slightly soft diagram rather than a small one
/// (TDD 13.11). At this budget the reference diagram re-rasterises at ~2.4× before the cap
/// binds, which is far past the point where the text inside it becomes readable.
///
/// **What it actually costs is ~3× the arithmetic**, and the figure is measured rather
/// than derived. 6 M pixels is 24 MB of ARGB32 and the obvious estimate stops there; a
/// process holding `sdd/system-overview.svg` grows **+74 MB** across the ladder to 300%.
/// The control says that is the re-render and not zoom in general: the same document with
/// a *raster* image of identical natural size grows +9 MB over the same steps. The gap is
/// the pixbuf and the texture copied from it both being live, plus the scaled surface GSK
/// caches. RSS is bounded rather than leaked — repeated ladder sweeps settle back below
/// the peak — and none of it is VRAM, since the Cairo renderer this project forces holds
/// no GPU memory (TDD §6). Re-measure rather than re-reason if this needs changing, and
/// measure RSS on a real document rather than multiplying pixels by four.
pub(crate) const MAX_VECTOR_RASTER_PIXELS: i64 = 6 * 1024 * 1024;

/// Is an image of `width`×`height` within [`MAX_IMAGE_PIXELS`]?
///
/// The arithmetic, separated from the two GTK probes that supply the dimensions, so the
/// rule is one statement and is unit-tested with no GTK type at all — the same split
/// `sprite::decoded_byte_size` makes for the cache's budget. A non-positive dimension
/// is admitted: it means the probe learned nothing, and refusing on an unknown is a
/// denial of service of its own.
pub(crate) fn image_pixels_within_cap(width: i32, height: i32) -> bool {
    if width <= 0 || height <= 0 {
        return true;
    }
    i64::from(width) * i64::from(height) <= MAX_IMAGE_PIXELS
}

/// Why a path was refused for loading. Distinct variants because the three
/// cases need different words in front of a user: an oversized document is the
/// user's document and they may reasonably be annoyed; a FIFO or device is
/// almost certainly not a document at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoadRefusal {
    /// A regular file, but larger than [`MAX_DOCUMENT_BYTES`].
    TooLarge { bytes: u64 },
    /// Not a regular file — a FIFO, socket, block/character device. Reading it
    /// would block forever or never end.
    NotARegularFile,
}

impl std::fmt::Display for LoadRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadRefusal::TooLarge { bytes } => write!(
                f,
                "document is {:.1} MiB, over the {:.0} MiB limit",
                *bytes as f64 / (1024.0 * 1024.0),
                MAX_DOCUMENT_BYTES as f64 / (1024.0 * 1024.0)
            ),
            LoadRefusal::NotARegularFile => {
                write!(f, "not a regular file (a pipe, socket or device)")
            }
        }
    }
}

/// The single admission test for "may this path be read as a document?".
///
/// Both halves are required and neither implies the other, which is exactly why
/// this is one function rather than a constant callers compare against:
///
/// * a **size** test alone passes a FIFO (`metadata().len()` is 0) and then
///   blocks the main thread forever on the read;
/// * a **type** test alone passes a 40 GiB regular file.
///
/// Deliberately takes already-obtained [`std::fs::Metadata`] rather than a path:
/// the caller is about to open the file anyway, and re-`stat`ing a path it will
/// then open by name is a TOCTOU seam this function has no business adding.
pub(crate) fn is_regular_file_within_limit(meta: &std::fs::Metadata) -> Result<(), LoadRefusal> {
    is_regular_file_within(meta, MAX_DOCUMENT_BYTES)
}

/// The same two-halves admission test against a **caller-supplied** byte cap.
///
/// Exists because a document is not the only untrusted file this application opens:
/// `crate::sprite` admits a theme-named image under its own, much smaller cap. That
/// caller reimplemented the size half and dropped the type half — the FIFO bug this
/// module's docs name verbatim — so the halves live here, once, and the cap is the
/// only thing a second caller supplies.
///
/// The returned [`LoadRefusal`]'s `Display` is worded for a **document**; a caller
/// passing a different cap matches the variant and words its own diagnostic.
pub(crate) fn is_regular_file_within(
    meta: &std::fs::Metadata,
    cap: u64,
) -> Result<(), LoadRefusal> {
    if !meta.is_file() {
        return Err(LoadRefusal::NotARegularFile);
    }
    if meta.len() > cap {
        return Err(LoadRefusal::TooLarge { bytes: meta.len() });
    }
    Ok(())
}

/// The deepest document measured to build successfully on the smallest (2 MiB)
/// stack this code runs on. Overflow was measured at 1100; this is the last
/// value that survived. It exists so the margin below can be checked rather than
/// asserted in prose.
const MEASURED_SAFE_DEPTH: usize = 1050;

/// [`MAX_NEST_DEPTH`] must keep a real margin under [`MEASURED_SAFE_DEPTH`], so
/// that raising the cap by feel cannot silently walk it back into abort range.
///
/// A `const` assertion rather than a `#[test]` deliberately: both operands are
/// compile-time constants, so a runtime test could never fail on a build that
/// compiled — it would be an assertion that cannot fail, which is exactly the
/// ScrAP-209 shape this round is about (clippy's `assertions_on_constants`
/// says the same thing and refused the test form). As a `const` assertion it
/// fails the BUILD, which is both stronger and honest about when the check
/// actually happens.
const _: () = assert!(
    MAX_NEST_DEPTH * 8 <= MEASURED_SAFE_DEPTH,
    "MAX_NEST_DEPTH is within 8x of the measured stack-overflow threshold. \
     Re-measure before raising it — MAX_NEST_DEPTH's doc comment says how."
);

#[cfg(test)]
mod tests {
    use super::*;

    /// Both sides of the size boundary, against the FILESYSTEM.
    ///
    /// The first version of this test wrote a one-byte file, asserted it was
    /// *accepted*, and then asserted the `Display` text of a hand-constructed
    /// `TooLarge` — so the branch at [`is_regular_file_within_limit`]'s size
    /// check was never executed by anything, in the module whose whole purpose
    /// is to be the single home of that decision (`ScrAP-221` — `ScrAP-209`'s shape
    /// inside the code that round wrote). Its stated excuse, that an oversized
    /// file is impractical to create, was simply false: `set_len` makes a
    /// **sparse** 64 MiB file instantly, allocating no blocks, on every
    /// filesystem this app runs on.
    #[test]
    fn an_oversized_regular_file_is_refused_and_says_by_how_much() {
        let dir = tempfile::tempdir().expect("tempdir");

        // Just over the limit: refused, and the refusal carries the real size.
        let big = dir.path().join("big.md");
        let f = std::fs::File::create(&big).expect("create");
        f.set_len(MAX_DOCUMENT_BYTES + 1).expect("set_len (sparse)");
        drop(f);
        let meta = std::fs::metadata(&big).expect("metadata");
        assert!(meta.is_file(), "precondition: a regular file, not a device");
        assert_eq!(
            meta.len(),
            MAX_DOCUMENT_BYTES + 1,
            "precondition: the sparse file really does report an over-limit size"
        );
        assert_eq!(
            is_regular_file_within_limit(&meta),
            Err(LoadRefusal::TooLarge {
                bytes: MAX_DOCUMENT_BYTES + 1
            })
        );
        assert_eq!(
            LoadRefusal::TooLarge {
                bytes: MAX_DOCUMENT_BYTES + 1
            }
            .to_string(),
            "document is 64.0 MiB, over the 64 MiB limit"
        );

        // Exactly AT the limit: accepted. The comparison is `>`, and a test that
        // only ever probes far from the boundary cannot tell `>` from `>=`.
        let edge = dir.path().join("edge.md");
        let f = std::fs::File::create(&edge).expect("create");
        f.set_len(MAX_DOCUMENT_BYTES).expect("set_len (sparse)");
        drop(f);
        let meta = std::fs::metadata(&edge).expect("metadata");
        assert_eq!(is_regular_file_within_limit(&meta), Ok(()));
    }

    /// The half a size check alone would miss. A FIFO's `len()` is 0, so a
    /// size-only gate admits it and the read then blocks forever.
    #[cfg(unix)]
    #[test]
    fn a_fifo_is_refused_even_though_its_size_is_zero() {
        use std::os::unix::fs::FileTypeExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("pipe");
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
            .expect("path has no interior NUL");
        // SAFETY: `c_path` is a valid NUL-terminated string that outlives the
        // call; `mkfifo` only reads it.
        let rc = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
        assert_eq!(rc, 0, "mkfifo failed: {}", std::io::Error::last_os_error());

        let meta = std::fs::metadata(&path).expect("metadata");
        assert!(meta.file_type().is_fifo(), "precondition: it is a FIFO");
        assert_eq!(
            meta.len(),
            0,
            "precondition: a FIFO reports size 0, which is why a size-only \
             check admits it"
        );
        assert_eq!(
            is_regular_file_within_limit(&meta),
            Err(LoadRefusal::NotARegularFile)
        );
    }

    /// F-SEC-206: the rule, without a GTK loader in sight.
    #[test]
    fn the_image_pixel_cap_admits_a_photograph_and_refuses_a_bomb() {
        // A 40-megapixel camera, and a 4K screenshot: both well inside.
        assert!(image_pixels_within_cap(7728, 5152));
        assert!(image_pixels_within_cap(3840, 2160));
        // The measured bomb: 400 M pixels, ~1.2 GB decoded.
        assert!(!image_pixels_within_cap(20000, 20000));
        // Exactly at the cap is admitted; one row past it is not.
        assert!(image_pixels_within_cap(8192, 8192));
        assert!(!image_pixels_within_cap(8192, 8193));
        // A probe that learned nothing must not become a refusal of its own.
        assert!(image_pixels_within_cap(0, 0));
        assert!(image_pixels_within_cap(-1, 5));
    }
}
