//! Pure scan of the disclosure tags in a raw-HTML fragment — `<details>`,
//! `</details>`, `<summary>`, `</summary>`. No GTK; fully unit-tested.
//!
//! Sibling of [`super::picture`], reading the same allowlist from
//! [`super::rawhtml`]. Two scanners rather than one because they answer different
//! questions about different elements; one allowlist because the *permitted set* is a
//! security posture and must have a single owner.
//!
//! # Why the grouping state cannot live in here
//!
//! A disclosure is not a fragment — it is a **span of the document**, and the parser
//! hands us its pieces separately. With the blank lines CommonMark requires,
//! pulldown-cmark emits:
//!
//! ```text
//!   HtmlBlock(<details><summary>Title</summary>)   <- one raw-HTML event
//!   Start(Paragraph) Text(…) End(Paragraph)        <- ORDINARY Markdown events
//!   HtmlBlock(</details>)                          <- another raw-HTML event
//! ```
//!
//! So the body needs no special rendering path at all — it is ordinary document
//! content, which is why rubric 2.26c can promise that everything renders exactly as
//! it does at top level. What the renderer must carry is the *pairing*, across events
//! this module never sees together. That state belongs to the renderer, exactly as
//! `<picture>`'s grouping does (ScrAP-147), and nesting makes it a **stack** rather
//! than a flag.
//!
//! # What "malformed" means here
//!
//! This scanner reports what it finds and judges nothing. An unclosed `<details>`, a
//! `<summary>` with no `<details>`, a second `<summary>` — all are emitted in
//! document order for the renderer to resolve, because the *recovery* rule is a
//! rendering decision (rubric 2.26d) and a scanner that silently dropped an unpaired
//! tag would deny the renderer the information it needs to recover well.

use super::rawhtml::{has_attr, recognise_html_element, tag_end, RawHtmlElement};

/// The collapsed-summary body-preview shortening rule (TDD 2.26) — split out rather
/// than grown in here, per the 500-line soft limit (`sdd/POLICY.md` § Code style),
/// mirroring how `tags.rs`/`tags/spec.rs` already split registration from decision.
mod preview;
pub(crate) use preview::preview_insert_text;

/// One disclosure-relevant tag from a raw-HTML fragment, in document order.
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) enum DetailsTag {
    /// `<details>` — opens a disclosure block. `open` carries the HTML `open`
    /// attribute, which renders the block expanded (rubric 2.26b).
    DetailsOpen { open: bool },
    /// `</details>` — closes the innermost open disclosure block.
    DetailsClose,
    /// `<summary>` — opens the summary line.
    SummaryOpen,
    /// The literal text between `<summary>` and `</summary>`, whitespace-trimmed.
    ///
    /// **The summary's text arrives inside the raw HTML, not as a Markdown `Text`
    /// event** — `<details><summary>Title</summary>` is one `Tag::HtmlBlock`, so
    /// "Title" never reaches the renderer's ordinary event path. Extracting it here
    /// keeps this module the only place that has to understand the fragment; the
    /// alternative is the renderer re-scanning the same string for a second purpose.
    ///
    /// Emitted only for a non-empty run, so `<summary></summary>` yields no text and
    /// the renderer applies its default label (rubric 2.26d).
    SummaryText(String),
    /// `</summary>` — closes the summary line.
    SummaryClose,
    /// A literal-text run the block contributes to the page, in document order with
    /// the tags around it.
    ///
    /// **Interleaved rather than collected**, because WHERE a run sits decides which
    /// frame it belongs to. A run between `</summary>` and `</details>` is the block's
    /// BODY — the unspaced case rubric 2.26d covers, where the whole construct is one
    /// raw-HTML block and the body never becomes Markdown events. Emitted after the
    /// whole tag stream instead, it landed *outside* the disclosure: a collapsed block
    /// printed its body anyway and its toggle became a visible no-op, and the export
    /// dropped the run entirely.
    ///
    /// Which runs exist at all is [`super::rawhtml::literal_text_runs`]'s decision —
    /// the allowlist still governs, so a `<script>`'s text is not here.
    ///
    /// `at` is the run's byte offset **within the fragment**, which is what lets
    /// [`scan_document`] give an unspaced block a real source range for its body — the
    /// range the fold splice re-renders from when the reader opens it again.
    Text { at: usize, text: String },
}

/// Scan a raw-HTML fragment for the ordered disclosure tag stream.
///
/// Every consumer of raw HTML calls **this**, never its own tag walk — the preview
/// renderer and the export sink alike — so the permitted set cannot be reproduced
/// approximately anywhere (CAM Document Rendering row 17: a construct taught to the
/// renderer alone is silently absent from every exported artefact).
pub(crate) fn scan_disclosure_tags(html: &str) -> Vec<DetailsTag> {
    let lower = html.to_ascii_lowercase();
    let mut tags = Vec::new();
    // The block's literal-text runs, merged into the tag stream by offset so each run
    // reaches the frame it sits inside. See [`DetailsTag::Text`].
    let mut runs = super::rawhtml::literal_text_runs(html)
        .into_iter()
        .peekable();
    // Byte offset just past the last `<summary>`'s `>`, while one is open. The text
    // run is closed by `</summary>`, never by the end of the fragment: an unclosed
    // `<summary>` yields no text rather than swallowing the rest of the block, which
    // is the same "does not swallow the remainder" rule rubric 2.26d states for an
    // unclosed `<details>`.
    let mut summary_from: Option<usize> = None;

    let mut i = 0usize;
    while let Some(rel) = lower[i..].find('<') {
        let start = i + rel;
        // Quote-aware: a `>` inside an attribute value does not end the tag, and a
        // scanner that thinks it does re-reads the tag's own tail as markup.
        let Some(end) = tag_end(html, start) else {
            break;
        };
        while runs.peek().is_some_and(|run| run.at < start) {
            let Some(run) = runs.next() else { break };
            push_text_run(&mut tags, run.at, run.text);
        }
        let tag_lower = &lower[start..=end];
        match recognise_html_element(tag_lower) {
            Some(RawHtmlElement::DetailsOpen) => tags.push(DetailsTag::DetailsOpen {
                // `open` is boolean: presence is the whole signal.
                open: has_attr(tag_lower, "open"),
            }),
            Some(RawHtmlElement::DetailsClose) => tags.push(DetailsTag::DetailsClose),
            Some(RawHtmlElement::SummaryOpen) => {
                tags.push(DetailsTag::SummaryOpen);
                summary_from = Some(end + 1);
            }
            Some(RawHtmlElement::SummaryClose) => {
                if let Some(from) = summary_from.take() {
                    let text = html[from..start].trim();
                    if !text.is_empty() {
                        tags.push(DetailsTag::SummaryText(text.to_owned()));
                    }
                }
                tags.push(DetailsTag::SummaryClose);
            }
            // Image elements are allowlisted but belong to `renderer::picture`. Named
            // explicitly rather than caught by a `_` arm so that adding an element to
            // the allowlist fails to compile here until someone decides whether this
            // scanner cares about it — a wildcard would render it as nothing, silently.
            Some(
                RawHtmlElement::PictureOpen
                | RawHtmlElement::PictureClose
                | RawHtmlElement::Source
                | RawHtmlElement::Img,
            ) => {}
            None => {}
        }
        i = end + 1;
    }
    for run in runs {
        push_text_run(&mut tags, run.at, run.text);
    }
    tags
}

/// Push one literal run, trimmed, dropping it when nothing but whitespace is left —
/// the block separator around it is the renderer's to decide, not the document's.
fn push_text_run(tags: &mut Vec<DetailsTag>, at: usize, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    // The offset follows the TRIM, so it names the first character actually shown.
    let lead = text.len() - text.trim_start().len();
    tags.push(DetailsTag::Text {
        at: at + lead,
        text: trimmed.to_owned(),
    });
}

/// One `<details>` block the document declares, in document order.
///
/// Produced by [`scan_document`], which walks the same event stream the renderer
/// walks — so the Nth span is the Nth `<details>` the renderer will open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisclosureSpan {
    /// Source byte offset where the opening raw-HTML **block** begins — the value
    /// the fold model keys a disclosure on.
    pub start: usize,
    /// The HTML `open` attribute, which renders the block expanded (rubric 2.26b).
    pub open: bool,
    /// Source byte range of the block's BODY: everything after the opening raw-HTML
    /// block up to the `</details>` that closes it.
    ///
    /// `None` when the block is **never closed** — and that distinction is the whole
    /// reason this scan exists. See [`scan_document`].
    pub body: Option<std::ops::Range<usize>>,
}

/// A document-order cursor over [`scan_document`]'s spans — the ONE answer to "is the
/// Nth `<details>` closed?", held by every walk that asks it.
///
/// **Both sinks ask, and the fail-safe is the answer's whole value.** The preview
/// renderer and the export walk each open frames from the same pre-scan, so each needs
/// the cursor; when each carried its own, the export's copy had lost the `start`
/// cross-check and the diagnostic with it, so a divergence between the two walks was
/// silent there and loud here. Divergence cannot happen while both parse the same
/// string with the same options — which is exactly why the check must exist: nothing
/// else would ever report that the premise had stopped holding.
#[derive(Debug, Default)]
pub(crate) struct SpanCursor {
    seen: usize,
}

impl SpanCursor {
    /// A cursor that has already answered for `seen` blocks — how a region render
    /// resumes a walk mid-document (`RegionSeed`).
    pub(crate) fn at(seen: usize) -> Self {
        Self { seen }
    }

    /// How many `<details>` this cursor has answered for — the index of the span the
    /// NEXT call will consume, which is a frame's identity in the pre-scan.
    pub(crate) fn seen(&self) -> usize {
        self.seen
    }

    /// Is the `<details>` opening at source byte `block_start` ever CLOSED?
    ///
    /// Only a closed block may collapse: a collapsed frame that never pops suppresses
    /// every remaining event, so an unclosed one would delete the rest of the document
    /// (rubric 2.26d).
    ///
    /// Advances once per `<details>` however the block turns out. A disagreement
    /// between the pre-scan's offset and the walk's is logged and answered **`false`**
    /// — the reply that can only ever render MORE of the document, never less.
    pub(crate) fn opening_is_closed(
        &mut self,
        spans: &[DisclosureSpan],
        block_start: usize,
    ) -> bool {
        let span = spans.get(self.seen);
        self.seen += 1;
        match span {
            Some(span) if span.start == block_start => span.body.is_some(),
            other => {
                log::error!(
                    "disclosure pre-scan disagrees with the render walk at source byte \
                     {block_start} (scan says {:?}); treating the block as unclosed so \
                     nothing after it can be suppressed",
                    other.map(|s| s.start)
                );
                false
            }
        }
    }
}

/// Every `<details>` the document declares, in document order, each paired with the
/// `</details>` that closes it — or marked as never closed.
///
/// # Why the renderer cannot answer this itself
///
/// The renderer walks the event stream once, forwards. When it meets a `<details>`
/// it must decide *there and then* whether to collapse the body, and "is this block
/// ever closed?" is a fact about source it has not read yet. Guessing "yes" is what
/// it used to do, and the consequence is not a cosmetic one: a collapsed frame that
/// never pops suppresses every event to the end of the stream, so **an unclosed
/// `<details>` renders its summary line and deletes the entire rest of the
/// document**. MEASURED on `before` + `<details><summary>S</summary>` + a body +
/// `## After`: the buffer held `before` and the summary line, and nothing else. That
/// is rubric 2.26d's exact prohibition, and for an untrusted document (TDD 2.7) one
/// stray tag blanks the page.
///
/// Browsers close an unclosed `<details>` implicitly at the end of its parent, which
/// makes the remainder of the document its body — deliberately NOT copied here, for
/// the reason above: an authoring slip, or a half-typed tag in a live-preview
/// session, must not be able to hide a document.
///
/// # Why the pairing is by ORDINAL, not by offset
///
/// A block's key is the offset of the raw-HTML *block* it opens in, and consecutive
/// raw-HTML lines with no blank line between them are ONE block — so two `<details>`
/// can share an offset. Document order cannot collide, and both this scan and the
/// renderer walk the same events in the same order, so the Nth here is the Nth
/// there. The caller checks the offsets agree anyway and fails safe if they do not.
pub(crate) fn scan_document(md: &str) -> Vec<DisclosureSpan> {
    use pulldown_cmark::{Event, Tag, TagEnd};

    // **One document, like every other parse site** (`super::normalize`), so this scan
    // and the renderer's walk agree about where blocks begin — which is what makes the
    // Nth span here the Nth `<details>` there.
    //
    // **No divergence is currently constructible, and that is stated rather than
    // implied.** The pre-pass changes block boundaries by making a tab-padded table
    // parse as a table instead of a paragraph, but `details` and `summary` are
    // CommonMark type-6 HTML blocks, which interrupt a paragraph — so the tags form
    // their own block either way. MEASURED over eight tab-padded shapes (a table above
    // the block, below it, a closer inside a cell, tabs in the tags and in the label,
    // a quoted table): identical spans every time. The call stays because the property
    // it buys is about the CALLER, not about tabs — a caller handing an unnormalised
    // document (the editor's raw source, say) would otherwise be parsing a different
    // document from the renderer, and the ordinal correspondence above is the thing
    // that breaks. The pre-pass preserves length and position, so every offset below
    // still indexes the caller's string.
    let normalized = super::NormalizedMd::new(md);
    let md = normalized.as_str();

    let mut spans: Vec<DisclosureSpan> = Vec::new();
    // Indices into `spans` for the blocks still open, innermost last.
    let mut open_stack: Vec<usize> = Vec::new();
    // The raw-HTML block currently being accumulated: its source range, and the
    // text of its lines. Mirrors the renderer's own `html_acc`/`in_html_block`,
    // because a block's tags are only complete at its `End` event.
    let mut block: Option<(std::ops::Range<usize>, String)> = None;

    for (ev, src) in pulldown_cmark::Parser::new_ext(md, super::md_options()).into_offset_iter() {
        match ev {
            Event::Start(Tag::HtmlBlock) => block = Some((src, String::new())),
            Event::Html(t) => {
                if let Some((_, acc)) = &mut block {
                    acc.push_str(&t);
                }
            }
            Event::End(TagEnd::HtmlBlock) => {
                let Some((range, html)) = block.take() else {
                    continue;
                };
                // Where this block's own literal text sits in the SOURCE, if any. An
                // unspaced `<details>` opens and closes inside ONE block, so its body
                // is this text rather than a run of Markdown events between two
                // blocks — and a body recorded as the degenerate `start..start` is a
                // block the reader can collapse and never open again, because the
                // fold splice re-renders from exactly that range.
                let mut literal: Option<std::ops::Range<usize>> = None;
                for tag in scan_disclosure_tags(&html) {
                    match tag {
                        DetailsTag::Text { at, ref text } => {
                            let run = range.start + at..range.start + at + text.len();
                            literal = Some(match literal {
                                Some(prev) => prev.start.min(run.start)..prev.end.max(run.end),
                                None => run,
                            });
                        }
                        DetailsTag::DetailsOpen { open } => {
                            open_stack.push(spans.len());
                            spans.push(DisclosureSpan {
                                start: range.start,
                                open,
                                // Filled in by the `</details>` that closes it; a
                                // block never closed keeps `None`, which is the
                                // answer this whole scan exists to give.
                                body: None,
                            });
                        }
                        DetailsTag::DetailsClose => {
                            // A stray `</details>` closes nothing rather than
                            // underflowing — malformed input is reported, not
                            // judged (rubric 2.26d).
                            if let Some(i) = open_stack.pop() {
                                // Opened in THIS block: unspaced, so the body is the
                                // literal text the block itself carries.
                                spans[i].body = Some(if spans[i].start == range.start {
                                    literal.clone().unwrap_or(range.start..range.start)
                                } else {
                                    spans[i].start..range.start
                                });
                            }
                        }
                        DetailsTag::SummaryOpen
                        | DetailsTag::SummaryText(_)
                        | DetailsTag::SummaryClose => {}
                    }
                }
            }
            // ── everything else, each named rather than swallowed (lint check 15) ──

            // An inline raw-HTML run is not a block, so it opens no disclosure this
            // scan would honour: a `<details>` that is not at block level has no
            // block offset to be keyed on. **Both sinks decline it too**, which is
            // what keeps the walks agreed rather than merely hoping they are —
            // `renderer::start::feed_inline_html` and `export::walk::inline_html`
            // each take the image scanner alone, and each says so. This claim was
            // once made here while the renderer did the opposite, and a paragraph
            // that merely MENTIONED `<details>` then disabled the feature for the
            // rest of the document.
            Event::InlineHtml(_) => {}
            // Every other container. A disclosure tag can only arrive inside raw
            // HTML, so no other construct's boundaries can open or close one — but
            // a `Tag::HtmlBlock` nested in one of these still reaches the arms
            // above, which is what lets a disclosure live inside a blockquote or a
            // list item (rubric 2.26c).
            Event::Start(_) | Event::End(_) => {}
            // Content, not markup: these carry no tags for this scan to pair.
            Event::Text(_)
            | Event::Code(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::TaskListMarker(_) => {}
            // Never emitted — `md_options` enables neither the math nor the footnote
            // extension (TDD 2.25). Named so that enabling one is a visible change
            // here rather than a silent omission from the pairing.
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {}
        }
    }
    spans
}

/// The text a disclosure body WOULD render as, for the consumers that must ask a
/// question about content this render withheld.
///
/// Find is the one that needs it (rubric 11.10): a collapsed body is in no buffer and
/// no label, so "does it contain the query?" can only be asked of the source — and
/// asking it of the *Markdown* source is wrong in both directions. Searching the raw
/// text finds `*` in every emphasised word, a match the reader could never see, and
/// misses `foobar` in `foo*bar*`, a match they can see the moment the block opens.
///
/// So the body is parsed and its text and code runs are concatenated: the same
/// reduction [`crate::outline`] performs to get a heading's display text, and for the
/// same reason — it is the closest a display-free walk gets to what the page will say.
/// Run separators are emitted as newlines so a query cannot match ACROSS a gap the
/// reader will see, claiming a hit the page does not have.
///
/// **The residue, stated:** content that renders into a WIDGET rather than into buffer
/// text — a table cell, an image's alt — is reported here as its source text, which is
/// not what a cell search would index. A query matching only inside a collapsed
/// table's cells may therefore be counted here and land, after expansion, on the cell
/// hit the ordinary widget-tree scan finds; the count is right, the intermediate
/// position is approximate.
pub(crate) fn body_plain_text(body_src: &str) -> String {
    use pulldown_cmark::{Event, TagEnd};

    // One document, like every other parse site (`super::normalize`) — and here the
    // pre-pass genuinely changes the answer: unnormalised, a tab-padded table is one
    // paragraph whose text carries the `|` separators, so a query spanning two cells
    // matches text the rendered page never shows.
    let normalized = super::NormalizedMd::new(body_src);
    let mut plain = String::new();
    for ev in pulldown_cmark::Parser::new_ext(normalized.as_str(), super::md_options()) {
        match ev {
            Event::Text(t) | Event::Code(t) => plain.push_str(&t),
            // A break or a rule separates two runs the reader sees apart.
            Event::SoftBreak | Event::HardBreak | Event::Rule => plain.push('\n'),
            // A BLOCK boundary separates runs; an INLINE one does not. `foo*bar*`
            // renders as the single word `foobar`, so treating its emphasis boundary
            // as a separator would make a search for that word miss it — which is one
            // of the two failures this whole reduction exists to avoid.
            Event::Start(tag) => {
                if !super::is_inline_tag(&tag) {
                    plain.push('\n');
                }
            }
            Event::End(end) => {
                if !matches!(
                    end,
                    TagEnd::Emphasis
                        | TagEnd::Strong
                        | TagEnd::Strikethrough
                        | TagEnd::Superscript
                        | TagEnd::Subscript
                        | TagEnd::Link
                        | TagEnd::Image
                ) {
                    plain.push('\n');
                }
            }
            // Raw HTML is sanitised by omission, exactly as the renderer omits it, so
            // it contributes no searchable text — including a nested disclosure's own
            // tags, whose body text still arrives above as ordinary events.
            Event::Html(_) | Event::InlineHtml(_) => {}
            // A checkbox is drawn, not written.
            Event::TaskListMarker(_) => {}
            // Never emitted: `md_options` enables neither the math nor the footnote
            // extension (TDD 2.25).
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {}
        }
    }
    plain
}

#[cfg(test)]
mod body_text_tests {
    use super::body_plain_text;

    #[test]
    fn emphasis_markers_are_stripped_so_a_query_matches_what_the_reader_would_see() {
        // Both directions of the defect this exists to avoid, in one fixture.
        let plain = body_plain_text("some **bold** and `code` here\n");
        assert!(plain.contains("bold"), "{plain:?}");
        assert!(plain.contains("code"), "{plain:?}");
        assert!(
            !plain.contains('*'),
            "a marker the page never shows: {plain:?}"
        );
        assert!(
            !plain.contains('`'),
            "a marker the page never shows: {plain:?}"
        );
    }

    #[test]
    fn a_query_split_by_markup_still_matches_the_rendered_word() {
        // `foo*bar*` renders as `foobar`, so searching the SOURCE would miss it.
        assert!(body_plain_text("foo*bar*\n").contains("foobar"));
    }

    #[test]
    fn separate_runs_do_not_run_together() {
        // Two paragraphs the reader sees apart must not form a match across the gap.
        let plain = body_plain_text("alpha\n\nbeta\n");
        assert!(!plain.contains("alphabeta"), "{plain:?}");
        assert!(plain.contains("alpha") && plain.contains("beta"));
    }

    #[test]
    fn a_tab_padded_table_reads_as_a_table_and_not_as_its_own_separators() {
        // The pre-pass case, stated as a behaviour: unnormalised this is ONE
        // paragraph whose text includes the `|`s, so `a | b` would match text the
        // rendered page never shows.
        let plain = body_plain_text("| a\t| b\t|\n|---\t|---\t|\n| c\t| d\t|\n");
        assert!(
            !plain.contains('|'),
            "cell separators are not text: {plain:?}"
        );
        for cell in ["a", "b", "c", "d"] {
            assert!(plain.contains(cell), "cell {cell:?} missing from {plain:?}");
        }
    }

    #[test]
    fn raw_html_inside_a_body_contributes_no_text() {
        let plain = body_plain_text("<div>markup</div>\n\nreal text\n");
        assert!(plain.contains("real text"));
        assert!(!plain.contains("div"), "{plain:?}");
    }

    #[test]
    fn an_empty_body_is_empty() {
        assert!(body_plain_text("").is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::{scan_disclosure_tags, DetailsTag};

    fn open() -> DetailsTag {
        DetailsTag::DetailsOpen { open: false }
    }
    fn open_expanded() -> DetailsTag {
        DetailsTag::DetailsOpen { open: true }
    }
    fn text_at(at: usize, text: &str) -> DetailsTag {
        DetailsTag::Text {
            at,
            text: text.to_owned(),
        }
    }

    #[test]
    fn scans_a_whole_disclosure_header_in_order() {
        assert_eq!(
            scan_disclosure_tags("<details><summary>Title</summary>"),
            vec![
                open(),
                DetailsTag::SummaryOpen,
                DetailsTag::SummaryText("Title".into()),
                DetailsTag::SummaryClose
            ]
        );
    }

    #[test]
    fn the_open_attribute_is_detected_in_every_html_spelling() {
        // HTML boolean attributes: presence is the signal, whatever the value says.
        for src in [
            "<details open>",
            "<details OPEN>",
            "<details open=\"\">",
            "<details open=\"open\">",
            "<details open=\"false\">",
            "<details\topen>",
            "<details open >",
        ] {
            assert_eq!(
                scan_disclosure_tags(src),
                vec![open_expanded()],
                "{src} should scan as expanded"
            );
        }
    }

    #[test]
    fn a_details_without_open_is_collapsed() {
        assert_eq!(scan_disclosure_tags("<details>"), vec![open()]);
        assert_eq!(scan_disclosure_tags("<details class=\"x\">"), vec![open()]);
    }

    #[test]
    fn an_attribute_merely_starting_with_open_does_not_expand() {
        // `opened` and `open-thing` are not the `open` attribute. Without the
        // name-end check this scans as expanded and a collapsed block silently
        // renders open — a wrong render with nothing to indicate it.
        assert_eq!(scan_disclosure_tags("<details opened>"), vec![open()]);
        assert_eq!(scan_disclosure_tags("<details data-open>"), vec![open()]);
    }

    #[test]
    fn close_tags_are_not_read_as_open_tags() {
        assert_eq!(
            scan_disclosure_tags("</details></summary>"),
            vec![DetailsTag::DetailsClose, DetailsTag::SummaryClose]
        );
    }

    #[test]
    fn nesting_is_reported_flat_for_the_renderer_to_stack() {
        // The scanner does not pair; the renderer carries the stack across events.
        assert_eq!(
            scan_disclosure_tags("<details><details open></details></details>"),
            vec![
                open(),
                open_expanded(),
                DetailsTag::DetailsClose,
                DetailsTag::DetailsClose
            ]
        );
    }

    #[test]
    fn malformed_input_is_reported_not_judged() {
        // Recovery is the renderer's decision (rubric 2.26d), so an unpaired tag must
        // reach it rather than being swallowed here.
        assert_eq!(
            scan_disclosure_tags("</details>"),
            vec![DetailsTag::DetailsClose]
        );
        assert_eq!(
            scan_disclosure_tags("<summary>orphan</summary>"),
            vec![
                DetailsTag::SummaryOpen,
                DetailsTag::SummaryText("orphan".into()),
                DetailsTag::SummaryClose
            ]
        );
        assert_eq!(scan_disclosure_tags("<details>"), vec![open()]);
    }

    #[test]
    fn unterminated_tag_does_not_hang_or_panic() {
        // A document is untrusted input (TDD 2.7); a truncated tag must terminate the
        // scan rather than loop.
        assert_eq!(scan_disclosure_tags("<details"), vec![]);
        assert_eq!(scan_disclosure_tags("<details><summary"), vec![open()]);
    }

    /// F-010 in this scanner: `find('>')` split the tag at a `>` inside a quoted
    /// attribute value, so the tag's own tail was re-scanned as markup.
    #[test]
    fn a_bracket_inside_a_quoted_attribute_does_not_split_the_tag() {
        assert_eq!(
            scan_disclosure_tags("<details title=\"a>b\"><summary>S</summary></details>"),
            vec![
                open(),
                DetailsTag::SummaryOpen,
                DetailsTag::SummaryText("S".into()),
                DetailsTag::SummaryClose,
                DetailsTag::DetailsClose,
            ]
        );
    }

    /// F-SEC-004: `open` was read out of a *different* attribute's quoted value, so a
    /// document could force a disclosure open by titling it.
    #[test]
    fn open_is_not_read_out_of_another_attributes_value() {
        assert_eq!(
            scan_disclosure_tags("<details title=\"open\"></details>"),
            vec![open(), DetailsTag::DetailsClose]
        );
    }

    #[test]
    fn image_elements_are_ignored_by_this_scanner() {
        // They are allowlisted, but they are `renderer::picture`'s to interpret.
        assert_eq!(
            scan_disclosure_tags("<picture><img src=\"a.png\"></picture>"),
            vec![]
        );
    }

    #[test]
    fn unpermitted_elements_are_dropped() {
        assert_eq!(
            scan_disclosure_tags("<script>alert(1)</script><div>x</div>"),
            vec![]
        );
    }

    #[test]
    fn tags_are_found_amongst_surrounding_text() {
        // And the text between them is interleaved in document order, so each run
        // reaches the frame it sits inside rather than following the whole stream.
        assert_eq!(
            scan_disclosure_tags("lead <details open> trail </details> end"),
            vec![
                text_at(0, "lead"),
                open_expanded(),
                text_at(20, "trail"),
                DetailsTag::DetailsClose,
                text_at(37, "end"),
            ]
        );
    }

    #[test]
    fn summary_text_is_extracted_and_trimmed() {
        // The renderer cannot get this from a Markdown Text event — it is inside the
        // raw HTML block — so the scanner is the only place it can come from.
        assert_eq!(
            scan_disclosure_tags("<details>\n<summary>  Spaced  </summary>\n"),
            vec![
                open(),
                DetailsTag::SummaryOpen,
                DetailsTag::SummaryText("Spaced".into()),
                DetailsTag::SummaryClose
            ]
        );
    }

    #[test]
    fn an_empty_summary_yields_no_text_so_the_renderer_can_default_it() {
        // Rubric 2.26d: a missing label shows "Details". Emitting an empty SummaryText
        // would make the renderer decide between "" and absent, which is a distinction
        // with no meaning here.
        assert_eq!(
            scan_disclosure_tags("<summary></summary>"),
            vec![DetailsTag::SummaryOpen, DetailsTag::SummaryClose]
        );
        assert_eq!(
            scan_disclosure_tags("<summary>   </summary>"),
            vec![DetailsTag::SummaryOpen, DetailsTag::SummaryClose]
        );
    }

    #[test]
    fn an_unclosed_summary_does_not_swallow_the_rest_of_the_block() {
        // Same rule rubric 2.26d states for an unclosed <details>: recover, never eat
        // the remainder. The text run is closed by `</summary>` or not at all.
        assert_eq!(
            scan_disclosure_tags("<details><summary>Title"),
            vec![open(), DetailsTag::SummaryOpen]
        );
    }
}

#[cfg(test)]
mod document_scan_tests {
    use super::{scan_document, DisclosureSpan};

    fn shape(md: &str) -> Vec<(usize, bool, bool)> {
        scan_document(md)
            .into_iter()
            .map(|DisclosureSpan { start, open, body }| (start, open, body.is_some()))
            .collect()
    }

    #[test]
    fn a_closed_block_reports_its_body() {
        let md = "before\n\n<details>\n<summary>S</summary>\n\nbody\n\n</details>\n";
        let spans = scan_document(md);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, md.find("<details>").unwrap());
        let body = spans[0].body.clone().expect("a closed block has a body");
        assert!(
            md[body.clone()].contains("body"),
            "the body spans the content between the tags: {:?}",
            &md[body]
        );
    }

    #[test]
    fn an_unclosed_block_reports_no_body() {
        // The whole reason this scan exists: the renderer must not collapse this one,
        // because a frame that never pops suppresses the rest of the document.
        let md = "<details>\n<summary>S</summary>\n\nbody\n\n## After\n";
        assert_eq!(shape(md), vec![(0, false, false)]);
    }

    #[test]
    fn the_open_attribute_is_carried_through() {
        let md = "<details open>\n<summary>S</summary>\n\nbody\n\n</details>\n";
        assert_eq!(shape(md), vec![(0, true, true)]);
    }

    #[test]
    fn nesting_pairs_innermost_first() {
        let md = concat!(
            "<details>\n<summary>Outer</summary>\n\n",
            "<details>\n<summary>Inner</summary>\n\ninner\n\n</details>\n\n",
            "</details>\n"
        );
        let spans = scan_document(md);
        assert_eq!(spans.len(), 2);
        assert!(spans[0].body.is_some(), "the outer block closes");
        assert!(spans[1].body.is_some(), "the inner block closes");
        let (outer, inner) = (
            spans[0].body.clone().unwrap(),
            spans[1].body.clone().unwrap(),
        );
        assert!(
            outer.start < inner.start && inner.end < outer.end,
            "the inner body nests inside the outer one: {outer:?} vs {inner:?}"
        );
    }

    #[test]
    fn an_unclosed_inner_block_takes_the_only_close_tag() {
        // `</details>` closes the INNERMOST open block, so with one close tag between
        // two openers it is the inner one that closes and the OUTER one that is left
        // unpaired. Stated as a test because the opposite reading is the intuitive
        // one, and it decides which block the renderer refuses to fold.
        let md = concat!(
            "<details>\n<summary>Outer</summary>\n\n",
            "<details>\n<summary>Inner</summary>\n\ninner\n\n</details>\n\n",
            "after\n"
        );
        let spans = scan_document(md);
        assert_eq!(spans.len(), 2);
        assert!(spans[0].body.is_none(), "the outer block never closes");
        assert!(spans[1].body.is_some(), "the inner block does");
    }

    #[test]
    fn a_stray_close_tag_pairs_with_nothing() {
        // It must not underflow, and it must not retroactively close a later block.
        let md = "</details>\n\n<details>\n<summary>S</summary>\n\nbody\n";
        assert_eq!(
            shape(md),
            vec![(md.find("<details>").unwrap(), false, false)]
        );
    }

    #[test]
    fn a_document_with_no_disclosures_scans_to_nothing() {
        assert!(scan_document("# Just a heading\n\nand prose.\n").is_empty());
        assert!(scan_document("").is_empty());
    }

    #[test]
    fn two_siblings_are_reported_separately_and_in_order() {
        let md = concat!(
            "<details>\n<summary>One</summary>\n\na\n\n</details>\n\n",
            "<details open>\n<summary>Two</summary>\n\nb\n\n</details>\n"
        );
        let spans = scan_document(md);
        assert_eq!(spans.len(), 2);
        assert!(spans[0].start < spans[1].start, "document order");
        assert!(
            !spans[0].open && spans[1].open,
            "each carries its own attribute"
        );
        assert!(spans.iter().all(|s| s.body.is_some()));
    }

    #[test]
    fn the_scan_agrees_with_the_fragment_scanner_about_what_a_details_is() {
        // One allowlist, one recogniser: a shape the fragment scanner refuses must not
        // appear here either, or the renderer would consult a pairing for a block it
        // never opens and every later ordinal would be off by one.
        for md in [
            "<detailsx>\n\nbody\n",
            "<div>\n\nbody\n",
            "<script>\n\nbody\n",
        ] {
            assert!(scan_document(md).is_empty(), "{md:?} is not a disclosure");
        }
    }
}

#[cfg(test)]
mod span_cursor_tests {
    use super::{DisclosureSpan, SpanCursor};

    fn span(start: usize, closed: bool) -> DisclosureSpan {
        DisclosureSpan {
            start,
            open: false,
            body: closed.then(|| start..start + 10),
        }
    }

    #[test]
    fn it_answers_each_span_in_document_order() {
        let spans = [span(0, true), span(50, false), span(90, true)];
        let mut cursor = SpanCursor::default();
        assert!(cursor.opening_is_closed(&spans, 0));
        assert!(!cursor.opening_is_closed(&spans, 50));
        assert!(cursor.opening_is_closed(&spans, 90));
        assert_eq!(cursor.seen(), 3);
    }

    /// The fail-safe. A walk that disagrees with the pre-scan is answered "not
    /// closed", which can only ever render MORE of the document — a wrong "closed"
    /// collapses a frame that never pops and deletes everything after it.
    #[test]
    fn a_disagreement_answers_unclosed_and_still_advances() {
        let spans = [span(0, true), span(50, true)];
        let mut cursor = SpanCursor::default();
        assert!(
            !cursor.opening_is_closed(&spans, 7),
            "offset 7 is not the span's own start"
        );
        assert_eq!(cursor.seen(), 1, "and the cursor still advanced past it");
        assert!(
            !cursor.opening_is_closed(&spans, 999),
            "the walk is now one ahead of the scan, so every later block disagrees too"
        );
    }

    #[test]
    fn running_past_the_end_is_a_disagreement_not_a_panic() {
        let mut cursor = SpanCursor::default();
        assert!(!cursor.opening_is_closed(&[], 0));
    }

    #[test]
    fn a_resumed_cursor_starts_where_the_scratch_walk_stood() {
        let spans = [span(0, false), span(50, true)];
        let mut cursor = SpanCursor::at(1);
        assert!(cursor.opening_is_closed(&spans, 50));
    }
}
