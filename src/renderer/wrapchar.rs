//! The paragraph-level wrap decision for GTK below 4.7: which paragraphs of a render get
//! the `wrap-char` tag. The mechanism, the trade-off and the derivation of the limit are
//! in [`crate::tags`]'s `wrap` module; this is the one place that applies it, called by
//! both render routes (the full walk and a region walk) once every margin and scale tag
//! is on, so the width a paragraph really has is readable from the buffer.

use super::Renderer;
use crate::tags::TagName;
use gtk::prelude::*;

/// The desktop's body font size in pixels: `gtk-font-name`'s size at `gtk-xft-dpi`.
/// 16 px (12 pt at 96 dpi, larger than any default) when there is no display to ask, so
/// the answer errs toward switching a paragraph rather than letting a run overflow.
fn body_em_px() -> f64 {
    const FALLBACK: f64 = 16.0;
    let Some(settings) = gtk::Settings::default() else {
        return FALLBACK;
    };
    let points = settings
        .gtk_font_name()
        .map(|name| gtk::pango::FontDescription::from_string(&name))
        .map(|d| f64::from(d.size()) / f64::from(gtk::pango::SCALE))
        .filter(|p| *p > 0.0)
        .unwrap_or(12.0);
    let dpi = match settings.gtk_xft_dpi() {
        d if d > 0 => f64::from(d) / 1024.0,
        _ => 96.0,
    };
    (points * dpi / 72.0).max(1.0)
}

impl Renderer {
    /// Give every paragraph in `[from, to)` that holds a run too wide to be sure of
    /// fitting the narrowest pane the `wrap-char` tag, when this GTK needs it. Code
    /// blocks are skipped: they keep their own mode.
    pub(crate) fn mark_wide_runs(&self, from: i32, to: i32) {
        if !crate::tags::wrap_policy().char_for_wide_runs {
            return;
        }
        let cfg = &crate::config::config().view;
        let default_left = f64::from(crate::theme::px(cfg.left_margin, self.zoom));
        let default_right = f64::from(crate::theme::px(cfg.right_margin, self.zoom));
        let em = body_em_px() * self.zoom;
        let pane = f64::from(crate::window::MIN_WINDOW_WIDTH);

        // From the start of the line `from` falls in: a region begins where the summary
        // line it belongs to is still open, and a paragraph is decided as a whole.
        let stale = self.buf.tag_table().lookup(TagName::WrapChar.name());
        let mut line = self.buf.iter_at_offset(from);
        line.set_line_offset(0);
        while line.offset() < to {
            // A paragraph the region only partly rewrote (a summary line whose collapsed
            // preview came or went) may carry a mark decided against its old text.
            if let Some(tag) = &stale {
                let mut first = line;
                if first.forward_char() {
                    self.buf.remove_tag(tag, &line, &first);
                }
            }
            let mut end = line;
            if !end.ends_line() {
                end.forward_to_line_end();
            }
            if end.offset() > line.offset() {
                self.mark_line(&line, &end, em, pane, default_left, default_right);
            }
            if !line.forward_line() {
                break;
            }
        }
    }

    fn mark_line(
        &self,
        line: &gtk::TextIter,
        end: &gtk::TextIter,
        em: f64,
        pane: f64,
        default_left: f64,
        default_right: f64,
    ) {
        let (mut left, mut right, mut scale) = (default_left, default_right, 1.0);
        for tag in line.tags() {
            if tag
                .name()
                .is_some_and(|n| TagName::is_code_block(n.as_str()))
            {
                return;
            }
            // Ascending priority, so the last tag to set a property wins, as in layout.
            if tag.is_left_margin_set() {
                left = f64::from(tag.left_margin());
            }
            if tag.is_right_margin_set() {
                right = f64::from(tag.right_margin());
            }
            if tag.is_scale_set() {
                scale = tag.scale();
            }
        }
        // An anchored child is one U+FFFC; it breaks like a space for this purpose.
        let text = self.buf.slice(line, end, true).replace('\u{FFFC}', " ");
        let limit = crate::tags::run_limit(pane - left - right, em * scale);
        if crate::tags::longest_unbreakable_run(&text) > limit {
            let mut first_end = *line;
            first_end.forward_char();
            self.apply(TagName::WrapChar, line, &first_end);
        }
    }
}
