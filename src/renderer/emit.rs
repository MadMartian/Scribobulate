//! Buffer-emission helpers and trivial accessors on [`Renderer`]: the low-level
//! `insert`/`newline`/`block_sep` primitives, the per-line blockquote tagging, and
//! the syntect-highlighted code-block emitter. Called by the event handlers in
//! [`super::events`] / [`super::start`] / [`super::end`].

use super::blockquote::logical_line_ranges;
use super::{syntect, Renderer};
use crate::tags::TagName;
use gtk::prelude::*;
use gtk::TextTag;
use syntect::easy::HighlightLines;

/// The buffer tag each heading slot renders with, indexed by
/// [`crate::theme::heading_slot`].
///
/// A table rather than a `match`, so the fold is not restated here: a `match` over
/// `HeadingLevel` has to name the h5/h6 collapse itself, which is how this became one
/// of five hand-rolled copies of one rule.
const HEADING_TAGS: [TagName; crate::theme::HEADING_LEVELS] = [
    TagName::H1,
    TagName::H2,
    TagName::H3,
    TagName::H4,
    TagName::H5,
];

/// Whether `iter`'s line has already been given a list-item content margin — i.e. it
/// carries any tag from the `li-*` family. Used to keep the "exactly one `li-*` tag per
/// line" invariant that the ACCUMULATIVE margins depend on (see
/// [`Renderer::apply_list_item_per_line`]). Membership is resolved through
/// [`TagName::is_list_item`], the single owner of the `li-*` family.
fn is_list_tagged(iter: &gtk::TextIter) -> bool {
    iter.tags()
        .iter()
        .any(|t| t.name().is_some_and(|n| TagName::is_list_item(&n)))
}

impl Renderer {
    /// The SINGLE sink through which every **fixed** [`TagName`] is applied to the
    /// buffer (harvest N6). The name is resolved from the enum, never a string
    /// literal, so it can neither drift from nor typo against what `tags.rs`
    /// registered. The dynamic `fg-{rrggbb}` syntect colour tags are not fixed and
    /// keep their own `apply_tag_by_name` path in [`Self::insert_code_block`].
    pub(super) fn apply(&self, tag: TagName, si: &gtk::TextIter, ei: &gtk::TextIter) {
        // The ONE sanctioned fixed-tag `apply_tag_by_name` (clippy.toml bans the rest,
        // N6): the name is `TagName`-derived, so it cannot typo or drift from `tags.rs`.
        #[allow(clippy::disallowed_methods)]
        self.buf.apply_tag_by_name(tag.name(), si, ei);
    }

    pub(super) fn in_table_cell(&self) -> bool {
        self.table.as_ref().is_some_and(|t| t.in_cell)
    }

    /// The left inset (px, at this render's zoom) an anchored block child inherits from
    /// its enclosing list items and/or blockquote — the SAME left-margin the `li-{depth}`
    /// and `blockquote` tags apply in `tags.rs`, expressed relative to the view's content
    /// edge. The preview view bounds every anchored child (table/rule) to `content − 1`
    /// as if it started at that edge; a child nested in a list/quote actually starts this
    /// far right, so the bound must subtract it or the child overflows the viewport by
    /// exactly this many px → spurious Automatic h-scrollbar → GTK4Rs/AP-22/23 churn/blank
    /// (GTK4Rs/AP-23a). It is the TOTAL horizontal margin the block steals from the
    /// column, because the bound is `content − inset` against the FULL column width:
    /// - a **list** adds only a `left_margin` (`tags.rs` `li-{depth}` sets no right
    ///   margin), accumulating per level → `depth * list_step`;
    /// - a **blockquote** sets BOTH a left AND a right margin (`view_lm + depth*(bar+gap)`
    ///   and `view_rm + depth*(bar+gap)`, `tags.rs`), so it narrows the usable column on
    ///   BOTH sides → `2 * depth * (bar+gap)`. **It IS multiplied by depth**, and was not
    ///   until nested quotes gained their own indent (TDD 2.11b): this clause used to read
    ///   "it applies ONE tag regardless of nesting depth", which was true while every level
    ///   shared one margin and became silently wrong the moment they stopped. A stale inset
    ///   here does not fail loudly — it under-reserves, so an anchored child inside a
    ///   nested quote overflows by exactly the levels this forgot (GTK4Rs/AP-23a).
    ///
    /// Zero at top level, so callers can apply it unconditionally.
    pub(super) fn block_inset(&self) -> i32 {
        let theme = crate::theme::active();
        let m = &theme.metrics;
        // BOTH terms come from `tags::spec`, which is the single supplier of how far a
        // block's tag pushes its text — including WHERE each term rounds. That is the
        // whole fix: this used to sum the raw metrics and scale the total once, which is
        // a different number from what the tags apply (`px` rounds, so `px(a + b)` is not
        // `px(a) + px(b)` and `n * px(a)` is not `px(n * a)`), leaving the inset up to a
        // pixel short per level. One pixel is enough — the Automatic h-scrollbar appears
        // on `upper > page_size`, at any magnitude — so the child overflowed the viewport
        // and re-armed the GTK4Rs/AP-22/23 churn (ScrAP-23a, through the rounding rather
        // than through the indent).
        //
        // A list adds only a LEFT margin; a blockquote sets BOTH, so it costs twice.
        let list = crate::tags::list_indent_px(self.lists.len() as i32, self.zoom, m);
        // Clamped exactly as the tag family is, so the inset can never claim more
        // margin than `bq-{depth}` actually applies on a pathologically nested document.
        let quote_depth = (self.blockquote_depth as u8).min(crate::tags::MAX_QUOTE_DEPTH) as i32;
        list + 2 * crate::tags::quote_indent_px(quote_depth, self.zoom, m)
    }

    /// The buffer char offset this render will write at next — [`Renderer::tip`]'s
    /// offset, and therefore the region's cursor rather than the buffer's end once
    /// [`Renderer::write_at`] has pointed the render somewhere.
    ///
    /// `pub(crate)` rather than `pub(super)` because `preview::build` must record each
    /// event's buffer range from the same definition the renderer writes by. It read
    /// `buf.char_count()` — the whole buffer's length — which is the same number only
    /// while every render appends. A region render would have recorded every map entry
    /// at the document's end instead of at the splice.
    pub(crate) fn end_offset(&self) -> i32 {
        self.tip().offset()
    }

    /// Insert `text` at the buffer end, applying all currently active inline
    /// tags (plus heading / blockquote context tags).
    pub(super) fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let start = self.end_offset();
        let mut iter = self.tip();
        self.buf.insert(&mut iter, text);

        let apply = |tag: TagName| {
            let si = self.buf.iter_at_offset(start);
            let ei = self.tip();
            self.apply(tag, &si, &ei);
        };

        for tag in self.inline_tags.clone() {
            apply(tag);
        }
        if let Some(level) = self.heading {
            // Indexed by `theme::heading_slot`, the one definition of the h6→h5
            // fold, rather than re-deriving it here — this arm was one of five
            // hand-rolled copies. See TECH.md § "Heading levels (h1–h5; h6 folds
            // to h5)".
            apply(HEADING_TAGS[crate::theme::heading_slot(level as u8)]);
        }
        self.trailing_newlines = 0;
        self.at_start = false;
    }

    pub(super) fn newline(&mut self) {
        let mut iter = self.tip();
        self.buf.insert(&mut iter, "\n");
        self.trailing_newlines += 1;
        self.at_start = false;
    }

    /// Apply a margin tag (`blockquote`, or a `li-{depth}` list hanging-indent) to each
    /// logical line within `[start, end)`, EXCLUDING the terminating `\n` of every line.
    /// This deliberately leaves the newlines untagged so the btree does not coalesce the
    /// per-line applies into one continuous run: each line then carries its own tag
    /// on/off toggle, which forces GtkTextView to rebuild that line's style (and its
    /// left-margin/indent) instead of reusing the previous line's cached style — the fix
    /// for the dropped-margin artifact on toggle-free middle lines (GTK4Rs/AP-72).
    /// A continuous apply instead lets a wrapped/multi-line list item's continuation
    /// lines lose their left-margin and outdent left of the marker.
    pub(super) fn apply_tag_per_line(&self, tag: TagName, start: i32, end: i32) {
        let full = self.buf.slice(
            &self.buf.iter_at_offset(start),
            &self.buf.iter_at_offset(end),
            true,
        );
        for (si, ei) in logical_line_ranges(&full, start, end) {
            let s = self.buf.iter_at_offset(si);
            let e = self.buf.iter_at_offset(ei);
            self.apply(tag, &s, &e);
        }
    }

    /// Apply a list item's uniform per-level content-margin tags over `[start, end)`,
    /// PER logical line with the terminating `\n`s left untagged (ScrAP-72/GTK4Rs/AP-72).
    /// The item's FIRST logical line gets `li-{depth}` (carries the small inter-item
    /// `pixels_above_lines` gap); every LATER logical line gets `li-{depth}-cont` (no
    /// gap). Both variants carry the SAME `left_margin` and `indent = 0` — the marker is
    /// drawn in the gutter (Phase 2) and is not in the flow, so there is no hanging indent
    /// to keep reliable. They differ only in the inter-item gap, which makes the split
    /// GTK4Rs/AP-72-safe (a per-line style-cache mix-up can't change a margin that is identical in
    /// both variants).
    ///
    /// **Exactly ONE `li-*` tag per line.** `[start, end)` is the item's WHOLE span, which
    /// ENCLOSES any nested list it contains — so a naive pass would stack the outer item's
    /// `li-1` onto the nested item's `li-2`. That is why lines already carrying an `li-*`
    /// tag are skipped: an inner `TagEnd::Item` fires before its outer one, so a nested
    /// line is always tagged at its own (deeper) depth first, and the outer pass must leave
    /// it alone. The `li-{depth}` margins are ACCUMULATIVE (they add onto the container's,
    /// which is what nests a quoted list inside its blockquote — `tags.rs`, GTK4Rs/AP-96), so two stacked `li-*` tags would literally SUM: a depth-2 item would land at
    /// `28 + 56` instead of `56`, stranding its drawn marker far left of its own text. The
    /// old non-accumulative margins masked this — the deeper tag is added to the tag table
    /// later, so it simply won on priority and nesting worked by accident.
    ///
    /// With the flow-to-spaces workaround reverted, a
    /// multi-line item now has SEVERAL logical lines — each hard-broken source line, plus
    /// any loose continuation paragraph — and every one of them sits at the same content
    /// margin. Depth is clamped to `1..=MAX_LIST_DEPTH`, read from `tags.rs` rather
    /// than restated, so the clamp and the tag family cannot fall out of step.
    pub(super) fn apply_list_item_per_line(&self, depth: usize, start: i32, end: i32) {
        // Derived from the tag family's own bound, NOT a literal 6 (QA round 3,
        // P-4). The two were textually decoupled: this line said `6` while the
        // comment above said `MAX_LIST_DEPTH`, so lowering the constant compiled
        // clean and armed an out-of-bounds index in `TagName::name`.
        let depth = depth.clamp(1, crate::tags::MAX_LIST_DEPTH as usize);
        let full = self.buf.slice(
            &self.buf.iter_at_offset(start),
            &self.buf.iter_at_offset(end),
            true,
        );
        for (idx, (si, ei)) in logical_line_ranges(&full, start, end)
            .into_iter()
            .enumerate()
        {
            let s = self.buf.iter_at_offset(si);
            // A nested list's lines already carry their own, deeper `li-*` — leave them.
            if is_list_tagged(&s) {
                continue;
            }
            // The first logical line carries the inter-item gap (`li-{depth}`); later
            // lines share the same margin without it (`li-{depth}-cont`). `depth` is
            // clamped 1..=MAX_LIST_DEPTH above, so the variant is always registered.
            let tag = TagName::ListItem {
                depth: depth as u8,
                cont: idx != 0,
            };
            let e = self.buf.iter_at_offset(ei);
            self.apply(tag, &s, &e);
        }
    }

    /// Insert a blank line between top-level blocks (skipped at document start).
    pub(super) fn block_sep(&mut self) {
        if self.at_start {
            return;
        }
        for _ in self.trailing_newlines..2 {
            self.newline();
        }
    }

    pub(super) fn insert_code_block(&mut self, lang: &str, text: &str) {
        let text = text.trim_end_matches('\n');
        let (ss, ts) = syntect();
        let syntax = if lang.is_empty() {
            ss.find_syntax_plain_text()
        } else {
            ss.find_syntax_by_token(lang)
                .unwrap_or_else(|| ss.find_syntax_plain_text())
        };
        // Falls back to the first bundled theme if `self.syntect_theme` doesn't match
        // one by name. The inner `expect` relies on a non-local invariant: `ts` comes
        // from `ThemeSet::load_defaults()` (see `syntect()` above), which always
        // bundles a fixed, non-empty set of default themes — syntect's own contract,
        // not something this crate can violate.
        let theme = ts
            .themes
            .get(self.syntect_theme.as_str())
            .unwrap_or_else(|| {
                ts.themes
                    .values()
                    .next()
                    .expect("syntect::ThemeSet::load_defaults() always bundles >= 1 theme")
            });
        let mut hl = HighlightLines::new(syntax, theme);

        let block_start = self.end_offset();

        for line in text.split('\n') {
            // The block background is self-drawn by the preview view across the
            // whole block (GTK4Rs/AP-21), so empty lines need no filler space.
            let line_with_nl = format!("{line}\n");
            let ranges = hl.highlight_line(&line_with_nl, ss).unwrap_or_default();
            for (style, s) in ranges {
                let tok_start = self.end_offset();
                let mut iter = self.tip();
                self.buf.insert(&mut iter, s);
                let tok_end = self.end_offset();

                // NOTE: `code-block` is NOT applied per-token here. Each syntect token
                // string is `line_with_nl` (it INCLUDES the trailing '\n'), so a per-token
                // apply tags the newlines and coalesces the whole block into one continuous
                // `code-block` run — which is exactly the toggle-free-middle-line state that
                // drops the left margin (GTK4Rs/AP-72). The tag is instead applied ONCE below, per
                // logical line with the '\n's left untagged (`apply_tag_per_line`). Only the
                // per-token `fg-*` colour tags are applied in this loop.
                let r = style.foreground.r;
                let g = style.foreground.g;
                let b = style.foreground.b;
                let fg_name = format!("fg-{r:02x}{g:02x}{b:02x}");
                if self.buf.tag_table().lookup(&fg_name).is_none() {
                    let tag = TextTag::new(Some(&fg_name));
                    tag.set_foreground(Some(&format!("#{r:02x}{g:02x}{b:02x}")));
                    self.buf.tag_table().add(&tag);
                }
                let si = self.buf.iter_at_offset(tok_start);
                let ei = self.buf.iter_at_offset(tok_end);
                // Dynamic per-syntect-colour tag: NOT a fixed, enumerable name, so it
                // stays outside `TagName`/the typed sink and keeps its own apply (N6).
                #[allow(clippy::disallowed_methods)]
                self.buf.apply_tag_by_name(&fg_name, &si, &ei);
            }
        }

        // Apply the code-block tag (monospace family + text inset margins) PER LOGICAL
        // LINE — the terminating '\n's left untagged — NOT as one continuous run over
        // the whole block. `code-block` carries a left/right margin (`tags.rs`
        // set_left_margin/set_right_margin), and a paragraph-attribute margin tag applied
        // as one multi-paragraph range drops on the toggle-free MIDDLE lines: GTK's
        // one-line style cache (gtktextlayout.c get_style) reuses the previous line's
        // style and loses the left margin (GTK4Rs/AP-72 — same fix blockquote/list
        // use). A per-token `fg-{rrggbb}` toggle used to rescue it incidentally, but a
        // uniformly-highlighted block (unlanguaged fence, or a theme mapping every token
        // to one colour) has no such toggle. Range boundaries are unchanged:
        // [block_start, end_iter). The block's *background* is self-drawn by the preview
        // view — record the block's char extent for it (GTK4Rs/AP-21).
        let ei = self.tip();
        self.apply_tag_per_line(TagName::CodeBlock, block_start, ei.offset());
        self.code_blocks
            .push(crate::span::BufferSpan::new(block_start, ei.offset()));

        // 12 px top padding on the first line; 12 px bottom padding on the last.
        // pixels_above/below_lines expand the paragraph height, and paragraph_background
        // covers the expanded area, so the padding is filled with the block colour.
        let mut first_end = self.buf.iter_at_offset(block_start);
        first_end.forward_line();
        self.apply(
            TagName::CodeBlockTop,
            &self.buf.iter_at_offset(block_start),
            &first_end,
        );
        let mut last_start = self.tip();
        last_start.backward_line();
        self.apply(TagName::CodeBlockBottom, &last_start, &self.tip());

        // Each line was inserted with a trailing \n, so the block ends with one newline.
        self.trailing_newlines = 1;
        self.at_start = false;
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod code_block_per_line_tests {
    use super::Renderer;
    use gtk::prelude::*;

    /// GTK4Rs/AP-72 regression guard. The `code-block` margin tag must be applied PER LOGICAL
    /// LINE (each `\n` left untagged), so every line of the block carries its own tag
    /// toggle — otherwise GTK's one-line style cache drops the left margin on the
    /// toggle-free middle lines.
    ///
    /// The fence has NO language, so syntect uses plain-text highlighting and every line
    /// gets the SAME `fg-*` colour — there is no incidental per-token colour toggle to
    /// rescue the middle line. That isolates the assertion to the per-line `code-block`
    /// application under test. Three lines guarantees a genuine MIDDLE line.
    ///
    /// Mutation-test (GTK4Rs/AP-78): reverting `insert_code_block` to the single continuous
    /// `apply_tag_by_name("code-block", block_start..end)` makes this fail — only line 0
    /// would then start the tag, so `toggles` becomes `[true, false, false]`.
    #[gtktest::test]
    fn code_block_tag_toggles_on_at_each_line_start() {
        let buf = gtk::TextBuffer::new(None);
        // Register the tags `insert_code_block` applies. `code-block` carries a
        // left/right margin (as `tags.rs` sets it) so the per-line apply is realistic.
        let cb = gtk::TextTag::new(Some("code-block"));
        cb.set_left_margin(24);
        cb.set_right_margin(24);
        buf.tag_table().add(&cb);
        buf.tag_table()
            .add(&gtk::TextTag::new(Some("code-block-top")));
        buf.tag_table()
            .add(&gtk::TextTag::new(Some("code-block-bottom")));

        let mut r = Renderer::new(
            buf.clone(),
            crate::theme::active(),
            "InspiredGitHub".to_string(),
            None,
            false,
            String::new(),
            Vec::new(),
            1.0,
            crate::fold::FoldState::default(),
        );
        r.insert_code_block("", "alpha\nbeta\ngamma");

        let cb = buf.tag_table().lookup("code-block").unwrap();
        // Each block line's content start must TOGGLE `code-block` ON. Under one
        // continuous apply only line 0 toggles; the middle/last starts sit inside an
        // uninterrupted run and do NOT start the tag (the dropped-margin bug).
        let mut toggles = Vec::new();
        for line in 0..buf.line_count() {
            let it = buf.iter_at_line(line).unwrap();
            // Skip the trailing empty line (its start already ends the line — no content).
            if it.ends_line() {
                continue;
            }
            toggles.push(it.starts_tag(Some(&cb)));
        }
        assert_eq!(
            toggles,
            vec![true, true, true],
            "code-block must toggle ON at the start of EACH of the 3 block lines \
             (per-line application, GTK4Rs/AP-72), not once across the whole range"
        );
    }
}
