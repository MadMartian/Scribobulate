//! TDD 18.2 — the regression bar: what the shipped `[themes.system]` block promises,
//! and what a hostile or malformed theme cannot do to it.

use super::super::keys;
use super::super::model::*;
use super::super::*;

fn builtin_system() -> ThemeSpec {
    Themes::parse_compiled(BUILTIN_THEMES_TOML)
        .expect("shipped themes.toml must parse")
        .get(SYSTEM_ID)
        .cloned()
        .expect("shipped themes.toml must define [themes.system]")
}

#[test]
fn builtin_parses_and_ships_the_two_themes() {
    let t = Themes::builtin();
    assert!(t.contains("system"));
    assert!(t.contains("sepia"));
}

/// **Every floor equals the shipped `[themes.system]` value — proved for all of them
/// at once, without a list.**
///
/// Resolve `[themes.system]` twice: once against the shipped `data/themes.toml` and
/// once against an EMPTY spec, where nothing is stated so every value can only be its
/// key's own floor. If the two `Theme`s are equal, every floor matches its shipped
/// value; if any one drifts, they are not.
///
/// This replaces a hand-written list of 19 assertions guarding **22** declared floors,
/// whose docstring — and `resolve.rs`, twice, in identical words — claimed it covered
/// *each one*. The three it missed were the ones that matter most:
/// `link_underline`'s (whose own comment explains it is `Single` and not `None`
/// "because changing it would move System", the byte-identical rendering guarantee of
/// TDD 18.2 — so `themes.toml` could have been changed to `"none"` with every System
/// regression test still passing), `heading_band_padding`'s (the one floor flagged as
/// exceptional, i.e. the one most likely to be "corrected" in one place and not the
/// other), and `heading_band_radius`, which was **absent from `[themes.system]`
/// entirely**, so for that key the floor genuinely WAS a second source of truth.
///
/// The list form could not have been fixed by lengthening it: the next key added would
/// have been the next one missing. One assertion with no list cannot be incomplete.
#[test]
fn every_floor_equals_the_shipped_system_value() {
    // Both spellings mirror production: `Themes::resolve` passes the SAME spec as
    // `selected` and `system` for this id, which is what deleting the carve-out made
    // true (see `Themes::resolve`).
    let sys = builtin_system();
    let shipped = Theme::resolve(SYSTEM_ID, &sys, &sys);
    let empty = ThemeSpec::default();
    let floors = Theme::resolve(SYSTEM_ID, &empty, &empty);
    // `name` and `symbol` are the two fields that legitimately differ: both are the
    // theme's own IDENTITY rather than a styling value with a floor, so an empty spec
    // carries neither. Everything else must match.
    assert_eq!(shipped.name, "System");
    assert_eq!(floors.name, SYSTEM_ID);
    assert!(
        shipped.symbol.is_some(),
        "the shipped system theme has a symbol"
    );
    assert!(
        floors.symbol.is_none(),
        "identity has no floor to fall back to"
    );
    let normalised = Theme {
        name: shipped.name.clone(),
        symbol: shipped.symbol.clone(),
        ..floors
    };
    assert_eq!(
        shipped, normalised,
        "a floor has drifted from its shipped [themes.system] value. The data file is \
         the source of truth a human reads and edits; the floor exists only to keep \
         resolution total, so change data/themes.toml and let the floor follow"
    );
}

/// Anti-vacuity for the guard above: it must be able to SEE a drift.
///
/// A test that compares two resolutions passes trivially if either operand stopped
/// depending on what it claims to. Perturbing one shipped value and asserting the two
/// now differ is what proves the comparison is live — without it, an accessor that
/// silently ignored the shipped spec would leave the guard permanently green.
#[test]
fn the_floor_guard_can_actually_see_a_drift() {
    let mut spec = builtin_system();
    let empty = ThemeSpec::default();
    let shipped = Theme::resolve(SYSTEM_ID, &spec, &spec);
    let floors = Theme::resolve(SYSTEM_ID, &empty, &empty);
    assert_eq!(shipped.metrics.list_step, floors.metrics.list_step);
    // A value the shipped file does state, moved off its floor.
    spec.overlay(
        Themes::parse_compiled("[themes.system]\nlist_step = 31\n")
            .expect("fixture parses")
            .get(SYSTEM_ID)
            .cloned()
            .expect("fixture defines [themes.system]"),
    );
    let moved = Theme::resolve(SYSTEM_ID, &spec, &spec);
    assert_ne!(
        moved.metrics.list_step, floors.metrics.list_step,
        "the guard compares the shipped spec against the floors; if moving a shipped \
         value does not move the resolution, it is comparing something else"
    );
    assert_ne!(moved, floors);
}

/// **TDD 18.1 — the two chooser surfaces always show the same choice**, including for
/// a theme that states no symbol of its own.
///
/// The two label paths are `Themes::chooser_list` (the menu, which reads each spec's
/// `own_text`) and the resolved `Theme::symbol` (the toolbar button, via
/// `window::actions::refresh_theme_button`). `symbol` used to take the two-source
/// walk while `name` beside it used `own_text`, so a symbol-less theme inherited
/// `[themes.system]`'s window glyph on ONE of the two surfaces: the menu offered
/// "Slate" and the button read "🪟 Slate".
///
/// **All seven shipped themes state a symbol, which is why this was latent** — and
/// why it bit exactly the case TDD 18.14 exists for, a theme added as data.
#[test]
fn a_symbol_less_theme_reads_the_same_on_both_chooser_surfaces() {
    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled("[themes.slate]\nname = \"Slate\"\nbackground = \"#222222\"\n")
            .expect("fixture parses"),
    );
    let resolved = themes.resolve("slate");
    assert_eq!(resolved.name, "Slate");
    assert_eq!(
        resolved.symbol, None,
        "a theme's picker symbol is its OWN; inheriting the base theme's puts a \
         window glyph on a theme that never asked for one"
    );
    let listed = themes
        .chooser_list()
        .into_iter()
        .find(|e| e.id == "slate")
        .expect("the merged theme appears in the chooser");
    assert_eq!(
        Themes::chooser_label(&listed.label, listed.symbol.as_deref()),
        Themes::chooser_label(&resolved.name, resolved.symbol.as_deref()),
        "the menu and the toolbar button must render one label"
    );

    // Anti-vacuity: a theme that DOES state a symbol still carries it on both, so the
    // agreement above is not "both paths return nothing".
    let sys_resolved = themes.resolve(SYSTEM_ID);
    let sys_listed = themes
        .chooser_list()
        .into_iter()
        .next()
        .expect("System leads the chooser");
    assert!(sys_resolved.symbol.is_some());
    assert_eq!(
        Themes::chooser_label(&sys_listed.label, sys_listed.symbol.as_deref()),
        Themes::chooser_label(&sys_resolved.name, sys_resolved.symbol.as_deref())
    );
}

/// The property the two deleted `[themes.system]` carve-outs were protecting between
/// them, pinned so neither can come back and so a future edit cannot lose it.
///
/// `Themes::resolve` used to blank `selected` for this id, and `Theme::resolve` had a
/// compensating `id == SYSTEM_ID` branch in a **different file** to put the name back.
/// Deleting either alone made the system theme silently become `"system"` in every
/// picker; nothing linked them and no test covered the coupling.
#[test]
fn the_system_theme_keeps_its_own_display_name_with_no_carve_out() {
    let themes = Themes::builtin();
    assert_eq!(themes.resolve(SYSTEM_ID).name, "System");
    // Through the OTHER surface too, which is the one the old test used and which
    // touched neither branch.
    assert_eq!(themes.chooser_list()[0].label, "System");
    // And an id nobody ships still falls back to the id itself rather than to
    // "System" — the fallback and the system theme's name are different mechanisms.
    assert_eq!(themes.resolve("no-such-theme").name, "no-such-theme");
}

/// TDD 18.2 — the regression bar. System must inject NO base colour, so every
/// one of them falls through to the desktop probe exactly as before theming.
#[test]
fn system_theme_injects_no_base_colour_and_no_font() {
    let t = Themes::builtin().resolve(SYSTEM_ID);
    assert_eq!(t.id, SYSTEM_ID);
    assert!(t.background.is_none());
    assert!(t.foreground.is_none());
    assert!(t.accent_color.is_none());
    assert!(t.font_family.is_none());
    assert!(t.syntect_theme.is_none());
    assert!(t.heading_color.is_none());
    assert!(t.list_marker_color.is_none());
    assert!(t.link_color.is_none());
    assert!(t.code_inline_bg.is_none());
    assert!(t.blockquote_bar_color.is_none());
}

/// TDD 18.11 — out-of-range geometry clamps rather than breaking layout.
#[test]
fn hostile_geometry_is_clamped() {
    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled(
            "[themes.evil]\nlist_step = -5\nblockquote_bar_width = 10000\n\
                 heading_weight = 99999\nsupsub_scale = -3.0\nsuperscript_rise = 9999\n",
        )
        .unwrap(),
    );
    // Each range is read off the key that owns it, not off a constant this test
    // names — the pairing of key to range is exactly what used to be hand-made at
    // the resolution site and is now the registry's job.
    let t = themes.resolve("evil");
    assert_eq!(t.metrics.list_step, keys::LIST_STEP.bound.int_range().min);
    assert_eq!(
        t.metrics.blockquote_bar_width,
        keys::BLOCKQUOTE_BAR_WIDTH.bound.int_range().max
    );
    assert_eq!(
        t.typography.heading_weight,
        [keys::HEADING_WEIGHT.bound.int_range().max; HEADING_LEVELS]
    );
    assert_eq!(
        t.typography.supsub_scale,
        keys::SUPSUB_SCALE.bound.float_range().min
    );
    assert_eq!(
        t.typography.superscript_rise,
        keys::SUPERSCRIPT_RISE.bound.int_range().max
    );
}

/// A theme file cannot kill the app with a value of the wrong shape.
///
/// The vocabulary carries no arrays at all now, so the length hazard this once
/// guarded is gone by construction; what remains is the two ways a scalar can be
/// wrong. A value of the wrong TYPE costs its own key and nothing else — the file
/// still loads and every other key in that theme applies — and a non-finite float
/// is clamped rather than propagated into Pango.
#[test]
fn a_malformed_value_costs_its_own_key_and_never_the_theme() {
    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled(
            "[themes.wrong]\nheading_scale_h1 = \"enormous\"\nheading_scale_h2 = 2.0\n\
                 [themes.nan]\nheading_scale_h1 = nan\nheading_scale_h2 = inf\n",
        )
        .unwrap(),
    );
    // The refused key falls back to its floor; its neighbour is untouched.
    let wrong = themes.resolve("wrong").typography.heading_scale;
    assert_eq!(wrong[0], keys::HEADING_SCALE.bound.float_floor(0));
    assert_eq!(wrong[1], 2.0);
    // Non-finite: clamped, never propagated into Pango.
    let n = themes.resolve("nan").typography.heading_scale;
    assert!(n.iter().all(|x| x.is_finite()));
    // A level nobody states keeps the system hierarchy, which is what the short
    // array used to buy by extending from the floor.
    let floor: Vec<f64> = (0..HEADING_LEVELS)
        .map(|i| keys::HEADING_SCALE.bound.float_floor(i))
        .collect();
    assert_eq!(&n[2..], &floor[2..]);
}

/// TDD 18.11 — a colour cannot escape a generated CSS rule, because it is
/// re-emitted from a parsed RGBA rather than echoed.
#[test]
fn a_hostile_colour_string_cannot_inject_css() {
    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled("[themes.evil]\nbackground = \"#fff; } * { color: red; }\"\n")
            .unwrap(),
    );
    // Unparseable → falls through to the desktop probe; nothing is interpolated.
    assert!(themes.resolve("evil").background.is_none());
}

/// A stale persisted selection (a theme the user deleted) must degrade to the
/// default, not fail.
#[test]
fn an_unknown_theme_id_resolves_as_system() {
    let t = Themes::builtin().resolve("no-such-theme");
    assert!(t.background.is_none());
    assert_eq!(
        t.typography.heading_scale,
        std::array::from_fn::<f64, HEADING_LEVELS, _>(|i| keys::HEADING_SCALE.bound.float_floor(i))
    );
}

/// TDD 18.1 — System leads the chooser; the rest follow by display name.
#[test]
fn chooser_lists_system_first() {
    let list = Themes::builtin().chooser_list();
    assert_eq!(list[0].id, SYSTEM_ID);
    assert_eq!(list[0].label, "System");
    assert!(list.iter().any(|e| e.id == "sepia" && e.label == "Sepia"));
}

/// **A gradient's far stop must not silently drift from the page it fades into.**
///
/// Every shipped gradient stop restates its own theme's `background` — Synthwave's
/// `#1a1033`, Candy's `#101a4d` — and the file's own prose says why: *"the gradient runs
/// from each level's own fill down to the deep indigo of the page, so the band dissolves
/// into the page instead of ending on a hard edge"*. Nothing linked the two hexes, so
/// re-tinting a theme's page left its bands ending on a hard edge against the old one,
/// with no gate and no log line.
///
/// **It covers every gradient key, not the heading band's.** The hazard belongs to the
/// SHAPE — a second stop that names a colour it must keep agreeing with — so the sweep
/// is driven off the registry rather than off a list of keys, and the disclosure band's
/// stop (TDD 18.48) was covered by this guard the moment it was declared. The
/// discriminator is the spelling, because the vocabulary has no gradient *kind* for the
/// registry to expose; a future stop key spelled otherwise would need adding here, which
/// is the one thing this derivation cannot catch for itself.
///
/// **This is a drift guard, not a rule that a gradient must end at the page.** A future
/// theme may legitimately fade somewhere else; the point is that doing so becomes a
/// deliberate edit to this test with a reason attached, where today the divergence is
/// invisible. The alternative — making the value DERIVABLE from `background` — needs a
/// cross-key reference grammar in `themes.toml`, which is a vocabulary change and not
/// this guard's to make. That edit has since been made once — see [`OFF_PAGE`], which is
/// where the reason lives, exactly as the assert message below instructs.
#[test]
fn a_shipped_bands_gradient_ends_on_that_themes_own_page() {
    let raw: toml::Value =
        toml::from_str(BUILTIN_THEMES_TOML).expect("the shipped themes file must parse");
    let mut checked = 0usize;
    for (id, block) in raw["themes"].as_table().expect("a themes table") {
        let table = block.as_table().expect("a theme block");
        let Some(page) = table.get("background").and_then(toml::Value::as_str) else {
            continue;
        };
        // Every spelling every gradient key CLAIMS — the bare form and each `_hN` —
        // walked off the block rather than generated, because `Key::spelling` never
        // yields the bare form for a levelled key and the bare form is what the shipped
        // themes actually write.
        for (spelling, value) in table {
            if !keys::KEYS
                .iter()
                .filter(|k| k.name.ends_with("_gradient_to_color"))
                .any(|k| k.claims(spelling))
            {
                continue;
            }
            let Some(far) = value.as_str() else { continue };
            if OFF_PAGE
                .iter()
                .any(|(theme, key, ..)| *theme == id.as_str() && *key == spelling.as_str())
            {
                continue;
            }
            checked += 1;
            assert_eq!(
                far.to_ascii_lowercase(),
                page.to_ascii_lowercase(),
                "theme {id:?}: {spelling} is {far} but the page is {page}, so this \
                 theme's bands end on a hard edge against a colour the page no longer \
                 has. Either follow the page, or name it in OFF_PAGE and say why."
            );
        }
    }
    assert!(
        checked > 0,
        "no shipped theme states a band gradient — this guard is vacuous"
    );
}

/// Bands that deliberately do NOT fade to their theme's page, with the reason each one
/// is worth the hard edge the guard above otherwise forbids.
///
/// Keyed by `(theme id, the key's exact spelling, the EXACT value the licence was
/// argued for, why)`. A spelling rather than a key name, because the levelled heading
/// keys claim several and an exemption should licence the one band it was argued for
/// rather than all five.
///
/// **The value column is the point** (F-AP-B-301). Exempting a key from the
/// "ends on its own page" guard used to exempt it from EVERY statement about its value:
/// the three stops moved in the same commit that licensed them, and nothing was left
/// that could tell. One value per row restores exact pinning for the licensed stops
/// with the reason still attached, which makes moving one a deliberate edit a reviewer
/// sees in the diff rather than a silent retune.
const OFF_PAGE: &[(&str, &str, &str, &str)] = &[
    (
        "candy",
        "disclosure_band_gradient_to_color",
        "#243e00",
        "operator-directed: the fold is banded in the theme's own confection hues — deep \
         raspberry running to deep lime — rather than in a lifted page surface, so a candy \
         wrapper reads across the summary line instead of a shelf. The hard edge is accepted \
         as the cost of that, and the pair is legible at BOTH stops, which is the property \
         that actually protects the reader (see data/themes.toml's `disclosure_band_color` \
         for why the requested full-strength hues could not be used)",
    ),
    (
        "candy",
        "heading_band_gradient_to_color_h1",
        "#116364",
        "operator-directed: the h1 band runs grape → turquoise, resolving into a second hue \
         rather than dissolving into the page, so a title reads as a banner with somewhere to \
         go. Licences the _h1 spelling rather than the bare key because this theme states no \
         bare one — it bands exactly h1 and h2 and narrows both (see data/themes.toml for why, \
         and for why the turquoise had to be tinted)",
    ),
    (
        "candy",
        "heading_band_gradient_to_color_h2",
        "#0d6811",
        "operator-directed, and the h1 entry's pair: h2 runs its hot-pink shade → green, tinted \
         to the SAME value as h1's turquoise so the two bands read as one decision at two hues. \
         The requested #149c1a is unusable undiluted — 2.88:1 under the lemon ink — which is the \
         luminance law, not a rejection of the hue",
    ),
];

/// Every off-page licence is still standing over the EXACT band it was argued for.
///
/// Same discipline as the contrast sweep's `every_deliberate_exception_is_still_below_its_floor`,
/// and for the same reason: a theme that has since been retuned to end on its page leaves
/// an exemption over nothing, and the next band to take that spelling inherits it in
/// silence — which is precisely the invisibility the guard above exists to end.
///
/// **It asserts the value, not merely that the value differs from the page.** The weaker
/// form let all three of Candy's stops move in the commit that licensed them with nothing
/// able to notice, which is the licence swallowing the guard rather than narrowing it
/// (F-AP-B-301). Every ratio quoted in a `why` below was measured against the value in
/// its own row; moving the stop without re-deriving them is what this now refuses.
#[test]
fn every_off_page_licence_still_covers_an_off_page_band() {
    let raw: toml::Value =
        toml::from_str(BUILTIN_THEMES_TOML).expect("the shipped themes file must parse");
    let themes = raw["themes"].as_table().expect("a themes table");
    for (id, key, licensed, why) in OFF_PAGE {
        let block = themes
            .get(*id)
            .and_then(toml::Value::as_table)
            .unwrap_or_else(|| panic!("{id}: no such shipped theme, so the licence is stale"));
        let page = block
            .get("background")
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| panic!("{id}: states no page of its own ({why})"));
        let far = block
            .get(*key)
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| panic!("{id}: states no {key}, so the licence is stale ({why})"));
        assert_ne!(
            far.to_ascii_lowercase(),
            page.to_ascii_lowercase(),
            "{id}: {key} now ends on the page after all — delete the licence rather than \
             leaving one standing over nothing ({why})"
        );
        assert_eq!(
            far.to_ascii_lowercase(),
            licensed.to_ascii_lowercase(),
            "{id}: {key} is {far}, but the licence was argued for {licensed} and every \
             contrast ratio in its reason was measured against that value. Re-derive \
             the ratios and update BOTH, or put the stop back ({why})"
        );
    }
}
