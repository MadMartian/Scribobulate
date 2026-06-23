//! Fenced code blocks: the Format-command wrap/unwrap ([`apply_code_block`]) and
//! the Enter-convenience lone-opening-fence auto-close ([`code_fence_close`], which
//! walks fence state via [`inside_fenced_block`]). See [`super`] for the rules.

use super::text::{self, block_span};
use super::Edit;

pub(super) fn apply_code_block(chars: &[char], start: usize, end: usize) -> Edit {
    let span = block_span(chars, start, end);
    // Code Block adds and removes whole LINES, so it rebuilds the span wholesale
    // instead of using the per-line `map_content` seam — but it still reads the
    // lines through that seam, so the fences it writes go INSIDE any blockquote
    // (`> ``` `) and a fenced block already sitting in one is still recognised.
    let lines = span.lines();

    let fenced = lines.len() >= 2
        && lines
            .first()
            .map(|l| l.content.trim_start().starts_with("```"))
            .unwrap_or(false)
        && lines
            .last()
            .map(|l| l.content.trim().starts_with("```"))
            .unwrap_or(false);

    if fenced {
        let inner = text::rejoin(&lines[1..lines.len() - 1]);
        return text::replace_block(&span, inner);
    }

    // The fence lines join the block inside its container, so they carry the first
    // spanned line's container prefix.
    let fence = format!("{}```", lines[0].prefix);
    let replacement = format!("{fence}\n{}\n{fence}", text::rejoin(&lines));
    // Select the WHOLE fenced block (fences included), exactly like every other
    // per-line block transform (`text::replace_block`: quote/list/heading/unwrap).
    // A prior version selected only the inner content ("```\n" = 4 chars in), but
    // then a re-apply's `block_span` — which expands to whole LINES, not across the
    // fence lines above/below — never re-saw the fences, so `fenced` stayed false
    // and Code Block NESTED a second fence instead of toggling off (TDD 10.5).
    // Keeping the fences in the selection is what makes the toggle symmetric.
    text::replace_block(&span, replacement)
}

/// A lone code fence to auto-close (see [`code_fence_close`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodeFenceClose {
    /// The fence line's leading indentation, repeated on the closing fence.
    pub(crate) indent: String,
    /// The backtick run itself (≥3 backticks), repeated as the closing delimiter.
    pub(crate) fence: String,
}

/// Whether `text` (the document content strictly ABOVE a given line) ends inside an
/// open fenced code block — i.e. an opening fence has not yet been matched by a
/// closing one. Walks CommonMark fences (backtick or tilde, ≥3): an opening fence
/// may carry an info string; a closing fence must be the same character, at least
/// as long, and bare (only whitespace after the run). Used to tell an **opening**
/// lone fence (auto-close it) from a **closing** one (don't) — [`code_fence_close`].
fn inside_fenced_block(text: &str) -> bool {
    let mut open: Option<(char, usize)> = None;
    for line in text.split('\n') {
        let after_indent = line.trim_start_matches([' ', '\t']);
        let Some(fc) = after_indent
            .chars()
            .next()
            .filter(|&c| c == '`' || c == '~')
        else {
            continue; // not a fence line
        };
        let run = after_indent.chars().take_while(|&c| c == fc).count();
        match open {
            // Outside: a run of ≥3 opens a block (info string allowed).
            None => {
                if run >= 3 {
                    open = Some((fc, run));
                }
            }
            // Inside: a matching, at-least-as-long, BARE fence closes it; any other
            // line (including a shorter/mismatched backtick run) is code content.
            Some((oc, ol)) => {
                let bare = after_indent.chars().skip(run).all(|c| c.is_whitespace());
                if fc == oc && run >= ol && bare {
                    open = None;
                }
            }
        }
    }
    open.is_some()
}

/// When `line` is a **lone OPENING code fence** — leading indentation, then a run of
/// ≥3 backticks, then only whitespace (no info string / language) — and the caret is
/// at or past the fence, return the pieces needed to drop a matching closing fence on
/// the line below (the caller inserts `"\n\n{indent}{fence}"` and leaves the caret on
/// the empty middle line). `None` otherwise.
///
/// `text_before` is the document content strictly above `line`'s buffer line: when it
/// ends inside an already-open fenced block, this lone fence is a *closing* delimiter,
/// not an opening one, so auto-close is suppressed (it would otherwise stack a spurious
/// closing fence — the bug this guard fixes).
///
/// Lazy (`impl FnOnce`, not `&str`): every other rejection below returns before this is
/// ever needed, and `line` failing to be a lone fence is the overwhelmingly common case
/// (an ordinary Enter). `text_before` is the WHOLE document above the caret's line, so a
/// caller building it eagerly on every keystroke pays that cost for a check almost never
/// taken (U-6). Only called once we already know `line` is a candidate.
pub(crate) fn code_fence_close(
    text_before: impl FnOnce() -> String,
    line: &str,
    cursor_col: usize,
) -> Option<CodeFenceClose> {
    let indent_len = line.chars().take_while(|&c| c == ' ' || c == '\t').count();
    let indent: String = line.chars().take(indent_len).collect();
    let after_indent: String = line.chars().skip(indent_len).collect();

    let fence: String = after_indent.chars().take_while(|&c| c == '`').collect();
    let fence_len = fence.chars().count();
    if fence_len < 3 {
        return None;
    }
    // "Alone": nothing but whitespace after the backtick run (no language tag).
    if !after_indent
        .chars()
        .skip(fence_len)
        .all(|c| c.is_whitespace())
    {
        return None;
    }
    if cursor_col < indent_len + fence_len {
        return None;
    }
    // This lone fence CLOSES an already-open block → not an opening fence; leave it.
    if inside_fenced_block(&text_before()) {
        return None;
    }
    Some(CodeFenceClose { indent, fence })
}

#[cfg(test)]
mod tests {
    // `CodeFenceClose` is module-private (not re-exported at `crate::format`, as no
    // external caller names it), so reach it — and the fns under test — via `super`.
    use super::*;
    use crate::format::{applied, FormatCmd};

    #[test]
    fn code_block_fences_the_spanned_lines() {
        let (text, s, e) = applied(FormatCmd::CodeBlock, "let x = 1;", 0, 10);
        assert_eq!(text, "```\nlet x = 1;\n```");
        // Selection covers the WHOLE fenced block (fences included) so a re-apply
        // toggles it back off — see `code_block_toggles_off_on_reapply`.
        assert_eq!((s, e), (0, 18));
    }

    #[test]
    fn code_block_toggles_off_on_reapply() {
        // Apply, then feed the resulting text + auto-selection back in: the block
        // must unwrap (toggle off), not nest a second fence (TDD 10.5 regression).
        let (text, s, e) = applied(FormatCmd::CodeBlock, "let x = 1;", 0, 10);
        assert_eq!((s, e), (0, 18));
        let (text2, _s2, _e2) = applied(FormatCmd::CodeBlock, &text, s, e);
        assert_eq!(text2, "let x = 1;");
    }

    #[test]
    fn code_block_unwraps_when_already_fenced() {
        let (text, _s, _e) = applied(FormatCmd::CodeBlock, "```\ncode\n```", 0, 12);
        assert_eq!(text, "code");
    }

    #[test]
    fn code_block_fences_inside_a_blockquote_stay_in_the_quote() {
        // A bare `` ``` `` fence at column 0 would end the quote and swallow the `> `
        // as code text; the fences take the block's container prefix instead.
        let (text, s, e) = applied(FormatCmd::CodeBlock, "> code", 2, 2);
        assert_eq!(text, "> ```\n> code\n> ```");
        // …and the round trip still toggles off (the detector reads the content).
        let (text2, _s2, _e2) = applied(FormatCmd::CodeBlock, &text, s, e);
        assert_eq!(text2, "> code");
    }

    #[test]
    fn code_fence_close_detects_a_lone_opening_fence() {
        // A bare "```" opening a block (nothing above) → close with the same run.
        assert_eq!(
            code_fence_close(String::new, "```", 3),
            Some(CodeFenceClose {
                indent: String::new(),
                fence: "```".into()
            })
        );
        // Indentation and a longer backtick run are both mirrored.
        assert_eq!(
            code_fence_close(String::new, "  ````", 6),
            Some(CodeFenceClose {
                indent: "  ".into(),
                fence: "````".into()
            })
        );
        // Prose above doesn't open a block, so this is still an opening fence.
        assert_eq!(
            code_fence_close(|| "hello\n".to_string(), "```", 3),
            Some(CodeFenceClose {
                indent: String::new(),
                fence: "```".into()
            })
        );
    }

    #[test]
    fn code_fence_close_leaves_a_closing_fence_alone() {
        // Above us an opening fence + content is already open → this bare "```"
        // CLOSES the block; do NOT stack another fence (the reported bug).
        assert_eq!(
            code_fence_close(|| "```\ncode\n".to_string(), "```", 3),
            None
        );
        // Directly after an opening fence (empty block) → still a closing fence.
        assert_eq!(code_fence_close(|| "```\n".to_string(), "```", 3), None);
        // After a balanced block, a fresh lone fence opens a NEW block → auto-close.
        assert_eq!(
            code_fence_close(|| "```\ncode\n```\n".to_string(), "```", 3),
            Some(CodeFenceClose {
                indent: String::new(),
                fence: "```".into()
            })
        );
    }

    #[test]
    fn code_fence_close_ignores_non_fences() {
        // Fewer than three backticks is not a fence. `text_before` must not even be
        // FORCED here — a panicking closure proves the lazy short-circuit still
        // holds for every one of these early rejections.
        let unreachable = || -> String { panic!("text_before forced despite an early reject") };
        assert_eq!(code_fence_close(unreachable, "``", 2), None);
        // A language/info string means it is not "alone".
        assert_eq!(code_fence_close(unreachable, "```rust", 7), None);
        // Caret inside the backtick run → normal newline.
        assert_eq!(code_fence_close(unreachable, "```", 1), None);
        // Not a fence at all.
        assert_eq!(code_fence_close(unreachable, "text", 4), None);
    }
}
