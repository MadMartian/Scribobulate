//! **Every declared key reaches every surface it claims — one sweep over the registry,
//! not one test per key.**
//!
//! The registry closed the drift hole for *parsing*: an unknown key warns (TDD 18.33).
//! It left it wide open for *consumption*. Adding one key takes up to five coordinated
//! edits — `data/themes.toml`, `theme/keys.rs`, `theme/resolve.rs` + `theme/model.rs`,
//! the preview path, `export/html.rs`, `export/pdf/*` — and **nothing failed when one
//! was missed**. Only the sprite family had a completeness guard at all, covering 10 of
//! 69 keys.
//!
//! A key declared in `KEYS` but never read by a sink is **worse than an unknown key**:
//! `ThemeSpec::validate` admits it WITHOUT a warning, because `keys::lookup` claims it,
//! so it is accepted, SCHEMA-documented and completely inert with no log line at all.
//! Nothing asserts a key is *used*, so it was a completeness obligation on the author
//! until this sweep existed — and eleven keys duly reached two surfaces of three.
//!
//! # The observable, and what it is not
//!
//! For each key the sweep resolves two themes — one stating nothing, one stating that
//! key at a distinctive value — and asserts the surface's OUTPUT differs. "Differs" is
//! the general form: it needs no per-kind sentinel and it works identically for a
//! colour, a metric, a glyph and a sprite.
//!
//! | Surface | What is compared |
//! |---|---|
//! | HTML | the whole artefact `export::html::render` produces |
//! | PDF | the laid-out page: every line's markup, indent, height, fill and marker |
//! | preview | its generated CSS, its `GtkTextTag` set, and the resolved decorations `snapshot_layer` paints from |
//!
//! **The preview's third column is a stated division of labour, not a shortcut.**
//! `snapshot_layer` cannot be reached without a realized view, so this proves the key
//! reaches the value the painter is handed; that the painter then *draws* it is what
//! the paint tests in `codeview` and `export::pdf::measure` assert, with pixels. The
//! failure this sweep exists to catch is a key that reaches **nothing**, and that one
//! it catches.

use super::super::keys::{Kind, KEYS};
use super::super::{Theme, Themes, SYSTEM_ID};

/// A value distinctive enough that a shipped theme cannot be stating it already, in the
/// TOML spelling this key's type takes.
fn probe(kind: Kind) -> &'static str {
    match kind {
        // A colour nothing ships, in the spelling every colour key parses.
        Kind::Color => "\"#7f0e5a\"",
        Kind::Font => "\"Probe Face, monospace\"",
        Kind::Text => "\"probe-sentinel\"",
        Kind::Glyph => "\"\u{2739}\"",
        // Unreached by this sweep — see the skip in the loop below.
        Kind::Sprite => "\"sprites/copper-plate.png\"",
        Kind::Line => "\"double\"",
        // Far from every shipped value and inside every clamp range in the registry.
        Kind::Int => "97",
        Kind::Float => "3.5",
    }
}

/// A theme stating exactly one key, at [`probe`]'s value for its type.
///
/// The **bare** spelling deliberately: a levelled key's bare form applies to every
/// level, so one probe covers the family, and the per-level narrowing is what
/// `keys::tests` and TDD 18.32 pin separately.
fn stating(key: &super::super::keys::Key) -> Theme {
    let mut themes = Themes::builtin();
    themes.merge_over_for_test(&format!(
        "[themes.probe]\n{}{} = {}\n",
        key.reach.needs,
        key.name,
        probe(key.kind)
    ));
    themes.resolve("probe")
}

/// The baseline: a theme stating nothing at all.
/// The baseline a key is compared against: a theme stating that key's PREREQUISITES
/// and nothing else.
///
/// Per key rather than once, because a gated key's prerequisite is itself a key that
/// moves every surface — so comparing `heading_band_radius` against a bandless theme
/// would report the BAND's arrival, not the radius's.
fn baseline_for(key: &super::super::keys::Key) -> Theme {
    let mut themes = Themes::builtin();
    themes.merge_over_for_test(&format!("[themes.probe]\n{}", key.reach.needs));
    themes.resolve("probe")
}

/// A document exercising every construct a theme key can style, so no surface's probe
/// is blind to a key merely because its fixture lacked the construct.
const FIXTURE: &str = "\
# Heading one

Body text with **bold**, *italics*, ~~strike~~, `code`, ==mark==, ^sup^, ~sub~ and a
[link](https://example.com/target).

## Heading two

- a bullet
  - a nested bullet
    - a deeper bullet
- [ ] an unchecked task
- [x] a checked task

1. an ordered item
2. another

> A quoted paragraph.
>
> - with a nested list

```
a fenced code block
```

| Head A | Head B |
|--------|--------|
| cell   | cell   |

---

Text with {==an annotated claim==}{>>a comment<<} in it.
";

/// A palette built the way production builds one: the theme's own page, ink and accent
/// where it states them, the desktop probe's where it does not.
///
/// Built this way deliberately. `background`, `foreground` and `accent_color` reach
/// every surface THROUGH the palette rather than through a sink reading the key, so a
/// probe that passed fixed base colours would report all three as reaching nothing —
/// which is a defect in the probe, not in the sinks.
fn palette_for(t: &Theme) -> crate::palette::Palette {
    const DESKTOP_BG: gtk::gdk::RGBA = gtk::gdk::RGBA::WHITE;
    const DESKTOP_FG: gtk::gdk::RGBA = gtk::gdk::RGBA::BLACK;
    let accent = gtk::gdk::RGBA::new(0.2, 0.5, 0.9, 1.0);
    crate::palette::Palette::from_base(
        t.background.unwrap_or(DESKTOP_BG),
        t.foreground.unwrap_or(DESKTOP_FG),
        t.foreground.unwrap_or(DESKTOP_FG),
        t.accent_color.unwrap_or(accent),
        t,
    )
}

fn export_doc() -> crate::export::ExportDoc {
    export_doc_of(FIXTURE)
}

fn export_doc_of(md: &str) -> crate::export::ExportDoc {
    crate::export::doc::build(md, &crate::export::RenderOptions::default())
}

/// The HTML sink's whole artefact.
fn html_of(t: &Theme) -> String {
    crate::export::html::render(&export_doc(), &palette_for(t), t)
}

/// The PDF sink's page: its geometry AND its ink.
///
/// **Both halves, because neither alone is enough.** The layout digest catches every
/// key that moves a line — an indent, a height, a gap, a fill, a marker — and is blind
/// to colour, because a run's ink lives in Pango ATTRIBUTES that `Layout::text()` does
/// not carry and `AttrList::to_str` cannot be read at this project's Pango floor
/// (it is `v1_50`; GTK4Rs/AP-114). The pixel hash catches the ink and is blind to a
/// change too subtle to move a byte. A key reaching this sink moves one or the other.
fn pdf_of(t: &Theme) -> String {
    pdf_of_md(FIXTURE, t)
}

fn pdf_of_md(md: &str, t: &Theme) -> String {
    use gtk::pango::prelude::FontMapExt;
    let ctx = pangocairo::FontMap::default().create_context();
    // Through `Paged`, the same entry point `window::export_pdf` uses — so this sweep
    // cannot pass against a stage sequence production does not perform
    // (F-PAGINATE-001).
    let paged = crate::export::pdf::Paged::prepare(
        &export_doc_of(md),
        &ctx,
        468.0,
        684.0,
        std::rc::Rc::new(t.clone()),
        54.0,
    );
    let laid = paged.laid();
    let mut out = String::new();
    for (line, frag) in laid.lines.iter().zip(&laid.fragments) {
        out.push_str(&line.digest_for_test());
        out.push_str(&format!(
            "|{:.3}|{:.3}|{:.3}\n",
            frag.height, frag.space_before, frag.space_after
        ));
    }
    let surface = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, 612, 792)
        .expect("an image surface needs no display");
    {
        let cr = gtk::cairo::Context::new(&surface).expect("a cairo context");
        // White paper under the ink, for the reason `pdf/measure/tests`'s own harness
        // gives: `draw_page` paints no background, so without this every glyph lands
        // premultiplied at partial coverage.
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().expect("a fill on a fresh surface");
        let palette = palette_for(t);
        for index in 0..paged.page_count() {
            let _drawn = paged.draw(&cr, index, &palette);
        }
    }
    let data = surface.take_data().expect("surface data");
    // A cheap rolling hash: the assertion is "did the page change", never "how".
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data.iter() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let _ = std::fmt::Write::write_fmt(&mut out, format_args!("ink:{h:016x}"));
    out
}

/// The preview's three observables, joined. See the module header for what the third
/// one proves and what it leaves to the paint tests.
fn preview_of(t: &Theme) -> String {
    let palette = palette_for(t);
    use gtk::prelude::*;
    let buffer = gtk::TextBuffer::new(None);
    crate::tags::setup_tags_with_theme(&buffer, &palette, 1.0, t);
    let mut tags = Vec::new();
    buffer.tag_table().foreach(|tag| tags.push(tag_digest(tag)));
    tags.sort();
    format!(
        "{}\n{}\n{}",
        crate::preview::theme_css(t, &palette),
        tags.join("\n"),
        decoration_digest(t)
    )
}

/// Every property `tags.rs` may set, read back off the tag.
fn tag_digest(tag: &gtk::TextTag) -> String {
    use gtk::prelude::*;
    // **Colours are read TYPED, not through `Value`'s `Debug`.** A boxed `GdkRGBA`
    // formats as its POINTER, which differs between two resolutions of the same theme
    // — so a digest built that way reports every key as reaching the preview, which is
    // the sweep's assertion passing for the wrong reason in both directions.
    const COLOURS: [&str; 5] = [
        "foreground-rgba",
        "background-rgba",
        "paragraph-background-rgba",
        "underline-rgba",
        "strikethrough-rgba",
    ];
    const PLAIN: [&str; 15] = [
        "name",
        "weight",
        "weight-set",
        "scale",
        "style",
        "underline",
        "overline",
        "strikethrough",
        "family",
        "rise",
        "left-margin",
        "right-margin",
        "indent",
        "pixels-above-lines",
        "pixels-below-lines",
    ];
    let mut out = String::new();
    for p in COLOURS {
        let v = tag.property::<Option<gtk::gdk::RGBA>>(p);
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "{p}={};",
                v.map(crate::palette::to_hex_rgba).unwrap_or_default()
            ),
        );
    }
    for p in PLAIN {
        let v = tag.property_value(p);
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{p}={v:?};"));
    }
    let _ = std::fmt::Write::write_fmt(&mut out, format_args!("prio={}", tag.priority()));
    out
}

/// The resolved decorations `snapshot_layer` paints from, and the metrics it scales.
fn decoration_digest(t: &Theme) -> String {
    use super::super::{MarkerKind, HEADING_LEVELS};
    // **The metrics `snapshot_layer` scales — NOT the whole `Metrics`, and not
    // `Typography` at all.** A whole-struct dump makes this arm answer "does the key
    // reach `Theme::resolve`", which `resolve` already guarantees; the sweep then
    // passes for every key whatever the preview does with it. MEASURED: with
    // `{t.typography:?}` in this digest, breaking `tags.rs`'s `bold_weight` read left
    // the sweep green. Everything typographic reaches the preview through a
    // `GtkTextTag`, and the tag digest is where it must show.
    let m = &t.metrics;
    let mut out = format!(
        "{}|{}|{}|{}|{}|{:?}|{:?}|",
        m.blockquote_bar_width,
        m.list_step,
        m.list_item_gap,
        m.rule_space,
        m.table_cell_radius,
        m.heading_band_radius,
        m.heading_band_padding,
    );
    for level in 0..HEADING_LEVELS {
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!("{:?}|", t.heading_band_decor(level)),
        );
    }
    for kind in [
        MarkerKind::Bullet,
        MarkerKind::Ordered,
        MarkerKind::Task,
        MarkerKind::TaskChecked,
    ] {
        for depth in 1..=3usize {
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!(
                    "{:?}/{:?}|",
                    t.marker_decor(kind, depth),
                    t.marker_ink(kind, depth)
                ),
            );
        }
    }
    // The rest of what `snapshot_layer` and the live highlight paths read: every
    // decoration the preview draws itself, with no tag and no CSS to carry it.
    //
    // **Enumerated, never `{t:?}`.** A whole-model dump would make this arm answer
    // "does the key reach `Theme::resolve`", which `resolve` already guarantees and
    // which is not the question. Each line here is a claim that the preview's painter
    // reads that field — and a key that reaches nothing else has to appear in one of
    // them or fail.
    let _ = std::fmt::Write::write_fmt(
        &mut out,
        format_args!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            t.blockquote_bar_decor(),
            t.rule_decor(),
            t.annotation_chip_decor(),
            t.annotation_chip_fg,
            t.blockquote_bg,
            t.blockquote_fg,
            t.code_block_bg,
            t.find_hl_all_color,
            t.find_hl_current_color,
            t.selection_bg,
            t.selection_fg,
        ),
    );
    let _ = std::fmt::Write::write_fmt(&mut out, format_args!("|{:?}", t.syntect_theme));
    out
}

/// **The sweep.**
///
/// A `#[gtktest::test]` because the preview's `GtkTextTag` set is one of the three
/// observables and a tag table needs a live GTK — not because anything here needs a
/// display or a window. POLICY § GTK-object integration tests sanctions exactly this:
/// the tag set is live GTK object state and is not decidable from data while
/// `setup_tags_with_theme` owns both the decisions and the mutation.
#[cfg(feature = "gtk-integration-tests")]
#[gtktest::test]
fn every_declared_key_reaches_every_surface_it_claims() {
    let mut unreached: Vec<String> = Vec::new();
    let mut spuriously_reached: Vec<String> = Vec::new();
    for key in KEYS {
        // A sprite key's probe names a file that resolves to nothing beside the
        // compiled-in themes, so `Some(sprite)` would be indistinguishable from the
        // baseline's `None`. Its family is guarded by `theme::tests::sprites`, which
        // resolves every built-in reference against the embedded table.
        if key.kind == Kind::Sprite {
            continue;
        }
        let base = baseline_for(key);
        let t = stating(key);
        for (surface, claimed, now, before) in [
            (
                "preview",
                key.reach.preview,
                preview_of(&t),
                preview_of(&base),
            ),
            ("HTML", key.reach.html, html_of(&t), html_of(&base)),
            ("PDF", key.reach.pdf, pdf_of(&t), pdf_of(&base)),
        ] {
            let moved = now != before;
            if claimed && !moved {
                unreached.push(format!("{} on the {surface}", key.name));
            }
            if !claimed && moved {
                spuriously_reached.push(format!(
                    "{} on the {surface} (declared unreachable: {:?})",
                    key.name, key.reach.why
                ));
            }
        }
    }
    assert!(
        unreached.is_empty(),
        "declared keys that reach NO surface output — accepted, SCHEMA-documented and \
         completely inert, with no log line, because `keys::lookup` claims them: {:#?}",
        unreached
    );
    assert!(
        spuriously_reached.is_empty(),
        "keys that reach a surface their `Reach` says they do not — the exception is \
         stale, so either wire it or restate it: {:#?}",
        spuriously_reached
    );
}

/// **The table header answers the same typography keys on all three surfaces.**
///
/// `Typography::bold_attr` is shared correctly by all three for INLINE bold — one of
/// the branch's better seams — and the header, which is also bold, hardcoded a weight
/// in three separate places instead: `font-weight: bold` in the preview's CSS, a
/// browser default in the HTML sink (which stated nothing at all), and a Pango `<b>` in
/// the PDF sink. All three are "bolder than the base", and none of them is the number
/// the theme stated (F-BOLD-001).
///
/// `heading_font` is the same shape one key over and it had already DRIFTED: it reached
/// the preview's `.cell-head` and not the export's `th`, so a theme with a display
/// heading face rendered its table headers in two different faces depending on where
/// you looked (F-CSS-001).
///
/// The probe values are chosen so nothing else in the fixture can produce them: no
/// shipped theme states weight 823, and "Probe Face" resolves to nothing anywhere.
#[test]
fn the_table_header_takes_the_themed_weight_and_face_on_every_surface() {
    let mut themes = Themes::builtin();
    themes.merge_over_for_test(
        "[themes.hdr]\nbold_weight = 823\nheading_font = \"Probe Face, monospace\"\n",
    );
    let t = themes.resolve("hdr");

    let css = crate::preview::theme_css(&t, &palette_for(&t));
    let head = css
        .lines()
        .find(|l| l.starts_with("scribtable .cell-head"))
        .expect("the preview styles the header cell");
    assert!(
        head.contains("font-weight: 823"),
        "the preview's header ignores bold_weight: {head}"
    );
    assert!(
        head.contains("Probe Face"),
        "the preview's header ignores heading_font: {head}"
    );

    let html = html_of(&t);
    let th = html
        .lines()
        .find(|l| l.starts_with("th {"))
        .expect("the HTML sink styles th");
    assert!(
        th.contains("font-weight: 823"),
        "the HTML header ignores bold_weight: {th}"
    );
    assert!(
        th.contains("Probe Face"),
        "the HTML header ignores heading_font: {th}"
    );

    // The PDF sink writes Pango markup, and the laid-out digest carries the line's TEXT
    // rather than its attributes — so the observable is the rendered INK, on a fixture
    // whose ONLY bold run is the header. Comparing the same document at two weights is
    // what makes the assertion about the header rather than about the page.
    let table_only = "| Head A | Head B |\n|--------|--------|\n| cell | cell |\n";
    let mut light = Themes::builtin();
    light.merge_over_for_test("[themes.hdr]\nbold_weight = 300\n");
    assert_ne!(
        pdf_of_md(table_only, &t),
        pdf_of_md(table_only, &light.resolve("hdr")),
        "the PDF header renders identically at weight 823 and weight 300, so it is \
         ignoring bold_weight and using Pango's own <b>"
    );

    // Anti-vacuity: a theme stating neither must NOT carry the probe values, or the
    // three assertions above are satisfiable by a build that hardcodes 823.
    let plain = Themes::builtin().resolve(SYSTEM_ID);
    let plain_css = crate::preview::theme_css(&plain, &palette_for(&plain));
    assert!(!plain_css.contains("823") && !plain_css.contains("Probe Face"));
    assert!(!html_of(&plain).contains("Probe Face"));
}
