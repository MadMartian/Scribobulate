//! The raw-HTML **allowlist** — the single owner of which HTML elements this
//! application renders rather than drops (ScrAP-147).
//!
//! Scribobulate sanitizes raw HTML **by omission**: an element absent from the set
//! below is neither executed nor shown as literal text, it simply does not exist as
//! far as the renderer is concerned. That makes this set the whole of what stands
//! between an untrusted document's markup and the surfaces that display it.
//!
//! **Why the set lives here rather than inside a scanner.** It is consumed by more
//! than one scanner (`picture`, `disclosure`) and by more than one *sink* — the
//! preview renderer and the display-free export pipeline both walk it, and they must
//! agree **exactly**. A permitted set that exists only as branches inside one
//! scanner can be reproduced elsewhere only by copying it, and a copy is how two
//! consumers silently drift apart. It was extracted from `picture.rs` when the
//! disclosure work made it span two features; before that it had one consumer and
//! living beside it was correct.
//!
//! **Adding a variant widens what is RENDERED. It must never widen what is
//! TRUSTED.** Only the attributes a scanner explicitly reads have any effect — no
//! other attribute does anything (an `onerror=` is inert; there is no HTML/JS
//! engine) — and every URL-bearing value still passes `links::resolve_image`'s
//! containment gate. An element that would need a *new* kind of trust (a URL in a
//! new position, a scripting surface, a style hook) is not a candidate for this list
//! without a security-posture decision, which is POLICY's call and not a rendering
//! one.

mod tags;

pub(crate) use tags::{attr, has_attr, tag_end};

/// One raw-HTML element this application renders rather than drops.
///
/// **This enum IS the allowlist**, and it is the whole of it — named data with one
/// owner rather than control flow, so a second consumer reproduces the set by
/// *reading* it rather than by restating it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RawHtmlElement {
    /// `<picture>` — opens a fallback group.
    PictureOpen,
    /// `</picture>` — closes the current fallback group.
    PictureClose,
    /// `<source>` — a candidate carrying `srcset`.
    Source,
    /// `<img>` — a candidate carrying `src`.
    Img,
    /// `<details>` — opens a collapsible disclosure block.
    DetailsOpen,
    /// `</details>` — closes the current disclosure block.
    DetailsClose,
    /// `<summary>` — opens the disclosure's summary line.
    SummaryOpen,
    /// `</summary>` — closes the disclosure's summary line.
    SummaryClose,
}

impl RawHtmlElement {
    /// The lowercase tag text this element is matched by, **including** the opening
    /// `<` (and the `/` of a close tag). The prefix travels with the element because
    /// the boundary rule below is stated in terms of it.
    const fn tag_prefix(self) -> &'static str {
        match self {
            Self::PictureOpen => "<picture",
            Self::PictureClose => "</picture",
            Self::Source => "<source",
            Self::Img => "<img",
            Self::DetailsOpen => "<details",
            Self::DetailsClose => "</details",
            Self::SummaryOpen => "<summary",
            Self::SummaryClose => "</summary",
        }
    }
}

/// Every raw-HTML element the renderer recognises, in match order. Anything absent
/// from this array is dropped wholesale — `<script>`, `<iframe>`, `<div>` and the
/// rest — neither executed nor shown as literal text.
///
/// **Close tags precede their open tags** for the `</picture>`/`<picture>` pair and
/// their siblings, because the boundary rule below matches on a prefix and `<picture`
/// is a prefix of nothing, while an unordered scan that tested `<details` before
/// `</details` would still be correct only by accident of the leading `/`. Keeping
/// the close-before-open order makes that independent of the rule's details.
pub(crate) const RENDERED_HTML_ELEMENTS: [RawHtmlElement; 8] = [
    RawHtmlElement::PictureOpen,
    RawHtmlElement::PictureClose,
    RawHtmlElement::Source,
    RawHtmlElement::Img,
    RawHtmlElement::DetailsOpen,
    RawHtmlElement::DetailsClose,
    RawHtmlElement::SummaryOpen,
    RawHtmlElement::SummaryClose,
];

/// Which permitted element, if any, the tag beginning at `tag_lower` is.
///
/// **The tag-name-boundary rule lives here, with the set**, because it is part of the
/// set's meaning rather than an implementation detail of one scanner: a prefix match
/// alone would admit `<sourcex>` as a `<source>` and SVG's `<image>` as an `<img>`,
/// silently widening the allowlist past what it says. The byte after the name must end
/// it — whitespace, `>`, or the `/` of a self-closing tag.
///
/// `tag_lower` is one whole tag, already lowercased, from its `<` through its `>`.
pub(crate) fn recognise_html_element(tag_lower: &str) -> Option<RawHtmlElement> {
    RENDERED_HTML_ELEMENTS.into_iter().find(|el| {
        let name = el.tag_prefix();
        tag_lower.starts_with(name)
            && tag_lower
                .as_bytes()
                .get(name.len())
                .is_none_or(|&b| b.is_ascii_whitespace() || b == b'>' || b == b'/')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_every_permitted_element() {
        // Every element in the set must be recognisable from its own canonical tag —
        // otherwise a variant can be added to the array and never match anything,
        // which looks exactly like a correctly-installed allowlist entry.
        for el in RENDERED_HTML_ELEMENTS {
            let tag = format!("{}>", el.tag_prefix());
            assert_eq!(
                recognise_html_element(&tag),
                Some(el),
                "allowlist entry {el:?} does not recognise its own tag {tag:?}"
            );
        }
    }

    #[test]
    fn tag_name_boundary_is_enforced() {
        // The whole point of the boundary rule: a prefix match alone would silently
        // widen the allowlist to elements nobody approved.
        assert_eq!(recognise_html_element("<sourcex>"), None);
        assert_eq!(recognise_html_element("<image>"), None);
        assert_eq!(recognise_html_element("<detailsx>"), None);
        assert_eq!(recognise_html_element("<summaryfoo>"), None);
    }

    #[test]
    fn boundary_bytes_all_terminate_a_name() {
        assert_eq!(
            recognise_html_element("<details>"),
            Some(RawHtmlElement::DetailsOpen)
        );
        assert_eq!(
            recognise_html_element("<details open>"),
            Some(RawHtmlElement::DetailsOpen)
        );
        assert_eq!(
            recognise_html_element("<details\topen>"),
            Some(RawHtmlElement::DetailsOpen)
        );
        // F-AP-022: a tag wrapped across lines is ordinary in a hand-written document,
        // and the boundary set omitted every vertical whitespace byte.
        for ws in ["\n", "\r", "\u{c}"] {
            assert_eq!(
                recognise_html_element(&format!("<details{ws}open>")),
                Some(RawHtmlElement::DetailsOpen),
                "{ws:?} ends a tag name"
            );
        }
        assert_eq!(
            recognise_html_element("<summary/>"),
            Some(RawHtmlElement::SummaryOpen)
        );
    }

    #[test]
    fn close_tags_are_not_confused_with_open_tags() {
        // `</details` and `<details` differ only by the slash; a scan that lost it
        // would close a block on its own opening tag.
        assert_eq!(
            recognise_html_element("</details>"),
            Some(RawHtmlElement::DetailsClose)
        );
        assert_eq!(
            recognise_html_element("</summary>"),
            Some(RawHtmlElement::SummaryClose)
        );
        assert_eq!(
            recognise_html_element("</picture>"),
            Some(RawHtmlElement::PictureClose)
        );
    }

    #[test]
    fn unpermitted_elements_are_dropped() {
        // Sanitize-by-omission: the set is the whole of the permission.
        for tag in ["<script>", "<iframe>", "<div>", "<style>", "<object>"] {
            assert_eq!(
                recognise_html_element(tag),
                None,
                "{tag} must not be permitted"
            );
        }
    }
}

/// The literal-text runs a raw-HTML block contributes to the rendered page.
///
/// **Raw HTML is sanitised by omission** (TDD 2.23): a block's tags are matched
/// against [`RENDERED_HTML_ELEMENTS`] and everything else is dropped, text included.
/// That rule is correct for `<script>` and wrong for one case — a `<details>` whose
/// body is not separated by the blank lines CommonMark requires. With no blank lines
/// the whole construct is ONE raw-HTML block, so the body never becomes Markdown
/// events and vanished entirely, where rubric 2.26d promises literal text.
///
/// **The allowlist still governs which elements may contribute text at all.** A run is
/// emitted only when it sits at the top level of the block or inside an allowlisted
/// element — never inside a `<script>`, `<style>`, `<iframe>` or any other unrecognised
/// element, whose text stays dropped exactly as before. Without that nesting rule this
/// would stop being a narrow widening of the sanitisation posture and become a general
/// one, because the unspaced case puts the script INSIDE the block being shown
/// (rubric 2.26d's `<script>` clause).
///
/// Whitespace-only runs are skipped, so a well-formed `<picture>` group — whose tags
/// are separated by newlines and nothing else — contributes nothing, and its rendering
/// is byte-identical to what it was before this existed.
///
/// **Suppression is tracked by element NAME, not by a depth count.** A counter is
/// defeated by any token shaped `</…>`, including one that closes nothing: two stray
/// `</x>` before a real `</script>` drop the count to zero while the cursor is still
/// inside the script, and every run after that is emitted as page content. So the name
/// is pushed and popped only against a match, which also makes an unmatched close tag
/// inert in *both* directions — the same answer a browser gives.
pub(crate) fn literal_text_runs(html: &str) -> Vec<LiteralRun<'_>> {
    let mut runs = Vec::new();
    // The UNRECOGNISED elements enclosing the cursor, innermost last. Text is dropped
    // while this is non-empty; an allowlisted element never enters it, so `<summary>`'s
    // own text is reached at depth 0 exactly as the top level is.
    let mut suppressors: Vec<String> = Vec::new();
    // `<summary>`'s text is NOT a literal-text run: the disclosure scanner already
    // extracts it and the renderer draws it as the block's label. Emitting it here too
    // would print the label twice — once as the summary line, once as body text.
    let mut in_summary = false;
    let mut cursor = 0usize;
    while let Some(rel) = html[cursor..].find('<') {
        let lt = cursor + rel;
        push_run(
            &mut runs,
            cursor,
            &html[cursor..lt],
            suppressors.len() + usize::from(in_summary),
        );
        // An unterminated `<` ends the block: the remainder is a partial tag, never
        // text, and treating it as text would print a half-typed tag at every keystroke
        // of a live-preview session.
        let Some(gt) = tag_end(html, lt) else {
            return runs;
        };
        let tag = &html[lt..=gt];
        let lower = tag.to_ascii_lowercase();
        cursor = gt + 1;
        match recognise_html_element(&lower) {
            Some(RawHtmlElement::SummaryOpen) => in_summary = true,
            Some(RawHtmlElement::SummaryClose) => in_summary = false,
            Some(_) => {}
            None => {
                let (close, name) = tags::tag_name(&lower);
                if close {
                    // A close tag that matches nothing on the stack closes nothing.
                    if let Some(at) = suppressors.iter().rposition(|n| n == name) {
                        suppressors.truncate(at);
                    }
                } else if lower.ends_with("/>") || is_void_name(name) {
                    // A void or self-closing unknown element encloses nothing, so it
                    // must not open a suppression that never closes — one `<br>` would
                    // otherwise silence the whole remainder of the block.
                } else if is_raw_text_name(name) {
                    // A raw-text element's content is not markup at all: nothing inside
                    // it can open or close a tag, so no `</b>` in a script's source can
                    // end the suppression. Skip to its own close tag, as a browser's
                    // tokenizer does; with none, the rest of the block IS its content.
                    let Some(after) = skip_raw_text(html, cursor, name) else {
                        return runs;
                    };
                    cursor = after;
                } else {
                    suppressors.push(name.to_owned());
                }
            }
        }
    }
    push_run(
        &mut runs,
        cursor,
        &html[cursor..],
        suppressors.len() + usize::from(in_summary),
    );
    runs
}

/// One literal-text run and where it sits in the fragment.
///
/// **The offset is what lets a consumer interleave runs with TAGS in document order.**
/// `disclosure::scan_disclosure_tags` does exactly that, so a run between `</summary>`
/// and `</details>` lands inside the block's body rather than after the whole
/// construct — which is the difference between a collapsed disclosure hiding its body
/// and printing it beneath a toggle that then does nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LiteralRun<'a> {
    /// Byte offset of the run within the fragment passed to [`literal_text_runs`].
    pub(crate) at: usize,
    /// The run's text, untrimmed.
    pub(crate) text: &'a str,
}

fn push_run<'a>(runs: &mut Vec<LiteralRun<'a>>, at: usize, text: &'a str, suppressed: usize) {
    if suppressed == 0 && !text.trim().is_empty() {
        runs.push(LiteralRun { at, text });
    }
}

/// The offset just past `</name …>`, searching from `from`. `None` when the element is
/// never closed, which means everything left is its content.
fn skip_raw_text(html: &str, from: usize, name: &str) -> Option<usize> {
    let lower = html.to_ascii_lowercase();
    let needle = format!("</{name}");
    let mut at = from;
    loop {
        let lt = at + lower[at..].find(&needle)?;
        // The name must END at the close tag's own boundary, or `</scriptx>` would do.
        let after = lt + needle.len();
        if lower[after..]
            .chars()
            .next()
            .is_none_or(|c| c.is_ascii_whitespace() || c == '>' || c == '/')
        {
            return Some(tag_end(html, lt).map_or(html.len(), |gt| gt + 1));
        }
        at = after;
    }
}

/// HTML elements whose content is text rather than markup. Inside one, a `<` opens
/// nothing — which is exactly why a `</…>` inside one must not be allowed to close the
/// suppression it sits in.
fn is_raw_text_name(name: &str) -> bool {
    matches!(name, "script" | "style" | "textarea" | "title" | "xmp")
}

/// HTML elements that never have a closing tag, so an opening one encloses nothing.
///
/// Only the ones a document plausibly carries inside a disclosure — the list does not
/// need to be exhaustive to be safe, because an unlisted void element merely suppresses
/// text that would otherwise show, which is the conservative direction.
fn is_void_name(name: &str) -> bool {
    matches!(
        name,
        "br" | "hr" | "wbr" | "meta" | "link" | "input" | "area"
    )
}

#[cfg(test)]
mod literal_text_tests {
    use super::literal_text_runs;

    /// Every run's text, in document order — what most of these assertions are about.
    fn texts(html: &str) -> Vec<&str> {
        literal_text_runs(html)
            .into_iter()
            .map(|run| run.text)
            .collect()
    }

    fn joined(html: &str) -> String {
        texts(html).concat()
    }

    #[test]
    fn an_unspaced_disclosure_body_becomes_literal_text() {
        let joined = joined("<details>\n<summary>S</summary>\nnot separated\n</details>");
        assert!(
            joined.contains("not separated"),
            "the body shows as literal text: {joined:?}"
        );
        assert!(
            !joined.contains('S'),
            "but the SUMMARY's text does not — the renderer already draws it as the \
             block's label, so emitting it here would print the label twice: {joined:?}"
        );
    }

    /// **Rubric 2.26d's security clause.** The unspaced case puts the script INSIDE the
    /// block being shown, so this is the assertion that keeps the widening narrow.
    #[test]
    fn a_script_inside_an_allowlisted_block_contributes_no_text() {
        let joined = joined(
            "<details>\n<summary>S</summary>\nvisible\n<script>alert('x')</script>\n</details>",
        );
        assert!(joined.contains("visible"), "the body's own text shows");
        assert!(
            !joined.contains("alert"),
            "the script's text does not: {joined:?}"
        );
    }

    #[test]
    fn nested_unknown_elements_stay_suppressed_until_all_close() {
        let runs = joined("<details>a<div>b<span>c</span>d</div>e</details>");
        assert!(runs.contains('a') && runs.contains('e'));
        for hidden in ['b', 'c', 'd'] {
            assert!(
                !runs.contains(hidden),
                "{hidden} is inside a dropped element"
            );
        }
    }

    #[test]
    fn a_void_element_does_not_suppress_the_rest_of_the_block() {
        let runs = joined("<details>before<br>after</details>");
        assert!(
            runs.contains("before") && runs.contains("after"),
            "a void element encloses nothing: {runs:?}"
        );
    }

    #[test]
    fn a_well_formed_picture_group_contributes_nothing() {
        let runs = joined(
            "<picture>\n<source srcset=\"a.webp\">\n<img src=\"a.png\" alt=\"x\">\n</picture>",
        );
        assert!(
            runs.is_empty(),
            "only whitespace between its tags: {runs:?}"
        );
    }

    #[test]
    fn an_unterminated_tag_is_not_printed_as_text() {
        let runs = joined("<details>shown<scr");
        assert!(runs.contains("shown"));
        assert!(
            !runs.contains("scr"),
            "a half-typed tag is not text: {runs:?}"
        );
    }

    #[test]
    fn a_script_at_the_top_level_of_a_block_is_still_dropped() {
        let runs = texts("<script>alert('x')</script>");
        assert!(runs.is_empty(), "unchanged by this widening: {runs:?}");
    }

    /// F-001, both reproductions. A depth COUNTER let any token shaped `</…>` decrement
    /// it, so a stray close tag inside a `<script>` released the suppression and the
    /// script's own text was emitted as page content.
    #[test]
    fn a_stray_close_tag_inside_a_script_does_not_release_it() {
        let runs = texts("<details>x<script>y</span>LEAK</script>z</details>");
        assert_eq!(runs, vec!["x", "z"], "the script's text stays dropped");

        let runs = joined(
            "<details>\n<summary>S</summary>\n<script>\n</b>\nalert(1)\n</script>\n</details>",
        );
        assert!(
            !runs.contains("alert"),
            "nor when the stray tag is on its own line: {runs:?}"
        );
    }

    /// The counter's other half: at depth 0 a stray close tag was free, and could take
    /// the depth *negative* were it not for `saturating_sub` — so the element opened
    /// next was one short of suppressing anything.
    #[test]
    fn a_close_tag_that_matches_nothing_is_inert() {
        let runs = joined("<details>a</x><div>b</div>c</details>");
        assert!(runs.contains('a') && runs.contains('c'));
        assert!(
            !runs.contains('b'),
            "the `<div>` still suppresses: {runs:?}"
        );
    }

    #[test]
    fn a_mismatched_close_tag_does_not_pop_an_inner_element() {
        let runs = joined("<details>a<div>b</span>c</div>d</details>");
        assert!(runs.contains('a') && runs.contains('d'));
        for hidden in ['b', 'c'] {
            assert!(!runs.contains(hidden), "still inside the div: {runs:?}");
        }
    }

    /// F-010, in this scanner: a `>` inside a quoted attribute value used to split the
    /// tag, leaving its own tail to be re-scanned as text.
    #[test]
    fn a_bracket_in_a_quoted_attribute_does_not_leak_the_tags_tail() {
        let runs = joined("<details>a<div title=\"x>y\">b</div>c</details>");
        assert!(runs.contains('a') && runs.contains('c'));
        assert!(
            !runs.contains('b') && !runs.contains("y\""),
            "neither the div's text nor the tag's own tail: {runs:?}"
        );
    }

    #[test]
    fn an_unclosed_raw_text_element_swallows_the_rest_of_the_block() {
        let runs = joined("<details>shown<style>body{}");
        assert!(runs.contains("shown"));
        assert!(!runs.contains("body"), "it is style source: {runs:?}");
    }
}
