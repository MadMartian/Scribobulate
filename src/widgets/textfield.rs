//! The application's text fields, built in one place.
//!
//! A `gtk::Entry` is not a text field this application can ship. It owes two follow-up
//! calls before it is one, and **both are silent when forgotten**:
//!
//! - [`crate::a11y::name_field`] — a field with no accessible name announces as nothing
//!   to a screen reader. Invisible to everyone not using one.
//! - [`crate::macwordnav::wire_field_word_navigation`] — Option+Left/Right word
//!   navigation, which every native macOS text field has. macOS-only, so its absence
//!   cannot be seen at all from the seat that runs the guards.
//!
//! Four surfaces each hand-repeated that pairing, and one had already drifted: the
//! prompt-dialog field wired word navigation and never named itself. `CommentEntry`'s
//! own doc comment stated the hazard in the same breath as committing it —
//!
//! > Wired here for exactly the reason this type exists at all: a surface that had to
//! > remember it would be the fourth surface that forgot.
//!
//! — which solved it for that one surface and left the other three remembering. This
//! module is that sentence applied to all of them: the constructors below are the only
//! way a text field is built, so "forgot the follow-up" stops being a spelling the
//! codebase admits rather than a thing four call sites each get right.
//!
//! The a11y name is a **required argument**, not an option, because that is what makes
//! omission non-compiling rather than invisible. A field that genuinely takes its
//! identity from an adjacent `GtkLabel` still passes the label's text — naming it twice
//! is harmless, and it keeps "this field is named by its neighbour" a thing the code
//! says rather than a thing it fails to say.
//!
//! **Enforcement: convention, deliberately — no `clippy.toml` ban on `Entry::new`.**
//! POLICY § Typed GTK seams requires this to be decided rather than left open, and the
//! true-positive test is what decides it: the raw constructor's remaining callers are
//! all tests whose SUBJECT is the bare widget (`macwordnav`'s delegate-resolution cases,
//! `codeview::navkeys`' key-routing case), and they outnumber the production sites this
//! module now owns. A ban would fire mostly on legitimate calls, which is the shape
//! POLICY says trains everyone to reach for `#[allow]` and costs more than it saves.
//! What backs the convention instead is `a11y`'s tree-walk guard, which fails on any
//! unnamed interactive control in a window.
//!
//! That guard has a known blind spot worth stating rather than discovering: it walks
//! WINDOWS, so a field built in a transient dialog — `editbar::dialog`'s prompt form —
//! is outside it. That is exactly where the drift was found, and it is why this module
//! is a constructor taking a required name rather than a lint.

use gtk::prelude::*;

/// Wire the follow-ups every text field owes, whatever its concrete type.
///
/// Takes the widget the *user types into*. For a `GtkEntry`/`GtkSearchEntry` that is the
/// wrapper — both delegate to an internal `GtkText`, and both
/// [`crate::a11y::name_field`] and [`crate::macwordnav::wire_field_word_navigation`]
/// already resolve the delegate themselves (GTK4Rs/AP-301), so the wrapper is the right
/// thing to hand them.
fn wire_field(field: &impl IsA<gtk::Widget>, accessible_name: &str) {
    crate::a11y::name_field(field, accessible_name);
    #[cfg(target_os = "macos")]
    crate::macwordnav::wire_field_word_navigation(field);
    // Referenced on every platform so the parameter is never "unused" off macOS.
    let _ = field;
}

/// The application's standard single-line text field: hexpanding, accessibly named, and
/// word-navigable on macOS.
///
/// `accessible_name` is what a screen reader announces the field as — the field's
/// *identity* ("Replace with", "Comment"), not a hint about its contents. A placeholder
/// is not a substitute: GTK publishes that as `Placeholder`, which AT reads as a hint
/// about expected content and which disappears the moment the user types.
pub(crate) fn named_entry(accessible_name: &str, initial: &str) -> gtk::Entry {
    let entry = gtk::Entry::new();
    entry.set_text(initial);
    entry.set_hexpand(true);
    wire_field(&entry, accessible_name);
    entry
}

/// [`named_entry`] for the find bar's `GtkSearchEntry`.
///
/// A separate constructor rather than a generic one because the two types share no
/// constructible trait, and because a search entry has no initial text worth passing —
/// it starts empty by definition.
pub(crate) fn named_search_entry(accessible_name: &str) -> gtk::SearchEntry {
    let entry = gtk::SearchEntry::new();
    entry.set_hexpand(true);
    wire_field(&entry, accessible_name);
    entry
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;

    /// Both constructors name the field they build.
    ///
    /// The half of the pairing that IS observable on Linux. The macOS half
    /// (`wire_field_word_navigation`) has no read-back at this project's GTK floor and is
    /// covered by `macwordnav`'s own tests plus the mac seat's manual pass — which is
    /// precisely why it must not be left to four call sites to remember.
    #[gtktest::test]
    fn a_constructed_field_carries_its_accessible_name() {
        let entry = named_entry("Replace with", "seed");
        assert!(
            crate::a11y::has_name(&entry),
            "named_entry produced a field with no accessible name"
        );
        assert_eq!(entry.text(), "seed", "initial text was not applied");
        assert!(entry.hexpands(), "a text field must fill its row");

        let search = named_search_entry("Find");
        assert!(
            crate::a11y::has_name(&search),
            "named_search_entry produced a field with no accessible name"
        );
        assert!(search.hexpands(), "a text field must fill its row");
    }
}
