//! The splice's TAG equality with a full render — the half [`super`]'s text oracle
//! cannot see.
//!
//! `assert_splice_matches_full_render` compares buffer TEXT, and text is most of the
//! correctness claim. It is not all of it: a `GtkTextTag` carries the decoration, and a
//! spliced region whose text is byte-identical can still be styled differently, with no
//! assertion anywhere in the tree able to tell. That gap was not hypothetical —
//! `Renderer::write_seeded_summary_tail` re-wrote the collapsed preview fragment and
//! did not re-ink it, so a collapse left the preview un-inked on the drawn summary band
//! (TDD 18.49), silently, on exactly the themes that state `disclosure_fg`.
//!
//! Its own file rather than more of `tests.rs`, which is at the 500-line soft limit,
//! and because the shape is different: a tag-range comparison rather than a text one.

use crate::fold::FoldState;
use gtk::prelude::*;

/// Every contiguous range carrying the tag `name`, as `(start, end)` char offsets.
///
/// Walked character by character rather than through `forward_to_tag_toggle`: the
/// fixtures here are a few hundred characters, and a toggle walk has to get its own
/// start/end pairing right, which is a second thing to be wrong about in a test whose
/// whole job is to be more trustworthy than the code it checks.
fn tag_ranges(buf: &gtk::TextBuffer, name: &str) -> Vec<(i32, i32)> {
    let Some(tag) = buf.tag_table().lookup(name) else {
        return Vec::new();
    };
    let count = buf.char_count();
    let mut out: Vec<(i32, i32)> = Vec::new();
    let mut run: Option<i32> = None;
    for offset in 0..count {
        match (buf.iter_at_offset(offset).has_tag(&tag), run) {
            (true, None) => run = Some(offset),
            (false, Some(start)) => {
                out.push((start, offset));
                run = None;
            }
            _ => {}
        }
    }
    if let Some(start) = run {
        out.push((start, count));
    }
    out
}

/// One tag's ranges, as the SPLICE produced them and as a fresh full render of the
/// same fold state produces them. Named rather than a bare tuple so the two are never
/// read in the wrong order — the spliced side is the subject and the fresh side is the
/// oracle, and a transposed comparison would still be an equality that passes.
struct BothWays {
    spliced: Vec<(i32, i32)>,
    fresh: Vec<(i32, i32)>,
}

/// Splice `md` from `before` to `after` and report `name`'s ranges either way.
fn ranges_both_ways(md: &str, before: &FoldState, after: &FoldState, name: &str) -> BothWays {
    let starting = crate::preview::build::build_render_products_with_theme(
        md,
        None,
        1.0,
        false,
        crate::theme::active(),
        before,
    );
    let key = crate::renderer::disclosure::scan_document(md)[0].fold_key();
    crate::preview::splice::splice(
        &starting.buf,
        None,
        &starting.anchored,
        &starting.maps.disclosure_extents,
        &crate::preview::build::Prepared::new(md, None, 1.0, false, crate::theme::active(), after),
        key,
    )
    .expect("the toggled block was drawn in the starting render");

    let full = crate::preview::build::build_render_products_with_theme(
        md,
        None,
        1.0,
        false,
        crate::theme::active(),
        after,
    );
    BothWays {
        spliced: tag_ranges(&starting.buf, name),
        fresh: tag_ranges(&full.buf, name),
    }
}

const INK: &str = "disclosure-ink";
const PREVIEW: &str = "disclosure-preview";

/// A body long enough that collapsing it writes a real preview fragment — a body
/// inside the preview limit renders to the same length either way and would leave the
/// re-inked range indistinguishable from the label's own.
const MD: &str = concat!(
    "# Title\n\nintro paragraph\n\n",
    "<details open>\n<summary>A summary label</summary>\n\n",
    "hidden body that runs on well past the preview limit so that collapsing the ",
    "block genuinely writes a preview fragment onto the summary line",
    "\n\n</details>\n\n",
    "## After\n\ntail paragraph\n"
);

/// **Collapsing through the splice inks the summary line exactly as a full render
/// does.**
///
/// This is the direction that writes a preview fragment, so it is the one the gap was
/// in. The full render's ranges are the oracle rather than a literal: the extent
/// depends on the label and the preview's character limit, and pinning either here
/// would make this a test of the fixture.
#[gtktest::test]
fn a_spliced_collapse_inks_the_summary_line_the_way_a_full_render_does() {
    let key = crate::renderer::disclosure::scan_document(MD)[0].fold_key();
    // The source says `open`; toggling the reader's state closes it.
    let mut closed = FoldState::default();
    closed.toggle(key);

    let BothWays { spliced, fresh } = ranges_both_ways(MD, &FoldState::default(), &closed, INK);
    assert!(
        !fresh.is_empty(),
        "the oracle is dead: a full collapsed render inked nothing at all, so the \
         comparison below would be satisfied by a splice that inked nothing either"
    );
    assert_eq!(
        spliced, fresh,
        "the spliced summary line must carry `disclosure-ink` over the same extent a \
         full render gives it — the label AND the collapsed preview, because both sit \
         on the drawn summary band and both have to stay legible on it (TDD 18.49)"
    );

    // The preview's own dimming, over the same fragment. Distinct from the ink and
    // registered later so a theme stating both still dims it, so they are asserted
    // separately rather than assumed to travel together.
    let BothWays { spliced, fresh } = ranges_both_ways(MD, &FoldState::default(), &closed, PREVIEW);
    assert!(
        !fresh.is_empty(),
        "the oracle is dead: a full collapsed render marked no preview fragment"
    );
    assert_eq!(
        spliced, fresh,
        "the spliced collapsed preview must carry `disclosure-preview` over the same \
         fragment a full render gives it"
    );
}

/// **And expanding leaves the ink over the label alone**, rather than over a preview
/// that is no longer there.
///
/// The opposite direction, asserted separately: an apply that works while its undo does
/// not still passes a one-way test, and here the two are genuinely different code paths
/// — one writes a fragment and re-inks over it, the other writes none and must not
/// widen the range it inherited.
#[gtktest::test]
fn a_spliced_expand_leaves_the_ink_over_the_label_alone() {
    let md = MD.replace("<details open>", "<details>");
    let key = crate::renderer::disclosure::scan_document(&md)[0].fold_key();
    let mut opened = FoldState::default();
    opened.toggle(key);

    let BothWays { spliced, fresh } = ranges_both_ways(&md, &FoldState::default(), &opened, INK);
    assert!(!fresh.is_empty(), "the oracle is dead");
    assert_eq!(
        spliced, fresh,
        "an expanded summary line's ink covers its label and nothing more"
    );

    assert_eq!(
        tag_ranges(
            &crate::preview::build::build_render_products_with_theme(
                &md,
                None,
                1.0,
                false,
                crate::theme::active(),
                &opened,
            )
            .buf,
            PREVIEW,
        ),
        Vec::new(),
        "sanity: an expanded block has no preview fragment at all, which is what makes \
         the ink extent above the label's own"
    );
}

/// **The CONTAINER tags, which the two assertions above do not reach at all.**
///
/// `disclosure-ink` and `disclosure-preview` are written by the summary line itself,
/// inside the region. Every other tag a spliced region inherits — a blockquote's
/// margin, a list item's hanging indent, a code block's inset, a heading's — is applied
/// at a `TagEnd` the region walk may never reach, because the walk stops the moment
/// `key`'s own frame pops. That is a different failure mode from an ink extent and it
/// had no assertion of any kind (F-AP2-006).
///
/// Driven over both container fixtures, and over every tag family the region can carry.
/// A family the fixture does not produce is skipped LOUDLY rather than silently
/// compared as two empty vectors: the whole hazard here is a check that passes because
/// it found nothing.
#[gtktest::test]
fn a_spliced_region_carries_the_same_container_tags_a_full_render_does() {
    // `li-1-cont` and `bq-1` are the families a disclosure inside a container actually
    // inherits; `code-block` and `h2` cover the two the region's own body writes.
    const FAMILIES: [&str; 6] = ["bq-1", "li-1", "li-1-cont", "code-block", "h1", "h2"];

    for (name, md) in [
        ("blockquote", super::IN_BLOCKQUOTE),
        ("list item", super::IN_LIST_ITEM),
    ] {
        let key = crate::renderer::disclosure::scan_document(md)[0].fold_key();
        let mut opened = FoldState::default();
        opened.toggle(key);

        let mut seen = 0usize;
        for family in FAMILIES {
            let BothWays { spliced, fresh } =
                ranges_both_ways(md, &FoldState::default(), &opened, family);
            if fresh.is_empty() && spliced.is_empty() {
                continue;
            }
            seen += 1;
            assert_eq!(
                spliced, fresh,
                "{name}: `{family}`'s ranges after a splice must be the ones a full \
                 render of the same fold state produces — a container tag applied at a \
                 TagEnd the region walk never reaches is how a spliced pane keeps a \
                 correct buffer and a wrong indent"
            );
        }
        assert!(
            seen >= 2,
            "{name}: only {seen} of the tag families were present in either render, so \
             this sweep is close to vacuous — the fixture no longer contains the \
             containers it is named for"
        );
    }
}
