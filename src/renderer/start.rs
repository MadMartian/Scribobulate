//! `Renderer::start_tag` — the block/inline OPEN handler: paragraph/heading/list/
//! blockquote spacing, table-cell markup accumulation, inline-tag pushes, and the
//! image safety gate + anchored-picture (or broken-image placeholder) build.

use super::image::image_placeholder_tooltip;
use super::{Renderer, TableState};
use crate::links::{resolve_image, ImageResolution};
use gtk::prelude::*;
use pulldown_cmark::{CodeBlockKind, Tag};

impl Renderer {
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
                if self.list_item_open {
                    // First paragraph immediately follows the list marker — no
                    // separator; clearing the flag allows subsequent paragraphs
                    // (loose lists) to get a newline via the else-if below.
                    self.list_item_open = false;
                } else if !self.lists.is_empty() {
                    // Subsequent paragraphs inside a loose list item: one newline
                    // is enough; a full block_sep would double-space them.
                    self.newline();
                } else {
                    // Top-level paragraphs AND blockquote paragraphs: block_sep is
                    // idempotent via trailing_newlines, so the first paragraph after
                    // a Tag::BlockQuote (which already block_sep'd) is a no-op and
                    // subsequent quote paragraphs get their blank line.
                    self.block_sep();
                }
            }
            Tag::BlockQuote(_) => {
                if self.blockquote_depth == 0 {
                    self.block_sep();
                    // Blockquote content now flows into the buffer as normal text +
                    // tags (selectable, links work, no anchored widget to churn —
                    // GTK4Rs/AP-23). Record where it starts; the range is closed and the
                    // `blockquote` indent tag applied at the matching TagEnd.
                    self.blockquote_start = Some(self.end_offset());
                }
                self.blockquote_depth += 1;
            }
            Tag::List(start) => {
                if self.lists.is_empty() {
                    // Top-level list: full block gap from preceding content.
                    self.block_sep();
                } else {
                    // Nested list inside a parent item: one newline is enough;
                    // block_sep would insert an empty line before the sub-list.
                    if !self.at_start {
                        self.newline();
                    }
                }
                // Always start ordered lists at 1 regardless of source numbers;
                // TagEnd::Item increments the counter, so any disordered or
                // repeated source numerals render as 1, 2, 3 …
                self.lists.push(start.map(|_| 1u64));
                self.list_first_item = true;
            }
            Tag::Item => {
                // Tag::List already inserted one newline (nested) or a full
                // block_sep (top-level).  Skip the newline for the first item
                // in each list; subsequent items each need exactly one.
                if self.list_first_item {
                    self.list_first_item = false;
                } else if !self.at_start {
                    self.newline();
                }
                // Record where this item starts (after the leading newline) so
                // that TagEnd::Item can apply the hanging-indent tag over the full
                // item span. Lists inside blockquotes are buffer text too now, so
                // this is unconditional — they just ALSO carry the blockquote tag,
                // whose margin the `li-{depth}` tag accumulates onto (`quoted` below).
                let item_start = self.end_offset();
                self.item_starts.push(item_start);
                // Record this item's marker for the planned drawn gutter.
                // Ordered/bullet is known here; a task item is upgraded to `Task` when
                // its `TaskListMarker` fires. Nothing draws from this yet.
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
                        ts.cell_markup.push_str("<b>");
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
                    if let Some(ts) = &mut self.table {
                        ts.cell_markup.push_str("<s>");
                    }
                } else {
                    self.inline_tags.push(crate::tags::TagName::Strike);
                }
            }
            Tag::Superscript => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        ts.cell_markup.push_str("<sup>");
                    }
                } else {
                    self.inline_tags.push(crate::tags::TagName::Superscript);
                }
            }
            Tag::Subscript => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        ts.cell_markup.push_str("<sub>");
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
            _ => {}
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
    pub(super) fn feed_html(&mut self, html: &str) {
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
        let mut iter = self.buf.end_iter();
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
        self.anchored.push((anchor.clone(), overlay.upcast()));
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
        let mut iter = self.buf.end_iter();
        let anchor = self.buf.create_child_anchor(&mut iter);
        let icon = gtk::Image::from_icon_name(crate::icons::Icon::ImageMissing.name());
        icon.set_pixel_size(32);
        icon.set_halign(gtk::Align::Start);
        crate::a11y::name(&icon, tooltip);
        self.anchored.push((anchor, icon.upcast()));
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
fn load_remote_texture(uri: &str) -> Option<gtk::gdk::Texture> {
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
}
