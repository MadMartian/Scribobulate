//! Tier-2 insertion markup — GTK-free string builders and parsers. The window
//! layer collects the fields via a dialog (pre-filling the caption/alt/first-cell
//! from the selection), then splices the returned string in as one undo step. The
//! parsers ([`parse_link`], [`parse_image`]) detect when the selection is already
//! exactly one link/image so the dialog EDITs it rather than re-wrapping.

/// `[caption](url)` inline-link markup.
pub(crate) fn link_markup(caption: &str, url: &str) -> String {
    format!("[{caption}]({url})")
}

/// Image markup: `![alt](url)`, or `![alt](url "title")` when a title is given.
pub(crate) fn image_markup(alt: &str, url: &str, title: &str) -> String {
    if title.is_empty() {
        format!("![{alt}]({url})")
    } else {
        format!("![{alt}]({url} \"{title}\")")
    }
}

/// A GFM table skeleton: a header row (`first_cell` in column 1, the rest blank),
/// the `---` separator, and `rows` empty body rows — all `cols` wide. `cols`/`rows`
/// are clamped to at least 1. Ends with a trailing newline so it reads as its own
/// block; the caller is responsible for a leading newline when not at a line start.
pub(crate) fn table_markup(cols: usize, rows: usize, first_cell: &str) -> String {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let mut out = String::new();
    out.push('|');
    for c in 0..cols {
        let cell = if c == 0 { first_cell } else { "" };
        out.push_str(&format!(" {cell} |"));
    }
    out.push('\n');
    out.push('|');
    for _ in 0..cols {
        out.push_str(" --- |");
    }
    out.push('\n');
    for _ in 0..rows {
        out.push('|');
        for _ in 0..cols {
            out.push_str("  |");
        }
        out.push('\n');
    }
    out
}

/// If `s` is **exactly** one inline link `[caption](url)` (trimmed, nothing else),
/// return `(caption, url)`. Image markup (`![…]`) or any surrounding text returns
/// `None` — so the Insert Link command edits an existing link but treats a plain or
/// mismatched selection as a new caption.
pub(crate) fn parse_link(s: &str) -> Option<(String, String)> {
    let inner = s.trim().strip_prefix('[')?.strip_suffix(')')?;
    let (caption, url) = inner.split_once("](")?;
    if caption.contains('[') || caption.contains(']') {
        return None;
    }
    Some((caption.to_string(), url.to_string()))
}

/// If `s` is **exactly** one image `![alt](url)` or `![alt](url "title")` (trimmed),
/// return `(alt, url, title)`. Link markup or surrounding text returns `None`.
pub(crate) fn parse_image(s: &str) -> Option<(String, String, String)> {
    let inner = s.trim().strip_prefix("![")?.strip_suffix(')')?;
    let (alt, rest) = inner.split_once("](")?;
    if alt.contains('[') || alt.contains(']') {
        return None;
    }
    let (url, title) = match rest.split_once(" \"") {
        Some((u, t)) => (u, t.strip_suffix('"')?),
        None => (rest, ""),
    };
    Some((alt.to_string(), url.to_string(), title.to_string()))
}

#[cfg(test)]
mod tests {
    use crate::format::*;

    #[test]
    fn link_markup_wraps_caption_and_url() {
        assert_eq!(
            link_markup("text", "https://x.com"),
            "[text](https://x.com)"
        );
        assert_eq!(link_markup("", ""), "[]()");
    }

    #[test]
    fn image_markup_includes_optional_title() {
        assert_eq!(image_markup("alt", "img.png", ""), "![alt](img.png)");
        assert_eq!(
            image_markup("alt", "img.png", "A cat"),
            "![alt](img.png \"A cat\")"
        );
    }

    #[test]
    fn table_markup_builds_a_gfm_skeleton() {
        assert_eq!(
            table_markup(2, 1, "H1"),
            "| H1 |  |\n| --- | --- |\n|  |  |\n"
        );
        // cols/rows clamp to at least 1.
        assert_eq!(table_markup(0, 0, ""), "|  |\n| --- |\n|  |\n");
    }

    #[test]
    fn parse_link_detects_exactly_one_link() {
        assert_eq!(
            parse_link("[text](http://x)"),
            Some(("text".into(), "http://x".into()))
        );
        assert_eq!(parse_link("  [a](b)  "), Some(("a".into(), "b".into())));
        // Image markup is not a link; surrounding text / plain text are not either.
        assert_eq!(parse_link("![alt](img.png)"), None);
        assert_eq!(parse_link("see [a](b)"), None);
        assert_eq!(parse_link("plain"), None);
    }

    #[test]
    fn parse_image_detects_alt_url_title() {
        assert_eq!(
            parse_image("![alt](img.png)"),
            Some(("alt".into(), "img.png".into(), String::new()))
        );
        assert_eq!(
            parse_image("![a](u \"t\")"),
            Some(("a".into(), "u".into(), "t".into()))
        );
        // Link markup is not an image; surrounding text is not either.
        assert_eq!(parse_image("[text](url)"), None);
        assert_eq!(parse_image("x ![a](b)"), None);
    }
}
