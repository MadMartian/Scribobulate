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
//! repetition continues for a few rounds. A check that finds a fence pasted into a
//! comment running past it masks every later comment pasting a fence in one step, as
//! the fallback below does, since each such fence can hide the next from the lexer.
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

/// The most parses one [`mask_and_parse`] takes: [`FILL_ROUNDS`], then one with every
/// body's indentation filled.
const MASK_PARSES: usize = FILL_ROUNDS + 1;

/// The most parses [`as_written`] takes on any document, the one statement of this
/// bound: the raw parse, [`SETTLE_ROUNDS`] mask checks, then the fallback's own raw
/// parse and its [`UNSETTLED_ROUNDS`] + 1 rounds, each check and round one
/// [`mask_and_parse`]. Constant in the size of the document and in its number of
/// comments.
pub(super) const MAX_PARSES: usize =
    1 + SETTLE_ROUNDS * MASK_PARSES + 1 + (UNSETTLED_ROUNDS + 1) * MASK_PARSES;

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

/// What [`settle`] read, how, how many masks it checked, and how many parses that and
/// any fallback took.
struct Reading<'a> {
    written: AsWritten<'a>,
    outcome: Outcome,
    rounds: usize,
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
    debug_assert!(reading.parses <= MAX_PARSES, "{} parses", reading.parses);
    if reading.outcome == Outcome::Unsettled {
        #[cfg(test)]
        FALLBACKS.set(FALLBACKS.get() + 1);
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
        "annotate: which comments lie outside code did not settle in {} mask checks \
         ({} comment openers, the first that changed at byte {}); reading the file's own \
         code instead ({} parses in all)",
        reading.rounds,
        source.matches(COMMENT_OPEN).count(),
        reading
            .flipped
            .map_or_else(|| "?".to_string(), |at| at.to_string()),
        reading.parses,
    );
}

/// Check `mask` against the parser, repeating until it is confirmed, for at most
/// `limit` checks; then, or on returning to a mask already tried, read the file's own
/// parse with [`sealed`] (see the module docs).
fn settle(source: &str, from: usize, mut mask: Vec<Range<usize>>, limit: usize) -> Reading<'_> {
    let mut tried: Vec<Vec<Range<usize>>> = Vec::new();
    let mut flipped = None;
    let mut rounds = 0;
    // The raw parse that found `mask`'s code blocks.
    let mut parses = 1;
    while rounds < limit {
        rounds += 1;
        let (text, parsed, spent) = mask_and_parse(source, from, &mask);
        parses += spent;
        let mut next = outside_code(source, from, &parsed.all);
        if next == mask {
            let written = AsWritten {
                text,
                body_from: from,
                code: parsed.all,
            };
            return Reading {
                written,
                outcome: Outcome::Settled,
                rounds,
                parses,
                flipped: None,
            };
        }
        // A block opening in a body this mask left unmasked and running past it is a
        // pasted fence; one the document's own next fence closes turns that fence's
        // closing line into an opener over every later comment, and each check would
        // uncover just one more of them. So every later body pasting a fence is masked
        // in this step, as the fallback does ([`unsettled`]), and the next check
        // confirms it or not.
        if let Some((block, _)) = escaping_block(source, &parsed.blocks, &next) {
            let mut rest = fence_bodies_after(source, from, &parsed, &block);
            rest.retain(|body| !next.contains(body));
            if !rest.is_empty() {
                next = merged(next.into_iter().chain(rest).collect());
            }
        }
        flipped = first_difference(&mask, &next);
        if tried.contains(&next) {
            break;
        }
        tried.push(std::mem::replace(&mut mask, next));
    }
    let (text, code, spent) = unsettled(source, from);
    let written = AsWritten {
        text,
        body_from: from,
        code: sealed(source, from, code),
    };
    Reading {
        written,
        outcome: Outcome::Unsettled,
        rounds,
        parses: parses + spent,
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
/// code results, so no reading of it swallows prose. Also returns how many parses it
/// took ([`MAX_PARSES`]).
fn unsettled(source: &str, from: usize) -> (Cow<'_, str>, Vec<Range<usize>>, usize) {
    let mut bodies: Vec<Range<usize>> = Vec::new();
    let mut text = Cow::Borrowed(source);
    let mut parsed = parse_code(source, from);
    let mut parses = 1;
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
            let mut rest = fence_bodies_after(source, from, &parsed, &block);
            rest.retain(|body| !bodies.contains(body));
            bodies.extend(rest);
        }
        let spent;
        (text, parsed, spent) = mask_and_parse(source, from, &bodies);
        parses += spent;
    }
    (text, parsed.all, parses)
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

/// Every body after `block`'s start that holds a line a fence could open, with
/// comments paired as if no code block began from `block` on.
///
/// Every code block from `block` on is set aside, not `block` alone: a pasted fence the
/// document's own next fence closes turns that fence's closing line into an opener, and
/// the block it opens runs over later comments whose pasted fences would then never be
/// masked. Read from a parse with `block`'s body masked instead, the next pasted fence
/// does the same, one parse per fence again. Inline code is kept: no code block it
/// could be hidden in is left to make it up. A comment written inside a document's own
/// code block past `block` that holds such a line is masked as well, which changes
/// nothing but that block's text unless the line would close it.
fn fence_bodies_after(
    source: &str,
    from: usize,
    parsed: &ParsedCode,
    block: &Range<usize>,
) -> Vec<Range<usize>> {
    let blocks_after = &parsed.blocks[parsed.blocks.partition_point(|b| b.start < block.start)..];
    let before: Vec<Range<usize>> = parsed
        .all
        .iter()
        .filter(|r| !blocks_after.contains(r))
        .cloned()
        .collect();
    outside_code(source, from, &before)
        .into_iter()
        .filter(|body| body.start > block.start && holds_fence(source, body))
        .collect()
}

/// Whether a line of `body` other than its first could open a fence: after the
/// indentation, quote markers and list markers, three backticks or tildes.
fn holds_fence(source: &str, body: &Range<usize>) -> bool {
    source[body.clone()].split('\n').skip(1).any(|line| {
        let mut rest = line;
        loop {
            rest = rest.trim_start_matches([' ', '\t', '>']);
            match past_list_marker(rest) {
                Some(after) => rest = after,
                None => break,
            }
        }
        rest.starts_with("```") || rest.starts_with("~~~")
    })
}

/// `line` past the list marker it starts with — `-`, `+` or `*`, or up to nine digits
/// and `.` or `)` — and the space or tab that must follow one, if it starts with one.
fn past_list_marker(line: &str) -> Option<&str> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let marker = match line.as_bytes().get(digits)? {
        b'-' | b'+' | b'*' if digits == 0 => 1,
        b'.' | b')' if (1..=9).contains(&digits) => digits + 1,
        _ => return None,
    };
    let after = &line[marker..];
    after.starts_with([' ', '\t']).then_some(after)
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
/// leaves no code able to start in any, so this costs at most [`MASK_PARSES`] parses,
/// the number it returns with the text and its parse.
fn mask_and_parse<'a>(
    source: &'a str,
    from: usize,
    bodies: &[Range<usize>],
) -> (Cow<'a, str>, ParsedCode, usize) {
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
            return (text, parsed, parses);
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
    /// How many times [`as_written`] has taken the fallback on this thread, so a test
    /// can tell that it did.
    pub(super) static FALLBACKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
mod tests;
