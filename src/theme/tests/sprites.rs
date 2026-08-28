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

/// TDD 18.31 / 18.2 — a sprite key and the FLAT key beside it are independent: the
/// sprite outranks the colour at paint time, and the colour is what a refused
/// reference falls back to, so resolving one must not disturb the other.
///
/// The opt-in, directory-relative and containment halves this test used to also assert
/// are now swept across every sprite key at once (see below); what is left here is the
/// claim that is about this PAIR of keys rather than about sprite resolution.
#[test]
fn a_sprite_key_leaves_the_flat_key_beside_it_standing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("rule.png"), b"bytes are not validated here").unwrap();
    let origin = crate::sprite::SpriteOrigin::Directory(dir.path());
    let good = resolved_from(
        "[themes.t]\nrule_sprite = \"rule.png\"\nrule_color = \"#123456\"\n",
        origin,
    );
    assert!(good.sprites.rule.is_some());
    assert_eq!(good.rule_color, parse_color("#123456"));

    // …and a REFUSED sprite leaves it standing too, which is the case the fallback
    // exists for.
    let escaping = resolved_from(
        "[themes.t]\nrule_sprite = \"../escape.png\"\nrule_color = \"#123456\"\n",
        origin,
    );
    assert!(escaping.sprites.rule.is_none());
    assert_eq!(escaping.rule_color, parse_color("#123456"));
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

/// **Every sprite key is opt-in, resolves against the file's own directory, and
/// refuses a reference that leaves it** — asserted across the WHOLE key set.
///
/// This replaces three near-identical hand-written variants (the rule's, the
/// blockquote bar's and the annotation chip's), each of which asked the same three
/// questions of one key. The file already carried the exemplary registry-driven sweep
/// beside them, so the hand-maintained mirrors were the odd ones out: a sprite key
/// added later got none of these three properties checked until somebody remembered to
/// write a fourth copy.
///
/// The three properties are asserted per key rather than per decoration because they
/// are properties of `SpriteOrigin::resolve`, which has one implementation for all of
/// them.
#[test]
fn every_sprite_key_is_opt_in_directory_relative_and_refuses_an_escape() {
    // The escape target REALLY EXISTS, one level above the theme directory. Pointing
    // at a missing file would be refused by the does-this-resolve gate instead, so the
    // containment half would pass with containment deleted (GTK4Rs/AP-254).
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("escape.png"), b"a real file, outside").unwrap();
    let theme_dir = root.path().join("theme");
    std::fs::create_dir(&theme_dir).unwrap();
    std::fs::write(theme_dir.join("art.png"), b"not a real png, just bytes").unwrap();
    let origin = crate::sprite::SpriteOrigin::Directory(&theme_dir);
    let system = Themes::builtin().resolve(SYSTEM_ID);

    let mut checked = 0usize;
    for key in keys::KEYS.iter().filter(|k| k.kind == keys::Kind::Sprite) {
        for idx in 0..key.slots() {
            let spelling = key.spelling(idx);
            checked += 1;

            // (a) OPT-IN: the base theme states no sprite anywhere, so every slot of
            // every sprite key is absent until a theme asks for one (TDD 18.2).
            assert!(
                sprite_slot(&system, key, idx).is_none(),
                "[themes.system] resolves a sprite for {spelling}, so it is not opt-in"
            );

            // (b) DIRECTORY-RELATIVE: a contained reference resolves to an absolute
            // path inside the stating file's own directory.
            let good = resolved_from(&format!("[themes.t]\n{spelling} = \"art.png\"\n"), origin);
            match sprite_slot(&good, key, idx) {
                Some(crate::sprite::SpriteRef::File(p)) => {
                    assert!(p.is_absolute(), "{spelling} resolved to a relative path");
                    assert!(p.ends_with("art.png"), "{spelling} resolved to {p:?}");
                }
                other => panic!("{spelling} did not resolve to a file: {other:?}"),
            }

            // (c) CONTAINED: a reference that leaves the directory is refused, and the
            // slot degrades to absent rather than to something outside the theme.
            let escaping = resolved_from(
                &format!("[themes.t]\n{spelling} = \"../escape.png\"\n"),
                origin,
            );
            assert!(
                sprite_slot(&escaping, key, idx).is_none(),
                "{spelling} accepted a reference outside the theme directory"
            );
        }
    }
    assert!(checked > 0, "the vocabulary declares no sprite key");
}

/// One resolved sprite slot, addressed by the registry key and index that produced it.
///
/// The `Sprites` model is a struct of named fields, so a registry-driven sweep needs
/// one place that maps a `Key` back onto its field — this is it, and the exhaustive
/// `else` means a sprite key added without a slot here fails loudly instead of being
/// skipped.
fn sprite_slot<'a>(
    t: &'a Theme,
    key: &keys::Key,
    idx: usize,
) -> &'a Option<crate::sprite::SpriteRef> {
    let s = &t.sprites;
    if key.name == keys::ANNOTATION_CHIP_SPRITE.name {
        &s.annotation_chip
    } else if key.name == keys::LIST_BULLET_SPRITE.name {
        &s.list_bullet[idx]
    } else if key.name == keys::LIST_ORDERED_SPRITE.name {
        &s.list_ordered
    } else if key.name == keys::LIST_TASK_SPRITE.name {
        &s.list_task
    } else if key.name == keys::LIST_TASK_CHECKED_SPRITE.name {
        &s.list_task_checked
    } else if key.name == keys::HEADING_BAND_SPRITE.name {
        &s.heading_band[idx]
    } else if key.name == keys::BLOCKQUOTE_BAR_SPRITE.name {
        &s.blockquote_bar
    } else if key.name == keys::RULE_SPRITE.name {
        &s.rule
    } else {
        panic!(
            "sprite key {:?} has no slot in this map — add it, or the sweep silently \
             stops covering it",
            key.name
        )
    }
}

/// **A themed bar width must not clip its own sprite tile** (ScrAP-324's lesson: where
/// a feature degrades silently, a guard must inspect what the INPUT said).
///
/// `blockquote_bar_sprite` and `blockquote_bar_width` are coupled in prose — in
/// `data/themes.toml` and in SCHEMA's Blockquote table — and by nothing else. Redraw
/// the plate wider, or drop the width while keeping the sprite, and the bar renders a
/// CLIPPED SLICE of a tile: a decoration that is present but wrong, which is worse than
/// this vocabulary's usual inert-by-default failure and produces no log line at all.
///
/// Driven off the shipped file with each theme's own reference in hand, so every future
/// theme naming a bar sprite is covered by having named one.
#[test]
fn a_shipped_bar_width_is_never_narrower_than_the_sprite_it_tiles() {
    let raw: toml::Value =
        toml::from_str(BUILTIN_THEMES_TOML).expect("the shipped themes file must parse");
    let mut checked = 0usize;
    for (id, block) in raw["themes"].as_table().expect("a themes table") {
        let table = block.as_table().expect("a theme block");
        let Some(rel) = table
            .get(keys::BLOCKQUOTE_BAR_SPRITE.name)
            .and_then(toml::Value::as_str)
        else {
            continue;
        };
        let sprite = crate::sprite::builtin(rel)
            .unwrap_or_else(|| panic!("theme {id:?} names sprite {rel:?}, not compiled in"));
        let tex = crate::sprite::texture(&sprite)
            .unwrap_or_else(|| panic!("theme {id:?}: sprite {rel:?} did not decode"));
        let tile_width = gtk::prelude::TextureExt::width(&tex);
        let bar = Themes::builtin().resolve(id).metrics.blockquote_bar_width;
        checked += 1;
        assert!(
            bar >= tile_width,
            "theme {id:?}: blockquote_bar_width is {bar} px but {rel:?} is \
             {tile_width} px wide, so the bar renders a clipped slice of the tile"
        );
    }
    assert!(
        checked > 0,
        "no shipped theme names a blockquote bar sprite — this guard is vacuous"
    );
}

/// **The sprite set on disk and the sprite set in the binary are the same set.**
///
/// The three packaging scripts ship `data/sprites/` wholesale; `crate::sprite::BUILTIN_SPRITES`
/// compiles the same files in. Nothing compared the two, and every way they can disagree is
/// silent — an unresolved sprite is inert by design (ScrAP-324), so the only symptom is a
/// decoration that is quietly flat on an installed copy.
///
/// **Direction 1 — a file no key names.** A sprite dropped into `data/sprites/` is shipped by
/// every packaging script the moment it lands there, with no edit; if nobody also adds it to
/// `BUILTIN_SPRITES` it exists on disk and not in the binary, and `builtin()` returns `None`
/// here. This is the direction packaging can open on its own, which is why it is the one that
/// needed a guard.
///
/// **Direction 2 — a compiled-in key that is not its own file.** The byte comparison fails when
/// a key's spelling and its `include_bytes!` path drift apart. The other half of that direction —
/// a key naming a file that is absent altogether — is refused by `include_bytes!` at compile
/// time and needs no runtime assertion.
///
/// Anchored to `CARGO_MANIFEST_DIR`: the working directory a test binary is launched from is not
/// a property this guard may rest on.
#[test]
fn every_sprite_on_disk_is_compiled_in_with_the_same_bytes() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/sprites");
    let mut checked = 0usize;
    for entry in std::fs::read_dir(&dir).expect("data/sprites/ must exist") {
        let path = entry.expect("a readable directory entry").path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // The packaging scripts survive a subdirectory rather than aborting on one; the key
        // space is flat, so a subdirectory would be shipped and nameable by nothing. Fail here,
        // where a human reads it, instead of at install time where nobody does.
        assert!(path.is_file(), "data/sprites/{name} is not a plain file");
        let key = format!("sprites/{name}");
        let compiled = crate::sprite::builtin(&key).unwrap_or_else(|| {
            panic!(
                "data/sprites/{name} is shipped by every packaging script but is not in \
                 crate::sprite::BUILTIN_SPRITES — add it there with include_bytes!, or the \
                 installed themes.toml names a sprite this binary cannot fall back to"
            )
        });
        let embedded = crate::sprite::bytes(&compiled).expect("compiled-in bytes");
        let on_disk = std::fs::read(&path).expect("the file reads");
        assert_eq!(
            &embedded[..],
            &on_disk[..],
            "{key} is compiled in, but not from the file of that name"
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "data/sprites/ is empty — this guard is vacuous"
    );
}
