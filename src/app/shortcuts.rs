//! The keyboard-shortcuts help window (`GtkShortcutsWindow`), wired to each
//! window's `win.show-help-overlay` action by `window::new_window`'s
//! `set_help_overlay` call and opened with Ctrl+? (TDD 16.1).
//!
//! ## Why this is built from Builder XML, not programmatically
//!
//! The obvious approach — `ShortcutsWindow::new()` +
//! `section.add_group()` / `group.add_shortcut()` — **cannot be used on GTK 4.6**.
//! `gtk_shortcuts_window_add_section` / `gtk_shortcuts_section_add_group` /
//! `gtk_shortcuts_group_add_shortcut` were only added in **GTK 4.14**; they are
//! absent from the 4.6.9 runtime this app links against (`nm -D libgtk-4.so.1`
//! confirms), so calling the gtk4-rs `add_*` wrappers references an undefined
//! symbol. Before 4.14 the *only* way to construct a `GtkShortcutsWindow` is via
//! its `Buildable` XML — which has been stable since GTK 4.0. So we generate the
//! `<interface>` XML as a string and load it with `gtk::Builder::from_string`.
//! (See ANTI-PATTERNS.md — "GtkShortcutsWindow programmatic API is 4.14+".)
//!
//! ## Drift
//!
//! Every row for a Cmd-table command (File/Edit/Format/View) is generated *from*
//! `FILE_CMDS` / `EDIT_CMDS` / `FORMAT_CMDS` / `VIEW_CMDS`, so it cannot drift
//! from the actual accelerators `setup::register_accelerators` binds (both read
//! the same arrays). The handful of commands whose accelerators are registered
//! inline in `setup.rs` rather than via a Cmd table (tab/window/zoom navigation,
//! the outline toggle, Go To Line, the shortcuts-help overlay) come from the
//! shared [`crate::app::INLINE_ACCEL_CMDS`] table — the SAME table
//! `setup::register_accelerators` binds from and the menu hints / toolbar tooltips
//! read — so the display here can no longer drift from the real binding (QA M-4;
//! this replaced a hand-mirrored `EXTRA_ROWS` list that had already drifted —
//! zoom-in bound `<Primary>plus`+`<Primary>equal` but showed only `plus`). The
//! `gtk_integration_tests` below assert both parseability and that what
//! `register_accelerators` binds equals what this window displays.

use crate::app::{EDIT_CMDS, FILE_CMDS, FORMAT_CMDS, INLINE_ACCEL_CMDS, VIEW_CMDS};

/// Escape the five XML predefined entities so a label or accelerator (which
/// contains `<`/`>`, e.g. `<Primary>o`) is safe to embed in the interface XML.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// One `<GtkShortcutsShortcut>` child (accelerator kind, the default). Empty
/// accelerators are the caller's responsibility to skip.
fn shortcut_xml(title: &str, accel: &str) -> String {
    format!(
        "<child><object class=\"GtkShortcutsShortcut\">\
           <property name=\"title\">{}</property>\
           <property name=\"accelerator\">{}</property>\
         </object></child>",
        xml_escape(title),
        xml_escape(accel),
    )
}

/// One `<GtkShortcutsGroup>` with a title and its accumulated shortcut children.
fn group_xml(title: &str, rows: &str) -> String {
    format!(
        "<child><object class=\"GtkShortcutsGroup\">\
           <property name=\"title\">{}</property>{}\
         </object></child>",
        xml_escape(title),
        rows,
    )
}

/// Collect the `(title, accelerator)` rows for one group: every Cmd-table entry
/// with a non-empty accel, then any [`crate::app::INLINE_ACCEL_CMDS`] tagged with
/// this group. Inline commands show their CANONICAL (first) accelerator — the same
/// one the menu hint / tooltip show — while `register_accelerators` binds all of
/// its aliases, both from the one table (QA M-4).
fn rows_for(group: &str, cmd_rows: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (title, accel) in cmd_rows {
        if !accel.is_empty() {
            out += &shortcut_xml(title, accel);
        }
    }
    for cmd in INLINE_ACCEL_CMDS.iter().filter(|c| c.group == group) {
        out += &shortcut_xml(cmd.label, cmd.accels[0]);
    }
    out
}

/// Build the interface XML for the whole shortcuts window.
fn interface_xml() -> String {
    // File / Edit / View rows come straight off the Cmd tables (labels + accels).
    let file: Vec<(&str, &str)> = FILE_CMDS.iter().map(|c| (c.label, c.accel)).collect();
    let edit: Vec<(&str, &str)> = EDIT_CMDS.iter().map(|c| (c.label, c.accel)).collect();
    let view: Vec<(&str, &str)> = VIEW_CMDS.iter().map(|c| (c.label, c.accel)).collect();
    // Format: the inline FORMAT_CMDS plus the six heading accelerators, which
    // setup.rs registers in a `1..=6` loop rather than from a table.
    let mut format: Vec<(String, String)> = FORMAT_CMDS
        .iter()
        .filter(|c| !c.accel.is_empty())
        .map(|c| (c.label.to_string(), c.accel.to_string()))
        .collect();
    for n in 1..=6u8 {
        format.push((format!("Heading {n}"), format!("<Shift>F{n}")));
    }
    let format_rows: String = format
        .iter()
        .map(|(t, a)| shortcut_xml(t, a))
        .collect::<String>();

    let mut groups = String::new();
    groups += &group_xml("File", &rows_for("File", &file));
    groups += &group_xml("Edit", &rows_for("Edit", &edit));
    groups += &group_xml("Format", &format_rows);
    groups += &group_xml("View", &rows_for("View", &view));
    // The Windows & Tabs group has no Cmd table behind it — every row comes from
    // INLINE_ACCEL_CMDS (group "Windows & Tabs"), pulled in by rows_for's empty
    // cmd slice (its second loop). (Was a hand-mirrored EXTRA_ROWS list, retired
    // into INLINE_ACCEL_CMDS — QA M-4.)
    groups += &group_xml("Windows & Tabs", &rows_for("Windows & Tabs", &[]));

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <interface>\
           <object class=\"GtkShortcutsWindow\" id=\"shortcuts\">\
             <property name=\"modal\">1</property>\
             <child>\
               <object class=\"GtkShortcutsSection\">\
                 <property name=\"section-name\">shortcuts</property>\
                 <property name=\"max-height\">10</property>\
                 {groups}\
               </object>\
             </child>\
           </object>\
         </interface>"
    )
}

/// Build a fresh `GtkShortcutsWindow` for one application window. Called once per
/// window; `window::new_window` passes it to `set_help_overlay`, which owns it,
/// creates the per-window `win.show-help-overlay` action, and hides (not
/// destroys) it on close so it can reopen.
pub(crate) fn make_shortcuts_window() -> gtk::ShortcutsWindow {
    let builder = gtk::Builder::from_string(&interface_xml());
    builder
        .object::<gtk::ShortcutsWindow>("shortcuts")
        .expect("shortcuts window is defined in the interface XML")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generated XML must be well-formed enough to contain every group and to
    /// escape the accelerator angle brackets (a raw `<Primary>` would break the
    /// XML). Checked without a display by inspecting the string.
    #[test]
    fn interface_xml_has_groups_and_escapes_accels() {
        let xml = interface_xml();
        for group in ["File", "Edit", "Format", "View", "Windows &amp; Tabs"] {
            assert!(xml.contains(group), "interface XML missing group {group:?}");
        }
        // Accelerators must be entity-escaped, never raw.
        assert!(xml.contains("&lt;Primary&gt;o"), "Open accel not escaped");
        assert!(
            !xml.contains("<property name=\"accelerator\"><Primary>"),
            "an accelerator was emitted with raw angle brackets"
        );
    }

    /// Every inline-accelerator command must actually be *displayed* in the
    /// window, not merely bound. `interface_xml` renders a FIXED set of group
    /// headings (File / Edit / Format / View / Windows & Tabs), so an
    /// `INLINE_ACCEL_CMDS` row tagged with a group outside that set — or any new
    /// off-table accelerator path — would be bound by `register_accelerators`
    /// yet silently never listed here: the accel works, the help window just
    /// never advertises it (the latent CAM-gap this guards). Asserting each row's
    /// label AND canonical accelerator reach the generated XML closes that hole
    /// for any future row regardless of its group, and needs no display (pure
    /// string check), unlike the `#[gtktest::test]` bind/display equality guard.
    /// (CAM Action matrix — "advertised in the Keyboard Shortcuts help window".)
    #[test]
    fn every_inline_accel_cmd_is_displayed() {
        let xml = interface_xml();
        for cmd in INLINE_ACCEL_CMDS {
            assert!(
                xml.contains(&xml_escape(cmd.label)),
                "INLINE_ACCEL_CMDS {:?} (group {:?}) is bound but never displayed \
                 in the shortcuts window — is its group a rendered heading?",
                cmd.action,
                cmd.group,
            );
            assert!(
                xml.contains(&xml_escape(cmd.accels[0])),
                "INLINE_ACCEL_CMDS {:?} canonical accelerator {:?} is not shown \
                 in the shortcuts window",
                cmd.action,
                cmd.accels[0],
            );
        }
    }
}

/// GTK-object tests (need `gtk::init`, hence the `gtk-integration-tests` feature
/// and `#[gtktest::test]`, same as `window/reload.rs` — see its note). `accelerator_parse`
/// and building the window both require an initialized GTK.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use gtk::prelude::*;

    /// Every accelerator we render must be parseable by GTK, or the
    /// `GtkShortcutsShortcut` shows nothing / warns. Guards EVERY accelerator in
    /// the shared `INLINE_ACCEL_CMDS` table (the Cmd tables are guarded elsewhere)
    /// against a typo'd accel string.
    #[gtktest::test]
    fn inline_accel_cmds_are_parseable() {
        for cmd in INLINE_ACCEL_CMDS {
            assert!(
                !cmd.accels.is_empty(),
                "INLINE_ACCEL_CMDS {:?} has no accelerator",
                cmd.action
            );
            for accel in cmd.accels {
                assert!(
                    gtk::accelerator_parse(*accel).is_some(),
                    "INLINE_ACCEL_CMDS {:?} has an unparseable accelerator {accel:?}",
                    cmd.action
                );
            }
        }
    }

    /// The M-4 drift guard proper: what `register_accelerators` actually BINDS for
    /// each inline command must equal the `accels` this window displays from — so
    /// the shortcuts window can never advertise a key the app doesn't bind (the
    /// zoom-in `plus`/`equal` drift the reviewer caught). Registering onto a real
    /// `GtkApplication` and reading `accels_for_action` back is the only faithful
    /// check, hence a `gtk-integration-tests` `#[gtktest::test]`.
    #[gtktest::test]
    fn registered_accels_match_the_inline_table() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.acceltest"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        crate::app::register_accelerators(&app);
        for cmd in INLINE_ACCEL_CMDS {
            // Compare PARSED (key, modifier) pairs, not raw strings: GTK
            // canonicalises `<Primary>` to `<Control>` when it stores an accel, so
            // `accels_for_action` reads back the normalized form. `accelerator_parse`
            // maps both spellings to the same pair.
            let bound: Vec<_> = app
                .accels_for_action(cmd.action)
                .iter()
                .map(|s| gtk::accelerator_parse(s.as_str()))
                .collect();
            let expected: Vec<_> = cmd
                .accels
                .iter()
                .map(|a| gtk::accelerator_parse(*a))
                .collect();
            assert_eq!(
                bound, expected,
                "binding for {:?} drifted from its INLINE_ACCEL_CMDS row",
                cmd.action
            );
        }
    }

    /// The interface XML must actually parse into a `GtkShortcutsWindow` (the 4.6
    /// Builder-XML construction path — a malformed group/section would make
    /// `builder.object` return `None`, so `make_shortcuts_window`'s `.expect`
    /// would panic here). Constructing it without a panic is the assertion.
    #[gtktest::test]
    fn xml_builds_a_shortcuts_window() {
        let w = make_shortcuts_window();
        // The window carries at least one child (the section) — a non-empty tree
        // confirms the Buildable XML populated, not just an empty shell.
        assert!(
            w.first_child().is_some(),
            "shortcuts window has no children"
        );
    }
}
