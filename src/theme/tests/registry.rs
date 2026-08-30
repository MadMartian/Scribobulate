//! The registry against the resolved model: shapes, not values.
//!
//! Everything here asks the same question from a different angle — *does the model a
//! consumer indexes still have the shape the registry declares?* That coupling is
//! carried by nothing at compile time: `Sources::each`'s `N` comes from the
//! destination field's array length, and a `Key`'s slot count comes from its
//! `Levelling`, with no type joining them.

use super::super::keys::{self, Levelling, BULLET_TIERS, HEADING_LEVELS};
use super::super::*;

/// **Every levelled key lands on a model array of exactly its own slot count.**
///
/// The sprite family had this guard and nothing else did (`sprites.rs`'s
/// `a_compiled_in_sprite_reaches_every_slot_it_can_be_named_in`, which counts resolved
/// sprite slots against registry spellings). Colours, fonts, glyphs, ints, floats and
/// lines had none — so `heading_color` wired into a `[_; BULLET_TIERS]` field would
/// have resolved three of five levels and dropped h4/h5, and only the runtime assert in
/// `Sources::each` (F-SHAPE-001) would have said so.
///
/// The count assertions are the half that catches an ADDITION: a heading key declared
/// in the registry and given a scalar field, or a field added with no key behind it,
/// moves one side of the equality and not the other.
#[test]
fn every_levelled_key_lands_on_a_model_array_of_its_own_slot_count() {
    let t = Themes::builtin().resolve(SYSTEM_ID);

    // One entry per HEADING-levelled key in the registry, in declaration order.
    let heading_arrays: Vec<(&str, usize)> = vec![
        ("heading_color", t.heading_colors.len()),
        ("heading_font", t.heading_fonts.len()),
        ("heading_scale", t.typography.heading_scale.len()),
        ("heading_weight", t.typography.heading_weight.len()),
        ("heading_overline", t.heading_rule.overline.len()),
        ("heading_underline", t.heading_rule.underline.len()),
        (
            "heading_underline_color",
            t.heading_rule.underline_color.len(),
        ),
        ("heading_band_color", t.heading_band.fills.len()),
        (
            "heading_band_gradient_to_color",
            t.heading_band.gradient_to.len(),
        ),
        ("heading_band_sprite", t.sprites.heading_band.len()),
        ("heading_band_radius", t.metrics.heading_band_radius.len()),
        ("heading_band_padding", t.metrics.heading_band_padding.len()),
        ("heading_space_above", t.metrics.heading_space_above.len()),
        ("heading_space_below", t.metrics.heading_space_below.len()),
    ];
    // …and one per DEPTH-levelled key.
    let depth_arrays: Vec<(&str, usize)> = vec![
        ("list_marker_color", t.list_bullet_colors.len()),
        ("list_bullet_glyph", t.list_glyphs.bullet.len()),
        ("list_bullet_sprite", t.sprites.list_bullet.len()),
    ];

    for (name, len) in &heading_arrays {
        assert_eq!(
            *len, HEADING_LEVELS,
            "{name} is declared Heading but its model field carries {len} slots"
        );
    }
    for (name, len) in &depth_arrays {
        assert_eq!(
            *len, BULLET_TIERS,
            "{name} is declared Depth but its model field carries {len} slots"
        );
    }

    let declared = |levelling: Levelling| {
        keys::KEYS
            .iter()
            .filter(|k| k.levelling == levelling)
            .map(|k| k.name)
            .collect::<Vec<_>>()
    };
    fn listed<'a>(rows: &[(&'a str, usize)]) -> Vec<&'a str> {
        rows.iter().map(|(n, _)| *n).collect()
    }
    assert_eq!(
        listed(&heading_arrays),
        declared(Levelling::Heading),
        "the registry's Heading family and the model fields above have diverged — a \
         key was added on one side only"
    );
    assert_eq!(
        listed(&depth_arrays),
        declared(Levelling::Depth),
        "the registry's Depth family and the model fields above have diverged — a key \
         was added on one side only"
    );
}
