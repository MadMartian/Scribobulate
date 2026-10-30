//! A source line break in the export model follows CommonMark (TDD 2.20): a soft break
//! is a space, a hard break is the one [`Inline::Break`] a sink draws as a line break.

use crate::export::{doc, Block, ExportDoc, Inline, RenderOptions};

fn doc_of(md: &str) -> ExportDoc {
    doc::build(md, &RenderOptions::default())
}

/// Every inline of every block, flattened, so an assertion can ask about the whole
/// document whatever container its text sits in.
fn inlines(blocks: &[Block]) -> Vec<Inline> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            Block::Heading { inlines: v, .. } | Block::Paragraph(v) => out.extend(v.clone()),
            Block::BlockQuote(inner) => out.extend(inlines(inner)),
            Block::Disclosure { summary, body, .. } => {
                out.extend(summary.clone());
                out.extend(inlines(body));
            }
            Block::List { items, .. } => {
                for item in items {
                    out.extend(inlines(&item.blocks));
                }
            }
            Block::Table { head, rows, .. } => {
                for cell in head.iter().chain(rows.iter().flatten()) {
                    out.extend(cell.clone());
                }
            }
            Block::CodeBlock { .. } | Block::Rule => {}
        }
    }
    out
}

fn break_count(md: &str) -> usize {
    inlines(&doc_of(md).blocks)
        .iter()
        .filter(|i| matches!(i, Inline::Break))
        .count()
}

fn plain(md: &str) -> String {
    crate::export::plain_text(&inlines(&doc_of(md).blocks))
}

/// Every fixture under both line endings: a Windows checkout reads each source line
/// break as `\r\n`, which pulldown-cmark reports as the same break event with a
/// two-byte range (MEASURED 0.13.4), so the export must not differ by ending.
fn both_endings(md: &str) -> [String; 2] {
    [md.to_string(), md.replace('\n', "\r\n")]
}

#[test]
fn a_soft_break_exports_as_a_space_in_every_container() {
    for fixture in [
        "Line one\nLine two\n",
        "- first\n  second\n- next\n",
        "- loose\n  wrap\n\n- next\n",
        "> quoted\n> wrap\n",
        "**bold**\n`code`\n[link](https://example.com)\n",
    ] {
        for md in both_endings(fixture) {
            assert_eq!(
                break_count(&md),
                0,
                "a soft break is not a line break: {md:?}"
            );
        }
    }
    for (fixture, shown) in [
        ("Line one\nLine two\n", "Line one Line two"),
        ("- first\n  second\n- next\n", "first second"),
        ("> quoted\n> wrap\n", "quoted wrap"),
        (
            "**bold**\n`code`\n[link](https://example.com)\n",
            "bold code link",
        ),
    ] {
        for md in both_endings(fixture) {
            let text = plain(&md);
            assert!(text.contains(shown), "{md:?} exported {text:?}");
            assert!(!text.contains('\r'), "{md:?} exported {text:?}");
        }
    }
}

#[test]
fn a_hard_break_still_exports_as_a_line_break() {
    for fixture in [
        "two spaces  \nnext\n",
        "backslash\\\nnext\n",
        "- item\\\n  next\n",
    ] {
        for md in both_endings(fixture) {
            assert_eq!(break_count(&md), 1, "{md:?}");
        }
    }
}

#[test]
fn a_soft_break_keeps_the_claim_mapper_counting_one_rendered_char() {
    // The space stands where the break's one rendered char stood, so every span after
    // it keeps its position (the claim mapper counts in the same units as the preview).
    for md in both_endings("ab\ncd\n") {
        let doc = doc_of(&md);
        let spans: Vec<(i32, i32)> = inlines(&doc.blocks)
            .iter()
            .filter_map(|i| match i {
                Inline::Text { span, .. } => Some(*span),
                _ => None,
            })
            .collect();
        assert_eq!(spans, vec![(0, 2), (2, 3), (3, 5)], "{md:?}");
    }
}
