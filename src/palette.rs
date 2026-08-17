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

pub(crate) fn to_hex(c: gdk::RGBA) -> String {
    let ch = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        ch(c.red()),
        ch(c.green()),
        ch(c.blue())
    )
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

/// Whether the DESKTOP theme is dark, independent of the active reading theme.
/// The editor pane, the toolbar, the tab strip and the outline sidebar all stay on
/// the desktop theme, so this — not `Palette::is_dark` — is what they follow.
#[allow(deprecated)] // style_context() deprecated in GTK ≥ 4.10; no stable alternative yet
pub(crate) fn desktop_is_dark() -> bool {
    let probe = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let ctx = probe.style_context();
    let fg = ctx
        .lookup_color("theme_text_color")
        .or_else(|| ctx.lookup_color("theme_fg_color"))
        .unwrap_or_else(|| gdk::RGBA::new(0.067, 0.067, 0.067, 1.0));
    let bg = ctx
        .lookup_color("theme_base_color")
        .or_else(|| ctx.lookup_color("view_bg_color"))
        .or_else(|| ctx.lookup_color("theme_bg_color"))
        .unwrap_or_else(|| {
            if luminance(fg) > 0.5 {
                gdk::RGBA::new(0.118, 0.118, 0.118, 1.0)
            } else {
                gdk::RGBA::new(1.0, 1.0, 1.0, 1.0)
            }
        });
    luminance(bg) < 0.5
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
        let is_dark = luminance(bg) < 0.5;

        // Walk link color toward white (dark) or black (light) until WCAG AA contrast met.
        let adjust_target = if is_dark {
            gdk::RGBA::new(1.0, 1.0, 1.0, 1.0)
        } else {
            gdk::RGBA::new(0.0, 0.0, 0.0, 1.0)
        };
        let mut link_fg = accent;
        for _ in 0..20 {
            if contrast(link_fg, bg) >= 4.5 {
                break;
            }
            link_fg = mix_rgba(link_fg, adjust_target, 0.1);
        }

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
            let theme_bg = syntect()
                .1
                .themes
                .get(&syntect_theme)
                .and_then(|t| t.settings.background)
                .map(syntect_color_to_rgba);
            match theme_bg {
                Some(tb) if contrast(tb, bg) >= 1.08 => tb,
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
            let mut best = if contrast(fg, selection_bg) >= contrast(bg, selection_bg) {
                fg
            } else {
                bg
            };
            let target = if luminance(selection_bg) < 0.5 {
                gdk::RGBA::new(1.0, 1.0, 1.0, 1.0)
            } else {
                gdk::RGBA::new(0.0, 0.0, 0.0, 1.0)
            };
            for _ in 0..20 {
                if contrast(best, selection_bg) >= 4.5 {
                    break;
                }
                best = mix_rgba(best, target, 0.1);
            }
            best
        });

        Palette {
            page_bg: bg,
            body_fg: fg,
            code_inline_bg,
            code_block_bg,
            link_fg: theme.link.unwrap_or(link_fg),
            blockquote_bar: theme.blockquote_bar.unwrap_or(accent),
            selection_bg,
            selection_fg,
            // The table chrome's CSS used alpha(@theme_fg_color, 0.25 / 0.08) — the
            // CHROME ink, not the body ink. The alpha()/mix() are computed HERE now,
            // so the generated rules carry concrete colours and the derivation stays
            // in Rust where it is tested; `alpha(c, t)` composited over the page is
            // exactly `mix(page, c, t)`, so this is byte-identical to the CSS it
            // replaced — but ONLY when fed the same source colour, hence `chrome_fg`.
            table_border: theme
                .table_border
                .unwrap_or_else(|| mix_rgba(bg, chrome_fg, 0.25)),
            table_head_bg: theme
                .table_head_bg
                .unwrap_or_else(|| mix_rgba(bg, chrome_fg, 0.08)),
            rule: theme.rule.unwrap_or_else(|| mix_rgba(bg, chrome_fg, 0.25)),
            syntect_theme,
        }
    }

    /// Resolve the palette for the ACTIVE theme: probe the desktop for whatever the
    /// theme leaves unstated (resolution link 3), then derive the rest purely.
    ///
    /// The probe reads theme named colors via a temporary widget's style context.
    /// Theme CSS is loaded for the default display by connect_startup, so
    /// lookup_color is available even on an unparented widget.
    #[allow(deprecated)] // style_context() deprecated in GTK ≥ 4.10; no stable alternative yet
    pub(crate) fn resolve() -> Self {
        Self::for_theme(&crate::theme::active())
    }

    /// [`Palette::resolve`] for an explicit theme — the seam the theme switch and
    /// the tests use.
    #[allow(deprecated)] // style_context() deprecated in GTK ≥ 4.10; no stable alternative yet
    pub(crate) fn for_theme(theme: &crate::theme::Theme) -> Self {
        let probe = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let ctx = probe.style_context();

        let fg = theme.foreground.unwrap_or_else(|| {
            ctx.lookup_color("theme_text_color")
                .or_else(|| ctx.lookup_color("theme_fg_color"))
                .unwrap_or_else(|| gdk::RGBA::new(0.067, 0.067, 0.067, 1.0))
        });

        let bg = theme.background.unwrap_or_else(|| {
            ctx.lookup_color("theme_base_color")
                .or_else(|| ctx.lookup_color("view_bg_color"))
                .or_else(|| ctx.lookup_color("theme_bg_color"))
                .unwrap_or_else(|| {
                    if luminance(fg) > 0.5 {
                        gdk::RGBA::new(0.118, 0.118, 0.118, 1.0) // #1e1e1e dark bg
                    } else {
                        gdk::RGBA::new(1.0, 1.0, 1.0, 1.0) // #ffffff light bg
                    }
                })
        });

        let accent = theme.accent.unwrap_or_else(|| {
            ctx.lookup_color("theme_selected_bg_color")
                .or_else(|| ctx.lookup_color("accent_bg_color"))
                .unwrap_or_else(|| gdk::RGBA::new(0.208, 0.518, 0.894, 1.0)) // #3584e4
        });

        // The CHROME ink — what the preview's table borders/header fill derive from,
        // matching the `@theme_fg_color` their CSS used to name. Distinct from `fg`
        // above (`theme_text_color`) in real themes; a named theme states one
        // foreground and both collapse onto it. See `from_base`.
        let chrome_fg = theme.foreground.unwrap_or_else(|| {
            ctx.lookup_color("theme_fg_color")
                .or_else(|| ctx.lookup_color("theme_text_color"))
                .unwrap_or_else(|| gdk::RGBA::new(0.180, 0.204, 0.212, 1.0)) // #2e3436
        });

        Self::from_base(bg, fg, chrome_fg, accent, theme)
    }
}

#[cfg(test)]
mod tests {
    use super::{contrast, luminance, mix_rgba, syntect_color_to_rgba, to_hex, Palette};
    use gtk::gdk;

    const BLACK: gdk::RGBA = gdk::RGBA::BLACK;
    const WHITE: gdk::RGBA = gdk::RGBA::WHITE;

    #[test]
    fn syntect_color_maps_channels_and_forces_opaque() {
        let c = syntect::highlighting::Color {
            r: 255,
            g: 0,
            b: 128,
            a: 0,
        };
        let rgba = syntect_color_to_rgba(c);
        assert_eq!(rgba.red(), 1.0);
        assert_eq!(rgba.green(), 0.0);
        assert!((rgba.blue() - 128.0 / 255.0).abs() < 1e-6);
        // Alpha is forced opaque regardless of the source `a`.
        assert_eq!(rgba.alpha(), 1.0);
    }

    #[test]
    fn to_hex_formats_channels() {
        assert_eq!(to_hex(BLACK), "#000000");
        assert_eq!(to_hex(WHITE), "#ffffff");
        assert_eq!(to_hex(gdk::RGBA::new(1.0, 0.0, 0.0, 1.0)), "#ff0000");
        // Out-of-range channels clamp rather than overflow.
        assert_eq!(to_hex(gdk::RGBA::new(2.0, -1.0, 0.0, 1.0)), "#ff0000");
    }

    #[test]
    fn luminance_endpoints() {
        assert!((luminance(BLACK) - 0.0).abs() < 1e-9);
        assert!((luminance(WHITE) - 1.0).abs() < 1e-9);
        // Mid-grey sits between the endpoints.
        let g = luminance(gdk::RGBA::new(0.5, 0.5, 0.5, 1.0));
        assert!(g > 0.0 && g < 1.0);
    }

    #[test]
    fn contrast_black_on_white_is_maximal() {
        // WCAG contrast ratio of pure black vs pure white is 21:1.
        assert!((contrast(BLACK, WHITE) - 21.0).abs() < 1e-6);
        // Contrast is symmetric.
        assert!((contrast(WHITE, BLACK) - contrast(BLACK, WHITE)).abs() < 1e-9);
        // A color against itself has ratio 1.
        assert!((contrast(WHITE, WHITE) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn mix_rgba_interpolates() {
        // t=0 yields a, t=1 yields b.
        assert_eq!(to_hex(mix_rgba(BLACK, WHITE, 0.0)), "#000000");
        assert_eq!(to_hex(mix_rgba(BLACK, WHITE, 1.0)), "#ffffff");
        // Midpoint of black→white is mid-grey (0.5*255 = 127.5 → 128 = 0x80).
        assert_eq!(to_hex(mix_rgba(BLACK, WHITE, 0.5)), "#808080");
    }

    /// Selected text stays legible on every shipped theme's selection fill.
    ///
    /// The bug this pins: styling the selection's background alone left its
    /// foreground to the desktop GTK theme, which painted selected text `#000000`
    /// at 2.1:1 on Bedtime's fill — measured on screen, not theorised. The
    /// derivation picks between the page ink and the page itself, so the assertion
    /// is on the RATIO rather than on a literal: a future theme is free to choose
    /// any fill, and this fails if that choice would strand its selected text.
    #[test]
    fn selected_text_clears_the_legibility_floor_on_every_theme() {
        let themes = crate::theme::Themes::builtin();
        for (id, _name, _sym) in themes.chooser_list() {
            let t = themes.resolve(&id);
            // Only a theme that states its own page emits a selection rule at all;
            // System defers the whole block to the desktop (TDD 18.2).
            let (Some(bg), Some(fg)) = (t.background, t.foreground) else {
                continue;
            };
            let p = Palette::from_base(bg, fg, fg, t.accent.unwrap_or(fg), &t);
            let c = contrast(p.selection_fg, p.selection_bg);
            assert!(
                c >= 4.5,
                "{id}: selected text {} on selection fill {} is {c:.2}:1",
                to_hex(p.selection_fg),
                to_hex(p.selection_bg)
            );
            assert_ne!(
                to_hex(p.selection_fg),
                to_hex(p.selection_bg),
                "{id}: selected text drawn in the fill's own colour"
            );
        }
    }
}
