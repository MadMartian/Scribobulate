//! A face for the scripts a requested font stack cannot draw.
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
