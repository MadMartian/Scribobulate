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

mod lex;
mod tags;

pub(crate) use lex::{RawHtml, RawItem, TagKind};
pub(crate) use tags::{attr, has_attr};

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
