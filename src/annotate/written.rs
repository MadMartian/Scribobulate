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
//! repetition continues for a few rounds.
//!
//! **If even that does not settle**, nothing is masked: the reading is the file's own
//! parse, so its block structure is exactly what other tools read and no fence is made
//! or hidden. Each comment that parse would let close past a closer it hides in code
//! is read, with the code it runs into, as code ([`sealed`]). That reading can mistake
//! prose for code, but never lets a comment close on a closer that is not its own, so
//! it can never swallow prose.
//!
//! **One rule for a blank line.** [`blank_lines`] is the block break every CriticMarkup
//! construct may not cross: extraction refuses a construct across one, creating an
//! annotation turns a selection across one into a point comment
//! ([`holds_block_break`]), and landing reads a line holding nothing else as blank
//! ([`is_line_space`]).

use super::mutate::{COMMENT_CLOSE, COMMENT_OPEN};
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

/// The byte every masked body byte becomes.
const FILLER: u8 = b'x';

/// How many parses [`settle`] spends checking the mask before it falls back to the
/// file's own parse. The lexer's mask is normally confirmed by the first.
const SETTLE_ROUNDS: usize = 4;

/// How many recent documents [`first_time_unsettled`] remembers, so a document that
/// keeps taking the fallback is reported once and not on every render.
const UNSETTLED_MEMORY: usize = 8;

/// The file as other Markdown tools read it. See the module docs.
pub(super) struct AsWritten<'a> {
    /// `source` with the body of every comment outside code masked, or `source` itself
    /// when no mask settled: the same bytes at the same offsets otherwise.
    pub(super) text: Cow<'a, str>,
    /// Where the Markdown body starts: just past the front matter, 0 without any. Every
    /// parse of [`Self::text`] reads `text[body_from..]`, as the renderer does.
    pub(super) body_from: usize,
    /// The byte span of every piece of code in the body — each inline code span,
    /// backticks included, and each fenced or indented code block, fences included — in
    /// ascending order. Empty when `source` holds no CriticMarkup opener, since then no
    /// caller asks. When no mask settled it also holds what [`sealed`] adds, which a
    /// parse of [`Self::text`] does not report, so a reader of code takes it from here.
    pub(super) code: Vec<Range<usize>>,
}

/// Whether [`settle`] confirmed a mask or fell back to the file's own parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Settled,
    Unsettled,
}

/// What [`settle`] read, how, and how many parses checking the mask took.
struct Reading<'a> {
    written: AsWritten<'a>,
    outcome: Outcome,
    parses: usize,
    /// Where the first comment body the last two masks disagreed on starts, when no
    /// mask settled: where to look in the file.
    flipped: Option<usize>,
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
    let reading = settle(source, body_from, first, SETTLE_ROUNDS);
    if reading.outcome == Outcome::Unsettled {
        report_unsettled(source, &reading);
    }
    reading.written
}

/// Say that `source` took the fallback: at `warn` the first time, at `debug` after.
fn report_unsettled(source: &str, reading: &Reading<'_>) {
    let level = if first_time_unsettled(source) {
        log::Level::Warn
    } else {
        log::Level::Debug
    };
    log::log!(
        level,
        "annotate: which comments lie outside code did not settle in {} parses \
         ({} comment openers, the first that changed at byte {}); reading the file's own \
         code instead",
        reading.parses,
        source.matches(COMMENT_OPEN).count(),
        reading
            .flipped
            .map_or_else(|| "?".to_string(), |at| at.to_string()),
    );
}

/// Check `mask` against the parser, repeating until it is confirmed, for at most
/// `rounds` parses; then, or on returning to a mask already tried, read the file's own
/// parse with [`sealed`] (see the module docs).
fn settle(source: &str, from: usize, mut mask: Vec<Range<usize>>, rounds: usize) -> Reading<'_> {
    let mut tried: Vec<Vec<Range<usize>>> = Vec::new();
    let mut flipped = None;
    let mut parses = 0;
    while parses < rounds {
        parses += 1;
        let text = masked(source, &mask);
        let code = parse_code(&text, from).all;
        let next = outside_code(source, from, &code);
        if next == mask {
            let written = AsWritten {
                text,
                body_from: from,
                code,
            };
            return Reading {
                written,
                outcome: Outcome::Settled,
                parses,
                flipped: None,
            };
        }
        flipped = first_difference(&mask, &next);
        if tried.contains(&next) {
            break;
        }
        tried.push(std::mem::replace(&mut mask, next));
    }
    let code = sealed(source, from, parse_code(source, from).all);
    let written = AsWritten {
        text: Cow::Borrowed(source),
        body_from: from,
        code,
    };
    Reading {
        written,
        outcome: Outcome::Unsettled,
        parses,
        flipped,
    }
}

/// Where the first comment body one of `a` and `b` masks and the other does not
/// starts, both ascending.
fn first_difference(a: &[Range<usize>], b: &[Range<usize>]) -> Option<usize> {
    let i = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    match (a.get(i), b.get(i)) {
        (Some(x), Some(y)) => Some(x.start.min(y.start)),
        (Some(r), None) | (None, Some(r)) => Some(r.start),
        (None, None) => None,
    }
}

/// Whether `source` is a document that has not taken the fallback in recent memory.
/// Whether a document settles is a fixed property of its text, so without this every
/// render of one that does not would report it again (each render reads it twice: the
/// preview and the outline). Keyed by a hash of the text, so no content is held. Per
/// thread: the app reads documents on the main thread alone, and each test, on its own
/// thread, starts with nothing remembered.
fn first_time_unsettled(source: &str) -> bool {
    use std::hash::{Hash, Hasher};
    thread_local! {
        static SEEN: std::cell::RefCell<std::collections::VecDeque<u64>> =
            const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = hasher.finish();
    SEEN.with_borrow_mut(|seen| remember(seen, key))
}

/// Add `key` to `seen`, forgetting the oldest beyond [`UNSETTLED_MEMORY`]; whether it
/// was new.
fn remember(seen: &mut std::collections::VecDeque<u64>, key: u64) -> bool {
    if seen.contains(&key) {
        return false;
    }
    if seen.len() == UNSETTLED_MEMORY {
        seen.pop_front();
    }
    seen.push_back(key);
    true
}

/// `code`, plus the opener through the end of that code of every comment outside it
/// whose first closer outside code lies past a closer inside it.
///
/// Extraction pairs a comment with the first closer outside code, so a closer the parse
/// hides in code — a code span that a stray backtick in the body opened — lets the
/// comment run on to a later comment's closer, swallowing the prose between. Reading
/// such a comment as code instead leaves it literal. That hides no closer that was not
/// hidden already (the comment's first visible closer lies past the code it runs into,
/// so the span added holds no other), so no comment before it pairs differently, and one
/// pass in order seals them all.
fn sealed(source: &str, from: usize, code: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let closers = positions(source, COMMENT_CLOSE);
    let visible: Vec<usize> = closers
        .iter()
        .copied()
        .filter(|&c| !in_code(&code, c))
        .collect();
    let breaks: Vec<Range<usize>> = blank_lines(source).collect();
    let mut seals = Vec::new();
    let mut at = from;
    while let Some(found) = source.get(at..).and_then(|rest| rest.find(COMMENT_OPEN)) {
        let open = at + found;
        let body_start = open + COMMENT_OPEN.len();
        at = body_start;
        if in_code(&code, open) {
            continue;
        }
        let Some(close) = next_at(&visible, body_start) else {
            break;
        };
        if crosses_block(&breaks, &(body_start..close)) {
            continue;
        }
        at = close + COMMENT_CLOSE.len();
        if let Some(hidden) = next_at(&closers, body_start).filter(|&k| k < close) {
            let end = code
                .get(code.partition_point(|r| r.end <= hidden))
                .map_or(close, |r| r.end);
            seals.push(open..end);
            at = end;
        }
    }
    if seals.is_empty() {
        return code;
    }
    merged(code.into_iter().chain(seals).collect())
}

/// Whether pulldown reads byte `b` as whitespace (`is_ascii_whitespace`: tab, line
/// feed, vertical tab, form feed, carriage return and space).
fn is_markdown_whitespace(b: u8) -> bool {
    matches!(b, b'\t'..=b'\r' | b' ')
}

/// `source` with each of `bodies` masked. Only bytes the parser does not read as
/// whitespace are overwritten, so every line keeps its indentation and a blank line
/// stays blank. Other block markup in a multi-line body (a list marker, a fence) is
/// masked like any text: the body is the annotator's prose, not the document's.
fn masked<'a>(source: &'a str, bodies: &[Range<usize>]) -> Cow<'a, str> {
    if bodies.is_empty() {
        return Cow::Borrowed(source);
    }
    let mut bytes = source.as_bytes().to_vec();
    for body in bodies {
        for b in &mut bytes[body.clone()] {
            if !is_markdown_whitespace(*b) {
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
/// `\{>>` still opens a comment. Whitespace-only lines end the block a span or comment
/// may run in, and so do the code blocks found by the raw parse — except one that opens
/// inside a comment's body: that is a sample the annotator pasted, the comment's text,
/// and masking the body leaves no fence. Trusted as a stop, an unclosed one hid the
/// rest of the file from the lexer.
///
/// This is CommonMark's code-span rule plus CriticMarkup's comment, read in one pass,
/// in O(n log n): the run and closer positions are indexed, so no search rescans text.
fn guess(source: &str, from: usize, blocks: &[Range<usize>]) -> Vec<Range<usize>> {
    let b = source.as_bytes();
    let breaks: Vec<Range<usize>> = blank_lines(source).collect();
    let runs = backtick_runs(source, from);
    let closers = positions(source, COMMENT_CLOSE);

    let mut bodies = Vec::new();
    let mut i = from;
    let (mut kb, mut kr) = (0, 0);
    while i < b.len() {
        while blocks.get(kb).is_some_and(|s| s.end <= i) {
            kb += 1;
        }
        while breaks.get(kr).is_some_and(|s| s.end <= i) {
            kr += 1;
        }
        let inside = |s: Option<&Range<usize>>| s.filter(|s| s.start <= i).map(|s| s.end);
        if let Some(end) = inside(blocks.get(kb)).or_else(|| inside(breaks.get(kr))) {
            i = end;
            continue;
        }
        let break_at = breaks.get(kr).map_or(b.len(), |s| s.start);
        let stop = blocks.get(kb).map_or(b.len(), |s| s.start).min(break_at);
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
                    Some(c) if c < break_at => {
                        if could_open_code(&source[body_start..c]) {
                            bodies.push(body_start..c);
                        }
                        // A block opening in the body is the body's text.
                        while blocks.get(kb).is_some_and(|s| s.start < c) {
                            kb += 1;
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
pub(super) fn merged(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
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
/// one of these whole. A line of spaces and tabs ([`is_line_space`]) ends a paragraph
/// just as an empty one does, in either line ending, so `\n\n`, `\n \t\n` and CRLF's
/// `\n\r\n` all count.
///
/// Only `\n` and `\r\n` end a line here: a document never holds a lone `\r`
/// (`crate::lineendings`).
pub(super) fn blank_lines(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let b = text.as_bytes();
    text.match_indices('\n').filter_map(move |(p, _)| {
        let gap = b[p + 1..]
            .iter()
            .take_while(|&&c| is_line_space(char::from(c)))
            .count();
        let q = p + 1 + gap;
        let eol = match b.get(q..) {
            Some([b'\n', ..]) => 1,
            Some([b'\r', b'\n', ..]) => 2,
            _ => return None,
        };
        Some(p..q + eol)
    })
}

/// Whether `text` holds a block break ([`blank_lines`]): whether a CriticMarkup
/// construct over exactly `text` is one extraction refuses.
pub(crate) fn holds_block_break(text: &str) -> bool {
    blank_lines(text).next().is_some()
}

/// What a blank line may hold besides its line end, as pulldown reads one: a paragraph
/// ends at a line of nothing but spaces and tabs (pulldown-cmark 0.13's `scan_all_space`,
/// then `is_at_eol`). A form feed or vertical tab is not among them, though pulldown
/// counts both as whitespace elsewhere: MEASURED on pulldown-cmark 0.13.4, `"a\n\x0c\nb"`
/// is one paragraph.
pub(super) fn is_line_space(c: char) -> bool {
    matches!(c, ' ' | '\t')
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
    use crate::annotate::{extract, landing::for_highlight, Landing};

    /// A list of one range (clippy reads `[a..b]` as a mistyped range of numbers).
    fn one(r: Range<usize>) -> Vec<Range<usize>> {
        vec![r]
    }

    /// `n` comments that each hold one stray backtick, then a real code span and a
    /// plain comment.
    fn chain(n: usize) -> String {
        "{>>a ` b<<} ".repeat(n) + "`z` w {>>e<<}"
    }

    /// A line that is a fence only once its comment is masked, then a comment whose
    /// stray backtick pairs with a later one: no mask settles.
    const FENCE_FLIP: &str = "````{>>`<<} a\n\nAfter {>>c ` d<<} `e`.\n";

    fn comments(src: &str) -> Vec<String> {
        extract(src)
            .annotations
            .into_iter()
            .filter_map(|a| a.comment)
            .collect()
    }

    fn reading(src: &str, rounds: usize) -> Reading<'_> {
        let raw = parse_code(src, 0);
        settle(src, 0, guess(src, 0, &raw.blocks), rounds)
    }

    /// Whether some comment outside `code` would close past a closer that `code` hides —
    /// what swallowing prose is. Stated from extraction's pairing rule, not from
    /// [`sealed`]'s.
    fn swallows(src: &str, code: &[Range<usize>]) -> bool {
        let breaks: Vec<Range<usize>> = blank_lines(src).collect();
        let closers = positions(src, COMMENT_CLOSE);
        let mut at = 0;
        while let Some(found) = src.get(at..).and_then(|rest| rest.find(COMMENT_OPEN)) {
            let body = at + found + COMMENT_OPEN.len();
            at = body;
            if in_code(code, body - COMMENT_OPEN.len()) {
                continue;
            }
            let Some(close) = closers
                .iter()
                .copied()
                .find(|&c| c >= body && !in_code(code, c))
            else {
                break;
            };
            if crosses_block(&breaks, &(body..close)) {
                continue;
            }
            if closers.iter().any(|&c| body <= c && c < close) {
                return true;
            }
            at = close + COMMENT_CLOSE.len();
        }
        false
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
            let r = settle(&src, 0, guess(&src, 0, &[]), 1);
            assert_eq!(
                r.outcome,
                Outcome::Settled,
                "n = {n}: one parse confirms it"
            );
        }
    }

    /// A comment that pastes an unclosed fence opens a code block to the end of the
    /// file in the raw parse. Trusted as a stop, it hid the chain after it from the
    /// lexer, which left the old one-comment-per-round repetition to settle it, and
    /// four rounds do not. Mutation check: make the lexer stop at that block again.
    #[test]
    fn a_fence_inside_a_comment_does_not_hide_the_rest_of_the_file_from_the_lexer() {
        let src = "{>>try:\n```\nfoo()\n<<} ".to_string() + &chain(9);
        let r = reading(&src, SETTLE_ROUNDS);
        assert_eq!(r.outcome, Outcome::Settled);
        assert_eq!(r.parses, 1, "the lexer's mask is confirmed at once");
        let mut want = vec!["try:\n```\nfoo()\n".to_string()];
        want.extend(vec!["a ` b".to_string(); 9]);
        want.push("e".into());
        assert_eq!(comments(&src), want);
    }

    /// Mutation check for the fallback's seal: start the repetition where the old code
    /// did (from the raw parse) with too few rounds to settle the chain. The fallback
    /// reads the file's own parse, in which each stray backtick pairs with the next and
    /// hides a closer; unsealed, every comment would close on the next one's closer.
    #[test]
    fn running_out_of_rounds_reads_the_files_own_code_and_swallows_nothing() {
        let src = chain(9);
        let raw = parse_code(&src, 0);
        let r = settle(&src, 0, outside_code(&src, 0, &raw.all), 2);
        assert_eq!(r.outcome, Outcome::Unsettled);
        assert_eq!(r.written.text, src, "nothing is masked");
        assert!(
            swallows(&src, &raw.all),
            "precondition: the raw parse swallows"
        );
        assert!(!swallows(&src, &r.written.code), "{:?}", r.written.code);
    }

    /// Masking keeps every byte the parser reads as whitespace, so a whitespace-only
    /// line in a body stays blank and the paragraphs either side stay apart.
    /// Overwriting the space joined them, which paired the backticks either side, which
    /// unmasked the comment, which split them again: the mask flipped every round.
    #[test]
    fn masking_keeps_blank_lines_and_indentation() {
        assert_eq!(masked("{>>a `\n\t b<<}", &one(3..10)), "{>>x x\n\t x<<}");
        assert_eq!(
            masked("{>>a\x0b\x0c`\r\n<<}", &one(3..9)),
            "{>>x\x0b\x0cx\r\n<<}",
            "pulldown's whole whitespace set is kept"
        );
        let src = "x ` y {>>a\n \nb<<} z ` w";
        let r = settle(src, 0, guess(src, 0, &[]), SETTLE_ROUNDS);
        assert_eq!(r.outcome, Outcome::Settled);
        assert_eq!(
            r.written.text, src,
            "a comment across a blank line is not masked"
        );
        assert!(
            r.written.code.is_empty(),
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
        assert_eq!(breaks("a\r\n \t\r\nb"), one(2..7));
        assert_eq!(breaks("- one\n  \n  two"), one(5..9));
        assert!(breaks("a\nb\n").is_empty());
        assert!(breaks("a\n\x0c\nb").is_empty(), "a form feed is text");
    }

    /// The block break is the parser's own paragraph break, line by line, in both line
    /// endings: checked against pulldown rather than restated.
    #[test]
    fn a_block_break_is_exactly_where_pulldown_ends_a_paragraph() {
        let paragraphs = |s: &str| {
            pulldown_cmark::Parser::new(s)
                .filter(|e| {
                    matches!(
                        e,
                        pulldown_cmark::Event::Start(pulldown_cmark::Tag::Paragraph)
                    )
                })
                .count()
        };
        for line in ["", " ", "\t", " \t ", "\x0b", "\x0c", " \x0c", "x", " x"] {
            for eol in ["\n", "\r\n"] {
                let src = format!("a{eol}{line}{eol}b");
                assert_eq!(holds_block_break(&src), paragraphs(&src) == 2, "{src:?}");
            }
        }
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
            for_highlight(src, at..at + 4),
            Landing::Highlight(at..at + 4),
            "landing reads the body as prose too"
        );
    }

    /// Over random runs of backticks, comments, escapes, words and line breaks, no
    /// comment ever holds a closer (that is what swallowing is). Within one line that
    /// cannot be a fence, the lexer's mask is confirmed within the rounds allowed;
    /// across lines, block structure can leave no mask to confirm (see
    /// `a_mask_that_makes_a_fence_has_no_settled_reading`), and then the reading is the
    /// file's own parse, sealed.
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
                let r = reading(&src, SETTLE_ROUNDS);
                let fence = src.trim_start_matches(' ').starts_with("```");
                if !src.contains('\n') && !fence {
                    assert_eq!(r.outcome, Outcome::Settled, "{src:?}");
                }
                if r.outcome == Outcome::Unsettled {
                    assert_eq!(r.written.text, src, "{src:?}");
                    assert!(!swallows(&src, &r.written.code), "{src:?}");
                }
            }
        }
    }

    /// A line opening with a backtick run is a fence only if its info string holds no
    /// backtick, so masking the comment in it makes it a fence, which puts the comment
    /// in code, which unmasks it. No mask settles, and the repetition stops on the mask
    /// it already tried. Masking every comment instead made that line a fence running
    /// to the end of the file: the comment after it vanished and a highlight there was
    /// moved to the end of the file. The file's own parse keeps the line a paragraph.
    #[test]
    fn a_mask_that_makes_a_fence_has_no_settled_reading() {
        let src = FENCE_FLIP;
        let r = reading(src, SETTLE_ROUNDS);
        assert_eq!(r.outcome, Outcome::Unsettled);
        assert_eq!(
            r.written.text, src,
            "nothing is masked, so no fence is made"
        );
        assert!(parse_code(src, 0).blocks.is_empty(), "nor is there one");
        assert!(!swallows(src, &r.written.code));
        // The comment whose closer the parse hides is read as code; the other stays.
        assert_eq!(comments(src), ["`"]);
        let after = src.find("After").unwrap();
        assert_eq!(
            for_highlight(src, after..after + 5),
            Landing::Highlight(after..after + 5),
            "a highlight on prose stays where it was asked for"
        );
    }

    /// A comment whose closer the file's own parse hides, with a later closer it would
    /// otherwise run on to, is read as code with the code it runs into: by extraction,
    /// and by landing, which keeps it whole. Mutation checks: drop the seal and `c ` d`
    /// swallows `` `e` `` up to `f`'s closer; drop landing's use of extraction's code
    /// and the highlight splits the sealed comment.
    #[test]
    fn a_comment_whose_closer_the_parse_hides_is_read_as_code() {
        let src = "````{>>`<<} a\n\nAfter {>>c ` d<<} `e` {>>f<<}.\n";
        assert_eq!(reading(src, SETTLE_ROUNDS).outcome, Outcome::Unsettled);
        assert_eq!(comments(src), ["`", "f"]);
        let d = src.find("d<<}").unwrap();
        let sealed = src.find("{>>c").unwrap()..src.find("e`").unwrap();
        assert_eq!(for_highlight(src, d..d + 1), Landing::Highlight(sealed));
    }

    /// The repetition stops as soon as it returns to a mask it already tried, rather
    /// than spending every round it is allowed. Mutation check: drop that stop and the
    /// fixture spends all 50.
    #[test]
    fn returning_to_a_mask_already_tried_stops_the_repetition() {
        assert_eq!(reading(FENCE_FLIP, 50).parses, 2);
    }

    /// Landing reads the fallback's code exactly as extraction does. Masking every
    /// comment, the fallback unmasked the backtick that closes this real code span, so
    /// a highlight on `a` was written inside the span. Mutation check: mask every
    /// comment in the fallback again.
    #[test]
    fn landing_on_the_fallback_keeps_a_real_code_span_whole() {
        let src = "Use `a {>>`<<} b.\n\n````{>>`<<} z\n";
        assert_eq!(reading(src, SETTLE_ROUNDS).outcome, Outcome::Unsettled);
        let span = src.find('`').unwrap()..src.find("<<}").unwrap();
        let a = src.find("a {").unwrap();
        assert_eq!(for_highlight(src, a..a + 1), Landing::Highlight(span));
    }

    /// The fallback is reported, once per document: whether a document settles is a
    /// property of its text, and every render reads it twice.
    #[test]
    fn the_fallback_is_reported_once_per_document() {
        let src = "````{>>`<<} once\n\nAfter {>>c ` d<<} `e`.\n";
        let cap = crate::testlog::capture();
        let _ = as_written(src);
        let _ = as_written(src);
        let at = |level| {
            cap.records()
                .iter()
                .filter(|r| r.level == level && r.message.contains("did not settle"))
                .count()
        };
        assert_eq!(at(log::Level::Warn), 1, "{:?}", cap.records());
        assert_eq!(at(log::Level::Debug), 1, "{:?}", cap.records());
        let mut seen = std::collections::VecDeque::new();
        assert!((0..UNSETTLED_MEMORY as u64).all(|k| remember(&mut seen, k)));
        assert!(!remember(&mut seen, 0), "a remembered document is not new");
        assert!(remember(&mut seen, 99));
        assert!(remember(&mut seen, 0), "the oldest is forgotten");
    }

    /// Extraction knows no backslash escapes, so neither may the lexer's comments:
    /// `\{>>` opens one. Read as an escape, the comment after the code span went
    /// unmasked and its backtick paired with a later one.
    #[test]
    fn a_backslash_before_a_comment_does_not_escape_it() {
        let src = "{>>`<<}``\\{>>`<<} a\\``\\{>>`<<}<<}{>>`<<}{>>{>>`<<}\\{>>`<<} a `";
        assert_eq!(reading(src, 1).outcome, Outcome::Settled);
    }
}
