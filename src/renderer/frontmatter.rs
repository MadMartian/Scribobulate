//! Front matter: the metadata block a document may open with, and the event stream
//! every walk reads it through. Pure, GTK-free, unit-tested.
//!
//! # What the parser does with it, and why that is not good enough
//!
//! CommonMark has no notion of front matter, and this build deliberately does not
//! enable pulldown-cmark's `ENABLE_YAML_STYLE_METADATA_BLOCKS` (see [`super::normalize`]
//! — an enabled-but-unhandled extension DROPS its construct, ScrAP-78). So the parser
//! reads `---` on line 1 as a thematic break, and the closing `---` as a **setext
//! underline** for everything between them. MEASURED on this project's own
//! `.claude/agents/dev.md`:
//!
//! ```text
//!   0..4    Rule
//!   4..197  Start(Heading { level: H2, … })   <- the ENTIRE front matter block
//!           Text("name: dev") SoftBreak Text("description: …") …
//! ```
//!
//! A horizontal rule and one enormous heading — which the outline sidebar then lists
//! as a real heading, because it walks the same events. The block is not silently
//! dropped, so this is not the ScrAP-78 failure; it is worse-looking than the literal
//! text TDD 2.25 used to promise for it, and it invents document structure that the
//! author never wrote.
//!
//! # What this module does instead
//!
//! [`scan`] answers *where* the front matter is, as byte extents, and nothing else.
//! [`events`] is the walk seam every parse site reads a document through: given
//! [`Show::Omitted`] it parses the document from **after** the closing fence and shifts
//! every range back into whole-document space, so the front matter is simply not there
//! for the outline and the exports; given [`Show::AsDisclosure`] it prepends the nine
//! events of a `<details>` wrapping a fenced code block, each pointing at the real
//! source bytes it stands for.
//!
//! **Synthetic events, not a rewritten document.** The preview's source map, the
//! char-precise copymap and the fold keys are all built from event source ranges, so a
//! rewrite that inserted `<details>` text would shift every offset below it and put the
//! editor and the preview into different coordinate systems. The nine events below tile
//! the front matter's real bytes exactly — opening fence line, body, closing fence line
//! — with no gap and no overlap, so a copy across the block reproduces what the author
//! typed, and scroll-sync lands on the line they are looking at.
//!
//! Nothing in the renderer knows this module exists: the events are indistinguishable
//! from the ones an author writing the `<details>` by hand would produce, which is what
//! "reuse the code-block render inside a disclosure" has to mean if the two are ever to
//! stay in step.

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Parser, Tag, TagEnd};
use std::ops::Range;

/// The raw-HTML fragment the synthetic opening block carries.
///
/// ONE constant rather than a label plus a format string: the summary's text is read
/// back out of this fragment by [`super::disclosure::scan_disclosure_tags`] on its way
/// to the rendered line, so a separately-stated label could disagree with what the
/// reader sees. No `open` attribute — front matter collapses by default (TDD 2.27).
const OPEN_HTML: &str = "<details><summary>Frontmatter</summary>\n";

/// The raw-HTML fragment the synthetic closing block carries.
const CLOSE_HTML: &str = "</details>\n";

/// Which metadata dialect a document's fences declare.
///
/// The two that exist in the wild: YAML (`---`, Jekyll's original and by far the
/// commonest) and TOML (`+++`, Hugo and Zola). Both are one fence line, a body, and a
/// closing fence line, which is why one scan answers for both — they differ only in the
/// marker and in the language the code block is highlighted as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flavour {
    Yaml,
    Toml,
}

impl Flavour {
    /// The flavour `line` opens, if it is a fence at all.
    ///
    /// Trailing blanks are tolerated because an editor that strips them and one that
    /// does not must read the same document; leading blanks are NOT, because a fence is
    /// a column-0 construct and an indented `---` inside a list is not front matter.
    fn opened_by(line: &str) -> Option<Self> {
        match line.trim_end_matches([' ', '\t']) {
            "---" => Some(Self::Yaml),
            "+++" => Some(Self::Toml),
            _ => None,
        }
    }

    /// Does `line` close a block this flavour opened?
    ///
    /// YAML accepts `...` as well as `---`: it is the YAML spec's own end-of-document
    /// marker, and a generator that emits it is writing valid front matter.
    fn closes(self, line: &str) -> bool {
        let line = line.trim_end_matches([' ', '\t']);
        match self {
            Self::Yaml => line == "---" || line == "...",
            Self::Toml => line == "+++",
        }
    }

    /// The fenced-code-block language the body is rendered and highlighted as.
    fn lang(self) -> &'static str {
        match self {
            Self::Yaml => "yaml",
            Self::Toml => "toml",
        }
    }
}

/// A document's front matter, as byte extents into the document it was scanned from.
///
/// The three ranges TILE the block: `open.end == body.start` and `body.end ==
/// close.start`, with every line terminator falling inside one of them. That is the
/// property the synthetic events rely on — nine events covering exactly these bytes,
/// once each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frontmatter {
    pub(crate) flavour: Flavour,
    /// The opening fence line, its terminator included.
    pub(crate) open: Range<usize>,
    /// Everything between the fences — the metadata itself.
    pub(crate) body: Range<usize>,
    /// The closing fence line, its terminator included.
    pub(crate) close: Range<usize>,
}

impl Frontmatter {
    /// The first byte of the document proper.
    pub(crate) fn end(&self) -> usize {
        self.close.end
    }
}

/// How a walk wants the front matter presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Show {
    /// A collapsed `<details>` whose body is a fenced code block — the preview.
    AsDisclosure,
    /// Not at all — the outline, and both exports. The document's own content is
    /// unaffected either way, because the events below the block are identical.
    Omitted,
}

/// Each line of `md` as its content (terminator excluded) and the byte range it
/// occupies (terminator INCLUDED).
///
/// Written out rather than taken from `str::lines`, for two reasons: `lines` yields no
/// ranges, and it treats a trailing terminator as ending the iteration rather than as
/// part of the final line — both of which this scan needs the other way round.
///
/// `\r\n` is recognised because CRLF SURVIVES a document read: `docio` normalises a
/// lone `\r` and nothing else, since a line ending is a property of the document rather
/// than of the host (POLICY.md § Cross-platform by default).
fn lines_with_ranges(md: &str) -> impl Iterator<Item = (&str, Range<usize>)> {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        if at >= md.len() {
            return None;
        }
        let start = at;
        let end = md[at..].find('\n').map_or(md.len(), |i| at + i + 1);
        at = end;
        let raw = &md[start..end];
        let content = raw.strip_suffix('\n').unwrap_or(raw);
        let content = content.strip_suffix('\r').unwrap_or(content);
        Some((content, start..end))
    })
}

/// The front matter `md` opens with, if it opens with any.
///
/// # What is deliberately NOT front matter
///
/// A document may legitimately open with a thematic break, and `---`, a blank line,
/// some prose and another `---` is exactly that — a rule, a paragraph, a rule. Taking
/// it as front matter would fold the author's opening paragraph into a metadata block
/// they never wrote. The discriminator is the FIRST BODY LINE: real front matter's is a
/// key, so it is never blank, and an empty block (`---` immediately followed by `---`)
/// is two rules rather than metadata with nothing in it. Both are refused here.
///
/// An UNCLOSED opening fence is not front matter either, for the reason the unclosed
/// `<details>` case is refused in [`super::disclosure::scan_document`]: the recovery
/// that treats the rest of the file as the block's body lets one stray line at the top
/// of a document hide all of it.
pub(crate) fn scan(md: &str) -> Option<Frontmatter> {
    let mut lines = lines_with_ranges(md);
    let (first, open) = lines.next()?;
    let flavour = Flavour::opened_by(first)?;

    let mut first_body_line = true;
    for (content, range) in lines {
        if flavour.closes(content) {
            // An empty block is two thematic breaks, not metadata.
            if first_body_line {
                return None;
            }
            return Some(Frontmatter {
                flavour,
                body: open.end..range.start,
                open,
                close: range,
            });
        }
        if first_body_line {
            // A blank opening line means the `---` above was a rule and this is prose.
            if content.trim().is_empty() {
                return None;
            }
            first_body_line = false;
        }
    }
    None
}

/// The nine events a front-matter block renders as: a raw-HTML `<details>` opening
/// block, a fenced code block holding the metadata, and a raw-HTML `</details>`.
///
/// Shaped to match what pulldown-cmark emits for the same Markdown written by hand —
/// verified against the parser rather than assumed: a raw-HTML block is a
/// `Start(HtmlBlock)`/`Html`/`End(HtmlBlock)` trio, and a fenced code block is a
/// `Start`, one `Text` per line **and one `Text("\n")` per line ending** (MEASURED
/// pulldown-cmark 0.13.4; the ending's range is two bytes under CRLF and the `\r` is in
/// no event's text), then `End`. The renderer therefore needs no front-matter branch at
/// all — and the body is split here exactly as pulldown splits it, because handing it
/// over as one `Text` put a Windows document's carriage returns on the page as extra
/// line breaks (TDD 2.27 under CRLF, the 1.0.0 manual pass).
fn disclosure_events<'a>(md: &'a str, fm: &Frontmatter) -> Vec<(Event<'a>, Range<usize>)> {
    let Frontmatter {
        flavour,
        open,
        body,
        close,
    } = fm;
    let mut events = vec![
        (Event::Start(Tag::HtmlBlock), open.clone()),
        (Event::Html(CowStr::Borrowed(OPEN_HTML)), open.clone()),
        (Event::End(TagEnd::HtmlBlock), open.clone()),
        (
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(CowStr::Borrowed(
                flavour.lang(),
            )))),
            body.clone(),
        ),
    ];
    for (content, line) in lines_with_ranges(&md[body.clone()]) {
        let start = body.start + line.start;
        let ending = start + content.len()..body.start + line.end;
        events.push((Event::Text(CowStr::Borrowed(content)), start..ending.start));
        if !ending.is_empty() {
            events.push((Event::Text(CowStr::Borrowed("\n")), ending));
        }
    }
    events.extend([
        (Event::End(TagEnd::CodeBlock), body.clone()),
        (Event::Start(Tag::HtmlBlock), close.clone()),
        (Event::Html(CowStr::Borrowed(CLOSE_HTML)), close.clone()),
        (Event::End(TagEnd::HtmlBlock), close.clone()),
    ]);
    events
}

/// **The walk seam.** Parse `md` — which the caller has already normalised and cleaned
/// — into events whose ranges index `md`, with its front matter presented as `show`
/// asks.
///
/// Every production site that walks a whole document reads it through here, so "what
/// does front matter render as?" is answered once rather than per consumer. A document
/// with no front matter takes the cost of one first-line comparison and is otherwise
/// the plain parse it always was.
pub(crate) fn events(md: &str, show: Show) -> impl Iterator<Item = (Event<'_>, Range<usize>)> {
    let fm = scan(md);
    let (prefix, body_at) = match (&fm, show) {
        (Some(fm), Show::AsDisclosure) => (disclosure_events(md, fm), fm.end()),
        (Some(fm), Show::Omitted) => (Vec::new(), fm.end()),
        (None, _) => (Vec::new(), 0),
    };
    // Parsing the REMAINDER, rather than the whole document with the front matter's
    // events filtered out, is what makes the omission exact: pulldown reads the closing
    // fence as a setext underline for the lines above it, so those lines are not
    // separable events to drop — the heading IS them.
    prefix.into_iter().chain(
        Parser::new_ext(&md[body_at..], super::md_options())
            .into_offset_iter()
            .map(move |(ev, r)| (ev, r.start + body_at..r.end + body_at)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape this feature exists for, byte for byte.
    const DEV_MD: &str = "---\nname: dev\nmodel: opus\n---\n\n# Role\n\nBody.\n";

    #[test]
    fn a_yaml_block_is_found_and_its_three_ranges_tile_it() {
        let fm = scan(DEV_MD).expect("front matter");
        assert_eq!(fm.flavour, Flavour::Yaml);
        assert_eq!(&DEV_MD[fm.open.clone()], "---\n");
        assert_eq!(&DEV_MD[fm.body.clone()], "name: dev\nmodel: opus\n");
        assert_eq!(&DEV_MD[fm.close.clone()], "---\n");
        // Tiling: no gap, no overlap, and the document proper starts after it.
        assert_eq!(fm.open.end, fm.body.start);
        assert_eq!(fm.body.end, fm.close.start);
        assert_eq!(&DEV_MD[fm.end()..], "\n# Role\n\nBody.\n");
    }

    #[test]
    fn a_toml_block_is_found_and_highlights_as_toml() {
        let md = "+++\ntitle = \"Post\"\n+++\n\nBody.\n";
        let fm = scan(md).expect("front matter");
        assert_eq!(fm.flavour, Flavour::Toml);
        assert_eq!(fm.flavour.lang(), "toml");
        assert_eq!(&md[fm.body.clone()], "title = \"Post\"\n");
    }

    #[test]
    fn a_yaml_block_may_close_with_the_yaml_end_of_document_marker() {
        let fm = scan("---\na: 1\n...\n\nBody.\n").expect("front matter");
        assert_eq!(fm.flavour, Flavour::Yaml);
        assert_eq!(fm.end(), 13);
    }

    #[test]
    fn crlf_line_endings_are_read_as_line_endings() {
        let md = "---\r\nname: dev\r\n---\r\n\r\n# Role\r\n";
        let fm = scan(md).expect("front matter");
        assert_eq!(&md[fm.open.clone()], "---\r\n");
        assert_eq!(&md[fm.body.clone()], "name: dev\r\n");
        assert_eq!(&md[fm.close.clone()], "---\r\n");
    }

    #[test]
    fn a_fence_with_trailing_blanks_still_opens_and_closes() {
        assert!(scan("--- \na: 1\n---\t\n").is_some());
    }

    /// The false positive the first-body-line rule exists to refuse: a document that
    /// opens with a horizontal rule and happens to have another one below it.
    #[test]
    fn a_document_opening_with_a_thematic_break_is_not_front_matter() {
        assert_eq!(scan("---\n\nSome prose.\n\n---\n\nMore.\n"), None);
    }

    #[test]
    fn an_empty_block_is_two_rules_rather_than_front_matter() {
        assert_eq!(scan("---\n---\n"), None);
    }

    #[test]
    fn an_unclosed_fence_is_not_front_matter() {
        assert_eq!(scan("---\nname: dev\n\n# Role\n"), None);
    }

    #[test]
    fn an_indented_fence_is_not_front_matter() {
        assert_eq!(scan("  ---\na: 1\n---\n"), None);
    }

    #[test]
    fn an_ordinary_document_has_none() {
        assert_eq!(scan("# Title\n\nBody.\n"), None);
        assert_eq!(scan(""), None);
        assert_eq!(scan("***\na: 1\n***\n"), None);
    }

    /// A front-matter block is not front matter anywhere but line 1.
    #[test]
    fn a_block_below_the_first_line_is_not_front_matter() {
        assert_eq!(scan("# Title\n\n---\na: 1\n---\n"), None);
    }

    #[test]
    fn omitted_yields_the_document_below_the_block_at_its_real_offsets() {
        let evs: Vec<_> = events(DEV_MD, Show::Omitted).collect();
        // Nothing from the block, and the first heading reports the offsets the
        // EDITOR's text has — the property scroll-sync and find both stand on.
        let (first, range) = &evs[0];
        assert!(matches!(first, Event::Start(Tag::Heading { .. })));
        assert_eq!(&DEV_MD[range.clone()], "# Role\n");
        assert!(
            !evs.iter().any(|(ev, _)| matches!(ev, Event::Rule)),
            "the opening fence reached the walk as a thematic break"
        );
    }

    #[test]
    fn as_disclosure_yields_a_details_wrapping_a_fenced_code_block() {
        let evs: Vec<_> = events(DEV_MD, Show::AsDisclosure).collect();
        let n = disclosure_events(DEV_MD, &scan(DEV_MD).expect("front matter")).len();
        let kinds: Vec<String> = evs
            .iter()
            .take(n)
            .map(|(ev, _)| format!("{ev:?}"))
            .collect();
        assert!(kinds[1].contains("<details><summary>Frontmatter</summary>"));
        assert!(matches!(
            evs[3].0,
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_)))
        ));
        // The body as pulldown would report it: a line, its ending, a line, its ending.
        let body: Vec<&str> = evs[4..n - 4]
            .iter()
            .map(|(ev, _)| match ev {
                Event::Text(t) => &**t,
                other => panic!("expected a Text event in the body, got {other:?}"),
            })
            .collect();
        assert_eq!(body, vec!["name: dev", "\n", "model: opus", "\n"]);
        assert!(kinds[n - 2].contains("</details>"));
        // And the document below it follows, unshifted.
        let (heading, range) = &evs[n];
        assert!(matches!(heading, Event::Start(Tag::Heading { .. })));
        assert_eq!(&DEV_MD[range.clone()], "# Role\n");
    }

    /// The synthetic events cover the block's bytes once each, in order — the property
    /// a copy across the rendered block reproduces the author's text by: the three
    /// constructs tile the block, and the body's line events tile the body.
    #[test]
    fn the_synthetic_events_tile_the_block_exactly() {
        for md in [
            DEV_MD,
            "---\r\nname: dev\r\nmodel: opus\r\n---\r\n\r\n# Role\r\n",
        ] {
            let fm = scan(md).expect("front matter");
            let evs = disclosure_events(md, &fm);
            // A construct's events share ONE range (pulldown reports the whole construct
            // on its Start, its Html/Text and its End alike).
            for (ev, range) in &evs[..4] {
                assert_eq!(
                    range,
                    if matches!(ev, Event::Start(Tag::CodeBlock(_))) {
                        &fm.body
                    } else {
                        &fm.open
                    }
                );
            }
            let n = evs.len();
            assert_eq!(
                evs[n - 4].1,
                fm.body,
                "End(CodeBlock) carries the body's range"
            );
            for (_, range) in &evs[n - 3..] {
                assert_eq!(range, &fm.close);
            }
            assert_eq!(fm.open.end, fm.body.start);
            assert_eq!(fm.body.end, fm.close.start);
            assert_eq!(
                fm.close.end,
                fm.end(),
                "the events stop where the block does"
            );
            // The line events tile the body, carry no carriage return, and every ending
            // is reported as "\n" over the bytes it occupies — two under CRLF.
            let mut at = fm.body.start;
            for (ev, range) in &evs[4..n - 4] {
                assert_eq!(range.start, at, "a gap or an overlap in the body's tiling");
                at = range.end;
                let Event::Text(t) = ev else {
                    panic!("a body event that is not Text: {ev:?}")
                };
                assert!(!t.contains('\r'), "a carriage return in {t:?}");
                if &**t == "\n" {
                    assert!(matches!(&md[range.clone()], "\n" | "\r\n"));
                } else {
                    assert_eq!(&**t, &md[range.clone()]);
                }
            }
            assert_eq!(at, fm.body.end, "the line events stop where the body does");
        }
    }

    /// Both modes agree about everything below the block — which is what lets the
    /// preview and the export disagree about the block alone.
    #[test]
    fn the_two_modes_agree_about_the_document_below_the_block() {
        let n = disclosure_events(DEV_MD, &scan(DEV_MD).expect("front matter")).len();
        let shown: Vec<_> = events(DEV_MD, Show::AsDisclosure)
            .skip(n)
            .map(|(ev, r)| (format!("{ev:?}"), r))
            .collect();
        let omitted: Vec<_> = events(DEV_MD, Show::Omitted)
            .map(|(ev, r)| (format!("{ev:?}"), r))
            .collect();
        assert_eq!(shown, omitted);
    }

    /// **Every sink agrees about front matter** (TDD 2.27), checked by what each one
    /// CONCLUDES rather than by this module asserting about itself — the sibling of
    /// `normalize::every_parse_site_reads_one_document`, and for the same reason: the
    /// modes only pay off if each consumer actually reads through the one it needs.
    ///
    /// The preview's own half is GTK-bound and lives with the render
    /// (`preview::build::front_matter_renders_as_a_collapsed_code_block`); what stands
    /// in for it here is the disclosure pre-scan, which is what tells that render there
    /// is a foldable block at all.
    #[test]
    fn every_sink_agrees_about_front_matter() {
        // outline/mod.rs — the sidebar lists the document's headings, and front matter
        // declares none. Unshielded, the closing fence underlines the whole block and
        // this returned one H2 whose text was every metadata key the author wrote.
        let headings = crate::outline::extract_headings(DEV_MD);
        assert_eq!(
            headings.iter().map(|h| h.text.as_str()).collect::<Vec<_>>(),
            vec!["Role"],
            "outline/mod.rs: a heading the document does not declare"
        );

        // export/doc.rs — an exported page or PDF carries content, and metadata is not
        // content. Unshielded, the first block was a thematic rule.
        let doc = crate::export::doc::build(
            DEV_MD,
            &crate::export::RenderOptions {
                doc_dir: None,
                allow_unsafe_images: false,
            },
        );
        assert!(
            matches!(
                doc.blocks.first(),
                Some(crate::export::Block::Heading { level: 1, .. })
            ),
            "export/doc.rs: expected the document to open at its first heading, got {:?}",
            doc.blocks.first()
        );

        // renderer/disclosure.rs — the pre-scan the preview render folds from. One
        // block, CLOSED (only a closed one may collapse, rubric 2.26d), keyed at the
        // document's first byte.
        let spans = super::super::disclosure::scan_document(DEV_MD, Show::AsDisclosure);
        assert_eq!(spans.len(), 1, "one foldable block: {spans:?}");
        assert_eq!(spans[0].start, 0);
        assert!(!spans[0].open, "collapsed by default");
        assert!(spans[0].body.is_some(), "closed, so it may collapse");

        // …and the same scan under the mode the EXPORT walks with sees nothing, which
        // is what keeps its ordinal cursor paired with its own walk.
        assert!(super::super::disclosure::scan_document(DEV_MD, Show::Omitted).is_empty());

        // winstate/statusbar.rs — the word count reads the document through the export
        // pipeline, so it inherits the omission rather than being taught it separately.
        // Asserted here anyway: the inheritance is the kind of fact that stops being
        // true without anyone editing this feature.
        let with_front_matter = crate::winstate::statusbar::TextCount::of_markdown(DEV_MD);
        let without = crate::winstate::statusbar::TextCount::of_markdown(
            &DEV_MD[scan(DEV_MD).expect("front matter").end()..],
        );
        assert_eq!(
            with_front_matter.words, without.words,
            "the status bar counted the metadata as prose"
        );
        assert!(
            without.words > 0,
            "a positive control: there is prose to count"
        );
    }

    /// The MANUAL-TEST fixtures say what a tester will see; this is what makes that
    /// claim checkable without a display. A manual step whose expected outcome nobody
    /// verified is a step that gets ticked rather than run.
    #[test]
    fn the_manual_test_fixtures_hold_what_their_steps_promise() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

        for (name, flavour, lang) in [
            ("frontmatter.md", Flavour::Yaml, "yaml"),
            ("frontmatter-toml.md", Flavour::Toml, "toml"),
        ] {
            let md = std::fs::read_to_string(dir.join(name)).expect("a checked-in fixture");
            let fm = scan(&md).unwrap_or_else(|| panic!("{name} opens with front matter"));
            assert_eq!(fm.flavour, flavour, "{name}");
            assert_eq!(fm.flavour.lang(), lang, "{name}");
        }

        // The outline step: exactly the two headings the fixture's own prose names.
        let md = std::fs::read_to_string(dir.join("frontmatter.md")).expect("fixture");
        assert_eq!(
            crate::outline::extract_headings(&md)
                .iter()
                .map(|h| h.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Front matter", "A heading below it"],
            "the outline step in MANUAL-TEST §2.27 names two entries"
        );

        // The "compare against the authored block" step needs an authored one to
        // compare against, and the preview must see BOTH.
        let spans = super::super::disclosure::scan_document(&md, Show::AsDisclosure);
        assert_eq!(spans.len(), 2, "the synthetic block and the authored one");

        // The export step. Asserted against the `ExportDoc` both sinks are built from
        // rather than against rendered HTML: the document is what either artefact can
        // possibly contain, and reading it needs no theme and therefore no display.
        let doc = crate::export::doc::build(
            &md,
            &crate::export::RenderOptions {
                doc_dir: None,
                allow_unsafe_images: false,
            },
        );
        let exported = format!("{:?}", doc.blocks);
        // Tokens only the METADATA carries. Deliberately not the summary label, which
        // the fixture's own prose mentions — an assertion that cannot tell the thing
        // from a description of the thing proves nothing.
        for absent in ["TAILMARKER", "Scribobulate test fixture", "draft"] {
            assert!(
                !exported.contains(absent),
                "the export carries {absent:?}, which MANUAL-TEST 2.27 says it must not"
            );
        }
        assert!(
            exported.contains("A heading below it"),
            "a positive control: the document's own content DID export"
        );
        assert!(
            matches!(
                doc.blocks.first(),
                Some(crate::export::Block::Heading { level: 1, .. })
            ),
            "the exported document opens at its first heading: {:?}",
            doc.blocks.first()
        );
    }

    /// A document with no front matter parses exactly as it did before this seam
    /// existed — the same events at the same offsets, under either mode.
    #[test]
    fn a_document_without_front_matter_is_untouched_by_either_mode() {
        const MD: &str = "# Title\n\nSome *prose* and a `code` span.\n\n- one\n- two\n";
        let plain: Vec<_> = Parser::new_ext(MD, super::super::md_options())
            .into_offset_iter()
            .map(|(ev, r)| (format!("{ev:?}"), r))
            .collect();
        for show in [Show::AsDisclosure, Show::Omitted] {
            let got: Vec<_> = events(MD, show)
                .map(|(ev, r)| (format!("{ev:?}"), r))
                .collect();
            assert_eq!(
                got, plain,
                "{show:?} disturbed a document with no front matter"
            );
        }
    }
}
