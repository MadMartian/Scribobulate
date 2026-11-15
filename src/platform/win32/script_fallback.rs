//! A face for the scripts a requested font stack cannot draw, one for the symbols
//! (`symbol_fallback_family`), and one for a letter's combining marks
//! (`mark_fallback_family`).
//!
//! **Cause.** Pango's Win32 font map falls back from one face to the next only through
//! its own built-in alias table; it never searches the installed faces for a character
//! the way fontconfig (Linux) and Core Text (macOS) do. The tables for the CSS generics
//! were written for older Windows: `serif`, `sans` and `monospace` each name GulimChe as
//! their Korean face, which a stock Windows 10 or 11 does not install. So a run laid out
//! in a generic draws Hangul as Pango's hex box. That is what the PDF export does when
//! the theme names no face, because a fresh Pango context's default family is `serif`
//! (measured: Georgia for the Latin, SimSun for Japanese, six unknown glyphs for
//! `한국어 텍스트`).
//!
//! **Why the preview's prose did not show it.** The preview lays unthemed text out in
//! the desktop's own face, which GDK-Win32 reads from the system's message font into
//! `GtkSettings:gtk-font-name` (Segoe UI on a stock install), and Pango's table for
//! Segoe UI names the faces Windows does ship for every script it covers — Malgun Gothic
//! among them. The preview's CODE did show it: a code tag or span names `monospace`,
//! which replaces the paragraph's family outright, so a fenced block, inline code and a
//! table cell's code drew Hangul as hex boxes on screen too.
//!
//! **The source.** So the face this supplies is the desktop's: the family of
//! `gtk-font-name`, appended after a requested stack by
//! `pangospan::family_with_fallback` — for every family the PDF sink requests, and for
//! the code face on both surfaces (`pangospan::with_script_fallback`). Text then falls
//! back exactly where the preview's prose does, through the same alias table, and to
//! nothing the preview would not have reached. It is read rather than written down so
//! that a desktop whose message font is not Segoe UI keeps the surfaces agreeing.

/// The family of the desktop's own face, or `None` where GTK is not running on this
/// thread (a unit test laying a document out with no display), which leaves a run's
/// faces exactly as requested.
pub(crate) fn script_fallback_family() -> Option<String> {
    if !gtk::is_initialized_main_thread() {
        return None;
    }
    let name = gtk::Settings::default()?.gtk_font_name()?;
    let family = gtk::pango::FontDescription::from_string(&name).family()?;
    Some(family.to_string())
}

/// The face Windows ships for symbols: Geometric Shapes, Arrows, Mathematical Operators.
///
/// **The same cause, landing on a face that covers the character and draws it wrong.**
/// Segoe UI has `▼` but not `▶`, and its alias table reaches MS UI Gothic first, whose
/// `▶` at the desktop's 9pt is an embedded bitmap a 3×4 px speck (MEASURED, GTK 4.22.4
/// / Pango 1.57 / Windows 10 19045: 4×4 dark pixels against Segoe UI Symbol's 8×10). So
/// this face cannot go LAST as the script fallback does: MS UI Gothic covers the
/// character and answers first. Appending it was measured and changed nothing; only a
/// family list that leads with it reaches it. Its caller leads with it only for a
/// string the page's own face cannot draw (`widgets::disclosure`).
///
/// A constant rather than read from the system: Windows has no setting naming a symbol
/// face, and this one ships on every Windows the app supports. Were it absent, Pango
/// would skip it and fall back exactly as before.
pub(crate) fn symbol_fallback_family() -> Option<&'static str> {
    Some("Segoe UI Symbol")
}

/// The face a base letter and its combining marks are set in together where the run's
/// lead face has the letter but not every mark.
///
/// **The same cause, landing on a face in another RUN.** Georgia — the lead of `serif`,
/// so of every unthemed PDF, and of Sepia's stack — has 11 of the 112 combining
/// diacriticals (U+0300–U+036F); it lacks U+0303. `serif`'s alias table takes the tilde
/// to MS Gothic, which itemizes it into a run of its own, and HarfBuzz positions a mark
/// only against a base in the same run: `n` + U+0303 drew `n` and a free-standing tilde
/// beside it (MEASURED, Pango 1.57 / Windows 10 19045: runs `an` Georgia, U+0303 MS
/// Gothic, the rest Georgia). Appending a face cannot help, for `symbol_fallback_family`'s
/// reason; leading every run with one would move every Latin letter off Georgia. So only
/// the CLUSTER — the letter and its marks — is set in this face
/// (`pangospan::mark_cover`), and shaped in one run it composes.
///
/// **Why Times New Roman.** It ships on every Windows the app supports and has all 112
/// combining diacriticals (measured), and so does Segoe UI, the desktop face. The lead
/// that lacks them is a SERIF (Tahoma and Courier New, which `sans` and `monospace` reach,
/// both carry U+0301, U+0303 and U+0323), so the cluster is set in the serif: its
/// letter keeps the strokes and contrast of the letters around it, where Segoe UI would
/// put one sans letter in a serif word. A constant for `symbol_fallback_family`'s reason:
/// Windows has no setting naming such a face.
pub(crate) fn mark_fallback_family() -> Option<&'static str> {
    Some("Times New Roman")
}
