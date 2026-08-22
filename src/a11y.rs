//! Accessible naming for interactive controls — the one place a control's
//! **name** and its **tooltip** are set, together.
//!
//! A tooltip is not an accessible name. The overlap is a coincidence of both being
//! short strings: a tooltip is pointer-only, transient, and never reaches a screen
//! reader, while an accessible name is what assistive technology *announces* and the
//! only thing an icon-only button has to identify it. GTK does not derive one from the
//! other — `gtk_widget_set_tooltip_text` writes no accessible property — so a control
//! that carries only a tooltip is, to AT, unnamed. That was the state of this
//! application's entire chrome: 34 tooltips, one accessibility call in the tree.
//!
//! The failure mode is silent in both directions. Omitting the name breaks nothing
//! visible, compiles cleanly, and passes every functional test — the button works, it
//! just has nothing to announce. And the omission re-appears with every control added
//! later, because nothing about `set_tooltip_text` suggests a second call is owed.
//!
//! So this is a choke point, not a convention (ScrAP-219's enforcement ladder — a helper
//! put where it is easier to use than to avoid, backed by the rung above it):
//! **`WidgetExt::set_tooltip_text` is banned in `clippy.toml`**, and every control is
//! named through one of the four entry points below, each of which sets both halves in
//! a single call. The ban is what makes the pairing unforgettable; the helpers are what
//! make it a one-liner. The ban alone is NOT sufficient — it matches a path, so the
//! builder spelling (`MenuButton::builder().tooltip_text(…)`) slips through it; the
//! tree-walk guard at the foot of this file is the other half of one mechanism, and is
//! what actually caught three unnamed controls (ScrAP-230).
//!
//! Picking an entry point:
//!
//! | The tooltip is… | Use |
//! |---|---|
//! | exactly the control's name | [`name`] |
//! | the name plus its shortcut, from the SSOT command tables | [`name_with_accel`] / [`name_from_action`] |
//! | the name plus a hint this module should not re-compose | [`name_with_tooltip`] |
//! | *not* a name — an explanation, a state, a file path | [`describe`] |
//!
//! The distinction in the last row matters: putting an explanation where the name goes
//! makes AT announce a sentence in place of the control's identity. A description is a
//! separate accessible property precisely so both can be true at once.

use gtk::accessible::Property;
use gtk::prelude::*;

/// Name a control whose tooltip *is* its name — no shortcut, no extra hint.
///
/// The accessible name and the tooltip are the same string by construction, which is
/// the point: they cannot drift.
pub(crate) fn name(control: &impl IsA<gtk::Widget>, label: &str) {
    let control = control.as_ref();
    #[allow(clippy::disallowed_methods)] // this module IS the sanctioned route
    control.set_tooltip_text(Some(label));
    control.update_property(&[Property::Label(label)]);
}

/// Name a control and give it a shortcut, from raw pieces: the tooltip becomes
/// `"Label (Hint)"` via the shared [`crate::app::tooltip_with_accel`] derivation, the
/// accessible name stays the **bare label**, and the shortcut is published separately as
/// `KeyShortcuts`.
///
/// Splitting them this way is the whole reason this takes the pieces rather than a
/// composed string. A screen reader announcing "Zoom In (Ctrl+plus)" as a control's
/// *name* is reading punctuation aloud; AT wants the name and the shortcut as separate
/// facts, and the pointer wants them as one line. `accel` is GTK accelerator syntax
/// (`""` = none), the same value the menu and the shortcuts window bind.
pub(crate) fn name_with_accel(control: &impl IsA<gtk::Widget>, label: &str, accel: &str) {
    let control = control.as_ref();
    #[allow(clippy::disallowed_methods)] // this module IS the sanctioned route
    control.set_tooltip_text(Some(&crate::app::tooltip_with_accel(label, accel)));
    match crate::app::accel_hint(accel) {
        Some(hint) => control.update_property(&[
            Property::Label(label),
            Property::KeyShortcuts(hint.as_str()),
        ]),
        None => control.update_property(&[Property::Label(label)]),
    }
}

/// Name a control from the single-source-of-truth inline-accel table, by action name.
///
/// Panics when the action is absent from `crate::app::INLINE_ACCEL_CMDS` — a
/// compile-time-constant wiring error, caught at startup, rather than a control that
/// silently ends up both tooltip-less and unnamed.
pub(crate) fn name_from_action(control: &impl IsA<gtk::Widget>, action: &str) {
    let cmd = crate::app::inline_cmd(action)
        .unwrap_or_else(|| panic!("a11y: {action} missing from INLINE_ACCEL_CMDS"));
    name_with_accel(control, cmd.label, cmd.accels[0]);
}

/// Name a control whose tooltip carries a hint this module should not re-derive — a
/// shortcut that lives in a key controller rather than in the command tables, for
/// instance. The tooltip is used verbatim; the accessible name is the bare `label`.
pub(crate) fn name_with_tooltip(control: &impl IsA<gtk::Widget>, label: &str, tooltip: &str) {
    let control = control.as_ref();
    #[allow(clippy::disallowed_methods)] // this module IS the sanctioned route
    control.set_tooltip_text(Some(tooltip));
    control.update_property(&[Property::Label(label)]);
}

/// Name a **text field** — an entry that carries a placeholder but no visible label.
///
/// Deliberately sets no tooltip: a hover tip over a text field is not this application's
/// idiom, and the field already shows its placeholder. This is the one entry point that
/// names without touching the tooltip, because a placeholder is the *third* string that
/// looks like a name and is not one — GTK publishes it as `Placeholder`, which AT reads
/// as a hint about the expected content rather than as the field's identity, and which
/// disappears the moment the user types.
pub(crate) fn name_field(field: &impl IsA<gtk::Widget>, label: &str) {
    field.as_ref().update_property(&[Property::Label(label)]);
}

/// Attach an explanatory **description** — a tooltip that is not a name: why a control
/// is unavailable, a tab's full path, an image's alt text.
///
/// `None` clears both. Deliberately does not touch the name: a control keeps whatever
/// identity it already has, and gains (or loses) the explanation on top of it.
pub(crate) fn describe(control: &impl IsA<gtk::Widget>, description: Option<&str>) {
    let control = control.as_ref();
    #[allow(clippy::disallowed_methods)] // this module IS the sanctioned route
    control.set_tooltip_text(description);
    control.update_property(&[Property::Description(description.unwrap_or(""))]);
}

/// Whether `control` carries an accessible **name**.
///
/// GTK 4.6 exposes no getter for an accessible property (`gtk_accessible_get_at_context`
/// is 4.10), so the only read-back at this project's floor is the testing entry point
/// `gtk_test_accessible_has_property` — present in 4.6.9 (`nm -D`-confirmed, the GTK4Rs/AP-114
/// discipline), non-variadic, and asking exactly the question the regression guard needs:
/// *was a name set at all*, which is the omission this module exists to prevent.
// Gated to the same cfg as its only caller, `gtk_integration_tests` below, not the
// broader `cfg(test)`: a bare `cargo test` does not compile that module and reported
// this as dead.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn has_name(control: &impl IsA<gtk::Widget>) -> bool {
    use gtk::glib::translate::{IntoGlib, ToGlibPtr};
    let accessible = control.as_ref().clone().upcast::<gtk::Accessible>();
    unsafe {
        gtk::ffi::gtk_test_accessible_has_property(
            accessible.to_glib_none().0,
            gtk::AccessibleProperty::Label.into_glib(),
        ) != 0
    }
}

/// The naming rule, enforced over a whole real window rather than per call site.
///
/// A per-widget test would only ever cover the widgets someone remembered to write a
/// test for — which is the same weakness as the convention this module replaced. Walking
/// the live tree makes the guard's coverage grow with the application: a control added
/// next year is in scope the day it is added, without anyone extending this file.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use gtk::prelude::*;

    /// Every descendant of `root`, depth-first — but **not descending into** a
    /// `GtkMenuButton`, `GtkEntry` or `GtkSearchEntry`, whose children are GTK's own
    /// internal delegates (a `GtkMenuButton` wraps a private `GtkToggleButton`; an entry
    /// wraps a private `GtkText`). Those internals are GTK's to name, and the public
    /// widget is the one this application is responsible for.
    fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
        let mut out = Vec::new();
        let mut child = root.first_child();
        while let Some(w) = child {
            out.push(w.clone());
            let opaque =
                w.is::<gtk::MenuButton>() || w.is::<gtk::Entry>() || w.is::<gtk::SearchEntry>();
            if !opaque {
                out.extend(descendants(&w));
            }
            child = w.next_sibling();
        }
        out
    }

    /// Whether this application owes `w` an explicit accessible name.
    ///
    /// SEMANTIC, not a type list. This used to be a downcast cascade over `GtkButton`,
    /// `GtkMenuButton`, `GtkEntry` and `GtkSearchEntry`, under a doc claim that the guard's
    /// coverage "grows with the application". It did not: a control of any other type — a
    /// `GtkToggleButton`, a `GtkSpinButton`, a subclassed widget, anything added next year —
    /// answered `None` on the first two arms and `false` on the last, so it was silently out
    /// of scope. The cascade decided coverage by naming types, which is exactly the
    /// remembered-to-write-it-down weakness the module doc says this replaces.
    ///
    /// The question GTK can answer directly is: **can a user land on this thing, and if they
    /// do, is there anything to announce?** A widget owes a name iff it is focusable — the
    /// operative definition of an interactive control — and neither carries an explicit
    /// accessible label nor derives one from visible text of its own.
    ///
    /// The third clause is what keeps a labelled button out of scope without naming
    /// `GtkButton`: GTK builds a real `GtkLabel` child for it and derives the accessible name
    /// from that, so "has a visible text descendant" is the general form of the old
    /// `b.label()` special case, and it covers every widget that gets its name the same way.
    fn owes_a_name(w: &gtk::Widget) -> bool {
        // SCOPE, not compliance. This answers "does this control need a name", and the
        // walk then asserts separately that each one HAS a name. Folding `has_name` in
        // here — which a first attempt did — collapses the candidate set to exactly the
        // failing widgets, so the guard passes vacuously the moment the application is
        // correct, and its own vacuity check is what catches that.
        if !is_interactive_role(w.accessible_role()) {
            return false;
        }
        !has_visible_text(w)
    }

    /// Roles a screen-reader user can act on, and which therefore have to announce as
    /// something.
    ///
    /// NOT `is_focusable()`, which was the first attempt and was wrong for this application
    /// specifically: its toolbar controls are deliberately not focusable — a toolbar button
    /// that takes focus on click steals it from the editor — yet they are exactly the
    /// controls this guard exists for. MEASURED: the focusable form found 7 nameable
    /// controls in a real window and tripped the walk's own vacuity check, which is the
    /// check earning its keep. A role is what the accessibility tree actually publishes, it
    /// is assigned to subclasses and custom widgets alike, and it does not enumerate types.
    fn is_interactive_role(role: gtk::AccessibleRole) -> bool {
        matches!(
            role,
            // No `ToggleButton` arm: that variant is above this project's 4.6 floor
            // (GTK4Rs/AP-114 — it compiles against a newer gtk4 and is simply absent here),
            // and at 4.6 a GtkToggleButton publishes `Button` anyway.
            gtk::AccessibleRole::Button
                | gtk::AccessibleRole::MenuItem
                | gtk::AccessibleRole::MenuItemCheckbox
                | gtk::AccessibleRole::MenuItemRadio
                | gtk::AccessibleRole::Checkbox
                | gtk::AccessibleRole::Radio
                | gtk::AccessibleRole::ComboBox
                | gtk::AccessibleRole::TextBox
                | gtk::AccessibleRole::SearchBox
                | gtk::AccessibleRole::SpinButton
                | gtk::AccessibleRole::Slider
                | gtk::AccessibleRole::Switch
                | gtk::AccessibleRole::Tab
        )
    }

    /// Does `w` show text of its own that GTK can derive an accessible name from?
    ///
    /// Depth-limited on purpose. A container full of labels is not "named by" them, and an
    /// unbounded walk would excuse any control that happens to contain text somewhere
    /// beneath it. GTK's own derivation is from the widget's immediate label child, which is
    /// what a `GtkButton`/`GtkToggleButton` builds for its `label` property.
    fn has_visible_text(w: &gtk::Widget) -> bool {
        fn text_in(w: &gtk::Widget, depth: usize) -> bool {
            if let Some(l) = w.downcast_ref::<gtk::Label>() {
                if !l.text().is_empty() {
                    return true;
                }
            }
            if depth == 0 {
                return false;
            }
            let mut child = w.first_child();
            while let Some(c) = child {
                if text_in(&c, depth - 1) {
                    return true;
                }
                child = c.next_sibling();
            }
            false
        }
        text_in(w, 2)
    }

    /// **Every icon-only control and label-less text field in a window carries an
    /// accessible name.**
    ///
    /// This is the whole of Tier 1: the application had 34 tooltips and one accessibility
    /// call, so a screen reader had nothing to announce for any toolbar, find-bar,
    /// sidebar or tab-strip control. The failure is invisible — the buttons work, they
    /// are simply anonymous — which is exactly why it needs a structural guard rather
    /// than review.
    ///
    /// The guard is deliberately "has a name", not "has *this* name": at GTK 4.6 the only
    /// read-back is `gtk_test_accessible_has_property` (see [`super::has_name`]), and
    /// wording is a product decision that would make this test a change-detector. What
    /// must never regress is the *omission*.
    ///
    /// Mutation check: dropping the `update_property` call from any one of the `a11y`
    /// entry points fails this with the offending widgets named in the message.
    #[gtktest::test]
    fn every_icon_only_control_in_a_window_has_an_accessible_name() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.a11ynames"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building any window");
        let window = crate::window::new_window(&app, "IT", "# Doc\n\nBody.\n", None);

        // Reveal the surfaces whose controls are built but hidden, so the walk reaches
        // the find bar's buttons and both sidebars' headers rather than silently passing
        // over an empty tree (ScrAP-209: a guard whose setup leaves nothing to inspect passes with the fix deleted).
        let chrome = crate::winstate::chrome(&window).expect("window chrome");
        chrome.find_bar_revealer.set_reveal_child(true);
        // ACTIVATE, not change_action_state: `find-replace` is a stateless
        // `SimpleAction` (window/findbar.rs), so changing its state is a silent no-op
        // that only emits `g_action_change_state: assertion 'state_type != NULL'`. That
        // is this comment's own ScrAP-209 in miniature — the setup step meant to reveal
        // the replace row did nothing, so the walk never reached those controls and the
        // guard passed on a smaller tree than it claimed to inspect.
        gtk::prelude::ActionGroupExt::activate_action(&window, "find-replace", None);
        window.change_action_state("outline", &true.to_variant());
        window.change_action_state("annotations", &true.to_variant());

        let all = descendants(window.upcast_ref::<gtk::Widget>());
        let candidates: Vec<gtk::Widget> = all.into_iter().filter(owes_a_name).collect();
        assert!(
            candidates.len() >= 20,
            "sanity: the walk reached the chrome — only {} nameable controls found, so \
             this guard would pass vacuously",
            candidates.len()
        );

        let unnamed: Vec<String> = candidates
            .iter()
            .filter(|w| !super::has_name(*w))
            .map(|w| {
                format!(
                    "{} (icon: {:?}, placeholder: {:?})",
                    w.type_(),
                    w.downcast_ref::<gtk::Button>().and_then(|b| b.icon_name()),
                    w.downcast_ref::<gtk::Entry>()
                        .and_then(|e| e.placeholder_text())
                )
            })
            .collect();
        assert!(
            unnamed.is_empty(),
            "{} of {} controls have no accessible name — a tooltip is not a name; route \
             them through crate::a11y:\n  {}",
            unnamed.len(),
            candidates.len(),
            unnamed.join("\n  ")
        );

        window.destroy();
    }
}
