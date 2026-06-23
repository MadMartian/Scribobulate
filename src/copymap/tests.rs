use super::*;
use pulldown_cmark::Parser;

// ── faithful render simulation (real pulldown offsets + mirrored buffer math) ──
//
// The resolver depends on two facts per event: its *source* byte range (from
// pulldown, taken verbatim here) and the *buffer* char range the renderer
// produced. This sim mirrors `renderer.rs`'s buffer arithmetic (block_sep
// idempotence, heading trailing newline, scan_scripts marker stripping) for the
// constructs these tests use, so `(md, evs)` is a faithful stand-in for a real
// render. The `gtk-integration-tests` in `preview.rs` validate the *real*
// buffer-offset capture end-to-end.

#[derive(Default)]
struct Sim {
    count: i32,
    trailing: i32,
    at_start: bool,
    /// The buffer *slice* — text plus one U+FFFC per anchored child, so its char
    /// count matches `count`/iter offsets (the copy + `debug_verify` basis).
    text: String,
    /// Mirrors `Renderer.list_first_item`: set by `Tag::List`, cleared by the
    /// first `Tag::Item` it opens — the first item takes no leading newline (the
    /// list's block_sep/newline already provided it).
    list_first_item: bool,
    /// Nesting depth of open lists (top-level list block_seps; nested lists take a
    /// single newline).
    list_depth: i32,
}

impl Sim {
    fn new() -> Self {
        Sim {
            at_start: true,
            ..Default::default()
        }
    }
    fn insert(&mut self, s: &str) {
        self.count += s.chars().count() as i32;
        self.text.push_str(s);
        self.trailing = 0;
        self.at_start = false;
    }
    fn newline(&mut self) {
        self.count += 1;
        self.text.push('\n');
        self.trailing += 1;
        self.at_start = false;
    }
    fn block_sep(&mut self) {
        if self.at_start {
            return;
        }
        while self.trailing < 2 {
            self.newline();
        }
    }
    fn anchor(&mut self) {
        self.count += 1; // one U+FFFC object-replacement char
        self.text.push('\u{FFFC}');
        self.trailing = 0;
        self.at_start = false;
    }
}

/// Apply an event's buffer effect, mirroring `renderer.rs`.
fn apply(sim: &mut Sim, ev: &Event, code_depth: &mut i32, opaque_depth: &mut i32) {
    // Inside a code block, Text is accumulated, not inserted, until End flushes it.
    match ev {
        Event::Start(Tag::Heading { .. })
        | Event::Start(Tag::Paragraph)
        | Event::Start(Tag::BlockQuote(_)) => sim.block_sep(),
        Event::Start(Tag::List(_)) => {
            if sim.list_depth == 0 {
                sim.block_sep();
            } else if !sim.at_start {
                sim.newline();
            }
            sim.list_depth += 1;
            sim.list_first_item = true;
        }
        Event::End(TagEnd::List(_)) => sim.list_depth -= 1,
        Event::Start(Tag::Table(_)) => {
            // A table renders as ONE anchored U+FFFC at End(Table); its cell text
            // never enters the buffer (it lives in the cell widgets), so suppress
            // inner Text like an image's alt.
            sim.block_sep();
            *opaque_depth += 1;
        }
        Event::End(TagEnd::Table) => {
            *opaque_depth -= 1;
            sim.anchor();
        }
        Event::Start(Tag::CodeBlock(_)) => {
            sim.block_sep();
            *code_depth += 1;
        }
        Event::End(TagEnd::CodeBlock) => {
            *code_depth -= 1;
            sim.insert("CODE\n"); // stand-in flushed block body
        }
        Event::Start(Tag::Item) => {
            if sim.list_first_item {
                sim.list_first_item = false;
            } else if !sim.at_start {
                sim.newline();
            }
            // No marker text: a bullet/number/checkbox is drawn in the gutter and
            // occupies zero buffer chars. The item content
            // starts immediately, exactly like the real renderer.
        }
        Event::Start(Tag::Image { .. }) => {
            sim.block_sep();
            sim.anchor();
            *opaque_depth += 1;
        }
        Event::End(TagEnd::Image) => *opaque_depth -= 1,
        Event::Text(t) => {
            if *code_depth > 0 {
                // accumulated, flushed at End(CodeBlock)
            } else if *opaque_depth > 0 {
                // suppressed image alt text
            } else {
                for (run, _) in scan_scripts(t) {
                    sim.insert(&run);
                }
            }
        }
        Event::Code(t) => sim.insert(t),
        // The task checkbox is drawn in the gutter — it inserts
        // NO buffer char (previously an anchored U+FFFC). Its `[ ]`/`[x]` source is still
        // reconstructed by the copymap from the classified event's source span.
        Event::TaskListMarker(_) => {}
        // A soft/hard break renders as a real newline everywhere, including inside a list
        // item — the flow-to-spaces workaround is reverted, now
        // that items carry a uniform per-level margin with no hanging indent (TDD 2.20).
        Event::SoftBreak | Event::HardBreak => sim.newline(),
        Event::End(TagEnd::Heading(_)) => sim.newline(),
        Event::Rule => {
            sim.block_sep();
            sim.anchor();
            sim.newline();
        }
        _ => {}
    }
}

/// Parse `md`, simulate the render, and build the copymap plus the buffer slice
/// (anchors as U+FFFC) — the pure pipeline a real render drives (minus GTK).
fn render(md: &str) -> (CopyTree, String, String) {
    let mut sim = Sim::new();
    let (mut code_depth, mut opaque_depth) = (0, 0);
    let mut evs = Vec::new();
    // Streamed: no list-item look-ahead is needed — markers are drawn in the gutter and
    // insert no buffer text, so there is nothing to suppress (mirrors `preview::build`).
    for (ev, r) in Parser::new_ext(md, crate::renderer::md_options()).into_offset_iter() {
        let before = sim.count;
        apply(&mut sim, &ev, &mut code_depth, &mut opaque_depth);
        let after = sim.count;
        if let Some(kind) = classify(&ev) {
            evs.push(RawEv {
                buf: (before, after),
                src: r,
                kind,
            });
        }
    }
    (build(md, &evs, sim.count), md.to_string(), sim.text)
}

fn tree(md: &str) -> (CopyTree, String) {
    let (t, md, _slice) = render(md);
    (t, md)
}

/// Resolve a selection by *rendered-text* substring, so tests read naturally:
/// find `needle` in the buffer text and copy that char range.
fn copy(md: &str, buffer_text: &str, needle: &str) -> String {
    let a = buffer_text.find(needle).expect("needle in buffer text");
    let a = buffer_text[..a].chars().count() as i32;
    let b = a + needle.chars().count() as i32;
    let (t, md) = tree(md);
    resolve(&t, &md, a, b)
}

// ── constraint A/D: within a construct excludes its delimiter ──────────────────

#[test]
fn within_bold_excludes_the_stars() {
    // buffer text: "a bold b"; select "ol" inside the bold run.
    assert_eq!(copy("a **bold** b", "a bold b", "ol"), "ol");
}

#[test]
fn within_heading_excludes_the_hash() {
    // D: selecting just heading text copies no `#`.
    assert_eq!(copy("# title", "title", "itl"), "itl");
}

#[test]
fn within_single_line_blockquote_excludes_the_marker() {
    assert_eq!(copy("> quote", "quote", "uot"), "uot");
}

#[test]
fn within_code_span_excludes_the_backticks() {
    assert_eq!(copy("`code` x", "code x", "od"), "od");
}

#[test]
fn within_link_caption_excludes_brackets_and_url() {
    // Partial caption crosses no boundary → plain caption fragment.
    assert_eq!(copy("[caption](http://x)", "caption", "apt"), "apt");
}

// ── constraint B/C: crossing a boundary balances delimiters ───────────────────

#[test]
fn crossing_out_of_bold_completes_the_delimiter() {
    // Select from inside the bold run through text outside → balanced `**…**`.
    let (t, md) = tree("a **bold** b");
    // buffer "a bold b": a0 sp1 b2 o3 l4 d5 sp6 b7 ; select [3,8) = "old b"
    assert_eq!(resolve(&t, &md, 3, 8), "**old** b");
}

#[test]
fn strikethrough_trace_c() {
    // The canonical PLAN trace C.
    // buffer "strike out outside"; select from "ke out" through " outs".
    let (t, md) = tree("~~strike out~~ outside");
    // "strike out outside": s0..; select [4,15) per the plan.
    assert_eq!(resolve(&t, &md, 4, 15), "~~ke out~~ outs");
}

#[test]
fn crossing_into_link_reconstructs_whole_url() {
    // From before the link into its caption → `[cap](url)` with the whole URL,
    // caption truncated at the interior endpoint.
    let (t, md) = tree("see [caption](http://x) end");
    // buffer "see caption end": select "e cap" [2,7)
    assert_eq!(resolve(&t, &md, 2, 7), "e [cap](http://x)");
}

#[test]
fn crossing_out_of_heading_into_paragraph() {
    // M1: mid-heading into the next paragraph captures a truncated marker.
    let (t, md) = tree("# title\n\nbody");
    // buffer "title\nbody"? heading trailing newline + block_sep → "title\n\nbody"
    // heading content buf [0,5); select from "tle" into "bo".
    // "title\n\nbody": t0 i1 t2 l3 e4 \n5 \n6 b7 o8 d9 y10 (len 11)
    assert_eq!(resolve(&t, &md, 2, 9), "# tle\n\nbo");
}

// ── constraint B whole-document = Copy Document ───────────────────────────────

#[test]
fn whole_document_selection_returns_all_source() {
    let md = "# H\n\n**b** and `c`";
    let (t, md) = tree(md);
    resolve(&t, &md, 0, t.char_count); // sanity: does not panic
    assert_eq!(resolve(&t, &md, 0, t.char_count), md);
}

// ── prose is character-precise across blocks ───────────────────────────────

#[test]
fn prose_across_paragraphs_is_char_precise_with_blank_line() {
    let (t, md) = tree("para one\n\npara two");
    // buffer "para one\n\npara two"; select "one\n\npara" -> tail+blank+head
    // p0 a1 r2 a3 sp4 o5 n6 e7 \n8 \n9 p10 a11 r12 a13 sp14 t15 w16 o17
    assert_eq!(resolve(&t, &md, 5, 16), "one\n\npara t");
}

// ── atomicity: escapes / entities copy whole (no half-token) ──────────────────

#[test]
fn superscript_and_subscript_reconstruct_char_precisely() {
    // Tight scripts are scan_scripts constructs (not pulldown events): the marker
    // becomes an open/close delimiter reconstructed on crossing.
    let (t, md) = tree("E=mc^2^"); // buffer "E=mc2"
    assert_eq!(resolve(&t, &md, 4, 5), "2"); // within the superscript → bare digit
    assert_eq!(resolve(&t, &md, 3, 5), "c^2^"); // crossing into it → `^2^`
    let (t2, md2) = tree("H~2~O"); // buffer "H2O"
    assert_eq!(resolve(&t2, &md2, 1, 2), "2"); // within the subscript
    assert_eq!(resolve(&t2, &md2, 0, 3), "H~2~O"); // whole run reconstructs markers
}

#[test]
fn padded_code_span_degrades_to_opaque() {
    // CommonMark strips one space each side of `` ` x ` `` → rendered "x" ≠ the
    // content source " x ", so the code span can't align 1:1 and is copied whole.
    let (t, md) = tree("a ` x ` b"); // buffer "a x b"
                                     // "a x b": a0 sp1 x2 sp3 b4 ; the code renders as a single "x" glyph at [2,3)
    assert_eq!(resolve(&t, &md, 2, 3), "` x `");
}

#[test]
fn entity_token_is_atomic() {
    // pulldown emits `&amp;` as its own Text token (src "&amp;", rendered "&"),
    // which is non-1:1 → any overlap copies the whole token source "&amp;".
    assert_eq!(copy("x &amp; y", "x & y", "&"), "&amp;");
}

#[test]
fn escaped_char_keeps_its_backslash_atomically() {
    // pulldown DROPS the escaping backslash from every token's source (for `a \* b`
    // the tokens are "a " and "* b"; byte 2, the `\`, belongs to no event — ScrAP-73).
    // The copymap folds it back onto its char as an ATOMIC `\x` leaf, so a
    // selection confined to the bare escaped char copies the backslash too.
    let (t, md) = tree(r"a \* b");
    // buffer "a * b": a0 sp1 *2 sp3 b4
    assert_eq!(resolve(&t, &md, 2, 3), r"\*"); // bare `*` → keeps its backslash
    assert_eq!(resolve(&t, &md, 1, 4), r" \* "); // across the boundary still works
    assert_eq!(resolve(&t, &md, 0, 5), r"a \* b"); // whole run: no double-count
                                                   // Two escapes reconstruct SYMMETRICALLY (formerly the leading `\` was lost).
    let (t2, md2) = tree(r"x \_y\_ z"); // buffer "x _y_ z"
    assert_eq!(resolve(&t2, &md2, 2, 5), r"\_y\_"); // select "_y_"
                                                    // The odd-run rule: an escaped backslash `\\` (renders one `\`) is NOT re-peeled
                                                    // into a second phantom backslash — it is already an atomic non-1:1 token.
    let (t3, md3) = tree(r"a \\ b"); // buffer "a \ b"
    assert_eq!(resolve(&t3, &md3, 2, 3), r"\\"); // the lone `\` glyph → `\\`
}

// ── table cells: char-precise Markdown per cell (formatting preserved) ─────────

/// Build the per-cell copymaps for a table, mirroring `preview.rs`'s capture:
/// a cell's offset basis is its label's plain text (`copymap::cell_width`), and
/// each cell is its own root (no Copy-Document special case). Returns one tree
/// per `TableCell` in document (row-major) order.
fn cell_trees(md: &str) -> Vec<CopyTree> {
    let mut maps = Vec::new();
    let mut active = false;
    let mut evs: Vec<RawEv> = Vec::new();
    let mut off = 0i32;
    for (ev, src) in Parser::new_ext(md, crate::renderer::md_options()).into_offset_iter() {
        let kind = classify(&ev);
        match &ev {
            Event::Start(Tag::TableCell) => {
                active = true;
                evs.clear();
                off = 0;
            }
            Event::End(TagEnd::TableCell) => {
                maps.push(build(md, &evs, off));
                active = false;
            }
            _ if active => {
                if let Some(k) = &kind {
                    let w = cell_width(k);
                    evs.push(RawEv {
                        buf: (off, off + w),
                        src: src.clone(),
                        kind: k.clone(),
                    });
                    off += w;
                }
            }
            _ => {}
        }
    }
    maps
}

#[test]
fn cell_copy_preserves_bold_char_precisely() {
    let md = "| **bold** cell | x |\n|---|---|\n| y | z |";
    let cells = cell_trees(md);
    // cell label plain text = "bold cell": b0 o1 l2 d3 sp4 c5 e6 l7 l8
    assert_eq!(resolve_cell(&cells[0], md, 0, 9), "**bold** cell"); // whole cell
    assert_eq!(resolve_cell(&cells[0], md, 0, 4), "bold"); // within bold → no **
    assert_eq!(resolve_cell(&cells[0], md, 5, 9), "cell"); // the plain tail
    assert_eq!(resolve_cell(&cells[0], md, 2, 6), "**ld** c"); // crossing out of bold
}

#[test]
fn cell_copy_preserves_code_span() {
    let md = "| a `code` b | x |\n|---|---|\n| y | z |";
    let cells = cell_trees(md);
    // "a code b": a0 sp1 c2 o3 d4 e5 sp6 b7
    assert_eq!(resolve_cell(&cells[0], md, 0, 8), "a `code` b");
    assert_eq!(resolve_cell(&cells[0], md, 2, 6), "code"); // within backticks
}

#[test]
fn cell_copy_preserves_a_link_in_a_mixed_cell() {
    let md = "| see [x](http://y) here | q |\n|---|---|\n| a | b |";
    let cells = cell_trees(md);
    // "see x here": s0 e1 e2 sp3 x4 sp5 h6 e7 r8 e9
    assert_eq!(resolve_cell(&cells[0], md, 0, 10), "see [x](http://y) here");
    assert_eq!(resolve_cell(&cells[0], md, 4, 5), "x"); // within the caption → no []()
}

// ── opaque constructs: whole source on any overlap ────────────────────────────

#[test]
fn image_is_opaque() {
    // buffer holds one U+FFFC for the image; any overlap copies the whole tag.
    let (t, md) = tree("![alt](img.png)");
    assert_eq!(resolve(&t, &md, 0, 1), "![alt](img.png)");
}

// ── Q2: multi-line blockquote is char-precise (per-line `> ` gap-gated) ────────

/// Select the buffer chars matching `needle` (found in the render's slice) and
/// resolve them — reads naturally across block constructs.
fn pick(md: &str, needle: &str) -> String {
    let (t, _, slice) = render(md);
    let a = slice.find(needle).expect("needle in buffer slice");
    let a = slice[..a].chars().count() as i32;
    let b = a + needle.chars().count() as i32;
    resolve(&t, md, a, b)
}

fn whole(md: &str) -> String {
    let (t, _, _) = render(md);
    resolve(&t, md, 0, t.char_count)
}

#[test]
fn multi_line_blockquote_within_excludes_all_markers() {
    // Constraint A: within the quote, exclude its `>` delimiters — including the
    // CONTINUATION `> ` on line 2 (the copymap suppresses that inter-line gap).
    let md = "pre\n\n> a\n> b\n\npost";
    assert_eq!(pick(md, "a"), "a"); // line 1 fragment
    assert_eq!(pick(md, "b"), "b"); // line 2 fragment — no leaked `> `
    assert_eq!(pick(md, "a\nb"), "a\nb"); // whole quote body → still no markers
}

#[test]
fn multi_line_blockquote_partial_cross_out_has_no_empty_quoted_line() {
    // A selection that begins at an in-quote line break and spans OUT of the quote
    // must not emit a spurious leading empty quoted line (`> \n> b`): line-1's
    // marker is dropped, the user's selected break is kept, and the first real
    // content line carries its reconstructed `> `.
    let md = "> a\n> b\n\npost";
    assert_eq!(pick(md, "\nb\n\npost"), "\n> b\n\npost");
    // Starting exactly at line-2 content (no leading break) still reconstructs the
    // marker directly on that line.
    assert_eq!(pick(md, "b\n\npost"), "> b\n\npost");
}

#[test]
fn multi_line_blockquote_whole_document_reconstructs_markers() {
    // Whole-doc = Copy Document: every line keeps its `> ` (and blank `>` lines,
    // and nested `>>`).
    assert_eq!(whole("> a\n> b"), "> a\n> b");
    assert_eq!(whole("> a\n>\n> b"), "> a\n>\n> b"); // blank quote line
    assert_eq!(whole(">> deep\n>> quote"), ">> deep\n>> quote"); // nested
}

// ── L3: list items are char-precise, including nested and loose items ──────────

#[test]
fn flat_list_item_within_excludes_the_marker() {
    let md = "pre\n\n- one\n- two\n\npost";
    assert_eq!(pick(md, "one"), "one"); // within item text → no `- `
    assert_eq!(pick(md, "ne"), "ne");
}

#[test]
fn list_whole_document_preserves_markers_numbers_and_task_boxes() {
    assert_eq!(whole("- one\n- two"), "- one\n- two");
    assert_eq!(whole("1. one\n2. two"), "1. one\n2. two"); // source numbers kept
    assert_eq!(whole("- [ ] a\n- [x] b"), "- [ ] a\n- [x] b"); // task boxes kept
                                                               // Task item text alone is char-precise (checkbox anchor + marker excluded).
    assert_eq!(pick("x\n\n- [ ] task\n- [x] done", "task"), "task");
}

#[test]
fn nested_list_item_is_char_precise() {
    // A nested list item reconstructs char-precisely (formerly deferred L3, now
    // viable since markers moved to the gutter — ScrAP-118): its marker/indent live
    // in inter-sibling source gaps and are gap-gated exactly like a blockquote's.
    let md = "x\n\n- a\n  - nested\n- b";
    assert_eq!(pick(md, "nested"), "nested"); // within nested item → no marker
                                              // Crossing from the outer item's text INTO the nested item reconstructs the
                                              // nested marker with its indent lead-in, and no spurious trailing newline.
    assert_eq!(pick(md, "a\nnested"), "a\n  - nested");
    // Selecting the whole outer subtree reconstructs every marker at its level.
    assert_eq!(pick(md, "a\nnested\nb"), "- a\n  - nested\n- b");
    // Ordered nesting keeps the source numbers and continuation indent.
    assert_eq!(
        pick("1. one\n   1. sub\n2. two", "one\nsub"),
        "one\n   1. sub"
    );
}

#[test]
fn loose_list_item_paragraphs_stay_separated() {
    // A loose item (blank-line-separated paragraphs) reconstructs char-precisely:
    // within the item the marker is excluded, but the structural blank line
    // between the two paragraphs survives (not collapsed onto one line).
    let md = "- top\n\n  loose para\n- next";
    assert_eq!(pick(md, "loose para"), "loose para"); // within one paragraph
    assert_eq!(pick(md, "top\n\nloose para"), "top\n\nloose para");
    // Whole document reconstructs the item marker and the loose indent.
    assert_eq!(whole(md), md);
}

// ── build-time drift guard ────────────────────────────────────────────────────

#[cfg(debug_assertions)]
#[test]
fn debug_verify_walks_a_consistent_render_without_panicking() {
    // Happy path: 1:1 leaves match the buffer the (simulated) renderer produced.
    let (t, md, slice) = render("a **bold** b");
    debug_verify(&t, &md, &slice.chars().collect::<Vec<_>>());
    // A non-1:1 (entity) leaf is skipped by the guard, not falsely flagged.
    let (t2, md2, slice2) = render("x &amp; y");
    debug_verify(&t2, &md2, &slice2.chars().collect::<Vec<_>>());
}

#[cfg(debug_assertions)]
#[test]
fn debug_verify_stays_aligned_when_an_anchor_precedes_text() {
    // Regression (live crash on a doc with tables): an anchored child (table)
    // occupies one U+FFFC in the buffer's *slice* but is OMITTED from its *text*.
    // The copymap capture is char_count-/slice-based (anchor = 1 char), so the
    // buffer passed to `debug_verify` MUST be the slice — otherwise every 1:1
    // leaf after an anchor is off by one char per preceding anchor. Two tables +
    // trailing prose reproduces the accumulating drift.
    let md = "before\n\n| a | b |\n|---|---|\n| c | d |\n\nmid text\n\n\
              | e | f |\n|---|---|\n| g | h |\n\nafter table text";
    let (t, md_s, slice) = render(md);
    debug_verify(&t, &md_s, &slice.chars().collect::<Vec<_>>()); // must not panic
                                                                 // And the copy of the trailing prose (past both table anchors) is exact:
    let start = slice.find("after table text").unwrap();
    let start = slice[..start].chars().count() as i32;
    let end = start + "after table text".chars().count() as i32;
    assert_eq!(resolve(&t, &md_s, start, end), "after table text");
}

#[cfg(debug_assertions)]
#[test]
fn debug_verify_passes_for_a_multi_line_list_item() {
    // With the flow-to-spaces workaround reverted, a list
    // item's source line break renders as a real '\n' in the buffer again (TDD 2.20) —
    // every line still sits at the uniform content margin, and the buffer text now
    // matches the source verbatim (no whitespace substitution), so the strict 1:1-leaf
    // guard must walk it cleanly and copy must stay byte-exact.
    let md = "- first line of the item\n  second source line of the same item\n- next";
    let (t, md_s, slice) = render(md);
    // The two source lines are separated by a real newline in the buffer now.
    assert!(
        slice.contains("item\nsecond"),
        "the in-item break renders as a newline in the sim buffer: {slice:?}"
    );
    debug_verify(&t, &md_s, &slice.chars().collect::<Vec<_>>()); // must not panic
                                                                 // Whole-document copy reconstructs the byte-exact source.
    let n = t.char_count;
    assert_eq!(resolve(&t, &md_s, 0, n), md);
    // A within-item selection that crosses the break copies the source text of that span
    // with the continuation indent suppressed (a within-block line marker).
    let a = slice.find("of the item").unwrap();
    let a = slice[..a].chars().count() as i32;
    let b = a + "of the item\nsecond".chars().count() as i32;
    assert_eq!(resolve(&t, &md_s, a, b), "of the item\nsecond");
}

// ── empty / degenerate selections never panic ─────────────────────────────────

#[test]
fn empty_and_out_of_range_selections_are_safe() {
    let (t, md) = tree("hello **world**");
    assert_eq!(resolve(&t, &md, 3, 3), ""); // empty selection
    let _ = resolve(&t, &md, 0, 9999); // over-range end must not panic
    let _ = resolve(&t, &md, -5, 2); // negative start must not panic
}

// ── wrap_span: the annotation OUTER (balanced) source range ────────────────────

/// Char offset of `sub` in the buffer `text` (the wrap-span selection basis).
fn woff(text: &str, sub: &str) -> i32 {
    text[..text.find(sub).expect("substring in buffer")]
        .chars()
        .count() as i32
}

#[test]
fn wrap_span_plain_selection_is_exact() {
    let (t, md, text) = render("just some plain words here.");
    let a = woff(&text, "some");
    let b = woff(&text, "words") + 5; // end of "words"
    let span = wrap_span(&t, &md, a, b).unwrap();
    assert_eq!(&md[span], "some plain words");
}

#[test]
fn wrap_span_touching_bold_extends_to_the_delimiters() {
    // Selecting part of a bold word includes the WHOLE `**bold**` (no split).
    let (t, md, text) = render("a **bold** tail.");
    let a = woff(&text, "bold");
    let span = wrap_span(&t, &md, a, a + 3).unwrap(); // "bol"
    assert_eq!(&md[span], "**bold**");
}

#[test]
fn wrap_span_touching_code_includes_the_backticks() {
    let (t, md, text) = render("run `the code` now.");
    let a = woff(&text, "the");
    let b = woff(&text, "code") + 4; // end of "code"
    let span = wrap_span(&t, &md, a, b).unwrap();
    assert_eq!(&md[span], "`the code`");
}

#[test]
fn wrap_span_across_code_and_bold_wraps_both_whole() {
    // The regression: a selection from plain text, through inline code, ending at
    // a bold word must yield ONE balanced span with the code and bold WHOLE — never
    // a `{==…==}` that splits a `` ` `` or `**`.
    let (t, md, text) = render("start with `code span` and **bold word** at end.");
    let a = woff(&text, "with");
    let b = woff(&text, "word") + 4; // end of "word"
    let span = wrap_span(&t, &md, a, b).unwrap();
    assert_eq!(&md[span], "with `code span` and **bold word**");
}

#[test]
fn wrap_span_inside_an_in_crate_construct_includes_its_markers() {
    // The PREVIEW path's half of the "never split an inline construct" rule, for the
    // four constructs pulldown-cmark does not parse (`==mark==`, `~~strike~~`,
    // `^sup^`, `~sub~` — tokenised by `renderer::scan_script_spans`). This path
    // resolves against the copymap, which models the stripped markers as inline
    // nodes, so it balances them without consulting pulldown — MEASURED here rather
    // than assumed, because the editor path with the same contract did not
    // (ScrAP-195). Selecting inside the content must yield the whole construct.
    for (src, sub, want) in [
        ("a ==mark== b", "ar", "==mark=="),
        ("a ~~strike~~ b", "rik", "~~strike~~"),
        ("a ^sup^ b", "u", "^sup^"),
        ("a ~sub~ b", "u", "~sub~"),
    ] {
        let (t, md, text) = render(src);
        let a = woff(&text, sub);
        let span = wrap_span(&t, &md, a, a + sub.chars().count() as i32)
            .unwrap_or_else(|| panic!("no span for {src:?}"));
        assert_eq!(&md[span], want, "preview balance for {src:?}");
    }
}

// ── input-limit regressions (QA round 3, D-1) ─────────────────────────────────

/// A pathologically nested document must not abort the process.
///
/// `Builder::construct` recursed once per nesting level with no bound. Measured
/// before the fix: 1050 levels build, **1100 levels overflow the stack** on a
/// 2 MiB thread — so a ~1.1 KiB file, small enough to paste into a chat message,
/// killed the process and every unsaved buffer in every window with it. A stack
/// overflow is not a catchable panic; `catch_unwind` would not have helped, and
/// there is none on the app path anyway.
///
/// `pulldown_cmark` is NOT the problem and must not be blamed for it: measured
/// separately, it parses 20 000 nesting levels into 40 003 events without
/// trouble, because it is iterative. The recursion was entirely ours.
///
/// 5000 levels here is ~5× past the pre-fix overflow point, so this test could
/// not have passed before [`limits::MAX_NEST_DEPTH`] existed.
///
/// Note for whoever mutation-tests this: removing the cap makes the test process
/// ABORT rather than fail, which is the finding rather than a flaw in the test.
#[test]
fn a_pathologically_nested_document_does_not_overflow_the_stack() {
    let md = format!("{} deep\n", ">".repeat(5000));
    let (t, md_owned) = tree(&md);
    // Reached at all = the build did not abort. Also assert the tree still
    // covers the buffer, so a cap that silently produced an empty tree would
    // not read as success.
    assert!(
        t.char_count > 0,
        "the capped build produced an empty tree — degradation should be in \
         copy PRECISION, not in coverage"
    );
    // Copy over the whole buffer still reproduces the whole source: the deeply
    // nested tail became one opaque node, and an opaque node reproduces its
    // source verbatim. Losing precision inside is the accepted cost; losing the
    // text is not.
    assert_eq!(
        resolve(&t, &md_owned, 0, t.char_count),
        md_owned,
        "whole-buffer copy must still equal the whole source past the cap"
    );
}

/// A long run of backslash escapes builds and round-trips.
///
/// **Coverage, not a regression guard, and labelled so deliberately.** It passes
/// on the recursive `text_nodes` too — measured, not assumed: 100 000 escapes
/// build fine either way, because pulldown splits the Text run at every escape
/// and the recursion therefore never went deep. Presenting it as the guard for
/// that rewrite would be #209's shape, an assertion that cannot fail dressed as
/// one that could.
///
/// What it does buy is real: nothing previously exercised more than a couple of
/// escapes, and the peel is the one place in this file that walks a Text run by
/// hand. If pulldown ever stops splitting at escapes, the loop absorbs it and
/// this test says the output is still right.
#[test]
fn a_long_escape_run_builds_and_round_trips() {
    let md = format!("{}\n", "\\*".repeat(10_000));
    let (t, md_owned) = tree(&md);
    assert_eq!(
        t.char_count, 10_000,
        "each `\\*` renders as one glyph, so the buffer is one char per escape"
    );
    assert_eq!(
        resolve(&t, &md_owned, 0, t.char_count),
        md_owned,
        "whole-buffer copy must reproduce every escaping backslash"
    );
    // A selection over the bare escaped char copies the backslash with it — the
    // atomicity the peel exists for, still holding at the far end of a long run.
    assert_eq!(resolve(&t, &md_owned, 9_999, 10_000), "\\*");
}

/// The cap must not perturb documents nobody would call pathological. Ordinary
/// nesting is far below [`limits::MAX_NEST_DEPTH`], so char-precise copy is
/// unchanged — this pins that the fix is inert on real input rather than
/// quietly coarsening it.
#[test]
fn ordinary_nesting_is_unaffected_by_the_depth_cap() {
    let md = "> - **bold** in a *nested* [link](http://e.test) item\n";
    let (t, md_owned, text) = render(md);
    let a = woff(&text, "bold");
    assert_eq!(resolve(&t, &md_owned, a, a + 4), "bold");
    assert_eq!(resolve(&t, &md_owned, 0, t.char_count), md_owned);
}
