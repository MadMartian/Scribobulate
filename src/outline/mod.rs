//! Document outline (table-of-contents) model.
//!
//! A *display-free* walk over the Markdown source that collects the document's
//! heading hierarchy — level, plain display text, and source byte offset — for
//! the outline sidebar (`window.rs`).  Kept free of any GTK type so it can be
//! unit-tested under POLICY's no-live-display coverage gate, like `source_slice`
//! / `finalize_source_map` in `preview.rs`.
//!
//! The source offset (rather than a preview-buffer position) is the anchor on
//! purpose: the outline must appear in *all three* view modes, and pure-edit
//! mode has no rendered preview to hang a `GtkTextMark` on.  A source offset maps
//! trivially to the editor buffer, and the preview maps it through its existing
//! `source_map` — so one extraction drives navigation in every mode.

mod tree;

pub(crate) use tree::{ancestor_chain, build_tree, HeadingNode};

use crate::span::{CleanedByteOffset, OriginalByteOffset};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

/// One heading in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Heading {
    /// Heading tier, 1–6 (H1…H6).
    pub level: u8,
    /// Plain display text: the concatenation of the heading's inline text and
    /// code runs, with emphasis/code/link *markup* stripped (e.g. `**bold**`
    /// and `` `code` `` contribute `bold` / `code`).
    pub text: String,
    /// Byte offset into the ORIGINAL source where the heading block begins (the `#`).
    pub src_offset: OriginalByteOffset,
}

/// Where a heading is reachable in the rendered preview, for **every** heading the
/// source declares — including the ones a collapsed disclosure is hiding.
///
/// The outline models the DOCUMENT and the preview renders a VIEW of it, and those
/// two disagree the moment a disclosure is collapsed: its body is not rendered at
/// all, so the headings inside it produce nothing in the buffer. A render product
/// listing only the headings that reached the buffer is therefore SHORTER than the
/// outline's own list, and every `doc_index` past the disclosure then indexes a
/// different heading — MEASURED on `# A` / a collapsed block holding `## Hidden` /
/// `# B`: activating "Hidden" scrolled to "B" and activating "B" did nothing. A
/// wrong navigation, silently, which is worse than none (rubric 12.22's "half-working
/// is worse than obviously broken").
///
/// One entry per SOURCE heading is what removes that class: the two lists are the
/// same length by construction, so no index can slip. A hidden heading is not absent
/// — it carries the summary line of the collapsed block that hides it, which is the
/// nearest position a reader can actually see, plus the fold to expand to reach the
/// heading itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeadingSite {
    /// Buffer char offset to scroll to: the heading's own text, or — when it is
    /// hidden — the summary line of the outermost collapsed disclosure above it.
    ///
    /// Always present, and non-decreasing across the list, so the scroll-spy's
    /// binary search over document order still holds.
    pub offset: i32,
    /// The collapsed disclosures hiding this heading, outermost first, or empty
    /// when it is rendered. **All** of them must be expanded before the heading
    /// itself can be scrolled to (rubric 12.22) — a collapsed block nested inside
    /// another renders nothing, not even its own summary line, so opening the outer
    /// one alone still leaves the heading unreachable.
    pub hidden_by: Vec<crate::fold::FoldKey>,
    /// The heading's anchor slug, or `None` when it is hidden — a slug names a
    /// buffer position, and a hidden heading has none of its own.
    pub slug: Option<String>,
}

fn level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Walk the Markdown source and return its headings in document order.
///
/// Display text mirrors what `renderer.rs` accumulates into a heading's anchor
/// slug: inline `Text` and `Code` runs are concatenated; emphasis, strikethrough,
/// and link wrappers contribute their inner text but not their markers.  An
/// empty document — or one with no headings — yields an empty vector (the panel
/// renders that as a muted "No headings" placeholder, not an error).
pub(crate) fn extract_headings(md: &str) -> Vec<Heading> {
    let mut headings: Vec<Heading> = Vec::new();
    // While inside a heading: (level, source-start offset, accumulated text).
    let mut current: Option<(u8, usize, String)> = None;

    // Read the document the way the preview does — and the page's reading is TWO
    // pre-passes, not one.
    //
    // 1. The inline-tab pre-pass (ScrAP-75). Skipping it made the outline disagree with
    //    the rendered page about the document's block structure — MEASURED, a
    //    tab-padded GFM table followed by a setext underline: the preview showed a table
    //    and no heading, while the outline listed a phantom H1 whose text was the whole
    //    table. A tab inside a heading's own text diverged too (`# Chapter\tOne` → the
    //    sidebar showed the tab, the page a space). The substitution is length- and
    //    position-preserving.
    // 2. The CriticMarkup lift. This one was MISSING, and the miss was visible: to
    //    pulldown-cmark CriticMarkup is plain text, so `# {==Chapter One==}{>>revisit<<}`
    //    put the whole marked-up string in the sidebar while the page and the PDF both
    //    showed `Chapter One`. Both of those run the lift; the outline did not, and the
    //    outline's source is the RAW document in every mode, so nothing upstream saved
    //    it.
    //
    // Unlike the tab pre-pass, the lift DELETES bytes, so a heading's offset in the
    // cleaned text is not its offset in the source the caller navigates against. That
    // is what `cleaned_to_original` is for, and the offset newtypes make forgetting it
    // a compile error rather than a scroll that lands in the wrong place.
    let md_norm = crate::renderer::NormalizedMd::new(md);
    let extraction = crate::annotate::extract(md_norm.as_str());
    let md = extraction.cleaned.as_str();

    // A tight construct's markers must be dropped from the label exactly as the
    // renderer drops them, including one whose fence wraps other inline markup —
    // so the outline consults the same block-scope table the preview does.
    let scripts = crate::renderer::BlockScripts::scan(md);

    for (ev, range) in Parser::new_ext(md, crate::renderer::md_options()).into_offset_iter() {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some((level_to_u8(level), range.start, String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, src_offset, text)) = current.take() {
                    headings.push(Heading {
                        level,
                        text: text.trim().to_string(),
                        src_offset: crate::annotate::cleaned_to_original(
                            &extraction.shifts,
                            CleanedByteOffset::new(src_offset),
                        ),
                    });
                }
            }
            Event::Text(t) => {
                if let Some((_, _, ref mut text)) = current {
                    // Mirror renderer.rs: drop tight construct markers so the
                    // outline label and heading slug match the rendered heading.
                    for seg in scripts.segments(range.start, &t) {
                        if !seg.marker {
                            text.push_str(seg.text(&t));
                        }
                    }
                }
            }
            Event::Code(t) => {
                if let Some((_, _, ref mut text)) = current {
                    text.push_str(&t);
                }
            }
            // Every other construct in pulldown-cmark's vocabulary contributes
            // nothing to a heading's label — spelled out per variant, not `_`, so a
            // parser upgrade that adds one fails to COMPILE here rather than
            // rendering as nothing (the failure mode check 15 exists to catch:
            // `copymap::classify` swallowed raw HTML into copied source this same
            // way, for as long as `<picture>` had existed, every gate green).
            // Mirrors `renderer::events::process`'s own exhaustive match.
            Event::Start(
                Tag::Paragraph
                | Tag::BlockQuote(_)
                | Tag::CodeBlock(_)
                | Tag::HtmlBlock
                | Tag::List(_)
                | Tag::Item
                | Tag::FootnoteDefinition(_)
                | Tag::DefinitionList
                | Tag::DefinitionListTitle
                | Tag::DefinitionListDefinition
                | Tag::Table(_)
                | Tag::TableHead
                | Tag::TableRow
                | Tag::TableCell
                | Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Superscript
                | Tag::Subscript
                | Tag::Link { .. }
                | Tag::Image { .. }
                | Tag::MetadataBlock(_),
            )
            | Event::End(
                TagEnd::Paragraph
                | TagEnd::BlockQuote(_)
                | TagEnd::CodeBlock
                | TagEnd::HtmlBlock
                | TagEnd::List(_)
                | TagEnd::Item
                | TagEnd::FootnoteDefinition
                | TagEnd::DefinitionList
                | TagEnd::DefinitionListTitle
                | TagEnd::DefinitionListDefinition
                | TagEnd::Table
                | TagEnd::TableHead
                | TagEnd::TableRow
                | TagEnd::TableCell
                | TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Superscript
                | TagEnd::Subscript
                | TagEnd::Link
                | TagEnd::Image
                | TagEnd::MetadataBlock(_),
            )
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::TaskListMarker(_) => {}
        }
    }
    headings
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sidebar label must match the rendered heading, including a fence that
    /// WRAPS other inline markup — pulldown splits that across events, so a label
    /// built one event at a time keeps the literal `~~` the page does not show.
    #[test]
    fn a_heading_drops_the_markers_of_a_markup_wrapping_fence() {
        assert_eq!(
            extract_headings("# ~~a **bold** b~~\n")
                .first()
                .map(|h| h.text.clone()),
            Some("a bold b".to_string()),
        );
    }

    #[test]
    fn nested_headings_keep_level_text_and_order() {
        let md = "# Top\n\n## Middle\n\n### Deep\n\n## Another";
        let hs = extract_headings(md);
        assert_eq!(
            hs.iter()
                .map(|h| (h.level, h.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, "Top"), (2, "Middle"), (3, "Deep"), (2, "Another")],
        );
        // Offsets are strictly increasing in document order.
        assert!(hs.windows(2).all(|w| w[0].src_offset < w[1].src_offset));
    }

    #[test]
    fn criticmarkup_is_lifted_out_of_a_heading_label_the_way_the_page_lifts_it() {
        // THE divergence (QA F-VIEW-001). CriticMarkup is plain text to pulldown-cmark,
        // so a heading carrying an annotation listed the whole marked-up string in the
        // sidebar while the page and the PDF — which both run the lift — showed the
        // clean text. The outline's source is the raw document in every mode, so there
        // was no upstream strip to save it.
        let md = "# {==Chapter One==}{>>revisit<<}\n\nBody.\n";
        let hs = extract_headings(md);
        assert_eq!(hs.len(), 1);
        assert_eq!(hs[0].text, "Chapter One");
    }

    #[test]
    fn a_heading_after_criticmarkup_still_points_at_the_original_source() {
        // The half a naive strip gets wrong. The lift DELETES bytes, so an offset taken
        // in the cleaned text is short by everything removed before it — and the caller
        // navigates the ORIGINAL. Without the mapping this heading's offset lands 22
        // bytes early, inside the previous paragraph, and the sidebar scrolls to the
        // wrong place with nothing failing.
        let md = "{==Some claim==}{>>a note<<}\n\n# Later Heading\n";
        let hs = extract_headings(md);
        assert_eq!(hs.len(), 1);
        let want = md.find("# Later Heading").expect("fixture");
        assert_eq!(
            hs[0].src_offset.raw(),
            want,
            "heading offset must index the original source, not the cleaned text"
        );
        assert_eq!(&md[hs[0].src_offset.raw()..][..1], "#");
    }

    #[test]
    fn no_headings_yields_empty() {
        assert!(extract_headings("").is_empty());
        assert!(extract_headings("Just a paragraph.\n\nAnd another.").is_empty());
    }

    #[test]
    fn inline_formatting_is_stripped_from_display_text() {
        let md = "# A **bold** and `code` and _em_ title";
        let hs = extract_headings(md);
        assert_eq!(hs.len(), 1);
        assert_eq!(hs[0].text, "A bold and code and em title");
    }

    #[test]
    fn link_in_heading_keeps_only_its_text() {
        let md = "## See [the docs](https://example.com) here";
        let hs = extract_headings(md);
        assert_eq!(hs.len(), 1);
        assert_eq!(hs[0].text, "See the docs here");
    }

    #[test]
    fn all_six_levels_map_correctly() {
        let md = "# 1\n\n## 2\n\n### 3\n\n#### 4\n\n##### 5\n\n###### 6";
        let levels: Vec<u8> = extract_headings(md).iter().map(|h| h.level).collect();
        assert_eq!(levels, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn duplicate_titles_are_each_recorded() {
        let md = "# Intro\n\nbody\n\n# Intro\n\nmore";
        let hs = extract_headings(md);
        assert_eq!(hs.len(), 2);
        assert_eq!(hs[0].text, "Intro");
        assert_eq!(hs[1].text, "Intro");
        assert_ne!(hs[0].src_offset, hs[1].src_offset);
    }

    #[test]
    fn src_offset_points_at_heading_start() {
        let md = "Intro paragraph.\n\n## Section";
        let hs = extract_headings(md);
        assert_eq!(hs.len(), 1);
        // The offset must land on the heading's '#', so the slice there starts with it.
        assert!(md[hs[0].src_offset.raw()..].starts_with("## Section"));
    }
}
