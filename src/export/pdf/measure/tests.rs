//! Layout tests for the PDF measurement pass.
//!
//! In their own file rather than an inline `#[cfg(test)] mod`, following
//! `copymap/tests.rs`: they outweigh the code they exercise, and a reader opening
//! `measure.rs` to change the measurement pass should meet the pass, not scroll past
//! seven hundred lines of fixtures to reach it.

use super::super::{draw_page, metrics_for};
use super::lay_out;
use crate::export::{doc, paginate, RenderOptions};
use gtk::cairo;

/// A Pango context from the default font map — **no display, no widget, no GTK
/// window**. That is what puts the layout and drawing halves of this sink inside
/// the coverage gate rather than resting on a human opening a viewer.
fn ctx() -> gtk::pango::Context {
    use gtk::pango::prelude::FontMapExt;
    pangocairo::FontMap::default().create_context()
}

fn theme() -> crate::theme::Theme {
    crate::theme::Themes::builtin().resolve("system")
}

/// The points a blockquote steps its content in by, under `t` — the bar plus the themed
/// gap between the bar and the quoted text.
///
/// Derived rather than written as a literal: the step was a flat `INDENT_PT` until
/// `blockquote_text_gap` reached this sink, and an assertion carrying its own copy of
/// the number could not have noticed that the key does nothing.
fn quote_step(t: &crate::theme::Theme) -> f64 {
    use crate::export::pdf::geometry::px_to_pt;
    px_to_pt(t.metrics.blockquote_bar_width) + px_to_pt(t.metrics.blockquote_text_gap)
}

fn palette(theme: &crate::theme::Theme) -> crate::palette::Palette {
    crate::palette::Palette::from_base(
        gtk::gdk::RGBA::WHITE,
        gtk::gdk::RGBA::BLACK,
        gtk::gdk::RGBA::BLACK,
        gtk::gdk::RGBA::new(0.2, 0.5, 0.9, 1.0),
        theme,
    )
}

const SAMPLE: &str = "# Title\n\nA paragraph with **bold** and `code`.\n\n\
    > quoted\n\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n\
    ```rust\nfn f() {}\n```\n\n---\n\nEnd.\n";

#[test]
fn a_document_lays_out_into_measured_fragments() {
    let t = theme();
    let d = doc::build(SAMPLE, &RenderOptions::default());
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    assert!(!laid.fragments.is_empty(), "nothing was laid out");
    assert_eq!(
        laid.fragments.len(),
        laid.lines.len(),
        "every fragment must have exactly one drawable line — the paginator \
         indexes both by the same number"
    );
    assert!(
        laid.fragments.iter().all(|f| f.height > 0.0),
        "a zero-height fragment would let a page hold unboundedly many"
    );
}

#[test]
fn a_heading_measures_taller_than_body_text_at_the_themes_scale() {
    // The theme's heading scale reaching the page, rather than a literal.
    let t = theme();
    let heading = lay_out(
        &doc::build("# H\n", &RenderOptions::default()),
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &t,
    );
    let body = lay_out(
        &doc::build("H\n", &RenderOptions::default()),
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &t,
    );
    assert!(
        heading.fragments[0].height > body.fragments[0].height,
        "heading {:?} was not taller than body {:?}",
        heading.fragments[0].height,
        body.fragments[0].height
    );
}

#[test]
fn a_long_paragraph_wraps_into_several_indivisible_fragments() {
    // Each wrapped line is its own fragment, which is what makes "a page break
    // never splits a line of text" structural rather than a rule to remember.
    let t = theme();
    let long = format!("{}\n", "word ".repeat(400));
    let laid = lay_out(
        &doc::build(&long, &RenderOptions::default()),
        &ctx(),
        200.0,
        PAGE_HEIGHT_PT,
        &t,
    );
    assert!(
        laid.fragments.len() > 5,
        "expected the paragraph to wrap, got {} fragments",
        laid.fragments.len()
    );
    assert!(
        laid.fragments[1..].iter().all(|f| f.space_before == 0.0),
        "only a block's FIRST line carries the inter-block gap"
    );
}

/// **TDD 25.12 for the PDF sink** — a local image is **drawn**, not described.
///
/// The defect this pins shipped: the sink emitted an italic `[image: alt]` note and
/// threw the decoded bytes away, so an exported PDF contained no image objects at
/// all. `pdfimages -list` on the artefact was empty, and the operator found it in a
/// PDF editor. Asserting the *fragment kind* is what makes it checkable here — the
/// broken version produced a perfectly good text fragment, so any assertion about
/// "did something get laid out" passed.
#[test]
fn a_local_image_becomes_a_drawn_fragment_not_a_text_note() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("pic.png"), png_4x4()).expect("write");
    let t = theme();
    let d = doc::build(
        "![a square](pic.png)\n",
        &RenderOptions {
            doc_dir: Some(dir.path().to_path_buf()),
            allow_unsafe_images: false,
        },
    );
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    assert!(
        laid.lines.iter().any(|l| l.is_image_for_test()),
        "the image was not laid out as a drawn fragment — it fell back to a note"
    );
    // …and nothing describes it in words instead.
    let text: String = laid
        .lines
        .iter()
        .filter_map(|l| l.layout_text_for_test())
        .collect();
    assert!(
        !text.contains("[image:"),
        "a drawable image must not also emit its placeholder note: {text:?}"
    );
}

#[test]
fn an_image_is_contained_to_the_column_and_the_page_and_never_upscaled() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("pic.png"), png_4x4()).expect("write");
    let t = theme();
    let d = doc::build(
        "![x](pic.png)\n",
        &RenderOptions {
            doc_dir: Some(dir.path().to_path_buf()),
            allow_unsafe_images: false,
        },
    );
    // A tiny image is never blown up to fill the column.
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    let h = laid
        .fragments
        .iter()
        .zip(&laid.lines)
        .find(|(_, l)| l.is_image_for_test())
        .map(|(f, _)| f.height)
        .expect("an image fragment");
    assert!(h <= 4.0, "a 4px image must not be upscaled, got {h}pt");

    // …and a narrow column scales it down rather than overflowing.
    let narrow = lay_out(&d, &ctx(), 2.0, PAGE_HEIGHT_PT, &t);
    let nh = narrow
        .fragments
        .iter()
        .zip(&narrow.lines)
        .find(|(_, l)| l.is_image_for_test())
        .map(|(f, _)| f.height)
        .expect("an image fragment");
    assert!(nh <= h, "a narrower column must not make the image taller");
}

#[test]
fn an_undecodable_image_falls_back_to_a_visible_note_rather_than_a_gap() {
    // A silent gap is the one outcome worth avoiding: the reader cannot tell an
    // image was expected. `doc::build` refuses to embed bytes it cannot sniff, so
    // this arrives as `Missing` and must still say something.
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("pic.png"), b"not an image at all").expect("write");
    let t = theme();
    let d = doc::build(
        "![broken](pic.png)\n",
        &RenderOptions {
            doc_dir: Some(dir.path().to_path_buf()),
            allow_unsafe_images: false,
        },
    );
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    let text = page_text(&laid);
    // **The note's actual SHAPE**, not merely "some text". `!text.trim().is_empty()`
    // is satisfied by any content at all, so a note replaced by something unrelated —
    // or by the paragraph that happened to follow — passed it.
    assert!(
        text.contains('[') && text.contains(']'),
        "the note must be bracketed, the way every other unrenderable construct is \
         announced on this page: {text:?}"
    );
    assert!(
        text.to_lowercase().contains("image"),
        "the note must say what is missing — a reader who cannot see an image cannot \
         infer that one was expected: {text:?}"
    );
    assert!(
        text.contains("pic.png"),
        "the note must name the file, so the reader can find what failed: {text:?}"
    );
}

#[test]
fn an_image_wrapped_in_a_link_is_still_drawn() {
    // `[![badge](b.png)](https://…)` — how every README status badge is written.
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("b.png"), png_4x4()).expect("write");
    let t = theme();
    let d = doc::build(
        "[![badge](b.png)](https://example.com)\n",
        &RenderOptions {
            doc_dir: Some(dir.path().to_path_buf()),
            allow_unsafe_images: false,
        },
    );
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    assert!(
        laid.lines.iter().any(|l| l.is_image_for_test()),
        "an image inside a link must still be drawn"
    );
}

/// A real 4×4 PNG — a decoder has to accept it, so a stub with only the magic
/// number would not exercise the path this is about.
fn png_4x4() -> Vec<u8> {
    png(4, 4, [0xFF, 0x40, 0x40])
}

/// A real, opaque `w`×`h` PNG in one flat colour.
///
/// Non-square and parameterised on purpose: a fixture whose width equals its height
/// cannot tell a transposed dimension from a correct one.
fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    fn chunk(tag: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        let body: Vec<u8> = tag.iter().chain(data).copied().collect();
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
        out
    }
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in bytes {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    // zlib stream, stored (uncompressed) blocks — no compressor needed.
    fn zlib(raw: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x01];
        out.push(0x01);
        out.extend_from_slice(&(raw.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(raw.len() as u16)).to_le_bytes());
        out.extend_from_slice(raw);
        let (mut a, mut b) = (1u32, 0u32);
        for &x in raw {
            a = (a + u32::from(x)) % 65521;
            b = (b + a) % 65521;
        }
        out.extend_from_slice(&((b << 16) | a).to_be_bytes());
        out
    }
    let mut ihdr = w.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB
    let mut raw = Vec::new();
    for _ in 0..h {
        raw.push(0); // filter: none
        raw.extend_from_slice(&rgb.repeat(w as usize));
    }
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &ihdr));
    png.extend(chunk(b"IDAT", &zlib(&raw)));
    png.extend(chunk(b"IEND", b""));
    png
}

#[test]
fn a_real_document_paginates_and_every_page_draws() {
    // The end-to-end headless proof: lay out, paginate, and draw every page onto a
    // real cairo surface. It is a positive control as much as a test — a drawing
    // path that silently did nothing would still let the other assertions pass.
    let t = theme();
    let p = palette(&t);
    let long = SAMPLE.repeat(20);
    let d = doc::build(&long, &RenderOptions::default());
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    let pages = paginate::paginate(&laid.fragments, &metrics_for(PAGE_HEIGHT_PT));
    assert!(
        pages.len() > 1,
        "expected several pages, got {}",
        pages.len()
    );

    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 612, 792)
        .expect("an image surface needs no display");
    let cr = cairo::Context::new(&surface).expect("a cairo context");
    for page in &pages {
        // The proof is deliberately dropped: this test asserts that every page
        // DRAWS without panicking, and has no promote gate to satisfy.
        let _drawn = draw_page(&cr, &laid, page.clone(), &p, &t, 54.0);
    }
    drop(cr);
    // Ink actually reached the surface: a draw path that no-opped would leave the
    // surface untouched, and every other assertion here would still pass.
    let data = surface.take_data().expect("surface data");
    assert!(
        data.iter().any(|&b| b != 0),
        "nothing was drawn — the page is entirely blank"
    );
}

/// Every x at which `rgb` appears on an ARgb32 surface, as a `(min, max)` pair, or
/// `None` if the colour is nowhere on it.
///
/// cairo's ARgb32 is premultiplied BGRA in native byte order, and at full alpha
/// premultiplication is the identity — so an opaque fill lands as its own bytes and a
/// literal comparison is exact rather than approximate.
fn extent_where(
    surface: cairo::ImageSurface,
    matches: impl Fn(u8, u8, u8) -> bool,
) -> Option<(usize, usize)> {
    let stride = surface.stride() as usize;
    let width = surface.width() as usize;
    let data = surface.take_data().expect("surface data");
    let mut extent: Option<(usize, usize)> = None;
    for row in data.chunks_exact(stride) {
        for (x, px) in row[..width * 4].chunks_exact(4).enumerate() {
            if px[3] == 0xff && matches(px[2], px[1], px[0]) {
                extent = Some(match extent {
                    None => (x, x),
                    Some((lo, hi)) => (lo.min(x), hi.max(x)),
                });
            }
        }
    }
    extent
}

/// [`extent_where`] for an exact colour — right for a FILL, which lands as its own
/// bytes, and wrong for a glyph, whose edges are antialiased against the page.
fn colour_extent(surface: cairo::ImageSurface, rgb: (u8, u8, u8)) -> Option<(usize, usize)> {
    let (r, g, b) = rgb;
    extent_where(surface, move |pr, pg, pb| pr == r && pg == g && pb == b)
}

/// Every x at which `rgb` appears, grouped by scanline — the row-resolved form of
/// [`colour_extent`], for an assertion about a fill's SHAPE rather than its bounding box.
///
/// A bounding box cannot see either half of the TDD 18.29 defect: three stacked
/// rectangles of differing widths and one continuous one have the same one.
fn colour_rows(surface: cairo::ImageSurface, rgb: (u8, u8, u8)) -> Vec<Vec<usize>> {
    let (r, g, b) = rgb;
    let stride = surface.stride() as usize;
    let width = surface.width() as usize;
    let data = surface.take_data().expect("surface data");
    data.chunks_exact(stride)
        .map(|row| {
            row[..width * 4]
                .chunks_exact(4)
                .enumerate()
                .filter(|(_, px)| px[3] == 0xff && (px[2], px[1], px[0]) == (r, g, b))
                .map(|(x, _)| x)
                .collect()
        })
        .collect()
}

/// The page box every layout test lays out against, in points — a US-Letter page
/// (612 × 792) inside a 72 pt margin. Two consts rather than one tuple, so a call site
/// reads `PAGE_WIDTH_PT, PAGE_HEIGHT_PT` and cannot transpose them; they were fourteen
/// unnamed literals, which is fourteen chances for one fixture to drift from the
/// assertions written against it.
const PAGE_WIDTH_PT: f64 = 468.0;
const PAGE_HEIGHT_PT: f64 = 684.0;

/// "Unmistakably magenta", the pixel predicate every ink assertion here uses.
///
/// A range rather than an exact `#ff00ff`: a glyph's edge pixels are antialiased
/// against the page, so an exact match tests the RASTERISER rather than the ink.
/// Nothing else any of these fixtures draws is remotely magenta.
fn magenta(r: u8, g: u8, b: u8) -> bool {
    r > 0x80 && b > 0x80 && g < 0x60
}

/// Every scanline carrying at least one fully-opaque pixel `matches` accepts.
///
/// The row-resolved sibling of [`extent_where`], for an assertion about which BAND of
/// the page an ink reached — the header row against the body rows, one tile's height
/// against a taller rule — where the x extent cannot tell them apart.
fn rows_where(surface: cairo::ImageSurface, matches: impl Fn(u8, u8, u8) -> bool) -> Vec<usize> {
    let stride = surface.stride() as usize;
    let width = surface.width() as usize;
    let data = surface.take_data().expect("surface data");
    data.chunks_exact(stride)
        .enumerate()
        .filter(|(_, row)| {
            row[..width * 4]
                .chunks_exact(4)
                .any(|px| px[3] == 0xff && matches(px[2], px[1], px[0]))
        })
        .map(|(y, _)| y)
        .collect()
}

/// The most fully-opaque pixels `matches` accepts on any one scanline.
///
/// Answers "how far across the page did this reach", which an x extent cannot: a
/// bounding box is the same for a tile drawn once at each end and for one tiled the
/// whole way.
fn widest_row_where(surface: cairo::ImageSurface, matches: impl Fn(u8, u8, u8) -> bool) -> usize {
    let stride = surface.stride() as usize;
    let width = surface.width() as usize;
    let data = surface.take_data().expect("surface data");
    data.chunks_exact(stride)
        .map(|row| {
            row[..width * 4]
                .chunks_exact(4)
                .filter(|px| px[3] == 0xff && matches(px[2], px[1], px[0]))
                .count()
        })
        .max()
        .unwrap_or(0)
}

/// Draw one document at `margin` onto a fresh white page and return the surface.
fn drawn_page(
    md: &str,
    t: &crate::theme::Theme,
    p: &crate::palette::Palette,
    margin: f64,
) -> cairo::ImageSurface {
    let d = doc::build(md, &RenderOptions::default());
    // Through `Paged`, the same entry point `window::export_pdf` uses. This harness used
    // to re-create the caller's wiring — lay out, paginate, then draw each range — which
    // meant every pixel test in this file could pass against a sequence production does
    // not perform (F-PAGINATE-001).
    let paged = crate::export::pdf::Paged::prepare(
        &d,
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        std::rc::Rc::new(t.clone()),
        margin,
    );
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 612, 792)
        .expect("an image surface needs no display");
    {
        let cr = cairo::Context::new(&surface).expect("a cairo context");
        // A white page under the ink. `draw_page` paints no background — a real PDF page
        // is white by being paper — so without this the surface stays transparent and
        // every glyph lands PREMULTIPLIED at partial coverage, which is why a colour
        // assertion over a bare surface reads glyph antialiasing rather than ink.
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().expect("a fill on a fresh surface");
        for index in 0..paged.page_count() {
            let _drawn = paged.draw(&cr, index, p);
        }
    }
    surface
}

/// TDD 18.29 / 25.3 — the quote panel reaches the PAGE, spanning the quote's own
/// column rather than the whole printable width, and is absent when unstated.
///
/// Asserted in pixels rather than on the theme key, because the sink can read a key
/// and draw nothing with it: this is the artefact half TDD 18.10's "verify by the
/// resolved position, never by the key having been read" asks for, in the medium §25
/// governs.
#[test]
fn a_quote_panel_fills_the_quoted_column_on_the_page() {
    const MARGIN: f64 = 54.0;
    const PANEL: (u8, u8, u8) = (0x0a, 0x18, 0x30);
    let md = "body line\n\n> quoted line\n";
    let base = theme();
    let p = palette(&base);

    assert_eq!(
        colour_extent(drawn_page(md, &base, &p, MARGIN), PANEL),
        None,
        "a theme that states no panel must put no panel on the page"
    );

    let mut panelled = theme();
    panelled.blockquote_bg = crate::theme::parse_color("#0a1830");
    let (lo, hi) = colour_extent(drawn_page(md, &panelled, &p, MARGIN), PANEL)
        .expect("the stated panel must reach the page");
    // Left edge: the quote's own indent, not the page margin — the page's reading of
    // the text column the preview's paragraph background is pinned to.
    // Within a pixel, not exactly: the quote's step is a themed metric converted from
    // design-time pixels, so its edge is no longer at an integral point and the first
    // FULLY OPAQUE column is the one after the antialiased boundary.
    let want = MARGIN + quote_step(&panelled);
    assert!(
        (lo as f64 - want).abs() <= 1.0,
        "the panel must start at the quote's indent ({want}pt), not at the page \
         margin — it starts at {lo}"
    );
    // Right edge: the printable column. `printable_width_pt` is measured from the
    // page's own width, so this is the same edge body text wraps at.
    assert!(
        hi >= (MARGIN + PAGE_WIDTH_PT) as usize - 1,
        "the panel must reach the printable edge, got {hi}"
    );
}

/// **TDD 2.11b on the page: a nested quote gets a bar per level, and the panel does NOT
/// nest with them.**
///
/// The fixture is quoted to depth 2 with **no depth-1 line of its own**, and that is the
/// whole trick. A document with outer-level text as well would place the panel's left
/// edge at the outer indent whatever the nested lines did, so the assertion would pass
/// against a per-level panel and prove nothing (ScrAP-132: a guard whose input cannot
/// exhibit the defect). With only depth-2 lines present, a panel drawn per level starts
/// one step further right, and the two answers are a whole `quote_step` apart.
///
/// Mutation check (measured): drawing the panel from `quote.indent` instead of the
/// root's moves the panel's left edge by exactly one step and fails the panel assert;
/// drawing only the innermost bar (`for level in 0..1`) moves the bar's left edge by one
/// step and fails the bar assert. Neither mutation touches the other assertion.
#[test]
fn a_nested_quote_bars_every_level_and_panels_only_the_outermost() {
    const MARGIN: f64 = 54.0;
    const PANEL: (u8, u8, u8) = (0x0a, 0x18, 0x30);
    const BAR: (u8, u8, u8) = (0xd2, 0x00, 0x7f);
    // Depth 2 throughout: no outer-level line to hide a per-level panel behind.
    let md = "body line\n\n> > doubly quoted line\n";

    let mut t = theme();
    t.blockquote_bg = crate::theme::parse_color("#0a1830");
    t.blockquote_bar_color = crate::theme::parse_color("#d2007f");
    let p = palette(&t);
    let step = quote_step(&t);

    let (panel_lo, _) = colour_extent(drawn_page(md, &t, &p, MARGIN), PANEL)
        .expect("the stated panel must reach the page");
    // The panel is laid down from the OUTERMOST indent (MARGIN + one step), but its
    // first VISIBLE column is one bar-width further in: the innermost level's bar is
    // painted over the panel's own left edge, exactly as the depth-1 bar always has
    // been. Stated as the sum rather than fudged into the tolerance, because the whole
    // discrimination here is one `step` wide and a tolerance big enough to absorb the
    // bar would also absorb the defect.
    let bar_w = crate::export::pdf::geometry::px_to_pt(t.metrics.blockquote_bar_width);
    let want_panel = MARGIN + step + bar_w;
    assert!(
        (panel_lo as f64 - want_panel).abs() <= 1.0,
        "the panel must be laid from the OUTERMOST quote's indent even where every line \
         is nested deeper — the background does not nest, an inner level inherits its \
         parent's fill (TDD 2.11b). Expected its first visible column at {want_panel}pt \
         (outermost indent {}pt, plus the {bar_w}pt bar painted over it); it starts at \
         {panel_lo}, which is one step further in and means a panel was drawn per level",
        MARGIN + step
    );

    let (bar_lo, _) = colour_extent(drawn_page(md, &t, &p, MARGIN), BAR)
        .expect("the stated bar colour must reach the page");
    // The outermost bar sits its own width plus the gap left of the outermost quote's
    // indent, and that indent is exactly one step: the two cancel, so it lands on the
    // page margin. A run that drew only the innermost bar would start one step right.
    assert!(
        (bar_lo as f64 - MARGIN).abs() <= 1.0,
        "every enclosing level must draw its bar on a nested line, so the LEFTMOST bar \
         is the outermost quote's, at the page margin ({MARGIN}pt) — a bar starting at \
         {bar_lo} means only the innermost level was drawn and the outer quote reads as \
         a hole in its own bar (TDD 2.11b)"
    );
}

/// TDD 18.29 regression — on the PAGE, a quote holding an intro paragraph, a nested
/// list and a closing paragraph panels as ONE rectangle, with no paper anywhere inside
/// the quote's own column.
///
/// Both halves failed, and each is invisible to the sibling test above (which quotes a
/// single line — the shape that renders correctly either way):
///
/// * **vertically**, the panel was drawn per LINE, so every `space_before` between the
///   quote's blocks showed the paper through;
/// * **horizontally**, it was drawn at each LINE's indent, so the nested list's rows
///   stepped `INDENT_PT` right of the paragraphs around them.
///
/// The oracle is *"no page colour inside the quote's column"*, which is the defect stated
/// as a property and catches both halves with one scan. The obvious alternative — assert
/// where each row's panel run STARTS — cannot be written: quoted text begins at that same
/// edge, and on a dense glyph row the antialiased ink covers the whole first `INDENT_PT`,
/// so a correct page fails it. Ink over the panel is fine; paper is the bug, and nothing
/// drawn here is white.
#[test]
fn a_quote_panel_leaves_no_paper_inside_the_quote_column() {
    const MARGIN: f64 = 54.0;
    const PANEL: (u8, u8, u8) = (0x0a, 0x18, 0x30);
    const PAPER: (u8, u8, u8) = (0xff, 0xff, 0xff);
    let md = "body line\n\n\
              > Intro paragraph\n>\n\
              > - item one\n> - item two\n>\n\
              > Closing paragraph\n\n\
              after\n";
    let mut panelled = theme();
    panelled.blockquote_bg = crate::theme::parse_color("#0a1830");
    let p = palette(&panelled);
    let rows = colour_rows(drawn_page(md, &panelled, &p, MARGIN), PANEL);

    let filled: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter_map(|(y, xs)| (!xs.is_empty()).then_some(y))
        .collect();
    let (first, last) = (
        *filled
            .first()
            .expect("the stated panel must reach the page"),
        *filled.last().expect("…and cover more than nothing"),
    );
    assert_eq!(
        last - first + 1,
        filled.len(),
        "the panel must be ONE continuous run over the whole quote — a gap between \
         rows {first} and {last} is the page showing between two of its blocks"
    );

    let left = (MARGIN + quote_step(&panelled)) as usize;
    let column = left..(MARGIN as usize + 468);
    let paper = colour_rows(drawn_page(md, &panelled, &p, MARGIN), PAPER);
    for (y, row) in paper.iter().enumerate().take(last + 1).skip(first) {
        assert!(
            !row.iter().any(|x| column.contains(x)),
            "row {y} shows the page inside the quote's own column — a nested list must \
             not walk the panel in from the paragraphs around it"
        );
    }
}

/// TDD 18.31 / 25.3 — the rule's sprite reaches the PAGE, tiled across the column, and
/// the rule's own line is made tall enough to hold a whole tile.
///
/// The height half is the one that would fail silently: `rule_space` is a gap around a
/// hairline and says nothing about a picture, so a rule line left at that height shows a
/// slice of the tile and looks like a rendering bug rather than a measurement one.
#[test]
fn a_rule_sprite_tiles_across_the_page_and_is_given_room_for_a_whole_tile() {
    const MARGIN: f64 = 54.0;
    let md = "before\n\n---\n\nafter\n";
    let base = theme();
    let p = palette(&base);

    assert_eq!(
        extent_where(drawn_page(md, &base, &p, MARGIN), magenta),
        None,
        "a theme stating no rule sprite must draw the flat rule and nothing else"
    );

    // A 2x6 magenta tile: wider than one pixel so "tiled" is falsifiable, and TALLER
    // than the shipped `rule_space` so the measured line has to grow to hold it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rule.png");
    std::fs::write(&path, png(2, 6, [255, 0, 255])).unwrap();
    let mut tiled = theme();
    tiled.sprites.rule = Some(crate::sprite::SpriteRef::File(path));
    assert!(
        f64::from(tiled.metrics.rule_space) < 6.0,
        "this fixture only tests the height fold while the tile is TALLER than \
         rule_space ({}) — pick a taller tile if that metric ever grows",
        tiled.metrics.rule_space
    );

    let rows = rows_where(drawn_page(md, &tiled, &p, MARGIN), magenta);
    // 5 or 6, not exactly 6: the rule's y is the running sum of Pango line heights in
    // points and is not an integer, so the band's top or bottom row is antialiased and
    // fails the fully-opaque filter above. The claim being made is that the line grew
    // past `rule_space` to hold a whole tile — at the un-grown height it would be 3-4.
    assert!(
        (5..=6).contains(&rows.len()),
        "the rule must be one whole tile tall — {} rows carry the tile, against a \
         6px tile and a rule_space of {}",
        rows.len(),
        tiled.metrics.rule_space
    );
    // Tiled ACROSS: a 2px tile drawn once would colour two columns, not the column.
    let widest = widest_row_where(drawn_page(md, &tiled, &p, MARGIN), magenta);
    assert!(
        widest as f64 >= PAGE_WIDTH_PT - 1.0,
        "the tile covered {widest} px of a {PAGE_WIDTH_PT}pt column — it was drawn once, not tiled"
    );
}

/// TDD 18.30 / 25.3 — the table header's ink reaches the PAGE, and only the header row.
///
/// This sink drew every cell in the body ink before the key existed — it read no header
/// colour of any kind — so the assertion that the BODY rows are untouched is the half
/// that says the new ink is scoped rather than a page-wide pen change.
#[test]
fn a_table_header_takes_its_own_ink_and_the_body_rows_do_not() {
    const MARGIN: f64 = 54.0;
    let md = "| head one | head two |\n|---|---|\n| body cell | another |\n";
    let base = theme();
    let p = palette(&base);

    assert_eq!(
        extent_where(drawn_page(md, &base, &p, MARGIN), magenta),
        None,
        "a theme stating no header ink must tint no cell"
    );

    let mut inked = theme();
    inked.table_head_fg = crate::theme::parse_color("#ff00ff");
    let surface = drawn_page(md, &inked, &p, MARGIN);
    // Row bands rather than an x extent: the header and the body rows share the same
    // columns, so only the y axis can tell them apart.
    let rows = rows_where(surface, magenta);
    assert!(
        !rows.is_empty(),
        "the stated header ink must reach the header row's glyphs"
    );
    // Every tinted row is inside ONE band — the header. A body row taking the ink too
    // would put a second, lower band in this list.
    let span = rows.last().unwrap() - rows.first().unwrap() + 1;
    assert_eq!(
        span,
        rows.len(),
        "the ink reached rows outside one contiguous band — a body row took the \
         header's colour"
    );
}

/// TDD 18.29 / 25.3 — the quote's INK reaches the page, and stops at the quote.
///
/// The stop is the assertion worth having. The ink is set on the cairo context rather
/// than into the cell's markup, and every other decoration in `draw_page` puts the body
/// pen back after itself — the text branch never had to before this, so an ink that
/// leaked would tint every line after the quote and nothing else in the suite would
/// notice.
#[test]
fn a_quoted_lines_ink_stops_at_the_quote() {
    const MARGIN: f64 = 54.0;
    // antialiased against the page, so an exact match tests the rasteriser's coverage
    // rather than the ink. Nothing else on this page is remotely magenta.
    let md = "> quoted line\n\nbody line after the quote\n";
    let base = theme();
    let p = palette(&base);

    assert_eq!(
        extent_where(drawn_page(md, &base, &p, MARGIN), magenta),
        None,
        "a theme that states no quote ink must not tint anything"
    );

    let mut inked = theme();
    inked.blockquote_fg = crate::theme::parse_color("#ff00ff");
    let (lo, hi) = extent_where(drawn_page(md, &inked, &p, MARGIN), magenta)
        .expect("the stated ink must reach the quoted glyphs");
    // The quote is indented; the body paragraph after it is not. So every pixel of the
    // ink must sit right of the page margin's own text column start — if the pen leaked
    // into the following paragraph, `lo` lands on the un-indented body line instead.
    assert!(
        lo >= (MARGIN + quote_step(&inked)) as usize,
        "the quote's ink reached something left of the quote's indent (lo = {lo}) — \
         the body pen was not put back after the quoted line"
    );
    assert!(hi > lo, "a single-pixel extent is not a drawn glyph run");
}

#[test]
fn hostile_markup_in_the_document_does_not_reach_pango_as_markup() {
    // A Pango parse failure renders the whole run EMPTY, silently (ScrAP-163), so
    // this is the difference between a page of text and a blank one.
    let t = theme();
    // The injected span rides inside a CODE SPAN, so it arrives as `Inline::Code`
    // text rather than as raw HTML. That distinction is the fixture's whole point:
    // bare `<span …>` in a document is `Event::InlineHtml`, which the export walk
    // drops by omission and which therefore never reaches Pango at all — a fixture
    // written that way tests the walk's sanitiser, not this sink's escaping.
    let d = doc::build(
        "A < B & C and `<span foreground='red'>x</span>`\n",
        &RenderOptions::default(),
    );
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    // **Assert on the TEXT, not on the line count.** The two assertions this replaces
    // were `!fragments.is_empty()` and `all(height > 0.0)`, and BOTH still hold with
    // `escape_pango` deleted: `set_markup` on a failed parse blanks the layout, and an
    // empty layout still reports `line_count() == 1` at the font's own row height. The
    // comment beside them stated the opposite premise, and that is where the false
    // confidence lived.
    let text = page_text(&laid);
    assert!(
        text.contains("A < B & C"),
        "the metacharacters must survive as TEXT: {text:?}"
    );
    assert!(
        text.contains("<span foreground='red'>x</span>"),
        "the injected span must survive as inert CONTENT, character for character — \
         if it is gone, Pango consumed it as markup: {text:?}"
    );
    assert!(!laid.fragments.is_empty(), "the run was dropped entirely");
    assert!(
        laid.fragments.iter().all(|f| f.height > 0.0),
        "a zero-height line is what a failed markup parse leaves behind"
    );
}

#[test]
fn an_empty_document_lays_out_to_nothing_rather_than_panicking() {
    let t = theme();
    let laid = lay_out(
        &doc::build("", &RenderOptions::default()),
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &t,
    );
    assert!(laid.fragments.is_empty());
    assert!(paginate::paginate(&laid.fragments, &metrics_for(PAGE_HEIGHT_PT)).is_empty());
}

#[test]
fn an_annotation_reaches_the_page_with_its_comment() {
    let t = theme();
    let d = doc::build(
        "The {==claim==}{>>reviewer says this<<} here.\n",
        &RenderOptions::default(),
    );
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    let text: String = laid
        .lines
        .iter()
        .filter_map(|l| l.layout_text_for_test())
        .collect();
    assert!(text.contains("claim"), "{text:?}");
    assert!(text.contains("reviewer says this"), "{text:?}");
}

/// Every table row in `laid`, as (column x-offsets, is_head, scale).
fn table_rows(laid: &super::Laid) -> Vec<(Vec<String>, bool, f64)> {
    laid.lines
        .iter()
        .filter_map(|line| match &line.kind {
            super::LineKind::TableRow {
                columns,
                is_head,
                scale,
                ..
            } => Some((
                columns.iter().map(|c| format!("{:.3}", c.x)).collect(),
                *is_head,
                *scale,
            )),
            _ => None,
        })
        .collect()
}

const TABLE: &str = "| Username | Date/Time | Method |\n\
    |---|---|---|\n\
    | Conrad | Aug 19, 2026 10:29 PM | DM |\n\
    | Gifny Richata - ORAY STUDIOS | Aug 20, 2026 8:41 PM | FR |\n";

#[test]
fn every_row_of_a_table_shares_one_column_grid() {
    // THE regression. Rows used to be tab-joined paragraphs, so a cell one
    // character too wide pushed the next column a whole tab stop right and the
    // columns zig-zagged down the page — measured on a real export as column two
    // starting at x=120 on one row, 144 on the next and 216 on the last.
    let laid = lay_out(
        &doc::build(TABLE, &RenderOptions::default()),
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &theme(),
    );
    let rows = table_rows(&laid);
    assert_eq!(rows.len(), 3, "expected a header and two body rows");
    let (first_grid, _, _) = &rows[0];
    for (grid, _, _) in &rows {
        assert_eq!(
            grid, first_grid,
            "a row was laid on a different grid: {rows:?}"
        );
    }
    assert_eq!(first_grid.len(), 3, "the delimiter row declared 3 columns");
}

#[test]
fn a_table_row_is_exactly_one_indivisible_fragment() {
    // TDD 25.16 for tables: one row is one fragment, so a page break can only ever
    // fall BETWEEN rows. A wrapped cell must not become a second fragment.
    let wide = "| A | B |\n|---|---|\n| short | ".to_string()
        + &"a long sentence that has to wrap several times ".repeat(6)
        + "|\n";
    let laid = lay_out(
        &doc::build(&wide, &RenderOptions::default()),
        &ctx(),
        200.0,
        PAGE_HEIGHT_PT,
        &theme(),
    );
    assert_eq!(
        table_rows(&laid).len(),
        2,
        "a wrapped cell split its row into extra fragments"
    );
    assert_eq!(
        laid.fragments.len(),
        laid.lines.len(),
        "fragments and lines must stay index-parallel"
    );
}

#[test]
fn a_tables_header_keeps_company_with_its_first_row() {
    let laid = lay_out(
        &doc::build(TABLE, &RenderOptions::default()),
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &theme(),
    );
    let head = laid
        .lines
        .iter()
        .position(|l| matches!(&l.kind, super::LineKind::TableRow { is_head: true, .. }))
        .expect("no header row");
    assert!(
        laid.fragments[head].keep_with_next,
        "a header orphaned at the foot of a page is the one break that matters"
    );
}

#[test]
fn a_table_too_wide_to_wrap_is_scaled_to_fit_the_page() {
    // TDD 25.17. Ten columns of one long unbreakable word cannot be wrapped into
    // the page, so the whole table is scaled — never clipped at the margin.
    let header = "| ".to_string() + &"Col | ".repeat(10) + "\n";
    let delim = "|".to_string() + &"---|".repeat(10) + "\n";
    let body = "| ".to_string() + &"Supercalifragilistic | ".repeat(10) + "\n";
    let laid = lay_out(
        &doc::build(&(header + &delim + &body), &RenderOptions::default()),
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &theme(),
    );
    let rows = table_rows(&laid);
    assert!(!rows.is_empty(), "no table was laid out");
    for (_, _, scale) in &rows {
        assert!(
            *scale < 1.0,
            "an unwrappable table was left unscaled at {scale}"
        );
        assert!(*scale > 0.0, "a non-positive scale draws nothing");
    }
}

#[test]
fn a_narrow_table_is_scaled_by_nothing_at_all() {
    let laid = lay_out(
        &doc::build(
            "| a | b |\n|---|---|\n| 1 | 2 |\n",
            &RenderOptions::default(),
        ),
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &theme(),
    );
    for (_, _, scale) in table_rows(&laid) {
        // EXACT on purpose, not a tolerance oversight: `fit` answers a literal `1.0`
        // for a table that fits, and `draw_table_row` skips its transform on exactly
        // that value (`scale != 1.0`). A tolerance here would stop pinning the branch
        // the drawing pass actually takes.
        assert_eq!(scale, 1.0, "a table that fits was scaled");
    }
}

#[test]
fn a_cells_text_reaches_the_page_through_a_real_layout() {
    // Content, not call-shape: the cell's own characters must be in a layout, or
    // the artefact is a picture of a table (TDD 25.18).
    let laid = lay_out(
        &doc::build(TABLE, &RenderOptions::default()),
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &theme(),
    );
    let mut seen = String::new();
    for line in &laid.lines {
        if let super::LineKind::TableRow { cells, .. } = &line.kind {
            for cell in cells {
                seen.push_str(&cell.layout.text());
                seen.push(' ');
            }
        }
    }
    for want in [
        "Username",
        "Conrad",
        "Aug 19, 2026 10:29 PM",
        "Gifny Richata - ORAY STUDIOS",
    ] {
        assert!(seen.contains(want), "{want:?} never reached a cell layout");
    }
}

#[test]
fn a_column_alignment_reaches_the_cells_layout() {
    // The delimiter row's alignments used to be parsed and then discarded.
    let src = "| L | C | R |\n|:---|:---:|---:|\n| a | b | c |\n";
    let laid = lay_out(
        &doc::build(src, &RenderOptions::default()),
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &theme(),
    );
    let row = laid
        .lines
        .iter()
        .find_map(|l| match &l.kind {
            super::LineKind::TableRow {
                cells,
                is_head: false,
                ..
            } => Some(cells),
            _ => None,
        })
        .expect("no body row");
    let got: Vec<gtk::pango::Alignment> = row.iter().map(|c| c.layout.alignment()).collect();
    assert_eq!(
        got,
        vec![
            gtk::pango::Alignment::Left,
            gtk::pango::Alignment::Center,
            gtk::pango::Alignment::Right,
        ]
    );
}

#[test]
fn the_water_fill_rule_reaches_the_page_and_not_only_the_grid() {
    // A wide prose column and two short ones, squeezed: the short columns keep
    // roughly their natural width and the prose column absorbs the squeeze. The
    // rule was pinned only by direct `fit` calls; nothing exercised it through
    // `Layouter::table`, which is the path that ships.
    //
    // MEASURED, and it corrects the review that prompted this: the finding argued
    // that swapping a column's two measurements "still produces a plausible grid"
    // with floors and wants exchanged. It does not reach the grid at all. `fit`
    // clamps each floor with `.min(natural.max(MIN_COLUMN_PT))`, and since a
    // max-content width is never below a min-content one, a transposed pair
    // normalises straight back to the same floors — verified by transposing the
    // assignment below and watching every export test, this one included, stay
    // green. So the argument-order hazard `ColumnWant` removes was real and the
    // *consequence* attributed to it was not; this test pins the rule rather than
    // pretending to detect a swap it cannot see.
    let md = "| A very long prose column that wants a great deal of horizontal room indeed | Yes | No |\n\
              |---|---|---|\n\
              | More long prose here, again wanting plenty of width to lay itself out | Yes | No |\n";
    let laid = lay_out(
        &doc::build(md, &RenderOptions::default()),
        &ctx(),
        200.0,
        PAGE_HEIGHT_PT,
        &theme(),
    );
    let row = laid
        .lines
        .iter()
        .find_map(|line| match &line.kind {
            super::LineKind::TableRow { columns, .. } => Some(columns.clone()),
            _ => None,
        })
        .expect("no table row");
    assert_eq!(row.len(), 3);
    let prose = row[0].text_width;
    let short = row[1].text_width.max(row[2].text_width);
    assert!(
        prose > short * 2.0,
        "the prose column should dominate; got prose={prose} short={short} \
         (converging widths mean natural and minimum were swapped)"
    );
}

#[test]
fn a_table_indented_past_the_page_still_lands_on_it() {
    // No test anywhere passed a non-zero indent to `Layouter::table`, which is how
    // the missing clamp survived: `indent` grows INDENT_PT per nesting level with
    // nothing bounding it against the page, so 26 nested quotes on a 468pt page
    // hand the grid a printable width of zero. `fit` then returned a scale of 0.0
    // or below, and `draw_table_row` SKIPS a non-positive transform — so the row
    // drew unscaled, off the page, through the code written to honour TDD 25.17.
    let width = PAGE_WIDTH_PT;
    let prefix = format!("{} ", ">".repeat(30));
    let quoted: String = TABLE
        .lines()
        .map(|line| format!("{prefix}{}\n", line.trim_start()))
        .collect();
    let laid = lay_out(
        &doc::build(&quoted, &RenderOptions::default()),
        &ctx(),
        width,
        PAGE_HEIGHT_PT,
        &theme(),
    );
    let mut rows = 0;
    for line in &laid.lines {
        if let super::LineKind::TableRow {
            columns,
            scale,
            chrome,
            ..
        } = &line.kind
        {
            rows += 1;
            assert!(
                *scale > 0.0 && *scale <= 1.0,
                "scale {scale} outside the (0, 1] the drawing pass relies on"
            );
            let right = line.indent
                + columns
                    .last()
                    .map(|c| (c.x + c.box_width + chrome.border) * scale)
                    .unwrap_or(0.0);
            assert!(
                right <= width + 0.01,
                "table ran {right}pt past a {width}pt page"
            );
        }
    }
    assert!(rows > 0, "the fixture produced no table rows to check");
}

#[test]
fn a_table_never_reaches_past_the_printable_width() {
    // The margin is the contract: scaled or wrapped, the drawn table ends inside
    // it. Swept across widths so the three regimes are all exercised.
    for width in [80.0, 140.0, 300.0, PAGE_WIDTH_PT] {
        let laid = lay_out(
            &doc::build(TABLE, &RenderOptions::default()),
            &ctx(),
            width,
            PAGE_HEIGHT_PT,
            &theme(),
        );
        for line in &laid.lines {
            if let super::LineKind::TableRow {
                columns,
                scale,
                chrome,
                ..
            } = &line.kind
            {
                let right = columns
                    .last()
                    .map(|c| (c.x + c.box_width + chrome.border) * scale)
                    .unwrap_or(0.0);
                assert!(
                    right <= width + 0.01,
                    "table ran {right}pt past a {width}pt page"
                );
            }
        }
    }
}

/// TDD 18.25 / 25.3 — a banded heading level reaches EVERY line of the heading, and only
/// the levels the theme bands.
///
/// Per line rather than per paragraph because that is the unit this sink paginates in:
/// consecutive lines produce abutting rects, which is one continuous band for a heading
/// that wrapped — so a wrapped heading whose second row carried no band would show as a
/// band that stops half-way, and that is what this asserts against.
#[test]
fn a_banded_heading_carries_its_band_on_every_line_it_occupies() {
    let mut t = theme();
    t.heading_band.fills[0] = Some(gtk::gdk::RGBA::new(0.2, 0.4, 0.6, 1.0));
    // Long enough to wrap in a narrow column, at h1's scale.
    let d = doc::build(
        "# a deliberately long heading that will not fit on one line at this width\n\n\
         ## an unbanded second level\n\nbody\n",
        &RenderOptions::default(),
    );
    let laid = lay_out(&d, &ctx(), 200.0, PAGE_HEIGHT_PT, &t);
    let banded = laid.lines.iter().filter(|l| l.fill.is_some()).count();
    assert!(
        banded > 1,
        "the h1 wrapped but only {banded} line(s) carry the band — a band that stops \
         half-way down its own heading"
    );
    // The h2 and the body carry none: the fill is per level, and only h1 is stated.
    let unbanded = laid.lines.len() - banded;
    assert!(
        unbanded > 0,
        "every line was banded, including the h2 and the body"
    );

    // And with no fill stated at all, nothing is banded — the System case (18.2).
    let plain = lay_out(&d, &ctx(), 200.0, PAGE_HEIGHT_PT, &theme());
    assert!(plain.lines.iter().all(|l| l.fill.is_none()));
}

/// TDD 18.26 — the `list_depth` counter this sink threads actually COUNTS: a bullet three
/// levels down reaches the deepest tier, and one at the top reaches the first.
///
/// Asserted on the laid-out page rather than on `list_marker_markup` in isolation,
/// because the thing that can be wrong here is the THREADING — `decide.rs`'s arms are
/// unit-tested against a depth handed to them, and a walk that always hands them `1`
/// would pass every one of those tests while painting one colour down the whole list.
#[test]
fn a_nested_bullet_reaches_its_own_depth_tier_through_the_layout_walk() {
    let mut t = theme();
    let mut themes = crate::theme::Themes::builtin();
    themes.merge_over_for_test(
        "[themes.tiered]\nlist_marker_color = \"#111111\"\nlist_marker_color_2 = \"#222222\"\n\
         list_marker_color_3 = \"#333333\"\n",
    );
    let tiered = themes.resolve("tiered");
    t.list_marker_color = tiered.list_marker_color;
    t.list_bullet_colors = tiered.list_bullet_colors;

    let d = doc::build(
        "- one\n    - two\n        - three\n            - four\n",
        &RenderOptions::default(),
    );
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    // Read the colours back off the LAYOUT's attributes rather than off a markup string:
    // this asserts that the span survived `set_markup` and landed on a real run, which
    // is a stronger claim than "the string we built contained a hex code".
    let colours: Vec<String> = laid
        .lines
        .iter()
        .filter_map(|l| match &l.kind {
            crate::export::pdf::LineKind::Text { layout, .. } => Some(layout.clone()),
            _ => None,
        })
        .filter_map(|layout| {
            let attrs = layout.attributes()?;
            attrs
                .attributes()
                .into_iter()
                .find_map(|a| a.downcast::<gtk::pango::AttrColor>().ok())
                .map(|c| {
                    let c = c.color();
                    format!(
                        "#{:02x}{:02x}{:02x}",
                        c.red() >> 8,
                        c.green() >> 8,
                        c.blue() >> 8
                    )
                })
        })
        .collect();
    assert_eq!(
        colours,
        vec!["#111111", "#222222", "#333333", "#333333"],
        "each nesting level must read its own tier, and level 4 shares level 3's"
    );
}

/// TDD 18.25's padding fix, in the sink where the inset and the band are computed from
/// the same number and have to move in OPPOSITE directions: the text goes in, the band
/// stays on the printable column both other renderings match against.
///
/// Asserted on the two edges together, because getting one right and the other wrong is
/// the failure — insetting the text without backing the band out shrinks the band, and
/// backing the band out without insetting the text is the bug.
#[test]
fn a_banded_headings_text_is_inset_while_its_band_keeps_the_column() {
    let mut t = theme();
    let mut themes = crate::theme::Themes::builtin();
    themes.merge_over_for_test(
        "[themes.banded]\nheading_band_color_h1 = \"#334455\"\n\
         heading_band_padding = 16\n",
    );
    let banded = themes.resolve("banded");
    t.heading_band = banded.heading_band;
    t.metrics.heading_band_padding = banded.metrics.heading_band_padding;

    let d = doc::build("# banded\n\n## plain\n", &RenderOptions::default());
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    let banded_line = laid
        .lines
        .iter()
        .find(|l| l.fill.is_some())
        .expect("the h1 carries a band");
    // The TEXT sits one padding in from the column — in POINTS, converted from the
    // theme's design-time pixels. Asserting `16.0` here was asserting the unit error:
    // a `heading_band_padding = 16` drew a 16 pt inset on the page beside a 16 px one
    // on screen, 33% over, while the table borders next to it converted correctly
    // (F-METRIC-001).
    let pad_pt = crate::export::pdf::geometry::px_to_pt(16);
    assert_eq!(banded_line.indent, pad_pt);
    // …and the band backs out to the column itself, so its extent is unchanged from a
    // theme that states no padding at all.
    assert_eq!(banded_line.fill.as_ref().unwrap().padding, pad_pt);
    // No third `indent - padding == 0.0` assertion: it is ENTAILED by the two above
    // (both are `pad_pt`), so it can never fail on its own and reads as a check that
    // is not one.

    // The unbanded h2 is untouched: same indent it had before any of this.
    let plain = laid
        .lines
        .iter()
        .find(|l| l.fill.is_none() && matches!(l.kind, crate::export::pdf::LineKind::Text { .. }))
        .expect("the h2 and the body carry none");
    assert_eq!(plain.indent, 0.0);
}

/// **TDD 18.25 / 25.3 — a heading band reaches the PAGE, and a band SPRITE replaces
/// its fill rather than painting over it.**
///
/// `a_banded_heading_carries_its_band_on_every_line_it_occupies` asserts on
/// `line.fill.is_some()`, which is a claim about the LAYOUT: deleting the whole
/// `if let Some(band)` block in `ink.rs` — the code that actually puts the band on the
/// page — left the suite green. This asserts on pixels, so it cannot.
#[test]
fn a_heading_band_reaches_the_page_and_a_sprite_replaces_its_fill() {
    const MARGIN: f64 = 54.0;
    const FILL: (u8, u8, u8) = (0x33, 0x66, 0x99);
    let md = "# a banded heading\n\nbody\n";
    let p = palette(&theme());

    // The control: an unbanded theme puts neither the fill nor a tile on the page, so
    // the assertions below are about the band and not about some other ink.
    assert_eq!(
        colour_extent(drawn_page(md, &theme(), &p, MARGIN), FILL),
        None
    );

    let mut flat = theme();
    flat.heading_band.fills[0] = Some(gtk::gdk::RGBA::new(0.2, 0.4, 0.6, 1.0));
    let (lo, hi) = colour_extent(drawn_page(md, &flat, &p, MARGIN), FILL)
        .expect("a banded heading must put its fill on the page");
    assert!(
        hi - lo > 300,
        "the band must span the printable column, not a fragment of it ({lo}..{hi})"
    );

    // A sprite outranks the fill, and REPLACES it: the flat colour must be gone, not
    // sitting under a tile that happens to be opaque.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("band.png");
    std::fs::write(&path, png(3, 5, [255, 0, 255])).unwrap();
    let mut tiled = flat.clone();
    tiled.sprites.heading_band[0] = Some(crate::sprite::SpriteRef::File(path.clone()));
    crate::sprite::clear_cache();
    let surface = drawn_page(md, &tiled, &p, MARGIN);
    let tile_extent = extent_where(surface, magenta).expect("the band sprite reaches the page");
    assert!(
        tile_extent.1 - tile_extent.0 > 300,
        "the band sprite must TILE across the column, not draw once ({tile_extent:?})"
    );
    crate::sprite::clear_cache();
    assert_eq!(
        colour_extent(drawn_page(md, &tiled, &p, MARGIN), FILL),
        None,
        "the flat band fill is still on the page under the sprite — the sprite must \
         REPLACE the fill, not sit on top of it"
    );

    // A sprite ALONE is a band: no `heading_band_color` anywhere on the theme.
    let mut sprite_only = theme();
    sprite_only.sprites.heading_band[0] = Some(crate::sprite::SpriteRef::File(path));
    crate::sprite::clear_cache();
    assert!(
        extent_where(drawn_page(md, &sprite_only, &p, MARGIN), magenta).is_some(),
        "a heading_band_sprite with no heading_band_color beside it must still band \
         the heading — SCHEMA's sprite row carries no fill precondition"
    );
    crate::sprite::clear_cache();
}

/// Every laid-out line's text, joined — the page's own words, for an assertion about
/// what a text run does or does not carry.
fn page_text(laid: &crate::export::pdf::Laid) -> String {
    laid.lines
        .iter()
        .filter_map(|l| l.layout_text_for_test())
        .collect::<Vec<_>>()
        .join("\n")
}

/// **TDD 18.24 / 25.3 — a list-marker SPRITE is drawn in the gutter AND suppresses the
/// text marker**, so the page carries a picture instead of a bullet rather than both.
///
/// Nothing asserted either half: no test said a `Line::marker` is produced, none said
/// it is drawn, and none said the text run loses its prefix — so deleting the
/// suppression in `measure.rs` put a bullet *and* a picture on the page with the suite
/// green.
#[test]
fn a_list_marker_sprite_is_drawn_in_the_gutter_and_suppresses_the_text_marker() {
    const MARGIN: f64 = 54.0;
    let md = "- first item\n- second item\n";
    let p = palette(&theme());

    // The control: with no sprite the item text carries the bullet prefix.
    let d = doc::build(md, &RenderOptions::default());
    let plain = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &theme());
    assert!(
        plain.lines.iter().all(|l| !l.has_marker_for_test()),
        "a theme stating no marker sprite must hang no picture on any line"
    );
    assert!(
        page_text(&plain).contains('\u{2022}'),
        "the unsprited control must carry the bullet in its own text run"
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bullet.png");
    std::fs::write(&path, png(4, 4, [255, 0, 255])).unwrap();
    let mut t = theme();
    t.sprites.list_bullet = [Some(crate::sprite::SpriteRef::File(path)), None, None];
    crate::sprite::clear_cache();

    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    let with_marker = laid
        .lines
        .iter()
        .filter(|l| l.has_marker_for_test())
        .count();
    assert_eq!(
        with_marker, 2,
        "one marker image per list item, on the item's own first line"
    );
    assert!(
        !page_text(&laid).contains('\u{2022}'),
        "the text run must lose its bullet when a sprite marker applies — otherwise \
         the page carries a bullet AND a picture"
    );

    // And it reaches the page, LEFT of the item's own indent (the gutter).
    let surface = drawn_page(md, &t, &p, MARGIN);
    let (lo, _hi) = extent_where(surface, magenta).expect("the marker sprite reaches the page");
    assert!(
        (lo as f64) < MARGIN + crate::export::pdf::geometry::px_to_pt(t.metrics.list_step),
        "the marker must sit in the gutter left of the item's indent, not inside the \
         text column (leftmost magenta x = {lo})"
    );
    crate::sprite::clear_cache();
}

/// **TDD 18.28 / 25.3 — the blockquote bar's SPRITE tiles down the bar on the page and
/// replaces the flat colour.**
///
/// The existing coverage asserted only that the fixture's bytes DECODE, which is a
/// claim about `sprite.rs` rather than about this sink.
#[test]
fn a_blockquote_bar_sprite_tiles_down_the_bar_on_the_page() {
    const MARGIN: f64 = 54.0;
    const FLAT: (u8, u8, u8) = (0x00, 0xcc, 0x00);
    let md = "> a quoted paragraph long enough to occupy more than one line of the page\n";
    let p = palette(&theme());

    let mut flat = theme();
    flat.blockquote_bar_color = Some(gtk::gdk::RGBA::new(0.0, 0.8, 0.0, 1.0));
    assert!(
        colour_extent(drawn_page(md, &flat, &p, MARGIN), FLAT).is_some(),
        "the control must put the flat bar on the page, or the absence below proves \
         nothing"
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bar.png");
    std::fs::write(&path, png(3, 3, [255, 0, 255])).unwrap();
    let mut tiled = flat.clone();
    tiled.sprites.blockquote_bar = Some(crate::sprite::SpriteRef::File(path));
    crate::sprite::clear_cache();
    assert!(
        extent_where(drawn_page(md, &tiled, &p, MARGIN), magenta).is_some(),
        "the bar sprite never reached the page"
    );
    crate::sprite::clear_cache();
    assert_eq!(
        colour_extent(drawn_page(md, &tiled, &p, MARGIN), FLAT),
        None,
        "the flat bar colour is still on the page under the sprite — the sprite must \
         REPLACE the fill, not sit on top of it"
    );
    crate::sprite::clear_cache();
}

/// **TDD 18.29 / 25.9 — a translucent theme colour reaches the page as a WASH, not as
/// a solid block.**
///
/// Every colour key in this vocabulary parses `#RRGGBBAA` (SCHEMA § Key naming) and two
/// shipped defaults are translucent, so this is not hypothetical — and `blockquote_bg`,
/// "a panel behind quoted text", is the key an author would most naturally make a wash.
/// `set_ink` passed three channels while the gradient arm four lines away passed four,
/// so the preview showed a wash and the page showed a solid block, with nothing warning.
///
/// The oracle scans for the COMPOSITED colour — half-strength navy over white paper —
/// which is a value neither operand can produce on its own, so it cannot be satisfied
/// by the un-composited fill or by the page.
#[test]
fn a_translucent_panel_colour_composites_onto_the_page_rather_than_covering_it() {
    const MARGIN: f64 = 54.0;
    // #000080 at 50% over white paper. cairo rounds the premultiplied blend, so allow
    // one unit of slack per channel rather than pinning an exact triple.
    let composited = |r: u8, g: u8, b: u8| {
        (126..=129).contains(&r) && (126..=129).contains(&g) && (190..=193).contains(&b)
    };
    let solid = |r: u8, g: u8, b: u8| r < 8 && g < 8 && b > 120 && b < 140;
    let md = "> a quoted paragraph on a translucent panel\n";
    let p = palette(&theme());

    let mut t = theme();
    t.blockquote_bg = Some(gtk::gdk::RGBA::new(0.0, 0.0, 0.502, 0.5));
    let surface = drawn_page(md, &t, &p, MARGIN);
    assert!(
        extent_where(surface, composited).is_some(),
        "the translucent panel must COMPOSITE onto the paper — a wash, not a block"
    );
    assert_eq!(
        extent_where(drawn_page(md, &t, &p, MARGIN), solid),
        None,
        "the panel is on the page at full strength: its alpha was discarded, so a \
         theme's wash prints as a solid block and the reader sees a different document \
         from the one on screen"
    );

    // The opaque case is unchanged, which is what keeps a theme that states no alpha
    // byte-identical (TDD 18.2).
    let mut opaque = theme();
    opaque.blockquote_bg = Some(gtk::gdk::RGBA::new(0.0, 0.0, 0.502, 1.0));
    assert!(
        extent_where(drawn_page(md, &opaque, &p, MARGIN), solid).is_some(),
        "an opaque panel must still print at full strength"
    );
}

/// **TDD 18.32 / 25.3 — the eleven keys that reached the preview and the HTML sink and
/// stopped at the page.**
///
/// One test per key would have been eleven tests and eleven chances to omit the twelfth;
/// this drives each key from a theme that states it and asserts the observable it is
/// *for*. What it does NOT do is assert that a key is "used" — that is `F-SINK-001`'s
/// registry sweep, and this is the behavioural half beneath it.
///
/// Four of five heading decorations reached the page before this — scale, weight, band,
/// rule — and the INK did not, so a Synthwave export printed banded, ruled,
/// correctly-scaled headings in body black.
#[test]
fn every_heading_key_reaches_the_page() {
    const MARGIN: f64 = 54.0;
    const INK: (u8, u8, u8) = (0xcc, 0x00, 0x66);
    let md = "# a themed heading\n\nbody text\n";
    let p = palette(&theme());

    // The control: no heading ink stated, so the colour below is evidence about the
    // key rather than about some other run on the page.
    assert_eq!(
        colour_extent(drawn_page(md, &theme(), &p, MARGIN), INK),
        None
    );

    let mut t = theme();
    t.heading_colors[0] = crate::theme::parse_color("#cc0066");
    let surface = drawn_page(md, &t, &p, MARGIN);
    // Glyph edges antialias against the paper, so the fully-opaque filter finds only
    // the interior of the strokes — presence is the claim, not an extent.
    assert!(
        extent_where(surface, |r, g, b| r > 0xa0
            && g < 0x40
            && b > 0x40
            && b < 0x90)
        .is_some(),
        "the heading's own ink never reached the page — it carried scale, weight, band \
         and rule, and printed in body black"
    );

    // The FACE. Asserted through the laid-out width rather than through pixels: a
    // different family lays the same string out to a different width, and there is no
    // colour to scan for.
    let d = doc::build(md, &RenderOptions::default());
    let base_w = laid_heading_width(&lay_out(
        &d,
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &theme(),
    ));
    let mut faced = theme();
    faced.heading_fonts[0] = crate::theme::sanitize_font_family("monospace");
    let faced_w = laid_heading_width(&lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &faced));
    assert!(
        (base_w - faced_w).abs() > 1.0,
        "heading_font did not change the heading's layout ({base_w}pt vs {faced_w}pt) \
         — this sink built ONE font descriptor and read `font_family` alone"
    );

    // The two SPACE keys, per level, EACH against a control that states the other —
    // so neither delta can be carried by the other key.
    let total = |l: &crate::export::pdf::Laid| -> f64 {
        l.fragments
            .iter()
            .map(|f| f.height + f.space_before + f.space_after)
            .sum()
    };
    let plain = total(&lay_out(
        &d,
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &theme(),
    ));
    let px_to_pt = crate::export::pdf::geometry::px_to_pt;

    // `heading_space_below` lands unfloored, so the page grows by exactly the POINT
    // equivalent of the pixel change. Asserting `> 55` for a 70 px change was
    // asserting the unit error itself (F-METRIC-001): these keys are design-time
    // pixels and this sink measures in points.
    let mut below = theme();
    below.metrics.heading_space_below[0] = 30;
    let want = px_to_pt(30) - px_to_pt(theme().metrics.heading_space_below[0]);
    let got = total(&lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &below)) - plain;
    assert!(
        (got - want).abs() < 0.5,
        "heading_space_below moved the page by {got}pt where its own px→pt \
         conversion says {want}pt"
    );

    // `heading_space_above` is floored at this sink's own `BLOCK_GAP_PT`, so the
    // delta is the converted value MINUS that floor rather than the value itself —
    // which is why it gets its own assertion instead of being summed with the one
    // above and compared to a single number.
    let mut above = theme();
    above.metrics.heading_space_above[0] = 40;
    let got_above = total(&lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &above)) - plain;
    assert!(
        got_above > 0.0 && got_above < px_to_pt(40) + 0.5,
        "heading_space_above moved the page by {got_above}pt, which is outside \
         (0, {}] — it is floored at BLOCK_GAP_PT, never multiplied",
        px_to_pt(40)
    );
}

/// The width of the first heading line in a laid-out document, in points.
/// **A NAMED font reaches the artefact, body face and heading face alike.**
///
/// TDD 25.25. The observable is the face Pango **resolved**, not the width and not the
/// requested string: the theme holds a CSS font stack, in which a multi-word family is
/// quoted (`"DejaVu Serif", serif`), and Pango's `FontDescription::set_family` does not
/// accept quotes — it parses the whole stack, fails, and falls through to the generic
/// terminator. So the broken sink laid `font_family = "DejaVu Serif"` out in plain
/// `serif`, which is a real face, a different width from the default sans, and exactly
/// what a reader would expect a serif theme to look like.
///
/// That is why the fixture uses **two-word, installed** families and asserts the face by
/// NAME. Both weaker oracles pass on the broken sink: a bare generic (`monospace`) is
/// left unquoted by `sanitize_font_family`, so the one existing heading-face assertion
/// went on passing, and "the width changed" is satisfied by the fall-through.
///
/// Skipped rather than failed where the fixture faces are not installed — the claim is
/// about this sink's plumbing, not about the host's font set.
#[test]
fn a_named_font_stack_reaches_the_pdf_as_that_named_face() {
    const BODY_FACE: &str = "DejaVu Serif";
    const HEADING_FACE: &str = "DejaVu Sans Mono";

    let ctx = ctx();
    let installed = |want: &str| -> bool {
        use gtk::pango::prelude::{FontFamilyExt, FontMapExt};
        ctx.font_map()
            .map(|m| m.list_families().iter().any(|f| f.name() == want))
            .unwrap_or(false)
    };
    if !installed(BODY_FACE) || !installed(HEADING_FACE) {
        println!(
            "SKIPPED [TDD 25.25]: this host has no `{BODY_FACE}` / `{HEADING_FACE}` to              resolve, so a named face cannot be told from the generic fall-through"
        );
        return;
    }

    let mut t = theme();
    t.font_family = crate::theme::sanitize_font_family(BODY_FACE);
    t.heading_fonts[0] = crate::theme::sanitize_font_family(HEADING_FACE);
    // The sanitiser is what introduces the quoting, so the fixture must go through it —
    // and the test is only about the quoted spelling if it really produced one.
    assert!(
        t.font_family.as_ref().unwrap().as_str().contains('"'),
        "fixture no longer discriminates: `{BODY_FACE}` came back unquoted, so this          asserts nothing about the CSS→Pango spelling"
    );

    let d = doc::build(
        "# A heading

Body prose.
",
        &RenderOptions::default(),
    );
    let laid = lay_out(&d, &ctx, PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    let faces: Vec<String> = laid
        .lines
        .iter()
        .filter_map(|l| l.resolved_family_for_test())
        .collect();

    assert!(
        faces.iter().any(|f| f == HEADING_FACE),
        "`heading_font` never reached the page as `{HEADING_FACE}` — resolved faces          were {faces:?}. A quoted stack Pango cannot parse falls through to the          generic, which is why this asserts the NAME and not the width"
    );
    assert!(
        faces.iter().any(|f| f == BODY_FACE),
        "`font_family` never reached the page as `{BODY_FACE}` — resolved faces were          {faces:?}"
    );
}

fn laid_heading_width(laid: &crate::export::pdf::Laid) -> f64 {
    laid.lines
        .iter()
        .find_map(|l| l.layout_width_for_test())
        .expect("the fixture's heading produced a line")
}

/// **The three metric keys that were literals in this sink.**
///
/// TDD 25.9: "a literal styling value anywhere in either sink is a defect". `list_step`
/// was `INDENT_PT`, `list_item_gap` was `BLOCK_GAP_PT`, and the gap between a quote's
/// bar and its text was whatever the bar's own width happened to be.
#[test]
fn the_three_metric_keys_reach_the_page_instead_of_a_literal() {
    let d_list = doc::build("- one\n- two\n", &RenderOptions::default());
    let d_quote = doc::build("> quoted\n", &RenderOptions::default());
    let indent_of = |laid: &crate::export::pdf::Laid| laid.lines[0].indent;

    // list_step: the item's own indent.
    let base = indent_of(&lay_out(
        &d_list,
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &theme(),
    ));
    let mut stepped = theme();
    stepped.metrics.list_step = 100;
    let wide = indent_of(&lay_out(
        &d_list,
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &stepped,
    ));
    assert!(
        wide > base + 40.0,
        "list_step did not move the item's indent ({base}pt vs {wide}pt)"
    );

    // list_item_gap: the space above the SECOND item.
    let gap_of = |t: &crate::theme::Theme| {
        let laid = lay_out(&d_list, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, t);
        laid.fragments
            .iter()
            .skip(1)
            .map(|f| f.space_before)
            .fold(0.0_f64, f64::max)
    };
    let mut gapped = theme();
    gapped.metrics.list_item_gap = 60;
    assert!(
        gap_of(&gapped) > gap_of(&theme()) + 20.0,
        "list_item_gap did not move the space between items ({} vs {})",
        gap_of(&theme()),
        gap_of(&gapped)
    );

    // blockquote_text_gap: the quote's own step.
    let quote_indent = |t: &crate::theme::Theme| {
        lay_out(&d_quote, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, t).lines[0]
            .quote_indent_for_test()
            .expect("a quoted line reports its quote")
    };
    let mut roomy = theme();
    roomy.metrics.blockquote_text_gap = 80;
    assert!(
        quote_indent(&roomy) > quote_indent(&theme()) + 40.0,
        "blockquote_text_gap did not move the quote's own column ({} vs {})",
        quote_indent(&theme()),
        quote_indent(&roomy)
    );
}

/// `code_inline_bg` and `code_block_bg` reach the page.
#[test]
fn the_two_code_fills_reach_the_page() {
    const MARGIN: f64 = 54.0;
    const INLINE: (u8, u8, u8) = (0xee, 0xdd, 0xcc);
    const BLOCK: (u8, u8, u8) = (0x22, 0x33, 0x44);
    let md = "a `span` of code\n\n```\nfenced\nlisting\n```\n";
    let p = palette(&theme());

    assert_eq!(
        colour_extent(drawn_page(md, &theme(), &p, MARGIN), INLINE),
        None,
        "a theme stating no inline-code fill must put none on the page"
    );
    assert_eq!(
        colour_extent(drawn_page(md, &theme(), &p, MARGIN), BLOCK),
        None
    );

    let mut t = theme();
    t.code_inline_bg = crate::theme::parse_color("#eeddcc");
    t.code_block_bg = crate::theme::parse_color("#223344");
    let surface = drawn_page(md, &t, &p, MARGIN);
    assert!(
        colour_extent(surface, INLINE).is_some(),
        "code_inline_bg never reached the page"
    );
    let (lo, hi) = colour_extent(drawn_page(md, &t, &p, MARGIN), BLOCK)
        .expect("code_block_bg never reached the page");
    assert!(
        hi - lo > 300,
        "the code block's card must span the printable column ({lo}..{hi})"
    );
}

/// `mark_fg` — the key whose absence from this sink was caused by the duplicated span
/// builder `crate::pangospan` replaced.
#[test]
fn mark_fg_reaches_the_page() {
    const MARGIN: f64 = 54.0;
    let md = "a ==highlighted== word\n";
    let p = palette(&theme());

    assert_eq!(
        extent_where(drawn_page(md, &theme(), &p, MARGIN), magenta),
        None,
        "a theme stating no mark ink must put none on the page"
    );
    let mut t = theme();
    t.mark_fg = crate::theme::parse_color("#ff00ff");
    assert!(
        extent_where(drawn_page(md, &t, &p, MARGIN), magenta).is_some(),
        "mark_fg never reached the page — it was the ONE difference between the two \
         copies of the mark span builder"
    );
}

/// **Every `Metrics` read in this sink converts px→pt, and nothing reads one raw.**
///
/// A unit error here is coherent PER KEY, which is what made it survive review: a
/// reader checking `table_border_width` found it converting through `PT_PER_PX` and
/// concluded the sink was right, while `blockquote_bar_width`, `rule_space` and
/// `heading_band_padding` beside it were read straight as points — 33% over on the page
/// against the same key on screen (F-METRIC-001).
///
/// A source scan rather than a behavioural assertion, deliberately. The property is
/// "no read takes the other route", and the only way a behavioural test could cover it
/// is one assertion per key per surface — which is the enumeration that let three keys
/// slip. `env!("CARGO_MANIFEST_DIR")` because a test's CWD is not the repo root.
#[test]
fn no_metrics_read_in_this_sink_bypasses_the_px_to_pt_conversion() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/export/pdf");
    let mut scanned = 0usize;
    let mut raw: Vec<String> = Vec::new();
    let mut walk = vec![root];
    while let Some(dir) = walk.pop() {
        for entry in std::fs::read_dir(&dir).expect("the sink's own directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                walk.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            // This file is the guard, not the subject.
            if path.file_name().is_some_and(|n| n == "tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a source file");
            for (n, line) in text.lines().enumerate() {
                let Some(at) = line.find("metrics.") else {
                    continue;
                };
                // `let m = &self.theme.metrics;` binds the struct; the READS are the
                // lines that go on to name a field.
                if line[at..].trim_end().ends_with("metrics;") {
                    continue;
                }
                scanned += 1;
                if !line.contains("px_to_pt") {
                    raw.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        scanned > 5,
        "the scan found {scanned} Metrics reads — it is looking in the wrong place"
    );
    assert!(
        raw.is_empty(),
        "these Metrics reads take the theme's design-time PIXELS as points:\n{}",
        raw.join("\n")
    );
}

/// **A list item whose first block is not a paragraph still gets its marker.**
///
/// The sprite and the marker markup were computed before the item's blocks were walked
/// and attached only inside `if i == 0 { if let Block::Paragraph | Block::Heading = … }`
/// — so an item beginning with a fenced code block or a nested list decoded its sprite,
/// DISCARDED it, and rendered with no marker at all, glyph or picture. That is ordinary
/// Markdown, reachable from any document, and it had no test.
///
/// Both marker shapes are driven, because they take different routes: a glyph or a
/// numeral is TEXT and can only ride a run, while a sprite is a picture drawn beside a
/// line. Testing one would have left the other's arm uncovered.
#[test]
fn a_list_item_that_opens_with_a_code_block_still_carries_its_marker() {
    let md = "1. ```\n   fenced\n   ```\n\n   trailing prose\n";
    let t = theme();
    let d = doc::build(md, &RenderOptions::default());
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    let line_text = |laid: &crate::export::pdf::Laid| -> String {
        laid.lines
            .iter()
            .filter_map(|l| match &l.kind {
                crate::export::pdf::LineKind::Text { layout, .. } => {
                    Some(layout.text().to_string())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let text = line_text(&laid);
    assert!(
        text.contains("fenced"),
        "the fixture did not produce a code-first list item:\n{text}"
    );
    assert!(
        text.contains('1'),
        "an item opening with a code block lost its ordered marker entirely:\n{text}"
    );

    // The SPRITE arm, which takes the other route: no marker text at all, a picture
    // attached to the item's first line.
    let mut sprited = theme();
    sprited.sprites.list_ordered = Some(crate::sprite::SpriteRef::Compiled(
        "sprites/copper-plate.png",
    ));
    let laid = lay_out(&d, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &sprited);
    assert!(
        laid.lines.iter().any(|l| l.marker.is_some()),
        "an item opening with a code block lost its sprite marker — it was decoded and \
         then discarded because the attachment sat inside the block-kind guard"
    );

    // Anti-vacuity: the ordinary paragraph-first item still carries its marker on the
    // SAME line as its text, so the fix did not turn every marker into a line of its own.
    let plain = doc::build("1. prose first\n", &RenderOptions::default());
    let laid = lay_out(&plain, &ctx(), PAGE_WIDTH_PT, PAGE_HEIGHT_PT, &t);
    let first = line_text(&laid);
    assert!(
        first.contains('1') && first.contains("prose"),
        "a paragraph-first item must keep its marker on its own first line: {first}"
    );
}

/// **`Paged` is the only way to reach the drawing stage, and it holds the
/// at-least-one-page policy.**
///
/// Both halves used to live at the caller in `window/export_pdf.rs`, which the coverage
/// gate excludes — so the empty-document rule had no test at all, and every test in this
/// file re-created the caller's stage sequence rather than using it (F-PAGINATE-001).
///
/// The empty case is the one that matters: `paginate` correctly answers "no fragments,
/// no pages", and a zero-page PDF is a file no reader will open. The policy that turns
/// that into one blank page is this boundary's, and it is now where a test can ask it.
#[test]
fn an_empty_document_still_gets_one_page_and_the_page_is_drawable() {
    let t = theme();
    let empty = doc::build("", &RenderOptions::default());
    let paged = crate::export::pdf::Paged::prepare(
        &empty,
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        std::rc::Rc::new(t.clone()),
        54.0,
    );
    assert!(
        paged.laid().fragments.is_empty(),
        "the fixture must produce no fragments, or this is not the empty case"
    );
    assert_eq!(
        paged.page_count(),
        1,
        "an empty document must still produce a page — a zero-page PDF is a file no \
         reader will open"
    );

    // …and that page is genuinely drawable, which is the half `n_pages().max(1)` used to
    // get wrong: it told GTK to draw a page the range vector had no entry for, so the
    // draw handler took its out-of-range early return and drew nothing.
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 612, 792)
        .expect("an image surface needs no display");
    let cr = cairo::Context::new(&surface).expect("a cairo context");
    let palette = crate::palette::Palette::for_paper(&t);
    assert!(
        paged.draw(&cr, 0, &palette).is_some(),
        "the blank page must be drawable, or the tally counts a page nothing produced"
    );
    // An index past the end is NOT drawable, and returns no proof — so a caller driven
    // by GTK's own page number cannot tally a page that was never drawn.
    assert!(paged.draw(&cr, 1, &palette).is_none());
}

/// A multi-page document reports its real page count, so the assertion above is not
/// passing because `page_count` always answers 1.
#[test]
fn a_long_document_reports_the_pages_it_actually_occupies() {
    let t = theme();
    let long: String = (0..400).map(|n| format!("Paragraph {n}.\n\n")).collect();
    let d = doc::build(&long, &RenderOptions::default());
    let paged = crate::export::pdf::Paged::prepare(
        &d,
        &ctx(),
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        std::rc::Rc::new(t),
        54.0,
    );
    assert!(
        paged.page_count() > 1,
        "a 400-paragraph document occupies one page — the paginator is not being run"
    );
}

/// **Rubric 2.26g at the PDF sink.** A PDF has no disclosure to offer, so a collapsed
/// block's body must be LAID OUT — a page the reader cannot expand must not be the one
/// place the document's content is missing.
///
/// Measured against a control rendered with the block's contents at top level: the
/// disclosure must cost at least as many lines, or the body is going somewhere other
/// than the page.
#[test]
fn a_collapsed_disclosure_lays_its_body_out_on_the_page() {
    let t = theme();
    let ctx = ctx();
    const BODY: &str = "the hidden paragraph\n\n- alpha\n- beta\n";
    let with_block = format!("<details>\n<summary>Show me</summary>\n\n{BODY}\n</details>\n");

    let laid = lay_out(
        &doc::build(&with_block, &RenderOptions::default()),
        &ctx,
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &t,
    );
    let bare = lay_out(
        &doc::build(BODY, &RenderOptions::default()),
        &ctx,
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &t,
    );
    assert!(!bare.lines.is_empty(), "control: the body lays out at all");
    assert!(
        laid.lines.len() > bare.lines.len(),
        "the body must reach the page, plus a line for the summary label: \
         {} lines with the disclosure vs {} for its body alone",
        laid.lines.len(),
        bare.lines.len()
    );
    assert!(
        laid.fragments.iter().all(|f| f.height > 0.0),
        "every fragment is drawable"
    );
}

/// **TDD 18.51 — a themed summary band reaches the page as a fill on the label's own
/// line, and the label's own ink reaches the run.**
///
/// Display-free on purpose, which is what puts this half of the decoration inside the
/// coverage gate rather than resting on the cross-surface sweep (which needs a live
/// tag table and is therefore feature-gated). Two-sided: a theme stating nothing must
/// fill nothing, or the assertion below is satisfied by a sink that bands every line.
#[test]
fn a_themed_disclosure_band_fills_the_summary_label_line() {
    const DOC: &str = "<details>\n<summary>Show me</summary>\n\nbody text\n\n</details>\n";
    let ctx = ctx();

    let plain = theme();
    let bare = lay_out(
        &doc::build(DOC, &RenderOptions::default()),
        &ctx,
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &plain,
    );
    assert!(
        !bare.lines.iter().any(|l| l.is_filled_for_test()),
        "System bands no summary line — an unset key is absent, never a default"
    );

    let mut themes = crate::theme::Themes::builtin();
    themes.merge_over_for_test(
        "[themes.banded]\ndisclosure_band_color = \"#339966\"\n\
         disclosure_band_gradient_to_color = \"#0a1830\"\n\
         disclosure_fg = \"#ffe9a8\"\n",
    );
    let t = themes.resolve("banded");
    let laid = lay_out(
        &doc::build(DOC, &RenderOptions::default()),
        &ctx,
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &t,
    );
    let filled = laid.lines.iter().filter(|l| l.is_filled_for_test()).count();
    assert_eq!(
        filled, 1,
        "exactly the summary LABEL's line carries the band — the body is ordinary \
         prose and must not be banded with it"
    );
    // The band must be the FIRST line, not some line of the body: the label opens the
    // block. Asserted separately because a count alone cannot tell which line it was.
    assert!(
        laid.lines.first().is_some_and(|l| l.is_filled_for_test()),
        "the band belongs to the summary label, which is the block's first line"
    );

    // …and the ink, which travels in the MARKUP rather than as a cairo pen on the
    // line. That is not a stylistic choice: `ink::draw_page` sets `blockquote_fg` as
    // the source for every line inside a quote, and only an attribute on the run can
    // override it (`markup::disclosure_span`).
    let (open, close) = crate::export::markup::disclosure_span(&t);
    assert!(open.contains("foreground=\"#ffe9a8\"") && close == "</span>");
    assert_eq!(
        crate::export::markup::disclosure_span(&plain),
        (String::new(), ""),
        "a theme stating no disclosure_fg must wrap the label in nothing at all"
    );

    // **The container context** (Document Rendering CAM row 2). A disclosure inside a
    // blockquote is a real construct here — MEASURED: its label reaches this sink with
    // `quote` set — so it is the case where the ink's spelling matters, and it must be
    // banded exactly as one at top level is.
    const QUOTED: &str =
        "> <details>\n> <summary>Show me</summary>\n>\n> body text\n>\n> </details>\n";
    let quoted = lay_out(
        &doc::build(QUOTED, &RenderOptions::default()),
        &ctx,
        PAGE_WIDTH_PT,
        PAGE_HEIGHT_PT,
        &t,
    );
    let label = quoted
        .lines
        .first()
        .expect("the quoted disclosure lays out a label line");
    assert!(
        label.is_filled_for_test(),
        "a quoted summary must carry the band a top-level one does"
    );
    assert!(
        label.quote_indent_for_test().is_some(),
        "the fixture must actually put the label INSIDE the quote, or this case is \
         the top-level one wearing a different fixture"
    );
}
