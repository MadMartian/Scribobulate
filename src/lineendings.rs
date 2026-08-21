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
//! `renderer::normalize_inline_tabs` states, and for the same reason.
//!
//! # Where the repair goes: the two doors, not the parse sites
//!
//! Normalising inside the parse pre-pass (`renderer::normalize_inline_tabs`) would fix
//! the preview and leave the outline and `copymap` broken, because they parse the
//! source directly rather than through that pre-pass — it documents itself as covering
//! "every parse site" and already does not. Repairing where text *arrives* means every
//! consumer is correct without any of them knowing this module exists.
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
//!    **NOT closed here** — the obvious mechanism for it (a `insert-text` hook) was
//!    written, measured and rejected because it corrupts CRLF on a same-application
//!    paste. See the note above `has_lone_cr`'s neighbours below, and ScrAP-312.
//!
//! ScrAP-312 records the whole episode, including the version of this fix that repaired
//! the buffer alone and passed every gate. Both halves of the disagreement were
//! re-measured on X11 as well as Quartz, so nothing here is macOS-specific.

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
    let mut out = text.as_bytes().to_vec();
    for i in 0..out.len() {
        if out[i] == CR && out.get(i + 1) != Some(&LF) {
            out[i] = LF;
        }
    }
    // SAFETY-equivalent argument without `unsafe`: the loop replaced ASCII bytes with
    // ASCII bytes at the same offsets, so the buffer is still valid UTF-8 and this
    // conversion cannot fail. `expect` rather than `unwrap_or` so a future edit that
    // broke the invariant would be loud instead of silently lossy.
    Cow::Owned(String::from_utf8(out).expect("ASCII-for-ASCII substitution preserves UTF-8"))
}

/// [`normalize_lone_cr`] for a raw-bytes reader. Same rule, same reason.
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

// The clipboard-side half of this rule is DELIBERATELY ABSENT. A
// `GtkTextBuffer::insert-text` hook that repaired the payload was written, measured and
// REJECTED — it corrupts a CRLF document on a same-application paste, because
// `insert_range_not_inside_self` chunks the copied region on tag toggles and a toggle
// can fall between the `\r` and the `\n`, manufacturing a lone `\r` no buffer contains.
// See ScrAP-312 for the measurements and `probes/` for the rigs. The remedy is to stop
// publishing a `GtkTextBuffer` on the clipboard, which makes every paste arrive in a
// fresh untagged buffer as a single emission; that is a behaviour change across three
// platforms and is not taken here. Until it is, text pasted from outside carrying lone
// CRs is repaired only if it is subsequently saved and reopened.
//
// If you are about to add that hook back, read ScrAP-312 first, and use
// `TextBufferImpl::insert_text` rather than the signal: gtk4-rs's `connect_insert_text`
// hands the closure a COPY of the caller's iterator and never writes it back, which
// scrambles a multi-run paste outright.

/// Install the clipboard-side half of the no-lone-carriage-return rule on a tab's
/// editor buffer.
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
/// 2. **No code reuses a `TextIter` across a `buffer.insert*()` on a repair-wired
///    buffer.** MEASURED: doing so raises `gtk_text_buffer_insert: assertion
///    'gtk_text_iter_get_buffer (iter) == buffer' failed` and leaves the iterator at
///    offset 0. Use a `TextMark` if you need a position to survive an insertion. This is
///    greppable, which is the only reason it is acceptable as an invariant.
///
/// Every single-emission route was driven in the real binding with a log handler
/// counting diagnostics — `set_text`, `insert_at_cursor`, `insert_interactive_at_cursor`,
/// undo replay, and a real foreign plain-text paste — all **zero** criticals, all
/// repairing the lone `\r` while leaving `\r\n` intact.
pub(crate) fn wire_paste_normalization(buffer: &sourceview::Buffer) {
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
    use super::{has_lone_cr, normalize_lone_cr};

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
