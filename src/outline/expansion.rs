//! Which outline nodes the reader has collapsed, in terms that survive a rebuild.
//!
//! # Why this exists
//!
//! `refresh_outline` throws the whole outline widget away and builds a new one — on a tab
//! switch, a view-mode switch, a reload, and every debounced keystroke in live preview. The
//! `GtkTreeListModel` and every `GtkTreeListRow` in it are destroyed with it, and GTK caches
//! nothing: collapsing a row *frees* its subtree rather than remembering it was open
//! (GTK4Rs/AP-111). So expansion cannot be recovered from the widgets after the fact. It has to be
//! recorded as data while the reader is acting, and re-applied to the rows the next build
//! produces.
//!
//! # Why the COLLAPSED set, and not the expanded one
//!
//! The outline opens fully expanded — the model is built `autoexpand=false` and then force-
//! opened, so "expanded" is the default state of every node that has never been touched.
//! Recording the collapsed set therefore makes an unrecognised node fall back to the default
//! **by construction**: a heading this type has never heard of is simply not in the set, and
//! stays open. Recording the expanded set would invert that — every node the reader edited
//! into existence would arrive collapsed, and the failure would grow with the document.
//!
//! # Why a title path, and what it costs
//!
//! A node needs a name that means the same thing before and after a rebuild, and the three
//! candidates already in the tree are all positional: `doc_index` is an index into a list
//! every render rebuilds (`outline_nav.rs` calls it "the weakest reference there is"),
//! `src_offset` moves when any character above it is typed, and a child-index path shifts the
//! moment a heading is inserted above a sibling. All three survive a tab switch and corrupt
//! under an edit — and an edit is the common case here, because live preview rebuilds the
//! outline as the reader types.
//!
//! A path of heading TITLES inverts that trade. Inserting, deleting or reordering *other*
//! headings leaves a node's path untouched, so the sections the reader collapsed stay
//! collapsed. **Renaming a heading — or any of its ancestors — loses its entry, and the node
//! re-expands.** That is the deliberate direction to fail in: a section that springs open is
//! a visible, self-correcting annoyance, where a stale key silently collapses the *wrong*
//! section and the reader cannot tell why.
//!
//! [`PathStep::nth`] separates siblings that share a title, so two "Notes" subsections under
//! one parent are distinct keys rather than one key that moves both together.

use super::HeadingNode;
use std::collections::{BTreeMap, BTreeSet};

/// One step of a heading's path: its title, and which same-titled sibling it is.
///
/// `nth` counts only siblings sharing this exact title under the same parent, so it is `0`
/// for the overwhelming majority of headings and only becomes interesting for genuine
/// duplicates. Counting *all* siblings instead would make the key positional again — the
/// thing this whole type exists to avoid — because inserting an unrelated sibling above
/// would renumber everything after it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PathStep {
    pub title: String,
    pub nth: usize,
}

/// A heading's identity: the path of titles from a root heading down to it, inclusive.
pub(crate) type HeadingPath = Vec<PathStep>;

/// The paths of every heading in the tree, indexed by `doc_index`.
///
/// Document order, so the returned vector can be indexed directly by the `doc_index` the
/// widget layer carries on each row — that index is a fine *transient* handle within one
/// build, which is all this is used for. The durable name is the path it maps to.
///
/// Returns a `Vec` rather than a map because `doc_index` is dense and assigned in this very
/// order (`tree::build_tree`), so the vector is total and a lookup cannot miss.
pub(crate) fn paths_in_document_order(roots: &[HeadingNode]) -> Vec<HeadingPath> {
    let mut out: Vec<HeadingPath> = Vec::new();
    walk(roots, &mut Vec::new(), &mut out);
    out
}

fn walk(nodes: &[HeadingNode], prefix: &mut HeadingPath, out: &mut Vec<HeadingPath>) {
    // Per-parent, per-title occurrence counter — reset for each sibling list, which is what
    // keeps `nth` local to the duplicates instead of counting position among all siblings.
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for node in nodes {
        let nth = seen.entry(node.text.as_str()).or_insert(0);
        prefix.push(PathStep {
            title: node.text.clone(),
            nth: *nth,
        });
        *nth += 1;
        // `doc_index` is assigned in document order, so writing at that index fills the
        // vector densely; the resize keeps it total even if a future tree ever skipped one.
        if out.len() <= node.doc_index {
            out.resize(node.doc_index + 1, HeadingPath::new());
        }
        out[node.doc_index] = prefix.clone();
        walk(&node.children, prefix, out);
        prefix.pop();
    }
}

/// The set of outline nodes the reader has collapsed, named by title path.
///
/// Per-document state, held on `TabState` beside `outline_selected` for the same reason that
/// field exists: the widget is rebuilt from scratch and would otherwise drop it. Deliberately
/// **not** round-tripped through `session/`, matching `TabState::folds` — a document may
/// have changed entirely between runs, and a key that means nothing then should not be
/// resurrected to collapse an unrelated section.
#[derive(Clone, Debug, Default)]
pub(crate) struct OutlineExpansion {
    collapsed: BTreeSet<HeadingPath>,
}

impl OutlineExpansion {
    /// Should the node at `path` be built collapsed?
    pub(crate) fn is_collapsed(&self, path: &HeadingPath) -> bool {
        self.collapsed.contains(path)
    }

    /// Record one node's state, as observed on a live row.
    ///
    /// Takes `expanded` rather than `collapsed` because that is the sense the GTK property
    /// carries; inverting at the boundary keeps the caller from having to remember to.
    pub(crate) fn note(&mut self, path: &HeadingPath, expanded: bool) {
        if expanded {
            self.collapsed.remove(path);
        } else {
            self.collapsed.insert(path.clone());
        }
    }

    /// The `doc_index`es a fresh build should leave collapsed, given that build's paths.
    ///
    /// The translation from durable names back to this build's transient indexes. Nodes with
    /// no entry are absent from the result and so are expanded by the caller's default.
    pub(crate) fn collapsed_indexes(&self, paths: &[HeadingPath]) -> BTreeSet<usize> {
        paths
            .iter()
            .enumerate()
            .filter(|(_, path)| self.is_collapsed(path))
            .map(|(doc_index, _)| doc_index)
            .collect()
    }
}

/// TDD 12.25.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::{build_tree, Heading};
    use crate::span::OriginalByteOffset;

    /// Build a tree from `(level, title)` pairs, as `extract_headings` would produce it.
    fn tree(rows: &[(u8, &str)]) -> Vec<HeadingNode> {
        let headings: Vec<Heading> = rows
            .iter()
            .enumerate()
            .map(|(i, (level, text))| Heading {
                level: *level,
                text: (*text).to_string(),
                src_offset: OriginalByteOffset::new(i),
            })
            .collect();
        build_tree(&headings)
    }

    fn path(steps: &[(&str, usize)]) -> HeadingPath {
        steps
            .iter()
            .map(|(title, nth)| PathStep {
                title: (*title).to_string(),
                nth: *nth,
            })
            .collect()
    }

    #[test]
    fn a_path_names_a_heading_by_its_ancestry() {
        let roots = tree(&[(1, "Intro"), (2, "Details"), (3, "Deep"), (1, "Outro")]);
        let paths = paths_in_document_order(&roots);
        assert_eq!(paths[0], path(&[("Intro", 0)]));
        assert_eq!(paths[1], path(&[("Intro", 0), ("Details", 0)]));
        assert_eq!(paths[2], path(&[("Intro", 0), ("Details", 0), ("Deep", 0)]));
        assert_eq!(paths[3], path(&[("Outro", 0)]));
    }

    /// Siblings sharing a title are distinct keys — otherwise collapsing one would collapse
    /// the other, which reads as the outline moving on its own.
    ///
    /// Mutation check: dropping `nth` from `PathStep` makes both paths equal and fails this.
    #[test]
    fn identical_sibling_titles_are_separated_by_occurrence() {
        let roots = tree(&[(1, "Notes"), (1, "Notes")]);
        let paths = paths_in_document_order(&roots);
        assert_eq!(paths[0], path(&[("Notes", 0)]));
        assert_eq!(paths[1], path(&[("Notes", 1)]));
        assert_ne!(paths[0], paths[1]);
    }

    /// The occurrence counter is per parent, so a repeated title under a *different* parent
    /// starts again at 0 rather than continuing a document-wide tally.
    #[test]
    fn the_occurrence_counter_is_local_to_its_parent() {
        let roots = tree(&[(1, "A"), (2, "Notes"), (1, "B"), (2, "Notes")]);
        let paths = paths_in_document_order(&roots);
        assert_eq!(paths[1], path(&[("A", 0), ("Notes", 0)]));
        assert_eq!(paths[3], path(&[("B", 0), ("Notes", 0)]));
    }

    /// **The whole point of a title path.** Inserting a heading above a collapsed one must
    /// leave it collapsed — this is the case a `doc_index` or `src_offset` key gets wrong,
    /// silently collapsing whichever section inherited the old index.
    #[test]
    fn inserting_a_heading_elsewhere_leaves_a_collapsed_section_collapsed() {
        let before = paths_in_document_order(&tree(&[(1, "Alpha"), (1, "Beta"), (2, "Leaf")]));
        let mut state = OutlineExpansion::default();
        state.note(&before[1], false); // reader collapses "Beta"

        let after =
            paths_in_document_order(&tree(&[(1, "New"), (1, "Alpha"), (1, "Beta"), (2, "Leaf")]));
        assert_eq!(state.collapsed_indexes(&after), BTreeSet::from([2]));
        assert!(!state.is_collapsed(&after[0]), "the new heading is open");
    }

    /// The documented cost: renaming loses the entry and the node re-expands. Asserted so the
    /// trade-off is pinned rather than merely described — if a future key survives renames,
    /// this test is the thing that should be made to fail deliberately.
    #[test]
    fn renaming_a_heading_re_expands_it() {
        let before = paths_in_document_order(&tree(&[(1, "Draft")]));
        let mut state = OutlineExpansion::default();
        state.note(&before[0], false);

        let after = paths_in_document_order(&tree(&[(1, "Final")]));
        assert!(state.collapsed_indexes(&after).is_empty());
    }

    /// Renaming an ancestor re-expands its descendants too, because their paths run through
    /// it. Same trade, one level down, and worth pinning separately: it is the case where a
    /// reader sees several sections open at once.
    #[test]
    fn renaming_an_ancestor_re_expands_its_descendants() {
        let before = paths_in_document_order(&tree(&[(1, "Old"), (2, "Kept")]));
        let mut state = OutlineExpansion::default();
        state.note(&before[1], false);

        let after = paths_in_document_order(&tree(&[(1, "New"), (2, "Kept")]));
        assert!(state.collapsed_indexes(&after).is_empty());
    }

    /// A node nobody has touched is expanded — the default the outline is built with.
    ///
    /// Mutation check: storing the EXPANDED set instead inverts this, and every untouched
    /// heading would arrive collapsed.
    #[test]
    fn an_unknown_node_is_not_collapsed() {
        let paths = paths_in_document_order(&tree(&[(1, "A"), (2, "B")]));
        let state = OutlineExpansion::default();
        assert!(!state.is_collapsed(&paths[0]));
        assert!(state.collapsed_indexes(&paths).is_empty());
    }

    #[test]
    fn expanding_again_forgets_the_node() {
        let paths = paths_in_document_order(&tree(&[(1, "A")]));
        let mut state = OutlineExpansion::default();
        state.note(&paths[0], false);
        assert!(state.is_collapsed(&paths[0]));
        state.note(&paths[0], true);
        assert!(!state.is_collapsed(&paths[0]));
    }

    #[test]
    fn an_empty_outline_has_no_paths() {
        assert!(paths_in_document_order(&[]).is_empty());
    }
}
