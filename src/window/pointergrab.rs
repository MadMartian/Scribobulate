//! Releasing the pointer grab GTK leaves behind after an extra mouse button.
//!
//! ## The GTK defect
//!
//! Every button press makes the widget under the pointer the window's implicit grab,
//! and GTK ends that grab on the release — but only when the release event's modifier
//! state shows exactly one of `GDK_BUTTON1_MASK`..`GDK_BUTTON5_MASK` held
//! (`gtkmain.c` `handle_pointing_event`, the `GDK_BUTTON_RELEASE` branch: GTK 4.6.9
//! about line 1457, unchanged on upstream `main`). GDK has no mask bit for a button
//! above 5, so the release of a mouse's thumb buttons (8 and 9 on X11 and Wayland)
//! never matches, and the grab outlives the click.
//!
//! While it lasts, every motion and press goes to the grabbed widget instead of the
//! widget under the pointer. Reported as: press the thumb Back button, the document
//! returns to the table of contents, the pointer over a link shows the text beam, and
//! the first click on the link does nothing. That click's release is a button-1
//! release, which finally ends the grab, so the second click works. The toolbar's
//! Back button never shows it because it is a button-1 click.
//!
//! On Windows and macOS GDK reports the thumb buttons as 4 and 5, which do have mask
//! bits, so this repair has nothing to do there. It is shared code all the same: it
//! keys on the button number, not on the platform.
//!
//! ## The repair
//!
//! GTK drops a grab whose widget, or an ancestor of it, changes sensitivity
//! (`gtk_widget_set_sensitive` → `gtk_window_update_pointer_focus_on_state_change`,
//! `gtkwindow.c`). So after an extra button's release, the widget the press grabbed is
//! made insensitive and sensitive again in one main-loop turn: no frame is drawn in
//! between, and the grab is gone. Which widget holds the grab is GTK's private state,
//! so the toggle is applied to the window's whole content, which holds it whichever
//! widget it is (see [`release_grab_within`]).
//!
//! Turning a widget insensitive also takes the keyboard focus off it when it holds it
//! (`gtk_widget_propagate_state`), so the focus is handed back afterwards. It also
//! clears every selection inside it (`GtkTextView` and a selectable `GtkLabel` each
//! unselect in their `state_flags_changed`), so the selections are read before the
//! toggle and put back after it. The focus hand-back is a programmatic focus grab,
//! which makes a selectable `GtkLabel` select all its text and a `GtkText` do the same
//! (GTK4Rs/AP-107, GTK4Rs/AP-124), so both settings are off for the duration and
//! returned to the platform afterwards.

use gtk::glib;
use gtk::prelude::*;

/// The highest pointer button GDK has a modifier mask for (`GDK_BUTTON5_MASK`).
const LAST_MASKED_BUTTON: u32 = 5;

/// Does the release of `button` leave GTK's implicit pointer grab in place?
fn release_strands_grab(button: u32) -> bool {
    button > LAST_MASKED_BUTTON
}

/// Watch `window` for releases of buttons GTK cannot ungrab, and release the grab each
/// one leaves.
pub(super) fn release_stranded_pointer_grabs(window: &gtk::ApplicationWindow) {
    // Observe only, in the capture phase so nothing below can hide the event from it,
    // and never claiming: the thumb buttons' own navigation gestures must still see it.
    let legacy = gtk::EventControllerLegacy::new();
    legacy.set_propagation_phase(gtk::PropagationPhase::Capture);
    legacy.connect_event(|controller, event| {
        let stranded = event.event_type() == gtk::gdk::EventType::ButtonRelease
            && event
                .downcast_ref::<gtk::gdk::ButtonEvent>()
                .is_some_and(|b| release_strands_grab(b.button()));
        if let (true, Some(window)) = (stranded, controller.widget()) {
            log::debug!("pointer grab: releasing the grab an extra mouse button left");
            // Not from inside the event's own propagation: the release is still on its
            // way to the grabbed widget, and an insensitive widget is skipped.
            let weak = window.downgrade();
            glib::idle_add_local_once(move || {
                if let Some(content) = weak
                    .upgrade()
                    .and_then(|w| w.downcast::<gtk::Window>().ok())
                    .and_then(|w| w.child())
                {
                    release_grab_within(&content);
                }
            });
        }
        glib::Propagation::Proceed
    });
    window.add_controller(legacy);
}

/// Drop any implicit pointer grab held by `content` or a descendant of it, leaving its
/// sensitivity and the window's keyboard focus as they were.
///
/// Called with the window's whole content rather than the widget under the pointer:
/// GTK grabs the widget it last picked on a MOTION, and a document that scrolled under
/// a still pointer (the thumb button's own navigation does exactly that) leaves that
/// widget somewhere other than under the pointer. Not the window itself: a toplevel's
/// own state change makes GTK discard its pointer record altogether (`gtkwindow.c`,
/// the "Unmapping the toplevel" branch), which is more than this needs.
pub(super) fn release_grab_within(content: &gtk::Widget) {
    if !content.is_realized() || !content.is_sensitive() {
        // An insensitive widget already had its grab dropped when it became so.
        return;
    }
    let root = content.root();
    let focus = root.as_ref().and_then(|r| r.focus());
    let mut kept = Vec::new();
    selections_within(content, &mut kept);
    content.set_sensitive(false);
    content.set_sensitive(true);
    if let (Some(root), Some(focus)) = (root, focus) {
        if root.focus().as_ref() != Some(&focus) && focus.is_sensitive() && focus.is_mapped() {
            without_select_on_focus(&focus, || {
                focus.grab_focus();
            });
        }
    }
    for selection in kept {
        selection.restore();
    }
}

/// A selection the sensitivity toggle clears, as it was before.
enum Selection {
    /// The buffer offsets of the insert mark and the selection bound, in that order.
    Text(gtk::TextView, i32, i32),
    /// The start and end character offsets.
    Label(gtk::Label, i32, i32),
}

impl Selection {
    fn restore(self) {
        match self {
            Selection::Text(view, insert, bound) => {
                let buffer = view.buffer();
                buffer.select_range(
                    &buffer.iter_at_offset(insert),
                    &buffer.iter_at_offset(bound),
                );
            }
            Selection::Label(label, start, end) => label.select_region(start, end),
        }
    }
}

/// Every non-empty selection in `widget` and its descendants.
///
/// A walk of the tree, because what it reads is whatever the toggle is about to touch,
/// which is the whole subtree. It runs once per extra-button release.
fn selections_within(widget: &gtk::Widget, kept: &mut Vec<Selection>) {
    if let Some(view) = widget.downcast_ref::<gtk::TextView>() {
        let buffer = view.buffer();
        if buffer.has_selection() {
            let insert = buffer.iter_at_mark(&buffer.get_insert()).offset();
            let bound = buffer.iter_at_mark(&buffer.selection_bound()).offset();
            kept.push(Selection::Text(view.clone(), insert, bound));
        }
    } else if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        if let Some((start, end)) = label.selection_bounds() {
            kept.push(Selection::Label(label.clone(), start, end));
        }
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        selections_within(&c, kept);
        child = c.next_sibling();
    }
}

/// Run `regrab` with GTK's select-all-on-focus settings for labels and entries off,
/// handing both back to the platform afterwards rather than pinning them.
fn without_select_on_focus(widget: &gtk::Widget, regrab: impl FnOnce()) {
    let settings = widget.settings();
    let label = settings.is_gtk_label_select_on_focus();
    let entry = settings.is_gtk_entry_select_on_focus();
    if label {
        settings.set_gtk_label_select_on_focus(false);
    }
    if entry {
        settings.set_gtk_entry_select_on_focus(false);
    }
    regrab();
    if label {
        settings.reset_property("gtk-label-select-on-focus");
    }
    if entry {
        settings.reset_property("gtk-entry-select-on-focus");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_buttons_without_a_mask_bit_strand_the_grab() {
        for button in 1..=LAST_MASKED_BUTTON {
            assert!(
                !release_strands_grab(button),
                "button {button} has a mask bit"
            );
        }
        // The thumb buttons as X11 and Wayland report them, and the next ones up.
        for button in [6, 7, 8, 9, 10] {
            assert!(
                release_strands_grab(button),
                "button {button} has no mask bit"
            );
        }
    }
}

/// What the repair does to the window it runs in. The grab itself is GTK's private
/// state and only a real pointer press makes one, so whether the grab goes is proved
/// against the running app (MANUAL-TEST 23.6a). What a test can hold is the cost of the
/// toggle: everything it touches must come back as it was.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::testpump::{until, Clock};

    fn present(child: &impl IsA<gtk::Widget>) -> gtk::Window {
        let window = gtk::Window::new();
        window.set_default_size(400, 200);
        window.set_child(Some(child));
        window.present();
        until(Clock::Idle, "the window to map", || window.is_mapped());
        window
    }

    /// A table cell is a selectable label, and the focus hand-back is a programmatic
    /// grab, which selects all of such a label's text. The reader's own partial
    /// selection must survive.
    ///
    /// Mutation-checked: without the restore the selection reads None; without
    /// `without_select_on_focus` it reads (0, 11); without the hand-back the label is
    /// no longer the focus.
    #[gtktest::test]
    fn a_focused_cells_selection_and_focus_survive_the_release() {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let cell = gtk::Label::new(Some("hello world"));
        cell.set_selectable(true);
        content.append(&cell);
        let window = present(&content);
        cell.grab_focus();
        cell.select_region(2, 5);
        until(Clock::Idle, "the cell to take the focus", || {
            cell.has_focus()
        });

        release_grab_within(content.upcast_ref());

        assert!(content.is_sensitive(), "the content is sensitive again");
        assert!(cell.has_focus(), "the cell has the focus back");
        assert_eq!(
            cell.selection_bounds(),
            Some((2, 5)),
            "the selection is untouched"
        );
        window.destroy();
    }

    /// The editor and the preview are text views, which unselect when they turn
    /// insensitive. The selection comes back, its direction included.
    ///
    /// Mutation-checked: without the restore the selection is gone.
    #[gtktest::test]
    fn a_text_views_selection_survives_the_release() {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let view = gtk::TextView::new();
        let buffer = view.buffer();
        buffer.set_text("hello world");
        content.append(&view);
        let window = present(&content);
        view.grab_focus();
        // Selected right to left: the insert mark sits before the bound.
        buffer.select_range(&buffer.iter_at_offset(2), &buffer.iter_at_offset(8));
        until(Clock::Idle, "the view to take the focus", || {
            view.has_focus()
        });

        release_grab_within(content.upcast_ref());

        assert!(view.has_focus(), "the view has the focus back");
        let insert = buffer.iter_at_mark(&buffer.get_insert()).offset();
        let bound = buffer.iter_at_mark(&buffer.selection_bound()).offset();
        assert_eq!((insert, bound), (2, 8), "the selection is untouched");
        window.destroy();
    }

    /// The same for a text entry, whose own select-on-focus setting is a different one.
    #[gtktest::test]
    fn a_focused_entrys_caret_survives_the_release() {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let entry = gtk::Entry::new();
        entry.set_text("find me");
        content.append(&entry);
        let window = present(&content);
        entry.grab_focus();
        entry.set_position(4);
        until(Clock::Idle, "the entry to take the focus", || {
            gtk::prelude::RootExt::focus(&window).is_some_and(|f| f.is_ancestor(&entry))
        });

        release_grab_within(content.upcast_ref());

        assert!(
            gtk::prelude::RootExt::focus(&window).is_some_and(|f| f.is_ancestor(&entry)),
            "the entry has the focus back"
        );
        assert_eq!(entry.selection_bounds(), None, "nothing was selected");
        assert_eq!(entry.position(), 4, "the caret stayed put");
        window.destroy();
    }
}
