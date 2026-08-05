//! Pure Markdown formatting transforms for the Format menu / toolbar / overlay.
//!
//! Everything here is GTK-free: it operates on a plain `&str` plus a char-offset
//! selection range and returns a minimal [`Edit`] (one replaced range plus the
//! post-edit selection, in char offsets).  The window layer translates char
//! offsets to `GtkTextIter`s, applies the edit inside a single user-action
//! (for one-step undo), and reselects.  Keeping the wrap/toggle rules pure makes
//! them unit-testable without a display — and the tests count toward the POLICY
//! coverage gate (this module is deliberately not in the gate's ignore list).
//!
//! ## Command semantics
//!
//! * **Inline** (Bold `**`, Italic `*`, Strikethrough `~~`, Code Span `` ` ``):
//!   wrap the selection in the marker.  If the selection is *already* wrapped —
//!   either the markers are inside the selection or immediately outside it — the
//!   command toggles them off instead.  An empty selection inserts an empty pair
//!   and parks the caret between the markers.
//! * **Heading(n)** (1..=6): a block prefix applied to every line the selection
//!   spans.  Any existing `#`-prefix is stripped first; if the first spanned line
//!   was already exactly heading `n`, the command toggles the heading off.
//! * **Code Block**: fence the spanned lines with ```` ``` ```` lines, or unwrap
//!   if they are already fenced.
//! * **Quote**: prefix every spanned line with `> `, or strip the prefix if the
//!   first spanned line is already quoted.
//! * **BulletedList / NumberedList / TaskList**: prefix every spanned line with a
//!   list marker (`- ` for bulleted; `1. `, `2. `… — renumbered from 1 — for
//!   numbered; `- [ ] ` for a GFM task/checkbox list), or strip the marker if the
//!   first spanned line is already that kind of list item.
//! * **Horizontal Rule**: insert a `---` thematic break on its own line.  This is
//!   an insertion, not a wrap, so it has no toggle.
//!
//! ## Block commands inside a container
//!
//! Every block command works on a line's **content**, not on the raw line: a
//! blockquote prefix (`> `, `>> `, `> > `) is held back by the block-span seam
//! ([`text::BlockSpan::lines`]) and re-attached afterwards.  So a marker always
//! lands *inside* the quote (`> ### Heading`, `> - item`), and the toggle-off
//! detectors still recognise a marker that sits behind one.  Quote itself is the
//! mirror case — it owns the container prefix, so it rewrites that half instead,
//! nesting (`> a` → `> > a`) or peeling one level.  See [`text`] for why the seam
//! is sealed rather than merely available.
//!
//! ## File layout
//!
//! The transforms are split by Markdown concern so each stays small and directly
//! unit-testable (the tests live beside the code they cover; every submodule is in
//! the coverage gate):
//!
//! * [`text`] — shared char-offset helpers ([`text::block_span`], the
//!   container-prefix seam [`text::map_content`] / [`text::map_prefix`],
//!   [`text::replace_block`], line-boundary lookups) reused by every block formatter.
//! * [`inline`] — the inline wrap/toggle core (`apply_inline`).
//! * [`heading`] — ATX heading prefix/toggle.
//! * [`quote`] — blockquote prefix/toggle (and the shared `quote_prefix` parser).
//! * [`list`] — bulleted and numbered list prefix/toggle (and the shared bullet /
//!   ordered marker parsers).
//! * [`codeblock`] — fenced-code-block wrap/unwrap plus the Enter-convenience
//!   `code_fence_close` (with the open-fence walker `inside_fenced_block`).
//! * [`hr`] — thematic-break insertion.
//! * [`continuation`] — the Enter-convenience list/blockquote `line_continuation`.
//! * [`insert`] — GTK-free link/image/table markup builders and their parsers,
//!   plus `link_target_at` (the destination of the link under the caret, which
//!   gates and feeds Edit ▸ Copy Link Location).
//!
//! `FormatCmd`, the [`Edit`] result type, and the [`apply`] dispatcher stay here;
//! the crate-level API (`format::apply`, `format::line_continuation`, …) is
//! re-exported so call sites are unchanged.

mod codeblock;
mod continuation;
mod heading;
mod hr;
mod inline;
mod insert;
mod list;
mod quote;
mod text;

pub(crate) use codeblock::code_fence_close;
pub(crate) use continuation::{line_continuation, LineContinuation};
pub(crate) use insert::{
    image_markup, link_markup, link_target_at, parse_image, parse_link, table_markup,
};

/// A formatting command.  `Heading` carries its tier (1..=6, validated by the
/// caller via the `win.format::h{n}` action target).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormatCmd {
    Bold,
    Italic,
    Strikethrough,
    CodeSpan,
    Superscript,
    Subscript,
    Highlight,
    Heading(u8),
    CodeBlock,
    Quote,
    BulletedList,
    NumberedList,
    TaskList,
    HorizontalRule,
}

impl FormatCmd {
    /// Parse the string target of the `win.format` action (e.g. `"bold"`,
    /// `"h3"`).  Returns `None` for anything unrecognised.
    pub(crate) fn from_target(target: &str) -> Option<FormatCmd> {
        Some(match target {
            "bold" => FormatCmd::Bold,
            "italic" => FormatCmd::Italic,
            "strike" => FormatCmd::Strikethrough,
            "code-span" => FormatCmd::CodeSpan,
            "sup" => FormatCmd::Superscript,
            "sub" => FormatCmd::Subscript,
            "highlight" => FormatCmd::Highlight,
            "code-block" => FormatCmd::CodeBlock,
            "quote" => FormatCmd::Quote,
            "bulleted-list" => FormatCmd::BulletedList,
            "numbered-list" => FormatCmd::NumberedList,
            "task-list" => FormatCmd::TaskList,
            "hr" => FormatCmd::HorizontalRule,
            _ => match target.strip_prefix('h').and_then(|n| n.parse::<u8>().ok()) {
                Some(n @ 1..=6) => FormatCmd::Heading(n),
                _ => return None,
            },
        })
    }

    /// The marker string for inline commands; `None` for block commands.
    fn inline_marker(self) -> Option<&'static str> {
        match self {
            FormatCmd::Bold => Some("**"),
            FormatCmd::Italic => Some("*"),
            FormatCmd::Strikethrough => Some("~~"),
            FormatCmd::CodeSpan => Some("`"),
            // Single-char wraps: `^x^` superscript, `~x~` subscript (pulldown-cmark
            // 0.13 ENABLE_SUPERSCRIPT/SUBSCRIPT). `~` is one tilde — `~~` stays
            // strikethrough, which the parser resolves first, so they don't collide.
            FormatCmd::Superscript => Some("^"),
            FormatCmd::Subscript => Some("~"),
            // `==highlight==` (mark). No pulldown-cmark option parses `==text==` at
            // all, so the renderer tokenises it itself in `scan_scripts`, exactly like
            // the tight scripts and `~~strike~~`. Wrap/toggle is otherwise identical to
            // every other inline marker.
            FormatCmd::Highlight => Some("=="),
            _ => None,
        }
    }
}

/// A minimal edit: replace the original-text char range `[start, end)` with
/// `replacement`, then select `[sel_start, sel_end)` in the resulting text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edit {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) replacement: String,
    pub(crate) sel_start: usize,
    pub(crate) sel_end: usize,
}

/// Compute the [`Edit`] for `cmd` over `text` with the selection `[sel_start,
/// sel_end)` (char offsets; order-insensitive — a reversed range is normalised).
pub(crate) fn apply(cmd: FormatCmd, text: &str, sel_start: usize, sel_end: usize) -> Edit {
    let chars: Vec<char> = text.chars().collect();
    let (mut start, mut end) = if sel_start <= sel_end {
        (sel_start, sel_end)
    } else {
        (sel_end, sel_start)
    };
    start = start.min(chars.len());
    end = end.min(chars.len());

    match cmd {
        FormatCmd::Bold
        | FormatCmd::Italic
        | FormatCmd::Strikethrough
        | FormatCmd::CodeSpan
        | FormatCmd::Superscript
        | FormatCmd::Subscript
        | FormatCmd::Highlight => {
            inline::apply_inline(cmd.inline_marker().unwrap(), &chars, start, end)
        }
        FormatCmd::Heading(level) => heading::apply_heading(level, &chars, start, end),
        FormatCmd::CodeBlock => codeblock::apply_code_block(&chars, start, end),
        FormatCmd::Quote => quote::apply_quote(&chars, start, end),
        FormatCmd::BulletedList => list::apply_bulleted_list(&chars, start, end),
        FormatCmd::NumberedList => list::apply_numbered_list(&chars, start, end),
        FormatCmd::TaskList => list::apply_task_list(&chars, start, end),
        FormatCmd::HorizontalRule => hr::apply_hr(&chars, start, end),
    }
}

/// Reconstruct the full post-edit document, so assertions across the submodules'
/// tests can read naturally (shared by every `apply`-based test module).
#[cfg(test)]
pub(crate) fn applied(cmd: FormatCmd, text: &str, s: usize, e: usize) -> (String, usize, usize) {
    let edit = apply(cmd, text, s, e);
    let chars: Vec<char> = text.chars().collect();
    let mut out: String = chars[..edit.start].iter().collect();
    out.push_str(&edit.replacement);
    out.extend(&chars[edit.end..]);
    (out, edit.sel_start, edit.sel_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_target_parses_every_command_and_rejects_junk() {
        assert_eq!(FormatCmd::from_target("bold"), Some(FormatCmd::Bold));
        assert_eq!(
            FormatCmd::from_target("code-block"),
            Some(FormatCmd::CodeBlock)
        );
        assert_eq!(
            FormatCmd::from_target("hr"),
            Some(FormatCmd::HorizontalRule)
        );
        assert_eq!(FormatCmd::from_target("quote"), Some(FormatCmd::Quote));
        assert_eq!(
            FormatCmd::from_target("bulleted-list"),
            Some(FormatCmd::BulletedList)
        );
        assert_eq!(
            FormatCmd::from_target("numbered-list"),
            Some(FormatCmd::NumberedList)
        );
        assert_eq!(
            FormatCmd::from_target("task-list"),
            Some(FormatCmd::TaskList)
        );
        assert_eq!(FormatCmd::from_target("sup"), Some(FormatCmd::Superscript));
        assert_eq!(FormatCmd::from_target("sub"), Some(FormatCmd::Subscript));
        assert_eq!(
            FormatCmd::from_target("highlight"),
            Some(FormatCmd::Highlight)
        );
        assert_eq!(FormatCmd::from_target("h1"), Some(FormatCmd::Heading(1)));
        assert_eq!(FormatCmd::from_target("h6"), Some(FormatCmd::Heading(6)));
        assert_eq!(FormatCmd::from_target("h0"), None);
        assert_eq!(FormatCmd::from_target("h7"), None);
        assert_eq!(FormatCmd::from_target("nope"), None);
    }

    #[test]
    fn reversed_selection_range_is_normalised() {
        let (text, _s, _e) = applied(FormatCmd::Bold, "hello world", 11, 6);
        assert_eq!(text, "hello **world**");
    }
}
