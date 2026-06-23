//! Shared char-offset text helpers for the block formatters.
//!
//! Everything works on a `&[char]` (buffer offsets stay char-based, never bytes)
//! and locates line boundaries around a selection.  The five block formatters
//! ([`super::heading`], [`super::quote`], [`super::list`], [`super::codeblock`])
//! all begin the same way — resolve the whole-line span covering the selection,
//! split it into lines, rebuild it — so that boilerplate is factored out here as
//! [`block_span`] and [`replace_block`].
//!
//! ## The container-prefix seam
//!
//! A spanned line is never handed to a transform whole: [`BlockSpan::lines`]
//! splits each one into its **container prefix** (the blockquote nesting) and its
//! **content**, and the transforms rewrite exactly one half —
//! [`map_content`] for the marker transforms (heading, the three list kinds) and
//! [`map_prefix`] for the one transform that owns the container itself (Quote).
//! That split is what puts a marker *inside* its container (`> ### Heading`,
//! never `### > Heading`) and what lets the toggle-off detectors see a marker
//! sitting behind a `> `.
//!
//! The seam is **sealed, not merely offered**: `BlockSpan::text` is private to
//! this module, so no formatter — present or future — can reach the raw lines and
//! re-acquire the blind spot.  Every block transform therefore inherits the
//! container-awareness instead of re-implementing it (four per-formatter copies
//! of one rule is exactly how the family shipped broken — ScrAP-194).

use super::quote::quote_prefix_len;
use super::Edit;

/// Collect the chars `[a, b)` into a `String`.
pub(super) fn slice(chars: &[char], a: usize, b: usize) -> String {
    chars[a..b].iter().collect()
}

/// Char offset of the start of the line containing `pos`.
pub(super) fn line_start_of(chars: &[char], pos: usize) -> usize {
    let mut i = pos.min(chars.len());
    while i > 0 && chars[i - 1] != '\n' {
        i -= 1;
    }
    i
}

/// Char offset of the end of the line containing `pos`, exclusive of the
/// trailing newline.
pub(super) fn line_end_of(chars: &[char], pos: usize) -> usize {
    let mut i = pos.min(chars.len());
    while i < chars.len() && chars[i] != '\n' {
        i += 1;
    }
    i
}

/// When a selection ends exactly past a newline, don't drag the following line
/// into a block transform.
pub(super) fn trim_trailing_newline(chars: &[char], start: usize, end: usize) -> usize {
    if end > start && end > 0 && end <= chars.len() && chars[end - 1] == '\n' {
        end - 1
    } else {
        end
    }
}

/// One spanned line, split at its **container prefix**.
///
/// `prefix` is the blockquote nesting captured verbatim (`"> "`, `">> "`,
/// `"> > "`, …; empty on an unquoted line) and `content` is everything after it —
/// the part a block transform is allowed to rewrite.
///
/// "Container prefix" is read narrowly: **blockquote nesting only**.  A list
/// marker is deliberately *not* one, so Heading on `- item` still yields
/// `## - item`, unchanged.
pub(super) struct BlockLine<'a> {
    pub(super) prefix: &'a str,
    pub(super) content: &'a str,
}

impl<'a> BlockLine<'a> {
    fn split(line: &'a str) -> BlockLine<'a> {
        // `>` and its optional space are ASCII, so this byte split is char-safe.
        let (prefix, content) = line.split_at(quote_prefix_len(line));
        BlockLine { prefix, content }
    }

    /// This line rebuilt with `content` behind its own container prefix.
    fn with_content(&self, content: &str) -> String {
        format!("{}{}", self.prefix, content)
    }
}

/// The whole-line span covering a selection: the `[start, end)` char offsets of
/// the spanned lines (trailing newline trimmed) plus the text between them.
/// Every block formatter opens by resolving this, then reaches the lines through
/// [`BlockSpan::lines`] / [`map_content`] / [`map_prefix`].
pub(super) struct BlockSpan {
    pub(super) start: usize,
    pub(super) end: usize,
    /// Private on purpose — see the module docs' "container-prefix seam".  Handing
    /// out the raw text is what let every per-line transform prepend its marker in
    /// front of a `> `; the only ways in are prefix-aware.
    text: String,
}

impl BlockSpan {
    /// The spanned lines, each split into container prefix + content.
    pub(super) fn lines(&self) -> Vec<BlockLine<'_>> {
        self.text.split('\n').map(BlockLine::split).collect()
    }

    /// The first spanned line — what every toggle-off detector keys on (the
    /// first line decides on/off for the whole block).
    pub(super) fn first_line(&self) -> BlockLine<'_> {
        // `str::split` always yields at least one item, so this cannot be empty.
        BlockLine::split(self.text.split('\n').next().unwrap_or(""))
    }
}

/// Resolve the [`BlockSpan`] covering the selection `[sel_start, sel_end)`.
pub(super) fn block_span(chars: &[char], sel_start: usize, sel_end: usize) -> BlockSpan {
    let start = line_start_of(chars, sel_start);
    let end = line_end_of(chars, trim_trailing_newline(chars, sel_start, sel_end));
    let text = slice(chars, start, end);
    BlockSpan { start, end, text }
}

/// Rewrite every spanned line's **content**, re-attaching each line's container
/// prefix verbatim: `f` receives `(line_index, content)` and returns the new
/// content.  The seam for the marker transforms — heading and the three list
/// kinds — which is why they cannot put their marker in front of a `> `.
pub(super) fn map_content(span: &BlockSpan, mut f: impl FnMut(usize, &str) -> String) -> Edit {
    let block = span
        .lines()
        .iter()
        .enumerate()
        .map(|(i, line)| line.with_content(&f(i, line.content)))
        .collect::<Vec<_>>()
        .join("\n");
    replace_block(span, block)
}

/// Rewrite every spanned line's **container prefix**, leaving its content
/// untouched: `f` receives the current prefix and returns the new one.  The
/// mirror seam, for the one transform that owns the container itself (Quote —
/// which adds or removes a nesting level).
pub(super) fn map_prefix(span: &BlockSpan, mut f: impl FnMut(&str) -> String) -> Edit {
    let block = span
        .lines()
        .iter()
        .map(|line| format!("{}{}", f(line.prefix), line.content))
        .collect::<Vec<_>>()
        .join("\n");
    replace_block(span, block)
}

/// Re-join whole lines (prefix + content) — for the one block transform that
/// rebuilds the span wholesale rather than line-for-line (Code Block, which adds
/// and removes fence lines and so cannot use [`map_content`]).
pub(super) fn rejoin(lines: &[BlockLine<'_>]) -> String {
    lines
        .iter()
        .map(|line| line.with_content(line.content))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build the [`Edit`] that replaces a whole [`BlockSpan`] with `replacement` and
/// selects the entire replacement — the common tail of every per-line block
/// transform (heading / quote / bulleted / numbered / code-block-unwrap).
pub(super) fn replace_block(span: &BlockSpan, replacement: String) -> Edit {
    let n = replacement.chars().count();
    Edit {
        start: span.start,
        end: span.end,
        replacement,
        sel_start: span.start,
        sel_end: span.start + n,
    }
}
