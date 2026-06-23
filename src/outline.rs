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

use crate::span::OriginalByteOffset;
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

/// A node in the outline tree: a heading plus the headings nested beneath it.
///
/// `doc_index` is the heading's position in the flat document-order list
/// (`extract_headings`'s output) — the stable key the preview/editor use to look
/// up where to scroll, independent of how the tree is expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeadingNode {
    pub level: u8,
    pub text: String,
    pub src_offset: OriginalByteOffset,
    pub doc_index: usize,
    pub children: Vec<HeadingNode>,
}

/// Fold a flat, document-order heading list into a forest by level.
///
/// A heading becomes a child of the nearest preceding heading with a *lower*
/// level; headings with no such ancestor are roots. Level skips (an H1 followed
/// directly by an H3) are tolerated — the H3 simply nests under the H1. The flat
/// order is preserved as a pre-order traversal, and `doc_index` records each
/// heading's original position so callers can map a node back to its buffer/source
/// location regardless of tree shape.
pub(crate) fn build_tree(headings: &[Heading]) -> Vec<HeadingNode> {
    let mut roots: Vec<HeadingNode> = Vec::new();
    // Indices into successive `children` vectors identifying the open ancestor
    // chain (the path from a root down to the most recently inserted node).
    let mut path: Vec<usize> = Vec::new();
    // Parallel record of each ancestor's level, to decide nesting.
    let mut path_levels: Vec<u8> = Vec::new();

    for (doc_index, h) in headings.iter().enumerate() {
        let node = HeadingNode {
            level: h.level,
            text: h.text.clone(),
            src_offset: h.src_offset,
            doc_index,
            children: Vec::new(),
        };
        // Unwind ancestors whose level is >= this heading's: they cannot parent it.
        while path_levels.last().is_some_and(|&l| l >= h.level) {
            path.pop();
            path_levels.pop();
        }
        // Descend `path` to the current parent's children vector and push there.
        let siblings = nav_to_children(&mut roots, &path);
        let new_index = siblings.len();
        siblings.push(node);
        path.push(new_index);
        path_levels.push(h.level);
    }
    roots
}

/// The ancestor chain of the heading at `target` in a flat, document-order level
/// list: the `doc_index`es from the outermost root down to `target` **inclusive**,
/// each a strictly-shallower (smaller-level) heading than the next. This is the
/// same "nearest preceding heading with a lower level, recursively" nesting
/// [`build_tree`] applies — the returned stack's last element is always `target`
/// and its first is `target`'s outermost root ancestor; a root's chain is just
/// `[target]`. Returns an empty vector when `target` is out of range.
///
/// The outline scroll-spy uses this to pick the deepest still-VISIBLE row when the
/// exact heading is hidden under a collapsed node: walk the chain from the end
/// (deepest) and take the first `doc_index` that still has a materialised row —
/// which rises to the nearest visible ancestor on collapse and descends back
/// toward the exact heading as ancestors are re-expanded (a `GtkTreeListModel`
/// collapse frees the descendant rows, so "visible" == "materialised", and the
/// materialised set is always ancestry-closed, making the deepest present chain
/// member unambiguous). The chain index of a member IS its depth (0 = root).
pub(crate) fn ancestor_chain(levels: &[u8], target: usize) -> Vec<usize> {
    if target >= levels.len() {
        return Vec::new();
    }
    // Same stack unwinding as `build_tree`: pop every open ancestor whose level is
    // >= this heading's (it cannot parent it), then push this one. After processing
    // index `target`, the stack IS its root→target ancestor path.
    let mut stack: Vec<usize> = Vec::new();
    for i in 0..=target {
        while stack.last().is_some_and(|&t| levels[t] >= levels[i]) {
            stack.pop();
        }
        stack.push(i);
    }
    stack
}

/// Walk `path` (a chain of child indices) from the roots and return a mutable
/// reference to the `children` vector at its end (the roots themselves when the
/// path is empty) — where the next sibling at that depth should be inserted.
fn nav_to_children<'a>(
    roots: &'a mut Vec<HeadingNode>,
    path: &[usize],
) -> &'a mut Vec<HeadingNode> {
    let mut current = roots;
    for &idx in path {
        current = &mut current[idx].children;
    }
    current
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
                        src_offset: OriginalByteOffset::new(src_offset),
                    });
                }
            }
            Event::Text(t) => {
                if let Some((_, _, ref mut text)) = current {
                    // Mirror renderer.rs: drop tight super/sub-script markers so the
                    // outline label and heading slug match the rendered heading.
                    for (run, _) in crate::renderer::scan_scripts(&t) {
                        text.push_str(&run);
                    }
                }
            }
            Event::Code(t) => {
                if let Some((_, _, ref mut text)) = current {
                    text.push_str(&t);
                }
            }
            _ => {}
        }
    }
    headings
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── build_tree ────────────────────────────────────────────────────────────

    fn h(level: u8, text: &str) -> Heading {
        Heading {
            level,
            text: text.into(),
            src_offset: OriginalByteOffset::new(0),
        }
    }

    #[test]
    fn tree_nests_by_level_and_preserves_doc_index() {
        let flat = vec![h(1, "A"), h(2, "B"), h(3, "C"), h(2, "D")];
        let roots = build_tree(&flat);
        // One root (A) with children B and D; B has child C.
        assert_eq!(roots.len(), 1);
        let a = &roots[0];
        assert_eq!((a.text.as_str(), a.doc_index), ("A", 0));
        assert_eq!(
            a.children
                .iter()
                .map(|n| n.text.as_str())
                .collect::<Vec<_>>(),
            vec!["B", "D"]
        );
        let b = &a.children[0];
        assert_eq!(
            b.children
                .iter()
                .map(|n| n.text.as_str())
                .collect::<Vec<_>>(),
            vec!["C"]
        );
        assert_eq!(b.children[0].doc_index, 2);
        assert_eq!(a.children[1].doc_index, 3); // D
    }

    #[test]
    fn tree_tolerates_level_skips() {
        // H1 then H3 (no H2): the H3 still nests under the H1.
        let flat = vec![h(1, "Top"), h(3, "Deep")];
        let roots = build_tree(&flat);
        assert_eq!(roots.len(), 1);
        assert_eq!(
            roots[0]
                .children
                .iter()
                .map(|n| n.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Deep"]
        );
    }

    #[test]
    fn tree_handles_multiple_roots_and_leading_deep_heading() {
        // A document that opens at H2, with two H2 siblings as roots.
        let flat = vec![h(2, "First"), h(2, "Second")];
        let roots = build_tree(&flat);
        assert_eq!(
            roots.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(),
            vec!["First", "Second"]
        );
        assert!(roots.iter().all(|n| n.children.is_empty()));
    }

    #[test]
    fn tree_of_no_headings_is_empty() {
        assert!(build_tree(&[]).is_empty());
    }

    #[test]
    fn ancestor_chain_of_a_root_is_just_itself() {
        // Flat siblings: each H1 is a root, so its chain is only itself (prior
        // same-or-shallower headings are unwound).
        let levels = [1, 1, 1];
        assert_eq!(ancestor_chain(&levels, 0), vec![0]);
        assert_eq!(ancestor_chain(&levels, 2), vec![2]);
    }

    #[test]
    fn ancestor_chain_walks_root_to_target_inclusive_deepest_last() {
        // H1 > H2 > H3, then H2, then H1  (indices 0..=4).
        let levels = [1, 2, 3, 2, 1];
        // The H3 nests under H2 under H1.
        assert_eq!(ancestor_chain(&levels, 2), vec![0, 1, 2]);
        // The second H2 nests under the H1 (its sibling H2 subtree is unwound).
        assert_eq!(ancestor_chain(&levels, 3), vec![0, 3]);
        // The trailing H1 is a fresh root.
        assert_eq!(ancestor_chain(&levels, 4), vec![4]);
        // Chain position == depth (0 = root), matching the scroll-spy's use.
        let chain = ancestor_chain(&levels, 2);
        assert_eq!(chain.iter().position(|&d| d == 0), Some(0)); // H1 at depth 0
        assert_eq!(chain.iter().position(|&d| d == 2), Some(2)); // H3 at depth 2
    }

    #[test]
    fn ancestor_chain_tolerates_a_level_skip() {
        // H1 immediately followed by H3 (no H2) — the H3 still nests under the H1,
        // matching build_tree's skip tolerance.
        let levels = [1, 3];
        assert_eq!(ancestor_chain(&levels, 1), vec![0, 1]);
    }

    #[test]
    fn ancestor_chain_out_of_range_is_empty() {
        assert!(ancestor_chain(&[1, 2], 5).is_empty());
        assert!(ancestor_chain(&[], 0).is_empty());
    }
}
