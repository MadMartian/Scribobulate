//! `Renderer::start_tag` — the block/inline OPEN handler: paragraph/heading/list/
//! blockquote spacing, table-cell markup accumulation, inline-tag pushes, and the
//! image safety gate + anchored-picture (or broken-image placeholder) build.

use super::blockspacing;
use super::image::image_placeholder_tooltip;
use super::{Renderer, TableState, DEFAULT_SUMMARY_LABEL};
use crate::links::{resolve_image, ImageResolution};
use gtk::prelude::*;
use pulldown_cmark::{CodeBlockKind, Tag};

impl Renderer {
    /// Write whatever separator `kind` needs before its own content.
    ///
    /// The GTK half of `blockspacing`: that module decides, this applies. Split so the
    /// spacing rules — the most-exercised decisions in the renderer, and the ones whose
    /// failure is a silently missing or doubled blank line — are reachable by a unit test
    /// rather than only by rendering a document and reading the text back.
    fn apply_lead_in(&mut self, kind: blockspacing::BlockKind) {
        let cx = blockspacing::BlockContext {
            list_item_open: self.list_item_open,
            inside_list: !self.lists.is_empty(),
            list_first_item: self.list_first_item,
            at_start: self.at_start,
        };
        match blockspacing::lead_in(kind, cx) {
            blockspacing::LeadIn::Nothing => {}
            blockspacing::LeadIn::Newline => self.newline(),
            blockspacing::LeadIn::BlockGap => self.block_sep(),
        }
    }

    pub(super) fn start_tag(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                self.block_sep();
                self.heading = Some(level);
                // Record where the heading text starts and reset its accumulator;
                // the slug is computed at TagEnd::Heading.
                self.heading_start = self.end_offset();
                self.heading_text.clear();
            }
            Tag::Paragraph => {
                // The RULE is `blockspacing`'s; this arm only carries it out. Setting
                // the flag unconditionally is equivalent to the old first-branch-only
                // clear: the other branches are reached only when it is already false.
                self.apply_lead_in(blockspacing::BlockKind::Paragraph);
                self.list_item_open = false;
            }
            Tag::BlockQuote(_) => {
                if self.blockquote_depth == 0 {
                    self.block_sep();
                }
                // Blockquote content flows into the buffer as normal text + tags
                // (selectable, links work, no anchored widget to churn — GTK4Rs/AP-23).
                // EVERY level records where it starts, not just the outermost: each one
                // closes into its own span so it can draw its own accent bar at its own
                // offset (TDD 2.11b). Innermost is last, so the matching TagEnd pops.
                self.blockquote_starts.push(self.end_offset());
                self.blockquote_depth += 1;
            }
            Tag::List(start) => {
                self.apply_lead_in(blockspacing::BlockKind::List);
                // Always start ordered lists at 1 regardless of source numbers;
                // TagEnd::Item increments the counter, so any disordered or
                // repeated source numerals render as 1, 2, 3 …
                self.lists.push(start.map(|_| 1u64));
                self.list_first_item = true;
            }
            Tag::Item => {
                // Tag::List already separated the first item; `blockspacing` holds that
                // rule. Clearing the flag unconditionally is equivalent to the old
                // first-branch-only clear, for the same reason as `Tag::Paragraph`.
                self.apply_lead_in(blockspacing::BlockKind::Item);
                self.list_first_item = false;
                // Record where this item starts (after the leading newline) so
                // that TagEnd::Item can apply the hanging-indent tag over the full
                // item span. Lists inside blockquotes are buffer text too now, so
                // this is unconditional — they just ALSO carry the blockquote tag,
                // whose margin the `li-{depth}` tag accumulates onto (`quoted` below).
                let item_start = self.end_offset();
                self.item_starts.push(item_start);
                // Record this item's marker for the drawn gutter. Ordered/bullet is
                // known here; a task item is upgraded to `Task` when its
                // `TaskListMarker` fires.
                let kind = match self.lists.last() {
                    Some(Some(n)) => crate::renderer::ListMarkerKind::Ordered(*n),
                    _ => crate::renderer::ListMarkerKind::Bullet,
                };
                self.list_markers.push(crate::renderer::ListMarker {
                    depth: self.lists.len(),
                    kind,
                    first_line: item_start,
                    quoted: self.blockquote_depth > 0,
                });
                self.list_item_open = true;
                // NO inline marker text is inserted: a bullet /
                // number / task checkbox is drawn in a left gutter in Phase 2 and occupies
                // ZERO buffer chars, so an item's content starts immediately with its text.
                // Moving the marker out of the buffer makes selection/copy skip it for free
                // (as major word processors do) and retires the fragile hanging-indent
                // (`indent`) that the old inline marker forced (ScrAP-118). The `ListMarker`
                // recorded just above is the data seam the gutter draw consumes; the
                // ordered counter still advances at TagEnd::Item.
            }
            Tag::CodeBlock(kind) => {
                self.block_sep();
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => l.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new()));
            }
            Tag::Table(aligns) => {
                self.block_sep();
                self.table = Some(TableState {
                    aligns: aligns
                        .iter()
                        .copied()
                        .map(crate::mdtable::align_of)
                        .collect(),
                    col: 0,
                    rows: Vec::new(),
                    in_cell: false,
                    in_head: false,
                    cell_markup: String::new(),
                    cell_plain: String::new(),
                    cell_sole_link: None,
                    cell_mixed: false,
                    in_link: None,
                    cell_content_evs: Vec::new(),
                    cell_off: 0,
                });
            }
            Tag::TableHead | Tag::TableRow => {
                // Each thead/tbody row starts a new row of cells; only the head row
                // (`Tag::TableHead`) styles its cells as a header.
                if let Some(ts) = &mut self.table {
                    ts.in_head = matches!(tag, Tag::TableHead);
                    ts.rows.push(Vec::new());
                    ts.col = 0;
                }
            }
            Tag::TableCell => {
                if let Some(ts) = &mut self.table {
                    ts.in_cell = true;
                    ts.cell_markup.clear();
                    ts.cell_plain.clear();
                    ts.cell_sole_link = None;
                    ts.cell_mixed = false;
                    ts.in_link = None;
                    ts.cell_content_evs.clear();
                    ts.cell_off = 0;
                }
            }
            Tag::Strong => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        // Themed: `bold_weight`, not a bare `<b>` — TDD 18.18.
                        let open = super::bold_open(&self.theme);
                        ts.cell_markup.push_str(&open);
                    }
                } else {
                    self.inline_tags.push(crate::tags::TagName::Bold);
                }
            }
            Tag::Emphasis => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        ts.cell_markup.push_str("<i>");
                    }
                } else {
                    self.inline_tags.push(crate::tags::TagName::Italic);
                }
            }
            Tag::Strikethrough => {
                if self.in_table_cell() {
                    // Themed: `strikethrough_rgba` — the cell twin of the body
                    // `TagName::Strike` tag (TDD 18.23). `</s>` vs `</span>` is decided
                    // by the same call in `end.rs`; see `strike_tags`.
                    let (open, _close) = super::strike_tags(&self.theme);
                    if let Some(ts) = &mut self.table {
                        ts.cell_markup.push_str(&open);
                    }
                } else {
                    self.inline_tags.push(crate::tags::TagName::Strike);
                }
            }
            Tag::Superscript => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        // Themed: `supsub_scale` + `superscript_rise` — TDD 18.18.
                        let open = super::superscript_open(&self.theme);
                        ts.cell_markup.push_str(&open);
                    }
                } else {
                    self.inline_tags.push(crate::tags::TagName::Superscript);
                }
            }
            Tag::Subscript => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        // Themed: `supsub_scale` + `subscript_rise` — TDD 18.18.
                        let open = super::subscript_open(&self.theme);
                        ts.cell_markup.push_str(&open);
                    }
                } else {
                    self.inline_tags.push(crate::tags::TagName::Subscript);
                }
            }
            Tag::Link { dest_url, .. } => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        // If anything was already in the cell, it's mixed content.
                        if !ts.cell_plain.is_empty() || ts.cell_sole_link.is_some() {
                            ts.cell_mixed = true;
                        }
                        ts.in_link = Some(dest_url.to_string());
                        // Open a Pango `<a href>` around the caption, exactly as
                        // `<b>`/`<i>` above wrap theirs — so a link composes with inline
                        // formatting and with the tight `==`/`~~`/`^`/`~` constructs
                        // `Event::Text` scans (Document Rendering CAM row 3), and a link
                        // inside a *mixed* cell is a real link and not inert text
                        // (row 2 — GTK4Rs/AP-239). A cell that turns out to be nothing but
                        // this link discards `cell_markup` for a `GtkLinkButton`, so the
                        // tag emitted here is simply unused in that case.
                        ts.cell_markup
                            .push_str(&crate::widgets::table::link_markup_open(&dest_url));
                    }
                } else {
                    self.link_start = Some((self.end_offset(), dest_url.to_string()));
                    self.inline_tags.push(crate::tags::TagName::Link);
                }
            }
            Tag::Image { dest_url, .. } => {
                // A Markdown image (`![alt](src)`): resolve its single src through the
                // safety gate, load a texture (native GdkTexture → gdk-pixbuf loader
                // fallback), and anchor the picture or a broken-image placeholder. The
                // exact same machinery serves a raw-HTML `<picture>`/`<img>` —
                // see `feed_html`/`render_image_slot` (ScrAP-147).
                let resolution = resolve_image(
                    dest_url.as_ref(),
                    self.doc_dir.as_deref(),
                    self.allow_unsafe_images,
                );
                let texture = load_texture(&resolution);
                // Decide what stands in for the image. If it loaded, the picture is
                // shown; otherwise a broken-image placeholder carries a reason in its
                // tooltip (blocked by policy / not found / failed to decode). Either way
                // the alt text is suppressed: the picture or the placeholder icon IS the
                // visual signal, so an unresolvable image never silently collapses to a
                // bare alt string (the "Show Unsafe Images left only alt text" report).
                let placeholder_tooltip =
                    image_placeholder_tooltip(&resolution, texture.is_some(), dest_url.as_ref());
                self.suppress_image_alt = texture.is_some() || placeholder_tooltip.is_some();
                if let Some(tex) = texture {
                    self.anchor_image(&tex);
                } else if let Some(tooltip) = placeholder_tooltip {
                    self.anchor_broken(&tooltip);
                }
            }
            Tag::HtmlBlock => {
                // Begin accumulating a raw HTML block. pulldown-cmark emits its body
                // line-by-line as `Event::Html` events between this and `TagEnd::HtmlBlock`
                // (events.rs appends them to `html_acc`); the block is fed through the
                // image scanner — rendering any `<picture>`/`<img>`, else dropped — when
                // it closes (end.rs → `feed_html`). See ScrAP-147.
                self.in_html_block = true;
                self.html_acc.clear();
            }

            // Inert BY OPTION — `normalize::md_options` enables neither FOOTNOTES,
            // DEFINITION_LIST nor either metadata block, so pulldown emits none of
            // these and their source arrives as literal text (ScrAP-78's visible
            // degradation). Spelled out rather than swept into a `_`, so enabling an
            // option without writing its handler stops compiling.
            Tag::FootnoteDefinition(_) => self.dropped_construct("a footnote definition"),
            Tag::DefinitionList | Tag::DefinitionListTitle | Tag::DefinitionListDefinition => {
                self.dropped_construct("a definition list");
            }
            Tag::MetadataBlock(_) => self.dropped_construct("a metadata block"),
        }
    }

    /// Feed a raw-HTML fragment (a whole accumulated block, or one inline tag)
    /// through the image scanner, replaying its [`ImgTag`](super::picture::ImgTag)
    /// stream against the `<picture>` grouping state carried on `self` ACROSS events.
    /// A `<picture>` collects its `<source>`/`<img>` candidates (first decodable wins
    /// at `</picture>`); each ungrouped `<img>`/`<source>` renders immediately as its
    /// own image. A fragment with no `<source>`/`<img>` renders NOTHING — all other
    /// raw HTML stays sanitized by omission (ScrAP-147 / TDD 2.23).
    ///
    /// The grouping MUST persist across events: a single-line `<picture>…</picture>`
    /// is not a CommonMark HTML block, so pulldown-cmark emits its tags as separate
    /// `Event::InlineHtml` events — grouping within one call would lose the fallback.
    /// `flush_open_picture` closes a still-open group at the end of its container.
    pub(super) fn feed_html_block(&mut self, html: &str) {
        self.feed_picture_html(html);
        self.feed_disclosure_html(html);
    }

    /// The INLINE half: an `Event::InlineHtml` carries a tag sitting mid-paragraph,
    /// and only the `<picture>` scanner may read it.
    ///
    /// **A disclosure is a BLOCK construct here, and declining it is a contract, not a
    /// simplification.** `disclosure::scan_document`'s pre-scan indexes block-HTML
    /// spans only, and the renderer checks each `<details>` it opens against that
    /// index by source offset — so an inline tag the renderer *accepted* would push a
    /// frame the pre-scan never counted, advance the shared cursor, and make every
    /// real disclosure below it fail the offset check and render unfoldable. Prose
    /// that merely mentions `<details>` mid-sentence is enough to trigger it. The two
    /// walks stay agreed by both declining an inline tag, and this function is the
    /// renderer's half of that agreement.
    ///
    /// Literal text is declined for the same reason it is emitted for a block: the
    /// rubric-2.26d case is a whole unspaced construct arriving as ONE raw-HTML block,
    /// which an inline event by definition is not.
    pub(super) fn feed_inline_html(&mut self, html: &str) {
        self.feed_picture_html(html);
    }

    fn feed_picture_html(&mut self, html: &str) {
        use super::picture::ImgTag;
        for tag in super::picture::scan_image_tags(html) {
            match tag {
                // A new `<picture>`: flush any still-open one first (malformed nesting).
                ImgTag::PictureOpen => {
                    self.flush_open_picture();
                    self.picture_open = Some(Vec::new());
                }
                ImgTag::PictureClose => self.flush_open_picture(),
                ImgTag::Candidate(src) => match &mut self.picture_open {
                    Some(group) => group.push(src),
                    None => self.render_image_slot(&[src]),
                },
            }
        }
    }

    /// Replay the disclosure tag stream against the renderer's `disclosure_stack`.
    ///
    /// Separate from the `<picture>` replay above rather than interleaved with it:
    /// the two constructs share a fragment but nothing else, and one loop handling
    /// both would make each one's state machine harder to read than either is alone.
    /// Both are driven from `feed_html` so a caller cannot reach one and miss the
    /// other.
    fn feed_disclosure_html(&mut self, html: &str) {
        use super::disclosure::DetailsTag;
        for tag in super::disclosure::scan_disclosure_tags(html) {
            match tag {
                DetailsTag::DetailsOpen { open } => {
                    // The block's identity is where its opening raw-HTML block begins
                    // in the SOURCE — stable across every re-render that does not
                    // change the text, which is exactly the set of events a reader
                    // expects a fold to survive (`crate::fold`).
                    let key = crate::fold::FoldKey::from_source_offset(self.event_src.start);
                    // A block the document never closes cannot fold — see
                    // `DisclosureFrame::foldable`. Asked FIRST, and unconditionally,
                    // because the answer comes from a cursor that must advance once
                    // per `<details>` however this block turns out.
                    let span_index = self.disclosure_cursor.seen();
                    let foldable = self.opening_details_is_closed(key.source_offset());
                    let collapsed = foldable && self.folds.is_collapsed(key, open);
                    self.disclosure_stack.push(super::DisclosureFrame {
                        key,
                        span_index,
                        foldable,
                        collapsed,
                        label: None,
                        in_summary: false,
                        emitted: false,
                        wrote_literal: false,
                        summary_offset: 0,
                        summary_end: 0,
                        body_start: 0,
                        label_end: 0,
                    });
                }
                DetailsTag::SummaryOpen => {
                    if let Some(frame) = self.disclosure_stack.last_mut() {
                        frame.in_summary = true;
                    }
                }
                DetailsTag::SummaryText(text) => {
                    if let Some(frame) = self.disclosure_stack.last_mut() {
                        frame.label = Some(text);
                    }
                }
                DetailsTag::SummaryClose => {
                    if let Some(frame) = self.disclosure_stack.last_mut() {
                        frame.in_summary = false;
                    }
                    self.emit_pending_summary();
                }
                // A literal-text run, in the frame it sits inside (rubric 2.26d, the
                // unspaced case). The summary line is written FIRST — it is what makes
                // `inside_collapsed_body` true, so writing the body before it would
                // put a closed block's body on the page under a toggle that then had
                // nothing to fold.
                DetailsTag::Text { text, .. } => {
                    self.emit_pending_summary();
                    // Recorded whatever the fold state: the extent this frame produces
                    // must say "not spliceable" in BOTH states, or the collapsed
                    // render would offer a splice the expanded one cannot fulfil.
                    if let Some(frame) = self.disclosure_stack.last_mut() {
                        frame.wrote_literal = true;
                    }
                    if !self.inside_collapsed_body() {
                        self.block_sep();
                        self.insert(&text);
                    }
                }
                // A stray `</details>` closes nothing rather than underflowing —
                // malformed input degrades predictably (rubric 2.26d).
                DetailsTag::DetailsClose => {
                    self.record_disclosure_extent();
                    self.disclosure_stack.pop();
                }
            }
        }
        // A `<details>` with no `<summary>` still gets its line, with the default
        // label — the affordance must exist wherever the construct does, or the
        // rendering is lossy with respect to the source (rubric 2.26d).
        self.emit_pending_summary();
    }

    /// Record where the disclosure now closing put its content, for
    /// [`super::DisclosureExtent`]'s consumers. Called at `</details>`, before the
    /// frame is popped, because the body's end is the renderer's position right now
    /// and nothing later can recover it.
    ///
    /// Two blocks get NO extent, and both omissions are deliberate. A block whose
    /// summary was never written (`emitted` false) drew nothing. A block nested
    /// inside a COLLAPSED ancestor also drew nothing — `emit_pending_summary`
    /// returns early for it, marking it emitted without writing a line — so the test
    /// is whether an ancestor is collapsed, not whether this frame is: a collapsed
    /// block that IS drawn earns an extent with an empty body, which is exactly the
    /// position a later expansion writes at.
    fn record_disclosure_extent(&mut self) {
        let Some(frame) = self.disclosure_stack.last() else {
            return;
        };
        if !frame.emitted {
            return;
        }
        let ancestors = &self.disclosure_stack[..self.disclosure_stack.len() - 1];
        if ancestors.iter().any(|f| f.collapsed && f.emitted) {
            return;
        }
        let (key, summary_offset, summary_end, body_start, label_end, wrote_literal) = (
            frame.key,
            frame.summary_offset,
            frame.summary_end,
            frame.body_start,
            frame.label_end,
            frame.wrote_literal,
        );
        let body_end = self.end_offset();
        self.disclosure_extents.push(super::DisclosureExtent {
            key,
            summary: crate::span::BufferSpan::new(summary_offset, summary_end),
            body: crate::span::BufferSpan::new(body_start, body_end),
            volatile: crate::span::BufferSpan::new(label_end, body_end),
            spliceable: !wrote_literal,
        });
    }

    /// Is the renderer currently inside a collapsed disclosure's BODY?
    ///
    /// True once a collapsed block's summary has been written and until its
    /// `</details>` pops the frame. An ancestor's collapse suppresses everything
    /// below it, which is why this asks about the whole stack rather than the top.
    ///
    /// Visible beyond the renderer because the buffer is not the only thing built
    /// from the event stream: the copy map, the per-cell copy maps, the source map
    /// and the heading index are built alongside it by `preview::build`, from the
    /// SAME events, and every one of them is wrong if it records an event the buffer
    /// never received. Asking the renderer is what keeps that one decision in one
    /// place — `preview::build` re-deriving "is this inside a collapsed body?" from
    /// the events would be a second implementation of the suppression rule, free to
    /// disagree with the first.
    pub(crate) fn inside_collapsed_body(&self) -> bool {
        self.disclosure_stack
            .iter()
            .any(|f| f.collapsed && f.emitted)
    }

    /// Write the innermost open disclosure's summary line, if it has not been written.
    ///
    /// The line is a real buffer line — an anchored toggle followed by the label as
    /// ordinary text — so find, selection and the `snapshot_layer` chrome all keep
    /// working across it. Only the affordance is a widget; nothing else about the
    /// disclosure leaves the buffer.
    fn emit_pending_summary(&mut self) {
        // A disclosure nested inside a COLLAPSED one is not rendered at all — not
        // even its summary. Checked before the frame is marked emitted so that the
        // inner block cannot leak a summary line into a body the reader has closed.
        if self.inside_collapsed_body() {
            if let Some(frame) = self.disclosure_stack.last_mut() {
                frame.emitted = true;
            }
            return;
        }
        let Some(frame) = self.disclosure_stack.last_mut() else {
            return;
        };
        if frame.emitted {
            return;
        }
        frame.emitted = true;
        let (key, span_index, expanded, foldable, label) = (
            frame.key,
            frame.span_index,
            !frame.collapsed,
            frame.foldable,
            frame
                .label
                .clone()
                .unwrap_or_else(|| DEFAULT_SUMMARY_LABEL.to_owned()),
        );
        // This block's BODY source range, resolved once and reused below — first by
        // the COLLAPSED preview (which needs it before the summary line ends), then
        // by the `CollapsedBlock` find-reach record (which needs it after). The
        // pre-scanned span `disclosure::scan_document` already produced; never
        // re-derived.
        let body_range = self
            .disclosures
            .get(span_index)
            .and_then(|span| span.body.clone());

        self.block_sep();
        let mut iter = self.tip();
        // Where the line starts, recorded on the frame before anything is written
        // into it: this is the offset every consumer that must address content
        // INSIDE the block resolves to, so it has to be the anchor's own position
        // rather than anywhere in the block separator ahead of it.
        let summary_offset = iter.offset();
        if let Some(frame) = self.disclosure_stack.last_mut() {
            frame.summary_offset = summary_offset;
        }
        // An UNCLOSED block gets its label and no toggle. The label is authored
        // content and stays; the control does not, because there is no body for it
        // to fold — the block has no end, so "collapse" would mean hiding the rest
        // of the document (`DisclosureFrame::foldable`, rubric 2.26d).
        if foldable {
            let anchor = self.buf.create_child_anchor(&mut iter);
            let toggle = crate::widgets::disclosure::build(expanded, self.zoom, &label);
            // Handed back paired with its fold so the preview layer can wire
            // activation without re-deriving which block a widget belongs to.
            self.disclosure_toggles.push(super::DisclosureToggle {
                toggle: toggle.clone(),
                key,
                summary_offset,
            });
            self.push_anchored(anchor, toggle.upcast());
            self.insert(&format!(" {label}"));
            // Before the preview below: the label renders the same under either fold
            // state, the preview does not, so this is where the two begin to diverge.
            let label_end = self.end_offset();
            if let Some(frame) = self.disclosure_stack.last_mut() {
                frame.label_end = label_end;
            }
            // A short preview of the body's OPENING text, dimmed by the active
            // reading theme (TDD 2.26) — collapsed blocks only: an expanded block
            // shows its body directly, so it needs no hint of what the body holds.
            //
            // These are real buffer characters, appended to the SAME summary line,
            // still inside the ONE `End(HtmlBlock)` event this whole `<details>`
            // opening fragment is processed under — which is exactly the buffer
            // range `preview::build`'s `(Some(site), None)` arm widens at
            // `</details>` to cover the block's whole source. So a copy across this
            // block still reconstructs its Markdown; this text is never part of it.
            //
            // Derived from `disclosure::preview_insert_text`, which reuses
            // `body_plain_text` — the SAME reduction find already applies to a
            // collapsed body (rubric 5): never the raw Markdown, or an emphasised
            // word would show its own `*`.
            if !expanded {
                let insert =
                    super::disclosure::preview_insert_text(&self.cleaned, body_range.clone());
                if let Some(insert) = insert {
                    let preview_start = self.end_offset();
                    self.insert(&insert);
                    let si = self.buf.iter_at_offset(preview_start);
                    let ei = self.tip();
                    self.apply(crate::tags::TagName::DisclosurePreview, &si, &ei);
                }
            }
        } else {
            self.insert(&label);
            let label_end = self.end_offset();
            if let Some(frame) = self.disclosure_stack.last_mut() {
                frame.label_end = label_end;
            }
        }
        // Recorded on either side of the newline, because the two answer different
        // questions: a line-wide decoration paints over the summary's TEXT, and a
        // splice writes the body AFTER the line terminator.
        let summary_end = self.end_offset();
        // The summary line's own INK (TDD 18.49), over the same extent the drawn band
        // fills — the label AND the collapsed preview, because both sit on the band
        // and both have to stay legible on it. Applied whatever the theme says: the
        // tag sets no foreground at all unless `disclosure_fg` is stated, so a theme
        // that states none re-inks nothing (TDD 18.2) — the same discipline the quote
        // panel's ink follows in `end.rs`. `disclosure-preview` is registered AFTER
        // this tag, so a theme stating both still dims the preview fragment.
        //
        // **Gated on `foldable`, which is what keeps the ink and the BAND agreeing.**
        // A block the document never closes gets a label and no toggle (rubric 2.26d),
        // and — because `record_disclosure_extent` runs at `</details>` and that event
        // never arrives — no `DisclosureExtent`, so no span reaches the drawn band
        // either. Inking a line the band cannot reach would put a themed foreground on
        // the one summary that is not drawn as a summary; the two are one decoration
        // and are absent together. `foldable` is exactly "this block is closed", so it
        // is the same fact both sides read rather than two conditions that can drift.
        if foldable && summary_end > summary_offset {
            let si = self.buf.iter_at_offset(summary_offset);
            let ei = self.buf.iter_at_offset(summary_end);
            self.apply(crate::tags::TagName::DisclosureInk, &si, &ei);
        }
        self.newline();
        let body_start = self.end_offset();
        if let Some(frame) = self.disclosure_stack.last_mut() {
            frame.summary_end = summary_end;
            frame.body_start = body_start;
        }
        // A block drawn COLLAPSED is announced with the source range it withheld, so
        // the consumers that must reach inside it — find above all — have something
        // to search. Recorded here rather than at `</details>`, because this is the
        // one place that knows the summary line was actually written: a disclosure
        // nested in a collapsed one returns above without reaching this.
        if !expanded {
            if let Some(body) = body_range {
                self.collapsed_blocks.push(super::CollapsedBlock {
                    summary_offset,
                    key,
                    body,
                });
            }
        }
    }

    /// Render a still-open `<picture>` group (its collected candidates) and clear the
    /// state — called at `</picture>` and at the end of the container the group lives
    /// in (`TagEnd::HtmlBlock`/`TagEnd::Paragraph`) so an unclosed `<picture>` can't
    /// swallow later content. A no-op when no group is open.
    pub(super) fn flush_open_picture(&mut self) {
        if let Some(candidates) = self.picture_open.take() {
            if !candidates.is_empty() {
                self.render_image_slot(&candidates);
            }
        }
    }

    /// Render one image slot: the FIRST candidate that decodes wins (reusing the
    /// Markdown-image path); if none decode, a broken-image marker stands in for the
    /// last (most meaningful — the `<img>` fallback) candidate. Every candidate goes
    /// through the SAME `resolve_image` safety gate as a Markdown image, so a
    /// remote/escaping/other-scheme src is Refused unless "Show Unsafe Images" —
    /// `<picture>`/`<img>` widens what renders, never what is trusted.
    fn render_image_slot(&mut self, candidates: &[String]) {
        for src in candidates {
            let resolution = resolve_image(src, self.doc_dir.as_deref(), self.allow_unsafe_images);
            if let Some(tex) = load_texture(&resolution) {
                self.anchor_image(&tex);
                return;
            }
        }
        // Total rather than `expect("a slot is never empty")` (QA round 3, P-7).
        // The invariant does hold today — both call sites check `!is_empty()`
        // first — but it is enforced 30 lines away in two places, and this runs
        // in the render walk where a panic is a PROCESS ABORT, not an error.
        // An empty slot means there was nothing to draw, so returning is also
        // the right answer on its own terms.
        let Some(fallback) = candidates.last() else {
            return;
        };
        let resolution = resolve_image(fallback, self.doc_dir.as_deref(), self.allow_unsafe_images);
        if let Some(tooltip) = image_placeholder_tooltip(&resolution, false, fallback) {
            self.anchor_broken(&tooltip);
        }
    }

    /// Anchor a loaded texture as a `GtkPicture` (with the selection-tint overlay)
    /// in the buffer — the shared build for a Markdown image and a `<picture>`.
    fn anchor_image(&mut self, tex: &gtk::gdk::Texture) {
        self.block_sep();
        let mut iter = self.tip();
        let anchor = self.buf.create_child_anchor(&mut iter);
        // GTK4Rs/AP-58 (researcher-sourced, 4.6 source-verified): an anchored GtkPicture
        // defaults to `can_shrink` → `min_width` 0, so the GtkTextView measures its
        // HEIGHT at for-width 0, which gtk_picture_measure short-circuits to height 0 →
        // the picture blanks (the rest of the doc renders). A definite size_request
        // (w AND h) lifts the minimum off 0 so it paints.
        //
        // Display policy (`max-width: 100%`): the image is shown at min(natural, pane)
        // — scaled down to fit a narrow pane, never upscaled past its own resolution.
        // The NATURAL size is registered in `image_bounded`;
        // `CodePreviewView::size_allocate` clamps it to the live column each width
        // change (aspect preserved), so a too-wide image fits instead of forcing an
        // over-wide line → GTK4Rs/AP-22/23 blank. The initial `set_size_request` is only a
        // first-frame SEED: it must be nonzero (GTK4Rs/AP-58) but not absurdly over-wide, or
        // the pre-allocate frame flashes the GTK4Rs/AP-22/23 transient; size_allocate then
        // scales it up (to the pane) or down (to fit) as needed.
        let (nat_w, nat_h) = (tex.width().max(1), tex.height().max(1));
        const INIT_SEED_W: i32 = 640;
        let seed_w = nat_w.min(INIT_SEED_W);
        let seed_h = (i64::from(nat_h) * i64::from(seed_w) / i64::from(nat_w)).max(1) as i32;
        let pic = gtk::Picture::for_paintable(tex);
        pic.set_halign(gtk::Align::Start);
        pic.set_size_request(seed_w, seed_h);
        pic.set_can_shrink(true);
        self.image_bounded
            .push((pic.clone().upcast(), nat_w, nat_h));
        // Wrap in an overlay so a selection tint can be drawn OVER the image when it
        // falls inside the buffer selection — the GtkTextView highlights surrounding
        // text but never an anchored widget. The tint is a click-through box (toggled
        // by the preview's connect_image_tints); the overlay's size is the picture's
        // (constant), so it still paints.
        let tint = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        tint.add_css_class("scrib-image-sel");
        tint.set_can_target(false);
        tint.set_visible(false);
        let overlay = gtk::Overlay::new();
        overlay.set_halign(gtk::Align::Start);
        overlay.set_child(Some(&pic));
        overlay.add_overlay(&tint);
        self.push_anchored(anchor.clone(), overlay.upcast());
        self.image_tints.push((anchor, tint.upcast()));
        self.trailing_newlines = 0;
        self.at_start = false;
    }

    /// Anchor a broken-image placeholder icon with `tooltip` — shown for any image
    /// (Markdown or `<picture>`) that is blocked / not found / undecodable, so the
    /// reader always sees that an image was expected and why it isn't there.
    fn anchor_broken(&mut self, tooltip: &str) {
        // GtkImage with "image-missing" is a GTK built-in fallback icon available on
        // every GTK4 installation; set_pixel_size sets both its natural width and
        // height (no GTK4Rs/AP-58 issue — GtkImage reports a definite size, unlike GtkPicture
        // with can_shrink).
        self.block_sep();
        let mut iter = self.tip();
        let anchor = self.buf.create_child_anchor(&mut iter);
        let icon = gtk::Image::from_icon_name(crate::icons::Icon::ImageMissing.name());
        icon.set_pixel_size(32);
        icon.set_halign(gtk::Align::Start);
        // TDD 18.20: reachable by the generated theme sheet (`preview/css.rs`), where
        // it was previously invisible to mechanism C — no class meant no theme could
        // reach it, so it sat on desktop colours under every reading theme.
        icon.add_css_class("scrib-broken-image");
        crate::a11y::name(&icon, tooltip);
        self.push_anchored(anchor, icon.upcast());
        self.trailing_newlines = 0;
        self.at_start = false;
    }
}

/// Load a `GdkTexture` for a resolved image. `GdkTexture::from_file` already tries
/// its native loaders (PNG/JPEG/TIFF), then falls back to gdk-pixbuf's INSTALLED
/// runtime loaders (`gdk_texture_new_from_bytes` → `gdk_pixbuf_new_from_stream`,
/// verified in GTK 4.6.9), so WebP/AVIF/etc. render iff the user has the loader AND
/// it is registered in the process's `loaders.cache` — no manual `Pixbuf` fallback
/// is needed. Deliberately NOT re-tried through `Pixbuf::from_file`: that is *less*
/// capable than `from_file` here — it errors "Cannot create WebP decoder" on an
/// animated WebP that `Texture::from_file` (via the stream path) decodes fine (a
/// format failing despite an installed loader is a REGISTRATION problem, cf.
/// ScrAP-146 / GTK4Rs/AP-66, not a `GdkTexture` limitation). `Refused`/
/// `Missing` never load. Remote fetches block the main thread for the request
/// (accepted for the opt-in "Show Unsafe Images" path, ScrAP-34, its 34a half).
///
/// **A remote image is fetched by [`crate::imagefetch`], not by GIO** — a
/// `gio::File::for_uri("https://…")` needs a GVfs backend that claims the scheme,
/// which exists on the Linux desktop and nowhere else, so that route rendered
/// nothing at all on macOS (ScrAP-292). The bytes then go through
/// `Texture::from_bytes`, which reaches the same loader chain `from_file` would
/// have, so decoding is unchanged.
pub(super) fn load_texture(resolution: &ImageResolution) -> Option<gtk::gdk::Texture> {
    match resolution {
        ImageResolution::Local(path) => {
            let file = gtk::gio::File::for_path(path);
            match gtk::gdk::Texture::from_file(&file) {
                Ok(texture) => Some(texture),
                Err(err) => {
                    log::warn!("image not loaded: {} ({err})", path.display());
                    None
                }
            }
        }
        ImageResolution::Remote(uri) => load_remote_texture(uri),
        ImageResolution::Refused | ImageResolution::Missing => None,
    }
}

/// Fetch and decode a remote image, logging why it did not appear.
///
/// Split from [`load_texture`] because it has two distinct failure stages — the
/// fetch and the decode — and collapsing them into one `.ok()` is what made the
/// GVfs gap above invisible for as long as it was: the placeholder tooltip said
/// "Could not load image", which reads as *the bytes were not an image* when in
/// fact no request had been made (ScrAP-292).
///
/// **Routed through [`crate::imagecache`].** A disclosure fold-toggle re-renders
/// its document into a scratch buffer to rebuild its offset maps, which walks
/// every image tag again — without a cache every toggle would re-run the fetch
/// below for every remote image in the document, freezing the UI each time.
/// `imagecache::get_or_fetch` calls this closure only on an outright cache miss;
/// a hit or a live cached failure returns with no network access at all.
fn load_remote_texture(uri: &str) -> Option<gtk::gdk::Texture> {
    crate::imagecache::get_or_fetch(uri, || {
        let bytes = match crate::imagefetch::fetch_image_bytes(uri) {
            Ok(bytes) => bytes,
            Err(err) => {
                log::warn!("remote image not fetched: {uri} ({err})");
                return None;
            }
        };
        match gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_owned(bytes)) {
            Ok(texture) => Some(texture),
            Err(err) => {
                log::warn!("remote image fetched but not decoded: {uri} ({err})");
                None
            }
        }
    })
}
