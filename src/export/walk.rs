//! The event walk: pulldown-cmark's stream → the [`ExportDoc`] tree.
//!
//! Display-free. Nesting — a list inside a block quote inside a list — falls out of
//! the two stacks rather than needing a case per combination, which is what makes
//! Document Rendering CAM row 2 (every container context) a property of the shape
//! rather than a checklist walked by hand.

use super::{align_of, heading_level, plain_text};
use super::{Align, Block, ExportDoc, ImageRef, ImageSource, Inline, ListItem, RenderOptions};
use crate::links::{self, ImageResolution};
use crate::renderer::{is_inline_tag, BlockScripts, Script};
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use std::collections::HashMap;

pub(super) struct Builder<'a> {
    opts: &'a RenderOptions,
    /// Completed blocks at each nesting depth, innermost last. Never empty.
    block_stack: Vec<Vec<Block>>,
    /// Open inline containers, innermost last.
    inline_stack: Vec<Vec<Inline>>,
    /// What each open frame closes into, innermost last.
    open: Vec<Open>,
    /// Running rendered-char counter — the coordinate `Inline::Text::span` and the
    /// claim mapper both count in.
    rendered: i32,
    content_evs: Vec<(usize, usize, i32, i32)>,
    /// Heading slugs already issued, so a repeated heading gets `-1`, `-2`… exactly
    /// as the preview's anchors do — an in-document link that works on screen works
    /// in the artefact.
    slugs: HashMap<String, u32>,
    title: Option<String>,
    table: Option<TableBuild>,
    in_table_head: bool,
    /// Items collected for each open list, innermost last.
    list_items: Vec<Vec<ListItem>>,
    /// A task-list marker seen inside the item currently open, if any.
    pending_task: Option<bool>,
    /// Fenced code text accumulates here between the block's start and end events.
    code: Option<(Option<String>, String)>,
    /// `<picture>` grouping carried ACROSS events, because a single-line
    /// `<picture>…</picture>` is not a CommonMark HTML block and arrives as separate
    /// inline events — grouping inside one parse call loses it (ScrAP-147).
    picture_open: bool,
    picture_taken: bool,
    has_unembedded_remote: bool,
    /// The block-scope tight-construct table for the document being walked, so a
    /// `~~ … ~~` fence wrapping other inline markup exports struck rather than as
    /// two literal `~~` — Document Rendering CAM row 17 (exports as it renders).
    scripts: BlockScripts,
}

/// A construct whose end event closes a frame.
enum Open {
    Heading(u8),
    Paragraph,
    /// A paragraph **pulldown-cmark never opened**.
    ///
    /// A *tight* list item's content arrives as bare inline events with no
    /// `Tag::Paragraph` around them, unlike a loose item's, which is wrapped. Without
    /// a frame to collect them, every inline in such an item becomes its own block —
    /// so an item holding inline code, a link or a soft break is exploded into one
    /// paragraph per token. Opened lazily by [`Builder::push_inline`] and closed by
    /// [`Builder::flush_implicit`] at the next block boundary.
    ImplicitParagraph,
    BlockQuote,
    List(Option<u64>),
    Item,
    Emphasis,
    Strong,
    Strikethrough,
    Link {
        href: String,
        title: Option<String>,
    },
    Image {
        url: String,
        title: Option<String>,
    },
    TableCell,
    /// Collected but emitting nothing of its own.
    Transparent,
}

struct TableBuild {
    aligns: Vec<Align>,
    head: Vec<Vec<Inline>>,
    rows: Vec<Vec<Vec<Inline>>>,
    row: Vec<Vec<Inline>>,
}

impl<'a> Builder<'a> {
    pub(super) fn new(opts: &'a RenderOptions, scripts: BlockScripts) -> Self {
        Self {
            opts,
            scripts,
            block_stack: vec![Vec::new()],
            inline_stack: Vec::new(),
            open: Vec::new(),
            rendered: 0,
            content_evs: Vec::new(),
            slugs: HashMap::new(),
            title: None,
            table: None,
            in_table_head: false,
            list_items: Vec::new(),
            pending_task: None,
            code: None,
            picture_open: false,
            picture_taken: false,
            has_unembedded_remote: false,
        }
    }

    pub(super) fn finish(mut self) -> ExportDoc {
        // Pulldown wraps top-level content in a paragraph, so this is normally a no-op
        // — but a document that ends mid-frame must not silently lose its last run.
        self.flush_implicit();
        ExportDoc {
            title: self.title.take(),
            blocks: self.block_stack.pop().unwrap_or_default(),
            annotations: Vec::new(),
            has_unembedded_remote_images: self.has_unembedded_remote,
            content_evs: self.content_evs,
        }
    }

    fn push_block(&mut self, block: Block) {
        if let Some(top) = self.block_stack.last_mut() {
            top.push(block);
        }
    }

    fn push_inline(&mut self, inline: Inline) {
        // No open container: a tight list item's content, or a bare `<img>` line. Open
        // an implicit paragraph and let every following inline join it, rather than
        // making each one a block of its own.
        if self.inline_stack.is_empty() {
            self.inline_stack.push(Vec::new());
            self.open.push(Open::ImplicitParagraph);
        }
        if let Some(top) = self.inline_stack.last_mut() {
            top.push(inline);
        }
    }

    /// Close an implicit paragraph, if one is open, into the enclosing block frame.
    ///
    /// Called at every block boundary — before a construct starts, before one ends, and
    /// before a block-level event of its own — because an implicit paragraph has no end
    /// event to close it. **Before** `end()` pops its own frame, so the paragraph lands
    /// inside the item that owned it rather than beside it.
    fn flush_implicit(&mut self) {
        if !matches!(self.open.last(), Some(Open::ImplicitParagraph)) {
            return;
        }
        self.open.pop();
        let inlines = self.close_inlines();
        if !inlines.is_empty() {
            self.push_block(Block::Paragraph(inlines));
        }
    }

    fn close_inlines(&mut self) -> Vec<Inline> {
        self.inline_stack.pop().unwrap_or_default()
    }

    pub(super) fn event(&mut self, ev: Event<'_>, src: std::ops::Range<usize>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.text(&t, src),
            Event::Code(c) => {
                let before = self.rendered;
                self.rendered += c.chars().count() as i32;
                self.content_evs
                    .push((src.start, src.end, before, self.rendered));
                self.push_inline(Inline::Code(c.to_string()));
            }
            Event::SoftBreak | Event::HardBreak => {
                let before = self.rendered;
                self.rendered += 1;
                self.content_evs
                    .push((src.start, src.end, before, self.rendered));
                self.push_inline(Inline::Break);
            }
            Event::Rule => {
                self.flush_implicit();
                self.push_block(Block::Rule);
            }
            // The marker belongs to the item already open around it; it is held
            // until that item closes rather than emitted, because a task marker is
            // a property of the item and not content inside it.
            Event::TaskListMarker(checked) => self.pending_task = Some(checked),
            // Raw HTML is DROPPED, exactly as the preview drops it — except the image
            // elements the one allowlist names. Not escaped (that would put text on
            // the page the preview never showed) and never passed through (that would
            // put an untrusted document's markup into a file the reader is about to
            // send). TDD 25.4.
            Event::Html(h) | Event::InlineHtml(h) => self.html(&h),
            // Not enabled in `md_options`, so these never arrive; the arm is explicit
            // so enabling one later is a visible change here rather than a silent
            // omission from every export.
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {}
        }
    }

    fn text(&mut self, t: &str, src: std::ops::Range<usize>) {
        // Code-block text is literal: it accumulates verbatim, with no tight-construct
        // scanning, because `~~` and `==` inside a fence are code.
        if let Some((_, body)) = self.code.as_mut() {
            body.push_str(t);
            return;
        }
        // The four tight constructs pulldown never sees arrive here as plain text.
        // `scan_scripts` is their single definition; consulting it is what keeps this
        // from being the *different renderer* a second parse would make it
        // (ScrAP-66, ScrAP-195).
        let before = self.rendered;
        for seg in self.scripts.segments(src.start, t) {
            // A delimiter is source, not content: it owns no rendered char, so it
            // must not advance the counter `Inline::Text::span` and the claim
            // mapper both count in.
            if seg.marker {
                continue;
            }
            let text = seg.text(t);
            if text.is_empty() {
                continue;
            }
            let start = self.rendered;
            self.rendered += text.chars().count() as i32;
            let run = Inline::Text {
                text: text.to_string(),
                span: (start, self.rendered),
            };
            self.push_inline(match seg.script {
                Script::None => run,
                Script::Superscript => Inline::Superscript(vec![run]),
                Script::Subscript => Inline::Subscript(vec![run]),
                Script::Strikethrough => Inline::Strikethrough(vec![run]),
                Script::Highlight => Inline::Highlight(vec![run]),
            });
        }
        self.content_evs
            .push((src.start, src.end, before, self.rendered));
    }

    fn start(&mut self, tag: Tag<'_>) {
        // A BLOCK construct opening ends any implicit paragraph before it — a sublist
        // inside a tight item must not swallow the item's own text. An INLINE one must
        // not: a link belongs *inside* the paragraph, and closing at its edge is what
        // splits an item into one paragraph per token.
        if is_block_start(&tag) {
            self.flush_implicit();
        }
        match tag {
            Tag::Heading { level, .. } => {
                self.inline_stack.push(Vec::new());
                self.open.push(Open::Heading(heading_level(level)));
            }
            Tag::Paragraph => {
                self.inline_stack.push(Vec::new());
                self.open.push(Open::Paragraph);
            }
            Tag::BlockQuote(_) => {
                self.block_stack.push(Vec::new());
                self.open.push(Open::BlockQuote);
            }
            Tag::List(start) => {
                self.open.push(Open::List(start));
                self.list_items.push(Vec::new());
            }
            Tag::Item => {
                self.block_stack.push(Vec::new());
                self.open.push(Open::Item);
                self.pending_task = None;
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        // The info string's first word is the language; the rest is
                        // metadata this application does not act on.
                        info.split_whitespace().next().map(str::to_string)
                    }
                    CodeBlockKind::Indented => None,
                };
                self.code = Some((lang, String::new()));
                self.open.push(Open::Transparent);
            }
            Tag::Emphasis => {
                self.inline_stack.push(Vec::new());
                self.open.push(Open::Emphasis);
            }
            Tag::Strong => {
                self.inline_stack.push(Vec::new());
                self.open.push(Open::Strong);
            }
            Tag::Strikethrough => {
                self.inline_stack.push(Vec::new());
                self.open.push(Open::Strikethrough);
            }
            Tag::Link {
                dest_url, title, ..
            } => {
                self.inline_stack.push(Vec::new());
                self.open.push(Open::Link {
                    href: dest_url.to_string(),
                    title: (!title.is_empty()).then(|| title.to_string()),
                });
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                // An image's inner events are its alt text, so it opens an inline
                // frame and resolves at its end when the alt is known.
                self.inline_stack.push(Vec::new());
                self.open.push(Open::Image {
                    url: dest_url.to_string(),
                    title: (!title.is_empty()).then(|| title.to_string()),
                });
            }
            Tag::Table(aligns) => {
                self.table = Some(TableBuild {
                    aligns: aligns.into_iter().map(align_of).collect(),
                    head: Vec::new(),
                    rows: Vec::new(),
                    row: Vec::new(),
                });
                self.open.push(Open::Transparent);
            }
            Tag::TableHead => {
                self.in_table_head = true;
                self.open.push(Open::Transparent);
            }
            Tag::TableRow => self.open.push(Open::Transparent),
            Tag::TableCell => {
                self.inline_stack.push(Vec::new());
                self.open.push(Open::TableCell);
            }
            // Footnotes/metadata/math are not enabled; collected transparently so a
            // stray one cannot unbalance the stacks.
            _ => self.open.push(Open::Transparent),
        }
    }

    fn end(&mut self, tag: TagEnd) {
        // Same rule as `start`, and for the same reason: only a block construct's close
        // is a paragraph boundary. Flushed BEFORE the frame is popped, so the paragraph
        // lands inside the item that owned it rather than beside it.
        if is_block_end(&tag) {
            self.flush_implicit();
        }
        let Some(open) = self.open.pop() else { return };
        match open {
            Open::Heading(level) => {
                let inlines = self.close_inlines();
                let text = plain_text(&inlines);
                let id = links::unique_slug(&links::slugify(&text), &mut self.slugs);
                if level == 1 && self.title.is_none() {
                    self.title = Some(text);
                }
                self.push_block(Block::Heading { level, id, inlines });
            }
            // `flush_implicit` above pops an implicit paragraph before this match, so
            // the second arm is not reachable today. It closes the same way rather than
            // panicking: an implicit paragraph IS a paragraph, and a total match here
            // costs nothing while an `unreachable!` would turn a future ordering change
            // into a crash instead of correct output.
            Open::Paragraph | Open::ImplicitParagraph => {
                let inlines = self.close_inlines();
                if !inlines.is_empty() {
                    self.push_block(Block::Paragraph(inlines));
                }
            }
            Open::BlockQuote => {
                let inner = self.block_stack.pop().unwrap_or_default();
                self.push_block(Block::BlockQuote(inner));
            }
            Open::List(start) => {
                let items = self.list_items.pop().unwrap_or_default();
                self.push_block(Block::List { start, items });
            }
            Open::Item => {
                let blocks = self.block_stack.pop().unwrap_or_default();
                let task = self.pending_task.take();
                if let Some(items) = self.list_items.last_mut() {
                    items.push(ListItem { task, blocks });
                }
            }
            Open::Emphasis => {
                let v = self.close_inlines();
                self.push_inline(Inline::Emphasis(v));
            }
            Open::Strong => {
                let v = self.close_inlines();
                self.push_inline(Inline::Strong(v));
            }
            Open::Strikethrough => {
                let v = self.close_inlines();
                self.push_inline(Inline::Strikethrough(v));
            }
            Open::Link { href, title } => {
                let inner = self.close_inlines();
                // The scheme allowlist is `links`', consulted rather than re-decided.
                // A link the application would refuse to open is emitted as its text
                // alone, so the artefact shows what the preview shows and carries no
                // destination this project would not follow itself.
                if links::is_allowed_url(&href) || links::doc_link_fragment(&href).is_some() {
                    self.push_inline(Inline::Link { href, title, inner });
                } else {
                    for i in inner {
                        self.push_inline(i);
                    }
                }
            }
            Open::Image { url, title } => {
                let alt_inlines = self.close_inlines();
                let alt = plain_text(&alt_inlines);
                let image = self.resolve(&url, alt, title);
                self.push_inline(Inline::Image(image));
            }
            Open::TableCell => {
                let cell = self.close_inlines();
                if let Some(t) = self.table.as_mut() {
                    t.row.push(cell);
                }
            }
            Open::Transparent => self.close_transparent(),
        }
    }

    /// Close whichever transparent construct is finishing: a code block, a table, its
    /// head, or a row. Each is identified by the state it left behind rather than by
    /// the tag, so the stacks stay balanced whatever pulldown emits.
    fn close_transparent(&mut self) {
        if let Some((lang, text)) = self.code.take() {
            self.push_block(Block::CodeBlock { lang, text });
            return;
        }
        let Some(t) = self.table.as_mut() else { return };
        if !t.row.is_empty() {
            let row = std::mem::take(&mut t.row);
            if self.in_table_head {
                t.head = row;
                self.in_table_head = false;
            } else {
                t.rows.push(row);
            }
            return;
        }
        if self.in_table_head {
            self.in_table_head = false;
            return;
        }
        // No pending row and not in the head — this closes the table itself.
        let Some(t) = self.table.take() else { return };
        self.push_block(Block::Table {
            aligns: t.aligns,
            head: t.head,
            rows: t.rows,
        });
    }

    fn html(&mut self, html: &str) {
        for tag in crate::renderer::scan_image_tags(html) {
            match tag {
                crate::renderer::ImgTag::PictureOpen => {
                    self.picture_open = true;
                    self.picture_taken = false;
                }
                crate::renderer::ImgTag::PictureClose => {
                    self.picture_open = false;
                    self.picture_taken = false;
                }
                crate::renderer::ImgTag::Candidate(src) => {
                    // Inside a `<picture>` the first candidate that resolves wins and
                    // the rest are its fallbacks; outside one, every candidate is its
                    // own independent image — nothing links them (ScrAP-147).
                    if self.picture_open && self.picture_taken {
                        continue;
                    }
                    let image = self.resolve(&src, String::new(), None);
                    let usable = !matches!(image.source, ImageSource::Missing(_));
                    if self.picture_open {
                        if !usable {
                            continue;
                        }
                        self.picture_taken = true;
                    }
                    self.push_inline(Inline::Image(image));
                }
            }
        }
    }

    /// Resolve an image `src` through the SAME gate the preview resolves it through,
    /// and read its bytes only where that gate admitted them.
    fn resolve(&mut self, src: &str, alt: String, title: Option<String>) -> ImageRef {
        let resolution = links::resolve_image(
            src,
            self.opts.doc_dir.as_deref(),
            self.opts.allow_unsafe_images,
        );
        let placeholder = |res: &ImageResolution| {
            ImageSource::Missing(
                crate::renderer::image_placeholder_tooltip(res, false, src).unwrap_or_default(),
            )
        };
        let source = match &resolution {
            ImageResolution::Local(path) => match super::doc::read_image(path) {
                Some((bytes, mime)) => ImageSource::Embedded { bytes, mime },
                None => placeholder(&resolution),
            },
            // Referenced, never fetched at export time. Reaching the network here
            // would be a second network path with its own timeouts, trust store and
            // proxy rule, which POLICY routes through `imagefetch` alone — and the
            // reader consented to these images being *shown*, not to this application
            // downloading them while a save dialog is open.
            ImageResolution::Remote(url) => {
                self.has_unembedded_remote = true;
                ImageSource::Remote(url.clone())
            }
            ImageResolution::Refused | ImageResolution::Missing => placeholder(&resolution),
        };
        ImageRef { alt, title, source }
    }
}

/// Whether a start tag opens a **block**-level construct — the boundary at which an
/// implicit paragraph closes.
///
/// Written as "not one of the inline set" rather than by listing the block set: the
/// inline constructs are few and closed, while the block ones grow with the parser, so
/// a construct pulldown adds later defaults to *block* and closes a paragraph — the
/// safe direction. Getting this backwards is the defect that split a tight list item
/// into one paragraph per token.
/// Whether `tag` opens a BLOCK — the complement of the inline set, which
/// `renderer::segments` owns because the same split decides where a tight fence
/// may span (one definition, two consumers).
fn is_block_start(tag: &Tag<'_>) -> bool {
    !is_inline_tag(tag)
}

/// [`is_block_start`]'s counterpart for an end tag.
fn is_block_end(tag: &TagEnd) -> bool {
    !matches!(
        tag,
        TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link
            | TagEnd::Image
    )
}
