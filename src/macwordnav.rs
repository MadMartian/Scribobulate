//! Option+Left / Option+Right word navigation in the editor, on macOS only.
//!
//! **The bug this fixes, and its real mechanism.** `GtkSourceView` — not base
//! `GtkTextView` — carries its own extra class keybinding on top of the ones
//! `crate::keynav::document_movement` mirrors: a `move-words` signal, **"default
//! binding key is Alt+Left/Right Arrow", which "moves the current selection, or the
//! current word, by one word"** (`gtksourceview/gtksourceview.c:953`, GIR-documented
//! in `GtkSource-5.gir`). That is not caret movement, it is a *word transposition* —
//! it edits the buffer, swapping the word at the cursor with its neighbour. A Mac
//! reader pressing Option+Left expecting the AppKit word-navigation convention (every
//! native macOS text field binds it there) instead got their document quietly
//! rearranged: exactly the reported symptom, "does not seem to navigate words, seems
//! to mutate the document." Confirmed both from the GIR source above and by driving
//! the unpatched behaviour live (a two-instance comparison against this fix, below).
//!
//! `win.nav-back`/`win.nav-forward` (TDD §23.6) sat on the *same* keystroke as a
//! window-level accelerator, and was the first, wrong hypothesis for this bug's
//! mechanism — plausible (GTK4Rs/AP-121's shape: a window accelerator competing with a
//! focused widget's own binding) but not what actually fires here: measured live **on
//! Quartz**, `GtkSourceView`'s own class binding wins over the `GAction` accelerator on
//! the same key while the view holds focus, so Back/Forward was never reachable from the
//! editor by this key to begin with **on that backend**. That ordering is **not** a
//! toolkit property and must not be quoted as one: the same contest was measured with
//! the opposite outcome on **Win32** (GTK 4.22.4, GtkSourceView 5.20.0) and on **X11**
//! (GTK 4.6.9, GtkSourceView 5.4.1), where the accelerator wins and `move-words` never
//! fires though the binding is present in both libraries. Quartz is one backend of the
//! three, and it is the one this module exists for — see ScrAP-311. Moving it off `<Alt>Left`/`<Alt>Right`
//! (`accel.rs`'s `MAC_RESERVED`, to `<Meta>bracketleft`/`<Meta>bracketright` —
//! Safari/Finder's own spelling) is still correct — it removes a claim on a keystroke
//! the platform owns, and closes the gap for whatever focus state *doesn't* favour the
//! view's own binding — but it is not, by itself, why the mutation happened, and it
//! does nothing to stop `move-words`. **This module is what stops it**: its
//! `GtkEventControllerKey` answers Option+Left/Right first and returns
//! `Propagation::Stop`, which pre-empts `move-words` entirely — GTK does not run a
//! widget's own class keybindings once a controller in the propagation chain has
//! consumed the event. Verified live, both directions, same document
//! (`"…runs away quickly."`, caret at the end): with this module wired (and
//! `MAC_RESERVED` in place), successive Option+Left presses move the caret one word
//! at a time (column 72→71→64, past the trailing `.` then to the start of
//! "quickly") with the text byte-identical throughout, and Option+Right retraces it
//! exactly (64→71); with this module's wiring removed and `MAC_RESERVED`'s two
//! entries also reverted (so `<Alt>Left` is declared to `win.nav-back` exactly as
//! before this fix), the second Option+Left instead rewrites the line to
//! `"…runs quickly away."` — `move-words` fired, and critically, `win.nav-back`'s
//! accelerator on the very same key did *not* — no tab switch, no history
//! navigation, confirming the class binding really does win over the `GAction`
//! accelerator on this key while the view holds focus.
//!
//! **Scope.** Wired onto the main document editor only (`build_tab_editor`, the one
//! place every editor `sourceview::View` is built) — the surface the report was
//! about. `GtkEntry`/`GtkText` fields elsewhere (the comment entry, Go To Line, the
//! rename/URL dialogs) are plain GTK widgets with no `move-words` binding, so they
//! don't carry this defect at all — nothing to fold into this fix there.
//!
//! **Why a plain key controller and not a `GtkShortcutController`/accelerator.** A
//! bubble-phase `GtkEventControllerKey` on the view, added after construction, runs
//! ahead of the view's own class keybindings (see above) — that ordering is exactly
//! the mechanism this fix depends on, not an incidental choice. Capture phase (as
//! `codeview::navkeys` uses, to run ahead of a focused *descendant's* bindings) is not
//! needed here: this view has no such descendant, and bubble already outruns the
//! view's own binding set.
use gtk::gdk::{Key, ModifierType};
use gtk::glib;
use gtk::prelude::*;
use gtk::MovementStep;

/// The word-movement Option+`key` means for the editor, or `None` if `key`/`mods`
/// is not that combination.
///
/// A pure function over plain data (mirroring `crate::keynav::document_movement`'s
/// own shape) so the decision is unit-testable without a display, and so a future
/// caller — the follow-up for the standalone `GtkEntry` fields noted above — can
/// reuse it without depending on a live widget.
///
/// Only a bare Option or Option+Shift qualifies. Any other modifier riding along
/// (Control, Command/Meta, Super, Hyper) means this is someone else's combination —
/// an accelerator, most likely — and must be left alone; matching on it here would
/// silently steal a keystroke this function has no business answering for.
/// Caps Lock / Num Lock are excluded from `significant` for the same reason
/// `keynav::document_movement` excludes them: they ride along on real events and
/// must not defeat the match for a reader who has either one on.
pub(crate) fn word_movement(key: Key, mods: ModifierType) -> Option<(i32, bool)> {
    let significant = ModifierType::SHIFT_MASK
        | ModifierType::CONTROL_MASK
        | ModifierType::ALT_MASK
        | ModifierType::SUPER_MASK
        | ModifierType::HYPER_MASK
        | ModifierType::META_MASK;
    let held = mods & significant;
    if !held.contains(ModifierType::ALT_MASK) {
        return None;
    }
    let extra = held - (ModifierType::ALT_MASK | ModifierType::SHIFT_MASK);
    if !extra.is_empty() {
        return None;
    }
    let count = match key {
        Key::Left | Key::KP_Left => -1,
        Key::Right | Key::KP_Right => 1,
        _ => return None,
    };
    Some((count, held.contains(ModifierType::SHIFT_MASK)))
}

/// Wire Option+Left/Option+Right (and Option+Shift, to extend the selection) onto
/// `view` as word movement. Call once, at the view's construction site
/// (`build_tab_editor`) — the same place `farscroll::wire_buffer_ends_scroll` and
/// `wire_newline_edits` are wired.
pub(crate) fn wire_word_navigation(view: &sourceview::View) {
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        view,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, key, _, mods| {
            let Some((count, extend)) = word_movement(key, mods) else {
                return glib::Propagation::Proceed;
            };
            // Exactly what GTK's own Ctrl+Left/Right binding does, just spelled
            // with Option instead of Control — see `keynav::document_movement`'s
            // `(control, Words)` arm for the binding this mirrors.
            view.emit_move_cursor(MovementStep::Words, count, extend);
            glib::Propagation::Stop
        }
    ));
    view.upcast_ref::<gtk::Widget>().add_controller(keys);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_option_left_moves_one_word_back() {
        assert_eq!(
            word_movement(Key::Left, ModifierType::ALT_MASK),
            Some((-1, false))
        );
    }

    #[test]
    fn bare_option_right_moves_one_word_forward() {
        assert_eq!(
            word_movement(Key::Right, ModifierType::ALT_MASK),
            Some((1, false))
        );
    }

    #[test]
    fn the_keypad_duplicates_are_covered() {
        assert_eq!(
            word_movement(Key::KP_Left, ModifierType::ALT_MASK),
            word_movement(Key::Left, ModifierType::ALT_MASK)
        );
        assert_eq!(
            word_movement(Key::KP_Right, ModifierType::ALT_MASK),
            word_movement(Key::Right, ModifierType::ALT_MASK)
        );
    }

    #[test]
    fn option_shift_extends_the_selection_by_word() {
        assert_eq!(
            word_movement(Key::Left, ModifierType::ALT_MASK | ModifierType::SHIFT_MASK),
            Some((-1, true))
        );
        assert_eq!(
            word_movement(
                Key::Right,
                ModifierType::ALT_MASK | ModifierType::SHIFT_MASK
            ),
            Some((1, true))
        );
    }

    #[test]
    fn a_bare_arrow_with_no_option_is_left_alone() {
        // Plain caret-by-character movement is GTK's own binding on the view
        // already; answering here would double-handle every arrow press.
        assert_eq!(word_movement(Key::Left, ModifierType::empty()), None);
    }

    #[test]
    fn control_plus_option_is_someone_elses_combination() {
        // Not a real Mac chord (Ctrl+Option+Left has no system meaning here), but
        // the rule is general: any modifier beyond Option/Shift means this
        // function must not answer for it.
        assert_eq!(
            word_movement(
                Key::Left,
                ModifierType::ALT_MASK | ModifierType::CONTROL_MASK
            ),
            None
        );
    }

    #[test]
    fn command_option_left_is_someone_elses_combination() {
        // A `<Meta><Alt>` accelerator (this app declares several, e.g.
        // `win.copy-document`'s `<Primary><Meta><Alt>c` family on other keys) must
        // never be reinterpreted as word movement just because Option is held.
        assert_eq!(
            word_movement(Key::Left, ModifierType::ALT_MASK | ModifierType::META_MASK),
            None
        );
    }

    #[test]
    fn a_lock_modifier_riding_along_does_not_defeat_the_match() {
        assert_eq!(
            word_movement(Key::Left, ModifierType::ALT_MASK | ModifierType::LOCK_MASK),
            Some((-1, false))
        );
    }

    #[test]
    fn a_non_navigation_key_means_nothing_here() {
        assert_eq!(word_movement(Key::a, ModifierType::ALT_MASK), None);
        assert_eq!(word_movement(Key::Home, ModifierType::ALT_MASK), None);
    }
}
