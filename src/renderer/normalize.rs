//! Shared parser options and the inline-tab normalisation pre-pass — both pure and
//! unit-tested. The pre-pass runs at every parse site, and the sites are ENUMERATED
//! rather than summarised: `preview/build.rs`, `export/doc.rs`, `outline.rs` and
//! `copymap::balance_source_span`. The summary form ("used at every parse site") is
//! what this comment used to say while reaching two of the four, and a claim like
//! that terminates the audit it appears to serve — see
//! `normalize_inline_tabs`'s own note on what holds it true now.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Parser options used everywhere a document is walked (preview render, outline,
/// copymap, and the tab-normalisation pre-pass).
///
/// This is an **explicit allowlist of the extensions the renderer actually
/// handles** — deliberately NOT `Options::all()` minus a few. The distinction is
/// a silent-content-loss fix (QA M-1): `Options::all()` also turns on MATH,
/// FOOTNOTES/OLD_FOOTNOTES, WIKILINKS, DEFINITION_LIST and the YAML/`+++` METADATA
/// blocks, none of which the renderer has a handler for. pulldown then emits their
/// events, the dispatcher's `_ => {}` (renderer/events.rs) drops them, and copymap
/// `classify` returns `None` — so `$E=mc^2$` rendered EMPTY, `[^1]` vanished, and
/// `---`/`+++` frontmatter leaked as a stray paragraph. Enabling only what we
/// support makes every unsupported construct degrade to its literal source text
/// (visible) instead of silently disappearing.
///
/// Deliberately NOT enabled: SUPERSCRIPT (`^`), SUBSCRIPT (`~`) and STRIKETHROUGH
/// (`~~`) — this crate tokenises all three itself (`scan_scripts`). pulldown's
/// native versions recognise the delimiter with CommonMark *flanking* rules (the
/// marker must sit against whitespace/punctuation on its OUTER side), so the tight,
/// Pandoc-style `E=mc^2^` / `H~2~O` authors actually type never match;
/// worse, any enabled tilde feature makes pulldown treat every `~` as a delimiter
/// candidate and *fragments a tight multi-tilde line across several `Text` events*
/// (`H~2~O and CO~2~` → `"…CO~2"`, `"~"`, `"…"`), which a per-event scanner cannot
/// reassemble. Keeping them off means each paragraph arrives as clean,
/// un-fragmented literal `Text` for `scan_scripts` to interpret with tight
/// semantics. (See ScrAP-66/ScrAP-75: pulldown-cmark caret/tilde flanking &
/// fragmentation.)
pub(crate) fn md_options() -> Options {
    // Only the extensions the renderer has handlers for: TABLES (incl. the
    // tab-normalised tables), TASKLISTS (the checkbox `TaskListMarker`),
    // SMART_PUNCTUATION + HEADING_ATTRIBUTES (text/attribute transforms already
    // rendered correctly), and GFM (GitHub-flavoured parsing parity). Anything not
    // listed degrades to literal text instead of vanishing (QA M-1).
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_GFM
}

/// Replace each hard tab (`\t`) with a single space, EXCEPT where a tab is
/// structurally load-bearing and must stay verbatim:
///   * a **leading** tab (only whitespace precedes it on its line) — CommonMark
///     expands leading tabs to 4-column tab stops for block structure (indented
///     code, list/quote continuation indent); collapsing one to a single space
///     would silently change the block structure.
///   * a tab inside **inline code** or a **code block** — code content is verbatim.
///
/// Everything else (a tab between table cells, or after `---` in a delimiter row,
/// or a mid-line alignment tab in prose) is normalised, because GFM rejects a table
/// whose delimiter row contains a tab — the whole block then renders as a literal
/// paragraph, and smart-punctuation even turns each `---` into an em-dash. The
/// substitution is **length- and position-preserving** (a tab and a space are both a
/// single ASCII byte at the same offset), so every source byte offset the renderer /
/// `copymap` / `source_map` capture emits still indexes the same logical position in
/// the editor's text — no scroll-sync or copy-offset drift. See ScrAP-75.
///
/// **Every site that parses the document calls this first**, so all four read one
/// document: `preview/build.rs`, `export/doc.rs`, `outline::extract_headings` and
/// `copymap::balance_source_span`. Two of them did not, and the divergence was
/// user-visible on both — the outline listed a phantom heading the page never showed,
/// and an editor annotation wrapped a span across a table cell boundary. The
/// enumeration above is backed by `every_parse_site_reads_one_document` (below)
/// rather than by this sentence: a comment asserting a contract is a claim that needs
/// a test behind it.
pub(crate) fn normalize_inline_tabs(md: &str) -> std::borrow::Cow<'_, str> {
    if !md.contains('\t') {
        return std::borrow::Cow::Borrowed(md);
    }
    // Verbatim (code) byte ranges from a structural pre-parse: a tab inside any of
    // these is code content and is left untouched. A table broken BY tabs parses as
    // prose here (not code), so its tabs fall outside every range and DO get
    // normalised — after which the real parse (of the normalised text) sees a table.
    let mut code: Vec<std::ops::Range<usize>> = Vec::new();
    let mut depth = 0i32;
    for (ev, range) in Parser::new_ext(md, md_options()).into_offset_iter() {
        match &ev {
            Event::Start(Tag::CodeBlock(_)) => {
                depth += 1;
                code.push(range);
            }
            Event::End(TagEnd::CodeBlock) => depth -= 1,
            Event::Code(_) => code.push(range),
            _ if depth > 0 => code.push(range),
            _ => {}
        }
    }
    // Coalesce the verbatim ranges into disjoint, ascending intervals so membership
    // is an O(log C) binary search per tab instead of an O(C) linear scan of every
    // range (F-PERF-001: the old `code.iter().any(...)` was O(T×C) for T tabs).
    // Merging is REQUIRED, not just sorting: the ranges nest — a code block pushes
    // its own whole-block range AND every inner event's sub-range — so a plain
    // start-ordered `partition_point` would miss a byte inside an outer block range
    // that sits past a later-starting inner range.
    code.sort_unstable_by_key(|r| r.start);
    let mut merged: Vec<std::ops::Range<usize>> = Vec::with_capacity(code.len());
    for r in code {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    let in_code = |i: usize| {
        let idx = merged.partition_point(|r| r.start <= i);
        idx > 0 && merged[idx - 1].end > i
    };

    let bytes = md.as_bytes();
    let mut out = md.to_owned();
    // SAFETY: we only overwrite a single-byte ASCII tab (0x09) with a single-byte
    // ASCII space (0x20) at the same index — UTF-8 validity and length are preserved.
    let out_bytes = unsafe { out.as_bytes_mut() };
    // "A leading (indentation) tab has only whitespace before it on its line" —
    // carried FORWARD as a one-bit state rather than re-derived by walking
    // backwards per tab (QA round 3, D-3). The backwards walk was bounded by the
    // line, which is fine for prose and quadratic for a document with no
    // newlines: measured before this change, release build, on a file of nothing
    // but tabs — 50 KiB 363 ms, 100 KiB 1.48 s, 200 KiB 5.89 s, 400 KiB 23.8 s,
    // a clean 4x per doubling, synchronously on the GTK main thread. The
    // predicate is bit-for-bit the same one, computed once per byte instead of
    // once per tab times the line length.
    //
    // `leading_ws` is the state BEFORE the byte at `i` is considered, which is
    // what the old `bytes[..i]` slice expressed — hence the read-then-update
    // order below. A `\r` is not whitespace here, exactly as `all(' ' | '\t')`
    // treated it.
    let mut leading_ws = true;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            leading_ws = true;
            continue;
        }
        if b == b'\t' && !leading_ws && !in_code(i) {
            out_bytes[i] = b' ';
        }
        if b != b' ' && b != b'\t' {
            leading_ws = false;
        }
    }
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod normalize_inline_tabs_tests {

    /// **Every parse site reads one document.** Checked by what each site CONCLUDES
    /// about a tab-padded table, not by the doc comment above saying so — the comment
    /// said it for as long as the pre-pass reached two sites of four, and a claim like
    /// that terminates the audit it appears to serve.
    ///
    /// The two divergences this pins were MEASURED before the fix, and both were
    /// user-visible: the outline listed a phantom H1 whose text was the entire table,
    /// and `balance_source_span` swallowed an emphasis run across a table cell
    /// boundary, so an editor annotation wrapped CriticMarkup over two cells.
    ///
    /// `export/doc.rs` stands in for the preview's own parse here — the preview is
    /// GTK-bound, and the two enter through the same pre-pass and the same
    /// `md_options` by construction (`export/doc.rs`'s module doc).
    #[test]
    fn every_parse_site_reads_one_document() {
        // A GFM table padded with hard TABS — the construct the pre-pass exists for
        // (ScrAP-75) — carrying an emphasis run that straddles a cell boundary, and
        // trailed by a setext underline. A site that skips the pre-pass reads this
        // whole block as one paragraph instead.
        const TAB_TABLE: &str = "| Name\t| Value\t|\n|---\t|---\t|\n| **a\t| b** |\nCaption\n===\n";

        let doc = crate::export::doc::build(
            TAB_TABLE,
            &crate::export::RenderOptions {
                doc_dir: None,
                allow_unsafe_images: false,
            },
        );
        assert!(
            matches!(doc.blocks.first(), Some(crate::export::Block::Table { .. })),
            "export/doc.rs: expected a Table, got {:?}",
            doc.blocks.first()
        );

        // outline.rs — the page shows a table and no heading, so the sidebar must not
        // invent one. (Unnormalised, `===` underlines the paragraph the broken table
        // collapsed into and this returned a phantom H1.)
        assert_eq!(
            crate::outline::extract_headings(TAB_TABLE),
            vec![],
            "outline.rs: a heading the rendered page does not have"
        );

        // copymap::balance_source_span — `**a` and `b**` sit in DIFFERENT cells, so
        // there is no inline construct to widen to. Unnormalised the block was a
        // paragraph, pulldown emitted one Strong over `**a\t| b**`, and the selection
        // widened across the cell boundary.
        let at = TAB_TABLE.find("**a").expect("fixture");
        let sel = at..at + 3;
        assert_eq!(
            crate::copymap::balance_source_span(TAB_TABLE, sel.clone()),
            sel,
            "copymap: widened across a table cell boundary"
        );

        // And the pre-pass keeps a heading's own text in step with the rendered one.
        assert_eq!(
            crate::outline::extract_headings("# Chapter\tOne\n")
                .first()
                .map(|h| h.text.clone()),
            Some("Chapter One".to_string())
        );
    }

    use super::{md_options, normalize_inline_tabs};
    use pulldown_cmark::{Event, Parser, Tag};

    fn norm(md: &str) -> String {
        normalize_inline_tabs(md).into_owned()
    }

    /// The whole point (ScrAP-75): a table whose header/delimiter/body cells carry
    /// tabs parses as a paragraph, but after normalisation it parses as a table.
    #[test]
    fn tab_broken_table_parses_after_normalisation() {
        let md = "|A\t|B\t|\n|---\t|---\t|\n|x|y|";
        let starts_table = |s: &str| {
            Parser::new_ext(s, md_options()).any(|ev| matches!(ev, Event::Start(Tag::Table(_))))
        };
        assert!(!starts_table(md), "tabs should break table recognition");
        assert!(
            starts_table(&norm(md)),
            "normalised source must parse as a table"
        );
        // Non-leading tabs became spaces; length is preserved.
        assert_eq!(norm(md), "|A |B |\n|--- |--- |\n|x|y|");
        assert_eq!(norm(md).len(), md.len());
    }

    /// A leading (indentation) tab is block structure — an indented code block — and
    /// must survive so it keeps rendering as code, not a one-space paragraph.
    #[test]
    fn leading_indentation_tab_is_preserved() {
        let md = "\tindented code line";
        assert_eq!(norm(md), md);
    }

    /// Tabs inside a fenced code block (mid-line) and an inline code span are
    /// verbatim content and must not be altered.
    #[test]
    fn tabs_inside_code_are_preserved() {
        let fenced = "```\nfoo\tbar\n```";
        assert_eq!(norm(fenced), fenced);
        let inline = "text `a\tb` more";
        assert_eq!(norm(inline), inline);
    }

    /// A mid-line prose tab is cosmetic whitespace → normalised to one space.
    #[test]
    fn mid_line_prose_tab_becomes_a_space() {
        assert_eq!(norm("a\tb"), "a b");
    }

    /// No tab → the input is borrowed unchanged (fast path).
    #[test]
    fn tab_free_input_is_borrowed() {
        assert!(matches!(
            normalize_inline_tabs("no tabs here"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    /// QA M-1 regression: unsupported extensions must degrade to LITERAL text, not
    /// vanish. With MATH/FOOTNOTES/WIKILINKS off, pulldown must NOT emit the special
    /// events the renderer's `_ => {}` would silently drop (`$…$` → empty, `[^1]` →
    /// gone); their raw source must survive in the `Text` stream instead.
    #[test]
    fn unsupported_extensions_degrade_to_literal_text_not_dropped_events() {
        let md = "Euler $E=mc^2$ and a note[^1] and a [[WikiLink]].";
        let evs: Vec<Event> = Parser::new_ext(md, md_options()).collect();
        // None of the drop-prone special events are produced.
        assert!(
            !evs.iter().any(|e| matches!(
                e,
                Event::InlineMath(_) | Event::DisplayMath(_) | Event::FootnoteReference(_)
            )),
            "math/footnote extensions must be OFF so their events are never emitted"
        );
        // The raw source survives as visible Text.
        let text: String = evs
            .iter()
            .filter_map(|e| match e {
                Event::Text(t) => Some(t.as_ref()),
                _ => None,
            })
            .collect();
        assert!(text.contains("$E=mc^2$"), "math renders as literal text");
        assert!(
            text.contains("[^1]"),
            "footnote ref renders as literal text"
        );
        assert!(
            text.contains("[[WikiLink]]"),
            "wikilink renders as literal text"
        );
    }

    /// The supported extensions the renderer DOES handle stay enabled — guards the
    /// allowlist against dropping a working feature (QA M-1).
    #[test]
    fn supported_extensions_stay_enabled() {
        let table = "| a | b |\n|---|---|\n| 1 | 2 |";
        assert!(
            Parser::new_ext(table, md_options()).any(|e| matches!(e, Event::Start(Tag::Table(_)))),
            "TABLES must stay on"
        );
        let task = "- [x] done\n- [ ] todo";
        assert!(
            Parser::new_ext(task, md_options()).any(|e| matches!(e, Event::TaskListMarker(_))),
            "TASKLISTS must stay on"
        );
    }

    // ── input-limit regressions (QA round 3, D-3) ─────────────────────────────

    /// Tab normalisation must stay linear on a document with no newlines.
    ///
    /// The leading-tab test used to walk BACKWARDS from each tab to the start of
    /// its line. Bounded by the line, which is invisible in prose and quadratic
    /// in a file that is one enormous line. Measured before the fix (release
    /// build, a file of nothing but tabs): 50 KiB 363 ms, 100 KiB 1.48 s,
    /// 200 KiB 5.89 s, 400 KiB 23.8 s — 4x per doubling. After carrying the
    /// predicate forward as one bit of state: 400 KiB 0.7 ms, 4 MiB 6.4 ms.
    ///
    /// Asserted as a growth RATIO for the same reason as the sibling guard in
    /// `annotate::scan` — the exponent is the property that regressed and it is
    /// machine-independent, where a wall-clock bound is either flaky or blind.
    #[test]
    fn tab_normalisation_over_a_single_enormous_line_grows_linearly() {
        fn time_norm(kib: usize) -> std::time::Duration {
            let src = "\t".repeat(kib * 1024);
            let t = std::time::Instant::now();
            let out = normalize_inline_tabs(&src);
            // Consume the result, and pin the BEHAVIOUR: every one of these tabs
            // is leading whitespace on its (single, endless) line, so not one of
            // them is rewritten.
            assert_eq!(out.len(), src.len());
            assert!(!out.contains(' '), "leading tabs must not be rewritten");
            t.elapsed()
        }

        let small = time_norm(128);
        let large = time_norm(512); // 4x the input
        let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-9);
        assert!(
            ratio < 8.0,
            "normalisation grew {ratio:.1}x for 4x the input ({small:?} -> \
             {large:?}). Linear is ~4x, quadratic ~16x — this looks like the \
             per-tab backwards line walk (QA R3 D-3) has come back."
        );
        // A ratio alone can be defeated by a CONSTANT-FACTOR speedup on a still
        // quadratic algorithm (qa's point): make it 4x faster and the ratio
        // drifts under the threshold while the exponent stands. So pair it with
        // an absolute ceiling that only a linear implementation can meet. The
        // post-fix debug measurement for this input is ~5 ms and the
        // PRE-fix measurement was tens of seconds — the budget below sits far above the
        // former and far below the latter, so it discriminates without being
        // sensitive to machine speed.
        assert!(
            large < std::time::Duration::from_millis(3000),
            "{large:?} for 512 KiB is far past any linear implementation's cost \
             — the growth ratio may have been masked by a constant-factor \
             speedup on a still-quadratic algorithm."
        );
    }

    /// The forward-carried `leading_ws` bit must be EXACTLY the predicate the
    /// backwards walk computed. Asserted DIFFERENTIALLY — the old expression is
    /// reproduced verbatim here and the two are compared at every byte of a
    /// corpus — rather than by hand-written expectations about the normaliser's
    /// output.
    ///
    /// That choice was forced by getting it wrong first. The hand-written
    /// version of this test asserted `norm("\ta\tb") == "\ta b"` and failed,
    /// and the rewrite was not the reason: a document beginning with a tab is an
    /// INDENTED CODE BLOCK, so `in_code` suppresses every rewrite in it and the
    /// expectation was simply wrong about Markdown. Comparing the predicate to
    /// its own predecessor tests the thing that actually changed, and cannot be
    /// fooled by a second mechanism sitting downstream of it.
    ///
    /// The corpus covers what a rewrite of this shape gets wrong: the state must
    /// be the one BEFORE the current byte, `\r` is not whitespace, a newline
    /// restarts it, and a run of mixed spaces and tabs is still "leading".
    #[test]
    fn the_leading_whitespace_predicate_is_unchanged_by_the_rewrite() {
        /// The pre-fix expression, preserved verbatim as the oracle.
        fn old_leading(bytes: &[u8], i: usize) -> bool {
            bytes[..i]
                .iter()
                .rev()
                .take_while(|&&c| c != b'\n')
                .all(|&c| c == b' ' || c == b'\t')
        }

        for src in [
            "\ta\tb",
            " \t \ta\tb",
            "a\tb\n\tc\td",
            "\r\ta",
            "x\n\ty",
            "\n\n\t\t x\ty\n\r\t\tz",
            "no tabs at all",
            "\t",
            "\n\t\n\t",
            "a\t\tb\t\n\t \tc",
            "\r\n\ta\tb\r\n\t",
            "  \t  \tq\tr\ts",
        ] {
            let bytes = src.as_bytes();
            let mut leading_ws = true;
            for (i, &b) in bytes.iter().enumerate() {
                if b == b'\n' {
                    leading_ws = true;
                    continue;
                }
                assert_eq!(
                    leading_ws,
                    old_leading(bytes, i),
                    "leading-whitespace predicate diverged at byte {i} of {src:?}"
                );
                if b != b' ' && b != b'\t' {
                    leading_ws = false;
                }
            }
        }
    }
}
