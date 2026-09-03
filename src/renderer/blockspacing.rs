//! What separator a block needs before its own content — decided without a buffer.
//!
//! # Why this is a module and not three `if` chains in a dispatcher
//!
//! `Renderer::start_tag` is a ~287-line dispatcher over a live `GtkTextBuffer`, and the
//! spacing rules for paragraphs, lists and list items were three interleaved condition
//! chains inside it. They are the most-exercised decisions in the renderer — every
//! document hits them, on every render — and they were reachable only by rendering a
//! document and reading the text back, which exercises them through everything else at
//! once and cannot address their edges directly.
//!
//! They are also the classic silent render defect: a wrong answer here is a missing or
//! doubled blank line, which looks like a styling quibble and is actually the renderer
//! and the copy map disagreeing about how many characters a block occupies.
//!
//! POLICY's coverage scope rule names this extraction as the mechanism by which the
//! floor rises: the GTK half stays in `start.rs` and applies the answer, the decision
//! lives here where a unit test can reach every combination.

/// The separator a block needs before its own content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeadIn {
    /// Nothing: the previous write already left the buffer where this block starts.
    Nothing,
    /// One newline — this block starts on its own line but shares its parent's block.
    Newline,
    /// A full block gap. Idempotent at the call site via `trailing_newlines`, so
    /// requesting one after another block already left one is a no-op.
    BlockGap,
}

/// The renderer state the answer depends on, named rather than passed as four bare
/// `bool`s — every one of them is a "is the writer currently inside X" question and at a
/// call site they are indistinguishable from one another.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BlockContext {
    /// A list item's marker has just been written and nothing follows it yet, so the
    /// item's first paragraph must not be separated from its own marker.
    pub(crate) list_item_open: bool,
    /// The writer is inside at least one list.
    pub(crate) inside_list: bool,
    /// The next item is the first of its list, which the list's own lead-in already
    /// separated.
    pub(crate) list_first_item: bool,
    /// Nothing has been written yet, so there is nothing to separate from.
    pub(crate) at_start: bool,
}

/// The block about to be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockKind {
    Paragraph,
    List,
    Item,
}

/// What separator `kind` needs, given `cx`.
///
/// One function over three kinds rather than three functions, because the kinds share
/// one context and the interesting property is how their answers DIFFER for the same
/// state — which is only visible when they are read together.
pub(crate) fn lead_in(kind: BlockKind, cx: BlockContext) -> LeadIn {
    match kind {
        // A list item's first paragraph follows the marker directly. Later paragraphs in
        // the same item (a loose list) take one newline; a full gap would double-space
        // them. Outside a list — including a blockquote's paragraphs — a full gap, which
        // is idempotent, so the first paragraph after a blockquote's own gap is free.
        BlockKind::Paragraph => {
            if cx.list_item_open || cx.at_start {
                // `at_start` is stated HERE, not left to the applier. `block_sep`
                // no-ops at the start of a document, so the two agree in the running
                // renderer either way — but this function is the thing a reader (and a
                // unit test) asks what the separator IS, and answering `BlockGap` for a
                // document's first paragraph described a gap nothing emits (F-AP2-010).
                // A decision core that is only correct once its applier has corrected
                // it is not a decision core.
                LeadIn::Nothing
            } else if cx.inside_list {
                LeadIn::Newline
            } else {
                LeadIn::BlockGap
            }
        }
        // A top-level list takes a full gap from whatever precedes it. A NESTED list is
        // inside its parent item, so one newline — a gap would put an empty line between
        // the item's text and its sub-list.
        BlockKind::List => {
            if !cx.inside_list {
                LeadIn::BlockGap
            } else if cx.at_start {
                LeadIn::Nothing
            } else {
                LeadIn::Newline
            }
        }
        // The list's own lead-in already separated the first item; every later one takes
        // exactly one newline.
        BlockKind::Item => {
            if cx.list_first_item || cx.at_start {
                LeadIn::Nothing
            } else {
                LeadIn::Newline
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default: nothing open, nothing written.
    fn fresh() -> BlockContext {
        BlockContext {
            list_item_open: false,
            inside_list: false,
            list_first_item: false,
            at_start: false,
        }
    }

    #[test]
    fn a_top_level_paragraph_takes_a_full_block_gap() {
        assert_eq!(lead_in(BlockKind::Paragraph, fresh()), LeadIn::BlockGap);
    }

    /// F-AP2-010: the first paragraph of a document has nothing to separate from, and
    /// this function must say so itself. It answered `BlockGap` and was correct only
    /// because `emit::block_sep` returns early at the start — a rule stated in the
    /// applier and contradicted in the decider.
    #[test]
    fn the_first_paragraph_of_a_document_takes_nothing() {
        let cx = BlockContext {
            at_start: true,
            ..fresh()
        };
        assert_eq!(lead_in(BlockKind::Paragraph, cx), LeadIn::Nothing);
    }

    #[test]
    fn an_items_first_paragraph_follows_its_marker_directly() {
        // The one that is visible immediately if it breaks: the text of every list item
        // would start on the line below its own bullet.
        let cx = BlockContext {
            list_item_open: true,
            inside_list: true,
            ..fresh()
        };
        assert_eq!(lead_in(BlockKind::Paragraph, cx), LeadIn::Nothing);
    }

    #[test]
    fn a_later_paragraph_in_a_loose_item_takes_one_newline_not_a_gap() {
        // A full gap here double-spaces every loose list.
        let cx = BlockContext {
            list_item_open: false,
            inside_list: true,
            ..fresh()
        };
        assert_eq!(lead_in(BlockKind::Paragraph, cx), LeadIn::Newline);
    }

    #[test]
    fn a_top_level_list_takes_a_full_block_gap() {
        assert_eq!(lead_in(BlockKind::List, fresh()), LeadIn::BlockGap);
    }

    #[test]
    fn a_nested_list_takes_one_newline_not_a_gap() {
        // A gap would put an empty line between an item's own text and its sub-list.
        let cx = BlockContext {
            inside_list: true,
            ..fresh()
        };
        assert_eq!(lead_in(BlockKind::List, cx), LeadIn::Newline);
    }

    #[test]
    fn a_nested_list_at_the_very_start_separates_from_nothing() {
        let cx = BlockContext {
            inside_list: true,
            at_start: true,
            ..fresh()
        };
        assert_eq!(lead_in(BlockKind::List, cx), LeadIn::Nothing);
    }

    #[test]
    fn the_first_item_is_already_separated_by_its_lists_own_lead_in() {
        let cx = BlockContext {
            inside_list: true,
            list_first_item: true,
            ..fresh()
        };
        assert_eq!(lead_in(BlockKind::Item, cx), LeadIn::Nothing);
    }

    #[test]
    fn every_later_item_takes_exactly_one_newline() {
        let cx = BlockContext {
            inside_list: true,
            list_first_item: false,
            ..fresh()
        };
        assert_eq!(lead_in(BlockKind::Item, cx), LeadIn::Newline);
    }

    #[test]
    fn an_item_at_the_very_start_separates_from_nothing() {
        let cx = BlockContext {
            inside_list: true,
            at_start: true,
            ..fresh()
        };
        assert_eq!(lead_in(BlockKind::Item, cx), LeadIn::Nothing);
    }

    /// The kinds disagree for the SAME state, which is the whole reason they are read
    /// together — a paragraph inside a list takes a newline where a list takes one too
    /// but an item does not.
    #[test]
    fn the_three_kinds_answer_the_same_state_differently() {
        let cx = BlockContext {
            inside_list: true,
            list_first_item: true,
            ..fresh()
        };
        assert_eq!(lead_in(BlockKind::Paragraph, cx), LeadIn::Newline);
        assert_eq!(lead_in(BlockKind::List, cx), LeadIn::Newline);
        assert_eq!(lead_in(BlockKind::Item, cx), LeadIn::Nothing);
    }
}
