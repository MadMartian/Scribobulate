//! The one rule about line separators this application holds: **a document buffer
//! never contains a lone carriage return.**
//!
//! # Why this module exists
//!
//! The editor half and the Markdown half of this application disagree about what a
//! bare `\r` (U+000D, no `\n` after it) means, and the disagreement is total. Both
//! halves were MEASURED on 2026-08-20, GTK 4.22.4 / Quartz / pulldown-cmark 0.13:
//!
//! * **GTK treats a lone `\r` as a line separator.** `gtk_text_buffer_set_text`
//!   with `"# Title\rpara\r- a\r- b\r"` stores the bytes verbatim and reports
//!   `line_count = 5`; `gtk_text_iter_forward_line` walks the four lines; the
//!   footer's Ln/Col indicator counts them; GtkSourceView's Markdown grammar even
//!   syntax-highlights `# Title` as a heading. The editor pane looks entirely correct.
//! * **pulldown-cmark does not.** A bare `\r` is not a block-level line ending to it,
//!   so `"# Title\rpara\r- a\r- b\r"` parses to exactly one `Start(Heading(H1))` whose
//!   `Text` is the whole document. Headings, blank lines and list structure all
//!   vanish. (It *is* honoured at inline level — `"line1\rline2\n"` yields a
//!   `SoftBreak` — which is why the failure looks like partial support rather than
//!   none.)
//!
//! Every derived surface is built on the second half, so a document carrying lone
//! CRs renders as one giant heading in the preview, collapses to a single entry in
//! the outline sidebar, and exports that way — while the editor beside it shows the
//! text laid out correctly. That split screen is the bug's signature.
//!
//! # Where a lone CR comes from
//!
//! Not from this application, and not from any file a modern tool writes. It arrives
//! on the clipboard from a **keyboard/mouse sharing tool** (Synergy and friends)
//! whose clipboard bridge still converts line endings to the *classic Mac OS*
//! convention when the receiving machine is a Mac. macOS has used `\n` since Mac OS X
//! shipped, so that conversion is a two-decade-old anachronism — but it is on the
//! wire, and `gdk_clipboard_read_text` does not normalise it (MEASURED: a lone-CR
//! pasteboard string arrives byte-identical).
//!
//! # Why lone CR only, and not CRLF
//!
//! CRLF parses identically to LF in pulldown-cmark (MEASURED), renders correctly, and
//! round-trips to disk today — `tests/fixtures/crlf-doc.md` exists to keep it doing
//! so. Collapsing CRLF here would silently rewrite every Windows-authored document on
//! open, which is a *line-ending policy* decision this application has not taken (see
//! the open item at `tests/MANUAL-TEST.md` §4.2, "Round-trip CRLF"). This module
//! deliberately settles nothing there: it repairs only the sequence that no part of
//! the stack agrees on.
//!
//! The narrow scope buys a property worth having: replacing a lone `\r` with `\n` is
//! **byte-length- and position-preserving**, one ASCII byte for one ASCII byte, so
//! every source offset the renderer, `copymap`, `source_map` and the scroll sync
//! capture still indexes the same logical position. That is the same contract
//! `renderer::NormalizedMd`'s pre-pass states, and for the same reason.
//!
//! # Where the repair goes: the two doors, not the parse sites
//!
//! Normalising inside the parse pre-pass (`renderer::NormalizedMd`) would fix
//! only what is *parsed*, and arrival is a different moment from parsing: the save
//! guard, crash recovery and the editor buffer never go through a parse at all, so a
//! lone `\r` would survive in each of them. (That pre-pass does now cover all four
//! parse sites, so this is a statement about its MOMENT, not its reach — an earlier
//! version of this note leaned on the reach gap, which has since been closed.)
//! Repairing where text *arrives* means every consumer is correct without any of them
//! knowing this module exists.
//!
//! There are exactly two such doors, and both are needed. Fixing only one of them was
//! the first attempt at this and it looked convincing:
//!
//! 1. **`docio`'s readers**, beside the BOM strip, for text arriving from a file. This
//!    is the load-bearing one, because it produces the *source string* — and the
//!    preview, the outline and `copymap` are all built from that string, never from the
//!    editor buffer. Repairing the buffer alone (via `window::actions::load_into_editor`)
//!    therefore fixed the editor, which already looked right, and left the preview
//!    rendering the whole document as one heading. Every automated test still passed;
//!    only a screenshot against an LF twin showed it.
//! 2. **The clipboard**, for text arriving by paste, drag-and-drop or middle-click
//!    PRIMARY. That is the route this defect was actually reported through, and it is
//!    closed by the `insert-text` hook [`new_editor_buffer`] arms — but only because
//!    [`crate::clipboard`] landed first and stopped publishing a `GtkTextBuffer`, so a
//!    same-application paste arrives as one untagged emission. An earlier version of
//!    that hook, wired while GTK's rich content was still being published, corrupted
//!    CRLF on a same-application paste; ScrAP-312 records why.
//!
//! ScrAP-312 records the whole episode, including the version of this fix that repaired
//! the buffer alone and passed every gate. Both halves of the disagreement were
//! re-measured on X11 as well as Quartz, so nothing here is macOS-specific.
//!
//! # Arming and populating are ONE step, deliberately
//!
//! [`new_editor_buffer`] is the only route to an armed buffer: the arming function is
//! private to this module, so no caller can create a buffer, populate it, and arm the
//! repair afterwards. That ordering is not stylistic — the whole soundness argument for
//! repairing during an **undo replay** rests on it, and ScrAP-316 records the decision.

use gtk::prelude::*;
use std::borrow::Cow;

/// A carriage return, and a line feed. Named because a bare byte literal in the
/// scanning loops below reads as noise.
const CR: u8 = b'\r';
const LF: u8 = b'\n';

/// Whether `text` contains at least one **lone** carriage return — a `\r` that is not
/// immediately followed by `\n`.
///
/// This is the guard every caller leads with, and its cheapness is the point: the
/// answer is `false` for every document this application produces and for every
/// same-application copy, so the repair path below is entered only by text that
/// genuinely arrived from outside carrying the anachronistic convention.
pub(crate) fn has_lone_cr(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes
        .iter()
        .enumerate()
        .any(|(i, &b)| b == CR && bytes.get(i + 1) != Some(&LF))
}

/// Rewrite every **lone** carriage return in `text` to a line feed, leaving `\r\n`
/// pairs and everything else untouched.
///
/// Returns `Cow::Borrowed` when there was nothing to do, which is the overwhelmingly
/// common case.
///
/// # Why a byte scan is safe on a `&str`
///
/// `\r` and `\n` are ASCII, and UTF-8 guarantees every byte of a multi-byte sequence
/// has its high bit set — so a byte equal to `0x0D` can never be part of one. The
/// substitution is therefore char-safe as well as length-preserving, and the result is
/// still valid UTF-8 by construction.
pub(crate) fn normalize_lone_cr(text: &str) -> Cow<'_, str> {
    if !has_lone_cr(text) {
        return Cow::Borrowed(text);
    }
    // Built as a `String` from the start, so no UTF-8 validity question arises and
    // there is nothing to assert. This used to substitute over a `Vec<u8>` and end in
    // `String::from_utf8(..).expect(..)` — an `expect` in production code, which
    // POLICY § Code style forbids, carrying a comment arguing why it could never fire.
    // The argument was sound; the point is that a structural impossibility beats a
    // correct argument, because the argument has to be re-read and re-believed by
    // whoever edits the loop next.
    //
    // `\r` is ASCII, so a lone CR can never be a continuation byte of some other
    // character and `char_indices` sees exactly the carriage returns a byte scan would.
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    for (i, ch) in text.char_indices() {
        if ch == '\r' && bytes.get(i + 1) != Some(&LF) {
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    Cow::Owned(out)
}

/// [`normalize_lone_cr`] for a raw-bytes reader. Same rule, same reason.
///
/// # Precondition: the bytes are UTF-8 (or at least ASCII-compatible)
///
/// The signature accepts any `Vec<u8>`, but the substitution is only *sound* on an
/// encoding where a `0x0D` byte can never be part of some other character. That holds
/// for UTF-8 (continuation bytes are all `>= 0x80`) and for every ASCII superset, and
/// it does NOT hold for UTF-16 or UTF-32, where `0x0D` occurs inside ordinary code
/// units and this function would corrupt them. The `&str` twin gets this precondition
/// from its type; this one cannot, so it is stated. Every caller today is a document
/// reader, and this application only ever admits UTF-8 documents — the type is wide
/// because the data arrives as bytes, not because the contract is.
///
/// It exists for the same reason `docio`'s `without_bom_bytes` does: crash recovery
/// digests the on-disk twin and compares it against a digest of the in-memory
/// baseline, and a repair applied to one side of that comparison and not the other
/// turns every recovery of a lone-CR document into a spurious stale-baseline verdict.
pub(crate) fn normalize_lone_cr_bytes(mut bytes: Vec<u8>) -> Vec<u8> {
    for i in 0..bytes.len() {
        if bytes[i] == CR && bytes.get(i + 1) != Some(&LF) {
            bytes[i] = LF;
        }
    }
    bytes
}

/// A fresh editor buffer with the no-lone-carriage-return repair already armed — the
/// **only** route to one, and the only reason [`wire_paste_normalization`] below is
/// private.
///
/// # Why creation and arming are one call
///
/// The repair is a `GtkTextBuffer::insert-text` hook, so it repairs what arrives
/// *after* it is wired and can say nothing about what was already there. Split the two
/// steps and the gap between them is a buffer that accepts a lone `\r` and keeps it —
/// which is not merely a missed repair, because a second property rests on that
/// buffer's contents:
///
/// **An undo replay re-enters this hook.** `gtk_text_buffer_history_insert` reaches the
/// buffer through the **public** `gtk_text_buffer_insert`, so the repair fires while
/// `GtkTextHistory` is replaying, and `GtkTextHistory` cannot notice — it sets
/// `applying` across the whole replay and GTK's implementation ignores the
/// `expected_text` its delete path is handed. So on a buffer that holds a lone `\r`,
/// undo restores a `\n` where a `\r` was deleted and *nothing warns*: the history's own
/// model still believes the original bytes went back. MEASURED on GTK 4.22.4 /
/// GtkSourceView 5.20.0 and matching 4.6.9 byte for byte; `probes/undo-replay.c` is the
/// rig.
///
/// **That divergence is unreachable, and this function is what makes it so** — not an
/// ordering convention someone has to remember. No buffer this process builds can hold
/// a lone `\r`: it is armed at birth here, and every route that fills it repairs
/// independently anyway (`docio`'s three readers, and
/// `window::actions::load_into_editor` for swap-recovery text that bypasses them). A
/// replay therefore only ever re-inserts text with no lone `\r` in it, `has_lone_cr`
/// answers `false`, and the hook returns before touching anything.
///
/// # The decision NOT to bracket the replay, and why it is not a free choice
///
/// The alternative is to suppress the repair across a replay — a `connect` plus
/// `connect_after` pair on `undo`/`redo` straddles the default handler exactly — which
/// restores the original bytes unconditionally. **Rejected deliberately** (ScrAP-316):
/// it buys byte-exact undo of a sequence no buffer may legally contain, and pays for it
/// by letting a lone `\r` back *in* through undo, where every derived surface collapses
/// on it — the preview renders the document as one heading, the outline as one entry,
/// and the export follows. The invariant is worth more than the bytes, and the bytes
/// only differ in a state this function prevents. Neither remedy is free, so the choice
/// is recorded rather than left to the next reader to re-derive.
pub(crate) fn new_editor_buffer() -> sourceview::Buffer {
    let buffer = sourceview::Buffer::new(None);
    wire_paste_normalization(&buffer);
    buffer
}

/// Install the clipboard-side half of the no-lone-carriage-return rule on a tab's
/// editor buffer.
///
/// **Private on purpose** — reachable only through [`new_editor_buffer`], so no caller
/// can arm a buffer that has already been populated. See that function for what rests
/// on it.
///
/// # This fix is a fence around the hole, not a filled-in hole — and the fence is marked
///
/// The distinction matters enough to spell out, because it decides what a future editor of
/// unrelated code can break. A fix that is correct **by construction** makes the wrong
/// behaviour impossible: nothing anyone writes later can reintroduce it. This one is
/// correct **by invariant** — the wrong behaviour is entirely possible, and two facts about
/// the rest of the codebase are all that prevent it. Break either, somewhere else, and this
/// silently starts scrambling pasted text with nothing failing.
///
/// Specifically: gtk4-rs's `connect_insert_text` hands this closure a COPY of the caller's
/// `GtkTextIter` and never writes it back, so after the re-insertion below GTK's own
/// iterator IS stale. That is a live defect sitting right here. It does not hurt us only
/// because of the two conditions listed next.
///
/// The mechanism that would be correct by construction is the `insert_text` **vfunc**
/// (gtk4-rs's subclass trampoline writes the caller's iterator back, where
/// `connect_insert_text`'s hands you a COPY and never does). That route is blocked:
/// instantiating **any** `sourceview::Buffer` subclass — even with a completely empty
/// `TextBufferImpl` — corrupts the heap and SIGSEGVs our test harness. Isolated to five
/// arms; ScrAP-314 records them, and the four theories already eliminated. Until that is
/// resolved this hook is the shipping shape, and
/// it is sound **only** while both of the following hold:
///
/// 1. **Every insertion arrives as ONE emission.** The stale-iterator defect bites only
///    when the emission's *caller* dereferences its iterator afterwards, which on the
///    single-emission routes none does — `set_text` and `history_insert` drop their
///    locals, and `insert_range_not_inside_self` touches its iter after the insert only
///    when the source and destination share a tag table, which a plain-text paste never
///    does (it deserialises into `gtk_text_buffer_new(NULL)`). This is why
///    [`crate::clipboard`] had to land FIRST, and why its
///    `a_same_application_paste_arrives_as_a_single_emission` test is load-bearing for
///    this module rather than only for its own.
///
///    **The preview pane is the case that looks like a hole and is not, so it is named
///    here rather than left to be re-derived.** `CodePreviewView` is an ordinary
///    `GtkTextView` for PRIMARY: nothing removes GTK's own `add_selection_clipboard`
///    registration, so a preview selection really does publish a rich
///    `GtkTextBufferContent` (MEASURED —
///    `clipboard::a_preview_selection_still_publishes_gtks_default_rich_content_to_primary`).
///    That does not reach this invariant, because the editor's middle-click paste is a
///    **consumer**-side route: it reads PRIMARY with `read_text_async`, never as
///    `GTK_TYPE_TEXT_BUFFER`, and GDK satisfies a text read from any publisher's content
///    whether tagged or not. So `insert_range_not_inside_self`'s per-tag-toggle chunking
///    is never entered, whichever pane the selection came from (MEASURED —
///    `clipboard::a_preview_selection_pastes_into_the_editor_as_one_plain_text_emission`,
///    which selects a CRLF-bearing document in the preview and asserts exactly one
///    `insert-text` emission with every `\r\n` byte-exact). A review raised this as a live
///    gap on the strength of a publisher-side takeover that no longer exists; the
///    protection is real, it simply sits at the other end.
/// 2. **No code reuses a `TextIter` across a `buffer.insert*()` on a repair-wired
///    buffer.** MEASURED: doing so raises `gtk_text_buffer_insert: assertion
///    'gtk_text_iter_get_buffer (iter) == buffer' failed` and leaves the iterator at
///    offset 0. Use a `TextMark` if you need a position to survive an insertion.
///
///    **CONVENTION-ONLY, declared as such under POLICY § Typed GTK seams — there is no
///    gate, and this comment used to imply there was.** It said the invariant "is
///    greppable, which is the only reason it is acceptable", which named an enforcement
///    that does not exist: nothing greps for it, and a reader was entitled to assume
///    something did. POLICY requires a promotion to carry its mechanism in the same
///    change, and permits an explicit convention-only note where neither a ban nor
///    encapsulation fits. Both were assessed and both were rejected:
///
///    - A `clippy.toml` ban cannot express it. The property is "this iterator is used
///      AFTER that insertion", which is dataflow; the only bannable thing is
///      `TextBufferExt::insert` itself, and POLICY forbids a ban whose true-positive rate
///      does not justify it — that one would fire on every legitimate call in the tree.
///    - Encapsulation would work and is not proportionate here: it means a newtype around
///      the repair-wired buffer whose insertion methods take and return `TextMark`s, and
///      routing every caller through it. That is a real seam and it is written down here
///      rather than left implicit, so whoever next pays for this defect can reach for it
///      with the reasoning already done.
///
///    Until then: this is a rule someone has to remember, and saying so is the point. A
///    seam whose mechanism is "remember to call it" is a latent regression; a seam that
///    ADMITS its mechanism is memory at least tells the next reader what they are
///    relying on.
///
/// Every single-emission route was driven in the real binding with a log handler
/// counting diagnostics — `set_text`, `insert_at_cursor`, `insert_interactive_at_cursor`,
/// undo replay, and a real foreign plain-text paste — all **zero** criticals, all
/// repairing the lone `\r` while leaving `\r\n` intact.
fn wire_paste_normalization(buffer: &sourceview::Buffer) {
    let in_repair = std::cell::Cell::new(false);
    buffer.connect_insert_text(move |buf, iter, text| {
        // Our own re-insertion arriving back through the signal. The flag is STRUCTURAL,
        // not an optimisation: mutation-testing an earlier version by neutering the
        // repair produced `fatal runtime error: stack overflow` rather than a failed
        // assertion, because termination had been resting on the repair being complete
        // instead of on the handler's shape.
        if in_repair.get() || !has_lone_cr(text) {
            return;
        }
        let repaired = normalize_lone_cr(text);
        glib::signal::signal_stop_emission_by_name(buf, "insert-text");
        in_repair.set(true);
        buf.insert(iter, &repaired);
        in_repair.set(false);
    });
}

#[cfg(test)]
mod tests {
    use super::{has_lone_cr, normalize_lone_cr, normalize_lone_cr_bytes};

    /// The bytes twin, tested directly rather than through its `&str` sibling.
    ///
    /// It had no direct test at all — including of the ONE property that makes it a
    /// separate function instead of a `to_string()` and a call to the sibling: it
    /// accepts bytes that are **not** required to round-trip through `String`. Its
    /// caller is crash recovery, which digests the on-disk twin, so the case that
    /// matters is a document whose bytes are not valid UTF-8: the repair must still
    /// apply and must not panic, truncate, or lossily substitute. Testing it only via
    /// UTF-8 fixtures would have proven the sibling and called it this function.
    #[test]
    fn the_bytes_twin_repairs_input_that_is_not_valid_utf8() {
        // A lone CR either side of an invalid byte sequence (0xFF is not legal
        // anywhere in UTF-8), plus a CRLF that must survive untouched.
        let src: Vec<u8> = vec![b'a', b'\r', 0xFF, 0xFE, b'\r', b'b', b'\r', b'\n', b'c'];
        let got = normalize_lone_cr_bytes(src.clone());

        assert_eq!(
            got,
            vec![b'a', b'\n', 0xFF, 0xFE, b'\n', b'b', b'\r', b'\n', b'c'],
            "lone CRs become LF, the CRLF pair is untouched, and the invalid bytes \
             pass through unchanged"
        );
        assert_eq!(got.len(), src.len(), "the repair is length-preserving");
        assert!(
            String::from_utf8(got).is_err(),
            "the fixture must really be invalid UTF-8, or this test proves nothing \
             the `&str` twin's tests do not already prove"
        );
    }

    /// A CR at the very end has no following byte to inspect — the `bytes.get(i + 1)`
    /// bounds case, which is where an off-by-one in either twin would live.
    #[test]
    fn a_trailing_carriage_return_is_repaired_in_both_twins() {
        assert_eq!(
            normalize_lone_cr_bytes(b"end\r".to_vec()),
            b"end\n".to_vec()
        );
        assert_eq!(normalize_lone_cr("end\r"), "end\n");
    }

    /// The two twins agree on the same input, which is the contract that lets crash
    /// recovery digest one side with each and compare the results.
    #[test]
    fn the_two_twins_agree_on_utf8_input() {
        for src in [
            "a\rb",
            "a\r\nb",
            "\r",
            "\r\n",
            "a\r\r\nb",
            "plain\ntext\n",
            "unicode ☃\rsnow",
        ] {
            assert_eq!(
                normalize_lone_cr_bytes(src.as_bytes().to_vec()),
                normalize_lone_cr(src).as_bytes().to_vec(),
                "twins disagreed on {src:?}"
            );
        }
    }

    #[test]
    fn leaves_lf_only_text_borrowed() {
        let src = "# Title\n\npara\n\n- a\n- b\n";
        assert!(!has_lone_cr(src));
        assert!(matches!(
            normalize_lone_cr(src),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn leaves_crlf_untouched() {
        // The Windows convention parses correctly and round-trips to disk today;
        // this module must not be the thing that quietly changes that.
        let src = "# CRLF document\r\n\r\nFirst line.\r\nSecond line.\r\n";
        assert!(!has_lone_cr(src));
        assert_eq!(normalize_lone_cr(src), src);
    }

    #[test]
    fn rewrites_lone_cr_to_lf() {
        let src = "# Title\rSome prose here.\r\r- item one\r- item two\r";
        assert!(has_lone_cr(src));
        assert_eq!(
            normalize_lone_cr(src),
            "# Title\nSome prose here.\n\n- item one\n- item two\n"
        );
    }

    #[test]
    fn rewrites_lone_cr_but_spares_crlf_in_the_same_text() {
        // Mixed input is the realistic clipboard case: a tool that converts only
        // some of what it forwards.
        let src = "a\r\nb\rc\r\nd\r";
        assert_eq!(normalize_lone_cr(src), "a\r\nb\nc\r\nd\n");
    }

    #[test]
    fn cr_at_end_of_text_is_lone() {
        // The final byte has no successor, so the `get(i + 1)` lookahead must treat
        // "nothing follows" as "not an LF" rather than panicking or skipping.
        assert!(has_lone_cr("trailing\r"));
        assert_eq!(normalize_lone_cr("trailing\r"), "trailing\n");
    }

    #[test]
    fn cr_cr_lf_repairs_only_the_first() {
        // `\r\r\n` is a lone CR followed by a CRLF pair, not two lone CRs.
        assert_eq!(normalize_lone_cr("a\r\r\nb"), "a\n\r\nb");
    }

    #[test]
    fn substitution_is_length_and_position_preserving() {
        // The property every source-offset consumer depends on. Asserted directly
        // rather than left as a comment, because it is what makes this repair safe
        // to perform underneath `copymap` / `source_map` without touching them.
        for src in [
            "a\rb",
            "# h\r\rtext\r",
            "\r",
            "\r\n\r",
            "mixed\r\nlone\rmore",
            "unicode ✓\rnext ✓\r",
        ] {
            let out = normalize_lone_cr(src);
            assert_eq!(out.len(), src.len(), "byte length changed for {src:?}");
            assert_eq!(
                out.chars().count(),
                src.chars().count(),
                "char count changed for {src:?}"
            );
        }
    }

    #[test]
    fn multibyte_text_is_not_corrupted() {
        // A byte scan over a `&str` is only safe because CR cannot appear inside a
        // multi-byte sequence. Pin it with characters whose encodings are 2, 3 and
        // 4 bytes wide.
        let src = "é\r漢\r😀\r";
        assert_eq!(normalize_lone_cr(src), "é\n漢\n😀\n");
    }

    #[test]
    fn empty_text_is_handled() {
        assert!(!has_lone_cr(""));
        assert_eq!(normalize_lone_cr(""), "");
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::saferizer::BufferText;

    /// Load `md` as a baseline edit that is not itself undoable, exactly as
    /// `window::actions::load_into_editor` does — otherwise the first undo of the test
    /// pops the load instead of the edit under test.
    fn baseline(buffer: &sourceview::Buffer, md: &str) {
        buffer.set_enable_undo(true);
        buffer.begin_irreversible_action();
        buffer.set_text(md);
        buffer.end_irreversible_action();
    }

    /// **A buffer from `new_editor_buffer` repairs the very first insertion.**
    ///
    /// The arming is what makes the undo-replay divergence below unreachable, and the
    /// only thing that guarantees it happens before anything can populate the buffer is
    /// that creation and arming are one call. Mutation-checked: drop the
    /// `wire_paste_normalization` call from `new_editor_buffer` and this fails with the
    /// lone `\r` intact.
    #[gtktest::test]
    fn a_fresh_editor_buffer_is_armed_before_anything_can_populate_it() {
        let buffer = new_editor_buffer();

        buffer.insert(&mut buffer.start_iter(), "one\rtwo\r\nthree");

        assert_eq!(
            BufferText::of(&buffer).as_str(),
            "one\ntwo\r\nthree",
            "the lone CR must be repaired and the CRLF left alone, with no load first"
        );
    }

    /// **Undo restores the exact bytes a delete removed, on every reachable document.**
    ///
    /// `gtk_text_buffer_history_insert` replays through the PUBLIC
    /// `gtk_text_buffer_insert`, so the repair hook fires during an undo — this is the
    /// assertion that it is a no-op there. The deleted range deliberately spans a CRLF,
    /// the one sequence a repair could damage while still being length-preserving.
    #[gtktest::test]
    fn an_undo_replay_restores_the_exact_bytes_a_delete_removed() {
        const DOC: &str = "first\r\nsecond\r\nthird\n";
        let buffer = new_editor_buffer();
        baseline(&buffer, DOC);

        let (mut start, mut end) = (buffer.iter_at_offset(3), buffer.iter_at_offset(10));
        assert_eq!(
            BufferText::of_range(&buffer, &start, &end).as_str(),
            "st\r\nsec",
            "precondition: the deleted range spans the CRLF, or this proves nothing"
        );
        buffer.delete(&mut start, &mut end);
        buffer.undo();

        assert_eq!(BufferText::of(&buffer).as_str(), DOC);
    }

    /// **If a buffer ever does hold a lone CR, undo REPAIRS it rather than restoring
    /// it — the deliberate half of the trade (ScrAP-316).**
    ///
    /// Reaching this state needs the precondition `new_editor_buffer` exists to prevent:
    /// a buffer populated before the repair is armed. The remedy not taken — bracketing
    /// the replay with a `connect`/`connect_after` pair on `undo`/`redo` — would assert
    /// `"a\rb"` here, restoring the original bytes and letting a lone `\r` back into a
    /// buffer, where the preview collapses the whole document into one heading. This
    /// asserts the choice that was made: the no-lone-CR invariant outranks byte-exact
    /// undo of a sequence no buffer may legally hold.
    #[gtktest::test]
    fn an_undo_replay_repairs_a_lone_cr_rather_than_restoring_it() {
        // Deliberately NOT `new_editor_buffer` — this is the precondition break.
        let buffer = sourceview::Buffer::new(None);
        baseline(&buffer, "a\rb");
        wire_paste_normalization(&buffer);
        assert!(
            has_lone_cr(BufferText::of(&buffer).as_str()),
            "precondition: the buffer really was populated before the repair was armed"
        );

        let (mut start, mut end) = (buffer.start_iter(), buffer.end_iter());
        buffer.delete(&mut start, &mut end);
        buffer.undo();

        assert_eq!(
            BufferText::of(&buffer).as_str(),
            "a\nb",
            "the repair fires during the replay, and the invariant wins over the bytes"
        );
    }
}
