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
                .is_none_or(|&b| b == b' ' || b == b'>' || b == b'/' || b == b'\t')
    })
}

// ── tag attribute reading ─────────────────────────────────────────────────────

/// Extract attribute `name`'s value from a single tag's inner text (already sliced
/// to `<tagname …` without the closing `>`). `name` must be lowercase; the tag is
/// matched case-insensitively. Returns the unquoted value, or `None` if absent.
///
/// The match requires `name` to sit on an attribute boundary (preceded by ASCII
/// whitespace or the tag's `<`) and be immediately followed — after optional
/// whitespace — by `=`. That `=` test is what keeps `src` from matching the `src`
/// prefix of `srcset` (`srcset` is followed by `set`, not `=`).
pub(crate) fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let bytes = tag.as_bytes();
    let mut from = 0usize;
    loop {
        let rel = lower[from..].find(name)?;
        let idx = from + rel;
        let boundary = idx == 0 || bytes[idx - 1].is_ascii_whitespace() || bytes[idx - 1] == b'<';
        let after = idx + name.len();
        let rest = tag[after..].trim_start();
        if boundary && rest.starts_with('=') {
            return Some(attr_value(rest[1..].trim_start()));
        }
        from = after;
    }
}

/// Read an attribute value starting just after `=` (leading whitespace trimmed):
/// a double- or single-quoted run up to the closing quote, or an unquoted token up
/// to the next whitespace or `>`.
fn attr_value(after_eq: &str) -> String {
    let mut chars = after_eq.chars();
    match chars.next() {
        Some(q @ ('"' | '\'')) => after_eq[1..].split(q).next().unwrap_or_default().to_owned(),
        _ => after_eq
            .split(|c: char| c.is_ascii_whitespace() || c == '>')
            .next()
            .unwrap_or_default()
            .to_owned(),
    }
}

/// Is boolean attribute `name` PRESENT on this tag? `<details open>` carries no value,
/// so [`attr`] — which requires an `=` — cannot answer it and would report the
/// attribute absent.
///
/// Shares [`attr`]'s boundary rule deliberately: both live here so the definition of
/// "this is an attribute rather than a substring of one" has one home. A second copy
/// inside a scanner is how `src` starts matching the `src` of `srcset` again.
///
/// HTML's rule is presence, not value — `open`, `open=""` and `open="false"` are all
/// TRUE. That last one reads wrong and is correct: the attribute's presence is the
/// whole signal, which is why a document cannot express "explicitly closed" and does
/// not need to.
pub(crate) fn has_attr(tag: &str, name: &str) -> bool {
    let lower = tag.to_ascii_lowercase();
    let bytes = tag.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find(name) {
        let idx = from + rel;
        let boundary = idx == 0 || bytes[idx - 1].is_ascii_whitespace() || bytes[idx - 1] == b'<';
        let after = idx + name.len();
        // The name must END here too, or `open` would match inside `opened`.
        let ends = tag[after..]
            .chars()
            .next()
            .is_none_or(|c| c.is_ascii_whitespace() || c == '=' || c == '>' || c == '/');
        if boundary && ends {
            return true;
        }
        from = after;
    }
    false
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
pub(crate) fn literal_text_runs(html: &str) -> Vec<&str> {
    let mut runs = Vec::new();
    // How many UNRECOGNISED elements enclose the cursor. Text is dropped while this is
    // non-zero; an allowlisted element never changes it, so `<summary>`'s own text is
    // reached at depth 0 exactly as the top level is.
    let mut suppressed = 0usize;
    // `<summary>`'s text is NOT a literal-text run: the disclosure scanner already
    // extracts it and the renderer draws it as the block's label. Emitting it here too
    // would print the label twice — once as the summary line, once as body text.
    let mut in_summary = false;
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        let (text, tail) = rest.split_at(lt);
        push_run(&mut runs, text, suppressed + usize::from(in_summary));
        // An unterminated `<` ends the block: the remainder is a partial tag, never
        // text, and treating it as text would print a half-typed tag at every keystroke
        // of a live-preview session.
        let Some(gt) = tail.find('>') else {
            return runs;
        };
        let tag = &tail[..=gt];
        match recognise_html_element(&tag.to_ascii_lowercase()) {
            Some(RawHtmlElement::SummaryOpen) => in_summary = true,
            Some(RawHtmlElement::SummaryClose) => in_summary = false,
            _ => {}
        }
        if recognise_html_element(&tag.to_ascii_lowercase()).is_none() {
            if tag.starts_with("</") {
                suppressed = suppressed.saturating_sub(1);
            } else if !tag.ends_with("/>") {
                // A void or self-closing unknown element encloses nothing, so it must
                // not open a suppression that never closes — one `<br>` would otherwise
                // silence the whole remainder of the block.
                suppressed += usize::from(!is_void_element(tag));
            }
        }
        rest = &tail[gt + 1..];
    }
    push_run(&mut runs, rest, suppressed + usize::from(in_summary));
    runs
}

fn push_run<'a>(runs: &mut Vec<&'a str>, text: &'a str, suppressed: usize) {
    if suppressed == 0 && !text.trim().is_empty() {
        runs.push(text);
    }
}

/// HTML elements that never have a closing tag, so an opening one encloses nothing.
///
/// Only the ones a document plausibly carries inside a disclosure — the list does not
/// need to be exhaustive to be safe, because an unlisted void element merely suppresses
/// text that would otherwise show, which is the conservative direction.
fn is_void_element(tag: &str) -> bool {
    const VOID: [&str; 7] = ["<br", "<hr", "<wbr", "<meta", "<link", "<input", "<area"];
    let lower = tag.to_ascii_lowercase();
    VOID.iter().any(|v| {
        lower.starts_with(v)
            && lower[v.len()..]
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || c == '>' || c == '/')
    })
}

#[cfg(test)]
mod literal_text_tests {
    use super::literal_text_runs;

    #[test]
    fn an_unspaced_disclosure_body_becomes_literal_text() {
        let runs = literal_text_runs("<details>\n<summary>S</summary>\nnot separated\n</details>");
        let joined = runs.concat();
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
        let runs = literal_text_runs(
            "<details>\n<summary>S</summary>\nvisible\n<script>alert('x')</script>\n</details>",
        );
        let joined = runs.concat();
        assert!(joined.contains("visible"), "the body's own text shows");
        assert!(
            !joined.contains("alert"),
            "the script's text does not: {joined:?}"
        );
    }

    #[test]
    fn nested_unknown_elements_stay_suppressed_until_all_close() {
        let runs = literal_text_runs("<details>a<div>b<span>c</span>d</div>e</details>").concat();
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
        let runs = literal_text_runs("<details>before<br>after</details>").concat();
        assert!(
            runs.contains("before") && runs.contains("after"),
            "a void element encloses nothing: {runs:?}"
        );
    }

    #[test]
    fn a_well_formed_picture_group_contributes_nothing() {
        let runs = literal_text_runs(
            "<picture>\n<source srcset=\"a.webp\">\n<img src=\"a.png\" alt=\"x\">\n</picture>",
        );
        assert!(
            runs.is_empty(),
            "only whitespace between its tags: {runs:?}"
        );
    }

    #[test]
    fn an_unterminated_tag_is_not_printed_as_text() {
        let runs = literal_text_runs("<details>shown<scr").concat();
        assert!(runs.contains("shown"));
        assert!(
            !runs.contains("scr"),
            "a half-typed tag is not text: {runs:?}"
        );
    }

    #[test]
    fn a_script_at_the_top_level_of_a_block_is_still_dropped() {
        let runs = literal_text_runs("<script>alert('x')</script>").concat();
        assert!(runs.is_empty(), "unchanged by this widening: {runs:?}");
    }
}
