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
    assert_eq!(r.rounds, 1, "the lexer's mask is confirmed at once");
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
        "", " ", "\t", " \t ", "\x0b", "\x0c", " \x0c", "x", " x", ">", "> ", " >\t", ">>", "> >",
        "   >",
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
    assert_eq!(reading(FENCE_FLIP, 50).rounds, 2);
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

/// A pasted fence before the document's own fenced block pairs, in the file's own
/// parse, with that block's opening fence, so its closing fence opens a block to the
/// end of the file. The lexer trusted that block, and each mask check uncovered just
/// one of the comments pasting a fence after it: four of them took more checks than
/// [`settle`] spends, so the document took the fallback, which reads the comment
/// holding a stray backtick as code and lost it. Mutation check: drop the step in
/// `settle` that masks every later body pasting a fence and the document falls back.
#[test]
fn pasted_fences_after_a_document_fence_settle_in_one_step() {
    // A Windows checkout may hold the fixture with CRLF line endings; the case is
    // about fence pairing, not line endings, so read it as LF everywhere.
    let raw = include_str!("../../../tests/fixtures/pasted-fence-after-doc-fence.md");
    let normalised = raw.replace("\r\n", "\n");
    let src = normalised.as_str();
    let found = reading(src, SETTLE_ROUNDS);
    assert_eq!(found.outcome, Outcome::Settled, "{} rounds", found.rounds);
    assert_eq!(found.rounds, 2);
    let got = comments(src);
    assert_eq!(got.len(), 8, "{got:?}");
    assert!(got.contains(&"c ` d".to_string()), "{got:?}");
    let written = as_written(src);
    let e = src.find("`e`").unwrap();
    assert!(written.code.contains(&(e..e + 3)), "{:?}", written.code);
    let doc = src.find("```\ndoc").unwrap();
    assert!(
        written.code.iter().any(|r| r.start == doc),
        "the document's own block stays code: {:?}",
        written.code
    );
    assert!(!swallows(src, &written.code));
}

/// The whole pasted-fence document of MANUAL-TEST 17.61 settles too, with every one of
/// its fourteen comments.
#[test]
fn the_manual_pasted_fence_document_keeps_every_comment() {
    let pasted = "p {>>see\n```py\n<<} q\n\n";
    let src = "{>>`<<} a\n\nAfter {>>c ` d<<} `e`.\n\n".to_string()
        + &pasted.repeat(5)
        + "```\ndoc\n```\n\n"
        + &pasted.repeat(3)
        + "- {>>see\n- ```py\n  x\n  <<} q\n\n"
        + &pasted.repeat(2)
        + "Last {>>z<<}.\n";
    assert_eq!(reading(&src, SETTLE_ROUNDS).outcome, Outcome::Settled);
    let got = comments(&src);
    assert_eq!(got.len(), 14, "{got:?}");
    assert!(got.contains(&"c ` d".to_string()), "{got:?}");
}
