//! Display-free: measured fragments + page metrics → page boundaries.
//!
//! # Why the decision is here and the measurement is not
//!
//! Measuring how tall a line of text is needs Pango, and Pango needs the toolkit.
//! *Deciding which page that line lands on* needs neither — it needs a height, a page,
//! and a rule. Splitting them on that seam is what puts the rule inside the coverage
//! gate: `pdf.rs` measures and draws, this file decides, and the decision is settled by
//! unit test rather than by a human counting pages in a viewer.
//!
//! # The one rule
//!
//! **A fragment is indivisible.** A fragment is one laid-out line, so "a fragment never
//! straddles a page boundary" and "a page break never splits a line of text"
//! (TDD 25.16) are the same statement. A fragment taller than a whole page is placed
//! alone on its own page and allowed to overflow, because the alternative — dropping it
//! — loses content silently, and the alternative to *that* is a line-splitting engine,
//! which is the scope creep the plan bounds.

use std::ops::Range;

/// One indivisible laid-out unit: a line of text, a rule, a table row, an image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fragment {
    /// Height in points, as measured.
    pub(crate) height: f64,
    /// Space to leave above this fragment when it is **not** the first on its page.
    /// Dropped at a page top, so a block's inter-paragraph gap never appears as a
    /// blank strip at the head of a page.
    pub(crate) space_before: f64,
    /// Keep this fragment on the same page as the one after it where possible — a
    /// heading with its first line of body, or a table's header row with its first
    /// body row. Advisory: honoured only when the pair actually fits.
    pub(crate) keep_with_next: bool,
}

impl Fragment {
    /// A plain fragment of `height` points with no spacing and no keep rule.
    #[cfg(test)]
    pub(crate) fn plain(height: f64) -> Self {
        Self {
            height,
            space_before: 0.0,
            keep_with_next: false,
        }
    }
}

/// The printable area a page offers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PageMetrics {
    /// Height in points available for content, margins already subtracted.
    pub(crate) content_height: f64,
}

/// Assign `fragments` to pages, returning one half-open index range per page.
///
/// Total by construction: every fragment lands on exactly one page, in order, and no
/// page is empty unless there were no fragments at all. That totality is the property
/// that makes "nothing is lost in pagination" checkable rather than hoped for.
pub(crate) fn paginate(fragments: &[Fragment], metrics: &PageMetrics) -> Vec<Range<usize>> {
    if fragments.is_empty() {
        return Vec::new();
    }
    // A degenerate page (zero or negative content height) would otherwise loop or
    // divide by nothing; one fragment per page is the only sane answer.
    if metrics.content_height <= 0.0 {
        return (0..fragments.len()).map(|i| i..i + 1).collect();
    }

    let mut pages = Vec::new();
    let mut start = 0usize;
    let mut used = 0.0f64;
    for (i, frag) in fragments.iter().enumerate() {
        let first_on_page = i == start;
        let needed = frag.height
            + if first_on_page {
                0.0
            } else {
                frag.space_before
            };
        // A fragment that does not fit starts a new page — unless it is already the
        // only thing on this one, in which case it is taller than any page and moving
        // it would loop forever.
        if !first_on_page && used + needed > metrics.content_height {
            pages.push(start..i);
            start = i;
            used = frag.height;
            continue;
        }
        used += needed;
    }
    pages.push(start..fragments.len());
    apply_keep_with_next(fragments, metrics, pages)
}

/// Move a widowed `keep_with_next` fragment onto the following page.
///
/// Only where it actually helps: the fragment must not be alone on its page (moving it
/// would leave an empty one), and the page it moves to must have room, or the move
/// trades one bad break for a worse one.
fn apply_keep_with_next(
    fragments: &[Fragment],
    metrics: &PageMetrics,
    mut pages: Vec<Range<usize>>,
) -> Vec<Range<usize>> {
    for p in 0..pages.len().saturating_sub(1) {
        let (this, next) = (pages[p].clone(), pages[p + 1].clone());
        let Some(last) = this.end.checked_sub(1) else {
            continue;
        };
        if !fragments[last].keep_with_next || this.len() < 2 {
            continue;
        }
        let moved = fragments[last].height + fragments[last].space_before;
        let next_used: f64 = height_of(fragments, &next);
        if next_used + moved > metrics.content_height {
            continue;
        }
        pages[p] = this.start..last;
        pages[p + 1] = last..next.end;
    }
    pages
}

/// The height a run of fragments occupies when laid out from a page top.
fn height_of(fragments: &[Fragment], range: &Range<usize>) -> f64 {
    fragments[range.clone()]
        .iter()
        .enumerate()
        .map(|(i, f)| f.height + if i == 0 { 0.0 } else { f.space_before })
        .sum()
}

#[cfg(test)]
mod paginate_tests {
    use super::*;

    fn metrics(h: f64) -> PageMetrics {
        PageMetrics { content_height: h }
    }

    fn plain(heights: &[f64]) -> Vec<Fragment> {
        heights.iter().copied().map(Fragment::plain).collect()
    }

    /// Every fragment lands on exactly one page, in order. Asserted from the RESULT
    /// rather than from the algorithm, so it survives a change to how pages are chosen.
    fn assert_total(fragments: &[Fragment], pages: &[Range<usize>]) {
        let covered: Vec<usize> = pages.iter().flat_map(|p| p.clone()).collect();
        assert_eq!(
            covered,
            (0..fragments.len()).collect::<Vec<_>>(),
            "pagination lost, duplicated or reordered a fragment: {pages:?}"
        );
        assert!(
            pages.iter().all(|p| !p.is_empty()),
            "an empty page: {pages:?}"
        );
    }

    #[test]
    fn an_empty_document_paginates_to_no_pages() {
        assert!(paginate(&[], &metrics(100.0)).is_empty());
    }

    #[test]
    fn fragments_fill_a_page_before_starting_the_next() {
        let f = plain(&[10.0; 10]);
        let pages = paginate(&f, &metrics(30.0));
        assert_eq!(pages, vec![0..3, 3..6, 6..9, 9..10]);
        assert_total(&f, &pages);
    }

    #[test]
    fn a_fragment_never_straddles_a_page_boundary() {
        // TDD 25.16, stated as the property rather than as a page count: no page's
        // content exceeds the page, so nothing was cut in half to make it fit.
        let f = plain(&[7.0, 7.0, 7.0, 7.0, 7.0]);
        let pages = paginate(&f, &metrics(20.0));
        assert_total(&f, &pages);
        for page in &pages {
            let used = height_of(&f, page);
            assert!(
                used <= 20.0 || page.len() == 1,
                "page {page:?} used {used} of 20"
            );
        }
    }

    #[test]
    fn a_fragment_taller_than_a_page_gets_its_own_page_rather_than_being_dropped() {
        // Overflowing is a visible defect; dropping is a silent one. Prefer visible.
        let f = plain(&[10.0, 500.0, 10.0]);
        let pages = paginate(&f, &metrics(100.0));
        assert_total(&f, &pages);
        assert!(
            pages.contains(&(1..2)),
            "the oversized fragment should stand alone: {pages:?}"
        );
    }

    #[test]
    fn leading_space_is_dropped_at_a_page_top() {
        // A block's inter-paragraph gap must not appear as a blank strip at the head
        // of a page. Two fragments of 10 with 50 of space_before fit a 20pt page only
        // because the second page's leading space is dropped.
        let f = vec![
            Fragment {
                height: 10.0,
                space_before: 0.0,
                keep_with_next: false,
            },
            Fragment {
                height: 10.0,
                space_before: 50.0,
                keep_with_next: false,
            },
        ];
        let pages = paginate(&f, &metrics(20.0));
        assert_eq!(pages, vec![0..1, 1..2]);
        assert_eq!(height_of(&f, &pages[1]), 10.0, "the gap was dropped");
    }

    #[test]
    fn a_widowed_heading_moves_to_the_page_its_body_starts() {
        // The heading is the last thing that fits on page 1; it belongs with the body
        // that follows it.
        let f = vec![
            Fragment::plain(10.0),
            Fragment::plain(10.0),
            Fragment {
                height: 10.0,
                space_before: 0.0,
                keep_with_next: true,
            },
            Fragment::plain(10.0),
        ];
        let pages = paginate(&f, &metrics(30.0));
        assert_total(&f, &pages);
        assert_eq!(
            pages,
            vec![0..2, 2..4],
            "the heading travelled with its body"
        );
    }

    #[test]
    fn a_keep_with_next_alone_on_its_page_stays_put() {
        // Moving it would leave an empty page, which is worse than the widow.
        let f = vec![
            Fragment::plain(30.0),
            Fragment {
                height: 10.0,
                space_before: 0.0,
                keep_with_next: true,
            },
            Fragment::plain(30.0),
        ];
        let pages = paginate(&f, &metrics(30.0));
        assert_total(&f, &pages);
        assert!(
            pages.iter().all(|p| !p.is_empty()),
            "no page was emptied: {pages:?}"
        );
    }

    #[test]
    fn a_degenerate_page_height_still_places_every_fragment() {
        // Zero content height must not loop or lose anything.
        let f = plain(&[10.0, 10.0, 10.0]);
        let pages = paginate(&f, &metrics(0.0));
        assert_total(&f, &pages);
        assert_eq!(pages.len(), 3);
    }

    #[test]
    fn per_page_cost_does_not_grow_with_document_length() {
        // TDD 25.22's shape assertion at the paginator: doubling the document doubles
        // the pages and nothing more. A SHAPE, never a wall-clock number a slower
        // machine would fail.
        let short = plain(&[10.0; 100]);
        let long = plain(&[10.0; 200]);
        let m = metrics(100.0);
        let (a, b) = (paginate(&short, &m).len(), paginate(&long, &m).len());
        assert_eq!(b, a * 2, "pages: {a} for 100 fragments, {b} for 200");
    }
}
