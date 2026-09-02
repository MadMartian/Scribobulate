//! Display-free: Markdown source → [`ExportDoc`].
//!
//! Enters through **the conditions the preview enters through** — tab normalisation,
//! CriticMarkup extraction, [`md_options`](crate::renderer::md_options), the
//! pulldown-cmark offset iterator, plus `scan_script_spans` for the four constructs
//! pulldown never sees — so the export agrees with the preview by construction. Every
//! decision about *what a construct is* was already made upstream; nothing here
//! re-decides one.
//!
//! **No GTK, no display, no filesystem beyond reading an image the containment gate
//! already admitted.** That is what puts this file inside the coverage gate and lets
//! TDD 25.2 (a never-rendered tab exports identically) be a unit test rather than a
//! human opening a file.

use super::walk::Builder;
use super::{Block, ExportAnnotation, ExportDoc, Inline, RenderOptions};
use crate::renderer::segments_of;
use pulldown_cmark::Parser;

/// Build the export model for `source`.
///
/// `source` is the document's **buffer** text, never the bytes on disk: an export is
/// of what the reader is looking at, including unsaved edits and an untitled buffer
/// that has never been written (TDD 25.5).
pub(crate) fn build(source: &str, opts: &RenderOptions) -> ExportDoc {
    // Normalise inline hard tabs exactly as the preview does, so a tab-separated
    // table row parses as a GFM table here too (ScrAP-75). Length- and
    // position-preserving, so every offset below indexes both texts identically.
    let normalised = crate::renderer::NormalizedMd::new(source);
    // Lift CriticMarkup out before pulldown sees it, and render the cleaned text —
    // the same pre-parse pass the preview runs, so the two cannot disagree about
    // what is document content and what is review apparatus.
    let extraction = crate::annotate::extract(normalised.as_str());
    let cleaned = extraction.cleaned.as_str();

    // One block-scope scan of the same cleaned text the walk below parses, so the
    // export segments every tight construct exactly as the preview does.
    let mut builder = Builder::new(
        opts,
        crate::renderer::BlockScripts::scan(cleaned),
        crate::renderer::disclosure::scan_document(cleaned),
    );
    for (ev, src) in Parser::new_ext(cleaned, crate::renderer::md_options()).into_offset_iter() {
        builder.event(ev, src);
    }
    let mut doc = builder.finish();
    // Taken once: the records exist to place claims and are of no interest to a sink.
    let content_evs = std::mem::take(&mut doc.content_evs);

    // Annotations: membership is `is_listed`'s — the same predicate the margin chips
    // and the annotations viewer use, so the export cannot hold a different opinion
    // about what an annotation is (TDD 20.2's shared-predicate rule).
    let listed: Vec<&crate::annotate::Annotation> = extraction
        .annotations
        .iter()
        .filter(|a| a.is_listed())
        .collect();
    for (idx, ann) in listed.iter().enumerate() {
        let (hs, he) = (
            ann.cleaned_content.start.raw(),
            ann.cleaned_content.end.raw(),
        );
        // The SAME mapper the preview places its claim highlight with, so the extent
        // obligation (Document Rendering CAM row 3) is satisfied by one implementation
        // rather than two that agree today.
        for range in crate::annotate::map_cleaned_highlight_to_local(cleaned, hs, he, &content_evs)
        {
            mark_claim(&mut doc.blocks, idx, range);
        }
        doc.annotations.push(ExportAnnotation {
            comment: ann.comment.clone().unwrap_or_default(),
            claim: claim_text(cleaned, hs, he),
        });
    }
    doc
}

/// The claim's own text, delimiters and tight-construct markers stripped, for a sink
/// that shows a comment away from the run it refers to.
fn claim_text(cleaned: &str, hs: usize, he: usize) -> String {
    if hs >= he || he > cleaned.len() {
        return String::new();
    }
    let run = &cleaned[hs..he];
    segments_of(run)
        .into_iter()
        .filter(|seg| !seg.marker)
        .map(|seg| seg.text(run))
        .collect()
}

/// Split every [`Inline::Text`] overlapping `range` and wrap the overlap in a
/// [`Inline::Claim`]. Recursive, because a claim can land inside emphasis, a link, a
/// list item or a table cell — every context Document Rendering CAM row 2 names.
fn mark_claim(blocks: &mut Vec<Block>, idx: usize, range: (i32, i32)) {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
                mark_inlines(inlines, idx, range)
            }
            Block::BlockQuote(inner) => mark_claim(inner, idx, range),
            // The BODY takes claims like any other content — it is ordinary document
            // text that the preview happened not to draw. The summary does not: it
            // lives inside raw HTML, which carries no annotations.
            Block::Disclosure { body, .. } => mark_claim(body, idx, range),
            Block::List { items, .. } => {
                for item in items {
                    mark_claim(&mut item.blocks, idx, range);
                }
            }
            Block::Table { head, rows, .. } => {
                for cell in head {
                    mark_inlines(cell, idx, range);
                }
                for row in rows {
                    for cell in row {
                        mark_inlines(cell, idx, range);
                    }
                }
            }
            Block::CodeBlock { .. } | Block::Rule => {}
        }
    }
}

fn mark_inlines(inlines: &mut Vec<Inline>, idx: usize, range: (i32, i32)) {
    let (rs, re) = range;
    let mut out: Vec<Inline> = Vec::with_capacity(inlines.len());
    for inline in std::mem::take(inlines) {
        match inline {
            Inline::Text { text, span } => {
                let (ts, te) = span;
                let (os, oe) = (rs.max(ts), re.min(te));
                if os >= oe {
                    out.push(Inline::Text { text, span });
                    continue;
                }
                // Split on CHAR boundaries: `span` counts rendered chars, which is
                // what the mapper answers in, so a byte index would tear multi-byte
                // text apart at the first accented character.
                let chars: Vec<char> = text.chars().collect();
                let cut = |a: i32, b: i32| -> String {
                    chars[(a - ts).max(0) as usize..(b - ts).max(0) as usize]
                        .iter()
                        .collect()
                };
                let before = cut(ts, os);
                let mid = cut(os, oe);
                let after = cut(oe, te);
                if !before.is_empty() {
                    out.push(Inline::Text {
                        text: before,
                        span: (ts, os),
                    });
                }
                out.push(Inline::Claim(
                    idx,
                    vec![Inline::Text {
                        text: mid,
                        span: (os, oe),
                    }],
                ));
                if !after.is_empty() {
                    out.push(Inline::Text {
                        text: after,
                        span: (oe, te),
                    });
                }
            }
            Inline::Emphasis(mut v) => {
                mark_inlines(&mut v, idx, range);
                out.push(Inline::Emphasis(v));
            }
            Inline::Strong(mut v) => {
                mark_inlines(&mut v, idx, range);
                out.push(Inline::Strong(v));
            }
            Inline::Strikethrough(mut v) => {
                mark_inlines(&mut v, idx, range);
                out.push(Inline::Strikethrough(v));
            }
            Inline::Superscript(mut v) => {
                mark_inlines(&mut v, idx, range);
                out.push(Inline::Superscript(v));
            }
            Inline::Subscript(mut v) => {
                mark_inlines(&mut v, idx, range);
                out.push(Inline::Subscript(v));
            }
            Inline::Highlight(mut v) => {
                mark_inlines(&mut v, idx, range);
                out.push(Inline::Highlight(v));
            }
            Inline::Link {
                href,
                title,
                mut inner,
            } => {
                mark_inlines(&mut inner, idx, range);
                out.push(Inline::Link { href, title, inner });
            }
            other => out.push(other),
        }
    }
    *inlines = out;
}

/// Read an image the containment gate already admitted, and infer its type from its
/// **magic number** rather than its filename — an untrusted document chooses the
/// extension, so trusting it would let the document mislabel what a reader's browser
/// is told to decode.
///
/// Goes through `limits::is_regular_file_within_limit` like every other read of a
/// path this application does not control: both halves, never a size test alone,
/// because a size test admits a FIFO whose reported length is zero (POLICY § Input
/// limits).
pub(super) fn read_image(path: &std::path::Path) -> Option<(Vec<u8>, &'static str)> {
    let meta = std::fs::metadata(path).ok()?;
    crate::limits::is_regular_file_within_limit(&meta).ok()?;
    let bytes = std::fs::read(path).ok()?;
    let mime = sniff_image_mime(&bytes)?;
    Some((bytes, mime))
}

/// The image type a byte string actually is. `None` for anything unrecognised, which
/// becomes a placeholder rather than an embed — an export never asserts a type it did
/// not verify.
fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }
    // SVG is text, so it is accepted only on an XML or `<svg` lead-in — after a BOM
    // and leading whitespace — rather than by extension, so an arbitrary text file is
    // never embedded as an image.
    let head = bytes.get(..512).unwrap_or(bytes);
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    let text = String::from_utf8_lossy(head);
    let trimmed = text.trim_start();
    if trimmed.starts_with("<?xml") || trimmed.starts_with("<svg") {
        return Some("image/svg+xml");
    }
    None
}

#[cfg(test)]
mod export_doc_tests {
    use super::*;
    use crate::export::{Align, ImageSource, RenderOptions};

    fn doc_of(md: &str) -> ExportDoc {
        build(md, &RenderOptions::default())
    }

    /// The plain text of a whole document, for assertions about content rather than
    /// shape.
    fn text_of(doc: &ExportDoc) -> String {
        fn blocks(bs: &[Block], out: &mut String) {
            for b in bs {
                match b {
                    Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
                        out.push_str(&crate::export::plain_text(inlines));
                        out.push('\n');
                    }
                    Block::CodeBlock { text, .. } => out.push_str(text),
                    Block::BlockQuote(inner) => blocks(inner, out),
                    Block::Disclosure { summary, body, .. } => {
                        out.push_str(&crate::export::plain_text(summary));
                        out.push('\n');
                        blocks(body, out);
                    }
                    Block::List { items, .. } => {
                        for i in items {
                            blocks(&i.blocks, out);
                        }
                    }
                    Block::Table { head, rows, .. } => {
                        for c in head {
                            out.push_str(&crate::export::plain_text(c));
                            out.push('\n');
                        }
                        for r in rows {
                            for c in r {
                                out.push_str(&crate::export::plain_text(c));
                                out.push('\n');
                            }
                        }
                    }
                    Block::Rule => out.push_str("---\n"),
                }
            }
        }
        let mut out = String::new();
        blocks(&doc.blocks, &mut out);
        out
    }

    #[test]
    fn headings_carry_their_level_and_a_unique_anchor_slug() {
        let doc = doc_of("# Title\n\n## Notes\n\n## Notes\n");
        let ids: Vec<(u8, String)> = doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading { level, id, .. } => Some((*level, id.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            ids,
            vec![
                (1, "title".to_string()),
                (2, "notes".to_string()),
                (2, "notes-1".to_string())
            ]
        );
        assert_eq!(doc.title.as_deref(), Some("Title"));
    }

    #[test]
    fn h6_keeps_its_own_level_rather_than_folding_into_h5() {
        // The preview folds h6 onto the h5 TAG because it has five heading tags. An
        // export has no such constraint, so the level the document stated survives.
        let doc = doc_of("###### Deep\n");
        assert!(matches!(
            doc.blocks.as_slice(),
            [Block::Heading { level: 6, .. }]
        ));
    }

    /// Document Rendering CAM row 17 — exports as it renders.
    ///
    /// A `~~ … ~~` fence wrapping other inline markup is split across events by
    /// pulldown; the export walks the same block-scope table the preview does, so
    /// the artefact must carry the strike rather than two literal `~~`.
    #[test]
    fn a_fence_wrapping_inline_markup_exports_struck() {
        let doc = doc_of("~~a **bold** b~~\n");
        let Some(Block::Paragraph(inlines)) = doc.blocks.first() else {
            panic!("expected a paragraph, got {:?}", doc.blocks);
        };
        // Every rendered run is struck, and no `~~` reaches the artefact.
        assert!(
            inlines
                .iter()
                .all(|i| matches!(i, Inline::Strikethrough(_) | Inline::Strong(_))),
            "unstruck run in {inlines:?}"
        );
        assert!(
            !crate::export::plain_text(inlines).contains('~'),
            "a delimiter reached the export: {inlines:?}"
        );
        assert_eq!(crate::export::plain_text(inlines), "a bold b");
    }

    #[test]
    fn the_four_tight_constructs_pulldown_never_sees_become_their_own_inlines() {
        // Each arrives from pulldown as plain `Text`; only `scan_scripts` knows what
        // it is (`renderer::segments`, ScrAP-66/ScrAP-195). A second parse would emit
        // all four literally.
        let doc = doc_of("H~2~O and E=mc^2^ and ~~gone~~ and ==marked==\n");
        let Some(Block::Paragraph(inlines)) = doc.blocks.first() else {
            panic!("expected a paragraph, got {:?}", doc.blocks);
        };
        let kinds: Vec<&str> = inlines
            .iter()
            .map(|i| match i {
                Inline::Subscript(_) => "sub",
                Inline::Superscript(_) => "sup",
                Inline::Strikethrough(_) => "strike",
                Inline::Highlight(_) => "mark",
                Inline::Text { .. } => "text",
                _ => "other",
            })
            .collect();
        assert!(kinds.contains(&"sub"), "no subscript in {kinds:?}");
        assert!(kinds.contains(&"sup"), "no superscript in {kinds:?}");
        assert!(kinds.contains(&"strike"), "no strikethrough in {kinds:?}");
        assert!(kinds.contains(&"mark"), "no highlight in {kinds:?}");
        // …and the delimiters never reach the text.
        let text = text_of(&doc);
        for delim in ["~~", "==", "^"] {
            assert!(!text.contains(delim), "{delim} survived into {text:?}");
        }
    }

    #[test]
    fn raw_html_is_dropped_not_escaped_and_not_passed_through() {
        // TDD 25.4: the export reproduces the preview's omission. Escaping would put
        // text on the page the preview never showed; passing through would put
        // executable markup from an untrusted document into a file about to be sent.
        let doc = doc_of(
            "before\n\n<script>alert(1)</script>\n\n<div class=\"x\">hi</div>\n\n\
             <iframe src=\"http://e.example\"></iframe>\n\nafter\n",
        );
        let text = text_of(&doc);
        assert!(text.contains("before") && text.contains("after"));
        for dropped in ["script", "alert", "iframe", "div", "<", ">"] {
            assert!(
                !text.contains(dropped),
                "{dropped:?} survived the drop: {text:?}"
            );
        }
    }

    /// **Regression.** A **tight** list item's content arrives from pulldown-cmark as
    /// bare inline events with **no `Tag::Paragraph` around them** — unlike a loose
    /// item, which is wrapped. An item's inlines must still land in ONE paragraph.
    ///
    /// The defect this pins shipped: every inline event in a tight item became its own
    /// block, so an item containing inline code, a link or a soft break was exploded
    /// into one paragraph per token — visible in both sinks as a line break after
    /// almost every word.
    ///
    /// **Why the original tests missed it**, which is the part worth keeping: they all
    /// used single-word items (`- one`), and for an item whose content is a *single*
    /// inline event the broken path produces exactly one paragraph — indistinguishable
    /// from correct. The bug needs **two or more** inline events in one item to appear
    /// at all, so the fixture must have them.
    #[test]
    fn a_tight_list_item_with_several_inlines_is_one_paragraph_not_one_per_token() {
        let doc = doc_of("1. **Bold lead.** See `POLICY.md`\n   and then some more prose.\n");
        let Some(Block::List { items, .. }) = doc.blocks.first() else {
            panic!("expected a list, got {:?}", doc.blocks);
        };
        assert_eq!(items.len(), 1, "one item");
        assert_eq!(
            items[0].blocks.len(),
            1,
            "a tight item's content is ONE paragraph, got {} blocks: {:?}",
            items[0].blocks.len(),
            items[0].blocks
        );
        let Some(Block::Paragraph(inlines)) = items[0].blocks.first() else {
            panic!("expected a paragraph, got {:?}", items[0].blocks);
        };
        // …and it holds every one of its inlines, in order.
        assert!(
            inlines.len() >= 4,
            "the paragraph should hold the strong run, the code span and the prose \
             around them, got {inlines:?}"
        );
        let text = crate::export::plain_text(inlines);
        assert!(text.contains("Bold lead."), "{text:?}");
        assert!(text.contains("POLICY.md"), "{text:?}");
        assert!(text.contains("and then some more prose."), "{text:?}");
    }

    /// The same defect through its other trigger: a **link** inside a tight item, which
    /// is the shape the real document that surfaced this used.
    #[test]
    fn a_tight_item_containing_a_link_stays_one_paragraph() {
        let doc = doc_of("1. Parity is a tax. [`POLICY.md`](POLICY.md) requires one key.\n");
        let Some(Block::List { items, .. }) = doc.blocks.first() else {
            panic!("expected a list");
        };
        assert_eq!(items[0].blocks.len(), 1, "got {:?}", items[0].blocks);
    }

    /// A **loose** item — one pulldown *does* wrap in `Tag::Paragraph` — must be
    /// unaffected, and a multi-paragraph item must keep both paragraphs.
    #[test]
    fn a_loose_item_keeps_each_of_its_paragraphs() {
        let doc = doc_of("1. First para.\n\n   Second para.\n\n2. Another item.\n");
        let Some(Block::List { items, .. }) = doc.blocks.first() else {
            panic!("expected a list, got {:?}", doc.blocks);
        };
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].blocks.len(),
            2,
            "a loose item with two paragraphs keeps both, got {:?}",
            items[0].blocks
        );
    }

    /// A nested list inside a tight item: the item's own text must be flushed into its
    /// paragraph *before* the sublist, not merged into it or dropped.
    #[test]
    fn a_tight_item_with_a_sublist_flushes_its_own_text_first() {
        let doc = doc_of("- outer text with `code`\n  - inner\n");
        let Some(Block::List { items, .. }) = doc.blocks.first() else {
            panic!("expected a list");
        };
        let blocks = &items[0].blocks;
        assert_eq!(blocks.len(), 2, "a paragraph then a list, got {blocks:?}");
        assert!(matches!(blocks[0], Block::Paragraph(_)), "{blocks:?}");
        assert!(matches!(blocks[1], Block::List { .. }), "{blocks:?}");
        let Block::Paragraph(inlines) = &blocks[0] else {
            unreachable!()
        };
        assert!(crate::export::plain_text(inlines).contains("outer text with code"));
    }

    #[test]
    fn a_task_list_item_carries_its_checkbox_state() {
        let doc = doc_of("- [x] done\n- [ ] todo\n");
        let Some(Block::List { items, .. }) = doc.blocks.first() else {
            panic!("expected a list, got {:?}", doc.blocks);
        };
        assert_eq!(
            items.iter().map(|i| i.task).collect::<Vec<_>>(),
            vec![Some(true), Some(false)]
        );
    }

    #[test]
    fn a_table_keeps_its_head_rows_and_alignments() {
        let doc = doc_of("| a | b |\n|:--|--:|\n| 1 | 2 |\n| 3 | 4 |\n");
        let Some(Block::Table { aligns, head, rows }) = doc.blocks.first() else {
            panic!("expected a table, got {:?}", doc.blocks);
        };
        assert_eq!(aligns, &vec![Align::Left, Align::Right]);
        assert_eq!(head.len(), 2);
        assert_eq!(rows.len(), 2, "two body rows");
        assert_eq!(rows[0].len(), 2);
    }

    #[test]
    fn nesting_survives_a_list_inside_a_quote_inside_a_list() {
        // Document Rendering CAM row 2: every container context, including the
        // combinations. The stacks make this fall out rather than need a case.
        let doc = doc_of("- outer\n  > quoted\n  >\n  > - inner\n");
        let Some(Block::List { items, .. }) = doc.blocks.first() else {
            panic!("expected a list, got {:?}", doc.blocks);
        };
        let quoted = items[0]
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::BlockQuote(inner) => Some(inner),
                _ => None,
            })
            .expect("a block quote inside the item");
        assert!(
            quoted.iter().any(|b| matches!(b, Block::List { .. })),
            "a list inside the quote inside the item: {quoted:?}"
        );
    }

    #[test]
    fn a_code_fence_keeps_its_language_and_its_text_verbatim() {
        // `~~` and `==` inside a fence are code, not constructs.
        let doc = doc_of("```rust\nlet x = a ~~ b == c;\n```\n");
        let Some(Block::CodeBlock { lang, text }) = doc.blocks.first() else {
            panic!("expected a code block, got {:?}", doc.blocks);
        };
        assert_eq!(lang.as_deref(), Some("rust"));
        assert_eq!(text, "let x = a ~~ b == c;\n");
    }

    #[test]
    fn a_link_with_a_refused_scheme_keeps_its_text_and_loses_its_destination() {
        let doc = doc_of("[click](javascript:alert(1)) and [ok](https://example.com)\n");
        let Some(Block::Paragraph(inlines)) = doc.blocks.first() else {
            panic!("expected a paragraph");
        };
        let hrefs: Vec<String> = inlines
            .iter()
            .filter_map(|i| match i {
                Inline::Link { href, .. } => Some(href.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(hrefs, vec!["https://example.com".to_string()]);
        assert!(text_of(&doc).contains("click"), "the text survives");
    }

    #[test]
    fn an_annotated_claim_is_marked_over_exactly_its_own_characters() {
        // TDD 25.13's extent clause, through the SAME mapper the preview uses.
        let doc = doc_of("The {==sky is green==}{>>No, it is blue.<<} today.\n");
        assert_eq!(doc.annotations.len(), 1);
        assert_eq!(doc.annotations[0].comment, "No, it is blue.");
        assert_eq!(doc.annotations[0].claim, "sky is green");
        let Some(Block::Paragraph(inlines)) = doc.blocks.first() else {
            panic!("expected a paragraph, got {:?}", doc.blocks);
        };
        let claimed: Vec<String> = inlines
            .iter()
            .filter_map(|i| match i {
                Inline::Claim(_, v) => Some(crate::export::plain_text(v)),
                _ => None,
            })
            .collect();
        assert_eq!(claimed, vec!["sky is green".to_string()]);
        // The CriticMarkup delimiters never reach the document text.
        let text = text_of(&doc);
        assert!(!text.contains("{==") && !text.contains("<<"), "{text:?}");
        assert!(text.contains("The sky is green today."), "{text:?}");
    }

    #[test]
    fn a_bare_highlight_without_a_comment_is_not_an_annotation() {
        // `is_listed`'s predicate — the same one the chips and the viewer use.
        let doc = doc_of("A {==claim==} with no comment.\n");
        assert!(doc.annotations.is_empty(), "{:?}", doc.annotations);
    }

    #[test]
    fn a_remote_image_is_referenced_and_never_fetched_when_the_gate_is_off() {
        // Gate off: refused, exactly as the preview refuses it.
        let doc = build(
            "![a](https://example.com/x.png)\n",
            &RenderOptions {
                doc_dir: None,
                allow_unsafe_images: false,
            },
        );
        let img = find_image(&doc).expect("an image");
        assert!(
            matches!(&img.source, ImageSource::Missing(r) if r.contains("Show Unsafe Images")),
            "{:?}",
            img.source
        );
        assert!(!doc.has_unembedded_remote_images);
    }

    #[test]
    fn a_remote_image_with_the_gate_on_is_referenced_by_url_not_downloaded() {
        let doc = build(
            "![a](https://example.com/x.png)\n",
            &RenderOptions {
                doc_dir: None,
                allow_unsafe_images: true,
            },
        );
        let img = find_image(&doc).expect("an image");
        assert_eq!(
            img.source,
            ImageSource::Remote("https://example.com/x.png".to_string())
        );
        // The flag is what lets a sink that cannot follow a URL say so (TDD 25.12).
        assert!(doc.has_unembedded_remote_images);
    }

    fn find_image(doc: &ExportDoc) -> Option<crate::export::ImageRef> {
        fn walk(bs: &[Block]) -> Option<crate::export::ImageRef> {
            for b in bs {
                let inlines = match b {
                    Block::Paragraph(i) | Block::Heading { inlines: i, .. } => i,
                    Block::BlockQuote(inner) => return walk(inner),
                    _ => continue,
                };
                for i in inlines {
                    if let Inline::Image(img) = i {
                        return Some(img.clone());
                    }
                }
            }
            None
        }
        walk(&doc.blocks)
    }
}

#[cfg(test)]
mod export_independence_tests {
    use super::build;
    use crate::export::RenderOptions;

    /// **TDD 25.2** — a never-rendered tab exports identically.
    ///
    /// The strongest form of this rubric is structural rather than behavioural: the
    /// builder's inputs are the document's **text** and its options, and nothing else
    /// is reachable from here. There is no preview to consult, no widget to read, no
    /// buffer to be allocated — so "the same source exports the same bytes whatever the
    /// reader did beforehand" is a property of the signature, and this test pins that
    /// the signature has not quietly grown a second input.
    #[test]
    fn the_same_source_produces_the_same_model_every_time() {
        const SRC: &str = "# T\n\nBody with **bold** and a [link](https://e.example).\n\n\
            - one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        let a = build(SRC, &RenderOptions::default());
        let b = build(SRC, &RenderOptions::default());
        assert_eq!(a, b, "the builder is not a pure function of its inputs");
        assert!(!a.blocks.is_empty(), "and it produced something");
    }

    /// **TDD 25.5** — an unsaved buffer exports the buffer, not the file on disk.
    ///
    /// Also structural: the builder takes the text, never a path to read. A source with
    /// no corresponding file on disk exports exactly as one with a file does, and there
    /// is no code path by which the disk could be consulted for content.
    #[test]
    fn a_source_with_no_file_behind_it_exports_its_own_text() {
        let doc = build(
            "# Never saved\n\nTyped just now.\n",
            &RenderOptions {
                doc_dir: None,
                allow_unsafe_images: false,
            },
        );
        assert_eq!(doc.title.as_deref(), Some("Never saved"));
        assert_eq!(doc.blocks.len(), 2);
    }

    /// A document's text is what reaches the artefact, so a change to the text changes
    /// the artefact — the other half of 25.5, which the equality test above cannot show.
    #[test]
    fn editing_the_source_changes_what_is_exported() {
        let before = build("original\n", &RenderOptions::default());
        let after = build("edited\n", &RenderOptions::default());
        assert_ne!(before, after);
    }
}

#[cfg(test)]
mod disclosure_export_tests {
    use super::build;
    use crate::export::{Block, RenderOptions};

    const MD: &str = concat!(
        "# Doc\n\n",
        "<details>\n<summary>Show me</summary>\n\n",
        "body **text**\n\n",
        "</details>\n\n",
        "after\n"
    );

    fn doc_of(md: &str) -> crate::export::ExportDoc {
        build(md, &RenderOptions::default())
    }

    /// **Rubric 2.26g — a disclosure exports as it renders.**
    ///
    /// MEASURED before this: the body exported fine, because it is ordinary Markdown
    /// events the walk never had to be taught, and the SUMMARY was dropped entirely —
    /// so every artefact was missing the one piece of the construct that names it,
    /// while still opening looking finished (Document Rendering CAM row 17).
    #[test]
    fn a_disclosures_summary_and_body_both_reach_the_model() {
        let doc = doc_of(MD);
        let disclosure = doc
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Disclosure { summary, body, .. } => Some((summary, body)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected a Disclosure block, got {:?}", doc.blocks));
        assert_eq!(
            crate::export::plain_text(disclosure.0),
            "Show me",
            "the summary label reaches the artefact"
        );
        assert!(
            matches!(disclosure.1.first(), Some(Block::Paragraph(_))),
            "the body is nested inside it, as Markdown: {:?}",
            disclosure.1
        );
        // And the content around it is untouched.
        assert!(matches!(doc.blocks.first(), Some(Block::Heading { .. })));
        assert!(matches!(doc.blocks.last(), Some(Block::Paragraph(_))));
    }

    /// A collapsed block exports exactly as an open one does. The preview's fold state
    /// is not an input to this builder at all — the document's own `open` attribute is
    /// the only thing that differs, and only in the flag a sink may offer.
    #[test]
    fn the_readers_fold_state_is_not_an_input_to_the_export() {
        let closed = doc_of(MD);
        let opened = doc_of(&MD.replace("<details>", "<details open>"));
        let body_of = |d: &crate::export::ExportDoc| {
            d.blocks.iter().find_map(|b| match b {
                Block::Disclosure { body, .. } => Some(body.clone()),
                _ => None,
            })
        };
        assert_eq!(
            body_of(&closed),
            body_of(&opened),
            "the same body, whichever way the document asked it to be shown"
        );
        let open_flag = |d: &crate::export::ExportDoc| {
            d.blocks.iter().find_map(|b| match b {
                Block::Disclosure { open, .. } => Some(*open),
                _ => None,
            })
        };
        assert_eq!(open_flag(&closed), Some(false));
        assert_eq!(open_flag(&opened), Some(true));
    }

    /// Nesting falls out of the frame stack, exactly as it does for a blockquote in a
    /// list item — so a disclosure inside a disclosure needs no case of its own.
    #[test]
    fn a_nested_disclosure_nests_in_the_model() {
        let doc = doc_of(concat!(
            "<details>\n<summary>Outer</summary>\n\n",
            "<details>\n<summary>Inner</summary>\n\ninner body\n\n</details>\n\n",
            "</details>\n"
        ));
        let Some(Block::Disclosure { body, .. }) = doc.blocks.first() else {
            panic!("expected an outer Disclosure, got {:?}", doc.blocks);
        };
        assert!(
            body.iter().any(|b| matches!(b, Block::Disclosure { .. })),
            "the inner block nests inside the outer one: {body:?}"
        );
    }

    /// An UNCLOSED `<details>` groups nothing — the same recovery the preview applies
    /// (rubric 2.26d), and it is the same pre-scan that decides it in both, so the two
    /// cannot disagree about which blocks are real.
    #[test]
    fn an_unclosed_disclosure_groups_nothing_and_loses_nothing() {
        let doc = doc_of("<details>\n<summary>Never closed</summary>\n\nbody\n\n## After\n");
        assert!(
            !doc.blocks
                .iter()
                .any(|b| matches!(b, Block::Disclosure { .. })),
            "an unpaired block is not a disclosure: {:?}",
            doc.blocks
        );
        assert!(
            doc.blocks
                .iter()
                .any(|b| matches!(b, Block::Heading { .. })),
            "and nothing after it is swallowed: {:?}",
            doc.blocks
        );
    }

    /// A `<details>` with no `<summary>` takes the same default label the preview
    /// shows, so the two do not name one construct two different things.
    #[test]
    fn a_summaryless_disclosure_takes_the_default_label() {
        let doc = doc_of("<details>\n\nbody\n\n</details>\n");
        let Some(Block::Disclosure { summary, .. }) = doc.blocks.first() else {
            panic!("expected a Disclosure, got {:?}", doc.blocks);
        };
        assert_eq!(
            crate::export::plain_text(summary),
            crate::renderer::DEFAULT_SUMMARY_LABEL
        );
    }
}
