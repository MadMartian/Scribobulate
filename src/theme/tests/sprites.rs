//! Sprites: which source a reference resolves against, and what a refused one
//! costs (TDD 18.19/18.28/18.31, ScrAP-324).

use super::super::model::*;
use super::super::value::*;
use super::super::*;

/// Parse a one-theme fragment against a directory origin and resolve it, so a
/// sprite test drives the same path a user file does — parse, validate, resolve
/// against the file's own directory — rather than reaching past it.
fn resolved_from(fragment: &str, origin: crate::sprite::SpriteOrigin<'_>) -> Theme {
    let specs = ThemeSpec::parse_file(fragment, origin).expect("the fragment parses");
    Theme::resolve("t", &specs["t"], &ThemeSpec::default())
}

/// TDD 18.31 / 18.2 — the rule's sprite is opt-in, resolves like every other sprite
/// key, and leaves the flat `rule` colour standing beside it as the fallback.
#[test]
fn the_rule_sprite_is_opt_in_and_keeps_the_flat_colour_beside_it() {
    assert!(Themes::builtin().resolve(SYSTEM_ID).sprites.rule.is_none());

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("rule.png"), b"bytes are not validated here").unwrap();
    let origin = crate::sprite::SpriteOrigin::Directory(dir.path());
    let good = resolved_from(
        "[themes.t]\nrule_sprite = \"rule.png\"\nrule_color = \"#123456\"\n",
        origin,
    );
    let crate::sprite::SpriteRef::File(got) = good.sprites.rule.expect("resolved") else {
        panic!("a directory origin must resolve to a file");
    };
    assert!(got.is_absolute());
    // The colour is UNTOUCHED by the sprite resolving: the sprite outranks it at
    // paint time, and the colour is what a refused reference falls back to.
    assert_eq!(good.rule_color, parse_color("#123456"));

    let escaping = resolved_from("[themes.t]\nrule_sprite = \"../escape.png\"\n", origin);
    assert!(escaping.sprites.rule.is_none());
}

/// **Every sprite a compiled-in theme names is compiled in too.**
///
/// The general guard, and the one that would have caught the defect it was written
/// for: Pixel Quest's blockquote-bar sprite was resolved against a themes-file
/// DIRECTORY that only a user file has, so it stayed unresolved forever and the
/// bar rendered flat navy on every fresh install — with no warning, no crash and a
/// green suite, because an unresolved sprite is inert by design and every existing
/// sprite test used a synthetic path of its own.
///
/// Deliberately keyed off the RAW file rather than a resolved `Themes`: after
/// resolution a refused reference and a key nobody set are both `None`, so a
/// resolved theme cannot tell you which one it is. Which keys name a sprite comes
/// from the registry, so a new sprite key is covered here the moment it is
/// declared — there is no second list to forget.
#[test]
fn every_built_in_theme_sprite_reference_is_embedded() {
    let raw: toml::Value =
        toml::from_str(BUILTIN_THEMES_TOML).expect("the shipped themes file must parse");
    let mut checked = 0usize;
    for (id, block) in raw["themes"].as_table().expect("a themes table") {
        for (spelling, value) in block.as_table().expect("a theme block") {
            let Some(key) = keys::lookup(spelling) else {
                continue;
            };
            if key.kind != keys::Kind::Sprite {
                continue;
            }
            let rel = value.as_str().expect("a sprite key names a file");
            checked += 1;
            assert!(
                crate::sprite::builtin(rel).is_some(),
                "built-in theme {id:?} names sprite {rel:?}, which is not in \
                     crate::sprite::BUILTIN_SPRITES — add it there with include_bytes!, \
                     or the decoration is silently absent on every host"
            );
        }
    }
    // A guard that iterates an empty set passes vacuously and reads exactly like one
    // that checked something, so pin that the shipped file names at least one.
    assert!(
        checked > 0,
        "no built-in theme names a sprite — this guard is now vacuous"
    );
}

/// The compiled-in source is a property of the KEY SET, not of one decoration.
///
/// The bug that prompted it was found through the blockquote bar, and a fix proved
/// only through the blockquote bar would be evidence about one arm of a ten-arm
/// loop. Every sprite spelling the registry declares is driven here from a
/// built-in-shaped fragment naming the one sprite this binary embeds, so the
/// mechanism is shown to be key-agnostic — and the two paths a sprite can reach
/// the screen by (a `Sprites` field the drawn decorations read, and `bytes`, which
/// both export sinks read) are both exercised.
///
/// It is driven off `keys::KEYS` rather than a list of its own, so a sprite key
/// added to the vocabulary is covered by this guard without anyone remembering to
/// extend it — which is exactly what the list it replaced needed.
#[test]
fn a_compiled_in_sprite_reaches_every_slot_it_can_be_named_in() {
    let embedded = "sprites/copper-plate.png";
    let mut fragment = String::from("[themes.embedded]\n");
    let mut spellings = 0usize;
    for key in keys::KEYS.iter().filter(|k| k.kind == keys::Kind::Sprite) {
        for idx in 0..key.slots() {
            fragment.push_str(&format!("{} = \"{embedded}\"\n", key.spelling(idx)));
            spellings += 1;
        }
    }
    assert!(spellings > 0, "the vocabulary declares no sprite key");

    let mut themes = Themes::builtin();
    themes.merge_over_for_test(&fragment);
    let s = themes.resolve("embedded").sprites;
    let expected = crate::sprite::SpriteRef::Compiled(embedded);
    let every: Vec<&Option<crate::sprite::SpriteRef>> = s
        .heading_band
        .iter()
        .chain(s.list_bullet.iter())
        .chain([
            &s.annotation_chip,
            &s.list_ordered,
            &s.list_task,
            &s.list_task_checked,
            &s.blockquote_bar,
            &s.rule,
        ])
        .collect();
    assert_eq!(
        every.len(),
        spellings,
        "the resolved Sprites and the registry disagree on how many sprites a \
             theme can name — one of them is carrying a slot the other cannot see"
    );
    for (i, slot) in every.iter().enumerate() {
        assert_eq!(
            slot.as_ref(),
            Some(&expected),
            "sprite slot {i} did not resolve to the compiled-in sprite"
        );
        // …and it is readable, which is the half that reaches an export sink. A
        // reference that resolves but yields no bytes is the same blank decoration
        // one step later.
        assert!(
            !crate::sprite::bytes(slot.as_ref().unwrap())
                .expect("compiled-in bytes")
                .is_empty(),
            "sprite slot {i} resolved but carries no bytes"
        );
    }
}

/// Pixel Quest is the first shipped theme to name a sprite, and the one the defect
/// was found through — so it is pinned by name as well as by the general guard
/// above, since the general guard would still pass if the key were simply deleted
/// from the theme.
#[test]
fn pixel_quests_blockquote_bar_resolves_to_a_compiled_in_sprite() {
    let bar = Themes::builtin()
        .resolve("pixelquest")
        .sprites
        .blockquote_bar
        .expect("Pixel Quest states blockquote_bar_sprite");
    assert!(
        matches!(bar, crate::sprite::SpriteRef::Compiled(_)),
        "a built-in theme's sprite must come from the binary, not from a path: {bar}"
    );
    assert!(crate::sprite::bytes(&bar).is_some_and(|b| !b.is_empty()));
}

/// **An installed copy of the shipped themes file cannot take a built-in
/// decoration away.**
///
/// `themes.toml` is installed on the search path (`/usr/share/scribobulate`,
/// `%APPDATA%`, …), so the app reads its own shipped file back as an ordinary
/// themes FILE, with its sprite references resolving against that directory. Two
/// outcomes, and both must be right: with the sprites installed beside it the
/// reference resolves to that file, and with them absent — a packaging omission,
/// or a platform like the macOS bundle that installs no themes file's assets at
/// all — the merge must leave the compiled-in sprite standing rather than
/// overwriting it with the refusal.
///
/// The second half is why the fix does not merely move the problem: it makes a
/// packaging mistake cost a log line instead of a decoration.
#[test]
fn an_installed_themes_file_cannot_unship_a_compiled_in_sprite() {
    let key = "sprites/copper-plate.png";
    let fragment = format!("[themes.pixelquest]\nblockquote_bar_sprite = \"{key}\"\n");

    // (a) sprites installed beside the file — it resolves to the installed file.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sprites")).unwrap();
    std::fs::write(dir.path().join(key), b"not a real png, just bytes").unwrap();
    let mut with_assets = Themes::builtin();
    with_assets.merge_over(
        Themes::parse(
            &fragment,
            crate::sprite::SpriteOrigin::Directory(dir.path()),
        )
        .expect("parses"),
    );
    assert!(matches!(
        with_assets
            .resolve("pixelquest")
            .sprites
            .blockquote_bar
            .expect("resolved"),
        crate::sprite::SpriteRef::File(_)
    ));

    // (b) sprites NOT installed — the reference is refused, and the compiled-in
    // sprite the built-in theme already carries survives the merge untouched.
    let bare = tempfile::tempdir().unwrap();
    let mut without_assets = Themes::builtin();
    without_assets.merge_over(
        Themes::parse(
            &fragment,
            crate::sprite::SpriteOrigin::Directory(bare.path()),
        )
        .expect("parses"),
    );
    assert_eq!(
        without_assets
            .resolve("pixelquest")
            .sprites
            .blockquote_bar
            .expect("the compiled-in sprite must survive a refused override"),
        crate::sprite::SpriteRef::Compiled(key)
    );
}

/// TDD 18.28 — the bar sprite is opt-in, and it goes through the SAME
/// theme-relative validation every other sprite key does — which is now structural
/// rather than remembered: resolution walks the values the registry typed as
/// sprites, so a new sprite key is validated by having been declared.
#[test]
fn the_blockquote_bar_sprite_is_opt_in_and_validated_like_every_other() {
    assert!(Themes::builtin()
        .resolve(SYSTEM_ID)
        .sprites
        .blockquote_bar
        .is_none());

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("bar.png"), b"not a real png, just bytes").unwrap();
    let origin = crate::sprite::SpriteOrigin::Directory(dir.path());
    let good = resolved_from("[themes.t]\nblockquote_bar_sprite = \"bar.png\"\n", origin);
    let crate::sprite::SpriteRef::File(got) = good.sprites.blockquote_bar.expect("resolved") else {
        panic!("a directory origin must resolve to a file");
    };
    assert!(got.is_absolute());

    let escaping = resolved_from(
        "[themes.t]\nblockquote_bar_sprite = \"../escape.png\"\n",
        origin,
    );
    assert!(escaping.sprites.blockquote_bar.is_none());
}

/// Sprite resolution is the ONE step that answers a sprite key — proves that
/// against a DIRECTORY origin it accepts a contained relative reference and drops
/// one that fails `crate::sprite::resolve`'s checks, independent of the XDG search
/// path.
#[test]
fn a_contained_reference_resolves_and_a_bad_one_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("chip.png"), b"not a real png, just bytes").unwrap();
    let origin = crate::sprite::SpriteOrigin::Directory(dir.path());

    let good = resolved_from(
        "[themes.t]\nannotation_chip_sprite = \"chip.png\"\n",
        origin,
    );
    // `resolve` only checks extension/containment/size, not that the bytes
    // decode — decoding is `sprite::texture`'s job, exercised in `sprite.rs`.
    let crate::sprite::SpriteRef::File(got) = good.sprites.annotation_chip.expect("resolved")
    else {
        panic!("a directory origin must resolve to a file");
    };
    assert!(got.is_absolute());
    assert!(got.ends_with("chip.png"));

    let bad = resolved_from(
        "[themes.t]\nannotation_chip_sprite = \"../escape.png\"\n",
        origin,
    );
    assert!(bad.sprites.annotation_chip.is_none());
}
