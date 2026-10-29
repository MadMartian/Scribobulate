//! Where a new annotation may be written without changing what the file means to
//! any other Markdown tool (TDD 17.59). Pure, GTK-free.
//!
//! Scribobulate strips CriticMarkup before it parses, so nothing written here can
//! ever look wrong in its own preview. Every other tool — GitHub, a static-site
//! generator, another editor — reads the file as it is, and to them `{==`, `==}` and
//! `{>>…<<}` are ordinary text. Ordinary text is harmless inside a paragraph, a
//! heading, a list item or a quote. It is not harmless in four places:
//!
//! - **Front matter** is only front matter while its opening fence is line one and
//!   nothing but metadata sits between its fences.
//! - **A code block** — fenced or indented — ends at its closing fence. `{==` ahead
//!   of the opening fence turns the code into a paragraph, and text after the closing
//!   fence un-closes it, so the block swallows the rest of the file.
//! - **An HTML block** runs on to the next blank line, so a comment written directly
//!   after it, and the paragraph after that, are read as raw HTML.
//! - **Math** (`$…$`, `$$…$$`) is read verbatim by a math renderer, which fails on
//!   markup inside the formula.
//! - **An inline code span** is read verbatim too: markup inside one is shown as code
//!   by every tool, this one included (`annotate::scan` reads CriticMarkup in code as
//!   literal, TDD 17.61), so the annotation would vanish into the code it was meant
//!   to mark.
//!
//! So the first three are never written into: an annotation inside one becomes a
//! comment on a line of its own just after the block, and the block keeps every byte.
//! Math and code spans are never split: a highlight touching one widens to the whole
//! span, delimiters included, and a point comment inside one moves to its end.
//!
//! **Why this parses the file a second time** (POLICY § Architecture rules: a
//! parallel path says why). The renderer's parse cannot answer the question: it reads
//! the text with CriticMarkup already stripped, without front matter, and without
//! math, which the preview shows as plain text. This scan reads the raw file with the
//! extensions other tools commonly enable, because those tools are who it protects —
//! the same set `annotate::scan` reads code through (`NormalizedMd::parse_as_written`),
//! so the two never disagree about what is code.

use pulldown_cmark::{Event, Tag};
use std::ops::Range;

/// Where a new annotation is written, once the places it must not touch are kept out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Landing {
    /// Wrap this byte range as `{==…==}{>>…<<}`.
    Highlight(Range<usize>),
    /// Write a point comment here instead.
    Point(Slot),
}

/// A point comment's place: `before`, then `{>>comment<<}`, then `after`, inserted at
/// byte offset `at`. A bare point comment has both empty; one written after a block
/// carries the line breaks and container prefix that put it on a line of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Slot {
    pub(crate) at: usize,
    pub(crate) before: String,
    pub(crate) after: String,
}

impl Slot {
    pub(crate) fn bare(at: usize) -> Self {
        Slot {
            at,
            before: String::new(),
            after: String::new(),
        }
    }

    /// A bare comment inside running text, as opposed to one on a line of its own.
    pub(crate) fn is_bare(&self) -> bool {
        self.before.is_empty() && self.after.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    FrontMatter,
    Code,
    IndentedCode,
    Html,
}

/// A region no annotation is written into, in whole lines: from the start of its
/// first line to just past its last line's terminator (or the end of the file).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    kind: Kind,
    lines: Range<usize>,
}

struct Verbatim {
    blocks: Vec<Block>,
    /// Math and inline code spans, in document order, which are kept whole rather
    /// than kept out.
    whole: Vec<Range<usize>>,
}

fn line_start(s: &str, at: usize) -> usize {
    s[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end_after(s: &str, at: usize) -> usize {
    s[at..].find('\n').map_or(s.len(), |i| at + i + 1)
}

/// The line ending the file uses around `at`, so an inserted line break matches its
/// neighbours: CRLF documents are kept as written (TDD 1.10), never left mixed.
fn eol_near(source: &str, at: usize) -> &'static str {
    let after = source[at..].find('\n').map(|i| at + i);
    let before = source[..at].rfind('\n');
    match after.or(before) {
        Some(nl) if nl > 0 && source.as_bytes()[nl - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

fn is_blank(line: &str) -> bool {
    line.trim_matches([' ', '\t', '>', '\r', '\n']).is_empty()
}

fn scan(source: &str) -> Verbatim {
    let mut blocks = Vec::new();
    let mut whole = Vec::new();
    let body_from = match crate::renderer::frontmatter::scan(source) {
        Some(fm) => {
            blocks.push(Block {
                kind: Kind::FrontMatter,
                lines: 0..fm.end(),
            });
            fm.end()
        }
        None => 0,
    };
    // Through the inline-tab pre-pass like every other parse site (ScrAP-75): it is
    // length-preserving and leaves leading tabs alone, so every offset and every code
    // block's indent read here are the file's own.
    let body = crate::renderer::NormalizedMd::new(&source[body_from..]);
    for (ev, r) in body.parse_as_written().into_offset_iter() {
        let r = r.start + body_from..r.end + body_from;
        let kind = match ev {
            Event::Start(Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(_))) => Kind::Code,
            Event::Start(Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Indented)) => {
                Kind::IndentedCode
            }
            Event::Start(Tag::HtmlBlock) => Kind::Html,
            Event::InlineMath(_) | Event::DisplayMath(_) | Event::Code(_) => {
                whole.push(r);
                continue;
            }
            // dispatch-selector: this picks out the few constructs other tools read verbatim; every other event is ordinary text to them, where an annotation is harmless
            _ => continue,
        };
        if r.is_empty() {
            continue;
        }
        let end = if source[..r.end].ends_with('\n') {
            r.end
        } else {
            line_end_after(source, r.end)
        };
        blocks.push(Block {
            kind,
            lines: line_start(source, r.start)..end,
        });
    }
    Verbatim { blocks, whole }
}

/// The comment on a line of its own just after `block`, in the block's container (a
/// quote's `>`, a list item's indent), so it neither joins the block nor ends the
/// container.
fn after_block(source: &str, block: &Block) -> Slot {
    let at = block.lines.end;
    let ends_in_newline = source[..at].ends_with('\n');
    let last_line =
        &source[line_start(source, at.saturating_sub(usize::from(ends_in_newline)))..at];
    let lead = last_line.len() - last_line.trim_start_matches([' ', '\t', '>']).len();
    let mut prefix = last_line[..lead].to_string();
    if block.kind == Kind::IndentedCode {
        // The last four columns are the code's own indent, not the container's.
        let spaces = prefix.len() - prefix.trim_end_matches(' ').len();
        prefix.truncate(prefix.len() - spaces.min(4));
    }
    if block.kind == Kind::FrontMatter {
        prefix.clear();
    }
    let blank = prefix.trim_end();
    let eol = eol_near(source, at.saturating_sub(1));

    let mut before = String::new();
    if !ends_in_newline {
        before.push_str(eol);
    }
    if block.kind == Kind::Html {
        // An HTML block runs on to the next blank line; without one the comment would
        // be read as part of it.
        before.push_str(blank);
        before.push_str(eol);
    }
    before.push_str(&prefix);

    let mut after = String::from(eol);
    // A line of only `-` or `=` straight after would make the comment a heading.
    let next = &source[at..line_end_after(source, at)];
    let next = next.trim_start_matches([' ', '\t', '>']).trim_end();
    if !next.is_empty() && (next.chars().all(|c| c == '-') || next.chars().all(|c| c == '=')) {
        after.push_str(blank);
        after.push_str(eol);
    }
    Slot { at, before, after }
}

/// Where a highlight over `range` lands. A highlight touching a block it must not
/// enter becomes a comment after that block; one touching math or a code span widens to
/// cover it.
pub(crate) fn for_highlight(source: &str, range: Range<usize>) -> Landing {
    let v = scan(source);
    if let Some(block) = v
        .blocks
        .iter()
        .rev()
        .find(|b| range.start < b.lines.end && b.lines.start < range.end.max(range.start + 1))
    {
        return Landing::Point(after_block(source, block));
    }
    let mut range = range;
    for m in &v.whole {
        if m.start < range.end && range.start < m.end {
            range.start = range.start.min(m.start);
            range.end = range.end.max(m.end);
        }
    }
    Landing::Highlight(range)
}

/// Where a point comment anchored at `at` lands: after any block it falls in or ends
/// (the start of the line after a block is that block's business: an HTML block's
/// terminating blank line, a fence's closing line), after any formula or code span it
/// would split, and never ONTO a blank line — text written at the start of one fills
/// it, joining the blocks either side.
pub(crate) fn for_point(source: &str, at: usize) -> Slot {
    let v = scan(source);
    if let Some(block) = v
        .blocks
        .iter()
        .find(|b| b.lines.start <= at && at <= b.lines.end)
    {
        return after_block(source, block);
    }
    let at = v
        .whole
        .iter()
        .find(|m| m.start < at && at < m.end)
        .map_or(at, |m| m.end);
    if line_start(source, at) == at
        && is_blank(&source[at..line_end_after(source, at)])
        && at < source.len()
    {
        return Slot {
            at,
            before: String::new(),
            after: eol_near(source, at).into(),
        };
    }
    Slot::bare(at)
}

/// The text to cut when removing a point comment at `span`: the construct alone, or —
/// when it sits on a line of its own — that whole line, plus one blank line it was
/// set off by, so a comment written after a block comes out leaving the file as it
/// was before.
pub(crate) fn removal_span(source: &str, span: Range<usize>) -> Range<usize> {
    let ls = line_start(source, span.start);
    let le = line_end_after(source, span.end);
    if !is_blank(&source[ls..span.start]) || !is_blank(&source[span.end..le]) {
        return span;
    }
    // The comment owns its line. Take one adjoining blank line with it when it is set
    // off by blank lines on BOTH sides (or a blank line and the end of the file);
    // otherwise the two blocks around it would be left run together or spread apart.
    let below_blank = le == source.len() || is_blank(&source[le..line_end_after(source, le)]);
    if ls > 0 && below_blank {
        let prev = line_start(source, ls - 1);
        if is_blank(&source[prev..ls]) {
            return prev..le;
        }
    }
    ls..le
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Annotate the first occurrence of `word` as a highlight, the way the apply path
    /// does, and return the file.
    fn annotate(md: &str, word: &str) -> String {
        let at = md.find(word).expect("word in fixture");
        apply(md, for_highlight(md, at..at + word.len()))
    }

    fn apply(md: &str, landing: Landing) -> String {
        match landing {
            Landing::Highlight(r) => crate::annotate::insert_or_extend_highlight(md, r, "c"),
            Landing::Point(slot) => crate::annotate::insert_point_comment(md, &slot, "c"),
        }
    }

    fn apply_point(md: &str, at: usize) -> String {
        apply(md, Landing::Point(for_point(md, at)))
    }

    fn remove_point(out: &str) -> String {
        let s = out.find("{>>c<<}").unwrap();
        let r = removal_span(out, s..s + "{>>c<<}".len());
        format!("{}{}", &out[..r.start], &out[r.end..])
    }

    #[test]
    fn front_matter_keeps_every_byte_and_the_comment_follows_it() {
        let md = "---\ntitle: Hello\n---\n\nBody.\n";
        let out = annotate(md, "Hello");
        assert_eq!(out, "---\ntitle: Hello\n---\n{>>c<<}\n\nBody.\n");
        assert_eq!(remove_point(&out), md, "removal restores the file");
        assert_eq!(
            apply_point(md, 0),
            out,
            "a point at its first byte moves too"
        );
    }

    #[test]
    fn a_fenced_block_is_never_wrapped_and_its_closing_fence_never_touched() {
        for fence in ["```", "~~~"] {
            let md = format!("Intro.\n\n{fence}rust\nlet x = 1;\n{fence}\n\nOutro.\n");
            let out = annotate(&md, "x = 1");
            assert_eq!(
                out,
                format!("Intro.\n\n{fence}rust\nlet x = 1;\n{fence}\n{{>>c<<}}\n\nOutro.\n")
            );
            assert_eq!(remove_point(&out), md);
            // A cross-block selection ending on the code's last character anchors
            // just before the closing fence's line break.
            let close = md.rfind(fence).unwrap() + 3;
            assert_eq!(apply_point(&md, close), out);
        }
    }

    #[test]
    fn a_fenced_block_at_the_end_of_the_file_still_closes() {
        let md = "Intro.\n\n```\nx\n```";
        let out = annotate(md, "x");
        assert_eq!(out, "Intro.\n\n```\nx\n```\n{>>c<<}\n");
    }

    #[test]
    fn a_fenced_block_inside_a_quote_or_list_keeps_the_comment_in_it() {
        let md = "> ```\n> x\n> ```\n> after\n";
        assert_eq!(annotate(md, "x"), "> ```\n> x\n> ```\n> {>>c<<}\n> after\n");
        let md = "- item\n\n  ```\n  x\n  ```\n- next\n";
        assert_eq!(
            annotate(md, "x"),
            "- item\n\n  ```\n  x\n  ```\n  {>>c<<}\n- next\n"
        );
    }

    #[test]
    fn an_indented_block_keeps_its_closing_blank_line() {
        let md = "Intro.\n\n    let x = 1;\n    let y = 2;\n\nOutro.\n";
        let out = annotate(md, "x = 1");
        assert_eq!(
            out,
            "Intro.\n\n    let x = 1;\n    let y = 2;\n{>>c<<}\n\nOutro.\n"
        );
        assert_eq!(remove_point(&out), md);
    }

    #[test]
    fn an_html_block_is_set_off_by_a_blank_line() {
        let md = "<details>\n<summary>Sum word</summary>\n</details>\n\nOutro.\n";
        let out = annotate(md, "word");
        assert_eq!(
            out,
            "<details>\n<summary>Sum word</summary>\n</details>\n\n{>>c<<}\n\nOutro.\n"
        );
        assert_eq!(remove_point(&out), md);
    }

    #[test]
    fn a_comment_never_becomes_a_heading_over_a_following_rule() {
        let md = "```\nx\n```\n---\n";
        assert_eq!(annotate(md, "x"), "```\nx\n```\n{>>c<<}\n\n---\n");
    }

    #[test]
    fn math_is_never_split() {
        let md = "Sum $x+y$ here.\n";
        assert_eq!(annotate(md, "x"), "Sum {==$x+y$==}{>>c<<} here.\n");
        assert_eq!(
            apply_point(md, md.find('+').unwrap()),
            "Sum $x+y${>>c<<} here.\n"
        );
        let md = "Text\n$$\nx = y\n$$\nmore.\n";
        let out = annotate(md, "y");
        assert!(out.contains("{==$$\nx = y\n$$==}"), "{out:?}");
    }

    /// Markup inside a code span is literal to every tool and to `extract`, so a
    /// highlight touching one wraps the whole span, backticks included, and a point
    /// comment inside one moves to its end. Each result still reads back as exactly
    /// one annotation, which is the property the write path exists to keep.
    ///
    /// Mutation: drop `Event::Code` from `scan`'s whole-span arm and every case fails.
    #[test]
    fn a_code_span_is_never_split() {
        let md = "Run `cargo test` now.\n";
        let out = annotate(md, "test");
        assert_eq!(out, "Run {==`cargo test`==}{>>c<<} now.\n");
        assert_eq!(
            crate::annotate::extract(&out).annotations.len(),
            1,
            "{out:?}"
        );
        // Starting outside the span and ending inside it, and the reverse.
        for (from, to) in [("Run", "cargo"), ("test", "now")] {
            let start = md.find(from).unwrap();
            let end = md.find(to).unwrap() + to.len();
            let out = apply(md, for_highlight(md, start..end));
            assert_eq!(
                crate::annotate::extract(&out).annotations.len(),
                1,
                "{from}..{to}: {out:?}"
            );
        }
        let out = apply_point(md, md.find("test").unwrap());
        assert_eq!(out, "Run `cargo test`{>>c<<} now.\n");
        assert_eq!(
            crate::annotate::extract(&out).annotations.len(),
            1,
            "{out:?}"
        );
    }

    /// Landing and extraction read the file through one parser configuration: an
    /// indented paragraph in a footnote definition is footnote text to both, so a
    /// highlight written there is one `extract` finds. With footnotes off on either
    /// side it is an indented code block to that side alone.
    ///
    /// Mutation: drop `ENABLE_FOOTNOTES` from `as_written_options` and the comment
    /// lands after a code block instead.
    #[test]
    fn landing_and_extraction_agree_on_footnote_content() {
        let md = "Text[^1].\n\n[^1]: First.\n\n    Second paragraph word.\n";
        let out = annotate(md, "word");
        assert_eq!(
            out,
            "Text[^1].\n\n[^1]: First.\n\n    Second paragraph {==word==}{>>c<<}.\n"
        );
        let found = crate::annotate::extract(&out).annotations;
        assert_eq!(found.len(), 1, "{out:?}");
    }

    #[test]
    fn ordinary_text_is_untouched() {
        for (md, word) in [
            ("## Heading word\n", "word"),
            ("- item word\n", "word"),
            ("> quoted word\n", "word"),
            ("Price is $5 and $10 today.\n", "and"),
            ("---\n\nA rule, not front matter.\n", "rule"),
        ] {
            let at = md.find(word).unwrap();
            assert_eq!(
                for_highlight(md, at..at + word.len()),
                Landing::Highlight(at..at + word.len()),
                "{md:?}"
            );
            assert_eq!(for_point(md, at), Slot::bare(at), "{md:?}");
        }
    }

    #[test]
    fn a_comment_at_a_blank_line_never_fills_it() {
        let md = "para\n\npara2\n";
        let out = apply_point(md, 5);
        assert_eq!(out, "para\n{>>c<<}\n\npara2\n");
        assert_eq!(remove_point(&out), md);
        // Directly after an HTML block, that blank line is what ends the block.
        let md = "<div>\nx\n</div>\n\nOutro.\n";
        let out = apply_point(md, md.find("\n\n").unwrap() + 1);
        assert_eq!(out, "<div>\nx\n</div>\n\n{>>c<<}\n\nOutro.\n");
        assert_eq!(remove_point(&out), md);
    }

    #[test]
    fn a_crlf_document_stays_crlf() {
        let md = "Intro.\r\n\r\n```\r\nx\r\n```\r\n\r\nOutro.\r\n";
        let out = annotate(md, "x");
        assert_eq!(
            out,
            "Intro.\r\n\r\n```\r\nx\r\n```\r\n{>>c<<}\r\n\r\nOutro.\r\n"
        );
        assert_eq!(remove_point(&out), md);
        let md = "<div>\r\nx\r\n</div>\r\n\r\nOutro.\r\n";
        let out = annotate(md, "x");
        assert_eq!(out, "<div>\r\nx\r\n</div>\r\n\r\n{>>c<<}\r\n\r\nOutro.\r\n");
        assert_eq!(remove_point(&out), md);
        let md = "---\r\ntitle: Hello\r\n---\r\n\r\nBody.\r\n";
        let out = annotate(md, "Hello");
        assert_eq!(
            out,
            "---\r\ntitle: Hello\r\n---\r\n{>>c<<}\r\n\r\nBody.\r\n"
        );
        assert_eq!(remove_point(&out), md);
        let md = "para\r\n\r\npara2\r\n";
        let out = apply_point(md, 6);
        assert_eq!(out, "para\r\n{>>c<<}\r\n\r\npara2\r\n");
        assert_eq!(remove_point(&out), md);
    }

    #[test]
    fn removing_a_comment_inside_text_takes_only_the_comment() {
        let md = "a {>>c<<} b\n";
        assert_eq!(remove_point(md), "a  b\n");
        let md = "para\n{>>c<<}\npara2\n";
        assert_eq!(remove_point(md), "para\npara2\n");
        let md = "para\n\n{>>c<<}\n\npara2\n";
        assert_eq!(remove_point(md), "para\n\npara2\n");
    }
}
