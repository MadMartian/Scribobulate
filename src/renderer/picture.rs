//! Pure scan of the image-relevant tags in a raw-HTML fragment (ScrAP-147).
//! No GTK; fully unit-tested.
//!
//! The renderer feeds raw HTML through this — a whole accumulated block at
//! `Tag::HtmlBlock` end, or a single inline tag at each `Event::InlineHtml` (see
//! `events.rs`/`end.rs`). It returns the ordered [`ImgTag`] stream — `<picture>`
//! open/close and each `<source>`/`<img>` candidate — which the renderer replays
//! against a `<picture>` grouping state it carries **across events**
//! (`Renderer::feed_html`). That cross-event state is essential: a *single-line*
//! `<picture><source><img></picture>` is NOT a CommonMark HTML block (the first tag
//! isn't followed by only whitespace), so pulldown-cmark emits its tags as
//! SEPARATE `Event::InlineHtml` events — grouping them inside one parse call would
//! lose the fallback across the event boundary (ScrAP-147).
//!
//! **The `<picture>` wrapper is what groups candidates into a fallback set.** Only
//! `<source>`/`<img>` *enclosed in a `<picture>…</picture>`* form one image (ordered
//! `<source srcset>`s, then the `<img src>` fallback — first decodable wins, exactly
//! as a browser resolves `<picture>`). Every `<img>` or `<source>` OUTSIDE a
//! `<picture>` renders independently — an ungrouped `<source>` must NOT suppress a
//! sibling `<img>`, since nothing links them (ScrAP-147). So a bare `<source>` + bare
//! `<img>` renders two images, while a `<picture>` wrapping the same two renders one.
//!
//! A fragment with no `<source>`/`<img>` yields no candidates, so the renderer drops
//! it — preserving sanitize-by-omission for all other raw HTML (`<script>`,
//! `<iframe>`, `<div>`, …). Attributes other than `srcset`/`src` are ignored: an
//! `onerror=` handler is never executed (there is no HTML/JS engine), and every
//! extracted `src` still passes through `links::resolve_image`, so this widens what
//! is *rendered*, never what is *trusted*.

/// One image-relevant tag from a raw-HTML fragment, in document order.
#[derive(Debug, PartialEq)]
pub(super) enum ImgTag {
    /// `<picture>` — opens a fallback group.
    PictureOpen,
    /// `</picture>` — closes the current fallback group.
    PictureClose,
    /// A `<source>`'s first `srcset` URL, or an `<img>`'s `src` — an image
    /// candidate. Inside a `<picture>` these are the group's fallback candidates;
    /// outside one, each is its own independent image.
    Candidate(String),
}

/// Scan a raw-HTML fragment for the ordered image tag stream (see the module docs).
/// The `<picture>` grouping itself is applied by the caller across events.
pub(super) fn scan_image_tags(html: &str) -> Vec<ImgTag> {
    let lower = html.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut tags = Vec::new();

    let mut i = 0usize;
    while let Some(rel) = lower[i..].find('<') {
        let start = i + rel;
        let Some(rel_end) = lower[start..].find('>') else {
            break;
        };
        let end = start + rel_end; // index of '>'
        let tag_lower = &lower[start..=end];
        let tag_orig = &html[start..=end];
        // Match a tag NAME on a boundary (next byte is space/tab, '>' or '/'), so
        // `<source>` ≠ `<sourcex>` and `<img>` ≠ `<image>` (SVG).
        let is = |name: &str| {
            tag_lower.starts_with(name)
                && bytes
                    .get(start + name.len())
                    .is_none_or(|&b| b == b' ' || b == b'>' || b == b'/' || b == b'\t')
        };
        if is("<picture") {
            tags.push(ImgTag::PictureOpen);
        } else if is("</picture") {
            tags.push(ImgTag::PictureClose);
        } else if is("<source") {
            if let Some(url) = attr(tag_orig, "srcset")
                .as_deref()
                .and_then(first_srcset_url)
            {
                tags.push(ImgTag::Candidate(url));
            }
        } else if is("<img") {
            if let Some(src) = attr(tag_orig, "src") {
                tags.push(ImgTag::Candidate(src));
            }
        }
        i = end + 1;
    }
    tags
}

/// The first URL of an HTML `srcset` value: `"a.webp 2x, b.webp 1x"` → `"a.webp"`.
/// A `<source>` carries at most one image here (candidate density descriptors are
/// irrelevant to a single decoded texture), so only the first entry's URL is kept.
fn first_srcset_url(srcset: &str) -> Option<String> {
    srcset
        .split(',')
        .next()?
        .split_whitespace()
        .next()
        .map(str::to_owned)
}

/// Extract attribute `name`'s value from a single tag's inner text (already sliced
/// to `<tagname …` without the closing `>`). `name` must be lowercase; the tag is
/// matched case-insensitively. Returns the unquoted value, or `None` if absent.
///
/// The match requires `name` to sit on an attribute boundary (preceded by ASCII
/// whitespace or the tag's `<`) and be immediately followed — after optional
/// whitespace — by `=`. That `=` test is what keeps `src` from matching the `src`
/// prefix of `srcset` (`srcset` is followed by `set`, not `=`).
fn attr(tag: &str, name: &str) -> Option<String> {
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

#[cfg(test)]
mod picture_tests {
    use super::{scan_image_tags, ImgTag};

    fn cand(s: &str) -> ImgTag {
        ImgTag::Candidate(s.to_string())
    }

    #[test]
    fn picture_emits_open_candidates_close_in_order() {
        // The GitHub-style hero: PictureOpen, the WebP source + GIF img candidates,
        // PictureClose — the renderer groups these into one fallback image.
        let html = "<picture>\n\
             <source srcset=\"splash.webp\" type=\"image/webp\">\n\
             <img src=\"splash.gif\" alt=\"Introducing Scribobulate!\">\n\
             </picture>";
        assert_eq!(
            scan_image_tags(html),
            vec![
                ImgTag::PictureOpen,
                cand("splash.webp"),
                cand("splash.gif"),
                ImgTag::PictureClose
            ]
        );
    }

    #[test]
    fn multiple_sources_keep_document_order_before_img() {
        let html = "<picture>\
             <source srcset='a.avif' type='image/avif'>\
             <source srcset='b.webp' type='image/webp'>\
             <img src='c.png'></picture>";
        assert_eq!(
            scan_image_tags(html),
            vec![
                ImgTag::PictureOpen,
                cand("a.avif"),
                cand("b.webp"),
                cand("c.png"),
                ImgTag::PictureClose
            ]
        );
    }

    #[test]
    fn bare_img_without_picture_is_one_candidate() {
        assert_eq!(
            scan_image_tags("<img src=\"logo.png\">"),
            vec![cand("logo.png")]
        );
        // Unquoted value terminated by whitespace before further attributes.
        assert_eq!(
            scan_image_tags("<img src=logo.png width=100>"),
            vec![cand("logo.png")]
        );
    }

    #[test]
    fn ungrouped_source_and_img_are_bare_candidates_no_picture_markers() {
        // The bug this file exists to prevent (ScrAP-147): with NO enclosing
        // `<picture>`, a `<source>` and an `<img>` are just two bare candidates —
        // no PictureOpen/Close — so the renderer renders each as its own image and a
        // `<source>` never suppresses a sibling `<img>`.
        assert_eq!(
            scan_image_tags("<source srcset=\"a.webp\">\n<img src=\"b.gif\">"),
            vec![cand("a.webp"), cand("b.gif")]
        );
        assert_eq!(
            scan_image_tags("<img src=\"a.png\"><img src=\"b.png\">"),
            vec![cand("a.png"), cand("b.png")]
        );
    }

    #[test]
    fn a_picture_followed_by_a_bare_img_delimits_with_close() {
        let html = "<picture><source srcset=\"a.webp\"><img src=\"b.gif\"></picture>\
                    <img src=\"c.png\">";
        assert_eq!(
            scan_image_tags(html),
            vec![
                ImgTag::PictureOpen,
                cand("a.webp"),
                cand("b.gif"),
                ImgTag::PictureClose,
                cand("c.png"),
            ]
        );
    }

    #[test]
    fn srcset_first_url_only_dropping_density_descriptor() {
        assert_eq!(
            scan_image_tags("<source srcset=\"hero@2x.webp 2x, hero.webp 1x\">"),
            vec![cand("hero@2x.webp")]
        );
    }

    #[test]
    fn non_image_html_yields_no_tags() {
        // Sanitize-by-omission stays intact for every non-image element.
        assert!(scan_image_tags("<script>alert('x')</script>").is_empty());
        assert!(scan_image_tags("<iframe src=\"file:///etc/passwd\"></iframe>").is_empty());
        assert!(scan_image_tags("<div class=\"src\">text</div>").is_empty());
        assert!(scan_image_tags("plain text, no tags").is_empty());
    }

    #[test]
    fn onerror_and_other_attributes_are_ignored_only_src_is_read() {
        // The `onerror=` handler is never returned — only the `src` is extracted
        // (and the caller still gates it through resolve_image). `srcset`'s `src`
        // prefix must not be mistaken for a `src` attribute.
        assert_eq!(
            scan_image_tags("<img src=\"does-not-exist.png\" onerror=\"steal()\">"),
            vec![cand("does-not-exist.png")]
        );
        assert_eq!(
            scan_image_tags("<source srcset=\"only.webp\">"),
            vec![cand("only.webp")]
        );
    }

    #[test]
    fn case_insensitive_tags_and_attributes() {
        let html = "<PICTURE><SOURCE SRCSET=\"X.WEBP\"><IMG SRC=\"Y.GIF\"></PICTURE>";
        assert_eq!(
            scan_image_tags(html),
            vec![
                ImgTag::PictureOpen,
                cand("X.WEBP"),
                cand("Y.GIF"),
                ImgTag::PictureClose
            ]
        );
    }

    #[test]
    fn img_tag_is_not_confused_with_svg_image_element() {
        // `<image>` (SVG) is not `<img>`; it yields no candidate.
        assert!(scan_image_tags("<image href=\"x.png\"/>").is_empty());
    }
}
