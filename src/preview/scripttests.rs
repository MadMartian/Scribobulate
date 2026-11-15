//! **Code in the preview draws Hangul in a face that has it** — the preview half of the
//! PDF's script fallback (TDD 25.18, and 25.3: the artefact looks as the preview does,
//! so the preview must not be the surface drawing boxes).
//!
//! On Windows the preview's prose drew Korean in Malgun Gothic while a fenced code block
//! under it drew hex boxes: the code tag names `monospace`, which replaces the page's
//! family, and Pango's Win32 alias for `monospace` names no Korean face a stock install
//! has (`platform::win32::script_fallback`). Inline code and a table cell's code take
//! the same face from `pangospan`, so all three are asked here.
//!
//! Test-only, and carrying `gtk-integration-tests` for the reason
//! `preview::altsuppression` gives: a render needs a `GtkTextBuffer`. Its own module
//! because `render.rs` and `build.rs` are past the file-size soft limit.

use gtk::pango;
use gtk::prelude::*;

/// Inline code in prose, a fenced block, and inline code in a table cell — each the
/// only Korean in its construct, so a word names its surface.
const MD: &str =
    "Prose with `인라인` code.\n\n```\n블록 코드\n```\n\n| head |\n|---|\n| `셀코드` |\n";

#[gtktest::test]
fn hangul_in_preview_code_draws_in_a_face_that_has_its_glyphs() {
    if !crate::pangospan::host_has_a_face_for('한') {
        println!("SKIPPED [TDD 25.18]: no installed face on this host draws Hangul");
        return;
    }
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = crate::preview::view_of(&pane).expect("the preview tree render() built");
    let buf = view.buffer();
    // `slice`, which keeps a U+FFFC per anchored child, so a char count into it is a
    // buffer offset (ScrAP-74).
    let text = buf
        .slice(&buf.start_iter(), &buf.end_iter(), true)
        .to_string();

    // The buffer's two code surfaces. GTK does not hand out a text view's own layout, so
    // the face is the one the view would merge: its own context, with the family of the
    // highest-priority tag on the word that sets one — which must be a code tag's.
    // Every surface is asked before anything is asserted, so a failure names them all.
    let mut boxed: Vec<String> = Vec::new();
    for word in ["인라인", "블록 코드"] {
        let byte = text
            .find(word)
            .unwrap_or_else(|| panic!("{word} is not in the preview's buffer"));
        let at = buf.iter_at_offset(text[..byte].chars().count() as i32);
        let family = at
            .tags()
            .iter()
            .rev()
            .find(|t| t.is_family_set())
            .and_then(|t| t.family())
            .unwrap_or_else(|| panic!("no tag on {word} sets a code face"));
        let layout = pango::Layout::new(&view.pango_context());
        let mut desc = view.pango_context().font_description().unwrap_or_default();
        desc.set_family(&family);
        if desc.size() <= 0 {
            desc.set_size(12 * pango::SCALE);
        }
        layout.set_font_description(Some(&desc));
        layout.set_text(word);
        if unknown_glyphs(&layout) > 0 {
            boxed.push(format!("{word} in the code face {family:?}"));
        }
    }

    // The table cell is a label outside the buffer, and a label DOES hand out its own
    // layout — the one it draws.
    let labels = crate::preview::cell_search_targets(&view);
    let (_, cell) = labels
        .iter()
        .find(|(_, l)| l.text().contains("셀코드"))
        .expect("the table cell holding the inline code");
    if unknown_glyphs(&cell.layout()) > 0 {
        boxed.push(format!("the table cell {}", cell.label()));
    }
    assert!(boxed.is_empty(), "code drawn as hex boxes: {boxed:?}");
}

/// How many of `layout`'s glyphs are Pango's unknown-glyph box — the tofu a reader sees.
fn unknown_glyphs(layout: &pango::Layout) -> usize {
    layout
        .lines_readonly()
        .iter()
        .flat_map(|line| line.runs())
        .map(|run| {
            run.glyph_string()
                .glyph_info()
                .iter()
                .filter(|g| g.glyph() & pango::GLYPH_UNKNOWN_FLAG != 0)
                .count()
        })
        .sum()
}
