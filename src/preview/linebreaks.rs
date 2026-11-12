//! **A source line break renders the CommonMark way** (TDD 2.20), asserted against
//! real renders: a soft break is a space, a hard break is a line break, a blank line
//! is a paragraph.
//!
//! Test-only, and carrying `gtk-integration-tests` for the reason
//! `preview::altsuppression` gives: a render needs a `GtkTextBuffer`. Its own module
//! because `build.rs` is far past the file-size soft limit.

use super::build::{build_render_products, RenderProducts};
use gtk::prelude::*;

fn render(md: &str) -> RenderProducts {
    build_render_products(md, None, 1.0, false)
}

/// The buffer's text, with a `U+FFFC` standing in for each anchored child.
fn slice(products: &RenderProducts) -> String {
    let buf = &products.buf;
    buf.slice(&buf.start_iter(), &buf.end_iter(), true)
        .to_string()
}

#[gtktest::test]
fn a_soft_break_renders_as_a_space_in_every_inline_context() {
    for (md, shown) in [
        (
            "Line one\nLine two\nLine three",
            "Line one Line two Line three",
        ),
        ("- first\n  second\n- next", "first second"),
        ("- loose\n  wrap\n\n  para\n- next", "loose wrap"),
        ("1. ordered\n   wrap", "ordered wrap"),
        ("> quoted\n> wrap", "quoted wrap"),
        ("> lazy\ncontinuation", "lazy continuation"),
        (
            "**bold**\n`code`\n[link](https://example.com)\n*em*",
            "bold code link em",
        ),
        ("a **bold\nrun** b", "a bold run b"),
    ] {
        let text = slice(&render(md));
        assert!(
            text.contains(shown),
            "{md:?} must flow as {shown:?}, rendered {text:?}"
        );
    }
}

#[gtktest::test]
fn a_hard_break_and_a_blank_line_still_break() {
    for (md, first, second) in [
        ("two spaces  \nnext", "two spaces", "next"),
        ("backslash\\\nnext", "backslash", "next"),
        ("- item\\\n  next", "item", "next"),
        ("para one\n\npara two", "para one", "para two"),
    ] {
        let text = slice(&render(md));
        let a = text.find(first).expect("first line rendered") + first.len();
        let b = text.find(second).expect("second line rendered");
        assert!(
            text[a..b].contains('\n') && !text[a..b].contains(' '),
            "{md:?} must break between the lines, rendered {text:?}"
        );
    }
}

/// The space is the break's own rendered char, so every map keyed on buffer offsets
/// stays aligned and copy still yields the Markdown source, newline included (TDD 2.8).
#[gtktest::test]
fn copy_across_a_soft_break_yields_the_source() {
    let md = "Line one\nLine two\n- item\n  wrap";
    let products = render(md);
    let n = products.buf.char_count();
    assert_eq!(
        crate::copymap::resolve(&products.maps.copymap, md, 0, n),
        md
    );
    let text = slice(&products);
    let a = text.find("one").expect("rendered") as i32;
    let b = a + "one Line".chars().count() as i32;
    assert_eq!(
        crate::copymap::resolve(&products.maps.copymap, md, a, b),
        "one\nLine"
    );
}

/// A setext heading's content may span source lines. It renders as one line, and its
/// anchor is slugged from what the page shows, so `#foo-bar` — the anchor the export
/// and the outline issue — lands on it.
#[gtktest::test]
fn a_multi_line_heading_renders_on_one_line_and_slugs_with_a_hyphen() {
    let products = render("Foo\nbar\n===\n\nbody");
    assert!(
        slice(&products).starts_with("Foo bar"),
        "{:?}",
        slice(&products)
    );
    assert!(
        products.maps.heading_map.resolve("foo-bar").is_some(),
        "the heading answers #foo-bar"
    );
}

/// A Windows checkout gives every source line break as `\r\n`. pulldown-cmark reports
/// the same break events with two-byte ranges, and splits a code block's line endings
/// into their own `Text("\n")` events (MEASURED 0.13.4), so the page, the copy map and
/// its debug consistency check must hold under CRLF exactly as under LF. The CRLF is
/// embedded here rather than read from a checkout, whose line endings vary.
#[gtktest::test]
fn a_crlf_document_renders_and_copies_as_an_lf_one_does() {
    let md = "Line one\r\nLine two\r\n\r\n- item\r\n  wrap\r\n\r\n> quoted\r\n> wrap\r\n\r\n\
              Foo\r\nbar\r\n===\r\n\r\n```\r\ncode\r\n\r\nmore\r\n```\r\n\r\ntwo  \r\nlines\r\n";
    // The render's own debug consistency check runs inside the build and panics on
    // drift, so reaching the assertions is itself the guard on the copy map.
    let products = render(md);
    let text = slice(&products);
    for shown in [
        "Line one Line two",
        "item wrap",
        "quoted wrap",
        "Foo bar",
        "code\n\nmore",
    ] {
        assert!(text.contains(shown), "{shown:?} missing from {text:?}");
    }
    assert!(
        !text.contains('\r'),
        "no carriage return reaches the page: {text:?}"
    );
    let a = text.rfind("two").expect("rendered") + "two".len();
    let b = text.rfind("lines").expect("rendered");
    assert_eq!(&text[a..b], "\n", "a CRLF hard break still breaks");
    assert!(products.maps.heading_map.resolve("foo-bar").is_some());
    // Copy across a CRLF soft break yields the source bytes, CR included (TDD 2.8).
    let a = text.find("one Line").expect("rendered") as i32;
    let b = a + "one Line".chars().count() as i32;
    assert_eq!(
        crate::copymap::resolve(&products.maps.copymap, md, a, b),
        "one\r\nLine"
    );
}

/// The committed CRLF fixture (`-text`, so CRLF on every checkout) renders its soft
/// break as a space.
#[gtktest::test]
fn the_crlf_fixture_renders_its_soft_break_as_a_space() {
    let md = include_str!("../../tests/fixtures/crlf-doc.md");
    assert!(md.contains("ending.\r\nSecond"), "fixture is CRLF");
    let text = slice(&render(md));
    assert!(
        text.contains("Windows ending. Second line."),
        "rendered {text:?}"
    );
}

/// Front matter under CRLF renders as it does under LF, in both folds: the synthetic
/// disclosure hands the renderer the body as ONE `Text` event with its line endings
/// inside it, where pulldown would have split each ending into its own event, so this is
/// the one code block whose carriage returns reach the code renderer unsplit. Found on
/// the 1.0.0 Windows manual pass (MANUAL-TEST 2.27, the TOML fixture through a CRLF
/// checkout): every line break in the expanded block was doubled.
#[gtktest::test]
fn crlf_front_matter_renders_as_lf_front_matter_does() {
    use gtk::prelude::TextBufferExt;
    let render_with = |md: &str, folds: &crate::fold::FoldState| {
        let products = super::build::build_render_products_with_theme(
            None,
            md,
            None,
            1.0,
            false,
            crate::theme::active(),
            folds,
        );
        let buf = &products.buf;
        buf.slice(&buf.start_iter(), &buf.end_iter(), true)
            .to_string()
    };
    for (lf, crlf) in [
        (
            "+++\ntitle = \"Post\"\ndate = 2026-01-01\n+++\n\n# Heading\n\nBody.\n",
            "+++\r\ntitle = \"Post\"\r\ndate = 2026-01-01\r\n+++\r\n\r\n# Heading\r\n\r\nBody.\r\n",
        ),
        (
            "---\nname: dev\nrole: tester\n---\n\n# Heading\n\nBody.\n",
            "---\r\nname: dev\r\nrole: tester\r\n---\r\n\r\n# Heading\r\n\r\nBody.\r\n",
        ),
    ] {
        let mut expanded = crate::fold::FoldState::default();
        expanded.toggle(crate::fold::FoldKey::from_source_offset(0));
        for (name, folds) in [
            ("collapsed", crate::fold::FoldState::default()),
            ("expanded", expanded),
        ] {
            let a = render_with(lf, &folds);
            let b = render_with(crlf, &folds);
            assert!(!b.contains('\r'), "{name}: a CR reached the page: {b:?}");
            assert_eq!(
                a, b,
                "{name}: CRLF front matter renders as LF front matter does"
            );
        }
    }
}
