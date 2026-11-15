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
//! And its two siblings in the same Win32 font map: a letter's **combining marks** draw
//! on the letter rather than beside it under a Georgia-led theme (TDD 25.18a), and the
//! coverage probe both that and the disclosure glyph ask resolves the face the preview
//! actually draws (`pangospan::lead_face`).
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

/// A body paragraph with three clusters, and a table cell with two. Georgia has U+0301
/// and lacks U+0303 and U+0323, so on Windows the first two must move and `é` must not.
const MARKS_MD: &str =
    "Espan\u{303}a, cafe\u{301}, q\u{323}.\n\n| n\u{303}o |\n|---|\n| q\u{323} |\n";

/// **A combining mark draws on its letter in the preview, under a Georgia-led theme**
/// (TDD 25.18a, the preview half).
///
/// On Windows Sepia's stack leads with Georgia, which lacks U+0303, and Pango's alias
/// table took the tilde to MS Gothic in a run of its own — HarfBuzz cannot place a mark
/// against a letter in another run, so `n` + U+0303 drew a tilde beside the `n`. The
/// oracle is the RUN: each cluster, set in the family the view sets it in (the
/// highest-priority tag naming one, else the view's own CSS face), shapes as ONE run with
/// no unknown glyph. GTK does not hand out a text view's own layout, so this lays the
/// cluster out on the view's context exactly as the view would; a table cell is a label,
/// which does hand out the layout it draws.
///
/// And the text is untouched: the buffer holds `n` + U+0303, never `ñ`, and a letter
/// whose marks the page's face has (`é` as `e` + U+0301) keeps the page's face.
///
/// Mutation: `pangospan::mark_cover` answering `None` fails this on Windows, with U+0303
/// alone in an MS Gothic run. Linux and macOS name no cover face, so this holds there on
/// their own font maps or reports a defect of the same shape.
#[gtktest::test]
fn a_combining_mark_draws_on_its_letter_under_a_georgia_led_theme() {
    let theme = crate::theme::Themes::builtin().resolve("sepia");
    let _active = crate::theme::activate_for_test(theme.clone());
    let _sheet = ThemeSheet::install(&theme);
    let (window, view) = styled_preview(MARKS_MD);
    let page = view_family(&view);
    let buf = view.buffer();
    let text = buf
        .slice(&buf.start_iter(), &buf.end_iter(), true)
        .to_string();
    assert!(
        text.contains("Espan\u{303}a") && !text.contains('ñ'),
        "the preview's text must keep `n` + U+0303 as written (TDD 25.18): {text:?}"
    );

    let mut split: Vec<String> = Vec::new();
    for cluster in ["n\u{303}", "q\u{323}", "e\u{301}"] {
        let byte = text
            .find(cluster)
            .unwrap_or_else(|| panic!("{cluster:?} is not in the preview's buffer"));
        let letter = text[..byte].chars().count() as i32;
        let (on_letter, on_mark) = (
            family_at(&buf.iter_at_offset(letter), &page),
            family_at(&buf.iter_at_offset(letter + 1), &page),
        );
        if on_letter != on_mark {
            split.push(format!(
                "{cluster:?}: letter in {on_letter:?}, mark in {on_mark:?}"
            ));
            continue;
        }
        let layout = pango::Layout::new(&view.pango_context());
        let mut desc = view.pango_context().font_description().unwrap_or_default();
        desc.set_family(&on_letter);
        layout.set_font_description(Some(&desc));
        layout.set_text(cluster);
        let faces = run_faces(&layout);
        if faces.len() != 1 || unknown_glyphs(&layout) > 0 {
            split.push(format!("{cluster:?} in {on_letter:?}: runs {faces:?}"));
        }
    }
    // The control: a letter whose marks the page's face draws stays in the page's face.
    let cafe = text.find("e\u{301}").expect("the fixture's é");
    let cafe = buf.iter_at_offset(text[..cafe].chars().count() as i32);
    assert_eq!(
        family_at(&cafe, &page),
        page,
        "`e` + U+0301 is drawn whole by the page's own face, so it must keep that face"
    );

    let labels = crate::preview::cell_search_targets(&view);
    for (_, cell) in labels.iter().filter(|(_, l)| l.text().chars().any(is_mark)) {
        let starts = run_texts(&cell.layout());
        if starts
            .iter()
            .any(|run| run.chars().next().is_some_and(is_mark))
        {
            split.push(format!("the table cell {:?}: runs {starts:?}", cell.text()));
        }
    }
    window.destroy();
    assert!(
        split.is_empty(),
        "a combining mark was shaped apart from its letter (page face {page:?}): {split:?}"
    );
}

/// **The coverage probe answers for the face the preview draws, under every built-in
/// theme that names a stack** — and never for `Sans`.
///
/// `pangospan::lead_face` is what the disclosure glyph and the combining-mark cover both
/// ask "does this run's face draw that?". It used to be handed the stack spelled with a
/// space after each comma; Pango does not trim the names it splits out, so on Windows
/// every family but the first missed, the load fell back to `Sans` (Tahoma) with a
/// `couldn't load font "<stack> 12", falling back to "Sans 12"` warning, and the probe
/// answered for Tahoma while the preview drew Georgia. The ground truth here is the face
/// the preview's own context — styled by the theme sheet, whose CSS stack GTK hands Pango
/// joined by a bare comma — lays a letter out in.
///
/// Mutation: restoring the `", "` join in `CssSafeFontStack::pango_family` fails this on
/// Windows for Sepia (Tahoma against Georgia).
#[gtktest::test]
fn the_coverage_probe_resolves_the_face_the_preview_draws() {
    use gtk::pango::prelude::FontExt;
    let themes = crate::theme::Themes::builtin();
    let probe_ctx = crate::pangospan::widget_font_context();
    let mut wrong: Vec<String> = Vec::new();
    let mut asked = 0;
    for entry in themes.chooser_list() {
        let theme = themes.resolve(&entry.id);
        let Some(stack) = theme.font_family.as_ref() else {
            continue;
        };
        asked += 1;
        let _active = crate::theme::activate_for_test(theme.clone());
        let _sheet = ThemeSheet::install(&theme);
        let (window, view) = styled_preview("n\n");
        let layout = pango::Layout::new(&view.pango_context());
        layout.set_text("n");
        let drawn = run_faces(&layout).into_iter().next().unwrap_or_default();
        window.destroy();
        let probed = crate::pangospan::lead_face(&probe_ctx, &stack.pango_family())
            .and_then(|face| face.describe().family())
            .map(|family| family.to_string())
            .unwrap_or_default();
        if probed != drawn {
            wrong.push(format!(
                "{}: the preview draws {drawn:?}, the probe resolved {probed:?} from {:?}",
                entry.id,
                stack.pango_family()
            ));
        }
    }
    assert!(
        asked > 0,
        "no built-in theme names a font stack, so nothing was asked"
    );
    assert!(
        wrong.is_empty(),
        "the coverage probe answered for the wrong face: {wrong:?}"
    );
}

/// The theme's generated sheet on the display, as `app::setup` installs it app-wide —
/// which a render alone does not, so a view in a test is otherwise styled in GTK's
/// default face. Removed when dropped, even on a panic: a display provider is
/// process-global and libtest runs the suite in one process (POLICY § Unit tests).
struct ThemeSheet(gtk::CssProvider);

impl ThemeSheet {
    /// At the app's own rung, `APPLICATION + 1` (`app::setup::theme_provider`).
    fn install(theme: &crate::theme::Theme) -> Self {
        let display = gtk::gdk::Display::default().expect("this test needs a display");
        let provider = gtk::CssProvider::new();
        provider.load_from_data(&crate::preview::theme_css(
            theme,
            &crate::palette::Palette::for_theme(theme),
        ));
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
        Self(provider)
    }
}

impl Drop for ThemeSheet {
    fn drop(&mut self) {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_remove_provider_for_display(&display, &self.0);
        }
    }
}

/// `md` rendered under the active theme and presented, so the view is styled — and its
/// Pango context carries the face — exactly as the app's would be.
fn styled_preview(md: &str) -> (gtk::Window, crate::codeview::CodePreviewView) {
    let pane = crate::preview::render(
        None,
        md,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = crate::preview::view_of(&pane).expect("the preview tree render() built");
    let window = gtk::Window::new();
    window.set_default_size(500, 300);
    window.set_child(Some(&pane));
    window.present();
    crate::testpump::until(crate::testpump::Clock::Frame, "the preview maps", || {
        view.is_mapped()
    });
    (window, view)
}

/// The family the view's own context lays its prose out in: the theme sheet's stack,
/// as GTK spells it for Pango.
fn view_family(view: &crate::codeview::CodePreviewView) -> String {
    view.pango_context()
        .font_description()
        .and_then(|desc| desc.family())
        .map(|family| family.to_string())
        .expect("a styled view names a family")
}

/// The family the view sets the character at `iter` in: the highest-priority tag there
/// that names one — tags come back in ascending priority — else the page's.
fn family_at(iter: &gtk::TextIter, page: &str) -> String {
    iter.tags()
        .iter()
        .rev()
        .find(|t| t.is_family_set())
        .and_then(|t| t.family())
        .map_or_else(|| page.to_string(), |family| family.to_string())
}

/// The face of each of `layout`'s runs, in order.
fn run_faces(layout: &pango::Layout) -> Vec<String> {
    use gtk::pango::prelude::FontExt;
    layout
        .lines_readonly()
        .iter()
        .flat_map(|line| line.runs())
        .map(|run| {
            run.item()
                .analysis()
                .font()
                .describe()
                .family()
                .map(|family| family.to_string())
                .unwrap_or_default()
        })
        .collect()
}

/// The text of each of `layout`'s runs, in order.
fn run_texts(layout: &pango::Layout) -> Vec<String> {
    let text = layout.text();
    layout
        .lines_readonly()
        .iter()
        .flat_map(|line| line.runs())
        .map(|run| {
            let item = run.item();
            let start = usize::try_from(item.offset()).unwrap_or(0);
            let end = start + usize::try_from(item.length()).unwrap_or(0);
            text.get(start..end).unwrap_or_default().to_string()
        })
        .collect()
}

/// Whether `c` is a combining mark — GLib's own answer, the one Pango itemizes by.
fn is_mark(c: char) -> bool {
    // SAFETY: a pure lookup in GLib's Unicode tables, defined for every code point.
    unsafe { gtk::glib::ffi::g_unichar_ismark(u32::from(c)) != 0 }
}
