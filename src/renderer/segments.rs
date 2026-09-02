//! Block-scope segmentation of the four tight constructs — the layer that lets a
//! `~~ … ~~` or `== … ==` fence WRAP other inline markup.
//!
//! [`super::scan::scan_script_spans`] defines what `^sup^`, `~sub~`,
//! `~~strike~~` and `==mark==` *are*. This module decides *where* they are in a
//! document whose inline markup pulldown-cmark has already fragmented into
//! separate events, and it exists because scanning one `Text` event at a time
//! cannot answer that: pulldown splits `~~a **bold** b~~` into `"~~a "`,
//! `Strong("bold")`, `" b~~"`, so the two halves of the fence never meet in one
//! scanned run and the `~~` render literally (ScrAP-66 explains why enabling
//! pulldown's own tilde extensions is not the escape — it fragments *more*).
//!
//! ## How it works
//!
//! One pre-pass over the same event stream every consumer walks. Within a block
//! (paragraph, heading, table cell, list item — anything not bounded by an
//! *inline* tag) the rendered text of each `Text` event is **stitched** into one
//! string, and [`super::scan::scan_script_spans`] runs over that. A construct is
//! then cut back up into per-event [`Seg`]s, so a consumer that still walks
//! events one at a time gets, for its event, exactly which bytes are delimiters
//! to drop and which are content carrying which [`Script`].
//!
//! Stitching **rendered** text rather than source is deliberate. The scanner's
//! existing callers all hold rendered text, entities and smart punctuation make
//! rendered ≠ source, and a source-driven scan would have to re-derive that
//! mapping at every call site. Stitching also excludes what must not be scanned
//! for free: an inline code span, an image's alt text and raw HTML are not
//! `Text` events, so their contents can never contribute a delimiter, and a
//! link's destination never appears at all.
//!
//! ## Well-formedness: a fence that INTERLEAVES is rejected
//!
//! `~~a **b~~ c**` closes its fence inside a `Strong` that opened inside the
//! fence. That is not a tree, and every consumer here builds one — `copymap`'s
//! node tree most sharply. Such a span is refused and its markers stay literal,
//! which is the pre-existing behaviour rather than a new failure mode. Proper
//! nesting in either direction (the fence inside the markup, or the markup
//! inside the fence) is accepted.

use super::scan::{scan_script_spans, Script};
use pulldown_cmark::{Event, Parser, Tag};
use std::collections::HashMap;
use std::ops::Range;

/// One segment of a single `Text` event's rendered text.
///
/// `range` indexes **that event's own rendered string**, so a consumer slices its
/// own `t` and never needs the document. The segments of one event partition it
/// completely and in order — including the `marker` ones — which is what lets
/// `copymap` reconstruct the event's source by concatenating them all while the
/// renderer emits only the rest.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Seg {
    pub(crate) range: Range<usize>,
    pub(crate) script: Script,
    /// A delimiter (`~~`, `==`, `^`, `~`): part of the source, never rendered.
    pub(crate) marker: bool,
}

impl Seg {
    /// This segment's slice of the event's rendered text.
    pub(crate) fn text<'a>(&self, rendered: &'a str) -> &'a str {
        rendered.get(self.range.clone()).unwrap_or_default()
    }
}

/// Every tight construct in one document, indexed by the `Text` event that each
/// piece of it belongs to.
///
/// Built once per parse of a given text and consulted per event, so the renderer,
/// the copymap, the outline and the export walk all segment a run identically —
/// the same single-definition discipline `scan_script_spans` already carries, one
/// level up.
#[derive(Clone, Debug, Default)]
pub(crate) struct BlockScripts {
    /// Keyed by the `Text` event's source byte start, which is unique per parse.
    by_event: HashMap<usize, Vec<Seg>>,
    /// Whole-construct source ranges (delimiters included), ascending and
    /// non-overlapping — what an annotation must never be allowed to split.
    outers: Vec<Range<usize>>,
}

/// A `Text` event's contribution to the stitched block text.
struct Chunk {
    /// Offset of this event's text within the stitched string.
    stitch: usize,
    /// Byte length of the event's rendered text.
    len: usize,
    /// The event's source byte start — the key it is later looked up by.
    src_start: usize,
}

/// A cut the stitched scan produced: one delimiter or one content run.
struct Cut {
    stitch: Range<usize>,
    script: Script,
    marker: bool,
}

/// Stands in for a construct that owns rendered width but whose text must not be
/// scanned — an inline code span, an image, raw HTML. Non-whitespace (so it does
/// not break a tight `^x^`/`~x~` the way a real space would) and never a
/// delimiter character.
const OPAQUE_FILLER: char = '\u{FFFC}';

/// Whether `tag` is an *inline* construct — one that may sit inside a tight
/// fence rather than bounding it.
///
/// The single definition of the inline/block split for both this module's block
/// flushing and `export::walk`'s implicit-paragraph handling; two copies of it
/// would let a fence span a boundary the exporter treats as a block, or the
/// reverse.
pub(crate) fn is_inline_tag(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Superscript
            | Tag::Subscript
            | Tag::Link { .. }
            | Tag::Image { .. }
    )
}

impl BlockScripts {
    /// Scan `md` — the same text, with the same [`super::md_options`], that the
    /// caller is about to walk.
    pub(crate) fn scan(md: &str) -> Self {
        let mut out = BlockScripts::default();
        let mut block = BlockAcc::default();
        // An image's alt text is a `Text` event that the preview never renders,
        // so it must not contribute delimiters to its block.
        let mut image_depth = 0usize;

        for (ev, src) in Parser::new_ext(md, super::md_options()).into_offset_iter() {
            match &ev {
                Event::Start(tag) => {
                    if is_inline_tag(tag) {
                        block.inline.push(src.clone());
                        if matches!(tag, Tag::Image { .. }) {
                            image_depth += 1;
                            block.push_filler();
                        }
                    } else {
                        block.flush(&mut out);
                    }
                }
                Event::End(end) => {
                    if matches!(
                        end,
                        pulldown_cmark::TagEnd::Emphasis
                            | pulldown_cmark::TagEnd::Strong
                            | pulldown_cmark::TagEnd::Strikethrough
                            | pulldown_cmark::TagEnd::Superscript
                            | pulldown_cmark::TagEnd::Subscript
                            | pulldown_cmark::TagEnd::Link
                            | pulldown_cmark::TagEnd::Image
                    ) {
                        if matches!(end, pulldown_cmark::TagEnd::Image) {
                            image_depth = image_depth.saturating_sub(1);
                        }
                    } else {
                        block.flush(&mut out);
                    }
                }
                Event::Text(t) => {
                    if image_depth > 0 {
                        // Alt text: rendered by nothing, scanned by nothing.
                    } else {
                        block.push_text(src.start, t);
                    }
                }
                // Rendered width with unscannable contents. `DisplayMath` belongs here
                // beside `InlineMath` on the merits, NOT because it is reachable: it is
                // inert today only because `md_options()` does not enable `ENABLE_MATH`,
                // which is inertness by option rather than by design — the shape this
                // check exists to catch. Naming it costs one token and stops the day the
                // option is enabled from being the day this scanner silently mis-stitches.
                Event::Code(_)
                | Event::InlineHtml(_)
                | Event::InlineMath(_)
                | Event::DisplayMath(_) => block.push_filler(),
                // A line break is real whitespace: it must be able to close a
                // tight `^x^`/`~x~` exactly as a space does, while still leaving a
                // `~~`/`==` fence (whose content may contain spaces) able to span it.
                Event::SoftBreak | Event::HardBreak => block.push_break(),
                // Named rather than wildcarded, each because it contributes no
                // stitchable text — which is a DECISION about the variant, not an
                // absence of one. `Rule` and `TaskListMarker` carry no text at all;
                // `Html` is block HTML, which per this module's doc never arrives as a
                // `Text` event; `FootnoteReference` carries a label the scanner has no
                // reason to search for a delimiter.
                Event::Rule
                | Event::TaskListMarker(_)
                | Event::Html(_)
                | Event::FootnoteReference(_) => {}
            }
        }
        block.flush(&mut out);
        out.outers.sort_by_key(|r| r.start);
        out
    }

    /// The segments of the `Text` event whose source begins at `src_start`.
    ///
    /// Total with a safe fallback (POLICY § Typed GTK seams): an event this table
    /// does not know — one produced by a parse of different text — degrades to the
    /// standalone, single-run segmentation, which is what every caller did before
    /// this module existed.
    pub(crate) fn segments(&self, src_start: usize, rendered: &str) -> Vec<Seg> {
        match self.by_event.get(&src_start) {
            Some(segs) => segs.clone(),
            None => segments_of(rendered),
        }
    }

    /// Append the RENDERED text of one `Event::Text` at source offset `src_start`,
    /// dropping the tight-construct delimiters the page never shows.
    ///
    /// The one implementation of "what this event's text WOULD say on the page", shared
    /// by every reduction that has to agree with the rendered document: the outline's
    /// heading labels and slugs, and the searchable text of a collapsed disclosure body.
    /// It exists because those two had it written out twice and the second copy omitted
    /// the marker suppression entirely — so a `^superscript^` inside a collapsed block
    /// contributed its `^` delimiters to the text find searched, which both invents
    /// matches on a character the reader cannot see and misses a search for the word as
    /// rendered.
    pub(crate) fn push_rendered_text(&self, src_start: usize, rendered: &str, out: &mut String) {
        for seg in self.segments(src_start, rendered) {
            if !seg.marker {
                out.push_str(seg.text(rendered));
            }
        }
    }

    /// Whole-construct source ranges, ascending. An annotation's wrap span must
    /// contain each of these whole or not at all.
    pub(crate) fn outers(&self) -> &[Range<usize>] {
        &self.outers
    }
}

/// Segment a single run with no block context — the constructs wholly inside it.
///
/// The degenerate case of [`BlockScripts::scan`] (one chunk, no neighbours), kept
/// as the fallback and as the unit under test for the tokenizer's own rules.
pub(crate) fn segments_of(text: &str) -> Vec<Seg> {
    let cuts: Vec<Cut> = scan_script_spans(text)
        .into_iter()
        .flat_map(|s| {
            [
                Cut {
                    stitch: s.outer.start..s.inner.start,
                    script: s.script,
                    marker: true,
                },
                Cut {
                    stitch: s.inner.clone(),
                    script: s.script,
                    marker: false,
                },
                Cut {
                    stitch: s.inner.end..s.outer.end,
                    script: s.script,
                    marker: true,
                },
            ]
        })
        .collect();
    cut_chunk(&cuts, 0, text.len())
}

/// One block's accumulated stitch, reset at every block boundary.
#[derive(Default)]
struct BlockAcc {
    stitched: String,
    chunks: Vec<Chunk>,
    /// Source ranges of the inline pulldown constructs opened in this block —
    /// what a candidate span must nest cleanly with.
    inline: Vec<Range<usize>>,
}

impl BlockAcc {
    fn push_text(&mut self, src_start: usize, t: &str) {
        self.chunks.push(Chunk {
            stitch: self.stitched.len(),
            len: t.len(),
            src_start,
        });
        self.stitched.push_str(t);
    }

    fn push_filler(&mut self) {
        self.stitched.push(OPAQUE_FILLER);
    }

    fn push_break(&mut self) {
        self.stitched.push('\n');
    }

    /// Scan what has accumulated, record its segments and constructs, and reset.
    fn flush(&mut self, out: &mut BlockScripts) {
        if self.chunks.is_empty() {
            self.reset();
            return;
        }
        let mut cuts: Vec<Cut> = Vec::new();
        for span in scan_script_spans(&self.stitched) {
            let (Some(lo), Some(hi)) = (
                self.source_at(span.outer.start),
                self.source_end_at(span.outer.end),
            ) else {
                continue;
            };
            if !self.nests_cleanly(lo..hi) || !self.markers_abut_content(&span) {
                continue;
            }
            out.outers.push(lo..hi);
            cuts.push(Cut {
                stitch: span.outer.start..span.inner.start,
                script: span.script,
                marker: true,
            });
            cuts.push(Cut {
                stitch: span.inner.clone(),
                script: span.script,
                marker: false,
            });
            cuts.push(Cut {
                stitch: span.inner.end..span.outer.end,
                script: span.script,
                marker: true,
            });
        }
        for chunk in &self.chunks {
            let segs = cut_chunk(&cuts, chunk.stitch, chunk.len);
            out.by_event.insert(chunk.src_start, segs);
        }
        self.reset();
    }

    fn reset(&mut self) {
        self.stitched.clear();
        self.chunks.clear();
        self.inline.clear();
    }

    /// The source offset of stitched byte `pos`, or `None` when it lands in
    /// filler (which no delimiter ever can).
    fn source_at(&self, pos: usize) -> Option<usize> {
        let c = self.chunk_at(pos)?;
        Some(c.src_start + (pos - c.stitch))
    }

    /// The source offset one past stitched byte `end - 1` — the exclusive end of
    /// a range whose last byte must itself lie in a chunk.
    fn source_end_at(&self, end: usize) -> Option<usize> {
        self.source_at(end.checked_sub(1)?).map(|p| p + 1)
    }

    fn chunk_at(&self, pos: usize) -> Option<&Chunk> {
        let i = self.chunks.partition_point(|c| c.stitch <= pos);
        let c = self.chunks.get(i.checked_sub(1)?)?;
        (pos < c.stitch + c.len).then_some(c)
    }

    /// Whether each delimiter shares its `Text` event with the content it
    /// delimits.
    ///
    /// `` ~~`code` x~~ `` opens its fence in an event that renders nothing else, so
    /// the `~~` would belong to no run and `copymap` would have no node to hang its
    /// source on — the delimiter would then vanish from a copy. Such a fence is
    /// refused and stays literal, which is what it already did.
    fn markers_abut_content(&self, span: &super::scan::ScriptSpan) -> bool {
        let same = |a: usize, b: usize| match (self.chunk_at(a), self.chunk_at(b)) {
            (Some(x), Some(y)) => x.stitch == y.stitch,
            _ => false,
        };
        span.inner.end > span.inner.start
            && same(span.outer.start, span.inner.start)
            && same(span.inner.end - 1, span.outer.end - 1)
    }

    /// Whether `span` is properly nested with every inline construct in the block
    /// — contained by it, containing it, or disjoint from it, but never
    /// straddling its boundary (the module header's interleaving rule).
    fn nests_cleanly(&self, span: Range<usize>) -> bool {
        self.inline.iter().all(|r| {
            let disjoint = r.end <= span.start || span.end <= r.start;
            let inside = span.start >= r.start && span.end <= r.end;
            let outside = r.start >= span.start && r.end <= span.end;
            disjoint || inside || outside
        })
    }
}

/// Cut `[base, base + len)` of the stitched text into segments, using the
/// construct intervals that overlap it. Positions are rebased to the chunk, so
/// the result indexes the event's own rendered text.
fn cut_chunk(cuts: &[Cut], base: usize, len: usize) -> Vec<Seg> {
    let end = base + len;
    let mut segs: Vec<Seg> = Vec::new();
    let mut at = base;
    for cut in cuts {
        if cut.stitch.end <= at || cut.stitch.start >= end || cut.stitch.is_empty() {
            continue;
        }
        let lo = cut.stitch.start.max(at);
        let hi = cut.stitch.end.min(end);
        if lo > at {
            segs.push(Seg {
                range: at - base..lo - base,
                script: Script::None,
                marker: false,
            });
        }
        segs.push(Seg {
            range: lo - base..hi - base,
            script: cut.script,
            marker: cut.marker,
        });
        at = hi;
    }
    if at < end {
        segs.push(Seg {
            range: at - base..end - base,
            script: Script::None,
            marker: false,
        });
    }
    segs
}

#[cfg(test)]
mod tests;
