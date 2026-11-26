//! A suggested edit's colours on one page: what it would add and what it would remove,
//! each an ink and an optional fill (PLAN.suggested-edit-colours.md, decision 1).
//!
//! The inks come from the theme (`addition_fg`, `deletion_fg`), which floors them at a
//! conventional green and crimson. A floor is a literal and a page is not: the system
//! theme's page is the desktop's, light or dark, so the ink is walked to the legibility
//! floor against what it is drawn on — its fill over the page, or the page — by the same
//! walk the derived link colour takes. An ink that already clears the floor is
//! untouched, which is every ink a built-in theme states (the contrast gate holds them
//! there). A fill a theme does not state is absent.

use super::codechips::over;
use super::walk_to_contrast;
use gtk::gdk;

/// One kind of change's ink and fill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SuggestionStyle {
    pub(crate) fg: gdk::RGBA,
    pub(crate) bg: Option<gdk::RGBA>,
}

/// Both kinds of change, resolved for one page. `Copy` and colour-only, like
/// [`super::CodeChips`]: a renderer handed one cannot reach back for a second opinion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SuggestionInks {
    /// What a suggested edit would add.
    pub(crate) addition: SuggestionStyle,
    /// What a suggested edit would remove; drawn struck through.
    pub(crate) deletion: SuggestionStyle,
}

impl SuggestionInks {
    /// The inks on paper: the theme's own page where it states one, else the light page
    /// every export falls back to (TDD 25.9) — the page [`super::Palette::for_paper`]
    /// resolves, so a printed suggestion is judged against what it is printed on.
    pub(crate) fn for_paper(theme: &crate::theme::Theme) -> Self {
        Self::resolve(theme, theme.background.unwrap_or(super::PAPER_BG))
    }

    pub(crate) fn resolve(theme: &crate::theme::Theme, page: gdk::RGBA) -> Self {
        let style = |fg: gdk::RGBA, bg: Option<gdk::RGBA>| SuggestionStyle {
            fg: walk_to_contrast(fg, bg.map_or(page, |b| over(page, b))),
            bg,
        };
        Self {
            addition: style(theme.addition_fg, theme.addition_bg),
            deletion: style(theme.deletion_fg, theme.deletion_bg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::{contrast, WCAG_AA_TEXT};

    fn rgba(hex: &str) -> gdk::RGBA {
        crate::theme::parse_color(hex).expect("a test colour")
    }

    /// The floors are legible on a light page and on a dark one, and an unset fill is
    /// absent — no wash borrowed from elsewhere.
    #[test]
    fn the_default_inks_are_legible_on_any_page_and_an_unset_fill_is_absent() {
        let theme = crate::theme::Themes::builtin().resolve("system");
        for page in ["#ffffff", "#fbf0d9", "#1e1e1e", "#000000"] {
            let page = rgba(page);
            let inks = SuggestionInks::resolve(&theme, page);
            for style in [inks.addition, inks.deletion] {
                assert!(
                    contrast(style.fg, page) >= WCAG_AA_TEXT,
                    "{page:?} {style:?}"
                );
                assert_eq!(style.bg, None);
            }
        }
    }

    /// The rubric's floor: on paper, every built-in theme's addition and deletion inks are
    /// legible on that theme's own page — stated or walked.
    #[test]
    fn every_built_in_theme_prints_both_inks_legibly() {
        let themes = crate::theme::Themes::builtin();
        for crate::theme::ChooserEntry { id, .. } in themes.chooser_list() {
            let theme = themes.resolve(&id);
            let page = theme.background.unwrap_or(super::super::PAPER_BG);
            let inks = SuggestionInks::for_paper(&theme);
            for (name, style) in [("addition", inks.addition), ("deletion", inks.deletion)] {
                let under = style.bg.map_or(page, |b| over(page, b));
                assert!(
                    contrast(style.fg, under) >= WCAG_AA_TEXT,
                    "{id}: {name} ink {:?} on {:?}",
                    style.fg,
                    under
                );
            }
        }
    }

    /// A stated fill is kept, and the ink is judged against it over the page.
    #[test]
    fn a_stated_fill_is_kept_and_the_ink_is_judged_against_it() {
        let mut theme = crate::theme::Themes::builtin().resolve("system");
        theme.addition_bg = Some(rgba("#003300"));
        theme.addition_fg = rgba("#004400");
        let inks = SuggestionInks::resolve(&theme, rgba("#ffffff"));
        assert_eq!(inks.addition.bg, Some(rgba("#003300")));
        assert!(contrast(inks.addition.fg, rgba("#003300")) >= WCAG_AA_TEXT);
    }
}
