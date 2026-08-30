use crate::config::config;
use crate::renderer::syntect;
use gtk::gdk;
use gtk::prelude::*;

/// Convert a syntect highlighting color (0–255 channels) to a GDK RGBA.
fn syntect_color_to_rgba(c: syntect::highlighting::Color) -> gdk::RGBA {
    gdk::RGBA::new(
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        1.0,
    )
}

pub(crate) fn luminance(c: gdk::RGBA) -> f64 {
    let lin = |x: f32| -> f64 {
        let x = x as f64;
        if x <= 0.03928 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.red()) + 0.7152 * lin(c.green()) + 0.0722 * lin(c.blue())
}

pub(crate) fn contrast(a: gdk::RGBA, b: gdk::RGBA) -> f64 {
    let (la, lb) = (luminance(a) + 0.05, luminance(b) + 0.05);
    if la > lb {
        la / lb
    } else {
        lb / la
    }
}

pub(crate) fn mix_rgba(a: gdk::RGBA, b: gdk::RGBA, t: f64) -> gdk::RGBA {
    gdk::RGBA::new(
        (a.red() as f64 * (1.0 - t) + b.red() as f64 * t) as f32,
        (a.green() as f64 * (1.0 - t) + b.green() as f64 * t) as f32,
        (a.blue() as f64 * (1.0 - t) + b.blue() as f64 * t) as f32,
        1.0,
    )
}

/// How far a syntax theme's own background must sit from the page before it is used
/// as the code panel. Below this the block would wash into the document, so a deeper
/// fg-mix is used instead — see [`Palette::from_base`].
const CODE_PANEL_MIN_CONTRAST: f64 = 1.08;

/// The luminance at which a surface counts as DARK. One threshold for every such
/// decision here: the desktop probe's ink/page pairing, the page-lightness branch
/// `from_base` picks a syntax palette with, and the direction [`walk_to_contrast`]
/// walks in.
const DARK_SURFACE_LUMINANCE: f64 = 0.5;

/// WCAG 2.1 AA's contrast floor for body-sized text (SC 1.4.3).
///
/// `pub(crate)` because the theme engine's legibility gates hold every ink a theme
/// *states* to the same floor this module walks every ink it *derives* up to. One
/// constant, so the two halves cannot drift apart.
pub(crate) const WCAG_AA_TEXT: f64 = 4.5;

/// How far one step of the contrast walk moves the ink toward its target, and how many
/// steps it may take. Twenty tenths leaves the ink ~88% of the way to pure white or
/// black, which clears AA against any fill; the cap is what makes a pathological
/// pairing terminate instead of looping.
const WALK_STEP: f64 = 0.1;
const WALK_STEPS: usize = 20;

/// Walk `ink` toward white or black — whichever `fill` is farther from — until it
/// clears [`WCAG_AA_TEXT`] against `fill`, or the walk runs out of steps. An ink that
/// already clears the floor is returned untouched.
///
/// **The only WCAG walk in this module.** It was written out twice, once for the link
/// colour and once for the selection ink, each with its own copy of the floor, the step
/// size and the step count — so either could be corrected without the other, which is
/// the whole of finding `F-WCAG-001`.
fn walk_to_contrast(ink: gdk::RGBA, fill: gdk::RGBA) -> gdk::RGBA {
    let target = if luminance(fill) < DARK_SURFACE_LUMINANCE {
        gdk::RGBA::WHITE
    } else {
        gdk::RGBA::BLACK
    };
    let mut ink = ink;
    for _ in 0..WALK_STEPS {
        if contrast(ink, fill) >= WCAG_AA_TEXT {
            break;
        }
        ink = mix_rgba(ink, target, WALK_STEP);
    }
    ink
}

/// `#rrggbb` — the colour's three channels, **alpha discarded**.
///
/// Named for what it does rather than for what it is, because the unnamed version was
/// the whole of `F-ALPHA-001`: every colour key in this vocabulary parses
/// `#RRGGBBAA` (SCHEMA § Key naming), two shipped defaults are translucent
/// (`mark_bg = #fff59d_88`, `annotation_hl_color = #FFD133_61`), and a translucent wash
/// is the natural authoring choice for `blockquote_bg` — "a panel behind quoted text".
/// A `to_hex` that silently dropped the alpha at 41 call sites made
/// `blockquote_bg = "#0a183080"` a translucent wash in the preview and a **solid navy
/// block** in both exports, with nothing warning and the reader seeing three different
/// documents.
///
/// Right for a `Palette`-derived colour — `Palette::from_base` has already composited
/// those against the page, so their alpha is 1.0 by construction and saying "opaque"
/// out loud costs nothing. Wrong for a colour that came straight off a theme key; use
/// [`to_hex_rgba`].
pub(crate) fn to_hex_opaque(c: gdk::RGBA) -> String {
    let ch = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        ch(c.red()),
        ch(c.green()),
        ch(c.blue())
    )
}

/// `#rrggbb` for an opaque colour, `#rrggbbaa` for a translucent one.
///
/// Eight-digit hex is CSS Color 4 and is supported by every browser this artefact
/// targets. The opaque case is spelled in six digits deliberately: it keeps a theme
/// that states no alpha producing byte-identical output to before this function existed
/// (TDD 18.2), so the only sheets that change are the ones that were wrong.
pub(crate) fn to_hex_rgba(c: gdk::RGBA) -> String {
    let ch = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    let a = ch(c.alpha());
    match a {
        0xff => to_hex_opaque(c),
        _ => format!(
            "#{:02x}{:02x}{:02x}{a:02x}",
            ch(c.red()),
            ch(c.green()),
            ch(c.blue())
        ),
    }
}

/// The preview's resolved colours. Every field is a concrete colour — link 3 of
/// the resolution order (the desktop probe + derivation) has already run by the
/// time a `Palette` exists.
pub(crate) struct Palette {
    /// The page background. Under the system theme this is the desktop's own base
    /// colour and reaches the widget through generated CSS like any other theme's;
    /// a named theme injects it (`theme.background`).
    pub(crate) page_bg: gdk::RGBA,
    /// The body foreground.
    pub(crate) body_fg: gdk::RGBA,
    pub(crate) code_inline_bg: gdk::RGBA,
    pub(crate) code_block_bg: gdk::RGBA,
    pub(crate) link_fg: gdk::RGBA,
    /// Colour of the blockquote's left accent bar (drawn by the preview view, not a
    /// widget — blockquotes live in the buffer as text now, GTK4Rs/AP-23/GTK4Rs/AP-24 family).
    pub(crate) blockquote_bar: gdk::RGBA,
    /// The selection / accent tint the preview's floating chrome derives from.
    pub(crate) selection_bg: gdk::RGBA,
    /// The ink SELECTED text is drawn in, over [`Self::selection_bg`].
    ///
    /// This is derived, not stated: styling the selection's background alone leaves
    /// its foreground to the desktop GTK theme, which is the one ink on the page that
    /// the reading theme does not own. Measured on Bedtime (sand ink on a neutral
    /// grey page): selecting text painted every glyph — body, headings, code alike —
    /// pure `#000000` at 2.1:1 on the selection fill, because the desktop's
    /// `theme_selected_fg_color` won. A themed page therefore has to state this
    /// colour as well as the fill, or the theme ends at the moment a reader drags
    /// across a paragraph.
    pub(crate) selection_fg: gdk::RGBA,
    pub(crate) table_border: gdk::RGBA,
    pub(crate) table_head_bg: gdk::RGBA,
    /// The horizontal rule's line colour (an anchored `GtkSeparator`, so it is
    /// styled by generated CSS rather than a tag).
    pub(crate) rule: gdk::RGBA,
    pub(crate) syntect_theme: String,
}

// NOTE: `Palette` deliberately carries no `is_dark`. It used to, and the editor's
// GtkSourceView style scheme read it — which was correct only while the preview's
// page WAS the desktop's. Under a reading theme the page's lightness is a property
// of the THEME (Sepia's page is light on any desktop), so that field would have
// flipped the editor to a light scheme whenever Sepia was selected, and the reading
// theme is preview-only (TDD 18.7). Anything outside the preview that needs the
// desktop's lightness calls [`desktop_is_dark`]; inside `from_base`, the page's own
// lightness is a local, used to pick the derivations tuned for it.

/// The colours resolution link 3 answers with when the desktop GTK theme defines none
/// of the names asked for — the `floor` argument of [`probe_named`]. Named rather than
/// spelled at each probe, because they were literals scattered across the four probe
/// sites and a correction to one silently left the others behind.
const BODY_INK_FLOOR: gdk::RGBA = gdk::RGBA::new(0.067, 0.067, 0.067, 1.0); // #111111
/// Adwaita's chrome ink, distinct from its body ink — see [`Palette::from_base`].
const CHROME_INK_FLOOR: gdk::RGBA = gdk::RGBA::new(0.180, 0.204, 0.212, 1.0); // #2e3436
const ACCENT_FLOOR: gdk::RGBA = gdk::RGBA::new(0.208, 0.518, 0.894, 1.0); // #3584e4
const DARK_PAGE_FLOOR: gdk::RGBA = gdk::RGBA::new(0.118, 0.118, 0.118, 1.0); // #1e1e1e
const LIGHT_PAGE_FLOOR: gdk::RGBA = gdk::RGBA::WHITE;

/// The desktop theme's named colours, in the order each probe asks for them; the first
/// the theme defines wins. One list per role, and the list IS the chain — a change to
/// a probe's order is an edit to one array.
const BODY_INK_NAMES: &[&str] = &["theme_text_color", "theme_fg_color"];
/// The CHROME ink chain: the body chain reversed, so a theme that names only one of the
/// pair still answers both roles with it.
const CHROME_INK_NAMES: &[&str] = &["theme_fg_color", "theme_text_color"];
const PAGE_NAMES: &[&str] = &["theme_base_color", "view_bg_color", "theme_bg_color"];
const ACCENT_NAMES: &[&str] = &["theme_selected_bg_color", "accent_bg_color"];

/// The base an exported page resolves against when its theme states no page of its
/// own — see [`Palette::for_paper`]. White page, near-black ink, and the same default
/// accent the desktop probe falls back to, so an export on a machine with no theme
/// colours at all looks like an export on one that has them — stated by *sharing* the
/// probe's floors rather than by restating their values.
///
/// These sit here rather than in an export sink deliberately: this module is where the
/// light/dark resolution floor already lives, and keeping them together is what lets
/// POLICY's "no hard-coded styling" rule hold for every renderer — none of them names a
/// colour, they ask this module for one.
const PAPER_BG: gdk::RGBA = LIGHT_PAGE_FLOOR;
const PAPER_FG: gdk::RGBA = BODY_INK_FLOOR;
const PAPER_ACCENT: gdk::RGBA = ACCENT_FLOOR;

/// The annotation chip's pre-theming appearance — the amber pill and the ink of the
/// count numeral on it, exactly as `codeview` drew them before `annotation_chip_bg` /
/// `annotation_chip_fg` existed, so a theme stating neither renders byte-identically
/// (TDD 18.2).
///
/// Here rather than at the paint site because this module's contract is that **none of
/// the renderers names a colour** — they ask this module for one. `codeview` was the
/// standing exception, with three literals in a `snapshot` body.
pub(crate) const ANNOTATION_CHIP_FLOOR: gdk::RGBA = gdk::RGBA::new(0.90, 0.62, 0.10, 0.95);
/// The ink on [`ANNOTATION_CHIP_FLOOR`] — white, for the collapsed-count numeral.
pub(crate) const ANNOTATION_CHIP_INK_FLOOR: gdk::RGBA = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);

/// The desktop's accent, through the SAME chain and floor `Palette::resolve_for` uses.
///
/// A fifth probe site (the hovered task checkbox's border) open-coded this chain and
/// re-spelled `ACCENT_FLOOR` as a literal — which is `F-PROBE-001`'s defect at a site
/// outside `palette/`, where the fix for the other four could not reach it. Anything
/// that wants "the desktop's accent" and is not building a `Palette` calls this.
pub(crate) fn desktop_accent() -> gdk::RGBA {
    probe_named(ACCENT_NAMES, ACCENT_FLOOR)
}

/// **Resolution link 3, spelled once.** Ask the desktop GTK theme for the first of
/// `names` it defines; answer with `floor` when it defines none of them.
///
/// The chain and its floor were written out at four probe sites, so a change to either
/// had to land in all four (finding `F-PROBE-001`). This function owns the "none of
/// them names a colour" contract, so the floor is not optional and no caller can forget
/// one.
///
/// The probe reads named colours through a temporary widget's style context. Theme CSS
/// is loaded for the default display by `connect_startup`, so `lookup_color` answers
/// even on an unparented widget.
#[allow(deprecated)] // style_context() deprecated in GTK ≥ 4.10; no stable alternative yet
fn probe_named(names: &[&str], floor: gdk::RGBA) -> gdk::RGBA {
    let probe = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let ctx = probe.style_context();
    names
        .iter()
        .find_map(|name| ctx.lookup_color(name))
        .unwrap_or(floor)
}

/// The page floor to pair with an ink the probe *did* answer with: the opposite
/// lightness, so a desktop that names an ink but no page still yields a legible pair.
fn page_floor_for_ink(ink: gdk::RGBA) -> gdk::RGBA {
    if luminance(ink) > DARK_SURFACE_LUMINANCE {
        DARK_PAGE_FLOOR
    } else {
        LIGHT_PAGE_FLOOR
    }
}

/// Whether the DESKTOP theme is dark, independent of the active reading theme.
/// The editor pane, the toolbar, the tab strip and the outline sidebar all stay on
/// the desktop theme, so this — not `Palette::is_dark` — is what they follow.
pub(crate) fn desktop_is_dark() -> bool {
    let ink = probe_named(BODY_INK_NAMES, BODY_INK_FLOOR);
    let page = probe_named(PAGE_NAMES, page_floor_for_ink(ink));
    luminance(page) < DARK_SURFACE_LUMINANCE
}

impl Palette {
    /// Derive the whole palette from the base colours plus a theme's explicit
    /// overrides. **Pure** — no display, no probe — so every derivation here is unit
    /// testable headlessly, and a named theme gets the same derivation the desktop
    /// gets for free, simply by injecting different bases (TDD 18.4).
    ///
    /// This is the split the theming engine needed: the derivation was always a
    /// pure function of the bases, it was just welded to the GTK probe.
    /// [`Palette::resolve`] is now only that probe, and is the system theme's
    /// implementation.
    ///
    /// **Two foregrounds, deliberately.** `fg` is the BODY ink (`theme_text_color` —
    /// what the document's text is drawn in); `chrome_fg` is the CHROME ink
    /// (`theme_fg_color` — what the app's furniture is drawn in). GTK genuinely
    /// distinguishes them (Adwaita: `#000000` vs `#2e3436`), and so did this app
    /// before theming — the table cells' CSS asked for `alpha(@theme_fg_color, …)`
    /// while the text tags used `Palette`'s `theme_text_color`. Collapsing them into
    /// one "foreground" during the theming refactor silently shifted the table
    /// header fill from `#EEEFEF` to `#EBEBEB` under System — invisible in isolation,
    /// but a real regression against the byte-identical bar (TDD 18.2), and an
    /// instance of ScrAP-131 (a refactor that redefines what a value means
    /// keeps compiling everywhere). A named theme states ONE foreground, so both
    /// resolve to it and the distinction costs a theme author nothing.
    pub(crate) fn from_base(
        bg: gdk::RGBA,
        fg: gdk::RGBA,
        chrome_fg: gdk::RGBA,
        accent: gdk::RGBA,
        theme: &crate::theme::Theme,
    ) -> Self {
        let is_dark = luminance(bg) < DARK_SURFACE_LUMINANCE;

        // The derived link colour is the accent walked up to the legibility floor
        // against the page; a theme that states `link_color` overrides it below.
        let link_fg = walk_to_contrast(accent, bg);

        let code_cfg = &config().code;
        // A theme may name its own syntax theme (a string — which is exactly what
        // GTK4 CSS could never have carried, and why themes are TOML). Otherwise
        // follow the page's own lightness, not the desktop's: a light reading theme
        // on a dark desktop still wants a light syntax palette.
        let syntect_theme = theme.syntect_theme.clone().unwrap_or_else(|| {
            if is_dark {
                code_cfg.dark_theme.clone()
            } else {
                code_cfg.light_theme.clone()
            }
        });

        let code_inline_bg = theme
            .code_inline_bg
            .unwrap_or_else(|| mix_rgba(bg, fg, 0.08));

        // Canonical code-block panel: the syntax-highlight theme's own background is
        // the backdrop its token colors are tuned for, so prefer it — but only when
        // it is visibly distinct from the document background.  Some themes (e.g.
        // InspiredGitHub) use a white background identical to a light page, which
        // would wash the block into the document; in that case fall back to a
        // stronger fg-mix than inline code so the block always reads as a panel.
        // (This gate needs no change for a reading theme: Solarized (light)'s
        // #fdf6e3 sits ~1.05 against the sepia page, correctly failing the gate and
        // falling back to a deeper sepia rather than washing into it.)
        let code_block_bg = theme.code_block_bg.unwrap_or_else(|| {
            let (_syntaxes, syntect_themes) = syntect();
            let theme_bg = syntect_themes
                .themes
                .get(&syntect_theme)
                .and_then(|t| t.settings.background)
                .map(syntect_color_to_rgba);
            match theme_bg {
                Some(tb) if contrast(tb, bg) >= CODE_PANEL_MIN_CONTRAST => tb,
                _ => mix_rgba(bg, fg, 0.12),
            }
        });

        // Selected text's own ink — stated by the theme if it cares, else derived.
        //
        // The derivation: the page ink and the page itself are the two colours already
        // proven to belong to this theme, so the selection takes whichever reads better
        // on the fill rather than inventing a third — on a dark fill usually the ink, on
        // a bright one usually the page (Terminal's cyan selection takes black,
        // Synthwave's magenta takes the indigo page). Only if BOTH fail AA does it walk
        // toward white or black, the same escape the link colour above uses.
        //
        // Why the key exists on top of that: the derivation optimises for CONTRAST, and
        // contrast is not taste. Bedtime's sand ink clears 5.3:1 on its violet selection
        // and still looks wrong on it — warm ink on a cool band — so Bedtime states a
        // near-white instead. No ratio would have caught that, which is precisely why
        // the answer has to be statable rather than only computed.
        let selection_bg = theme.selection_bg.unwrap_or(accent);
        let selection_fg = theme.selection_fg.unwrap_or_else(|| {
            let better = if contrast(fg, selection_bg) >= contrast(bg, selection_bg) {
                fg
            } else {
                bg
            };
            walk_to_contrast(better, selection_bg)
        });

        Palette {
            page_bg: bg,
            body_fg: fg,
            code_inline_bg,
            code_block_bg,
            link_fg: theme.link_color.unwrap_or(link_fg),
            blockquote_bar: theme.blockquote_bar_color.unwrap_or(accent),
            selection_bg,
            selection_fg,
            // The table chrome's CSS used alpha(@theme_fg_color, 0.25 / 0.08) — the
            // CHROME ink, not the body ink. The alpha()/mix() are computed HERE now,
            // so the generated rules carry concrete colours and the derivation stays
            // in Rust where it is tested; `alpha(c, t)` composited over the page is
            // exactly `mix(page, c, t)`, so this is byte-identical to the CSS it
            // replaced — but ONLY when fed the same source colour, hence `chrome_fg`.
            table_border: theme
                .table_border_color
                .unwrap_or_else(|| mix_rgba(bg, chrome_fg, 0.25)),
            table_head_bg: theme
                .table_head_bg
                .unwrap_or_else(|| mix_rgba(bg, chrome_fg, 0.08)),
            rule: theme
                .rule_color
                .unwrap_or_else(|| mix_rgba(bg, chrome_fg, 0.25)),
            syntect_theme,
        }
    }

    /// Resolve the palette for the ACTIVE theme: probe the desktop for whatever the
    /// theme leaves unstated (resolution link 3), then derive the rest purely.
    ///
    /// The probe itself is [`probe_named`]; this is only the entry point that hands it
    /// the active theme.
    pub(crate) fn resolve() -> Self {
        Self::for_theme(&crate::theme::active())
    }

    /// The palette an **exported page** resolves to: the given theme's stated keys
    /// over a paper base, with no desktop probe at all.
    ///
    /// **Paper has no dark mode.** Link 3 of the resolution order is "the desktop GTK
    /// theme probe + derivation", and on a dark desktop that probe answers with a dark
    /// page — which is right for a screen and wrong for a sheet of paper, where it
    /// prints as a washed-out ghost of itself. So an export asks for a *light*
    /// resolution instead of probing.
    ///
    /// That is a **resolution request, not a literal**: everything distinctive still
    /// comes from the theme (overlays, typography, metrics) and everything derived
    /// still comes from [`from_base`](Self::from_base)'s WCAG walk. The one thing that
    /// changes is which base the derivation starts from, and the paper base lives here
    /// — beside the light/dark floor this module already owns — rather than in an
    /// export sink, so no rendering code outside this file names a colour.
    pub(crate) fn for_paper(theme: &crate::theme::Theme) -> Self {
        // A theme that states its own page is honoured: a reader who chose Sepia and
        // exports gets Sepia's warm page, which is a light page already. Only the
        // *fall-through* is forced light, because that is the branch the desktop
        // would otherwise darken.
        let bg = theme.background.unwrap_or(PAPER_BG);
        let fg = theme.foreground.unwrap_or(PAPER_FG);
        let accent = theme.accent_color.unwrap_or(PAPER_ACCENT);
        Self::from_base(bg, fg, fg, accent, theme)
    }

    /// [`Palette::resolve`] for an explicit theme — the seam the theme switch and
    /// the tests use.
    pub(crate) fn for_theme(theme: &crate::theme::Theme) -> Self {
        let fg = theme
            .foreground
            .unwrap_or_else(|| probe_named(BODY_INK_NAMES, BODY_INK_FLOOR));
        let bg = theme
            .background
            .unwrap_or_else(|| probe_named(PAGE_NAMES, page_floor_for_ink(fg)));
        let accent = theme
            .accent_color
            .unwrap_or_else(|| probe_named(ACCENT_NAMES, ACCENT_FLOOR));
        // The CHROME ink — what the preview's table borders/header fill derive from,
        // matching the `@theme_fg_color` their CSS used to name. Distinct from `fg`
        // above (`theme_text_color`) in real themes; a named theme states one
        // foreground and both collapse onto it. See `from_base`.
        let chrome_fg = theme
            .foreground
            .unwrap_or_else(|| probe_named(CHROME_INK_NAMES, CHROME_INK_FLOOR));
        Self::from_base(bg, fg, chrome_fg, accent, theme)
    }
}

/// Gated tests live beside the module rather than inside it: `sdd/POLICY.md` § Code
/// style caps a file at 500 lines and this one had outgrown it.
#[cfg(test)]
mod tests;
