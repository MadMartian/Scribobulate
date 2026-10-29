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
//! **If even that does not settle**, the reading is the file's own parse, so its block
//! structure is what other tools read and no fence is made or hidden. The one
//! exception is a fence pasted into a comment and never closed, which runs on past
//! the comment (to the end of the file, its container's end or the document's own next
//! fence): that comment alone is masked, and past a few of them every later comment
//! pasting a fence is masked in one step ([`unsettled`]). Each comment
//! that parse would let close past a closer it hides in code
//! is read, with the code it runs into, as code ([`sealed`]). That reading can mistake
//! prose for code, but never lets a comment close on a closer that is not its own, so
//! it can never swallow prose.
//!
//! **One rule for a blank line.** [`blank_lines`] is the block break every CriticMarkup
//! construct may not cross: extraction refuses a construct across one, creating an
//! annotation turns a selection across one into a point comment
//! ([`holds_block_break`]), and landing reads a line holding nothing else as blank
//! ([`is_blank_line_char`]).

use super::mutate::{COMMENT_CLOSE, COMMENT_OPEN};
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

/// The byte every masked body byte becomes.
const FILLER: u8 = b'x';

/// How many parses [`settle`] spends checking the mask before it falls back to the
/// file's own parse. The lexer's mask is normally confirmed by the first.
const SETTLE_ROUNDS: usize = 4;

/// How many bodies [`unsettled`] masks one parse at a time before it masks the rest
/// in one step. An unclosed fence pasted into a comment is rare, so honest documents
/// stay within it.
const UNSETTLED_ROUNDS: usize = 4;

/// How many parses [`mask_and_parse`] spends filling the indentation of only the bodies
/// a code block starts in before it fills every body's.
const FILL_ROUNDS: usize = 2;

/// How many recent documents [`first_time_unsettled`] remembers, so a document that
/// keeps taking the fallback is reported once and not on every render.
const UNSETTLED_MEMORY: usize = 8;

/// The file as other Markdown tools read it. See the module docs.
pub(super) struct AsWritten<'a> {
    /// `source` with the body of every comment outside code masked, or, when no mask
    /// settled, `source` with only [`unsettled`]'s bodies masked: the same bytes at the
    /// same offsets otherwise.
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
        let (text, parsed) = mask_and_parse(source, from, &mask);
        let code = parsed.all;
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
    let (text, code) = unsettled(source, from);
    let written = AsWritten {
        text,
        body_from: from,
        code: sealed(source, from, code),
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

/// The text and code the fallback reads: the file's own, except for each comment body
/// in which a code block starts that ends outside it — a fence the annotator pasted
/// and never closed. Read as written, that fence runs on past the comment: to the end
/// of the file, to the end of the quote or list item it is in, or to the document's
/// own next fence, which then opens a block where the page has none. Every annotation
/// in that code vanishes. So that body is masked, which leaves no code block in it
/// ([`mask_and_parse`]), and the file is parsed again. The fence's end outside the body
/// is document text, so once the body is prose it reads as the page reads it.
///
/// A block that opens in a body and closes in it is left as it is: its closing fence
/// is the annotator's too, and it hides no closer.
///
/// Each body is found by a parse of the whole file, so a run of comments that each
/// paste a fence that cannot close (one with an info string) cost one parse per comment
/// and hung the window on a large file. So after [`UNSETTLED_ROUNDS`] bodies the rest
/// are masked in one step ([`fence_bodies_after`]). [`sealed`] then reads whatever
/// code results, so no reading of it swallows prose.
fn unsettled(source: &str, from: usize) -> (Cow<'_, str>, Vec<Range<usize>>) {
    let mut bodies: Vec<Range<usize>> = Vec::new();
    let mut text = Cow::Borrowed(source);
    let mut parsed = parse_code(source, from);
    // Each round masks at least one more body, and the last masks the rest at once.
    for round in 0..=UNSETTLED_ROUNDS {
        let open = outside_code(source, from, &parsed.all);
        let Some((block, body)) =
            escaping_block(source, &parsed.blocks, &open).filter(|(_, b)| !bodies.contains(b))
        else {
            break;
        };
        bodies.push(body);
        if round == UNSETTLED_ROUNDS {
            let mut rest = fence_bodies_after(source, from, &parsed.all, &block);
            rest.retain(|body| !bodies.contains(body));
            bodies.extend(rest);
        }
        (text, parsed) = mask_and_parse(source, from, &bodies);
    }
    (text, parsed.all)
}

/// The first of `blocks` that starts inside one of `bodies` (both ascending) and ends
/// past it, with that body.
fn escaping_block(
    source: &str,
    blocks: &[Range<usize>],
    bodies: &[Range<usize>],
) -> Option<(Range<usize>, Range<usize>)> {
    blocks.iter().find_map(|block| {
        let body = &bodies[opened_in(source, bodies, block.start)?];
        (block.end > body.end).then(|| (block.clone(), body.clone()))
    })
}

/// Which of `bodies` (ascending) a code block starting at `at` opens in: the one
/// holding `at`, or the indentation before it on its line, since pulldown starts an
/// indented code block after its indentation (`# {>>a\n    <<}` opens one on the
/// closer).
fn opened_in(source: &str, bodies: &[Range<usize>], at: usize) -> Option<usize> {
    let line = source[..at].rfind('\n').map_or(0, |n| n + 1);
    let i = bodies.partition_point(|body| body.end < at);
    let body = bodies.get(i)?;
    (body.start <= at && (at < body.end || body.start <= line)).then_some(i)
}

/// The one of `ranges` (ascending, non-overlapping) that holds byte `p`.
fn containing(ranges: &[Range<usize>], p: usize) -> Option<&Range<usize>> {
    ranges
        .get(ranges.partition_point(|r| r.end <= p))
        .filter(|r| r.start <= p)
}

/// Every body after `block`'s start, outside the rest of `code`, that holds a line a
/// fence could open, with comments paired as if `block` were prose, as the page reads
/// it once the body `block` opens in is masked.
fn fence_bodies_after(
    source: &str,
    from: usize,
    code: &[Range<usize>],
    block: &Range<usize>,
) -> Vec<Range<usize>> {
    let before: Vec<Range<usize>> = code.iter().filter(|r| *r != block).cloned().collect();
    outside_code(source, from, &before)
        .into_iter()
        .filter(|body| body.start > block.start && holds_fence(source, body))
        .collect()
}

/// Whether a line of `body` other than its first could open a fence: after the
/// indentation and quote markers, three backticks or tildes.
fn holds_fence(source: &str, body: &Range<usize>) -> bool {
    source[body.clone()].split('\n').skip(1).any(|line| {
        let rest = line.trim_start_matches([' ', '\t', '>']);
        rest.starts_with("```") || rest.starts_with("~~~")
    })
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

/// `source` with each of `bodies` masked, and its parse, such that no code begins in a
/// body: a body is the annotator's prose, not the document's.
///
/// [`masked`] overwrites all block markup in a body but the indentation that starts
/// each later line, which is also what keeps that line in the list item or quote the
/// comment opened in: filled, a line after a heading in a list item fell out of it,
/// which ended the list, and a later item paragraph became an indented code block. So
/// the indentation is kept, and filled only in a body that a code block starts in (the
/// four columns that open an indented code block after a heading or a rule), then the
/// text is parsed again. After [`FILL_ROUNDS`] parses every body's is filled, which
/// leaves no code able to start in any, so this costs at most that many parses more.
fn mask_and_parse<'a>(
    source: &'a str,
    from: usize,
    bodies: &[Range<usize>],
) -> (Cow<'a, str>, ParsedCode) {
    let mut sorted = bodies.to_vec();
    sorted.sort_by_key(|body| body.start);
    let mut filled = vec![false; sorted.len()];
    let mut parses = 0;
    loop {
        let text = masked(source, &sorted, &filled);
        let parsed = parse_code(&text, from);
        parses += 1;
        let mut fill = false;
        for block in &parsed.blocks {
            if let Some(i) = opened_in(&text, &sorted, block.start).filter(|&i| !filled[i]) {
                filled[i] = true;
                fill = true;
            }
        }
        if !fill {
            return (text, parsed);
        }
        if parses == FILL_ROUNDS {
            filled.fill(true);
        }
    }
}

/// `source` with each of `bodies` (ascending) masked: every byte the parser does not
/// read as whitespace is overwritten, so a list marker or a fence in a multi-line body
/// is masked like any text; where `filled` says so, the indentation that starts each
/// later line of the body is too. Line ends and the whitespace within a line are kept,
/// so every line stays a line (a masked body never holds a blank one: it does not
/// cross a block break). See [`mask_and_parse`].
fn masked<'a>(source: &'a str, bodies: &[Range<usize>], filled: &[bool]) -> Cow<'a, str> {
    if bodies.is_empty() {
        return Cow::Borrowed(source);
    }
    let mut bytes = source.as_bytes().to_vec();
    for (body, &fill) in bodies.iter().zip(filled) {
        // Whether the bytes so far on this line of the body are its indentation.
        let mut leading = false;
        for b in &mut bytes[body.clone()] {
            if *b == b'\n' {
                leading = fill;
            } else if !is_markdown_whitespace(*b) {
                leading = false;
                *b = FILLER;
            } else if leading && *b != b'\r' {
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
/// and masking the body leaves no code block there ([`masked`]). Trusted as a stop,
/// an unclosed one hid the rest of the file from the lexer.
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

/// Every blank line in `text` ([`is_blank_line_char`]), as the span from the line break
/// before it through its own: a span of the text crosses a block break exactly when it
/// contains one of these whole. A line of spaces and tabs ends a paragraph just as an
/// empty one does, in either line ending, so `\n\n`, `\n \t\n` and CRLF's `\n\r\n` all
/// count; so does a line of nothing but a quote's `>` markers, a blank line inside the
/// quote (`> a\n>\n> b` is two paragraphs).
///
/// Only `\n` and `\r\n` end a line here: a document never holds a lone `\r`
/// (`crate::lineendings`).
pub(super) fn blank_lines(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let b = text.as_bytes();
    text.match_indices('\n').filter_map(move |(p, _)| {
        let gap = b[p + 1..]
            .iter()
            .take_while(|&&c| is_blank_line_char(char::from(c)))
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
fn is_line_space(c: char) -> bool {
    matches!(c, ' ' | '\t')
}

/// What a blank line may hold besides its line end: line space ([`is_line_space`]) and a
/// block quote's `>` markers. The one rule for a blank line, which landing reads too.
///
/// Inside a quote, a line of only markers is the quote's blank line; outside one, a `>`
/// opens a quote, which ends the paragraph before it just the same. The one place
/// pulldown disagrees is a `>` indented four columns or more past its container, which
/// is paragraph text: counting that line blank only ever refuses a construct, or turns a
/// selection into a point comment, so it errs the safe way.
pub(super) fn is_blank_line_char(c: char) -> bool {
    c == '>' || is_line_space(c)
}

/// Whether byte `i` lies inside one of `code` (ascending, non-overlapping).
pub(super) fn in_code(code: &[Range<usize>], i: usize) -> bool {
    containing(code, i).is_some()
}

/// What one parse of `text[from..]` reads as code, in `text`'s coordinates.
struct ParsedCode {
    /// Every code span and code block, ascending.
    all: Vec<Range<usize>>,
    /// The code blocks alone.
    blocks: Vec<Range<usize>>,
}

#[cfg(test)]
thread_local! {
    /// How many times [`parse_code`] has run on this thread, so a test can bound it.
    pub(super) static PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn parse_code(text: &str, from: usize) -> ParsedCode {
    #[cfg(test)]
    PARSES.set(PARSES.get() + 1);
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

    /// Masking keeps line ends and the whitespace within a line, so a whitespace-only
    /// line next to a body stays blank and the paragraphs either side stay apart.
    /// Overwriting the space joined them, which paired the backticks either side, which
    /// unmasked the comment, which split them again: the mask flipped every round. The
    /// indentation that starts a later line of a body is masked only where asked (see
    /// `an_indented_line_in_a_comment_after_a_heading_is_not_code`).
    #[test]
    fn masking_keeps_blank_lines_and_line_ends() {
        let src = "{>>a `\n\t b<<}";
        assert_eq!(masked(src, &one(3..10), &[false]), "{>>x x\n\t x<<}");
        assert_eq!(masked(src, &one(3..10), &[true]), "{>>x x\nxxx<<}");
        assert_eq!(
            masked("{>>a\x0b\x0c`\r\n<<}", &one(3..9), &[true]),
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

    /// An indented code block may follow a heading or a rule directly, so a body line
    /// indented four columns after a comment opened on one was code to the parse, which
    /// hid the comment's closer, and extraction paired the opener with the next
    /// comment's closer instead, swallowing the prose between. Masking the indentation
    /// leaves no code there, so the comment closes on its own closer. Masking it in
    /// every body instead closed the list item of the last case after its heading, so
    /// the item's later paragraph read as an indented code block and `c` vanished: it
    /// is masked only where code starts. Mutation checks: never fill the indentation in
    /// `mask_and_parse` and the comments read `["a\n    b<<}\nmore {>>c"]`; always fill
    /// it and the last case loses `c`.
    #[test]
    fn an_indented_line_in_a_comment_after_a_heading_is_not_code() {
        for (src, first) in [
            ("# {>>a\n    b<<}\nmore {>>c<<}", "a\n    b"),
            ("***\n{>>a\n    b<<}\nmore {>>c<<}", "a\n    b"),
            ("# {>>a\r\n\t b<<}\r\nmore {>>c<<}", "a\r\n\t b"),
            ("# {>>a\n    <<}\nmore {>>c<<}", "a\n    "),
            ("- # {>>a\n  b<<}\n\n    text {>>c<<}\n", "a\n  b"),
        ] {
            assert_eq!(comments(src), [first, "c"], "{src:?}");
            let r = reading(src, SETTLE_ROUNDS);
            assert_eq!(r.outcome, Outcome::Settled, "{src:?}");
            assert!(!swallows(src, &r.written.code), "{src:?}");
            assert!(r.written.code.is_empty(), "{src:?}: {:?}", r.written.code);
        }
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
        assert_eq!(breaks("> a\n>\n> b"), one(3..6), "a quote's blank line");
        assert_eq!(breaks("> a\r\n> \r\n> b"), one(4..9));
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
        let lines = [
            "", " ", "\t", " \t ", "\x0b", "\x0c", " \x0c", "x", " x", ">", "> ", " >\t", ">>",
            "> >", "   >",
        ];
        for line in lines {
            for eol in ["\n", "\r\n"] {
                for quote in ["", "> ", "> > "] {
                    let src = format!("{quote}a{eol}{quote}{line}{eol}{quote}b");
                    let want = paragraphs(&src) == 2;
                    assert_eq!(holds_block_break(&src), want, "{src:?}");
                }
            }
        }
        // The one disagreement, on the safe side: a `>` four columns in is text.
        assert_eq!(paragraphs("a\n    >\nb"), 1);
        assert!(holds_block_break("a\n    >\nb"));
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

    /// Over random runs of backticks, comments, escapes, words, line breaks, headings,
    /// indentation and pasted fences, no comment ever holds a closer (that is what
    /// swallowing is). Within one line that cannot be a fence, the lexer's mask is
    /// confirmed within the rounds allowed; across lines, block structure can leave no
    /// mask to confirm (see `a_mask_that_makes_a_fence_has_no_settled_reading`), and
    /// then the reading is the file's own parse, sealed, masked only where a code block
    /// escapes the comment it starts in.
    #[test]
    fn no_mix_of_backticks_and_comments_swallows_prose() {
        const TOKENS: [&str; 12] = [
            "{>>",
            "<<}",
            "`",
            "``",
            " ",
            "a",
            "\\",
            "{>>`<<}",
            "\n",
            "# ",
            "    ",
            "{>>try:\n```\n<<}",
        ];
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
                assert!(!swallows(&src, &r.written.code), "{src:?}");
                if r.outcome == Outcome::Unsettled && r.written.text != src {
                    let open = outside_code(&src, 0, &raw.all);
                    let escapes = escaping_block(&src, &raw.blocks, &open).is_some();
                    assert!(escapes, "{src:?}: masked with no block escaping a comment");
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

    /// A fence pasted into a comment and never closed runs to the end of the file in
    /// the file's own parse, so the fallback read every later annotation as code: `note`
    /// vanished, and a highlight on `Later` was widened to the end of the file. Masking
    /// that one body leaves the rest of the file prose. Mutation check: mask nothing in
    /// `unsettled` and `note` is lost again.
    #[test]
    fn an_unclosed_fence_pasted_into_a_comment_does_not_make_the_fallback_code() {
        let src = "Use the ` key {>>try:\n```\nfoo()\n<<} and `x`.\n\nLater {>>note<<} here.\n";
        assert_eq!(reading(src, SETTLE_ROUNDS).outcome, Outcome::Unsettled);
        assert_eq!(
            parse_code(src, 0).blocks.last().map(|b| b.end),
            Some(src.len()),
            "precondition: the file's own parse is code to its end"
        );
        let written = as_written(src);
        assert!(!swallows(src, &written.code), "{:?}", written.code);
        assert!(
            comments(src).contains(&"note".to_string()),
            "{:?}",
            comments(src)
        );
        let later = src.find("Later").unwrap();
        assert_eq!(
            for_highlight(src, later..later + 5),
            Landing::Highlight(later..later + 5)
        );
    }

    /// A fence pasted into a comment and never closed pairs, in the file's own parse,
    /// with the document's own next fence, or ends with the quote it is in: either way
    /// the block it opens escapes the comment, and the fallback masked only a block
    /// running to the end of the file. So everything up to the document's opening fence
    /// was code and its closing fence opened a block to the end, or the comment's own
    /// closer was hidden: annotations there were read as code. Mutation check: mask
    /// only a block running to the end of the file in `unsettled` again.
    #[test]
    fn a_pasted_fence_closed_outside_its_comment_does_not_make_the_fallback_code() {
        for (tail, want) in [
            (
                "\nUse {>>try:\n```\n<<} it, {>>note<<} here.\n\n```\ncode\n```\n\n\
                 Later {>>z<<} end.\n",
                &["try:\n```\n", "note", "z"][..],
            ),
            (
                "\n> Use {>>try:\n> ```\n> <<} it.\n\nLater {>>z<<} end.\n",
                &["try:\n> ```\n> ", "z"][..],
            ),
        ] {
            let src = FENCE_FLIP.to_string() + tail;
            assert_eq!(reading(&src, SETTLE_ROUNDS).outcome, Outcome::Unsettled);
            let raw = parse_code(&src, 0);
            let body = src.find("try").unwrap();
            assert!(
                raw.blocks
                    .iter()
                    .any(|b| b.start > body && b.end < src.len()),
                "precondition: a block opens past the pasted fence and ends before the \
                 end of the file: {:?}",
                raw.blocks
            );
            let got = comments(&src);
            for w in want {
                assert!(got.contains(&w.to_string()), "{tail:?}: {got:?}");
            }
            assert!(!swallows(&src, &as_written(&src).code), "{tail:?}");
            let later = src.find("end.").unwrap();
            assert_eq!(
                for_highlight(&src, later..later + 3),
                Landing::Highlight(later..later + 3),
                "{tail:?}"
            );
        }
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
