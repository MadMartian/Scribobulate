//! **One lexer per fragment, read by every walk over it.**
//!
//! Three walks ask questions about the same raw-HTML fragment — which literal text it
//! contributes (the `Text` items of its stream), which disclosure tags it carries
//! (`renderer::disclosure`), and which image candidates it carries
//! (`renderer::picture`). Each used to find its own `<`, call its own [`tag_end`], and
//! decide for itself what a tag was, which meant three tokenizers over one string that
//! could disagree — and did. `literal_text_runs` skipped a `<script>`'s content while
//! the other two walked straight into it, so a `<summary>` written inside a script
//! became a real disclosure's label and an `<img src>` in script source became a live
//! image candidate.
//!
//! **The fix is structural rather than three matching patches.** The fragment is lexed
//! **once**, here, into an ordered stream of tags and shown text runs; a walk consumes
//! that stream and never sees the bytes. An element this module treats as opaque is
//! therefore opaque to every consumer by construction, and a fourth consumer inherits
//! the property without being told about it.
//!
//! Two consequences worth stating because they are the whole point:
//!
//! * **Raw-text content is not in the stream at all.** `<script>`, `<style>`,
//!   `<iframe>` and their siblings ([`is_raw_text_name`]) have their content consumed
//!   by the lexer, so no consumer can be tempted to look inside one.
//! * **The fragment is lowercased once**, not once per raw-text element and again per
//!   consumer. `str::to_ascii_lowercase` preserves byte length, so every offset in the
//!   stream indexes the caller's own string as well as the lowercased twin.

use super::tags::{self, tag_end};
use super::{recognise_html_element, RawHtmlElement};

/// What a lexed tag does to the walks that read it.
///
/// **This is the tag's effect, not its spelling** — a walk asks what a tag *does* and
/// the lexer has already decided, so no consumer re-derives "is this void?" or "does
/// this open something?" from the text and gets a different answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagKind {
    /// One of the allowlisted elements — the only kind a consumer acts on.
    Known(RawHtmlElement),
    /// An unrecognised OPEN tag: it encloses the markup that follows it, so text
    /// inside it is dropped until its close tag.
    Opens,
    /// An unrecognised CLOSE tag.
    Closes,
    /// A tag that encloses nothing. Void (`<br>`), self-closing, a raw-text element
    /// whose content the lexer has already consumed, or one of the comment-family
    /// tokens that is not an element at all.
    ///
    /// **These are one variant deliberately.** A consumer's only question is whether
    /// the tag opens a suppression, and every member of this set answers "no"; giving
    /// them separate variants would invite a walk to treat one of them specially and
    /// re-create the divergence this module exists to prevent.
    Empty,
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
    /// Byte offset of the run within the fragment that was lexed.
    pub(crate) at: usize,
    /// The run's text, untrimmed.
    pub(crate) text: &'a str,
}

/// One tag from the lexed stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RawTag<'a> {
    /// Byte offset of the tag's `<` within the fragment.
    pub(crate) at: usize,
    /// Byte offset one past the tag's `>`.
    pub(crate) end: usize,
    /// The tag exactly as the document spells it. Attribute **values** are
    /// case-sensitive — a URL is — so this is what `attr` must read.
    pub(crate) text: &'a str,
    /// The same tag lowercased, which is what every name and boolean-attribute test
    /// reads. Sliced from the fragment's single lowercased twin, so it costs nothing.
    pub(crate) lower: &'a str,
    /// What this tag does to the walk — see [`TagKind`].
    pub(crate) kind: TagKind,
}

/// One item of the lexed stream, in document order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RawItem<'a> {
    /// A literal-text run the page SHOWS. Runs inside a dropped element, and the
    /// summary's own text, are not in the stream — see [`RawHtml::lex`].
    Text(LiteralRun<'a>),
    /// A tag.
    Tag(RawTag<'a>),
}

/// A raw-HTML fragment, lexed once.
pub(crate) struct RawHtml<'a> {
    src: &'a str,
    /// The fragment lowercased. Byte-for-byte the same length as `src`, so one set of
    /// offsets indexes both.
    lower: String,
    items: Vec<Item>,
}

/// The stored form of one item: offsets only, so the vector borrows nothing.
#[derive(Debug, Clone, Copy)]
enum Item {
    Text {
        at: usize,
        end: usize,
        /// Was the cursor at the top level of the fragment (or inside an allowlisted
        /// element) when this run was met? Decided **here**, during the one walk that
        /// tracks the suppressor stack, so a consumer cannot compute it differently.
        ///
        /// **Raw HTML is sanitised by omission** (TDD 2.23): a fragment's tags are
        /// matched against [`super::RENDERED_HTML_ELEMENTS`] and everything else is
        /// dropped, text included. That rule is correct for `<script>` and wrong for
        /// one case — a `<details>` whose body is not separated by the blank lines
        /// CommonMark requires. With no blank lines the whole construct is ONE
        /// raw-HTML block, so the body never becomes Markdown events and vanished
        /// entirely, where rubric 2.26d promises literal text. The allowlist still
        /// governs which elements may contribute text at all: a run is shown only
        /// when it sits at the top level or inside an allowlisted element, never
        /// inside a `<script>`, `<style>`, `<iframe>` or any other unrecognised one.
        shown: bool,
    },
    Tag {
        at: usize,
        end: usize,
        kind: TagKind,
    },
}

impl<'a> RawHtml<'a> {
    /// Lex one raw-HTML fragment.
    ///
    /// **Suppression is tracked by element NAME, not by a depth count.** A counter is
    /// defeated by any token shaped `</…>`, including one that closes nothing: two
    /// stray `</x>` before a real `</script>` drop the count to zero while the cursor
    /// is still inside the script, and every run after that is emitted as page
    /// content. So the name is pushed and popped only against a match, which also
    /// makes an unmatched close tag inert in *both* directions — the same answer a
    /// browser gives.
    ///
    /// **The raw-text test comes before the self-closing test, and the order is the
    /// security content of this function.** HTML5 does not acknowledge a self-closing
    /// flag on a non-void element: `<script/>` enters script-data state exactly as
    /// `<script>` does. Testing `/>` first made the application the permissive one of
    /// the two, and `<script/>alert(1)</script>` printed its own source as page text.
    /// The two name sets are disjoint — no raw-text element is void — so the order is
    /// free to be the safe one.
    pub(crate) fn lex(src: &'a str) -> Self {
        let lower = src.to_ascii_lowercase();
        let mut items: Vec<Item> = Vec::new();
        // The UNRECOGNISED elements enclosing the cursor, innermost last. Text is
        // dropped while this is non-empty; an allowlisted element never enters it, so
        // `<summary>`'s own text is reached at depth 0 exactly as the top level is.
        let mut suppressors: Vec<String> = Vec::new();
        // `<summary>`'s text is NOT a literal-text run: the disclosure scanner already
        // extracts it and the renderer draws it as the block's label. Emitting it here
        // too would print the label twice — once as the summary line, once as body
        // text.
        let mut in_summary = false;
        let mut cursor = 0usize;
        // Set when the fragment ends mid-construct. Everything left is then a partial
        // tag or a raw-text element's content — never text — and treating it as text
        // would print a half-typed tag at every keystroke of a live-preview session.
        let mut halted = false;

        while let Some(rel) = lower[cursor..].find('<') {
            let lt = cursor + rel;
            items.push(Item::Text {
                at: cursor,
                end: lt,
                shown: suppressors.is_empty() && !in_summary,
            });

            // A comment, doctype, CDATA section or processing instruction is not an
            // element and encloses nothing. `tag_end` is the wrong function for the
            // whole family: it stops at the first `>`, so `<!-- note -->` lexed as a
            // tag whose "name" was `!--`, which nothing ever closes — every literal
            // run after a comment, to the end of the block, was silently dropped.
            if let Some(end) = comment_family_end(&lower, lt) {
                items.push(Item::Tag {
                    at: lt,
                    end,
                    kind: TagKind::Empty,
                });
                cursor = end;
                continue;
            }

            let Some(gt) = tag_end(src, lt) else {
                halted = true;
                break;
            };
            let end = gt + 1;
            cursor = end;
            let tag_lower = &lower[lt..end];

            let kind = match recognise_html_element(tag_lower) {
                Some(el) => {
                    match el {
                        RawHtmlElement::SummaryOpen => in_summary = true,
                        RawHtmlElement::SummaryClose => in_summary = false,
                        _ => {}
                    }
                    TagKind::Known(el)
                }
                None => {
                    let (close, name) = tags::tag_name(tag_lower);
                    if close {
                        // A close tag that matches nothing on the stack closes nothing.
                        if let Some(at) = suppressors.iter().rposition(|n| n == name) {
                            suppressors.truncate(at);
                        }
                        TagKind::Closes
                    } else if is_raw_text_name(name) {
                        // A raw-text element's content is not markup at all: nothing
                        // inside it can open or close a tag, so no `</b>` in a
                        // script's source can end the suppression. Skip to its own
                        // close tag, as a browser's tokenizer does; with none, the
                        // rest of the fragment IS its content.
                        match skip_raw_text(&lower, cursor, name) {
                            Some(after) => cursor = after,
                            None => halted = true,
                        }
                        TagKind::Empty
                    } else if tag_lower.ends_with("/>") || is_void_name(name) {
                        // A void or self-closing unknown element encloses nothing, so
                        // it must not open a suppression that never closes — one
                        // `<br>` would otherwise silence the whole remainder.
                        TagKind::Empty
                    } else {
                        suppressors.push(name.to_owned());
                        TagKind::Opens
                    }
                }
            };
            items.push(Item::Tag { at: lt, end, kind });
            if halted {
                break;
            }
        }

        if !halted {
            items.push(Item::Text {
                at: cursor,
                end: src.len(),
                shown: suppressors.is_empty() && !in_summary,
            });
        }

        Self { src, lower, items }
    }

    /// The fragment this stream was lexed from — for a consumer that needs the bytes
    /// BETWEEN two tags, which the stream deliberately does not carry (a `<summary>`'s
    /// label is the standing case).
    pub(crate) fn src(&self) -> &'a str {
        self.src
    }

    /// The whole stream, in document order.
    pub(crate) fn items(&self) -> impl Iterator<Item = RawItem<'_>> + '_ {
        self.items.iter().filter_map(move |item| match *item {
            Item::Text { .. } => self.shown_run(item).map(RawItem::Text),
            Item::Tag { at, end, kind } => Some(RawItem::Tag(RawTag {
                at,
                end,
                text: &self.src[at..end],
                lower: &self.lower[at..end],
                kind,
            })),
        })
    }

    /// Whether one stored item is a run the page shows, and the run if so.
    ///
    /// **One rule, one place**: [`Self::items`] and [`Self::literal_runs`] are two
    /// views of the same stream and must never differ about which runs exist — which
    /// is this module's whole thesis applied to itself.
    fn shown_run(&self, item: &Item) -> Option<LiteralRun<'a>> {
        let Item::Text { at, end, shown } = *item else {
            return None;
        };
        let text = &self.src[at..end];
        (shown && !text.trim().is_empty()).then_some(LiteralRun { at, text })
    }
}

/// The offset one past a comment, doctype, CDATA section or processing instruction
/// beginning at `lt` — or `None` when `lt` does not begin one.
///
/// **A comment is `<!--` … `-->`, not `<!--` … first `>`.** The two abrupt-closing
/// forms HTML's own tokenizer accepts (`<!-->` and `<!--->`) are honoured, because
/// without them either one would swallow the rest of the fragment as comment text.
fn comment_family_end(lower: &str, lt: usize) -> Option<usize> {
    let rest = lower.get(lt..)?;
    if rest.starts_with("<!--") {
        let body = lt + "<!--".len();
        for abrupt in ["->", ">"] {
            if lower[body..].starts_with(abrupt) {
                return Some(body + abrupt.len());
            }
        }
        // `--!>` is HTML's "incorrectly closed comment", which still ends it. The
        // EARLIER of the two wins; searching for one and falling back to the other
        // would end the comment at the wrong place whenever both appear.
        let end = ["-->", "--!>"]
            .into_iter()
            .filter_map(|close| {
                lower[body..]
                    .find(close)
                    .map(|rel| body + rel + close.len())
            })
            .min();
        return Some(end.unwrap_or(lower.len()));
    }
    if rest.starts_with("<!") || rest.starts_with("<?") {
        // HTML's bogus-comment state ends at the first `>`, quotes and all.
        return Some(
            lower[lt..]
                .find('>')
                .map_or(lower.len(), |rel| lt + rel + 1),
        );
    }
    None
}

/// The offset just past `</name …>`, searching from `from`. `None` when the element is
/// never closed, which means everything left is its content.
fn skip_raw_text(lower: &str, from: usize, name: &str) -> Option<usize> {
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
            return Some(tag_end(lower, lt).map_or(lower.len(), |gt| gt + 1));
        }
        at = after;
    }
}

/// HTML elements whose content is text rather than markup. Inside one, a `<` opens
/// nothing — which is exactly why a `</…>` inside one must not be allowed to close the
/// suppression it sits in.
///
/// **This list is a SECURITY BOUNDARY and its incompleteness is not conservative.**
/// Contrast [`is_void_name`], where an unlisted element merely suppresses *more* text
/// than it needs to. An unlisted raw-text element suppresses *less*: it takes the
/// ordinary suppressor-stack path, where a close tag naming an element further out
/// truncates the stack and releases every suppressor above it, the raw-text one
/// included. `<div><iframe></div>LEAK</iframe>` printed `LEAK` for exactly that reason
/// — in a browser the `</div>` is iframe *content* and nothing is shown.
fn is_raw_text_name(name: &str) -> bool {
    matches!(
        name,
        "script"
            | "style"
            | "textarea"
            | "title"
            | "xmp"
            | "iframe"
            | "noembed"
            | "noframes"
            | "plaintext"
    )
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
    use super::{RawHtml, RawItem, TagKind};

    /// The runs one fragment contributes, read off the stream every consumer reads —
    /// so these assertions cannot pass against a view no production code takes.
    fn literal_text_runs<'a>(doc: &'a RawHtml<'a>) -> Vec<super::LiteralRun<'a>> {
        doc.items()
            .filter_map(|item| match item {
                RawItem::Text(run) => Some(run),
                RawItem::Tag(_) => None,
            })
            .collect()
    }

    /// Every run's text, in document order — what most of these assertions are about.
    fn texts(html: &str) -> Vec<String> {
        literal_text_runs(&RawHtml::lex(html))
            .into_iter()
            .map(|run| run.text.to_owned())
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

    /// F-SEC-201. HTML5 does not acknowledge the self-closing flag on a non-void
    /// element, so `<script/>` enters script-data state exactly as `<script>` does.
    /// Testing the `/>` arm first made the application the permissive one, and the
    /// script's source printed as page text.
    ///
    /// **This is the mutation guard for the arm ORDER**, which is otherwise invisible:
    /// the correctly-spelled forms pass either way, which is why the existing tests
    /// were green over the defect.
    #[test]
    fn a_self_closing_raw_text_element_still_suppresses() {
        for name in ["script", "style", "textarea", "title", "xmp"] {
            let runs = joined(&format!("<details>a<{name}/>LEAK</{name}>b</details>"));
            assert_eq!(
                runs, "ab",
                "<{name}/> must suppress exactly as <{name}> does: {runs:?}"
            );
        }
        // With attributes, and never closed at all.
        assert_eq!(
            joined("<details>a<script src=x/>LEAK</script>b</details>"),
            "ab"
        );
        assert_eq!(joined("<details>a<script/>LEAK"), "a");
    }

    /// F-SEC-202. An outer element's close tag truncated the suppressor stack and
    /// released the raw-text suppressor above it. In a browser the `</div>` is the
    /// iframe's own *content* and nothing is shown.
    #[test]
    fn an_outer_close_tag_cannot_release_a_raw_text_element() {
        for name in ["iframe", "noembed", "noframes", "plaintext"] {
            // Only `a` survives, and that is the browser's answer too: the `</div>`
            // is the raw-text element's own CONTENT, so it closes nothing and `c` is
            // still inside the `<div>`. Before the fix the truncate released both the
            // div and the raw-text suppressor, and `LEAK` printed.
            let runs = joined(&format!(
                "<details>a<div>b<{name}></div>LEAK</{name}>c</details>"
            ));
            assert_eq!(runs, "a", "<{name}> content must stay dropped: {runs:?}");
        }
        assert_eq!(
            joined("<details>a<p>b<iframe></p>LEAK</iframe>c</details>"),
            "a"
        );
        // With no enclosing element to stay inside, the text after the close tag is
        // top-level again — which is what proves the skip ENDS rather than swallows.
        assert_eq!(joined("<details>a<iframe>LEAK</iframe>b</details>"), "ab");
        assert_eq!(joined("<details>a<iframe/>LEAK</iframe>b</details>"), "ab");
    }

    /// The `truncate` rule itself is NOT wrong in general — a browser also treats
    /// `</div>` as implicitly closing an open `<span>`. F-SEC-202 was specifically the
    /// four missing raw-text names, so this pins the general rule against a fix that
    /// over-corrects by removing it.
    #[test]
    fn an_outer_close_tag_still_closes_an_ordinary_inner_element() {
        assert_eq!(
            joined("<details>a<div>b<span></div>LEAK</span>c</details>"),
            "aLEAKc"
        );
    }

    /// F-SEC-203. `tag_end` stops at the first `>`, so `<!-- note -->` lexed as a tag
    /// named `!--` and was pushed onto the suppressor stack, where nothing could ever
    /// pop it — every literal run after the comment was silently deleted.
    #[test]
    fn a_comment_does_not_delete_the_rest_of_the_block() {
        assert_eq!(joined("<details>a<!-- note -->b</details>"), "ab");
        assert_eq!(joined("<details>a<!-- c -->b<!-- d -->e</details>"), "abe");
        assert_eq!(joined("<details>a<!DOCTYPE html>b</details>"), "ab");
        assert_eq!(joined("<details>a<![CDATA[x]]>b</details>"), "ab");
        assert_eq!(joined("<details>a<?php echo 1; ?>b</details>"), "ab");
        // The interaction with `in_summary`: a comment before the summary used to
        // suppress the body outright.
        assert_eq!(
            joined("<details><!-- x --><summary>S</summary>body</details>"),
            "body"
        );
    }

    /// A comment's own text is never page content, and a `>` inside one does not end
    /// it — the whole reason `tag_end` is the wrong function for the family.
    #[test]
    fn a_comments_own_text_is_not_shown() {
        assert_eq!(joined("<details>a<!-- if 1 > 2 then -->b</details>"), "ab");
        assert_eq!(joined("<details>a<!-- unterminated"), "a");
        // HTML's two abrupt-closing forms are complete comments, not runaways.
        assert_eq!(joined("<details>a<!-->b</details>"), "ab");
        assert_eq!(joined("<details>a<!--->b</details>"), "ab");
        // `--!>` is an incorrectly-closed comment that still ends it.
        assert_eq!(joined("<details>a<!-- x --!>b</details>"), "ab");
    }

    /// The shape from F-SEC-203's report: an unspaced `<details>` whose body contains a
    /// bare `<` used to lose the summary label AND the body.
    #[test]
    fn a_bare_less_than_in_a_body_does_not_delete_it() {
        let runs = joined("<details><summary>S</summary>if 1 < 2 then done</details>");
        assert!(
            runs.contains("if 1"),
            "the text before the bare `<` shows: {runs:?}"
        );
    }

    /// **The property the whole module exists for.** Every walk reads one stream, so a
    /// tag inside a raw-text element is invisible to all of them — it is not in the
    /// stream at all, rather than skipped by each consumer in its own way.
    #[test]
    fn no_tag_inside_a_raw_text_element_reaches_the_stream() {
        let doc =
            RawHtml::lex("<details>\n<script>\n<summary>SRC</summary>\n</script>\n</details>");
        let known: Vec<_> = doc
            .items()
            .filter_map(|item| match item {
                RawItem::Tag(tag) => match tag.kind {
                    TagKind::Known(el) => Some(el),
                    _ => None,
                },
                RawItem::Text(_) => None,
            })
            .collect();
        assert_eq!(
            known,
            vec![
                super::RawHtmlElement::DetailsOpen,
                super::RawHtmlElement::DetailsClose
            ],
            "the `<summary>` inside the script is not a tag at all: {known:?}"
        );
        assert!(
            literal_text_runs(&doc).is_empty(),
            "and its text is not a run either"
        );
    }

    /// The two walks used to disagree about the same bytes. They cannot now, because
    /// there is one stream — this asserts the property directly rather than through a
    /// consumer.
    #[test]
    fn text_runs_and_tags_come_from_one_walk() {
        let doc = RawHtml::lex("a<div>b</div>c<script>d</script>e");
        let runs: Vec<_> = literal_text_runs(&doc)
            .into_iter()
            .map(|r| r.text)
            .collect();
        assert_eq!(runs, vec!["a", "c", "e"]);
        let kinds: Vec<_> = doc
            .items()
            .filter_map(|item| match item {
                RawItem::Tag(tag) => Some(tag.kind),
                RawItem::Text(_) => None,
            })
            .collect();
        assert_eq!(
            kinds,
            vec![TagKind::Opens, TagKind::Closes, TagKind::Empty],
            "the `<script>` is Empty because its content is already consumed"
        );
    }
}
