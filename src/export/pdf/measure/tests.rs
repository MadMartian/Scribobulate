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
    let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
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
        468.0,
        684.0,
        &t,
    );
    let body = lay_out(
        &doc::build("H\n", &RenderOptions::default()),
        &ctx(),
        468.0,
        684.0,
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
        684.0,
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
    let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
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
    let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
    let h = laid
        .fragments
        .iter()
        .zip(&laid.lines)
        .find(|(_, l)| l.is_image_for_test())
        .map(|(f, _)| f.height)
        .expect("an image fragment");
    assert!(h <= 4.0, "a 4px image must not be upscaled, got {h}pt");

    // …and a narrow column scales it down rather than overflowing.
    let narrow = lay_out(&d, &ctx(), 2.0, 684.0, &t);
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
    let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
    let text: String = laid
        .lines
        .iter()
        .filter_map(|l| l.layout_text_for_test())
        .collect();
    assert!(!text.trim().is_empty(), "an undecodable image said nothing");
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
    let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
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
    let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
    let pages = paginate::paginate(&laid.fragments, &metrics_for(684.0));
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

/// Draw one document at `margin` onto a fresh white page and return the surface.
fn drawn_page(
    md: &str,
    t: &crate::theme::Theme,
    p: &crate::palette::Palette,
    margin: f64,
) -> cairo::ImageSurface {
    let d = doc::build(md, &RenderOptions::default());
    let laid = lay_out(&d, &ctx(), 468.0, 684.0, t);
    let pages = paginate::paginate(&laid.fragments, &metrics_for(684.0));
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
        for page in &pages {
            let _drawn = draw_page(&cr, &laid, page.clone(), p, t, margin);
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
    assert_eq!(
        lo,
        (MARGIN + crate::export::pdf::geometry::INDENT_PT) as usize,
        "the panel must start at the quote's indent, not at the page margin"
    );
    // Right edge: the printable column. `printable_width_pt` is measured from the
    // page's own width, so this is the same edge body text wraps at.
    assert!(
        hi >= (MARGIN + 468.0) as usize - 1,
        "the panel must reach the printable edge, got {hi}"
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

    let left = (MARGIN + crate::export::pdf::geometry::INDENT_PT) as usize;
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
    let magenta = |r: u8, g: u8, b: u8| r > 0x80 && b > 0x80 && g < 0x60;
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

    let surface = drawn_page(md, &tiled, &p, MARGIN);
    let stride = surface.stride() as usize;
    let width = surface.width() as usize;
    let data = surface.take_data().expect("surface data");
    let rows: Vec<usize> = data
        .chunks_exact(stride)
        .enumerate()
        .filter(|(_, row)| {
            row[..width * 4]
                .chunks_exact(4)
                .any(|px| px[3] == 0xff && magenta(px[2], px[1], px[0]))
        })
        .map(|(y, _)| y)
        .collect();
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
    let widest = data
        .chunks_exact(stride)
        .map(|row| {
            row[..width * 4]
                .chunks_exact(4)
                .filter(|px| px[3] == 0xff && magenta(px[2], px[1], px[0]))
                .count()
        })
        .max()
        .unwrap_or(0);
    assert!(
        widest as f64 >= 468.0 - 1.0,
        "the tile covered {widest} px of a 468pt column — it was drawn once, not tiled"
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
    let magenta = |r: u8, g: u8, b: u8| r > 0x80 && b > 0x80 && g < 0x60;
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
    let stride = surface.stride() as usize;
    let width = surface.width() as usize;
    let data = surface.take_data().expect("surface data");
    let rows: Vec<usize> = data
        .chunks_exact(stride)
        .enumerate()
        .filter(|(_, row)| {
            row[..width * 4]
                .chunks_exact(4)
                .any(|px| px[3] == 0xff && magenta(px[2], px[1], px[0]))
        })
        .map(|(y, _)| y)
        .collect();
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
    // Unmistakably magenta rather than exactly `#ff00ff`: a glyph's edge pixels are
    // antialiased against the page, so an exact match tests the rasteriser's coverage
    // rather than the ink. Nothing else on this page is remotely magenta.
    let magenta = |r: u8, g: u8, b: u8| r > 0x80 && b > 0x80 && g < 0x60;
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
        lo >= (MARGIN + crate::export::pdf::geometry::INDENT_PT) as usize,
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
    let d = doc::build(
        "A < B & C \"quoted\" and <span foreground='red'>x</span>\n",
        &RenderOptions::default(),
    );
    let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
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
        468.0,
        684.0,
        &t,
    );
    assert!(laid.fragments.is_empty());
    assert!(paginate::paginate(&laid.fragments, &metrics_for(684.0)).is_empty());
}

#[test]
fn an_annotation_reaches_the_page_with_its_comment() {
    let t = theme();
    let d = doc::build(
        "The {==claim==}{>>reviewer says this<<} here.\n",
        &RenderOptions::default(),
    );
    let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
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
        468.0,
        684.0,
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
        684.0,
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
        468.0,
        684.0,
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
        468.0,
        684.0,
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
        468.0,
        684.0,
        &theme(),
    );
    for (_, _, scale) in table_rows(&laid) {
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
        468.0,
        684.0,
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
        468.0,
        684.0,
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
        684.0,
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
    let width = 468.0;
    let prefix = format!("{} ", ">".repeat(30));
    let quoted: String = TABLE
        .lines()
        .map(|line| format!("{prefix}{}\n", line.trim_start()))
        .collect();
    let laid = lay_out(
        &doc::build(&quoted, &RenderOptions::default()),
        &ctx(),
        width,
        684.0,
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
    for width in [80.0, 140.0, 300.0, 468.0] {
        let laid = lay_out(
            &doc::build(TABLE, &RenderOptions::default()),
            &ctx(),
            width,
            684.0,
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
    let laid = lay_out(&d, &ctx(), 200.0, 684.0, &t);
    let banded = laid.lines.iter().filter(|l| l.band.is_some()).count();
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
    let plain = lay_out(&d, &ctx(), 200.0, 684.0, &theme());
    assert!(plain.lines.iter().all(|l| l.band.is_none()));
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
        "[themes.tiered]\nlist_marker = \"#111111\"\nlist_marker_2 = \"#222222\"\n\
         list_marker_3 = \"#333333\"\n",
    );
    let tiered = themes.resolve("tiered");
    t.list_marker = tiered.list_marker;
    t.list_bullet_colors = tiered.list_bullet_colors;

    let d = doc::build(
        "- one\n    - two\n        - three\n            - four\n",
        &RenderOptions::default(),
    );
    let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
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
        "[themes.banded]\nheading_band_bg = [\"#334455\", \"\", \"\", \"\", \"\"]\n\
         heading_band_padding = 16\n",
    );
    let banded = themes.resolve("banded");
    t.heading_band = banded.heading_band;
    t.metrics.heading_band_padding = banded.metrics.heading_band_padding;

    let d = doc::build("# banded\n\n## plain\n", &RenderOptions::default());
    let laid = lay_out(&d, &ctx(), 468.0, 684.0, &t);
    let banded_line = laid
        .lines
        .iter()
        .find(|l| l.band.is_some())
        .expect("the h1 carries a band");
    // The TEXT sits one padding in from the column…
    assert_eq!(banded_line.indent, 16.0);
    // …and the band backs out to the column itself, so its extent is unchanged from a
    // theme that states no padding at all.
    assert_eq!(banded_line.band.as_ref().unwrap().padding, 16.0);
    assert_eq!(
        banded_line.indent - banded_line.band.as_ref().unwrap().padding,
        0.0
    );

    // The unbanded h2 is untouched: same indent it had before any of this.
    let plain = laid
        .lines
        .iter()
        .find(|l| l.band.is_none() && matches!(l.kind, crate::export::pdf::LineKind::Text { .. }))
        .expect("the h2 and the body carry none");
    assert_eq!(plain.indent, 0.0);
}
