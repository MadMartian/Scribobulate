//! The document as other Markdown tools read it, for deciding what is code. Pure,
//! GTK-free.
//!
//! [`as_written`] is the ONE answer both extraction (`annotate::scan`) and landing
//! (`annotate::landing`) take their reading of code from: the text, where its Markdown
//! body starts, and the code in it. Both read the same bytes over the same span, so
//! they cannot disagree about where code is.
//!
//! **Why a comment's body is masked.** A comment's body is the annotator's prose, not
//! the document's Markdown, so a backtick in it must not pair with one in the text
//! around it. Pulldown does not know CriticMarkup and would pair them, hiding a real
//! closer inside a "code span" and letting a construct close on a later one, swallowing
//! the prose between. So the body of every comment that lies outside code is
//! overwritten by filler before the parse. A comment opening inside real code is that
//! code's text and stays.
//!
//! **Why masking is decided by a lexer and then checked.** Whether a comment lies
//! outside code depends on the code, which depends on which comments are masked. Asking
//! the parser alone means repeating mask → parse until the set stops changing, and that
//! takes one round per comment in a chain of comments that each hold a stray backtick:
//! each round settles one more of them. So the mask is first decided in one
//! left-to-right pass that reads comments and backtick runs the way CommonMark reads
//! code spans ([`guess`]), and the parser then checks it. The check normally agrees at
//! once; where it does not (raw HTML, block structure the lexer does not model) the
//! repetition continues for a few rounds. If even that does not settle, every comment
//! is masked: that reading can mistake code for prose, but it never lets a comment
//! close on another's closer, so it can never swallow prose (see [`settle`]).

use super::mutate::{COMMENT_CLOSE, COMMENT_OPEN};
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

/// The byte every masked body byte becomes.
const FILLER: u8 = b'x';

/// How many parses [`settle`] spends checking the mask before it masks every comment.
/// The lexer's mask is normally confirmed by the first.
const SETTLE_ROUNDS: usize = 4;

/// The file as other Markdown tools read it. See the module docs.
pub(super) struct AsWritten<'a> {
    /// `source` with the body of every comment outside code masked: the same bytes at
    /// the same offsets otherwise.
    pub(super) text: Cow<'a, str>,
    /// Where the Markdown body starts: just past the front matter, 0 without any. Every
    /// parse of [`Self::text`] reads `text[body_from..]`, as the renderer does.
    pub(super) body_from: usize,
    /// The byte span of every piece of code in the body — each inline code span,
    /// backticks included, and each fenced or indented code block, fences included — in
    /// ascending order. Empty when `source` holds no CriticMarkup opener, since then no
    /// caller asks.
    pub(super) code: Vec<Range<usize>>,
}

/// Whether [`settle`] confirmed a mask or fell back to masking every comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Settled,
    MaskedEvery,
}

/// `source` as other Markdown tools read it. See the module docs.
pub(super) fn as_written(source: &str) -> AsWritten<'_> {
    let body_from = crate::renderer::frontmatter::scan(source).map_or(0, |fm| fm.end());
    if !super::scan::has_opener(source) {
        return AsWritten {
            text: Cow::Borrowed(source),
            body_from,
            code: Vec::new(),
        };
    }
    let raw = parse_code(source, body_from);
    if outside_code(source, body_from, &raw.all).is_empty() {
        // Nothing to mask: the raw parse is already the answer.
        return AsWritten {
            text: Cow::Borrowed(source),
            body_from,
            code: raw.all,
        };
    }
    let first = guess(source, body_from, &raw.blocks);
    settle(source, body_from, first, SETTLE_ROUNDS).0
}

/// Check `mask` against the parser, repeating until it is confirmed, for at most
/// `rounds` parses; then, or on returning to a mask already tried, mask every comment.
///
/// Masking every comment cannot swallow prose: a masked body holds no backtick or
/// tilde, so no code span or fence can begin or end inside it, and each comment's
/// opener and closer are then both inside code or both outside it. A comment outside
/// code therefore closes on its own closer.
fn settle(
    source: &str,
    from: usize,
    mut mask: Vec<Range<usize>>,
    rounds: usize,
) -> (AsWritten<'_>, Outcome) {
    let mut tried: Vec<Vec<Range<usize>>> = Vec::new();
    for _ in 0..rounds {
        let text = masked(source, &mask);
        let code = parse_code(&text, from).all;
        let next = outside_code(source, from, &code);
        if next == mask {
            let written = AsWritten {
                text,
                body_from: from,
                code,
            };
            return (written, Outcome::Settled);
        }
        if tried.contains(&next) {
            break;
        }
        tried.push(std::mem::replace(&mut mask, next));
    }
    let every = outside_code(source, from, &[]);
    log::warn!(
        "annotate: which of {} comments lie outside code did not settle in {rounds} \
         parses; masking every one",
        every.len()
    );
    let text = masked(source, &every);
    let code = parse_code(&text, from).all;
    let written = AsWritten {
        text,
        body_from: from,
        code,
    };
    (written, Outcome::MaskedEvery)
}

/// `source` with each of `bodies` masked. Only bytes that are not whitespace are
/// overwritten, so every line keeps its indentation and a whitespace-only line stays
/// blank: the block structure is the file's own.
fn masked<'a>(source: &'a str, bodies: &[Range<usize>]) -> Cow<'a, str> {
    if bodies.is_empty() {
        return Cow::Borrowed(source);
    }
    let mut bytes = source.as_bytes().to_vec();
    for body in bodies {
        for b in &mut bytes[body.clone()] {
            if !matches!(*b, b' ' | b'\t' | b'\n' | b'\r') {
                *b = FILLER;
            }
        }
    }
    // Every byte of a multi-byte character is overwritten together (none of them is
    // whitespace), so the text stays UTF-8; the fallback is unreachable.
    String::from_utf8(bytes).map_or(Cow::Borrowed(source), Cow::Owned)
}

/// Whether a body could open or close code, and so needs masking.
fn could_open_code(body: &str) -> bool {
    body.bytes().any(|b| matches!(b, b'`' | b'~' | b'\n'))
}

/// The bodies to mask given `code`: every comment from `from` on whose opener lies
/// outside it, that stays within one block and could open code.
///
/// Comments are paired as extraction pairs them: an opener whose first closer lies
/// past a block break is literal text, and a later opener may still pair with that
/// closer. Closers and breaks are indexed, so a run of such openers costs no rescan.
fn outside_code(source: &str, from: usize, code: &[Range<usize>]) -> Vec<Range<usize>> {
    let closers = positions(source, COMMENT_CLOSE);
    let breaks: Vec<Range<usize>> = blank_lines(source).collect();
    let mut bodies = Vec::new();
    let mut at = from;
    while let Some(found) = source.get(at..).and_then(|rest| rest.find(COMMENT_OPEN)) {
        let open = at + found;
        let body_start = open + COMMENT_OPEN.len();
        at = body_start;
        if in_code(code, open) {
            continue;
        }
        let Some(close) = next_at(&closers, body_start) else {
            break;
        };
        let body = body_start..close;
        if crosses_block(&breaks, &body) {
            continue;
        }
        at = close + COMMENT_CLOSE.len();
        if could_open_code(&source[body.clone()]) {
            bodies.push(body);
        }
    }
    bodies
}

/// The ascending start of every `needle` in `source`.
fn positions(source: &str, needle: &str) -> Vec<usize> {
    source.match_indices(needle).map(|(i, _)| i).collect()
}

/// The first of the ascending `v` at or after `p`.
fn next_at(v: &[usize], p: usize) -> Option<usize> {
    v.get(v.partition_point(|&q| q < p)).copied()
}

/// Whether `range` contains one of `breaks` (as [`blank_lines`] gives them) whole.
///
/// The guard is on the break's END, not its start — an off-by-one here would reject
/// a legal span whose content merely ENDS with a newline. Breaks start at ascending
/// line breaks and end at the next one, so the first break starting in `range` is
/// also the first to end. O(log n).
pub(super) fn crosses_block(breaks: &[Range<usize>], range: &Range<usize>) -> bool {
    let i = breaks.partition_point(|b| b.start < range.start);
    breaks.get(i).is_some_and(|b| b.end <= range.end)
}

/// The mask a left-to-right reading gives: a comment opener outside an open code span
/// runs to the first closer after it, and its body is skipped; a run of L backticks
/// opens a code span when a run of exactly L follows within the block, and everything
/// up to that run, comment openers included, is the span's text. Outside code a backslash
/// escapes the punctuation after it, except a `{`: extraction knows no escapes, so
/// `\{>>` still opens a comment. Code blocks found by the raw parse, and
/// whitespace-only lines, end the block a span or comment may run in.
///
/// This is CommonMark's code-span rule plus CriticMarkup's comment, read in one pass,
/// in O(n log n): the run and closer positions are indexed, so no search rescans text.
fn guess(source: &str, from: usize, blocks: &[Range<usize>]) -> Vec<Range<usize>> {
    let b = source.as_bytes();
    let stops = merged(blocks.iter().cloned().chain(blank_lines(source)).collect());
    let runs = backtick_runs(source, from);
    let closers = positions(source, COMMENT_CLOSE);

    let mut bodies = Vec::new();
    let mut i = from;
    let mut k = 0;
    while i < b.len() {
        while stops.get(k).is_some_and(|s| s.end <= i) {
            k += 1;
        }
        let stop = match stops.get(k) {
            Some(s) if s.start <= i => {
                i = s.end;
                continue;
            }
            Some(s) => s.start,
            None => b.len(),
        };
        match b[i] {
            b'\\'
                if b.get(i + 1)
                    .is_some_and(|&c| c.is_ascii_punctuation() && c != b'{') =>
            {
                i += 2;
            }
            b'`' => {
                let len = b[i..].iter().take_while(|&&c| c == b'`').count();
                let close = runs.get(&len).and_then(|v| next_at(v, i + len));
                i = match close {
                    Some(j) if j < stop => j + len,
                    _ => i + len,
                };
            }
            b'{' if source[i..].starts_with(COMMENT_OPEN) => {
                let body_start = i + COMMENT_OPEN.len();
                match next_at(&closers, body_start) {
                    Some(c) if c < stop => {
                        if could_open_code(&source[body_start..c]) {
                            bodies.push(body_start..c);
                        }
                        i = c + COMMENT_CLOSE.len();
                    }
                    _ => i = body_start,
                }
            }
            _ => i += 1,
        }
    }
    bodies
}

/// The start of every maximal backtick run from `from` on, by run length.
fn backtick_runs(source: &str, from: usize) -> HashMap<usize, Vec<usize>> {
    let b = source.as_bytes();
    let mut runs: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut i = from;
    while i < b.len() {
        if b[i] == b'`' {
            let len = b[i..].iter().take_while(|&&c| c == b'`').count();
            runs.entry(len).or_default().push(i);
            i += len;
        } else {
            i += 1;
        }
    }
    runs
}

/// `ranges` sorted and with overlapping ones joined.
fn merged(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|r| r.start);
    let mut out: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for r in ranges {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

/// Every whitespace-only line in `text`, as the span from the line break before it
/// through its own: a span of the text crosses a block break exactly when it contains
/// one of these whole. A line of spaces, tabs or a lone `\r` ends a paragraph just as
/// an empty one does, so `\n\n`, `\n \t\n` and CRLF's `\n\r\n` all count.
pub(super) fn blank_lines(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let b = text.as_bytes();
    text.match_indices('\n').filter_map(move |(p, _)| {
        let gap = b[p + 1..]
            .iter()
            .take_while(|&&c| matches!(c, b' ' | b'\t' | b'\r'))
            .count();
        let q = p + 1 + gap;
        (b.get(q) == Some(&b'\n')).then_some(p..q + 1)
    })
}

/// Whether byte `i` lies inside one of `code` (ascending, non-overlapping).
pub(super) fn in_code(code: &[Range<usize>], i: usize) -> bool {
    let k = code.partition_point(|r| r.end <= i);
    code.get(k).is_some_and(|r| r.start <= i)
}

/// What one parse of `text[from..]` reads as code, in `text`'s coordinates.
struct ParsedCode {
    /// Every code span and code block, ascending.
    all: Vec<Range<usize>>,
    /// The code blocks alone.
    blocks: Vec<Range<usize>>,
}

fn parse_code(text: &str, from: usize) -> ParsedCode {
    // Through the normalising seam, like every parse site, so a tab-padded table is a
    // table here exactly as on the page. The pre-pass keeps every byte where it was,
    // so the ranges index `text` itself.
    let mut parsed = ParsedCode {
        all: Vec::new(),
        blocks: Vec::new(),
    };
    for (ev, r) in crate::renderer::NormalizedMd::new(&text[from..])
        .parse_as_written()
        .into_offset_iter()
    {
        let r = r.start + from..r.end + from;
        match ev {
            pulldown_cmark::Event::Code(_) => parsed.all.push(r),
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(_)) => {
                parsed.blocks.push(r.clone());
                parsed.all.push(r);
            }
            // dispatch-selector: only code decides whether a delimiter is literal
            _ => {}
        }
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::extract;

    /// A list of one range (clippy reads `[a..b]` as a mistyped range of numbers).
    fn one(r: Range<usize>) -> Vec<Range<usize>> {
        vec![r]
    }

    /// `n` comments that each hold one stray backtick, then a real code span and a
    /// plain comment.
    fn chain(n: usize) -> String {
        "{>>a ` b<<} ".repeat(n) + "`z` w {>>e<<}"
    }

    fn comments(src: &str) -> Vec<String> {
        extract(src)
            .annotations
            .into_iter()
            .filter_map(|a| a.comment)
            .collect()
    }

    /// One round per comment was what the old repetition needed, so a chain longer
    /// than its cap swallowed `` `z` w `` into a comment. The lexer reads the chain in
    /// one pass, and the parser confirms it at once.
    #[test]
    fn a_long_chain_of_stray_backtick_comments_swallows_no_prose() {
        for n in [1, 2, 8, 9, 10, 40] {
            let src = chain(n);
            let mut want = vec!["a ` b".to_string(); n];
            want.push("e".into());
            assert_eq!(comments(&src), want, "n = {n}");
            assert!(extract(&src).cleaned.contains("`z` w"), "n = {n}");
            let first = guess(&src, 0, &[]);
            let (_, outcome) = settle(&src, 0, first, 1);
            assert_eq!(outcome, Outcome::Settled, "n = {n}: one parse confirms it");
        }
    }

    /// Mutation check for the fallback: start the repetition where the old code did
    /// (from the raw parse) with too few rounds to settle the chain. It must mask every
    /// comment rather than return the half-settled mask, which swallows `` `z` w ``.
    #[test]
    fn running_out_of_rounds_masks_every_comment_and_swallows_nothing() {
        let src = chain(9);
        let raw = parse_code(&src, 0);
        let start = outside_code(&src, 0, &raw.all);
        let (written, outcome) = settle(&src, 0, start, 2);
        assert_eq!(outcome, Outcome::MaskedEvery);
        assert_eq!(written.text, masked(&src, &outside_code(&src, 0, &[])));
        let z = src.find("`z`").unwrap();
        assert_eq!(written.code, one(z..z + 3), "only `z` is code");
    }

    /// Masking keeps spaces, tabs and line breaks, so a whitespace-only line in a body
    /// stays blank and the paragraphs either side stay apart. Overwriting the space
    /// joined them, which paired the backticks either side, which unmasked the comment,
    /// which split them again: the mask flipped every round.
    #[test]
    fn masking_keeps_blank_lines_and_indentation() {
        assert_eq!(masked("{>>a `\n\t b<<}", &one(3..10)), "{>>x x\n\t x<<}");
        let src = "x ` y {>>a\n \nb<<} z ` w";
        let first = guess(src, 0, &[]);
        let (written, outcome) = settle(src, 0, first, SETTLE_ROUNDS);
        assert_eq!(outcome, Outcome::Settled);
        assert_eq!(
            written.text, src,
            "a comment across a blank line is not masked"
        );
        assert!(
            written.code.is_empty(),
            "the backticks are paragraphs apart"
        );
        assert!(extract(src).annotations.is_empty(), "nor is it a comment");
    }

    #[test]
    fn a_whitespace_only_line_is_a_block_break() {
        let breaks = |s: &str| blank_lines(s).collect::<Vec<_>>();
        assert_eq!(breaks("a\n\nb"), one(1..3));
        assert_eq!(breaks("a\n \t\nb"), one(1..5));
        assert_eq!(breaks("a\r\n\r\nb"), one(2..5));
        assert!(breaks("a\nb\n").is_empty());
    }

    /// A comment opening inside a real code span is that code's text, both to the
    /// lexer and to the parser.
    #[test]
    fn a_comment_opening_inside_code_stays_code() {
        let src = "`a {>>c ` d<<} and {>>x ` y<<} `q`";
        assert_eq!(comments(src), ["x ` y"]);
    }

    /// Front matter is not Markdown to the renderer, so neither extraction nor landing
    /// parses it. Parsed as Markdown, the fence in this block scalar interrupts the
    /// paragraph and, never closed, made the whole body code to extraction alone: the
    /// comment vanished there while landing still wrote beside it.
    #[test]
    fn front_matter_cannot_turn_the_body_into_code() {
        let src = "---\nkey: |\n  ```\n---\n\nBody {>>c<<} word.\n";
        let written = as_written(src);
        assert_eq!(written.body_from, src.find("\nBody").unwrap());
        assert!(written.code.is_empty(), "{:?}", written.code);
        assert_eq!(comments(src), ["c"]);
        let at = src.find("word").unwrap();
        assert_eq!(
            crate::annotate::landing::for_highlight(src, at..at + 4),
            crate::annotate::Landing::Highlight(at..at + 4),
            "landing reads the body as prose too"
        );
    }

    /// Over random runs of backticks, comments, escapes, words and line breaks, no
    /// comment ever holds a closer (that is what swallowing is). Within one line that
    /// cannot be a fence, the lexer's mask is confirmed within the rounds allowed;
    /// across lines, block structure can leave no mask to confirm (see
    /// `a_mask_that_makes_a_fence_has_no_settled_reading`).
    #[test]
    fn no_mix_of_backticks_and_comments_swallows_prose() {
        const TOKENS: [&str; 9] = ["{>>", "<<}", "`", "``", " ", "a", "\\", "{>>`<<}", "\n"];
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..3000 {
            let len = 4 + (next() % 24) as usize;
            let src: String = (0..len)
                .map(|_| TOKENS[(next() % TOKENS.len() as u64) as usize])
                .collect();
            for c in comments(&src) {
                assert!(!c.contains(COMMENT_CLOSE), "{src:?} swallowed into {c:?}");
            }
            let raw = parse_code(&src, 0);
            if !outside_code(&src, 0, &raw.all).is_empty() {
                let first = guess(&src, 0, &raw.blocks);
                let (_, outcome) = settle(&src, 0, first, SETTLE_ROUNDS);
                let fence = src.trim_start_matches(' ').starts_with("```");
                if !src.contains('\n') && !fence {
                    assert_eq!(outcome, Outcome::Settled, "{src:?}");
                }
            }
        }
    }

    /// A line opening with a backtick run is a fence only if its info string holds no
    /// backtick, so masking the comment in it makes it a fence, which puts the comment
    /// in code, which unmasks it. No mask settles: the repetition stops on the mask it
    /// already tried and masks every comment, which swallows nothing.
    #[test]
    fn a_mask_that_makes_a_fence_has_no_settled_reading() {
        let src = "````{>>`<<} a\n\nAfter {>>c ` d<<} `e`.\n";
        let raw = parse_code(src, 0);
        let first = guess(src, 0, &raw.blocks);
        let (written, outcome) = settle(src, 0, first, SETTLE_ROUNDS);
        assert_eq!(outcome, Outcome::MaskedEvery);
        assert_eq!(written.text, masked(src, &outside_code(src, 0, &[])));
        for c in comments(src) {
            assert!(!c.contains(COMMENT_CLOSE), "swallowed into {c:?}");
        }
    }

    /// Extraction knows no backslash escapes, so neither may the lexer's comments:
    /// `\{>>` opens one. Read as an escape, the comment after the code span went
    /// unmasked and its backtick paired with a later one.
    #[test]
    fn a_backslash_before_a_comment_does_not_escape_it() {
        let src = "{>>`<<}``\\{>>`<<} a\\``\\{>>`<<}<<}{>>`<<}{>>{>>`<<}\\{>>`<<} a `";
        let raw = parse_code(src, 0);
        let first = guess(src, 0, &raw.blocks);
        assert_eq!(settle(src, 0, first, 1).1, Outcome::Settled);
    }
}
