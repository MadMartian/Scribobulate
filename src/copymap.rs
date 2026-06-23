//! Character-precise copy-as-Markdown for the preview pane (TDD 2.8).
//! A **pure**, unit-tested resolver over a buffer-annotated construct
//! tree built in the render loop, where the *real* preview buffer offsets are
//! known (they cannot be re-derived from source alone — the renderer strips
//! syntax and synthesises content, ScrAP-5/ScrAP-23).
//!
//! # The one rule (the uniform resolver model)
//!
//! Reconstruct source for a buffer selection `[a, b)` by walking the tree. For a
//! construct with **content** buffer-range `[c0, c1)`:
//! - **Leaves** (literal text runs) contribute source **character-precise**,
//!   never any delimiter bytes.
//! - **Constructs** emit `open_delim + inner + close_delim` **iff** a boundary is
//!   crossed (`a < c0 || b > c1`); otherwise just `inner`.
//! - Delimiters are **always reconstructed whole from source**, so output is
//!   always balanced (the artificial completion of constraint C falls out).
//! - A **whole-buffer** selection returns the entire source (= Copy Document,
//!   constraint B) — handled in [`resolve`].
//!
//! A, B, C and D (the operator's copy constraints) are facets of this single rule.
//!
//! # Staleness safety
//!
//! The [`CopyTree`] is a **pure function of the rendered source**, rebuilt
//! wholesale in the same pass that fills the preview buffer
//! (`preview::build_render_products`). It is never incrementally mutated to track
//! edits, so buffer and tree are always mutually consistent — the same contract
//! the existing `source_map` honours (the copymap is purely additive to it).

use crate::limits;
use crate::renderer::{scan_scripts, Script};
use pulldown_cmark::{Event, Tag, TagEnd};
use std::ops::Range;

/// The Markdown constructs the copy resolver distinguishes. Table/cell internals
/// are deliberately absent — a table is [`opaque`](Node::Opaque) and its inner
/// events are skipped wholesale.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Construct {
    Emphasis,
    Strong,
    Link,
    Heading,
    Paragraph,
    BlockQuote,
    List,
    Item,
    CodeBlock,
    Table,
    Image,
}

/// One render event, classified for the copymap and carrying the two facts only
/// the live render knows: the **buffer** char range it produced and its
/// **source** byte range.
#[derive(Clone, Debug)]
pub(crate) enum RawKind {
    Start(Construct),
    End(Construct),
    /// A literal text run — the *rendered* (post-`scan_scripts`, un-escaped) text.
    Text(String),
    /// An inline code span — the *rendered* code text (backticks stripped).
    Code(String),
    /// A soft/hard line break (one buffer `\n`).
    Break,
    /// An opaque single unit that owns buffer glyphs but no reconstructable
    /// interior: a horizontal rule, a task-list checkbox.
    Atomic,
}

/// A classified render event with its live buffer + source coordinates.
#[derive(Clone, Debug)]
pub(crate) struct RawEv {
    /// Buffer char range `[before, after)` the event's processing produced.
    pub buf: (i32, i32),
    /// Source byte range of the event (from pulldown's offset iterator).
    pub src: Range<usize>,
    pub kind: RawKind,
}

/// Classify a pulldown event for the copymap, or `None` to ignore it (events
/// that produce no buffer text in this renderer, or table-internal structure
/// that an opaque `Table` skips). The caller pairs this with the live buffer
/// range and source range to build a [`RawEv`].
pub(crate) fn classify(ev: &Event) -> Option<RawKind> {
    Some(match ev {
        Event::Start(tag) => RawKind::Start(construct_of_tag(tag)?),
        Event::End(end) => RawKind::End(construct_of_tagend(*end)?),
        Event::Text(t) => RawKind::Text(t.to_string()),
        Event::Code(t) => RawKind::Code(t.to_string()),
        Event::SoftBreak | Event::HardBreak => RawKind::Break,
        Event::Rule | Event::TaskListMarker(_) => RawKind::Atomic,
        _ => return None,
    })
}

fn construct_of_tag(tag: &Tag) -> Option<Construct> {
    Some(match tag {
        Tag::Emphasis => Construct::Emphasis,
        Tag::Strong => Construct::Strong,
        Tag::Link { .. } => Construct::Link,
        Tag::Heading { .. } => Construct::Heading,
        Tag::Paragraph => Construct::Paragraph,
        Tag::BlockQuote(_) => Construct::BlockQuote,
        Tag::List(_) => Construct::List,
        Tag::Item => Construct::Item,
        Tag::CodeBlock(_) => Construct::CodeBlock,
        Tag::Table(_) => Construct::Table,
        Tag::Image { .. } => Construct::Image,
        // TableHead/Row/Cell and any strike/script tags (disabled in md_options)
        // are never independently reconstructed.
        _ => return None,
    })
}

fn construct_of_tagend(end: TagEnd) -> Option<Construct> {
    Some(match end {
        TagEnd::Emphasis => Construct::Emphasis,
        TagEnd::Strong => Construct::Strong,
        TagEnd::Link => Construct::Link,
        TagEnd::Heading(_) => Construct::Heading,
        TagEnd::Paragraph => Construct::Paragraph,
        TagEnd::BlockQuote(_) => Construct::BlockQuote,
        TagEnd::List(_) => Construct::List,
        TagEnd::Item => Construct::Item,
        TagEnd::CodeBlock => Construct::CodeBlock,
        TagEnd::Table => Construct::Table,
        TagEnd::Image => Construct::Image,
        _ => return None,
    })
}

// ── the tree ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum Node {
    /// A literal run. `src` is the *content* source (no delimiters); when
    /// `one_to_one`, buffer chars align 1:1 with `src`'s chars and a partial
    /// selection is interpolated char-precisely. When not (escapes, entities,
    /// smart-punctuation, super/sub runs that aren't 1:1), any overlap copies the
    /// whole run source — the atomicity guarantee (no half-token).
    Leaf {
        buf: (i32, i32),
        src: Range<usize>,
        one_to_one: bool,
    },
    /// An opaque construct (image, table, or code block): any buffer overlap
    /// copies its whole source.
    Opaque { buf: (i32, i32), src: Range<usize> },
    /// A paired/leading-marker construct reconstructed from source delimiters,
    /// or a delimiter-free container (document root, list, paragraph).
    Branch {
        /// Content buffer range `[c0, c1)`.
        buf: (i32, i32),
        /// Source range of the open delimiter (empty for a container).
        open: Range<usize>,
        /// Source range of the close delimiter (empty for a container).
        close: Range<usize>,
        /// A **leading-marker block** (heading, blockquote, list item): its
        /// per-line markers (`> `, the list continuation indent, …) live in the
        /// inter-sibling source gaps and must be **suppressed** when the selection
        /// stays *within* the block (constraint A — exclude the block's own
        /// delimiters) but **emitted** when the selection crosses out of it. This
        /// flag drives that gate over the whole subtree; `false` for containers
        /// and paired inline constructs, whose gaps are ordinary content.
        line_marker: bool,
        /// An INLINE construct (emphasis/strong/link) — as opposed to a block
        /// container (paragraph/list) or a leading-marker block. Set from the
        /// `Construct` kind at build time, NOT inferred from the delimiter ranges:
        /// a paragraph can have a non-empty source `close` (trailing bytes past its
        /// last child), so a delimiter-emptiness heuristic wrongly flags it inline.
        /// [`wrap_span`] includes an inline construct WHOLE (so `{==…==}` never
        /// splits its `**`/`[]()`) but recurses into a container.
        inline: bool,
        children: Vec<Node>,
    },
}

impl Node {
    fn buf(&self) -> (i32, i32) {
        match self {
            Node::Leaf { buf, .. } | Node::Opaque { buf, .. } | Node::Branch { buf, .. } => *buf,
        }
    }

    /// The node's full source range (delimiters included), used by a parent to
    /// splice inter-sibling source (block separators, list-item newlines).
    fn src(&self) -> Range<usize> {
        match self {
            Node::Leaf { src, .. } | Node::Opaque { src, .. } => src.clone(),
            Node::Branch {
                open,
                close,
                children,
                ..
            } => {
                let start = if open.start != open.end {
                    open.start
                } else {
                    children.first().map_or(open.start, |c| c.src().start)
                };
                let end = if close.start != close.end {
                    close.end
                } else {
                    children.last().map_or(close.end, |c| c.src().end)
                };
                start..end
            }
        }
    }
}

/// A buffer-annotated construct tree for one render, plus the buffer's total char
/// count (for the whole-document = Copy Document special case).
#[derive(Clone, Debug)]
pub(crate) struct CopyTree {
    root: Node,
    char_count: i32,
}

// ── building ──────────────────────────────────────────────────────────────────

/// Build the copymap from the classified render-event stream. `char_count` is
/// the finished buffer's char count. Pure: no GTK, fully unit-testable.
pub(crate) fn build(md: &str, evs: &[RawEv], char_count: i32) -> CopyTree {
    let mut b = Builder {
        md,
        evs,
        i: 0,
        depth: 0,
    };
    let children = b.children(None);
    let root = Node::Branch {
        buf: (0, char_count),
        open: 0..0,
        close: md.len()..md.len(),
        line_marker: false,
        inline: false,
        children,
    };
    CopyTree { root, char_count }
}

/// Report a copymap drift: **fatal under test, loud everywhere else.**
///
/// QA round 3, P-9. `debug_verify`'s assertions are driven by DOCUMENT content,
/// so in a debug build of the application they turn a malformed or merely
/// unanticipated document into a process abort. That is not theoretical — the
/// regression test titled *"live crash on a doc with tables"* in
/// `copymap/tests.rs` exists because it happened.
///
/// The check itself is valuable and stays: it is what catches drift between the
/// renderer's insertions and the offset capture, and losing it would cost more
/// than it saves. What changes is who pays for a failure. Under `cfg(test)` the
/// assertion is hard, so every existing test keeps exactly the protection it
/// had — including the mutation-tested drift regressions. Running the debug
/// application, the same condition is logged at error level and the document
/// still opens, because a copy that is imprecise in one leaf is a far better
/// outcome for the person holding unsaved work than an abort.
#[cfg(debug_assertions)]
macro_rules! drift {
    ($left:expr, $right:expr, $($msg:tt)+) => {
        if $left != $right {
            if cfg!(test) {
                panic!("copymap {}: {:?} != {:?}", format_args!($($msg)+), $left, $right);
            } else {
                log::error!(
                    "copymap drift — {}: {:?} != {:?}. Copy may be imprecise here; \
                     this is a renderer/copymap consistency bug, please report it.",
                    format_args!($($msg)+),
                    $left,
                    $right
                );
            }
        }
    };
}

/// Debug-only structural check (copy-as-Markdown build-time consistency guard):
/// every 1:1 leaf's source slice must EQUAL the buffer text it claims to cover,
/// catching drift between the renderer's insertions and the offset capture.
/// `buffer_chars` is the finished preview buffer as a char slice — it MUST be the
/// buffer's *slice* (anchored children as one U+FFFC each), not its *text* (which
/// omits anchors), so its indices match `char_count`/iter offsets.
///
/// Failures are reported through [`drift!`] — fatal under test, logged in a
/// running debug build. See that macro for why.
#[cfg(debug_assertions)]
pub(crate) fn debug_verify(tree: &CopyTree, md: &str, buffer_chars: &[char]) {
    fn walk(node: &Node, md: &str, chars: &[char]) {
        match node {
            Node::Leaf {
                buf,
                src,
                one_to_one,
            } if *one_to_one => {
                let slice: String = chars
                    .get(buf.0 as usize..buf.1 as usize)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();
                drift!(
                    sl(md, src.clone()),
                    slice.as_str(),
                    "1:1 leaf source/buffer drift at buffer {buf:?}"
                );
            }
            Node::Branch { children, .. } => {
                for c in children {
                    walk(c, md, chars);
                }
            }
            _ => {}
        }
    }
    drift!(
        tree.root.buf(),
        (0, tree.char_count),
        "root miscovers buffer"
    );
    walk(&tree.root, md, buffer_chars);
}

struct Builder<'a> {
    md: &'a str,
    evs: &'a [RawEv],
    i: usize,
    /// Current construct-nesting depth, bounded by [`limits::MAX_NEST_DEPTH`].
    ///
    /// `construct` recurses once per level and the event stream's depth is
    /// attacker-controlled — `pulldown_cmark` is iterative and will happily hand
    /// us 20 000 levels from a 20 KB file, so without this counter a ~1.1 KiB
    /// document of nested `>` overflows the stack and **aborts the process**
    /// (measured; see [`limits::MAX_NEST_DEPTH`]). A stack overflow is not a
    /// catchable panic, so this has to be prevented rather than handled.
    depth: usize,
}

impl Builder<'_> {
    /// Consume events until `End(stop)` (or end of stream when `stop` is `None`,
    /// for the document root), returning the child nodes in order.
    fn children(&mut self, stop: Option<Construct>) -> Vec<Node> {
        let mut out = Vec::new();
        while self.i < self.evs.len() {
            let ev = &self.evs[self.i];
            match &ev.kind {
                RawKind::End(c) => {
                    self.i += 1;
                    if Some(*c) == stop {
                        return out;
                    }
                    // A stray End (should not happen with well-formed input).
                }
                RawKind::Start(c) => {
                    let c = *c;
                    self.depth += 1;
                    let node = self.construct(c);
                    self.depth -= 1;
                    if let Some(node) = node {
                        out.push(node);
                    }
                }
                RawKind::Text(t) => {
                    let (buf, src, t) = (ev.buf, ev.src.clone(), t.clone());
                    self.i += 1;
                    out.extend(text_nodes(self.md, buf, src, &t));
                }
                RawKind::Code(t) => {
                    let node = code_node(self.md, ev.buf, ev.src.clone(), t);
                    self.i += 1;
                    out.push(node);
                }
                RawKind::Break => {
                    let (buf, src) = (ev.buf, ev.src.clone());
                    self.i += 1;
                    out.push(leaf(self.md, buf, src));
                }
                RawKind::Atomic => {
                    let node = Node::Opaque {
                        buf: ev.buf,
                        src: ev.src.clone(),
                    };
                    self.i += 1;
                    out.push(node);
                }
            }
        }
        out
    }

    /// Build the node for the construct whose `Start` is at `self.i`.
    ///
    /// **Depth-bounded.** Past [`limits::MAX_NEST_DEPTH`] the construct is
    /// recorded as one opaque node instead of being recursed into: copy still
    /// reproduces its whole source, it is simply not char-precise *inside*. That
    /// is a real (if invisible) loss of fidelity, accepted because the
    /// alternative is a process abort — and because a document nested that deep
    /// was not written by a person. Degrading is deliberate rather than
    /// erroring: refusing to render a document because one construct is deeply
    /// nested would be a worse answer than rendering it with coarser copy.
    fn construct(&mut self, c: Construct) -> Option<Node> {
        let start = self.evs[self.i].clone();
        self.i += 1;

        if is_opaque(c) || self.depth >= limits::MAX_NEST_DEPTH {
            let end = self.skip_to_end(c);
            // (start.before, end.after): spans exactly the construct's glyphs
            // while excluding a following block's separator; a preceding block
            // separator is harmlessly absorbed (overlap-detection only —
            // inter-sibling source is spliced from ranges, not buffer gaps).
            let buf = (start.buf.0, end.buf.1);
            let src = start.src.start..end.src.end;
            return Some(Node::Opaque { buf, src });
        }

        // Char-precise construct: recurse, capturing the matching End for the
        // content buffer range.
        let mut children = Vec::new();
        let mut end_ev = start.clone();
        while self.i < self.evs.len() {
            let ev = &self.evs[self.i];
            match &ev.kind {
                RawKind::End(cc) if *cc == c => {
                    end_ev = ev.clone();
                    self.i += 1;
                    break;
                }
                RawKind::Start(cc) => {
                    let cc = *cc;
                    self.depth += 1;
                    let node = self.construct(cc);
                    self.depth -= 1;
                    if let Some(node) = node {
                        children.push(node);
                    }
                }
                RawKind::Text(t) => {
                    let (buf, src, t) = (ev.buf, ev.src.clone(), t.clone());
                    self.i += 1;
                    children.extend(text_nodes(self.md, buf, src, &t));
                }
                RawKind::Code(t) => {
                    let node = code_node(self.md, ev.buf, ev.src.clone(), t);
                    self.i += 1;
                    children.push(node);
                }
                RawKind::Break => {
                    let (buf, src) = (ev.buf, ev.src.clone());
                    self.i += 1;
                    children.push(leaf(self.md, buf, src));
                }
                RawKind::Atomic => {
                    children.push(Node::Opaque {
                        buf: ev.buf,
                        src: ev.src.clone(),
                    });
                    self.i += 1;
                }
                RawKind::End(_) => {
                    self.i += 1; // stray End
                }
            }
        }

        // Content buffer range: after the Start's processing (block separators,
        // markers land before this) up to before the End's processing (a
        // heading's trailing newline lands after this).
        let content_buf = (start.buf.1, end_ev.buf.0);
        let full = start.src.start..end_ev.src.end;

        // A construct with no interior (rare/degenerate) falls back to opaque.
        let Some(content_src) = children_span(&children) else {
            return Some(Node::Opaque {
                buf: content_buf,
                src: full,
            });
        };

        let open = full.start..content_src.start.min(full.end);
        let close = content_src.end.max(full.start)..full.end;
        Some(Node::Branch {
            buf: content_buf,
            open,
            close,
            // Leading-marker blocks whose per-line markers must be gap-gated.
            line_marker: matches!(
                c,
                Construct::Heading | Construct::BlockQuote | Construct::Item
            ),
            inline: matches!(c, Construct::Emphasis | Construct::Strong | Construct::Link),
            children,
        })
    }

    /// Skip a nested-aware run of events up to and including the matching
    /// `End(c)`, returning that End event. Depth-counts `c` so a construct that
    /// nests itself (lists, list items, blockquotes) closes correctly.
    fn skip_to_end(&mut self, c: Construct) -> RawEv {
        let mut depth = 1usize;
        while self.i < self.evs.len() {
            let ev = self.evs[self.i].clone();
            self.i += 1;
            match &ev.kind {
                RawKind::Start(cc) if *cc == c => depth += 1,
                RawKind::End(cc) if *cc == c => {
                    depth -= 1;
                    if depth == 0 {
                        return ev;
                    }
                }
                _ => {}
            }
        }
        // Malformed stream: synthesize a zero-width End at the stream tail.
        self.evs.last().cloned().unwrap_or(RawEv {
            buf: (0, 0),
            src: 0..0,
            kind: RawKind::End(c),
        })
    }
}

/// Constructs always copied opaquely (whole source on any overlap): an image,
/// table, or code block owns buffer glyphs with no reconstructable interior.
/// Blockquotes AND list items — including nested and loose items — are instead
/// char-precise (leading-marker gap-gated; markers live out of the buffer in the
/// gutter, ScrAP-118), so they recurse rather than skip.
fn is_opaque(c: Construct) -> bool {
    matches!(
        c,
        Construct::Image | Construct::Table | Construct::CodeBlock
    )
}

/// The source span covered by a node's children (their full source ranges).
fn children_span(children: &[Node]) -> Option<Range<usize>> {
    let start = children.first()?.src().start;
    let end = children.iter().map(|c| c.src().end).max()?;
    Some(start..end)
}

/// A plain 1:1-if-possible leaf over a buffer range and its source.
fn leaf(md: &str, buf: (i32, i32), src: Range<usize>) -> Node {
    let one_to_one = sl(md, src.clone()).chars().count() as i32 == buf.1 - buf.0;
    Node::Leaf {
        buf,
        src,
        one_to_one,
    }
}

/// Build the node(s) for a `Text` event, re-splitting it with `scan_scripts` so
/// tight strikethrough / super / subscript reconstruct char-precisely (their
/// markers are stripped by the renderer, so they are *not* pulldown events).
///
/// A Text event contributes *several* sibling nodes. When the run contains
/// escapes/entities (its rendered text, with the scan markers reinstated, does
/// not equal the source), it degrades to one opaque leaf — the atomicity
/// guarantee.
///
/// **Iterative on purpose, and the reason is a measurement rather than a
/// suspicion.** The leading-escape peel below used to recurse once per escape,
/// which made its stack depth a function of how pulldown-cmark chooses to split
/// a Text run at escapes — the seventh recursion in this crate, and the one that
/// [`crate::limits::MAX_NEST_DEPTH`] does *not* bound, because it is driven by an
/// event's contents rather than by the built tree (QA round 3, R3).
///
/// So it was measured, both ways, rather than argued about:
///
/// * **The recursion was never deep.** A document of 100 000 consecutive `\*`
///   escapes builds fine under the *recursive* version — pulldown splits the
///   Text run at every escape, so each call peeled exactly one and re-entered on
///   a remainder that had none. Instrumenting the loop below confirms it from
///   the other side: across the whole test suite and that 100 000-escape
///   document, it never iterates more than once.
/// * **Which is precisely why the shape had to change rather than be capped.**
///   The old depth bound was not a property of this code, it was a property of
///   an upstream crate's tokenisation that nothing here pins and no test would
///   notice changing. Peeling in a loop removes the dependency instead of
///   bounding it: there is no depth left to be wrong about, so no cap is needed
///   and none can be outgrown.
///
/// Note for whoever mutation-tests this: restoring the recursion **does not fail
/// any test**, and that is the honest finding, not a gap. No input reachable
/// today drives it deep, so a "deep escapes do not overflow" test would pass on
/// both versions — `ScrAP-209`'s shape, an assertion that cannot fail. The change is
/// justified by removing an unowned assumption, not by a defect it fixes.
fn text_nodes(md: &str, buf: (i32, i32), src: Range<usize>, rendered: &str) -> Vec<Node> {
    // CommonMark backslash-escape: pulldown splits the Text run AT an escape and
    // drops the `\` from every token's range (it belongs to no event — ScrAP-73), so
    // an escaped character begins this run with its escaping backslash sitting
    // just before `src.start`. Peel that first char into an ATOMIC `\x` leaf (its
    // source absorbs the backslash, its buffer stays one glyph) so a selection
    // confined to the bare escaped char copies the backslash too (`\*`, not `*`),
    // while the remainder of the run stays char-precise. The absorbed backslash
    // was the inter-sibling gap byte, so the parent no longer splices it — no
    // double-count. Guard on rendered==source for the first char so we never peel
    // an entity (`&amp;` ≠ `&`, handled whole below).
    //
    // The loop peels EVERY leading escape, exactly as the old recursion did by
    // re-entering on the remainder — same nodes, same order, bounded stack.
    let mut nodes: Vec<Node> = Vec::new();
    let mut buf = buf;
    let mut src = src;
    let mut rendered = rendered;
    while let Some(ch) = rendered.chars().next() {
        let clen = ch.len_utf8();
        if !escaped_at(md, src.start) || sl(md, src.start..src.start + clen) != ch.to_string() {
            break;
        }
        nodes.push(Node::Leaf {
            buf: (buf.0, buf.0 + 1),
            src: (src.start - 1)..(src.start + clen),
            one_to_one: false, // atomic: any overlap copies the whole `\x`
        });
        buf = (buf.0 + 1, buf.1);
        src = (src.start + clen)..src.end;
        rendered = &rendered[clen..];
    }
    nodes.extend(text_nodes_unescaped(md, buf, src, rendered));
    nodes
}

/// The remainder of [`text_nodes`] once any leading escapes have been peeled:
/// split the run into script segments and emit one node per segment. Separate
/// only so the peel above can be a loop with a single exit.
fn text_nodes_unescaped(md: &str, buf: (i32, i32), src: Range<usize>, rendered: &str) -> Vec<Node> {
    let segments = scan_scripts(rendered);

    // Reconstruct the pre-scan source by reinstating the stripped markers; if it
    // matches the real source, every segment aligns 1:1 (no escapes/entities).
    let mut expected = String::new();
    for (run, script) in &segments {
        let m = marker(*script);
        expected.push_str(m);
        expected.push_str(run);
        expected.push_str(m);
    }
    if expected != sl(md, src.clone()) {
        return vec![Node::Opaque { buf, src }];
    }

    let mut out = Vec::new();
    let mut sb = src.start;
    let mut cb = buf.0;
    for (run, script) in &segments {
        let m = marker(*script);
        let mlen = m.len();
        let run_chars = run.chars().count() as i32;
        let content_src = (sb + mlen)..(sb + mlen + run.len());
        let child_buf = (cb, cb + run_chars);
        if mlen == 0 {
            out.push(Node::Leaf {
                buf: child_buf,
                src: content_src,
                one_to_one: true,
            });
        } else {
            let full = sb..(sb + 2 * mlen + run.len());
            let child = Node::Leaf {
                buf: child_buf,
                src: content_src.clone(),
                one_to_one: true,
            };
            out.push(Node::Branch {
                buf: child_buf,
                open: sb..sb + mlen,
                close: content_src.end..full.end,
                line_marker: false,
                inline: true, // script marker (^sup^/~sub~/~~strike~~)
                children: vec![child],
            });
        }
        sb += 2 * mlen + run.len();
        cb += run_chars;
    }
    out
}

/// Build the node for an inline `Code` span. Reconstructs the backtick fence
/// from source when the content aligns 1:1; degrades to opaque otherwise (e.g. a
/// CommonMark one-space strip, doubled fences).
fn code_node(md: &str, buf: (i32, i32), src: Range<usize>, rendered: &str) -> Node {
    let s = sl(md, src.clone());
    let ticks = s.bytes().take_while(|&c| c == b'`').count();
    if ticks > 0 && s.len() >= 2 * ticks {
        let content = src.start + ticks..src.end - ticks;
        if sl(md, content.clone()) == rendered && rendered.chars().count() as i32 == buf.1 - buf.0 {
            let child = Node::Leaf {
                buf,
                src: content.clone(),
                one_to_one: true,
            };
            return Node::Branch {
                buf,
                open: src.start..src.start + ticks,
                close: src.end - ticks..src.end,
                line_marker: false,
                inline: true, // inline code fence — wrap whole so backticks don't split
                children: vec![child],
            };
        }
    }
    Node::Opaque { buf, src }
}

/// Whether the source character at byte `pos` is CommonMark backslash-escaped —
/// i.e. it is immediately preceded by an ODD run of `\`. The final backslash of
/// an odd run escapes the next char (each earlier `\\` pair is a literal
/// backslash), so `\*` is escaped but `\\*` (an escaped backslash then a literal
/// `*`) is not. Used to fold a dropped escaping backslash back onto its char.
fn escaped_at(md: &str, pos: usize) -> bool {
    let bytes = md.as_bytes();
    let mut run = 0usize;
    while pos > run && bytes[pos - 1 - run] == b'\\' {
        run += 1;
    }
    run % 2 == 1
}

/// The source marker `scan_scripts` strips for each script kind.
fn marker(script: Script) -> &'static str {
    match script {
        Script::None => "",
        Script::Superscript => "^",
        Script::Subscript => "~",
        Script::Strikethrough => "~~",
        Script::Highlight => "==",
    }
}

// ── resolving ─────────────────────────────────────────────────────────────────

/// Translate a preview buffer selection `[a, b)` (char offsets) into the
/// Markdown source it should copy, per the uniform boundary rule.
pub(crate) fn resolve(tree: &CopyTree, md: &str, a: i32, b: i32) -> String {
    // Whole-buffer selection = Copy Document (constraint B): return all source,
    // exactly (guarantees byte-identical leading/trailing source).
    if a <= 0 && b >= tree.char_count {
        return md.to_string();
    }
    resolve_node(md, &tree.root, a, b, true)
}

/// Resolve a selection within a single table cell's copymap (`a`/`b` are char
/// offsets into the cell label's plain text). Unlike [`resolve`] there is no
/// whole-buffer = Copy-Document special case — a cell has no "whole document", so
/// a whole-cell selection reconstructs the cell's own Markdown (with delimiters).
pub(crate) fn resolve_cell(tree: &CopyTree, md: &str, a: i32, b: i32) -> String {
    resolve_node(md, &tree.root, a, b, true)
}

/// The number of rendered (plain-text) characters a classified cell event
/// contributes to its table-cell label — the offset basis a per-cell copymap is
/// built on (the label's plain text, which `label.selection_bounds()` indexes).
/// Only `Text`/`Code` add glyphs; construct wrappers add none.
pub(crate) fn cell_width(kind: &RawKind) -> i32 {
    match kind {
        RawKind::Text(t) => scan_scripts(t)
            .iter()
            .map(|(run, _)| run.chars().count() as i32)
            .sum(),
        RawKind::Code(t) => t.chars().count() as i32,
        _ => 0,
    }
}

/// The CLEANED-source byte span to WRAP for an annotation over buffer selection
/// `[a, b)` (the CriticMarkup cleaned↔original mapping). Unlike [`resolve`] (which reconstructs balanced
/// copy *text* and CLIPS inline content), this returns the OUTER source range: any
/// inline construct — emphasis/strong/link, or an atomic code span — that the
/// selection touches is included WHOLE, so wrapping the range in `{==…==}` can
/// never split its `**`/`*`/`` ` ``/`[]()` delimiters. Block containers
/// (paragraph/list/heading) are recursed into, contributing only their touched
/// content. `None` when the selection overlaps nothing.
pub(crate) fn wrap_span(tree: &CopyTree, md: &str, a: i32, b: i32) -> Option<Range<usize>> {
    wrap_span_node(md, &tree.root, a, b)
}

fn wrap_span_node(md: &str, node: &Node, a: i32, b: i32) -> Option<Range<usize>> {
    let (c0, c1) = node.buf();
    if a.max(c0) >= b.min(c1) {
        return None; // no overlap
    }
    match node {
        Node::Leaf {
            buf,
            src,
            one_to_one,
        } => {
            if *one_to_one {
                // Partial plain run: the selected chars' byte sub-range.
                let content = sl(md, src.clone());
                let s_char = (a.max(c0) - buf.0) as usize;
                let e_char = (b.min(c1) - buf.0) as usize;
                let byte_of = |n: usize| {
                    content
                        .char_indices()
                        .nth(n)
                        .map_or(content.len(), |(i, _)| i)
                };
                Some(src.start + byte_of(s_char)..src.start + byte_of(e_char))
            } else {
                Some(src.clone()) // atomic (code span, …): whole run
            }
        }
        Node::Opaque { src, .. } => Some(src.clone()),
        Node::Branch {
            inline, children, ..
        } => {
            // An inline construct (emphasis/strong/link) is included WHOLE so its
            // `**`/`*`/`[]()` never split; a container (paragraph/list) or a
            // leading-marker block (heading/blockquote/item) recurses.
            if *inline {
                let s = node.src();
                Some(s.start..s.end)
            } else {
                let (mut lo, mut hi): (Option<usize>, Option<usize>) = (None, None);
                for child in children {
                    if let Some(r) = wrap_span_node(md, child, a, b) {
                        lo = Some(lo.map_or(r.start, |l: usize| l.min(r.start)));
                        hi = Some(hi.map_or(r.end, |h: usize| h.max(r.end)));
                    }
                }
                Some(lo?..hi?)
            }
        }
    }
}

/// The EDITOR-pane sibling of [`wrap_span`]: expand a raw source-byte selection
/// `[sel.start, sel.end)` outward until it cannot SPLIT an inline construct, so
/// wrapping the result in `{==…==}` can never land between a `**`/`*`/`~~`/`` ` ``/
/// `[]()` and its partner.
///
/// **Why this is not [`wrap_span`].** `wrap_span` resolves a *rendered preview*
/// selection and therefore needs a [`CopyTree`], whose construction routes through
/// `text_nodes` — and that helper is intrinsically about the buffer DIFFERING from
/// the source: it peels an escaped `\x` into a 1-buffer-char / 2-source-char atom
/// (ScrAP-73) and re-splits `~~`/`^`/`~` runs whose markers the renderer stripped
/// (ScrAP-66). The editor pane is the **identity render** (every source char IS a
/// buffer char), so both of those would corrupt a tree built over it — silently, and
/// exactly on the escaped/script text where offsets matter most. Hence the shared
/// *rule* ("an inline construct the selection touches is included WHOLE") is applied
/// here directly against pulldown's source ranges instead. Keep the two adjacent:
/// if the boundary rule changes, it must change in BOTH.
///
/// Block constructs are deliberately NOT expanded into — a selection inside one
/// paragraph must not swallow the whole paragraph; only *inline* constructs
/// (emphasis/strong/link/image), atomic code spans, and the four this crate
/// tokenises itself balance.
/// Iterated to a fixpoint, since expanding to swallow an inner construct can newly
/// straddle an outer one (`**a [link](u) b**`); nesting depth bounds the loop.
///
/// **Two tokenisers, both consulted.** pulldown-cmark's event stream reports only
/// the constructs pulldown parses. `==highlight==`, `~~strikethrough~~`, `^sup^`
/// and `~sub~` are tokenised by this crate instead (`renderer::scan_script_spans`,
/// ScrAP-66/ScrAP-75) — pulldown has no highlight option at all and its
/// caret/tilde flanking rules never match the tight Pandoc forms — so they arrive
/// here as plain `Text` with no event to match, and a pulldown-only walk sees
/// nothing to balance (ScrAP-195). Hence the second pass below, over each `Text`
/// event's source slice, using the very scanner that owns those constructs. Note
/// there is deliberately no `Tag::Strikethrough` arm: `md_options()` does not
/// enable pulldown's strikethrough, so such an event can never be emitted — the
/// scanner pass is what covers `~~`.
pub(crate) fn balance_source_span(md: &str, sel: Range<usize>) -> Range<usize> {
    let mut cur = sel;
    // Nesting is finite; the guard only stops a pathological non-convergence.
    for _ in 0..32 {
        let mut next = cur.clone();
        let swallow = |src: Range<usize>, next: &mut Range<usize>| {
            let overlaps = src.start < next.end && next.start < src.end;
            let contained = src.start >= next.start && src.end <= next.end;
            if overlaps && !contained {
                next.start = next.start.min(src.start);
                next.end = next.end.max(src.end);
            }
        };
        for (ev, src) in
            pulldown_cmark::Parser::new_ext(md, crate::renderer::md_options()).into_offset_iter()
        {
            // `into_offset_iter` reports a Start tag's range as the WHOLE construct
            // (delimiters included), which is exactly the span to swallow.
            if matches!(
                &ev,
                Event::Code(_)
                    | Event::Start(
                        Tag::Emphasis | Tag::Strong | Tag::Link { .. } | Tag::Image { .. }
                    )
            ) {
                swallow(src, &mut next);
                continue;
            }
            // A `Text` event's source slice is verbatim (pulldown emits escapes and
            // entities as their own events), so a span the scanner finds within it is
            // directly a source range once shifted by the event's start.
            if matches!(&ev, Event::Text(_)) {
                // `sl()` (not a raw `md[..]`): every other slice in this module
                // goes through it, and this one did not (QA round 3, P-8).
                for found in crate::renderer::scan_script_spans(sl(md, src.clone())) {
                    swallow(
                        src.start + found.outer.start..src.start + found.outer.end,
                        &mut next,
                    );
                }
            }
        }
        if next == cur {
            break;
        }
        cur = next;
    }
    cur
}

/// `emit_gaps` controls whether inter-sibling source gaps are spliced (see
/// [`reconstruct`]). It is `true` at the root and inherited down, except a
/// leading-marker block (`line_marker`) forces it to its OWN crossing state for
/// its whole subtree: suppress the per-line markers when the selection stays
/// within (constraint A), emit them when it crosses out.
fn resolve_node(md: &str, node: &Node, a: i32, b: i32, emit_gaps: bool) -> String {
    let (c0, c1) = node.buf();
    if a.max(c0) >= b.min(c1) {
        return String::new(); // no overlap
    }
    match node {
        Node::Leaf {
            buf,
            src,
            one_to_one,
        } => {
            if *one_to_one {
                let chars: Vec<char> = sl(md, src.clone()).chars().collect();
                let s = (a.max(c0) - buf.0) as usize;
                let e = (b.min(c1) - buf.0) as usize;
                chars
                    .get(s..e)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default()
            } else {
                sl(md, src.clone()).to_string() // atomic: whole run on any overlap
            }
        }
        Node::Opaque { src, .. } => sl(md, src.clone()).to_string(),
        Node::Branch {
            open,
            close,
            line_marker,
            inline,
            children,
            ..
        } => {
            let cross_end = b > c1;
            let crossed = a < c0 || cross_end;
            let child_gaps = if *line_marker { crossed } else { emit_gaps };
            let inner = reconstruct(md, children, a, b, child_gaps);
            if !crossed {
                return inner;
            }
            // Only an INLINE paired construct (emphasis/strong/link/code span) has a
            // true trailing delimiter, reconstructed whenever crossed (2.8b balanced
            // closure). A block — a leading-marker block (heading/blockquote/list
            // item) or a container (paragraph) — has only structure: its `close` is
            // a line terminator that participates in the block separator ONLY when
            // the selection crosses OUT the bottom (`cross_end`). On an *entering*
            // cross (selection ends inside the block) emitting it would append a
            // spurious trailing newline (`a\n  - nested\n`).
            let close_s = if !*inline && !cross_end {
                ""
            } else {
                sl(md, close.clone())
            };
            // A leading-marker block whose selection begins with an in-block line
            // break — its first selected glyph is a continuation newline, not
            // line-1 content — must NOT prepend line-1's marker: that would emit a
            // spurious empty quoted/indented line (`> \n> b`). The marker for the
            // first *real* content line is already reconstructed inside `inner`
            // (the gap-spliced continuation `> `/indent), so the leading one is
            // redundant. Drop it, keeping the user's selected newline (`\n> b`).
            let open_s = if *line_marker && inner.starts_with('\n') {
                ""
            } else {
                sl(md, open.clone())
            };
            format!("{open_s}{inner}{close_s}")
        }
    }
}

/// Reconstruct a branch's interior: resolve each overlapped child and, when
/// `emit_gaps`, splice the inter-sibling source (block separators, list-item
/// newlines, per-line `> `/indent markers) between two consecutive overlapped
/// children so structure survives even when the buffer has no gap between them.
fn reconstruct(md: &str, children: &[Node], a: i32, b: i32, emit_gaps: bool) -> String {
    let mut out = String::new();
    let mut prev: Option<Range<usize>> = None;
    for child in children {
        let (k0, k1) = child.buf();
        if a.max(k0) >= b.min(k1) {
            continue; // this child is outside the selection
        }
        if let Some(prev_src) = &prev {
            let gap = prev_src.end..child.src().start;
            if gap.start < gap.end {
                let g = sl(md, gap);
                if emit_gaps || emits_leading_marker(child, a, b) {
                    // Full inter-sibling source: block separators, and the per-line
                    // `> `/`- `/indent markers a crossed (or marker-fronting) block
                    // needs — the nested item's marker requires its indent lead-in,
                    // or it would abut the previous line's text (`a- nested`).
                    out.push_str(g);
                } else {
                    // Within an un-crossed leading-marker block the markers/indent
                    // are suppressed (constraint A), but the gap's structural
                    // BLANK-LINE newlines must survive so stitched siblings keep
                    // their separation — a loose list item's two paragraphs stay
                    // `top\n\nloose para`, not collapsed onto one line.
                    out.extend(g.chars().filter(|&c| c == '\n'));
                }
            }
        }
        out.push_str(&resolve_node(md, child, a, b, emit_gaps));
        prev = Some(child.src());
    }
    out
}

/// Whether resolving `node` for selection `[a, b)` begins by emitting a
/// leading-marker block's reconstructed `open` delimiter — i.e. the leftmost
/// selected leaf sits under a **crossed** `line_marker` branch (nested list item,
/// blockquote) whose marker precedes it. Used by [`reconstruct`] to decide
/// whether that marker's structural lead-in gap must be spliced even when the
/// parent block is not itself crossed.
fn emits_leading_marker(node: &Node, a: i32, b: i32) -> bool {
    let Node::Branch {
        buf,
        open,
        line_marker,
        children,
        ..
    } = node
    else {
        return false;
    };
    let (c0, c1) = *buf;
    if a.max(c0) >= b.min(c1) {
        return false; // not selected
    }
    if *line_marker && (a < c0 || b > c1) && open.start != open.end {
        return true; // this crossed block emits its own leading marker
    }
    // Otherwise recurse into the first overlapped child (a container, or an
    // un-crossed leading-marker block, may still front a crossed nested one).
    children
        .iter()
        .find(|c| {
            let (k0, k1) = c.buf();
            a.max(k0) < b.min(k1)
        })
        .is_some_and(|c| emits_leading_marker(c, a, b))
}

/// Byte-range slice that never panics on a bad range (degrade to empty).
fn sl(md: &str, r: Range<usize>) -> &str {
    md.get(r).unwrap_or_default()
}

#[cfg(test)]
mod tests;
