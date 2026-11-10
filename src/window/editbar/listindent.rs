//! Tab / Shift+Tab on a list item: Increase / Decrease Indent from the keyboard,
//! the way every outliner and word processor does it. The rule is the pure
//! [`format::list_indent`]; this module only decides when a Tab is ours.
//!
//! # Why a key controller and not an accelerator
//!
//! The commands' own accelerators (Ctrl+] / Ctrl+[) are application accelerators,
//! bound for the whole window. Tab cannot be one: it moves focus between controls,
//! and in the editor it types a tab on every line that is not a list item. So the
//! editor takes Tab only when the caret's line, or a line of the selection, is a
//! list item, and passes every other Tab on untouched. On a list item whose level
//! cannot change (the first item of a list, an item at the margin) it is still
//! claimed, so the key never drops a stray tab into the item.
//!
//! CAPTURE phase, beside the Enter conveniences ([`super::newline`]): GtkSourceView
//! indents a multi-line selection on Tab from its own capture-phase controller
//! (`indent-on-tab`). GTK runs a widget's controllers newest first
//! (`gtk_widget_add_controller` prepends, `gtk_widget_run_controllers` walks
//! forward; gtkwidget.c, 4.6.9), and this one is added after the view is built, so
//! it answers first.

use super::super::*;
use super::edit::{apply_edit, selection_range_and_text};

/// The controller's name, so a test can find it among the view's controllers.
const LIST_INDENT_KEYS_NAME: &str = "scribobulate-list-indent-keys";

/// Which way a key press moves list items, or `None` if it is not a plain Tab or
/// Shift+Tab. Shift+Tab arrives as `ISO_Left_Tab` on X11 and Windows; any other
/// modifier (Ctrl+Tab, Alt+Tab) belongs to someone else.
fn key_dir(keyval: gdk::Key, state: gdk::ModifierType) -> Option<format::IndentDir> {
    // The modifiers a person holds, spelled out rather than asked of
    // `gtk::accelerator_get_default_mod_mask`, which needs GTK initialised and so
    // would keep this decision out of the plain unit tests. Lock keys are ignored.
    let held = gdk::ModifierType::SHIFT_MASK
        | gdk::ModifierType::CONTROL_MASK
        | gdk::ModifierType::ALT_MASK
        | gdk::ModifierType::SUPER_MASK
        | gdk::ModifierType::HYPER_MASK
        | gdk::ModifierType::META_MASK;
    let mods = state.intersection(held);
    let shift = mods == gdk::ModifierType::SHIFT_MASK;
    if !(mods.is_empty() || shift) {
        return None;
    }
    match keyval {
        gdk::Key::ISO_Left_Tab => Some(format::IndentDir::Out),
        gdk::Key::Tab | gdk::Key::KP_Tab if shift => Some(format::IndentDir::Out),
        gdk::Key::Tab | gdk::Key::KP_Tab => Some(format::IndentDir::In),
        _ => None,
    }
}

/// The text of the whole lines the selection (or caret) spans, and the selection
/// relative to it — enough to tell whether a list item is involved without copying
/// the whole document on every Tab.
fn spanned_lines(buf: &gtk::TextBuffer) -> (String, usize, usize) {
    let (s, e) = buf.selection_bounds().unwrap_or_else(|| {
        let c = buf.iter_at_mark(&buf.get_insert());
        (c, c)
    });
    let mut first = s;
    first.set_line_offset(0);
    let mut last = e;
    if !last.ends_line() {
        last.forward_to_line_end();
    }
    let text = crate::saferizer::BufferText::of_range(buf, &first, &last).into_string();
    let base = first.offset();
    (
        text,
        (s.offset() - base) as usize,
        (e.offset() - base) as usize,
    )
}

/// Tab / Shift+Tab indent and dedent list items in `view` (see the module docs).
pub(crate) fn wire_list_indent_keys(view: &sourceview::View) {
    let keys = gtk::EventControllerKey::new();
    keys.set_name(Some(LIST_INDENT_KEYS_NAME));
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        view,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, keyval, _, state| {
            let Some(dir) = key_dir(keyval, state).filter(|_| view.is_editable()) else {
                return glib::Propagation::Proceed;
            };
            let Ok(buf) = view.buffer().downcast::<sourceview::Buffer>() else {
                return glib::Propagation::Proceed;
            };
            let (span, s, e) = spanned_lines(buf.upcast_ref());
            if format::list_indent(&span, s, e, dir) == format::ListIndent::NotAList {
                return glib::Propagation::Proceed;
            }
            let (sel_start, sel_end, _) = selection_range_and_text(&buf);
            let text = crate::saferizer::BufferText::of(&buf).into_string();
            if let format::ListIndent::Changed(edit) =
                format::list_indent(&text, sel_start, sel_end, dir)
            {
                apply_edit(&buf, &edit);
                view.scroll_mark_onscreen(&buf.get_insert());
            }
            glib::Propagation::Stop
        }
    ));
    view.add_controller(keys);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_tab_or_shift_tab_is_a_direction() {
        let none = gdk::ModifierType::empty();
        let shift = gdk::ModifierType::SHIFT_MASK;
        assert_eq!(key_dir(gdk::Key::Tab, none), Some(format::IndentDir::In));
        assert_eq!(key_dir(gdk::Key::KP_Tab, none), Some(format::IndentDir::In));
        assert_eq!(
            key_dir(gdk::Key::ISO_Left_Tab, shift),
            Some(format::IndentDir::Out)
        );
        assert_eq!(key_dir(gdk::Key::Tab, shift), Some(format::IndentDir::Out));
        assert_eq!(
            key_dir(gdk::Key::Tab, gdk::ModifierType::CONTROL_MASK),
            None
        );
        assert_eq!(key_dir(gdk::Key::Tab, gdk::ModifierType::ALT_MASK), None);
        assert_eq!(key_dir(gdk::Key::a, none), None);
    }
}

/// The controller as GTK delivers to it (`key-pressed` on the named controller).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    fn editor(md: &str) -> (sourceview::Buffer, sourceview::View) {
        let buf = sourceview::Buffer::new(None);
        buf.set_enable_undo(true);
        // Loaded as the real editor loads a document: not an undo step of its own.
        buf.begin_irreversible_action();
        buf.set_text(md);
        buf.end_irreversible_action();
        let view = sourceview::View::with_buffer(&buf);
        view.set_editable(true);
        wire_list_indent_keys(&view);
        (buf, view)
    }

    fn press(view: &sourceview::View, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        let controllers = view.observe_controllers();
        let key = (0..controllers.n_items())
            .filter_map(|i| {
                controllers
                    .item(i)
                    .and_downcast::<gtk::EventControllerKey>()
            })
            .find(|c| c.name().as_deref() == Some(LIST_INDENT_KEYS_NAME))
            .expect("wire_list_indent_keys installs its controller");
        key.emit_by_name("key-pressed", &[&keyval, &0u32, &state])
    }

    fn text_of(buf: &sourceview::Buffer) -> String {
        crate::saferizer::BufferText::of(buf).into_string()
    }

    #[gtktest::test]
    fn tab_nests_a_list_item_and_shift_tab_lifts_it_in_one_undo_step_each() {
        let (buf, view) = editor("- a\n- b");
        buf.place_cursor(&buf.end_iter());
        assert!(press(&view, gdk::Key::Tab, gdk::ModifierType::empty()));
        assert_eq!(text_of(&buf), "- a\n  - b");
        assert_eq!(
            buf.iter_at_mark(&buf.get_insert()).offset(),
            9,
            "caret stays after b"
        );
        assert!(press(
            &view,
            gdk::Key::ISO_Left_Tab,
            gdk::ModifierType::SHIFT_MASK
        ));
        assert_eq!(text_of(&buf), "- a\n- b");
        buf.undo();
        assert_eq!(
            text_of(&buf),
            "- a\n  - b",
            "one Undo reverts only the dedent"
        );
    }

    /// The Keyboard Shortcuts window advertises `EDITOR_KEYS`; this handler must take
    /// exactly those keys, each for the command it is listed under.
    #[gtktest::test]
    fn the_handler_takes_the_keys_the_shortcuts_window_shows() {
        for k in crate::app::EDITOR_KEYS.iter() {
            let (keyval, mods) = gtk::accelerator_parse(k.accel).expect("parseable");
            let want = match k.target {
                "indent" => format::IndentDir::In,
                "dedent" => format::IndentDir::Out,
                other => panic!("EDITOR_KEYS target {other:?} has no handler here"),
            };
            assert_eq!(key_dir(keyval, mods), Some(want), "{:?}", k.accel);
        }
    }

    #[gtktest::test]
    fn tab_off_a_list_item_is_left_to_the_editor() {
        let (buf, view) = editor("plain");
        buf.place_cursor(&buf.end_iter());
        assert!(!press(&view, gdk::Key::Tab, gdk::ModifierType::empty()));
        assert_eq!(text_of(&buf), "plain");
    }

    #[gtktest::test]
    fn tab_on_an_item_that_cannot_move_is_claimed_and_changes_nothing() {
        let (buf, view) = editor("- a");
        buf.place_cursor(&buf.end_iter());
        assert!(press(&view, gdk::Key::Tab, gdk::ModifierType::empty()));
        assert_eq!(text_of(&buf), "- a");
        assert!(!buf.can_undo(), "nothing to undo");
    }

    #[gtktest::test]
    fn a_read_only_editor_keeps_its_tab() {
        let (buf, view) = editor("- a\n- b");
        view.set_editable(false);
        buf.place_cursor(&buf.end_iter());
        assert!(!press(&view, gdk::Key::Tab, gdk::ModifierType::empty()));
        assert_eq!(text_of(&buf), "- a\n- b");
    }
}
