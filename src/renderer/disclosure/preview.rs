//! Pure derivation of a collapsed disclosure's body-opening PREVIEW text (TDD 2.26) —
//! display-free, so the shortening rule is unit-tested without a live buffer
//! (`sdd/POLICY.md` § coverage gate: extract the decision core).
//!
//! Reuses [`super::body_plain_text`] rather than adding a second reduction of the same
//! body (POLICY "prefer extending an existing code path over adding a parallel one"),
//! so the preview and a find match inside the SAME collapsed body always agree about
//! what the reader would see — both skip raw HTML, both concatenate text/code runs,
//! neither ever shows a Markdown delimiter byte the reader would not.

/// Maximum CHARACTERS a preview carries before its trailing ellipsis.
///
/// Chosen so the preview reads as one short fragment on the summary line at the app's
/// default zoom — long enough that a reader recognises the body's topic, short enough
/// that it cannot itself wrap the summary line onto a second row (which would push the
/// toggle affordance off the line it belongs to), and short enough that the trailing
/// ellipsis reads as a genuine truncation rather than padding on an already-short line
/// (POLICY "no magic numbers").
pub(crate) const MAX_PREVIEW_CHARS: usize = 60;

/// The short, single-line preview a collapsed disclosure's summary shows for its
/// body's **opening** text (TDD 2.26) — or `None` when there is nothing worth showing.
///
/// `body_src` is the block's own body Markdown source — the range
/// [`super::DisclosureSpan::body`] names — reduced through [`super::body_plain_text`]
/// exactly as find reduces it (TDD 11.10), never the raw Markdown, which would show a
/// `*` in every emphasised word.
///
/// Every run of whitespace `body_plain_text` emits — including the `\n` it inserts
/// between blocks — collapses to a single space, because the destination is ONE buffer
/// line: a literal newline part-way through would split the summary line in two.
/// Leading and trailing whitespace is trimmed the same way, which is also what makes an
/// all-whitespace (or otherwise textless — a bare image, a raw-HTML-only body) body
/// report `None` rather than a bare ellipsis.
///
/// The result always ends in an ellipsis when it is `Some` — a preview names a fold
/// that still hides more, whether or not this particular fragment happened to fit
/// under [`MAX_PREVIEW_CHARS`] — matching TDD 2.26's "ending in an ellipsis".
///
/// Cuts on a `char` boundary, never a byte one, so a multi-byte or multi-codepoint
/// sequence straddling the cut is never split mid-character (see the unit tests below
/// for a body engineered to land the cut exactly there).
pub(crate) fn body_preview(body_src: &str) -> Option<String> {
    let plain = super::body_plain_text(body_src);
    let collapsed = collapse_whitespace(&plain);
    if collapsed.is_empty() {
        return None;
    }
    let mut preview: String = collapsed.chars().take(MAX_PREVIEW_CHARS).collect();
    preview.push('…');
    Some(preview)
}

/// [`body_preview`], resolved against the whole document string and a body's
/// (possibly absent) SOURCE range, formatted exactly as the caller writes it to the
/// buffer — the precise question `Renderer::emit_pending_summary` has to answer for a
/// collapsed block, pulled out here so it is proven without a live `GtkTextBuffer`
/// (`sdd/POLICY.md` § coverage gate: GTK buffer mutation cannot be unit-tested, but
/// deciding WHETHER and WHAT to write can, and should be extracted rather than left
/// inline where only a `#[gtktest::test]` can reach it).
///
/// `body_range` is [`super::DisclosureSpan::body`] — `None` for an unclosed block,
/// which this renders as `None` too (nothing to preview when there is no body).
/// Leading space included, so the caller can `self.insert(&preview_insert_text(...))`
/// directly onto the summary line right after its label with no punctuation of its
/// own to remember.
pub(crate) fn preview_insert_text(
    cleaned: &str,
    body_range: Option<std::ops::Range<usize>>,
) -> Option<String> {
    let preview = body_range
        .and_then(|body| cleaned.get(body))
        .and_then(body_preview)?;
    Some(format!(" {preview}"))
}

/// Collapse every run of whitespace (spaces, tabs, the `\n`s [`super::body_plain_text`]
/// inserts between runs) to a single space, and trim the ends — the reduction from
/// "several block-shaped runs of text" to "one line a summary can carry".
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    // Starts `true` so LEADING whitespace is trimmed for free — the first
    // non-whitespace character never sees a run to collapse against.
    let mut last_was_space = true;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
            }
            last_was_space = true;
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }
    // TRAILING whitespace left exactly one space in `out` (the run before the string
    // ended) rather than none — pop it rather than special-casing the loop's last
    // iteration.
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{body_preview, preview_insert_text, MAX_PREVIEW_CHARS};

    #[test]
    fn an_empty_body_has_no_preview() {
        assert_eq!(body_preview(""), None);
    }

    #[test]
    fn a_whitespace_only_body_has_no_preview() {
        assert_eq!(body_preview("   \n\n\t  \n"), None);
    }

    #[test]
    fn a_body_shorter_than_the_limit_still_ends_in_an_ellipsis() {
        let preview = body_preview("Short body.\n").expect("a non-empty body previews");
        assert_eq!(preview, "Short body.…");
    }

    #[test]
    fn a_long_body_is_cut_at_the_limit_and_ellipsised() {
        let body = "word ".repeat(40); // far past MAX_PREVIEW_CHARS
        let preview = body_preview(&body).expect("a non-empty body previews");
        assert!(preview.ends_with('…'));
        // One char for the ellipsis itself, on top of the cut text.
        assert_eq!(preview.chars().count(), MAX_PREVIEW_CHARS + 1);
    }

    #[test]
    fn a_multi_byte_cut_point_does_not_split_a_character() {
        // Every kept character is a 4-byte emoji, so the cut sits at a CHARACTER
        // boundary that a naive BYTE slice at the same offset could never legally
        // land on — this guards `body_preview` against panicking or emitting
        // invalid UTF-8 at the cut.
        let body = "😀".repeat(MAX_PREVIEW_CHARS + 5);
        let preview = body_preview(&body).expect("a non-empty body previews");
        assert!(preview.is_char_boundary(preview.len()));
        assert_eq!(
            preview.chars().count(),
            MAX_PREVIEW_CHARS + 1,
            "cut at the character limit, not the byte one: {preview:?}"
        );
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn multi_block_whitespace_collapses_to_one_line() {
        // `body_plain_text` separates block boundaries with `\n` (see its own doc
        // comment) — a preview must never carry that newline into the summary LINE.
        let preview = body_preview("First paragraph.\n\nSecond paragraph.\n")
            .expect("a non-empty body previews");
        assert!(
            !preview.contains('\n'),
            "the preview must be one line: {preview:?}"
        );
        assert!(preview.contains("First paragraph."), "{preview:?}");
        assert!(preview.contains("Second paragraph"), "{preview:?}");
    }

    #[test]
    fn an_unclosed_blocks_absent_range_previews_nothing() {
        // `None` is exactly what `DisclosureSpan::body` carries for a block the
        // document never closes — the renderer must never call `body_preview` on a
        // guess for it.
        assert_eq!(
            preview_insert_text("<details>\n<summary>S</summary>\n", None),
            None
        );
    }

    #[test]
    fn a_range_the_string_does_not_contain_previews_nothing_rather_than_panicking() {
        // `str::get` returns `None` for an out-of-bounds or char-boundary-violating
        // range instead of panicking — never reachable from a real render (the range
        // always comes from parsing the same string), but the seam's OWN contract
        // is worth pinning independently of that invariant holding elsewhere.
        assert_eq!(preview_insert_text("short", Some(10..20)), None);
    }

    #[test]
    fn a_real_body_range_previews_with_a_leading_space() {
        let cleaned = "before\n\n<details>\n<summary>S</summary>\n\nthe body\n\n</details>\n";
        let body_start = cleaned
            .find("the body")
            .expect("fixture contains its own body");
        let body_end = body_start + "the body\n".len();
        let preview =
            preview_insert_text(cleaned, Some(body_start..body_end)).expect("a real body previews");
        assert_eq!(preview, " the body…");
    }
}
