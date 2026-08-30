use super::*;
use pulldown_cmark::Parser;

// ── faithful render simulation (real pulldown offsets + mirrored buffer math) ──
//
// The resolver depends on two facts per event: its *source* byte range (from
// pulldown, taken verbatim here) and the *buffer* char range the renderer
// produced. This sim mirrors `renderer.rs`'s buffer arithmetic (block_sep
// idempotence, heading trailing newline, tight-construct marker stripping) for the
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
///
/// `code` is the accumulated code-block body: inside a code block `Text` inserts
/// nothing and is buffered here, then flushed at `End(CodeBlock)` exactly as
/// `renderer::emit::insert_code_block` does (trailing blank lines trimmed, one
/// `\n` per line). That fidelity is the point — the copymap has to re-derive the
/// block's buffer layout from that rule (ScrAP-255), so a stand-in body would test
/// nothing.
fn apply(
    sim: &mut Sim,
    ev: &Event,
    code: &mut Option<String>,
    opaque_depth: &mut i32,
    scripts: &crate::renderer::BlockScripts,
    src: &std::ops::Range<usize>,
) {
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
            *code = Some(String::new());
        }
        Event::End(TagEnd::CodeBlock) => {
            let body = code.take().unwrap_or_default();
            sim.insert(&format!("{}\n", body.trim_end_matches('\n')));
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
            if let Some(body) = code.as_mut() {
                body.push_str(t); // accumulated, flushed at End(CodeBlock)
            } else if *opaque_depth > 0 {
                // suppressed image alt text
            } else {
                for seg in scripts.segments(src.start, t) {
                    if !seg.marker {
                        sim.insert(seg.text(t));
                    }
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
    let (mut code, mut opaque_depth) = (None, 0);
    let mut evs = Vec::new();
    // Streamed: no list-item look-ahead is needed — markers are drawn in the gutter and
    // insert no buffer text, so there is nothing to suppress (mirrors `preview::build`).
    let scripts = std::rc::Rc::new(crate::renderer::BlockScripts::scan(md));
    for (ev, r) in Parser::new_ext(md, crate::renderer::md_options()).into_offset_iter() {
        let before = sim.count;
        apply(&mut sim, &ev, &mut code, &mut opaque_depth, &scripts, &r);
        let after = sim.count;
        if let Some(kind) = classify(&ev) {
            evs.push(RawEv {
                buf: (before, after),
                src: r,
                kind,
            });
        }
    }
    (
        build(md, &evs, sim.count, &scripts),
        md.to_string(),
        sim.text,
    )
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

/// A fence that WRAPS other inline markup is one construct across three events.
///
/// pulldown splits `~~a **bold** b~~` into `"~~a "`, `Strong("bold")`, `" b~~"`, so
/// the copymap sees the opening `~~` and the closing `~~` in different events with
/// the Strong's own nodes between them. The markers are stripped from the buffer,
/// so this map is what has to put them back — and each half is reconstructed from
/// the event that owns it.
#[test]
fn a_fence_wrapping_inline_markup_round_trips() {
    let (t, md, text) = render("~~a **bold** b~~ tail");
    assert_eq!(text, "a bold b tail", "the delimiters must not render");

    // The whole struck run, selected exactly ("a bold b" = chars 0..8): every
    // delimiter comes back, both halves of the fence included.
    assert_eq!(resolve(&t, &md, 0, 8), "~~a **bold** b~~");

    // Selecting inside the fence's leading half, without crossing out of it,
    // clips to the content — exactly as an outer `**` would.
    assert_eq!(resolve(&t, &md, 0, 1), "a");
    // Crossing out of that half reconstructs the `~~` it owns, and clips the
    // Strong it crossed into (2.8b: a crossed pair is always closed).
    assert_eq!(resolve(&t, &md, 0, 3), "~~a **b**");

    // The whole buffer is Copy Document — byte-identical source.
    assert_eq!(resolve(&t, &md, 0, t.char_count), md);
}

/// The annotation wrap span must take such a fence WHOLE.
///
/// Half of it is not a construct: `{==` landing between the `~~` halves produces
/// markup that parses as neither. The tree alone cannot see the whole (the two
/// halves are separate nodes), so `wrap_span` widens through the construct table.
#[test]
fn wrap_span_takes_a_markup_wrapping_fence_whole() {
    for (src, sub, want) in [
        ("~~a **bold** b~~ tail", "old", "~~a **bold** b~~"),
        ("~~a **bold** b~~ tail", "a", "~~a **bold** b~~"),
        ("==a *em* b== tail", "em", "==a *em* b=="),
    ] {
        let (t, md, text) = render(src);
        let a = woff(&text, sub);
        let span = wrap_span(&t, &md, a, a + sub.chars().count() as i32)
            .unwrap_or_else(|| panic!("no span for {src:?}"));
        assert_eq!(&md[span], want, "preview balance for {src:?}");
    }
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
    let scripts = std::rc::Rc::new(crate::renderer::BlockScripts::scan(md));
    for (ev, src) in Parser::new_ext(md, crate::renderer::md_options()).into_offset_iter() {
        let kind = classify(&ev);
        match &ev {
            Event::Start(Tag::TableCell) => {
                active = true;
                evs.clear();
                off = 0;
            }
            Event::End(TagEnd::TableCell) => {
                maps.push(build(md, &evs, off, &scripts));
                active = false;
            }
            _ if active => {
                if let Some(k) = &kind {
                    let w = cell_width(&scripts, src.start, k);
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

/// The EDITOR path's half of the same rule (Document Rendering CAM row 3).
///
/// `wrap_span` above resolves against the copymap; this resolves against the
/// source, and the two balance through different code — which is exactly how
/// `~~`/`==` came to be handled on one path and not the other (ScrAP-195).
#[test]
fn balance_source_span_takes_a_markup_wrapping_fence_whole() {
    for (src, sub) in [
        ("~~a **bold** b~~ tail", "bold"),
        ("~~a **bold** b~~ tail", "a "),
        ("==a *em* b== tail", "em"),
    ] {
        let at = src.find(sub).expect("fixture");
        let normalized = crate::renderer::NormalizedMd::new(src);
        let got = balance_source_span(&normalized, at..at + sub.len());
        let want = &src[..src.rfind(' ').expect("fixture")];
        assert_eq!(&src[got], want, "editor balance for {src:?} at {sub:?}");
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

// ── code blocks: char-precise inside, fenced when crossed (ScrAP-255) ──────────
//
// A code block's body is buffered in ONE flush at its `End` event, so its
// interior events carry zero-width buffer ranges and it *looked* unreconstructable
// — it was `Node::Opaque`, and a two-word selection inside a 50-line block copied
// all 50 lines plus both fences. These pin the re-derived layout: within the body
// is char-precise (2.8a), crossing out of it reconstructs BOTH fences (2.8b), and
// anything the layout cannot account for degrades to the whole block (2.8e).

#[test]
fn within_a_code_block_excludes_the_fences() {
    // The reported bug: selecting part of a code block copied the whole block.
    assert_eq!(
        copy(
            "```rust\nlet a = 1;\nlet b = 2;\n```\n",
            "let a = 1;\nlet b = 2;\n",
            "a = 1"
        ),
        "a = 1"
    );
}

#[test]
fn a_whole_line_of_a_code_block_copies_that_line_only() {
    assert_eq!(
        copy(
            "```rust\nlet a = 1;\nlet b = 2;\n```\n",
            "let a = 1;\nlet b = 2;\n",
            "let b = 2;"
        ),
        "let b = 2;"
    );
}

#[test]
fn selecting_a_code_blocks_whole_body_still_excludes_the_fences() {
    // Exactly the content range is INSIDE the construct (2.8a), like selecting the
    // four letters of `**bold**`: no boundary is crossed, so no fence is emitted.
    // (Wrapped in prose so the selection is not the WHOLE buffer, which is Copy
    // Document by definition — 2.8c.)
    let md = "intro\n\n```rust\nlet a = 1;\nlet b = 2;\n```\n\nafter\n";
    let (t, md_owned, text) = render(md);
    let a = woff(&text, "let a");
    assert_eq!(
        resolve(
            &t,
            &md_owned,
            a,
            a + "let a = 1;\nlet b = 2;\n".chars().count() as i32
        ),
        "let a = 1;\nlet b = 2;\n"
    );
}

#[test]
fn crossing_into_a_code_block_reconstructs_both_fences() {
    // The fences are a matched PAIR: a selection that starts outside the block and
    // stops mid-body must still close the fence, or the paste is unparseable
    // Markdown (2.8b / 2.8e).
    let md = "intro\n\n```rust\nlet a = 1;\nlet b = 2;\n```\n";
    let (t, md_owned, text) = render(md);
    let a = woff(&text, "ntro");
    let b = woff(&text, " = 1");
    assert_eq!(resolve(&t, &md_owned, a, b), "ntro\n\n```rust\nlet a\n```");
}

#[test]
fn crossing_out_of_a_code_block_reconstructs_both_fences() {
    let md = "```rust\nlet a = 1;\nlet b = 2;\n```\n\nafter\n";
    let (t, md_owned, text) = render(md);
    let a = woff(&text, "b = 2");
    let b = woff(&text, "after") + 3;
    assert_eq!(resolve(&t, &md_owned, a, b), "```rust\nb = 2;\n```\n\naft");
}

#[test]
fn within_an_indented_code_block_keeps_the_continuation_indent() {
    // An indented block has no fences (empty open/close); each line's own source
    // excludes the 4-space indent, which lives in the inter-run GAP and is spliced
    // back so the copy re-parses as the same code block.
    let md = "intro\n\n    indented one\n    indented two\n\nafter\n";
    let (t, md_owned, text) = render(md);
    let a = woff(&text, "indented one");
    assert_eq!(
        resolve(
            &t,
            &md_owned,
            a,
            // BUFFER chars: the indent is not in the buffer, only in the source.
            a + "indented one\nindented two".chars().count() as i32
        ),
        "indented one\n    indented two"
    );
    // A fragment of ONE line is exactly that fragment.
    assert_eq!(copy(md, &text, "dented t"), "dented t");
}

#[test]
fn within_a_quoted_code_block_excludes_the_quote_markers() {
    // Inside an un-crossed blockquote the per-line `> ` is suppressed (2.8g) — the
    // code block's own children inherit that gate, so a two-line body copies bare.
    let md = "> ```\n> quoted one\n> quoted two\n> ```\n";
    let (t, md_owned, text) = render(md);
    let a = woff(&text, "quoted one");
    assert_eq!(
        resolve(
            &t,
            &md_owned,
            a,
            a + "quoted one\nquoted two".chars().count() as i32
        ),
        "quoted one\nquoted two"
    );
}

#[test]
fn a_code_block_in_a_list_item_is_char_precise() {
    let md = "- item\n\n  ```\n  in list\n  ```\n";
    let (t, md_owned, text) = render(md);
    let a = woff(&text, "in list");
    assert_eq!(resolve(&t, &md_owned, a, a + 7), "in list");
}

#[test]
fn a_code_block_whose_flush_cannot_be_accounted_for_stays_whole() {
    // The renderer TRIMS trailing blank lines, so this run's 18 rendered chars
    // become 16 buffer chars. The block's total still reconciles (so the block is
    // not opaque), but the run itself is not 1:1 — any overlap copies it whole,
    // the same atomicity guarantee an escape or an entity gets.
    let md = "```\ntrailing blanks\n\n\n```\n";
    let (t, md_owned, text) = render(md);
    let a = woff(&text, "blanks");
    assert_eq!(resolve(&t, &md_owned, a, a + 3), "trailing blanks\n\n\n");
}

#[test]
fn an_empty_code_block_is_opaque() {
    // No interior Text event at all: nothing to lay out, so the pre-existing
    // whole-source fallback stands — the block's one buffer newline copies as its
    // whole source.
    let md = "intro\n\n```\n```\n";
    let (t, md_owned, text) = render(md);
    let a = text.chars().count() as i32 - 1; // the empty block's sole buffer char
    assert_eq!(resolve(&t, &md_owned, a, a + 1), "```\n```");
}

#[test]
fn select_all_over_a_code_block_is_the_whole_document() {
    let md = "# Title\n\n```rust\nlet a = 1;\n```\n\nAfter.\n";
    let (t, md_owned, _text) = render(md);
    assert_eq!(resolve(&t, &md_owned, 0, t.char_count), md_owned);
}

#[test]
fn wrap_span_inside_a_code_block_takes_the_whole_fenced_block() {
    // The OTHER consumer of this tree: an annotation must never wrap a fragment of
    // a code block, or `{==` lands inside the fence and is rendered as code. Making
    // the block char-precise for COPY must not make it divisible for ANNOTATE.
    let md = "```rust\nlet a = 1;\nlet b = 2;\n```\n";
    let (t, md_owned, text) = render(md);
    let a = woff(&text, "a = 1");
    let span = wrap_span(&t, &md_owned, a, a + 5).unwrap();
    assert_eq!(&md_owned[span], "```rust\nlet a = 1;\nlet b = 2;\n```");
}

/// The alignment gate at its seam. The only way a code block's re-derived layout
/// can be *wrong* is if the renderer's flush stops matching `insert_code_block`'s
/// rule (e.g. a syntect highlight that yields no tokens drops a line's glyphs
/// entirely). No document can produce that today, so the gate cannot be reached
/// through `render()` — it is pinned here directly, with the aligned case beside
/// it so the check is proved to DISCRIMINATE rather than to always fall back.
#[test]
fn a_code_block_flush_that_does_not_reconcile_degrades_to_opaque() {
    let md = "```\nalpha\nbeta\n```\n";
    let body = "alpha\nbeta\n";
    let texts = vec![(4..15, body.to_string())];
    let start = RawEv {
        buf: (0, 0),
        src: 0..18,
        kind: RawKind::Start(Construct::CodeBlock),
    };
    let node_for = |flushed_chars: i32| {
        let end = RawEv {
            buf: (0, flushed_chars),
            src: 0..18,
            kind: RawKind::End(Construct::CodeBlock),
        };
        CopyTree {
            scripts: std::rc::Rc::new(crate::renderer::BlockScripts::default()),
            root: code_block_node(md, &start, &end, &texts, true),
            char_count: flushed_chars,
        }
    };
    // Aligned (11 body chars flushed): char-precise, as every other test asserts.
    assert_eq!(resolve(&node_for(11), md, 6, 10), "beta");
    // Short by five: the layout is unprovable, so the whole block copies.
    assert_eq!(resolve(&node_for(6), md, 1, 3), "```\nalpha\nbeta\n```");
}

/// A non-`Text` event inside a code block is likewise unmodelled — the same
/// fallback, from the other input.
#[test]
fn a_code_block_with_an_unmodelled_interior_event_degrades_to_opaque() {
    let md = "```\nalpha\nbeta\n```\n";
    let texts = vec![(4..15, "alpha\nbeta\n".to_string())];
    let start = RawEv {
        buf: (0, 0),
        src: 0..18,
        kind: RawKind::Start(Construct::CodeBlock),
    };
    let end = RawEv {
        buf: (0, 11),
        src: 0..18,
        kind: RawKind::End(Construct::CodeBlock),
    };
    let tree = CopyTree {
        scripts: std::rc::Rc::new(crate::renderer::BlockScripts::default()),
        root: code_block_node(md, &start, &end, &texts, false),
        char_count: 11,
    };
    assert_eq!(resolve(&tree, md, 6, 10), "```\nalpha\nbeta\n```");
}

// ── degenerate event streams: the "should not happen" branches ────────────────
//
// Every test above feeds the builder a stream a real pulldown parse produced. The
// builder also carries fallbacks for streams that are malformed or merely shaped in
// a way no fixture here happens to produce — a construct that never closes, a stray
// `End`, an empty construct, a break or an opaque unit at document level. Those
// branches decide what a COPY does when a document is unusual, and until now none of
// them had ever run: the code path a reader would reach for to explain a wrong copy
// was the one path with no evidence behind it.
//
// They are reachable directly, because `build` is pure and takes the classified
// stream as data. A hand-built stream is not a worse test than a parsed one here —
// it is the only way to express a stream a parser will not emit.

/// Build a copymap from a hand-written event stream, then copy `[a, b)` from it.
fn resolve_raw(md: &str, evs: &[RawEv], char_count: i32, a: i32, b: i32) -> String {
    resolve(
        &build(
            md,
            evs,
            char_count,
            &std::rc::Rc::new(crate::renderer::BlockScripts::scan(md)),
        ),
        md,
        a,
        b,
    )
}

fn ev(buf: (i32, i32), src: Range<usize>, kind: RawKind) -> RawEv {
    RawEv { buf, src, kind }
}

/// A `Break` and an opaque unit (a rule, a task-list checkbox) sitting at DOCUMENT
/// level rather than inside a paragraph. Both own buffer glyphs, so a selection over
/// them must still resolve to their own source rather than swallowing a neighbour's.
#[test]
fn a_break_and_an_opaque_unit_at_document_level_resolve_to_their_own_source() {
    // md: "a\n---\n" — text, break, rule, with no enclosing paragraph events.
    let md = "a\n---\n";
    let evs = [
        ev((0, 1), 0..1, RawKind::Text("a".into())),
        ev((1, 2), 1..2, RawKind::Break),
        ev((2, 3), 2..6, RawKind::Atomic),
    ];
    // The break alone.
    assert_eq!(resolve_raw(md, &evs, 3, 1, 2), "\n");
    // The opaque rule alone — opaque means its whole source, never a slice of it.
    assert_eq!(resolve_raw(md, &evs, 3, 2, 3), "---\n");
}

/// A stray `End` with no matching `Start` — at document level and inside a
/// construct. It must be skipped, leaving the surrounding content intact, rather
/// than terminating the walk early and truncating the copy.
#[test]
fn a_stray_end_event_is_skipped_and_does_not_truncate_the_copy() {
    let md = "ab";
    // Document level: End(Emphasis) between two text runs that never opened one.
    let evs = [
        ev((0, 1), 0..1, RawKind::Text("a".into())),
        ev((1, 1), 1..1, RawKind::End(Construct::Emphasis)),
        ev((1, 2), 1..2, RawKind::Text("b".into())),
    ];
    assert_eq!(
        resolve_raw(md, &evs, 2, 0, 2),
        md,
        "both runs survive the stray"
    );

    // Inside a construct: the stray must not close the paragraph early either.
    let evs = [
        ev((0, 0), 0..0, RawKind::Start(Construct::Paragraph)),
        ev((0, 1), 0..1, RawKind::Text("a".into())),
        ev((1, 1), 1..1, RawKind::End(Construct::Strong)),
        ev((1, 2), 1..2, RawKind::Text("b".into())),
        ev((2, 2), 2..2, RawKind::End(Construct::Paragraph)),
    ];
    assert_eq!(resolve_raw(md, &evs, 2, 0, 2), md);
}

/// A construct with no interior at all (`**bold**` with the text run missing, an
/// empty heading). There is no content to clip a selection against, so it falls back
/// to opaque — the whole construct's source, delimiters included — rather than
/// resolving to nothing.
#[test]
fn a_construct_with_no_interior_falls_back_to_opaque() {
    let md = "x****y";
    let evs = [
        ev((0, 1), 0..1, RawKind::Text("x".into())),
        ev((1, 1), 1..3, RawKind::Start(Construct::Strong)),
        ev((1, 1), 3..5, RawKind::End(Construct::Strong)),
        ev((1, 2), 5..6, RawKind::Text("y".into())),
    ];
    // Selecting across the empty construct yields its whole source, not "".
    assert_eq!(resolve_raw(md, &evs, 2, 0, 2), md);
}

/// A construct whose `End` never arrives — a truncated stream. The builder
/// substitutes the last event it saw as the close, which BOUNDS the walk: it
/// terminates, and everything outside the unclosed construct still resolves.
///
/// **Measured degenerate outcome, asserted as-is rather than as an aspiration:** the
/// unclosed construct's own interior resolves to nothing, because the substituted
/// close is the interior event itself, so the construct's content range collapses to
/// empty. That is a lossy answer, and it is acceptable only because a parser cannot
/// emit this stream — `pulldown-cmark` closes every tag it opens. It is pinned here
/// so the behaviour is a recorded fact rather than a surprise found while debugging a
/// wrong copy, and so that a future change which starts SALVAGING the interior fails
/// this test loudly rather than passing unnoticed.
#[test]
fn an_unclosed_construct_is_bounded_and_costs_only_its_own_interior() {
    let md = "a *b";
    let evs = [
        ev((0, 2), 0..2, RawKind::Text("a ".into())),
        ev((2, 2), 2..3, RawKind::Start(Construct::Emphasis)),
        ev((2, 3), 3..4, RawKind::Text("b".into())),
        // no End(Emphasis)
    ];
    assert_eq!(
        resolve_raw(md, &evs, 4, 0, 2),
        "a ",
        "content BEFORE the unclosed construct is unaffected"
    );
    assert_eq!(
        resolve_raw(md, &evs, 4, 2, 3),
        "",
        "the unclosed construct's interior is lost (see the doc comment)"
    );
}

/// A code block whose interior is not a plain text run (an emphasis event inside a
/// fence — impossible from a parse, possible from a stream) is treated as opaque:
/// a selection inside it copies the whole fence rather than a reconstruction that
/// would drop the fence markers.
#[test]
fn a_code_block_with_a_non_text_interior_is_opaque() {
    let md = "```\nab\n```";
    let evs = [
        ev((0, 0), 0..4, RawKind::Start(Construct::CodeBlock)),
        ev((0, 1), 4..5, RawKind::Text("a".into())),
        ev((1, 1), 5..5, RawKind::Start(Construct::Emphasis)),
        ev((1, 2), 5..6, RawKind::Text("b".into())),
        ev((2, 2), 6..6, RawKind::End(Construct::Emphasis)),
        ev((2, 3), 6..10, RawKind::End(Construct::CodeBlock)),
    ];
    assert_eq!(resolve_raw(md, &evs, 3, 1, 2), md);
}

/// An unclosed code block — the fence-close fallback, the code-block twin of the
/// unclosed-construct case above.
#[test]
fn an_unclosed_code_block_closes_at_the_last_event_it_saw() {
    let md = "```\nab";
    let evs = [
        ev((0, 0), 0..4, RawKind::Start(Construct::CodeBlock)),
        ev((0, 2), 4..6, RawKind::Text("ab".into())),
        // no End(CodeBlock)
    ];
    // char_count 3 (a trailing buffer newline the stream does not describe), so this
    // is a PARTIAL selection — not the whole-buffer shortcut, which would return the
    // source without consulting the tree at all and prove nothing.
    assert_eq!(resolve_raw(md, &evs, 3, 0, 2), md);
}

/// `wrap_span` (the annotation path, not the copy path) over an opaque unit and over
/// an atomic code span returns the WHOLE unit's source. Wrapping half of either in
/// `{==…==}` would split a fence or a rule.
#[test]
fn wrap_span_takes_an_opaque_or_atomic_unit_whole() {
    let md = "a `code` b";
    let evs = [
        ev((0, 2), 0..2, RawKind::Text("a ".into())),
        ev((2, 6), 2..8, RawKind::Code("code".into())),
        ev((6, 8), 8..10, RawKind::Text(" b".into())),
    ];
    let t = build(
        md,
        &evs,
        8,
        &std::rc::Rc::new(crate::renderer::BlockScripts::scan(md)),
    );
    // A selection of ONE character inside the code span still wraps the whole span,
    // backticks included.
    assert_eq!(super::wrap_span(&t, md, 3, 4), Some(2..8));

    let md2 = "a\n---\n";
    let evs2 = [
        ev((0, 1), 0..1, RawKind::Text("a".into())),
        ev((1, 2), 1..2, RawKind::Break),
        ev((2, 3), 2..6, RawKind::Atomic),
    ];
    let t2 = build(
        md2,
        &evs2,
        3,
        &std::rc::Rc::new(crate::renderer::BlockScripts::scan(md2)),
    );
    assert_eq!(super::wrap_span(&t2, md2, 2, 3), Some(2..6));
}
