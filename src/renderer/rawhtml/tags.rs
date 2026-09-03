//! Tag lexing for the raw-HTML allowlist: where one tag ENDS, and what attributes it
//! carries. One owner for both, because both answers are decided by the same thing —
//! quote state — and a scanner that answers either by searching for a delimiter is
//! wrong in the same way.
//!
//! **A `>` inside a quoted attribute value does not end a tag.** `tail.find('>')` says
//! it does, and every scanner in this module used to. The consequence was not cosmetic:
//! a tag split at the wrong `>` leaves its own tail to be re-scanned as markup, so
//! `<div title="a>b">` was read as a tag `<div title="a>` followed by text `b">`, and
//! a non-allowlisted element's text reached the page.
//!
//! **Quote state is entered only where an attribute VALUE may begin** — after an `=`,
//! optional whitespace aside. That is the HTML tokenizer's own rule
//! (*before-attribute-value* is the only state that can enter a quoted value), and it
//! is why an apostrophe in an unquoted value (`<a href=it's>`) is ordinary text here
//! rather than the start of a quoted run that swallows the rest of the block.

/// One attribute read off a tag: its lowercased name, and its value when it has one.
///
/// `value: None` is the boolean form (`<details open>`), which is distinct from an
/// empty value (`open=""`) — HTML treats both as TRUE, but only [`attr`] cares which
/// it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TagAttr {
    /// Lowercased attribute name.
    pub(crate) name: String,
    /// The unquoted value, or `None` for a valueless (boolean) attribute.
    pub(crate) value: Option<String>,
}

/// The byte index of the `>` that terminates the tag starting at `lt`, or `None` when
/// the tag is unterminated (the caller's block ends there).
///
/// `lt` must index a `<`. Quote-aware — see the module docs for why that is the whole
/// point of this function existing.
pub(crate) fn tag_end(html: &str, lt: usize) -> Option<usize> {
    let bytes = html.as_bytes();
    debug_assert_eq!(bytes.get(lt), Some(&b'<'), "tag_end starts at a `<`");
    let mut i = lt + 1;
    // Is the cursor at a point where an attribute value may begin? Set by `=`, held
    // across the whitespace that may follow it, cleared by anything else.
    let mut value_may_begin = false;
    while i < bytes.len() {
        match bytes[i] {
            b'>' => return Some(i),
            q @ (b'"' | b'\'') if value_may_begin => {
                // An unterminated quote is an unterminated TAG: a browser keeps
                // consuming to the end of input looking for the closing quote, and so
                // do we by refusing to name a `>` at all.
                let close = html[i + 1..].find(q as char)?;
                i += 1 + close + 1;
                value_may_begin = false;
                continue;
            }
            b'=' => value_may_begin = true,
            b if b.is_ascii_whitespace() => {}
            _ => value_may_begin = false,
        }
        i += 1;
    }
    None
}

/// Whether `tag_lower` is a CLOSE tag, and its lowercased element name.
///
/// `tag_lower` is one whole tag from `<` through `>`, already lowercased.
pub(crate) fn tag_name(tag_lower: &str) -> (bool, &str) {
    let body = tag_lower.strip_prefix('<').unwrap_or(tag_lower);
    let (close, body) = match body.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, body),
    };
    let end = body
        .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
        .unwrap_or(body.len());
    (close, &body[..end])
}

/// Every attribute on one tag, in document order, with quote state honoured.
///
/// `tag` is one whole tag; the enclosing `<`/`>` are optional, so a caller holding an
/// already-sliced inner text may pass that. Names come back lowercased; values do not,
/// because a URL is case-sensitive.
///
/// **This is the only place that decides where one attribute ends and the next
/// begins.** [`attr`] and [`has_attr`] are two questions about the same list, and the
/// boundary rule they used to share was a substring search over the raw tag text —
/// which reads a name out of *another attribute's quoted value* (`<details
/// title="open">` answered `has_attr(…, "open")` with TRUE).
pub(crate) fn attributes(tag: &str) -> Vec<TagAttr> {
    let s = tag.strip_prefix('<').unwrap_or(tag);
    let s = s.strip_suffix('>').unwrap_or(s);
    let bytes = s.as_bytes();
    let mut out = Vec::new();

    // Step over the element name (and a close tag's `/`); attributes start after it.
    let mut i = usize::from(bytes.first() == Some(&b'/'));
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'/' {
        i += 1;
    }

    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'/'
        {
            i += 1;
        }
        if i == start {
            // A position where no attribute NAME can start — a stray `=`, say. HTML's
            // tokenizer reports the parse error and carries on with the next
            // character; abandoning the tag here instead lost every attribute after
            // it, so `<img =x src=a.png>` yielded no `src` at all (F-TEST-A-011).
            if i < bytes.len() {
                i += 1;
                continue;
            }
            break;
        }
        let name = s[start..i].to_ascii_lowercase();

        // The `=` may be separated from its name by whitespace, and so may its value.
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        let value = if bytes.get(j) == Some(&b'=') {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            Some(read_value(s, &mut j))
        } else {
            None
        };
        i = j;
        out.push(TagAttr { name, value });
    }
    out
}

/// Read one attribute value starting at `*at`, advancing `*at` past it: a quoted run
/// up to its closing quote, or an unquoted token up to the next whitespace.
fn read_value(s: &str, at: &mut usize) -> String {
    let bytes = s.as_bytes();
    match bytes.get(*at) {
        Some(&q @ (b'"' | b'\'')) => {
            let rest = &s[*at + 1..];
            let value = rest.split(q as char).next().unwrap_or_default();
            // `+1` for the closing quote, clamped for the unterminated case.
            *at = (*at + 1 + value.len() + 1).min(s.len());
            value.to_owned()
        }
        Some(_) => {
            let start = *at;
            while *at < bytes.len() && !bytes[*at].is_ascii_whitespace() {
                *at += 1;
            }
            s[start..*at].to_owned()
        }
        None => String::new(),
    }
}

/// Extract attribute `name`'s value from one tag. `name` must be lowercase; the tag is
/// matched case-insensitively. Returns the unquoted value, or `None` when the attribute
/// is absent **or** carries no value.
pub(crate) fn attr(tag: &str, name: &str) -> Option<String> {
    attributes(tag)
        .into_iter()
        .find(|a| a.name == name)
        .and_then(|a| a.value)
}

/// Is boolean attribute `name` PRESENT on this tag? `<details open>` carries no value,
/// so [`attr`] — which reports a valueless attribute as `None` — cannot answer it.
///
/// HTML's rule is presence, not value — `open`, `open=""` and `open="false"` are all
/// TRUE. That last one reads wrong and is correct: the attribute's presence is the
/// whole signal, which is why a document cannot express "explicitly closed" and does
/// not need to.
pub(crate) fn has_attr(tag: &str, name: &str) -> bool {
    attributes(tag).iter().any(|a| a.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end_of(html: &str) -> Option<&str> {
        tag_end(html, 0).map(|gt| &html[..=gt])
    }

    #[test]
    fn a_plain_tag_ends_at_its_own_bracket() {
        assert_eq!(end_of("<img src=a>tail"), Some("<img src=a>"));
    }

    /// F-010's defect: the `>` inside a quoted value split the tag, and the tail was
    /// re-scanned as markup — which is how text escaped a non-allowlisted element.
    #[test]
    fn a_bracket_inside_a_quoted_value_does_not_end_the_tag() {
        assert_eq!(
            end_of(r#"<div title="a>b">x"#),
            Some(r#"<div title="a>b">"#)
        );
        assert_eq!(end_of("<div title='a>b'>x"), Some("<div title='a>b'>"));
    }

    #[test]
    fn an_apostrophe_in_an_unquoted_value_is_ordinary_text() {
        // Not the start of a quoted run: quote state is only entered where a value may
        // begin, so this tag ends at its own `>` rather than swallowing the block.
        assert_eq!(end_of("<a href=it's>x"), Some("<a href=it's>"));
    }

    #[test]
    fn whitespace_around_the_equals_still_enters_the_value() {
        assert_eq!(
            end_of(r#"<div title = "a>b">x"#),
            Some(r#"<div title = "a>b">"#)
        );
    }

    #[test]
    fn an_unterminated_quote_leaves_the_tag_unterminated() {
        assert_eq!(tag_end(r#"<div title="a>b"#, 0), None);
    }

    #[test]
    fn an_unterminated_tag_has_no_end() {
        assert_eq!(tag_end("<scr", 0), None);
    }

    #[test]
    fn tag_names_read_open_and_close_forms() {
        assert_eq!(tag_name("<details open>"), (false, "details"));
        assert_eq!(tag_name("</details>"), (true, "details"));
        assert_eq!(tag_name("<br/>"), (false, "br"));
        assert_eq!(tag_name("<script>"), (false, "script"));
    }

    #[test]
    fn attributes_come_back_in_order_with_lowercased_names() {
        let got = attributes(r#"<img SRC="a.png" alt='x y' hidden>"#);
        assert_eq!(
            got,
            vec![
                TagAttr {
                    name: "src".into(),
                    value: Some("a.png".into())
                },
                TagAttr {
                    name: "alt".into(),
                    value: Some("x y".into())
                },
                TagAttr {
                    name: "hidden".into(),
                    value: None
                },
            ]
        );
    }

    #[test]
    fn a_self_closing_tag_yields_no_phantom_attribute() {
        assert_eq!(
            attributes("<img src=a.png />"),
            vec![TagAttr {
                name: "src".into(),
                value: Some("a.png".into())
            }]
        );
    }

    /// F-SEC-004: the old substring-plus-boundary rule read `open` out of a
    /// *different* attribute's quoted value, so a document could force a disclosure
    /// open by titling it.
    #[test]
    fn an_attribute_name_inside_another_attributes_value_is_not_an_attribute() {
        assert!(!has_attr(r#"<details title="open">"#, "open"));
        assert!(!has_attr(r#"<details title='keep open'>"#, "open"));
        assert!(has_attr(r#"<details title="x" open>"#, "open"));
        assert!(has_attr("<details open>", "open"));
        assert!(has_attr(r#"<details open="false">"#, "open"));
    }

    #[test]
    fn a_name_that_is_a_prefix_of_another_is_not_that_other() {
        assert_eq!(
            attr(r#"<source srcset="a.webp 2x" src="b.png">"#, "src"),
            Some("b.png".into())
        );
        assert!(!has_attr("<details opened>", "open"));
    }

    /// F-TEST-A-011: a stray `=` where a name should start made the reader abandon
    /// the whole tag, so every attribute after it was invisible — including the one
    /// carrying the URL.
    #[test]
    fn a_stray_delimiter_does_not_abandon_the_rest_of_the_tag() {
        assert_eq!(attr("<img =x src=a.png>", "src"), Some("a.png".into()));
        assert_eq!(attr("<img = src=a.png>", "src"), Some("a.png".into()));
        assert_eq!(attr("<details == open>", "open"), None);
        assert!(has_attr("<details == open>", "open"));
    }

    #[test]
    fn a_value_containing_a_bracket_survives_intact() {
        assert_eq!(
            attr(r#"<img src="a>b.png">"#, "src"),
            Some("a>b.png".into())
        );
    }

    #[test]
    fn a_valueless_attribute_reads_as_present_but_valueless() {
        assert_eq!(attr("<details open>", "open"), None);
        assert!(has_attr("<details open>", "open"));
    }
}
