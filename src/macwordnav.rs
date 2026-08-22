//! Option+Left / Option+Right word navigation in the editor, on macOS only.
//!
//! **The module compiles everywhere; only the WIRING is `cfg`-gated.** [`word_movement`]
//! is a pure function over a keyval and a modifier mask — there is nothing
//! platform-bound about deciding what Option+Left MEANS, and a `#[cfg]` on the module
//! declaration deleted its nine tests from every platform but the target. Not skipped,
//! not reported, not counted: `cargo test --lib -- --list` found zero cases here on the
//! platform POLICY names as canonical, so the decision that carries the whole feature
//! was unverified by the only gate that runs (ScrAP-212, which
//! `scripts/pipeline.steps` states as a rule about tests and which this module was
//! breaking through its module declaration instead).
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
//! **Scope: two wirings, one decision function, two different defects.**
//!
//! [`wire_word_navigation`] covers the main document editor (`build_tab_editor`, the
//! one place every editor `sourceview::View` is built) — the surface the report was
//! about, and the only one that carries the *mutation* defect above, because
//! `move-words` is `GtkSourceView`'s binding and nothing else in the tree is a
//! `sourceview::View`.
//!
//! [`wire_field_word_navigation`] covers the in-app text fields — the shared prompt
//! form behind Go To Line, Rename and Insert Link/Image/Table (`window::editbar::dialog`), the
//! annotation comment entry (`widgets::comment_entry`), and the find and replace
//! fields (`window::chrome`). Those are plain `GtkEditable` wrappers with **no**
//! `move-words` binding, so they never rearranged anything — an earlier revision of
//! this paragraph concluded from that there was "nothing to fold into this fix there",
//! and that was wrong. Absence of the mutation is not presence of the navigation:
//! `GtkText` binds word movement to Ctrl+Left/Right only, so Option+Left in any of
//! these fields did nothing at all, against a convention every native macOS text field
//! honours. Inert is a milder defect than destructive, not a different one, and both
//! are the same gap between GTK's Ctrl-based bindings and AppKit's Option-based ones.
//! Hence one [`word_movement`] and two thin wirings, never two decision functions.
//!
//! **The field wiring goes on the delegate, not on the widget the caller built** — see
//! [`key_target`]. That is the one non-obvious mechanical fact here and it decides both
//! halves of the wiring: where the controller is attached *and* what the signal is
//! emitted on.
//!
//! **Why a plain key controller and not a `GtkShortcutController`/accelerator.** A
//! bubble-phase `GtkEventControllerKey` on the view, added after construction, runs
//! ahead of the view's own class keybindings (see above) — that ordering is exactly
//! the mechanism this fix depends on, not an incidental choice. Capture phase (as
//! `codeview::navkeys` uses, to run ahead of a focused *descendant's* bindings) is not
//! needed here: this view has no such descendant, and bubble already outruns the
//! view's own binding set.
use gtk::gdk::{Key, ModifierType};
// The wiring's imports, gated with the wiring itself: on a platform that compiles only
// the decision, an ungated `use` here is an unused import and the `-D warnings` gate is
// a build failure. `prelude` is a glob and needs no gate.
#[cfg(target_os = "macos")]
use gtk::glib;
#[cfg(target_os = "macos")]
use gtk::prelude::*;
#[cfg(target_os = "macos")]
use gtk::MovementStep;

/// The word-movement Option+`key` means for a text surface, or `None` if `key`/`mods`
/// is not that combination.
///
/// A pure function over plain data (mirroring `crate::keynav::document_movement`'s
/// own shape) so the decision is unit-testable without a display, and so both callers
/// — the editor view and the `GtkEditable` fields — share one answer rather than
/// growing a second, drifting copy of the modifier rules below.
///
/// Only a bare Option or Option+Shift qualifies. Any other modifier riding along
/// (Control, Command/Meta, Super, Hyper) means this is someone else's combination —
/// an accelerator, most likely — and must be left alone; matching on it here would
/// silently steal a keystroke this function has no business answering for.
/// Caps Lock / Num Lock are excluded from `significant` for the same reason
/// `keynav::document_movement` excludes them: they ride along on real events and
/// must not defeat the match for a reader who has either one on.
// Compiled on every platform so its tests are, which is the whole point of the split;
// only macOS has a caller, so every other target sees a function with none. The
// alternative is the `cfg` that deleted these tests in the first place (ScrAP-212).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // no caller off macOS; see above
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
#[cfg(target_os = "macos")]
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

/// The `GtkText` that actually receives key events for `field`, or `None` if `field`
/// is a `GtkEditable` implemented some other way.
///
/// **`GtkEntry`, `GtkSearchEntry` and friends are wrappers, not text widgets.** Each
/// one delegates its editing — focus, key events, and the `move-cursor` signal alike —
/// to an internal `GtkText` child reachable through `gtk_editable_get_delegate`. The
/// project already has this recorded from the other side: `editoractions`' test probe
/// `focus_is_within` exists because `entry.has_focus()` stays *false* for an entry the
/// reader is typing into, since the focus is on that child.
///
/// Two consequences, and together they are the whole reason this function exists
/// rather than the caller's widget being used directly:
///
/// 1. **The controller belongs on the delegate.** A controller on the wrapper sits
///    behind the delegate in the propagation chain, so a key the delegate answers is
///    already spent by the time the wrapper's bubble phase runs.
/// 2. **The emission target is the delegate.** `move-cursor` is `GtkText`'s signal;
///    `GtkEntry` has no such signal at all, so an emission aimed at the wrapper is not
///    a subtler bug, it is a panic.
///
/// A bare `gtk::Text` (no wrapper) is its own delegate — `gtk_editable_get_delegate`
/// returns NULL for it — so it is returned directly rather than being treated as an
/// unsupported shape.
#[cfg(target_os = "macos")]
pub(crate) fn key_target(field: &impl IsA<gtk::Editable>) -> Option<gtk::Text> {
    let field = field.as_ref();
    if let Some(text) = field.downcast_ref::<gtk::Text>() {
        return Some(text.clone());
    }
    field.delegate()?.downcast::<gtk::Text>().ok()
}

/// Wire Option+Left/Option+Right (and Option+Shift, to extend the selection) onto a
/// `GtkEditable` field as word movement — the sibling of [`wire_word_navigation`] for
/// everything in the app that is not the document editor.
///
/// Call once, at the field's construction site, so a field cannot be built without it
/// (the same siting rule as [`wire_word_navigation`], and for the same reason: an
/// obligation attached to a *use* site regresses at the next use site).
///
/// **Capture phase, deliberately.** `GtkText` does its own key handling through a
/// `GtkEventControllerKey` of its own, installed at construction, and the relative
/// order of two controllers on one widget in the same phase is a list-order detail of
/// `gtk_widget_add_controller` — not a documented contract, and not something this
/// wiring should depend on. Capture runs before target and bubble by definition, so
/// the ordering is decided by the phase rather than by construction order. This
/// differs from [`wire_word_navigation`], whose bubble phase was *measured* to outrun
/// `GtkSourceView`'s class binding; here there is no class binding to outrun (nothing
/// binds Option+Left in `GtkText`) and the only requirement is to answer before the
/// widget's own controller can decide the key means something else.
///
/// A field whose delegate is not a `GtkText` is left alone rather than half-wired —
/// see [`key_target`].
///
/// **Measured live on Quartz** (GTK 4.22.4, release build), each of the four sites in
/// turn, by typing `alpha beta gamma`, pressing Option+Left twice and typing `|`:
/// every one read back `alpha |beta gamma`, so the caret had moved two words rather
/// than none. The control was run first and matters, because a key that never reaches
/// the app is indistinguishable from a wiring that is not there (GTK4Rs/AP-245 — this
/// platform has such a channel, `cliclick kp:`): a plain Left arrow through the same
/// `System Events` route gave `alpha beta gamm^a`, proving delivery before any
/// Option result was believed. Option+Right moved forward one word
/// (`alpha |beta] gamma`) and Option+Shift+Left extended the selection over the
/// preceding word.
#[cfg(target_os = "macos")]
pub(crate) fn wire_field_word_navigation(field: &impl IsA<gtk::Editable>) {
    let Some(text) = key_target(field) else {
        log::debug!(
            "macwordnav: {} has no GtkText delegate; Option+Left/Right word navigation not wired",
            field.as_ref().type_().name()
        );
        return;
    };
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        text,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, key, _, mods| {
            let Some((count, extend)) = word_movement(key, mods) else {
                return glib::Propagation::Proceed;
            };
            // `GtkText` has no `emit_move_cursor` in gtk4-rs — `move-cursor` is bound
            // for connecting, not for emitting — but it is a `G_SIGNAL_ACTION` signal,
            // so emitting it by name is the sanctioned route and does exactly what
            // GTK's own Ctrl+Left/Right binding does.
            text.emit_by_name::<()>("move-cursor", &[&MovementStep::Words, &count, &extend]);
            glib::Propagation::Stop
        }
    ));
    text.add_controller(keys);
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

// The GTK half asserts on live controllers and delegates, so it stays with the wiring.
// The DECISION's tests, above, are display-free and compile everywhere on purpose.
#[cfg(all(test, target_os = "macos", feature = "gtk-integration-tests"))]
mod gtk_tests {
    use super::*;

    /// Every `GtkEventController` currently attached to `w`, in `observe_controllers`
    /// order.
    ///
    /// Returned as objects rather than counted so a test can identify the controller a
    /// wiring call ADDED (set difference by object identity) instead of asserting on a
    /// total — a count says a controller appeared, not that it is ours, and `GtkText`
    /// arrives with controllers of its own.
    fn controllers(w: &impl IsA<gtk::Widget>) -> Vec<gtk::EventController> {
        let model = w.as_ref().observe_controllers();
        (0..model.n_items())
            .filter_map(|i| model.item(i))
            .filter_map(|o| o.downcast::<gtk::EventController>().ok())
            .collect()
    }

    /// The capture-phase key controller `wire_field_word_navigation` installs, found on
    /// `field`'s delegate — the signature a production wiring site is checked by below.
    ///
    /// Phase is the discriminator because `GtkText` installs key controllers of its own
    /// and none of them is capture-phase (asserted directly by
    /// [`the_controller_lands_on_the_delegate_in_capture_phase`], which measures the
    /// before-state rather than assuming it).
    fn capture_key_controllers(field: &impl IsA<gtk::Editable>) -> usize {
        let Some(text) = key_target(field) else {
            return 0;
        };
        controllers(&text)
            .into_iter()
            .filter(|c| {
                c.is::<gtk::EventControllerKey>()
                    && c.propagation_phase() == gtk::PropagationPhase::Capture
            })
            .count()
    }

    #[gtktest::test]
    fn a_wrapper_entrys_key_target_is_the_gtk_text_inside_it() {
        for (what, field) in [
            ("GtkEntry", gtk::Entry::new().upcast::<gtk::Widget>()),
            ("GtkSearchEntry", gtk::SearchEntry::new().upcast()),
        ] {
            let editable = field
                .clone()
                .downcast::<gtk::Editable>()
                .unwrap_or_else(|_| panic!("{what} is a GtkEditable"));
            let target =
                key_target(&editable).unwrap_or_else(|| panic!("{what} delegates to a GtkText"));
            assert!(
                target.is_ancestor(&field),
                "{what}'s delegate must be the GtkText INSIDE it — a wiring aimed at the \
                 wrapper would sit behind that child in the propagation chain"
            );
        }
    }

    #[gtktest::test]
    fn a_bare_gtk_text_is_its_own_key_target() {
        // `gtk_editable_get_delegate` returns NULL for GtkText itself, so the
        // delegate lookup alone would report "unsupported" for the one widget that
        // needs no delegation at all.
        let text = gtk::Text::new();
        assert_eq!(key_target(&text).as_ref(), Some(&text));
    }

    #[gtktest::test]
    fn move_cursor_by_word_is_emittable_on_the_delegate() {
        // The load-bearing fact of the whole field wiring: gtk4-rs binds no
        // `emit_move_cursor` for GtkText (unlike `sourceview::View`, whose typed
        // emitter `wire_word_navigation` uses), so this signal is reachable only by
        // name — and by-name emission is checked at RUNTIME, not by the compiler. A
        // wrong signal name or argument list is a panic in the reader's hands, so it
        // is proved here rather than trusted.
        let entry = gtk::Entry::new();
        entry.set_text("alpha beta gamma");
        let text = key_target(&entry).expect("GtkEntry delegates to a GtkText");

        entry.set_position(-1);
        assert_eq!(entry.position(), 16, "set_position(-1) parks at the end");

        text.emit_by_name::<()>("move-cursor", &[&MovementStep::Words, &-1i32, &false]);
        assert_eq!(
            entry.position(),
            11,
            "Option+Left from the end must land on the start of `gamma`"
        );

        text.emit_by_name::<()>("move-cursor", &[&MovementStep::Words, &-1i32, &true]);
        assert_eq!(
            entry.selection_bounds(),
            Some((6, 11)),
            "Option+Shift+Left must EXTEND from where the caret was, not re-anchor"
        );
    }

    #[gtktest::test]
    fn the_controller_lands_on_the_delegate_in_capture_phase() {
        let entry = gtk::Entry::new();
        let text = key_target(&entry).expect("GtkEntry delegates to a GtkText");
        let wrapper_before = controllers(&entry).len();
        let text_before = controllers(&text);
        assert_eq!(
            capture_key_controllers(&entry),
            0,
            "GtkText's OWN key controllers must not be capture-phase, or the phase \
             below is not a signature for ours"
        );

        wire_field_word_navigation(&entry);

        assert_eq!(
            controllers(&entry).len(),
            wrapper_before,
            "nothing may be attached to the wrapper — the events do not land there"
        );
        let added: Vec<_> = controllers(&text)
            .into_iter()
            .filter(|c| !text_before.contains(c))
            .collect();
        assert_eq!(
            added.len(),
            1,
            "exactly one controller added, on the delegate"
        );
        assert!(added[0].is::<gtk::EventControllerKey>());
        assert_eq!(
            added[0].propagation_phase(),
            gtk::PropagationPhase::Capture,
            "capture, so the ordering against GtkText's own key controller is decided \
             by the phase rather than by add_controller list order"
        );
    }

    #[gtktest::test]
    fn the_annotation_comment_entry_is_wired_at_construction() {
        let entry = crate::widgets::comment_entry::CommentEntry::new("", |_| {}).entry;
        assert_eq!(
            capture_key_controllers(&entry),
            1,
            "CommentEntry::new must wire word navigation, like every other commit route \
             it owns — a surface that gets it from its CALLER is a surface the next \
             caller forgets"
        );
    }

    #[gtktest::test]
    fn the_find_and_replace_fields_are_wired_in_a_real_window() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.macwordnav.fields"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let window = crate::window::new_window(&app, "IT", "# Heading\n\nbody text", None);
        let chrome = crate::winstate::chrome(&window).expect("chrome registered");

        assert_eq!(
            capture_key_controllers(&chrome.find_entry),
            1,
            "the find field must carry the wiring"
        );

        // The replace entry is not held in `WindowChrome` (only its row is), so it is
        // reached the way the chrome holds it — the row's first GtkEntry child.
        let replace_entry =
            std::iter::successors(chrome.replace_row.first_child(), |w| w.next_sibling())
                .find_map(|w| w.downcast::<gtk::Entry>().ok())
                .expect("the replace row holds a GtkEntry");
        assert_eq!(
            capture_key_controllers(&replace_entry),
            1,
            "the replace field must carry the wiring"
        );
    }

    /// The first `GtkEntry` anywhere under `root`, depth-first.
    ///
    /// A recursive walk rather than a child scan because the prompt form nests its
    /// fields two levels down (form box → labelled row → entry), and the test has no
    /// handle on the dialog's interior — `input_form` returns nothing and stores
    /// nothing.
    fn first_entry(root: &gtk::Widget) -> Option<gtk::Entry> {
        if let Some(entry) = root.downcast_ref::<gtk::Entry>() {
            return Some(entry.clone());
        }
        std::iter::successors(root.first_child(), |w| w.next_sibling())
            .find_map(|c| first_entry(&c))
    }

    #[gtktest::test]
    fn the_shared_prompt_forms_field_is_wired_when_the_form_opens() {
        // The choke point that matters most: `input_form` builds the field for Go To
        // Line, Rename AND all three Insert dialogs, so this one call site is FIVE
        // commands. Rename is the one an enumeration keeps missing -- it lives in
        // `window::rename`, not under `editbar/`, so it is invisible to anyone who
        // enumerates this choke point's callers by reading the module beside it.
        // Driven through the real action rather than by calling the builder, because
        // the builder is private to `window` — and because what is being pinned is
        // that the *production* path reaches the wiring.
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.macwordnav.prompt"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let window = crate::window::new_window(&app, "IT", "# Heading\n\nbody text", None);

        // The action is gated on edit mode AND editor focus (`setup_editor_focus_gate`);
        // both are someone else's contract and neither is what this test is about, so
        // the gate is opened directly instead of being satisfied by driving focus.
        const GO_TO_LINE: &str = "go-to-line";
        crate::window::set_action_enabled(&window, GO_TO_LINE, true);
        gtk::gio::prelude::ActionGroupExt::activate_action(&window, GO_TO_LINE, None);

        let title = "Go To Line";
        let toplevels = gtk::Window::toplevels();
        let dialog = (0..toplevels.n_items())
            .filter_map(|i| toplevels.item(i))
            .filter_map(|o| o.downcast::<gtk::Window>().ok())
            .find(|w| {
                w.title().is_some_and(|t| t == title)
                    && w.transient_for().as_ref() == Some(window.upcast_ref::<gtk::Window>())
            })
            .expect("win.go-to-line presents its prompt form");

        let entry = first_entry(dialog.child().expect("the form has content").as_ref())
            .expect("the prompt form holds a GtkEntry");
        assert_eq!(
            capture_key_controllers(&entry),
            1,
            "the shared prompt field must carry the wiring — this one site is Go To \
             Line plus Insert Link/Image/Table"
        );

        // The suite shares one process across every body, and a live modal toplevel
        // is process-global state: left open it stays in `gtk::Window::toplevels()`
        // for every later body, and this test's own `find` would then be picking one
        // of several. Destroy, not close — `close()` is a request the window may
        // answer on a later main-loop turn, which is exactly the deferral this test
        // must not leave behind.
        dialog.destroy();
    }
}
