//! The CSS fragments the preview's sheet and the HTML sink's sheet genuinely share.
//!
//! The two generators are **not** one sheet with two spellings and merging them would be
//! wrong: the preview expresses most of this vocabulary through `GtkTextTag`s and
//! self-drawn snapshots, so its CSS covers a much smaller surface than an artefact's
//! does, and the two use different GTK/browser cascades. What they do share is small and
//! specific — the `Option<colour>` → declaration idiom, and the link-underline decision —
//! and each was spelled out independently on both sides, which is where they drift.
//!
//! **What is deliberately NOT shared: how a colour is spelled.** The HTML sink writes
//! `#rrggbbaa` because a browser honours a theme's alpha; the preview writes
//! `#rrggbb` because the surfaces it styles composite differently. That is a real
//! difference between the two media, so the hex function stays at the call site where a
//! reader can see which one was chosen, and only the "state it or say nothing" decision
//! is shared.

use crate::theme::LineStyle;

/// A CSS declaration for a value the theme stated, or **nothing at all** where it
/// stated none.
///
/// The distinction is the whole point and it is easy to invert by accident: "the theme
/// said nothing" must emit nothing, so the cascade (the desktop GTK theme, or the
/// browser's default sheet) decides — where an `unwrap_or_default()` on an
/// already-formatted string quietly emits an empty declaration instead. A leading space
/// is included so a caller can splice the result straight after another declaration.
pub(crate) fn decl(property: &str, value: Option<impl std::fmt::Display>) -> String {
    value
        .map(|v| format!(" {property}: {v};"))
        .unwrap_or_default()
}

/// The `text-decoration-line` keyword a themed link underline implies.
///
/// Stated rather than omitted when the theme turns the line off: omitting it hands the
/// decision back to the desktop GTK theme or the browser, which is the drift the themed
/// key exists to prevent.
pub(crate) fn link_underline_line(style: LineStyle) -> &'static str {
    if style.is_none() {
        "none"
    } else {
        "underline"
    }
}

/// The `text-decoration-style` declaration a themed link underline implies, and empty
/// for `solid` — which is CSS's own initial value, so stating it would only make the
/// rule differ from the one both sheets emitted before the key existed (TDD 18.2).
pub(crate) fn link_underline_style_decl(style: LineStyle) -> String {
    match style.css_style() {
        None | Some("solid") => String::new(),
        Some(spelling) => format!(" text-decoration-style: {spelling};"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The idiom's whole value is the difference between "nothing" and "empty", which is
    /// the thing an `unwrap_or_default()` on a formatted string gets wrong.
    #[test]
    fn an_unstated_value_produces_no_declaration_at_all() {
        assert_eq!(decl("color", None::<String>), "");
        assert_eq!(decl("color", Some("#ff0000")), " color: #ff0000;");
    }

    /// Both sheets must answer the same link-underline question the same way, including
    /// the `solid` case — where stating the style would move a rule that has never
    /// carried one.
    #[test]
    fn the_link_underline_decision_is_one_answer_for_both_sheets() {
        assert_eq!(link_underline_line(LineStyle::None), "none");
        for style in [LineStyle::Single, LineStyle::Double, LineStyle::Wavy] {
            assert_eq!(link_underline_line(style), "underline", "{style:?}");
        }
        assert_eq!(link_underline_style_decl(LineStyle::None), "");
        assert_eq!(link_underline_style_decl(LineStyle::Single), "");
        assert_eq!(
            link_underline_style_decl(LineStyle::Double),
            " text-decoration-style: double;"
        );
        assert_eq!(
            link_underline_style_decl(LineStyle::Wavy),
            " text-decoration-style: wavy;"
        );
    }
}
