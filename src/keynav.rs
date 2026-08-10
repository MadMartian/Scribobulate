//! `keynav` — the display-free half of "a navigation key moves the *document*".
//!
//! A text pane's navigation keys (`Home`, `End`, `Ctrl+Home`, the arrows, the page
//! keys) are `GtkTextView` class key bindings, so they fire only when the event
//! reaches the view. In the preview that is not a given: a table cell is a real
//! widget anchored in the buffer, it takes focus when the reader clicks or tabs into
//! it, and a focused **selectable `GtkLabel`** consumes the horizontal and
//! buffer-ends keys with its own `move-cursor` bindings — they never bubble to the
//! host view, so the document does not move and nothing at all appears to happen
//! (ScrAP-264).
//!
//! This module owns the two decisions the repair turns on, both as pure functions
//! over plain data so they are settled without a display:
//!
//! * [`document_movement`] — which caret movement a key press means, mirroring
//!   `gtktextview.c`'s own binding table, so a redirected key does exactly what the
//!   same key does when the view holds focus and nothing else.
//! * [`FocusSite`] — whether the widget holding focus is an anchored child of the
//!   pane (so the key is the document's to act on) rather than the pane itself, a
//!   popover parented to it, or something the reader is typing into.
//!
//! The GTK side — the capture-phase controller that reads these and emits
//! `move-cursor` — is `codeview::navkeys`.

use gtk::gdk::{Key, ModifierType};
use gtk::MovementStep;

/// One caret movement, in `GtkTextView`'s own terms: the step it moves by and how
/// many of them, negative counting backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Movement {
    pub(crate) step: MovementStep,
    pub(crate) count: i32,
}

/// Where the keyboard focus sits relative to a text pane, reduced to the four facts
/// that decide whether a navigation key belongs to the document.
///
/// Kept as data rather than as a widget query so the rule below is testable without a
/// display; `codeview::navkeys::focus_site` is the one place that answers these from
/// live widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusSite {
    /// The focused widget *is* the pane. GTK's own bindings already work; leave them.
    pub(crate) is_the_pane: bool,
    /// The focused widget is the pane or a descendant of it.
    pub(crate) inside_the_pane: bool,
    /// The focused widget is in the same `GtkNative` (the same GDK surface) as the
    /// pane. A `GtkPopover` `set_parent`ed to the view — the annotation card — is a
    /// descendant of the pane in the widget tree but a surface of its own, and its
    /// contents' keys are emphatically not the document's.
    pub(crate) shares_the_pane_surface: bool,
    /// The focused widget implements `GtkEditable` — an entry the reader is typing
    /// into, whose `Home`/`End`/arrows are its own (GTK4Rs/AP-120/GTK4Rs/AP-121).
    pub(crate) editable: bool,
}

impl FocusSite {
    /// Whether focus sits on an anchored child of the pane, so a navigation key the
    /// child would otherwise swallow should move the document instead.
    ///
    /// All four facts are load-bearing and none implies another: a popover parented
    /// to the view satisfies the first two, and an editable inside an anchored child
    /// would satisfy the first three.
    pub(crate) fn is_an_anchored_child(self) -> bool {
        let FocusSite {
            is_the_pane,
            inside_the_pane,
            shares_the_pane_surface,
            editable,
        } = self;
        inside_the_pane && !is_the_pane && shares_the_pane_surface && !editable
    }
}

/// The caret movement `key` means for a document pane, or `None` if it means none.
///
/// **This is a mirror of `gtktextview.c`'s own key bindings, deliberately** — the
/// redirected key must be indistinguishable from the same key pressed with the view
/// focused, and the only way to guarantee that is to emit what GTK would have.
///
/// The table covers the whole navigation set rather than the subset a selectable
/// `GtkLabel` was measured to swallow (`Home`, `End`, `Ctrl+Home`, `Ctrl+End`,
/// `Left`, `Right` — the vertical and page keys bubble through untouched on GTK
/// 4.6.9). Redirecting a key the child would have let through is a no-op by
/// construction, since this produces the same emission GTK's binding would; keying
/// the table to one version's swallow-set is what would rot.
///
/// **`Shift` is deliberately absent.** A selection-extending key inside a cell
/// extends *that cell's* selection, which is the only selection a table cell can
/// have (tables are selection islands), so it stays with the child. Any other
/// modifier combination is somebody else's — an accelerator, most likely — and is
/// left alone.
pub(crate) fn document_movement(key: Key, mods: ModifierType) -> Option<Movement> {
    // The modifiers that distinguish one binding from another; the rest (Caps Lock,
    // Num Lock, held mouse buttons) ride along on real events and mean nothing here.
    let significant = ModifierType::SHIFT_MASK
        | ModifierType::CONTROL_MASK
        | ModifierType::ALT_MASK
        | ModifierType::SUPER_MASK
        | ModifierType::HYPER_MASK
        | ModifierType::META_MASK;
    let held = mods & significant;
    let ctrl = if held.is_empty() {
        false
    } else if held == ModifierType::CONTROL_MASK {
        true
    } else {
        return None;
    };

    use MovementStep as Step;
    // (plain step, Ctrl step, direction) — `gtktextview.c` `gtk_text_view_class_init`.
    let (plain, control, count) = match key {
        Key::Left | Key::KP_Left => (Step::VisualPositions, Step::Words, -1),
        Key::Right | Key::KP_Right => (Step::VisualPositions, Step::Words, 1),
        Key::Up | Key::KP_Up => (Step::DisplayLines, Step::Paragraphs, -1),
        Key::Down | Key::KP_Down => (Step::DisplayLines, Step::Paragraphs, 1),
        Key::Home | Key::KP_Home => (Step::DisplayLineEnds, Step::BufferEnds, -1),
        Key::End | Key::KP_End => (Step::DisplayLineEnds, Step::BufferEnds, 1),
        Key::Page_Up | Key::KP_Page_Up => (Step::Pages, Step::HorizontalPages, -1),
        Key::Page_Down | Key::KP_Page_Down => (Step::Pages, Step::HorizontalPages, 1),
        _ => return None,
    };
    Some(Movement {
        step: if ctrl { control } else { plain },
        count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(is_the_pane: bool, inside: bool, same_surface: bool, editable: bool) -> FocusSite {
        FocusSite {
            is_the_pane,
            inside_the_pane: inside,
            shares_the_pane_surface: same_surface,
            editable,
        }
    }

    #[test]
    fn focus_on_an_anchored_cell_makes_the_key_the_documents() {
        assert!(site(false, true, true, false).is_an_anchored_child());
    }

    #[test]
    fn focus_on_the_pane_itself_is_left_to_gtks_own_bindings() {
        // Redirecting here would double-handle every navigation key in the pane that
        // is working correctly today.
        assert!(!site(true, true, true, false).is_an_anchored_child());
    }

    #[test]
    fn focus_outside_the_pane_is_not_ours() {
        assert!(!site(false, false, true, false).is_an_anchored_child());
    }

    #[test]
    fn focus_in_a_popover_parented_to_the_pane_is_not_ours() {
        // The annotation card is a descendant of the view AND a surface of its own —
        // the case the widget-tree test alone gets wrong.
        assert!(!site(false, true, false, false).is_an_anchored_child());
    }

    #[test]
    fn focus_in_something_the_reader_types_into_is_not_ours() {
        assert!(!site(false, true, true, true).is_an_anchored_child());
    }

    #[test]
    fn the_buffer_ends_keys_map_to_gtks_own_binding() {
        assert_eq!(
            document_movement(Key::Home, ModifierType::CONTROL_MASK),
            Some(Movement {
                step: MovementStep::BufferEnds,
                count: -1
            })
        );
        assert_eq!(
            document_movement(Key::End, ModifierType::CONTROL_MASK),
            Some(Movement {
                step: MovementStep::BufferEnds,
                count: 1
            })
        );
    }

    #[test]
    fn a_bare_home_or_end_moves_within_the_display_line() {
        assert_eq!(
            document_movement(Key::Home, ModifierType::empty()),
            Some(Movement {
                step: MovementStep::DisplayLineEnds,
                count: -1
            })
        );
        assert_eq!(
            document_movement(Key::End, ModifierType::empty()),
            Some(Movement {
                step: MovementStep::DisplayLineEnds,
                count: 1
            })
        );
    }

    #[test]
    fn control_promotes_a_horizontal_step_to_words() {
        assert_eq!(
            document_movement(Key::Right, ModifierType::CONTROL_MASK),
            Some(Movement {
                step: MovementStep::Words,
                count: 1
            })
        );
        assert_eq!(
            document_movement(Key::Left, ModifierType::empty()),
            Some(Movement {
                step: MovementStep::VisualPositions,
                count: -1
            })
        );
    }

    #[test]
    fn the_vertical_and_page_keys_are_covered_too() {
        // They bubble through a focused label on GTK 4.6.9 and so are not part of the
        // observed defect; the table carries them anyway so it cannot rot into a
        // per-version swallow-list.
        assert_eq!(
            document_movement(Key::Down, ModifierType::empty()),
            Some(Movement {
                step: MovementStep::DisplayLines,
                count: 1
            })
        );
        assert_eq!(
            document_movement(Key::Page_Up, ModifierType::empty()),
            Some(Movement {
                step: MovementStep::Pages,
                count: -1
            })
        );
    }

    #[test]
    fn the_keypad_duplicates_of_every_navigation_key_are_covered() {
        // A numeric keypad with Num Lock off sends the KP_ keyvals, and GTK binds
        // both — a table that carried only the main block would fix the bug for one
        // keyboard and not the other.
        assert_eq!(
            document_movement(Key::KP_Home, ModifierType::CONTROL_MASK),
            document_movement(Key::Home, ModifierType::CONTROL_MASK)
        );
        assert_eq!(
            document_movement(Key::KP_Page_Down, ModifierType::empty()),
            document_movement(Key::Page_Down, ModifierType::empty())
        );
    }

    #[test]
    fn a_selection_extending_key_stays_with_the_cell() {
        // Shift+Home inside a cell selects that cell's text — the only selection a
        // table cell can hold. Redirecting it would take the feature away.
        assert_eq!(document_movement(Key::Home, ModifierType::SHIFT_MASK), None);
        assert_eq!(
            document_movement(
                Key::Home,
                ModifierType::SHIFT_MASK | ModifierType::CONTROL_MASK
            ),
            None
        );
    }

    #[test]
    fn an_accelerator_modifier_is_left_alone() {
        assert_eq!(document_movement(Key::Home, ModifierType::ALT_MASK), None);
        assert_eq!(document_movement(Key::Left, ModifierType::SUPER_MASK), None);
    }

    #[test]
    fn a_lock_modifier_riding_along_does_not_defeat_the_match() {
        // Caps Lock, Num Lock and held mouse buttons arrive on real events; matching
        // on the raw mask would make the fix fail for anyone with Num Lock on.
        assert_eq!(
            document_movement(
                Key::Home,
                ModifierType::CONTROL_MASK | ModifierType::LOCK_MASK
            ),
            Some(Movement {
                step: MovementStep::BufferEnds,
                count: -1
            })
        );
    }

    #[test]
    fn a_key_that_is_not_navigation_means_nothing_here() {
        assert_eq!(document_movement(Key::a, ModifierType::empty()), None);
        assert_eq!(document_movement(Key::Tab, ModifierType::empty()), None);
        assert_eq!(document_movement(Key::Return, ModifierType::empty()), None);
    }
}
