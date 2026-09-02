//! Tests for [`super`] — split out of `splice.rs` past the 500-line soft limit
//! (POLICY § Code style), same shape as `copymap/tests.rs`. The whole file is
//! `#[cfg(all(test, feature = "gtk-integration-tests"))]`, applied once at the
//! `mod tests;` declaration in `splice.rs` rather than repeated here.

/// The TAG half of the same claim — a spliced region whose TEXT matches a full render
/// can still be styled differently, and nothing in this file can see that.
mod tags;

use crate::fold::{FoldKey, FoldState};
use crate::span::BufferSpan;

/// One document rendered under both fold states: each render's buffer text and the
/// volatile region THAT render recorded for `key`.
struct BothWays {
    closed_text: Vec<char>,
    opened_text: Vec<char>,
    closed_region: BufferSpan,
    opened_region: BufferSpan,
}

fn both_ways(md: &str, key: FoldKey) -> BothWays {
    use gtk::prelude::TextBufferExt;
    let render = |folds: &FoldState| {
        let products = super::super::build::build_render_products_with_theme(
            md,
            None,
            1.0,
            false,
            crate::theme::active(),
            folds,
        );
        let buf = &products.buf;
        let text: Vec<char> = buf
            .slice(&buf.start_iter(), &buf.end_iter(), true)
            .to_string()
            .chars()
            .collect();
        let region = products
            .disclosure_extents
            .iter()
            .find(|e| e.key == key)
            .expect("this render records the block")
            .volatile;
        (text, region)
    };

    let mut open = FoldState::default();
    open.toggle(key);
    let (closed_text, closed_region) = render(&FoldState::default());
    let (opened_text, opened_region) = render(&open);
    BothWays {
        closed_text,
        opened_text,
        closed_region,
        opened_region,
    }
}

/// **A fold changes exactly ONE contiguous region of the rendered text**, and that
/// region is the one `DisclosureExtent::volatile` names.
///
/// This is the splice's founding claim: if the text outside the region were not
/// identical, the scratch walk's offsets would not be the live buffer's offsets and
/// every map below the splice would be wrong — silently, because each map would
/// still be internally consistent. Asserted as TEXT, because that is what has to
/// match.
///
/// The region begins after the summary LABEL rather than at the body, and that is
/// not a detail: a collapsed block previews its body's opening text ON the summary
/// line, so the divergence starts there. A splice aimed at the body alone would
/// strand a stale preview and put every offset below it out by that fragment's
/// length. This test earned its keep by failing the moment the preview landed.
fn assert_one_region(w: &BothWays) {
    let at = w.closed_region.start as usize;
    assert_eq!(
        w.opened_region.start as usize, at,
        "the region starts at the same place either way — the label above it is \
         authored content and renders identically"
    );
    assert_eq!(
        w.closed_text[..at],
        w.opened_text[..at],
        "everything before the region renders identically under either fold state"
    );
    // **Normalised through the newline run, and that is a constraint on the
    // splice rather than a looseness in the test.** `block_sep` is written lazily
    // by the NEXT block, so the separator following a disclosure is not part of
    // the block's own render and its LENGTH depends on what preceded it: MEASURED,
    // a collapsed block leaves one newline before the next heading where an
    // expanded one leaves two. So the splice must delete through the newline run
    // after the region and let the region render re-establish it — a splice that
    // stopped at the region's end would leave the separator wrong by one character
    // and shift every offset below it.
    let tail = |text: &[char], from: usize| -> Vec<char> {
        text[from..]
            .iter()
            .skip_while(|c| **c == '\n')
            .copied()
            .collect()
    };
    assert_eq!(
        tail(&w.closed_text, w.closed_region.end as usize),
        tail(&w.opened_text, w.opened_region.end as usize),
        "and everything after each render's own region — past the block separator \
         the next block writes — is the same text, so one delete plus one region \
         render is the whole of the difference"
    );
}

#[gtktest::test]
fn a_fold_changes_exactly_one_region_of_the_rendered_text() {
    const MD: &str = concat!(
        "# Title\n\nintro paragraph\n\n",
        "<details>\n<summary>S</summary>\n\n",
        "hidden body that runs on well past the preview limit so that expanding ",
        "the block genuinely adds characters rather than merely rearranging them",
        "\n\n</details>\n\n",
        "## After\n\ntail paragraph\n"
    );
    let spans = crate::renderer::disclosure::scan_document(MD);
    let w = both_ways(MD, FoldKey(spans[0].start));
    assert!(
        w.opened_text.len() > w.closed_text.len(),
        "the fixture's body is long enough that expanding it GROWS the document — \
         a short body previews in full and can render to the same length either way"
    );
    assert_one_region(&w);
}

/// The same claim where it is least obvious: a disclosure inside a blockquote, after
/// a list (rubric 2.26c). That is where the renderer reaches the region with its
/// list, quote and inline-tag state NON-EMPTY, which is what decides whether a
/// region render's seed needs offset translation. It does not, because the text
/// before the region is identical — which is exactly what this asserts.
#[gtktest::test]
fn a_fold_inside_a_container_still_changes_only_its_own_region() {
    const MD: &str = concat!(
        "intro\n\n",
        "- item one\n",
        "- item two\n\n",
        "> quoted lead-in\n>\n",
        "> <details>\n> <summary>S</summary>\n>\n",
        "> quoted hidden body, again long enough to outrun the preview limit so ",
        "the two renders differ in length\n>\n> </details>\n\n",
        "closing paragraph\n"
    );
    let spans = crate::renderer::disclosure::scan_document(MD);
    assert!(!spans.is_empty(), "the nested block is found at all");
    let w = both_ways(MD, FoldKey(spans[0].start));
    assert_one_region(&w);
}

/// The route's whole correctness claim, checked against the LIVE splice itself
/// rather than against two independent full renders (as [`both_ways`]/
/// [`assert_one_region`] do above): build a render under `before`, splice it to
/// `after` via [`super::splice`], and assert its buffer's resulting text is
/// char-identical to a FRESH full render of `after`. Nothing enforces this
/// today except a test that runs the splice itself — every map recorded below
/// it is silently wrong the moment this stops holding, and every field would
/// still look internally well-formed.
fn assert_splice_matches_full_render(md: &str, before: &FoldState, after: &FoldState) {
    use gtk::prelude::TextBufferExt;
    let starting = super::super::build::build_render_products_with_theme(
        md,
        None,
        1.0,
        false,
        crate::theme::active(),
        before,
    );
    let spans = crate::renderer::disclosure::scan_document(md);
    let key = FoldKey(spans[0].start);

    super::splice(
        &starting.buf,
        None,
        &starting.anchored,
        &starting.disclosure_extents,
        md,
        None,
        1.0,
        false,
        crate::theme::active(),
        after,
        key,
    )
    .expect("the toggled block was drawn in the starting render");

    let spliced_text = starting
        .buf
        .slice(&starting.buf.start_iter(), &starting.buf.end_iter(), true)
        .to_string();

    let full = super::super::build::build_render_products_with_theme(
        md,
        None,
        1.0,
        false,
        crate::theme::active(),
        after,
    );
    let full_text = full
        .buf
        .slice(&full.buf.start_iter(), &full.buf.end_iter(), true)
        .to_string();

    assert_eq!(
        spliced_text, full_text,
        "a spliced toggle must produce a buffer byte-identical to a full render of \
         the same fold state — this is the splice's whole correctness claim"
    );
}

/// The founding claim, end to end: splicing CLOSED→OPEN reproduces exactly what
/// a fresh full OPEN render would have written.
#[gtktest::test]
fn splicing_open_matches_a_full_open_render() {
    const MD: &str = concat!(
        "# Title\n\nintro paragraph\n\n",
        "<details>\n<summary>S</summary>\n\n",
        "hidden body that runs on well past the preview limit so that expanding ",
        "the block genuinely adds characters rather than merely rearranging them",
        "\n\n</details>\n\n",
        "## After\n\ntail paragraph\n"
    );
    let spans = crate::renderer::disclosure::scan_document(MD);
    let key = FoldKey(spans[0].start);
    let mut opened = FoldState::default();
    opened.toggle(key);
    assert_splice_matches_full_render(MD, &FoldState::default(), &opened);
}

/// The reverse direction: splicing OPEN→CLOSED reproduces exactly what a fresh
/// full CLOSED render (with its preview text) would have written. Distinct from
/// the open case: this one writes NO body events at all, only the preview text
/// (`write_seeded_collapsed_preview`), which is the branch a body-only test
/// would never exercise.
#[gtktest::test]
fn splicing_closed_matches_a_full_closed_render() {
    const MD: &str = concat!(
        "# Title\n\nintro paragraph\n\n",
        "<details open>\n<summary>S</summary>\n\n",
        "hidden body that runs on well past the preview limit so that collapsing ",
        "the block genuinely removes characters rather than merely rearranging them",
        "\n\n</details>\n\n",
        "## After\n\ntail paragraph\n"
    );
    let spans = crate::renderer::disclosure::scan_document(MD);
    let key = FoldKey(spans[0].start);
    // The source says `open`; toggling the reader's state closes it.
    let mut closed = FoldState::default();
    closed.toggle(key);
    assert_splice_matches_full_render(MD, &FoldState::default(), &closed);
}

/// Rubric 2.26c's exact case, driven through the live splice: a disclosure
/// nested inside a list item inside a blockquote, where the region render's
/// seed must carry non-empty list/quote/inline-tag state for the spliced text
/// to come out right.
#[gtktest::test]
fn splicing_inside_a_container_matches_a_full_render() {
    const MD: &str = concat!(
        "intro\n\n",
        "- item one\n",
        "- item two\n\n",
        "> quoted lead-in\n>\n",
        "> <details>\n> <summary>S</summary>\n>\n",
        "> quoted hidden body, again long enough to outrun the preview limit so ",
        "the two renders differ in length\n>\n> </details>\n\n",
        "closing paragraph\n"
    );
    let spans = crate::renderer::disclosure::scan_document(MD);
    assert!(!spans.is_empty(), "the nested block is found at all");
    let key = FoldKey(spans[0].start);
    let mut opened = FoldState::default();
    opened.toggle(key);
    assert_splice_matches_full_render(MD, &FoldState::default(), &opened);
}

/// The merge half of the splice — widgets, not text. A table BEFORE the
/// toggled block and a table AFTER it must both survive the splice as the
/// SAME widget objects (never rebuilt: that is the whole point of splicing
/// instead of re-rendering), while the toggled block's own table (inside its
/// body) must be freshly created by the region render.
#[gtktest::test]
fn tables_outside_the_region_survive_the_splice_as_the_same_widgets() {
    use crate::widgets::table::ScribTableWidget;
    use gtk::prelude::Cast;
    const MD: &str = concat!(
        "| before | col |\n|---|---|\n| a | b |\n\n",
        "<details>\n<summary>S</summary>\n\n",
        "| inside | col |\n|---|---|\n| c | d |\n\n",
        "</details>\n\n",
        "| after | col |\n|---|---|\n| e | f |\n"
    );
    let spans = crate::renderer::disclosure::scan_document(MD);
    let key = FoldKey(spans[0].start);

    let starting = super::super::build::build_render_products_with_theme(
        MD,
        None,
        1.0,
        false,
        crate::theme::active(),
        &FoldState::default(),
    );
    let before_table = starting
        .anchored
        .iter()
        .find_map(|(_, w)| w.clone().downcast::<ScribTableWidget>().ok())
        .expect("the render drew the table before the disclosure");
    let after_table = starting
        .anchored
        .iter()
        .rev()
        .find_map(|(_, w)| w.clone().downcast::<ScribTableWidget>().ok())
        .expect("the render drew the table after the disclosure");
    assert_ne!(
        before_table, after_table,
        "sanity: the fixture drew two distinct tables outside the disclosure"
    );

    let mut opened = FoldState::default();
    opened.toggle(key);
    let outcome = super::splice(
        &starting.buf,
        None,
        &starting.anchored,
        &starting.disclosure_extents,
        MD,
        None,
        1.0,
        false,
        crate::theme::active(),
        &opened,
        key,
    )
    .expect("the toggled block was drawn");

    let survives = |w: &ScribTableWidget| {
        outcome
            .merged_anchored
            .iter()
            .any(|(_, mw)| mw.upcast_ref::<gtk::Widget>() == w.upcast_ref::<gtk::Widget>())
    };
    assert!(
        survives(&before_table),
        "the table BEFORE the region must be the same live widget after the splice"
    );
    assert!(survives(&after_table), "the table AFTER the region must be the same live widget after the splice — this is what a full re-render would have rebuilt instead");

    // The region itself drew its OWN table — a THIRD widget, not either survivor.
    assert_eq!(
        outcome.region.tables.len(),
        1,
        "the disclosure's own table must be freshly built by the region render"
    );
    assert_ne!(outcome.region.tables[0], before_table);
    assert_ne!(outcome.region.tables[0], after_table);
}

/// This module's own share of `renderer::normalize`'s "every parse site reads
/// one document" contract (`the_set_of_production_parse_sites_is_the_one_this_module_guards`
/// requires a new `Parser::new_ext` call site to prove this, in the same
/// change that adds it to `SANCTIONED`). A GTK-gated test rather than a plain
/// one — unlike that module's own check, a splice needs a live buffer — but
/// the construct is the same one: a hard-TAB-padded GFM table (ScrAP-75) with
/// an emphasis run straddling a cell boundary, this time inside a disclosure
/// body, where a site that skipped the pre-pass would read one paragraph
/// instead of a table and every downstream offset would disagree.
#[gtktest::test]
fn the_region_render_normalises_tabs_the_same_way_every_other_parse_site_does() {
    const MD: &str = concat!(
        "<details>\n<summary>S</summary>\n\n",
        "| Name\t| Value\t|\n|---\t|---\t|\n| **a\t| b** |\n\n",
        "</details>\n\n",
        "tail paragraph\n"
    );
    let spans = crate::renderer::disclosure::scan_document(MD);
    let key = FoldKey(spans[0].start);
    let mut opened = FoldState::default();
    opened.toggle(key);
    // The equality oracle already proves the table parsed identically: if the
    // splice's parse disagreed with a full render's about where the table
    // starts/ends, or read it as a paragraph instead, the buffer texts could
    // not come out byte-identical.
    assert_splice_matches_full_render(MD, &FoldState::default(), &opened);
}

/// **Rubric 2.26j** — everything that points into the document survives a toggle.
///
/// The rubric the whole scratch-re-walk route exists to satisfy. The failure it guards
/// is the one that does not look like a failure: a stale map still resolves, still
/// returns text and still names a position — it simply names the wrong one, so nothing
/// appears broken until a reader reads what they actually copied.
///
/// Copy is asserted directly, being the map with the least forgiving contract
/// (character-precise, over a tree). The heading site and link span are asserted as
/// POSITIONS rather than by driving the UI: they are buffer offsets, and an offset that
/// survived the splice pointing at its own text is the whole of what 2.26j asks.
#[gtktest::test]
fn everything_below_a_toggled_block_still_addresses_its_own_text() {
    use gtk::prelude::TextBufferExt;
    const MD: &str = concat!(
        "# Top\n\nlead paragraph\n\n",
        "<details>\n<summary>S</summary>\n\n",
        "body text long enough to outrun the preview limit so the renders differ\n\n",
        "</details>\n\n",
        "## Below\n\n",
        "a distinctive tail paragraph\n\n",
        "[link text](https://example.invalid/target)\n"
    );
    let key = FoldKey(crate::renderer::disclosure::scan_document(MD)[0].start);
    let mut after = FoldState::default();
    after.toggle(key);

    let starting = super::super::build::build_render_products_with_theme(
        MD,
        None,
        1.0,
        false,
        crate::theme::active(),
        &FoldState::default(),
    );
    let outcome = super::splice(
        &starting.buf,
        None,
        &starting.anchored,
        &starting.disclosure_extents,
        MD,
        None,
        1.0,
        false,
        crate::theme::active(),
        &after,
        key,
    )
    .expect("the toggled block was drawn in the starting render");

    let buf = &starting.buf;
    let text = buf
        .slice(&buf.start_iter(), &buf.end_iter(), true)
        .to_string();
    let chars: Vec<char> = text.chars().collect();
    let off = |needle: &str| -> i32 {
        let byte = text.find(needle).expect("fixture text is present");
        text[..byte].chars().count() as i32
    };

    // Copy, at a position BELOW the toggled block.
    let tail = off("a distinctive tail paragraph");
    assert_eq!(
        crate::copymap::resolve(&outcome.products.copymap, MD, tail, tail + 28),
        "a distinctive tail paragraph",
        "text below the block copies as ITSELF, not as its neighbour"
    );

    // The link span below the block still covers the link's own rendered text.
    let link = outcome
        .products
        .links
        .iter()
        .find(|(_, _, url)| url.contains("example.invalid"))
        .expect("the link below the block is mapped");
    let (start, end) = (link.0, link.1);
    let covered: String = chars[start as usize..end as usize].iter().collect();
    assert_eq!(
        covered, "link text",
        "the link span below the block covers its own text"
    );

    // The heading below the block resolves to a line that really is that heading.
    let below = outcome
        .products
        .heading_sites
        .iter()
        .find(|h| h.slug.as_deref().is_some_and(|s| s.contains("below")))
        .expect("the heading below the block has a site");
    let at: String = chars[below.offset as usize..].iter().take(5).collect();
    assert_eq!(
        at, "Below",
        "the heading site below the block names its own line"
    );
}
