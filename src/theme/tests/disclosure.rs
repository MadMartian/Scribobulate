//! TDD 18.48, 18.49, 18.50 / 18.2 — the disclosure summary's BAND and INK, on every surface that
//! carries them, and what a theme that states none of them must leave untouched.
//!
//! **The absence direction is the one worth a test of its own.** A decoration whose
//! keys are unset must be *not present*, never a guessed default (POLICY "No
//! hard-coded styling"), and the way that promise breaks is quiet: a floor added to a
//! key "so resolution is total", or a sink emitting an empty rule, moves every theme
//! including System and nothing says so. So each of the three surfaces is asserted
//! both ways in one body — silent under a theme that states nothing, and moved by a
//! theme that states something — because the first assertion alone is satisfied by a
//! build that dropped the keys entirely.

use super::super::{keys, Themes};
/// Named only by the feature-gated body and its helpers, so the import carries the
/// same cfg they do — a bare one is an unused-import error under a plain
/// `cargo test`, which is the build the coverage gate runs.
#[cfg(feature = "gtk-integration-tests")]
use super::super::{Theme, SYSTEM_ID};

/// A document whose only construct is a disclosure, so a mark found on a surface
/// belongs to this decoration and not to something else the fixture carried.
#[cfg(feature = "gtk-integration-tests")]
const DOC: &str = "<details>\n<summary>Summary</summary>\n\nhidden body\n\n</details>\n";

/// Every key this rubric adds, at values nothing shipped can be stating already.
#[cfg(feature = "gtk-integration-tests")]
const STATES_EVERYTHING: &str = "[themes.banded]\n\
     background = \"#ffffff\"\nforeground = \"#000000\"\n\
     disclosure_band_color = \"#339966\"\n\
     disclosure_band_radius = 7\n\
     disclosure_fg = \"#7f0e5a\"\n";

#[cfg(feature = "gtk-integration-tests")]
fn banded() -> Theme {
    let mut themes = Themes::builtin();
    themes.merge_over_for_test(STATES_EVERYTHING);
    themes.resolve("banded")
}

/// A palette built the way production builds one, so a key that reaches a surface
/// THROUGH the palette is not reported as reaching nothing.
#[cfg(feature = "gtk-integration-tests")]
fn palette_for(t: &Theme) -> crate::palette::Palette {
    let ink = t.foreground.unwrap_or(gtk::gdk::RGBA::BLACK);
    crate::palette::Palette::from_base(
        t.background.unwrap_or(gtk::gdk::RGBA::WHITE),
        ink,
        ink,
        t.accent_color
            .unwrap_or(gtk::gdk::RGBA::new(0.2, 0.5, 0.9, 1.0)),
        t,
    )
}

#[cfg(feature = "gtk-integration-tests")]
fn html_of(t: &Theme) -> String {
    let doc = crate::export::doc::build(DOC, &crate::export::RenderOptions::default());
    crate::export::html::render(&doc, &palette_for(t), t)
}

/// Whether any line the PDF sink lays out for [`DOC`] carries a background fill —
/// which for this document can only be the summary label's band.
#[cfg(feature = "gtk-integration-tests")]
fn pdf_bands_anything(t: &Theme) -> bool {
    use gtk::pango::prelude::FontMapExt;
    let ctx = pangocairo::FontMap::default().create_context();
    let doc = crate::export::doc::build(DOC, &crate::export::RenderOptions::default());
    // Through `Paged`, the same entry point `window::export_pdf` uses, so this cannot
    // pass against a stage sequence production does not perform.
    let paged = crate::export::pdf::Paged::prepare(
        &doc,
        &ctx,
        468.0,
        684.0,
        std::rc::Rc::new(t.clone()),
        54.0,
    );
    paged.laid().lines.iter().any(|l| l.is_filled_for_test())
}

/// Every tag's priority in a table built from `t`, by name — the number that decides
/// which of two tags setting `foreground` on one run wins.
#[cfg(feature = "gtk-integration-tests")]
fn priorities(t: &Theme) -> std::collections::HashMap<String, i32> {
    use gtk::prelude::*;
    let buffer = gtk::TextBuffer::new(None);
    crate::tags::setup_tags_with_theme(&buffer, &palette_for(t), 1.0, t);
    let mut out = std::collections::HashMap::new();
    buffer.tag_table().foreach(|tag| {
        if let Some(name) = tag.name() {
            out.insert(name.to_string(), tag.priority());
        }
    });
    out
}

/// The `disclosure-ink` tag's foreground, as the tag table actually holds it.
///
/// Read TYPED rather than through `Value`'s `Debug`: a boxed `GdkRGBA` formats as a
/// POINTER, which differs between two resolutions of the same theme and would make
/// this answer "yes" for every theme (ScrAP-327).
#[cfg(feature = "gtk-integration-tests")]
fn disclosure_ink_of(t: &Theme) -> Option<gtk::gdk::RGBA> {
    use gtk::prelude::*;
    let buffer = gtk::TextBuffer::new(None);
    crate::tags::setup_tags_with_theme(&buffer, &palette_for(t), 1.0, t);
    let tag = buffer
        .tag_table()
        .lookup(crate::tags::TagName::DisclosureInk.name())
        .expect("the disclosure ink tag is registered whatever the theme says");
    tag.property::<Option<gtk::gdk::RGBA>>("foreground-rgba")
}

/// **TDD 18.2 — a theme stating none of these keys leaves all three surfaces exactly
/// as they were before the keys existed.**
///
/// A `#[gtktest::test]` because one of the three observables is a live
/// `GtkTextTagTable`; nothing here needs a display or a window. That is also why this
/// body — and only this body — carries the feature cfg: the resolution guard below it
/// is display-free and belongs inside the unit-only coverage gate.
#[cfg(feature = "gtk-integration-tests")]
#[gtktest::test]
fn a_theme_stating_no_disclosure_band_or_ink_leaves_every_surface_untouched() {
    let plain = Themes::builtin().resolve(SYSTEM_ID);

    // The engine: unset means NOT PRESENT. `is_present()` is the predicate every one
    // of the three renderers gates on, so a floor sneaking onto either colour key
    // would light all three at once and this is where it shows first.
    assert!(
        !plain.disclosure_band_decor().is_present(),
        "System must band no summary line — an unset key is absent, never a default"
    );
    assert_eq!(plain.metrics.disclosure_band_radius, 0);
    assert!(plain.disclosure_fg.is_none());

    // The preview: the tag is REGISTERED either way, so the vocabulary does not vary
    // by theme, and it sets no foreground at all — which is what makes System's tag
    // table byte-identical rather than merely its pixels.
    assert!(
        disclosure_ink_of(&plain).is_none(),
        "the disclosure-ink tag must set no foreground under a theme that states none"
    );

    // **The priority ladder**, which is the whole reason this ink is a tag rather than
    // part of the band. Two orderings are live and both are decided by REGISTRATION
    // ORDER in `tags.rs`, which nothing else records: a summary inside a blockquote
    // must take this ink and not `blockquote_fg`'s, and a collapsed block's body
    // preview must keep its own. Asserted on the resolved priorities rather than on
    // the source order, because the source order is what a refactor moves.
    let prio = priorities(&plain);
    let of = |name: &str| {
        *prio
            .get(name)
            .unwrap_or_else(|| panic!("{name} is registered whatever the theme says"))
    };
    assert!(
        of("blockquote-ink") < of("disclosure-ink"),
        "a summary line inside a quote is the NARROWER statement, so its ink must beat \
         the quote's"
    );
    assert!(
        of("disclosure-ink") < of("disclosure-preview"),
        "a collapsed block's body preview is narrower still, so it must beat the \
         summary line's own ink"
    );
    assert!(
        of("disclosure-ink") < of("link"),
        "the summary ink sits at the bottom of the ink stack, so anything the label \
         may later carry keeps its own colour without this tag having to change"
    );

    // The HTML sink: no `summary` rule at all, not an empty one.
    assert!(
        !html_of(&plain).contains("summary {"),
        "the artefact carries a summary rule for a theme that asked for none"
    );

    // The PDF sink: no line carries a fill, so the label prints exactly as it did.
    assert!(
        !pdf_bands_anything(&plain),
        "the page bands the summary label for a theme that asked for none"
    );

    // …and the anti-vacuity half. Without it every assertion above is satisfied by a
    // build that ignores these keys completely, which is the same green suite and a
    // decoration nobody can use (ScrAP-209's shape).
    let banded = banded();
    assert!(banded.disclosure_band_decor().is_present());
    assert_eq!(banded.metrics.disclosure_band_radius, 7);
    assert_eq!(
        disclosure_ink_of(&banded).map(crate::palette::to_hex_opaque),
        Some("#7f0e5a".to_string()),
        "a stated disclosure_fg must reach the tag the preview inks the line with"
    );
    let html = html_of(&banded);
    assert!(
        html.contains("summary {")
            && html.contains("#339966")
            && html.contains("border-radius: 7px"),
        "the band and its radius must reach the artefact's summary rule"
    );
    assert!(
        html.contains("#7f0e5a"),
        "the summary ink must reach the artefact"
    );
    assert!(
        pdf_bands_anything(&banded),
        "the band must reach the page as a fill on the label's own line"
    );
}

/// **TDD 18.50 — the band's three appearances and its ink are independent of each
/// other**, so a theme may state any one of them alone.
///
/// The independence is the rubric, not the parsing: collapsing the ink into the band
/// (or the gradient into the fill) is the shape that makes a themed fill silently
/// re-ink every summary label, which is the defect `blockquote_bg`/`blockquote_fg`
/// were split to avoid.
#[test]
fn the_summary_band_and_its_ink_resolve_independently() {
    for (key, value) in [
        ("disclosure_band_color", "\"#339966\""),
        ("disclosure_fg", "\"#7f0e5a\""),
        ("disclosure_band_radius", "7"),
    ] {
        let mut themes = Themes::builtin();
        themes.merge_over_for_test(&format!("[themes.one]\n{key} = {value}\n"));
        let t = themes.resolve("one");
        match key {
            "disclosure_band_color" => {
                assert!(t.disclosure_band_decor().is_present());
                assert!(t.disclosure_fg.is_none(), "a fill must not seed an ink");
                assert_eq!(t.metrics.disclosure_band_radius, 0);
            }
            "disclosure_fg" => {
                assert!(
                    !t.disclosure_band_decor().is_present(),
                    "an ink must not seed a band"
                );
                assert!(t.disclosure_fg.is_some());
            }
            _ => {
                assert_eq!(t.metrics.disclosure_band_radius, 7);
                assert!(
                    !t.disclosure_band_decor().is_present(),
                    "a radius must not seed a band — it is only ever consulted for one"
                );
            }
        }
    }

    // A gradient is a SECOND stop: stated alone it renders nothing, exactly as
    // `heading_band_gradient_to_color` does, and the discard is logged rather than
    // silent (ScrAP-324).
    let mut themes = Themes::builtin();
    themes.merge_over_for_test("[themes.grad]\ndisclosure_band_gradient_to_color = \"#339966\"\n");
    let t = themes.resolve("grad");
    assert!(
        !t.disclosure_band_decor().is_present(),
        "a second stop with no first one is not a band"
    );
    assert_eq!(t.disclosure_band_decor().without_sprite(), None);
}

/// **A shipped theme that plates its indicator with a sprite keeps a GLYPH rung
/// beneath it.**
///
/// The registry-driven sweeps prove each key REACHES its surface; none of them proves
/// a shipped theme states one, so a dressing deleted from `data/themes.toml` leaves
/// every other guard green — the same gap
/// `sprites::pixel_quests_blockquote_bar_resolves_to_a_compiled_in_sprite` is pinned
/// against.
///
/// **The rung rule is the load-bearing half.** `decor`'s `candidates()` hands back
/// ORDERED RUNGS precisely so a sprite that will not decode falls to the next one, and
/// the indicator is this control's ENTIRE feedback channel — a build that fired
/// `toggled` without changing its arrow was reported from a live session as doing
/// nothing at all. A theme naming a plate and no glyph beneath it satisfies every
/// other guard in this tree and degrades, on a host whose decoder refuses that file,
/// to a control with no visible state. Driven off the shipped file per state, so a
/// future theme is covered by having named a sprite rather than by anyone remembering
/// this test.
#[test]
fn a_shipped_indicator_sprite_always_has_a_glyph_beneath_it() {
    let raw: toml::Value = toml::from_str(super::super::BUILTIN_THEMES_TOML)
        .expect("the shipped themes file must parse");
    let mut checked = 0usize;
    for (id, block) in raw["themes"].as_table().expect("a themes table") {
        let table = block.as_table().expect("a theme block");
        for (state, sprite, glyph) in [
            (
                "collapsed",
                keys::DISCLOSURE_SPRITE.name,
                keys::DISCLOSURE_GLYPH.name,
            ),
            (
                "expanded",
                keys::DISCLOSURE_EXPANDED_SPRITE.name,
                keys::DISCLOSURE_EXPANDED_GLYPH.name,
            ),
        ] {
            if !table.contains_key(sprite) {
                continue;
            }
            checked += 1;
            assert!(
                table.contains_key(glyph),
                "theme {id:?} plates its {state} indicator with {sprite} and states no \
                 {glyph} beneath it — a plate this host's decoder refuses then erases \
                 the arrow instead of degrading to a character"
            );
        }
    }
    assert!(
        checked > 0,
        "no shipped theme plates its indicator — this guard is vacuous"
    );
}

/// **Pixel Quest's indicator plates are compiled in, decode, and match the box they
/// are resampled into.**
///
/// The size half is the coupling `a_shipped_bar_width_is_never_narrower_than_the_sprite_it_tiles`
/// exists for, one decoration over and with the opposite failure: the bar CLIPS its
/// tile, where the indicator RESAMPLES its plate — so a plate whose natural size is not
/// `disclosure_marker_size` is neither absent nor sliced but *blurred*, through a
/// nearest-neighbour filter with no clean answer for a non-integer ratio. Neither
/// failure raises anything, and only the theme file's prose couples the two numbers.
#[test]
fn pixel_quests_indicator_plates_are_compiled_in_and_match_their_box() {
    let t = Themes::builtin().resolve("pixelquest");
    let box_px = t.metrics.disclosure_marker_size;
    for (state, slot) in [
        ("collapsed", &t.sprites.disclosure),
        ("expanded", &t.sprites.disclosure_expanded),
    ] {
        let sprite = slot
            .as_ref()
            .unwrap_or_else(|| panic!("Pixel Quest states no {state} indicator sprite"));
        assert!(
            matches!(sprite, crate::sprite::SpriteRef::Compiled(_)),
            "a built-in theme's sprite must come from the binary, not from a path: {sprite}"
        );
        let tex = crate::sprite::texture(sprite)
            .unwrap_or_else(|| panic!("Pixel Quest's {state} plate did not decode"));
        let (w, h) = (
            gtk::prelude::TextureExt::width(&tex),
            gtk::prelude::TextureExt::height(&tex),
        );
        assert_eq!(
            (w, h),
            (box_px, box_px),
            "Pixel Quest's {state} plate is {w}×{h} but disclosure_marker_size is \
             {box_px}, so it is resampled to a size it was not drawn at"
        );
        // …and the call the WIDGET actually makes, not just the one this test finds
        // convenient. `widgets::disclosure` asks for `scaled`, whose `None` is what
        // drops the indicator to its glyph rung — silently, since degrading is the
        // designed behaviour. A guard that only proves `texture` decodes cannot tell a
        // plate that reaches the screen from one that never does.
        assert!(
            crate::sprite::scaled(sprite, box_px, box_px).is_some(),
            "Pixel Quest's {state} plate decodes but does not resample to {box_px}², so \
             the control silently shows its glyph rung instead"
        );
    }
}

/// **Synthwave and Candy dress BOTH indicator states.**
///
/// The two states resolve independently, so a theme can dress one and leave the other
/// on its stock icon — which renders as a fold whose arrow changes toolkit halfway
/// through a click. Pinned by name because "this theme dresses its indicator" is a
/// claim about the shipped file that no registry-driven sweep can make.
#[test]
fn the_glyph_themes_dress_both_indicator_states() {
    for id in ["synthwave", "candy"] {
        let t = Themes::builtin().resolve(id);
        for expanded in [false, true] {
            let rungs = t.disclosure_marker_decor(expanded).candidates();
            assert!(
                matches!(
                    rungs.first(),
                    Some(crate::theme::MarkerSubstitute::Glyph(_))
                ),
                "theme {id:?} leaves its {} indicator on the stock icon",
                if expanded { "expanded" } else { "collapsed" }
            );
        }
        assert!(
            t.disclosure_marker_color.is_some(),
            "theme {id:?} states a glyph indicator and no ink for it, so it keeps the \
             desktop theme's foreground on a page of its own"
        );
    }
}
