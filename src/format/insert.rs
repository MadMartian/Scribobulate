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

/// The destination URL of the Markdown inline link (or image) whose construct
/// contains the caret at **char** offset `col` of the single line `line`, or
/// `None` when the caret is not inside one. Drives `win.copy-link-location`'s
/// enabled state AND its clipboard write, from the one function, so the command
/// can never be enabled for a link it would then fail to read.
///
/// **A line, not the document.** The caller passes the caret's own line (and the
/// caret's offset within it) rather than the whole buffer, so the scan costs one
/// line however large the document is — this runs on every caret move. The price
/// is that a link split across a newline (legal CommonMark, vanishingly rare in
/// practice) is not recognised; neither are reference links (`[text][ref]`) or
/// link reference definitions (`[ref]: url`), which carry no inline destination.
///
/// Recognised, because each appears in ordinary documents:
/// * `[caption](url)` and the image form `![alt](url)` — the `!` counts as part
///   of the construct, so a caret sitting on it still resolves.
/// * A title: `(url "A cat")` → `url` (the destination is the first run of
///   non-whitespace, per CommonMark).
/// * An angle-bracketed destination: `(<url with spaces>)` → `url with spaces`.
/// * **Balanced parentheses inside the destination** —
///   `(https://en.wikipedia.org/wiki/Ruby_(gem))` yields the whole URL, not a
///   truncation at the first `)`. A first-`)` scan is the obvious implementation
///   and silently corrupts exactly the links people most want to copy.
/// * A backslash escape (`\[`, `\]`, `\)`) neither opens nor closes a construct.
///
/// The caret is "inside" from the construct's first character through the
/// position just after its closing `)`, so a caret parked at either end resolves
/// — the same reach a user reads as "the caret is in this link".
///
/// An empty destination (`[x]()`) yields `None`: there is nothing to copy, and a
/// command that copies an empty string is worse than one that stays disabled.
pub(crate) fn link_target_at(line: &str, col: usize) -> Option<&str> {
    let scan = Scan::of(line);
    for open in 0..scan.chars.len() {
        if !scan.is(open, '[') {
            continue;
        }
        // `![alt](url)` — the `!` belongs to the construct the caret can sit in.
        let start = if open > 0 && scan.is(open - 1, '!') {
            open - 1
        } else {
            open
        };
        let Some(close) = scan.caption_end(open) else {
            continue;
        };
        if close + 1 >= scan.chars.len() || !scan.is(close + 1, '(') {
            continue;
        }
        let Some(rparen) = scan.dest_end(close + 2) else {
            continue;
        };
        if !(start..=rparen + 1).contains(&col) {
            continue;
        }
        let dest = &line[scan.chars[close + 2].0..scan.chars[rparen].0];
        return match destination(dest) {
            "" => None,
            url => Some(url),
        };
    }
    None
}

/// One line, indexed by char position, with each position's backslash-escaped-ness
/// resolved in a single forward pass.
///
/// The pass is the point: a per-position "count the backslashes behind me" test is
/// the natural way to write this and is **quadratic** on a line of backslashes —
/// and this scan runs on every caret move, over a document that is untrusted
/// content whose cost is part of the threat model (POLICY § Input limits). One
/// pass up front makes every later test O(1) and the whole scan linear.
struct Scan {
    chars: Vec<(usize, char)>,
    /// `escaped[i]` — char `i` is preceded by an odd number of backslashes, so it
    /// is a literal and neither opens nor closes anything.
    escaped: Vec<bool>,
}

impl Scan {
    fn of(line: &str) -> Self {
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let mut escaped = Vec::with_capacity(chars.len());
        let mut pending = false; // the previous char was an unescaped backslash
        for &(_, c) in &chars {
            escaped.push(pending);
            pending = c == '\\' && !pending;
        }
        Self { chars, escaped }
    }

    /// Is char `i` an unescaped `c`?
    fn is(&self, i: usize, c: char) -> bool {
        self.chars[i].1 == c && !self.escaped[i]
    }

    /// The index of the `]` closing the caption opened at `open`, or `None` when
    /// the caption holds an unescaped `[` (nested brackets — [`parse_link`]
    /// refuses those too) or never closes.
    fn caption_end(&self, open: usize) -> Option<usize> {
        for j in open + 1..self.chars.len() {
            if self.is(j, ']') {
                return Some(j);
            }
            if self.is(j, '[') {
                return None;
            }
        }
        None
    }

    /// The index of the `)` closing the destination that starts at `from`,
    /// counting nested parentheses so a URL containing a balanced pair survives
    /// intact.
    fn dest_end(&self, from: usize) -> Option<usize> {
        let mut depth = 1usize;
        for j in from..self.chars.len() {
            if self.is(j, '(') {
                depth += 1;
            } else if self.is(j, ')') {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
        }
        None
    }
}

/// The destination part of a link's `(…)` body: the angle-bracketed form's
/// contents, else the first run of non-whitespace (dropping any `"title"`).
fn destination(raw: &str) -> &str {
    let raw = raw.trim();
    match raw.strip_prefix('<') {
        Some(rest) => rest.split('>').next().unwrap_or(rest),
        None => raw.split_whitespace().next().unwrap_or(""),
    }
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

    /// The caret's reach: every offset from the construct's first character
    /// through the one just past its `)` resolves, and nothing outside does.
    /// This is the whole enabled-state contract of `win.copy-link-location`, so
    /// the boundaries are asserted rather than sampled — an off-by-one here is a
    /// command that greys out one character early.
    #[test]
    fn link_target_at_spans_the_whole_construct_and_no_further() {
        //            0123456789...
        let line = "see [a](u) here";
        for col in 0..4 {
            assert_eq!(link_target_at(line, col), None, "col {col} precedes `[`");
        }
        for col in 4..=10 {
            assert_eq!(link_target_at(line, col), Some("u"), "col {col} is inside");
        }
        for col in 11..=line.chars().count() {
            assert_eq!(link_target_at(line, col), None, "col {col} follows `)`");
        }
    }

    #[test]
    fn link_target_at_reads_images_titles_and_angle_brackets() {
        // The `!` is part of the construct, so a caret on it still resolves.
        assert_eq!(link_target_at("![alt](img.png)", 0), Some("img.png"));
        assert_eq!(link_target_at("![alt](img.png)", 8), Some("img.png"));
        // A title is not part of the destination.
        assert_eq!(
            link_target_at("![a](img.png \"A cat\")", 6),
            Some("img.png")
        );
        // Angle-bracketed destinations may contain spaces.
        assert_eq!(link_target_at("[a](<my file.md>)", 5), Some("my file.md"));
    }

    /// A destination containing a BALANCED parenthesis pair — the case a
    /// first-`)` scan truncates silently, and the one people hit first
    /// (Wikipedia-style URLs).
    #[test]
    fn link_target_at_keeps_balanced_parentheses_in_the_url() {
        let line = "[Ruby](https://en.wikipedia.org/wiki/Ruby_(gem))";
        assert_eq!(
            link_target_at(line, 3),
            Some("https://en.wikipedia.org/wiki/Ruby_(gem)")
        );
    }

    #[test]
    fn link_target_at_picks_the_link_the_caret_is_in() {
        //                   0        9
        let line = "[one](a) and [two](b)";
        assert_eq!(link_target_at(line, 2), Some("a"));
        assert_eq!(link_target_at(line, 15), Some("b"));
        // Between the two constructs, no link is under the caret.
        assert_eq!(link_target_at(line, 11), None);
    }

    #[test]
    fn link_target_at_declines_non_links() {
        // Plain text, a bare bracket pair, a reference link, a link reference
        // definition, and an unterminated construct all have no inline target.
        assert_eq!(link_target_at("plain text", 4), None);
        assert_eq!(link_target_at("[just brackets]", 5), None);
        assert_eq!(link_target_at("[text][ref]", 3), None);
        assert_eq!(link_target_at("[ref]: https://x", 2), None);
        assert_eq!(link_target_at("[a](unclosed", 5), None);
        // An empty destination is nothing to copy.
        assert_eq!(link_target_at("[a]()", 2), None);
        // Escaped brackets are literals, not a construct.
        assert_eq!(link_target_at(r"\[a](b)", 3), None);
        // A nested `[` in the caption is refused, as `parse_link` refuses it.
        assert_eq!(link_target_at("[a [b] c](u)", 2), None);
        // An empty line cannot panic on the `col == 0` boundary.
        assert_eq!(link_target_at("", 0), None);
    }

    /// Non-ASCII before the link: the caret offset is in CHARACTERS (what
    /// `GtkTextIter::line_offset` reports), while the destination is sliced by
    /// BYTES — mixing the two would slice mid-character and panic, or address the
    /// wrong link.
    #[test]
    fn link_target_at_uses_char_offsets_not_byte_offsets() {
        let line = "héllo — [a](u)";
        let open = line.chars().position(|c| c == '[').expect("has a link");
        assert_eq!(link_target_at(line, open), Some("u"));
        assert_eq!(link_target_at(line, open + 1), Some("u"));
        assert_eq!(link_target_at(line, open - 1), None);
    }
}
