//! How the preview wraps prose, and the one decision that depends on the GTK it runs on.
//!
//! # The mechanism, and why the answer depends on the GTK version
//!
//! The mode the preview wants is `WordChar`: break at the spaces between words, and
//! inside a word only when that word cannot fit, so a long URL or identifier never makes
//! a line wider than the pane (the no-overflow invariant GTK4Rs/AP-22 and GTK4Rs/AP-23 depend
//! on). GTK before 4.7 cannot carry it. Its AT-SPI text-attribute path
//! (`gtkatspitextbuffer.c`) casts a raw `GtkWrapMode` to `PangoWrapMode` with no
//! translation, and the two enums disagree past the first value: `GtkWrapMode` is
//! NONE, CHAR, WORD, WORD_CHAR (0..3), `PangoWrapMode` is WORD, CHAR, WORD_CHAR (0..2).
//! `WordChar`(3) is out of range, so `pango_wrap_mode_to_string` hits
//! `g_assert_not_reached()` and the whole app aborts the moment a screen reader reads the
//! view's default attributes, or any offset inside a tag that sets the mode
//! (GTK4Rs/AP-136). The conversion this needs (`gtk_wrap_mode_to_string`) first appears
//! in 4.7.0, so 4.7 is the floor.
//!
//! Below it, `Word` and `Char` are the only safe modes, and each fails one way: `Word`
//! lets a run with no break opportunity overflow the pane, `Char` breaks prose in the
//! middle of words. So below 4.7 the view and its tags stay at `Word`, and a PARAGRAPH
//! holding a run too wide to be sure of fitting is given `Char` instead.
//!
//! # Why it is chosen per paragraph
//!
//! GTK takes one wrap mode per paragraph (`GtkTextLine`), from the style at the
//! paragraph's FIRST visible segment (`set_para_values`, `gtktextlayout.c`); a tag that
//! starts mid-paragraph does nothing. So the mode is carried by a tag over the
//! paragraph's first character ([`TagName::WrapChar`](super::TagName::WrapChar)), and
//! the trade-off is stated plainly: below GTK 4.7, only a paragraph that holds an
//! unbreakable run longer than [`run_limit`] breaks mid-word. Every other paragraph
//! breaks at spaces.
//!
//! # What counts as unbreakable, and how long is too long
//!
//! A run is the text between two line-break opportunities as Pango reports them
//! (`pango_get_log_attrs`, `is_line_break`, Unicode UAX #14 as Pango implements it). That
//! is the rule `Word` wrapping itself breaks by, so a run this module calls unbreakable
//! is exactly one the layout cannot split: a slash or a hyphen ends a run, an underscore
//! does not, and ideographs each end one. There is no Rust binding for the attribute, so
//! it is one small FFI call, checked against those cases in the tests below.
//!
//! The limit is derived so that any run of at most that many characters fits the
//! narrowest pane. No face this application sets text in has a glyph advance wider than
//! the em (a full-width ideograph is exactly one em, Latin capitals are narrower, and a
//! monospace face's Latin advance is about 0.6 em), so a run of `n` characters is at most
//! `n` em wide. The narrowest pane is the preview alone at the window's width floor
//! (`MIN_WINDOW_WIDTH`), less the paragraph's own left and right margins, which already
//! include any list or quote indent. The em is the desktop's font size at the render's
//! zoom, times the paragraph's own scale (a heading's). That gives
//! `limit = floor(available_width / em)`.

use gtk::pango;
use std::os::raw::{c_char, c_int};

/// The first GTK minor version of 4 whose AT-SPI layer translates `WordChar`.
pub(crate) const WORD_CHAR_FLOOR: (u32, u32) = (4, 7);

/// What the preview does about wrapping on one GTK version: the view's mode, and whether
/// paragraphs holding a wide unbreakable run are switched to `Char`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WrapPolicy {
    pub(crate) mode: gtk::WrapMode,
    pub(crate) char_for_wide_runs: bool,
}

/// [`policy`]'s decision over a runtime GTK version. Never answers `Char` or `WordChar`
/// for the view below [`WORD_CHAR_FLOOR`].
pub(crate) fn policy_for(major: u32, minor: u32) -> WrapPolicy {
    if (major, minor) >= WORD_CHAR_FLOOR {
        WrapPolicy {
            mode: gtk::WrapMode::WordChar,
            char_for_wide_runs: false,
        }
    } else {
        WrapPolicy {
            mode: gtk::WrapMode::Word,
            char_for_wide_runs: true,
        }
    }
}

/// The policy for the GTK this process runs on.
pub(crate) fn policy() -> WrapPolicy {
    policy_for(gtk::major_version(), gtk::minor_version())
}

/// The longest run of `text`, in characters, with no line-break opportunity inside it.
pub(crate) fn longest_unbreakable_run(text: &str) -> usize {
    let n = text.chars().count();
    if n == 0 {
        return 0;
    }
    // `PangoLogAttr` is a 32-bit bitfield whose first bit is `is_line_break`; the tests
    // pin that reading against known break and non-break positions.
    let mut attrs = vec![0u32; n + 1];
    let language = pango::Language::from_string("en");
    // SAFETY: `text` is valid UTF-8 of `text.len()` bytes; `attrs` holds the `n + 1`
    // entries Pango writes for `n` characters, each at least as large as a log attr.
    unsafe {
        use gtk::glib::translate::ToGlibPtr;
        pango::ffi::pango_get_log_attrs(
            text.as_ptr().cast::<c_char>(),
            text.len() as c_int,
            -1,
            language.to_glib_none().0,
            attrs.as_mut_ptr().cast(),
            (n + 1) as c_int,
        );
    }
    let (mut longest, mut start) = (0, 0);
    for (i, a) in attrs.iter().enumerate().skip(1) {
        if a & 1 != 0 {
            longest = longest.max(i - start);
            start = i;
        }
    }
    longest.max(n - start)
}

/// The most characters a run may hold and still be sure to fit `available_px`, at
/// `em_px` per character (see the module's derivation). Never below one.
pub(crate) fn run_limit(available_px: f64, em_px: f64) -> usize {
    if em_px <= 0.0 || available_px <= 0.0 {
        return 1;
    }
    ((available_px / em_px).floor() as usize).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::WrapMode;

    /// TDD 2.2·a11y: `WordChar` only where AT-SPI translates it (GTK 4.7+). Below, the
    /// view is `Word`, never `Char` or `WordChar`, and wide-run paragraphs go to `Char`.
    #[test]
    fn the_preview_breaks_words_only_where_gtk_can_report_it() {
        for minor in [0, 4, 6] {
            let p = policy_for(4, minor);
            assert_eq!(p.mode, WrapMode::Word, "4.{minor}");
            assert!(p.char_for_wide_runs, "4.{minor}");
        }
        for (major, minor) in [(4, 7), (4, 8), (4, 24), (5, 0)] {
            let p = policy_for(major, minor);
            assert_eq!(p.mode, WrapMode::WordChar, "{major}.{minor}");
            assert!(!p.char_for_wide_runs, "{major}.{minor}");
        }
    }

    /// TDD 2.2·a11y: the detector reads UAX #14 as Pango applies it.
    #[test]
    fn the_run_detector_follows_the_line_break_rule() {
        assert_eq!(longest_unbreakable_run(""), 0);
        assert_eq!(longest_unbreakable_run("a"), 1);
        // A space ends a run, and is counted with the word before it.
        assert_eq!(longest_unbreakable_run("ab cdef gh"), 5);
        // A slash and a hyphen are break opportunities: a path or a compound breaks.
        assert_eq!(longest_unbreakable_run("aaaa/bbbbbb/cc"), 7);
        assert_eq!(longest_unbreakable_run("aaaa-bbbbbbb-cc"), 8);
        // An underscore is not: an identifier is one run.
        assert_eq!(longest_unbreakable_run("snake_case_name"), 15);
        // No opportunity at all.
        assert_eq!(longest_unbreakable_run(&"a".repeat(200)), 200);
        // Ideographs break between each other.
        assert_eq!(longest_unbreakable_run("漢字漢字漢字漢字"), 1);
    }

    /// TDD 2.2·a11y: the limit is the pane over the em, floored, and never zero.
    #[test]
    fn the_run_limit_is_the_pane_over_the_em() {
        assert_eq!(run_limit(320.0, 16.0), 20);
        assert_eq!(run_limit(319.0, 16.0), 19);
        assert_eq!(run_limit(10.0, 16.0), 1);
        assert_eq!(run_limit(-5.0, 16.0), 1);
        assert_eq!(run_limit(320.0, 0.0), 1);
    }
}
