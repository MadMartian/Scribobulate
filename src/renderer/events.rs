//! `Renderer::process` — the top-level pulldown-cmark `Event` dispatcher. Start/End
//! tags route to [`super::start`] / [`super::end`]; text/code/breaks/rules are
//! handled inline here.

use super::scan::{scan_scripts, script_tag, Script};
use super::Renderer;
use gtk::glib;
use gtk::prelude::*;
use pulldown_cmark::Event;

impl Renderer {
    pub(crate) fn process(&mut self, ev: Event) {
        match ev {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(end) => self.end_tag(end),

            Event::Text(t) => {
                if self.suppress_image_alt {
                    // Alt text of a rendered image — the picture stands in for it.
                } else if let Some((_, ref mut acc)) = self.code {
                    acc.push_str(&t);
                } else if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        // Text outside a link in a cell that already has/had a link = mixed.
                        if ts.in_link.is_none()
                            && (ts.cell_sole_link.is_some() || !ts.cell_markup.is_empty())
                        {
                            ts.cell_mixed = true;
                        }
                        let before = ts.cell_off;
                        for (run, script) in scan_scripts(&t) {
                            let esc = glib::markup_escape_text(&run);
                            // Highlight's open tag is theme-generated (`mark_open`,
                            // the cell twin of the `TagName::Mark` body tag, Document
                            // Rendering CAM row 12), so `open` is a `Cow`; the rest
                            // are static Pango tags.
                            // Superscript/highlight are theme-generated (TDD 18.18,
                            // 18.6) — `open` is a `Cow` for exactly those two branches.
                            let (open, close): (std::borrow::Cow<str>, &str) = match script {
                                Script::Superscript => {
                                    (super::superscript_open().into(), super::SUPERSCRIPT_CLOSE)
                                }
                                Script::Subscript => {
                                    (super::subscript_open().into(), super::SUBSCRIPT_CLOSE)
                                }
                                Script::Strikethrough => {
                                    let (open, close) = super::strike_tags();
                                    (open.into(), close)
                                }
                                Script::Highlight => (super::mark_open().into(), super::MARK_CLOSE),
                                Script::None => ("".into(), ""),
                            };
                            ts.cell_markup.push_str(&open);
                            ts.cell_markup.push_str(&esc);
                            ts.cell_markup.push_str(close);
                            ts.cell_plain.push_str(&run);
                            ts.cell_off += run.chars().count() as i32;
                        }
                        let after = ts.cell_off;
                        if after > before {
                            // Table-cell annotation: content event for map_cleaned_highlight_to_local.
                            let src = self.event_src.clone();
                            ts.cell_content_evs
                                .push((src.start, src.end, before, after));
                        }
                    }
                } else {
                    let runs = scan_scripts(&t);
                    if self.heading.is_some() {
                        // The slug mirrors the rendered text with markers dropped.
                        for (run, _) in &runs {
                            self.heading_text.push_str(run);
                        }
                    }
                    for (run, script) in &runs {
                        let tag = script_tag(*script);
                        if let Some(name) = tag {
                            self.inline_tags.push(name);
                        }
                        self.insert(run);
                        if tag.is_some() {
                            self.inline_tags.pop();
                        }
                    }
                }
            }

            Event::Code(t) => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        let before = ts.cell_off;
                        ts.cell_markup.push_str("<tt>");
                        ts.cell_markup.push_str(&glib::markup_escape_text(&t));
                        ts.cell_markup.push_str("</tt>");
                        ts.cell_plain.push_str(&t);
                        ts.cell_off += t.chars().count() as i32;
                        let after = ts.cell_off;
                        let src = self.event_src.clone();
                        ts.cell_content_evs
                            .push((src.start, src.end, before, after));
                    }
                } else {
                    if self.heading.is_some() {
                        self.heading_text.push_str(&t);
                    }
                    let start = self.end_offset();
                    self.insert(&t);
                    let si = self.buf.iter_at_offset(start);
                    let ei = self.buf.end_iter();
                    self.apply(crate::tags::TagName::CodeInline, &si, &ei);
                }
            }

            Event::TaskListMarker(checked) => {
                // Upgrade this item's gutter marker to a task checkbox.
                // `event_src` is the `[ ]`/`[x]` source span the future preview toggle
                // flips. The item's marker is the last one pushed (TaskListMarker is the
                // item's first inline child, so no deeper item can precede it).
                //
                // NO anchored `GtkCheckButton` is inserted: the
                // checkbox is drawn in the left gutter in Phase 2 and occupies ZERO buffer
                // chars, exactly like the bullet/number. This also drops the old anchored,
                // inert (`set_sensitive(false)`) checkbox that sat in the text flow — an
                // anchored child parked off-screen until its line validates (GTK4Rs/AP-91) and
                // part of the selection. The item's text now follows immediately.
                if let Some(m) = self.list_markers.last_mut() {
                    m.kind = crate::renderer::ListMarkerKind::Task {
                        checked,
                        src: self.event_src.clone(),
                    };
                }
            }

            // A single source newline (CommonMark "soft break") renders as a real
            // line break, not a space — the hard-wrap model authors expect from an
            // editor previewing their own notes (TDD 2.20).
            // Trade-off: prose hard-wrapped at a fixed column breaks at each wrap
            // point. HardBreak (two trailing spaces / backslash) is identical.
            //
            // This holds INSIDE list items too: the
            // old flow-to-spaces workaround — which joined an item's source lines into one
            // paragraph so the fragile `indent` hanging-indent stayed reliable (ScrAP-118) —
            // is retired now that items carry a uniform per-level `left_margin` with NO
            // `indent`. Every line of an item (first, soft wrap, hard break, continuation
            // paragraph) left-justifies to the same content margin, so an in-item break can
            // render as a real newline again without re-outdenting.
            Event::SoftBreak | Event::HardBreak => self.newline(),

            // Raw block HTML arrives line-by-line between `Tag::HtmlBlock` start/end
            // (start.rs / end.rs); accumulate each line so the whole block is parsed
            // once, at its close, as a `<picture>`/`<img>` image or dropped. Guard on
            // `in_html_block` so a stray `Html` event outside a block can't leak.
            Event::Html(t) if self.in_html_block => self.html_acc.push_str(&t),
            // Inline raw HTML (mid-paragraph, e.g. a bare `<img …>`, or the separate
            // tags of a single-line `<picture>`): feed each tag through the scanner,
            // which honours the `<picture>` grouping carried across events. Non-image
            // HTML is dropped (sanitize by omission). See ScrAP-147 / TDD 2.23.
            Event::InlineHtml(t) => self.feed_html(&t),

            Event::Rule => {
                self.block_sep();
                let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
                // NO initial width_request: it must NOT start over-wide, or an
                // Automatic preview scroller churns from frame 1 and the view never
                // gets a clean allocation (GTK4Rs/AP-23). `CodePreviewView::size_allocate`
                // grows it to the live content column before the first paint.
                //
                // A rule is a STOCK GtkSeparator — the only anchored widget the app
                // doesn't build itself, and therefore the surface most likely to stay
                // system-coloured on a reading page. It carries a class so the generated
                // theme CSS can recolour it (it has no tag and no snapshot of ours to
                // theme through), and its margins are a themed decoration metric scaled
                // by the live zoom like every other.
                sep.add_css_class("scrib-rule");
                let space = crate::theme::px(crate::theme::active().metrics.rule_space, self.zoom);
                sep.set_margin_top(space);
                sep.set_margin_bottom(space);
                let mut iter = self.buf.end_iter();
                let anchor = self.buf.create_child_anchor(&mut iter);
                // Inset a rule nested in a list/blockquote by its enclosing content margin
                // so it fills `content − inset`, not the whole column — an indented rule
                // sized to the full column overflows by the indent → spurious Automatic
                // h-scrollbar → GTK4Rs/AP-22/23 churn (GTK4Rs/AP-23a). 0 at top level.
                self.width_bounded
                    .push((sep.clone().upcast(), self.block_inset()));
                self.anchored.push((anchor, sep.upcast()));
                self.trailing_newlines = 0;
                self.at_start = false;
                self.newline();
            }

            _ => {}
        }
    }
}
