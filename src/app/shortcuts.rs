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

use crate::app::commands::FormatGroup;
use crate::app::{EDITOR_KEYS, EDIT_CMDS, FILE_CMDS, FORMAT_CMDS, INLINE_ACCEL_CMDS, VIEW_CMDS};

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
///
/// `accel` arrives as the **declared** string and is re-spelled for the host by
/// [`crate::accel::for_host`] — the same transform `register_accelerators` and the
/// menu hints apply, so this window advertises the key the platform actually
/// binds. `GtkShortcutsShortcut` renders the modifier glyphs itself from the
/// parsed accelerator, so on macOS `<Meta>` draws as ⌘ with no further help.
fn shortcut_xml(title: &str, accel: &str) -> String {
    format!(
        "<child><object class=\"GtkShortcutsShortcut\">\
           <property name=\"title\">{}</property>\
           <property name=\"accelerator\">{}</property>\
         </object></child>",
        xml_escape(title),
        xml_escape(&crate::accel::for_host(accel)),
    )
}

/// A shortcut that is a **pointer gesture held under a key** — the modifier is a
/// real keystroke this window is the right place to advertise, the motion is not.
///
/// It carries no `action`, and that is the point of it being a separate table:
/// nothing here is bound by `register_accelerators`, because a wheel is not
/// expressible as an accelerator string. A row states the modifier as a **declared**
/// accelerator so it is re-spelled for the host like every other row (⌘ on macOS),
/// and names the motion in a subtitle.
struct PointerCmd {
    /// The shortcuts-window group heading this row appears under.
    group: &'static str,
    /// Human label — what the gesture does.
    label: &'static str,
    /// The modifier held, as a **declared, modifier-only** accelerator. GTK parses
    /// one of these to `keyval 0` with the modifier mask set, which
    /// `GtkShortcutLabel` renders as the bare modifier keycap and nothing else.
    accel: &'static str,
    /// The motion, in words, since GTK's accelerator vocabulary has none for a
    /// wheel: `gtkshortcutlabel.c` understands only `...`, `+` and `&`, and hands
    /// everything else to `gtk_accelerator_parse`.
    subtitle: &'static str,
}

/// The pointer gestures worth advertising here. **A gesture with no key in it is
/// deliberately absent** — the Back/Forward thumb buttons are pointer buttons 8
/// and 9 with no keystroke at all, and this window describes keys (TDD 23.6).
const POINTER_CMDS: &[PointerCmd] = &[PointerCmd {
    group: "Zoom",
    label: "Zoom Preview In / Out",
    // The same `<Primary>` the zoom accelerators declare, so the keycap drawn here
    // and the modifier `window::zoomwheel` tests for are one platform decision, not
    // two. `pointer_row_modifier_matches_the_gesture_mask` below is what holds them
    // together.
    accel: "<Primary>",
    subtitle: "With the mouse wheel",
}];

/// One `<GtkShortcutsShortcut>` for a [`PointerCmd`].
///
/// An **accelerator** row, not `shortcut-type="gesture"`: that type draws a
/// `GtkImage` from an icon GTK derives from the type, and derives none for a plain
/// gesture (`gtkshortcutsshortcut.c:206-260`), so the row would render a visible
/// empty image at the default 64px and no words at all. `subtitle` displays on an
/// accelerator row — the explicit setter controls its visibility on its own and is
/// conditioned on nothing (`:132-133`, `:201`).
fn pointer_shortcut_xml(cmd: &PointerCmd) -> String {
    subtitled_shortcut_xml(cmd.label, cmd.accel, cmd.subtitle)
}

/// An accelerator row with a subtitle: a [`PointerCmd`], or an
/// [`crate::app::EDITOR_KEYS`] key whose subtitle says where it works.
fn subtitled_shortcut_xml(label: &str, accel: &str, subtitle: &str) -> String {
    format!(
        "<child><object class=\"GtkShortcutsShortcut\">\
           <property name=\"title\">{}</property>\
           <property name=\"accelerator\">{}</property>\
           <property name=\"subtitle\">{}</property>\
         </object></child>",
        xml_escape(label),
        xml_escape(&crate::accel::for_host(accel)),
        xml_escape(subtitle),
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
/// this group, then any [`POINTER_CMDS`] tagged with it. Inline commands show their
/// CANONICAL (first) accelerator — the same one the menu hint / tooltip show —
/// while `register_accelerators` binds all of its aliases, both from the one table
/// (QA M-4).
fn rows_for(group: &str, cmd_rows: &[(&str, &str)]) -> Vec<String> {
    let mut out = Vec::new();
    for (title, accel) in cmd_rows {
        if !accel.is_empty() {
            out.push(shortcut_xml(title, accel));
        }
    }
    for cmd in INLINE_ACCEL_CMDS.iter().filter(|c| c.group == group) {
        out.push(shortcut_xml(cmd.label, cmd.accels[0]));
    }
    // Last in the group: a gesture is a footnote to the keys above it, not a peer.
    for cmd in POINTER_CMDS.iter().filter(|c| c.group == group) {
        out.push(pointer_shortcut_xml(cmd));
    }
    out
}

/// The most shortcuts one group may hold.
///
/// GtkShortcutsSection lays out whole groups, never splitting one, and every page of
/// the section is as tall as its tallest group (both the window's and the section's
/// internal stacks are `vhomogeneous`). The window is not resizable and cannot be
/// shorter than that, so one long group pushes the page switcher off a short screen:
/// a 22-row Format group made the window 1095 px tall. `max-height` cannot help,
/// because it only decides which whole groups share a column.
#[cfg(test)]
const MAX_GROUP_ROWS: usize = 12;

/// Lines per column in the finished window.
const MAX_HEIGHT: u32 = 10;

/// The section's `max-height` while Builder adds the groups: tall enough that every
/// group fits one page.
///
/// GtkShortcutsSection re-lays out its groups each time one is added, and each re-layout
/// collects them page by page but PREPENDS every page's groups (gtkshortcutssection.c
/// `reflow_groups`, 4.6.9 lines 544-565), so every re-layout of a multi-page section
/// reverses its page order. Adding ten groups at the real height scrambled them (File and
/// Edit landed on page 4 of 5). A single page has no order to reverse, so the groups are
/// added at this height and the real [`MAX_HEIGHT`] is applied once, afterwards, in
/// [`make_shortcuts_window`].
const BUILD_MAX_HEIGHT: u32 = 1000;

/// Every group's title and its shortcut rows, in display order.
fn group_rows(
    file: &[(&str, &str)],
    edit: &[(&str, &str)],
    view: &[(&str, &str)],
) -> Vec<(&'static str, Vec<String>)> {
    // The Format menu is split across four groups to respect MAX_GROUP_ROWS: text
    // styles, blocks, headings, insertions — by each command's own `group`, so a new
    // FORMAT_CMDS entry lands in exactly one.
    let format_group = |group: FormatGroup| -> Vec<String> {
        let editor_keys = EDITOR_KEYS.iter().filter(|k| {
            FORMAT_CMDS
                .iter()
                .any(|c| c.target == k.target && c.group == group)
        });
        FORMAT_CMDS
            .iter()
            .filter(|c| !c.accel.is_empty() && c.group == group)
            .map(|c| shortcut_xml(c.label, c.accel))
            // After the group's accelerators: a key that works only in one place
            // is a footnote to them, like a pointer gesture.
            .chain(editor_keys.map(|k| subtitled_shortcut_xml(k.label, k.accel, k.context)))
            .collect()
    };
    // The six heading accelerators are registered by setup.rs in a `1..=6` loop
    // rather than from a table.
    let headings: Vec<String> = (1..=6u8)
        .map(|n| shortcut_xml(&format!("Heading {n}"), &format!("<Shift>F{n}")))
        .collect();
    vec![
        ("File", rows_for("File", file)),
        ("Edit", rows_for("Edit", edit)),
        ("Format", format_group(FormatGroup::Text)),
        ("Blocks", format_group(FormatGroup::Block)),
        ("Headings", headings),
        ("Insert", format_group(FormatGroup::Insert)),
        ("View", rows_for("View", view)),
        ("Navigate", rows_for("Navigate", &[])),
        ("Zoom", rows_for("Zoom", &[])),
        // The Windows & Tabs group has no Cmd table behind it — every row comes from
        // INLINE_ACCEL_CMDS (group "Windows & Tabs"), pulled in by rows_for's empty
        // cmd slice.
        ("Windows & Tabs", rows_for("Windows & Tabs", &[])),
    ]
}

/// Build the interface XML for the whole shortcuts window.
fn interface_xml() -> String {
    // File / Edit / View rows come straight off the Cmd tables (labels + accels).
    let file: Vec<(&str, &str)> = FILE_CMDS.iter().map(|c| (c.label, c.accel)).collect();
    let edit: Vec<(&str, &str)> = EDIT_CMDS.iter().map(|c| (c.label, c.accel)).collect();
    let view: Vec<(&str, &str)> = VIEW_CMDS.iter().map(|c| (c.label, c.accel)).collect();
    let mut groups = String::new();
    for (title, rows) in group_rows(&file, &edit, &view) {
        groups += &group_xml(title, &rows.concat());
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <interface>\
           <object class=\"GtkShortcutsWindow\" id=\"shortcuts\">\
             <property name=\"modal\">1</property>\
             <child>\
               <object class=\"GtkShortcutsSection\" id=\"section\">\
                 <property name=\"section-name\">shortcuts</property>\
                 <property name=\"max-height\">{BUILD_MAX_HEIGHT}</property>\
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
    use gtk::prelude::ObjectExt;
    let builder = gtk::Builder::from_string(&interface_xml());
    // One re-layout, from a single page, in declaration order (see BUILD_MAX_HEIGHT).
    builder
        .object::<gtk::ShortcutsSection>("section")
        .expect("shortcuts section is defined in the interface XML")
        .set_property("max-height", MAX_HEIGHT); // GObject property; no typed setter below 4.14
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
    /// The host's spelling of File ▸ Open's accelerator, as it must appear
    /// escaped in the generated XML. Pinned as a `#[cfg]`'d literal rather than
    /// derived through `accel::for_host` (GEP-18): the *point* of the
    /// assertion is that this window advertises Command on macOS, and deriving
    /// the expectation from the transform under test would make it agree with
    /// itself and pass whichever modifier it emitted.
    #[cfg(target_os = "macos")]
    const OPEN_ACCEL_XML: &str = "&lt;Meta&gt;o";
    #[cfg(not(target_os = "macos"))]
    const OPEN_ACCEL_XML: &str = "&lt;Primary&gt;o";

    /// No group may be taller than MAX_GROUP_ROWS: the tallest group sets the
    /// window's minimum height, and the window cannot shrink below it (issue found
    /// at 22 rows → 1095 px, switcher off a laptop screen).
    #[test]
    fn no_shortcut_group_is_taller_than_the_cap() {
        let file: Vec<(&str, &str)> = FILE_CMDS.iter().map(|c| (c.label, c.accel)).collect();
        let edit: Vec<(&str, &str)> = EDIT_CMDS.iter().map(|c| (c.label, c.accel)).collect();
        let view: Vec<(&str, &str)> = VIEW_CMDS.iter().map(|c| (c.label, c.accel)).collect();
        for (title, rows) in group_rows(&file, &edit, &view) {
            assert!(!rows.is_empty(), "shortcut group {title:?} is empty");
            assert!(
                rows.len() <= MAX_GROUP_ROWS,
                "shortcut group {title:?} has {} rows; the cap is {MAX_GROUP_ROWS}",
                rows.len()
            );
        }
    }

    /// Every Format command with a shortcut is listed exactly once across the
    /// split Format groups.
    #[test]
    fn every_format_shortcut_is_listed_once() {
        let xml = interface_xml();
        for c in FORMAT_CMDS.iter().filter(|c| !c.accel.is_empty()) {
            let needle = format!(
                "<property name=\"title\">{}</property>",
                xml_escape(c.label)
            );
            assert_eq!(
                xml.matches(&needle).count(),
                1,
                "{:?} listed wrongly",
                c.label
            );
        }
    }

    /// Every editor-only key is shown, with where it works.
    /// MUTATION-CHECKED: dropping the `editor_keys` chain in `group_rows` fails it.
    #[test]
    fn every_editor_key_is_displayed_with_its_context() {
        let xml = interface_xml();
        for k in EDITOR_KEYS.iter() {
            for part in [k.label, k.context] {
                assert!(
                    xml.contains(&xml_escape(part)),
                    "EDITOR_KEYS {:?} is missing {part:?}",
                    k.label
                );
            }
            assert!(
                FORMAT_CMDS.iter().any(|c| c.target == k.target),
                "EDITOR_KEYS {:?} names no Format command",
                k.label
            );
        }
    }

    #[test]
    fn interface_xml_has_groups_and_escapes_accels() {
        let xml = interface_xml();
        for group in [
            "File",
            "Edit",
            "Format",
            "Blocks",
            "Headings",
            "Insert",
            "View",
            "Navigate",
            "Zoom",
            "Windows &amp; Tabs",
        ] {
            assert!(xml.contains(group), "interface XML missing group {group:?}");
        }
        // Accelerators must be entity-escaped, never raw — and spelled for this
        // platform, so a macOS reader is told ⌘O rather than a key the app does
        // not bind.
        assert!(
            xml.contains(OPEN_ACCEL_XML),
            "Open accel missing or not escaped as {OPEN_ACCEL_XML:?}"
        );
        assert!(
            !xml.contains("<property name=\"accelerator\"><"),
            "an accelerator was emitted with raw angle brackets"
        );
    }

    /// Every inline-accelerator command must actually be *displayed* in the
    /// window, not merely bound. `interface_xml` renders a FIXED set of group
    /// headings (see `group_rows`), so an
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
            // The exact title, once: a bare substring let "New Window" be satisfied by
            // "Move Tab to New Window".
            let needle = format!(
                "<property name=\"title\">{}</property>",
                xml_escape(cmd.label)
            );
            assert_eq!(
                xml.matches(&needle).count(),
                1,
                "INLINE_ACCEL_CMDS {:?} (group {:?}) must be displayed exactly once \
                 in the shortcuts window — is its group a rendered heading?",
                cmd.action,
                cmd.group,
            );
            // The host's spelling, since that is what the window renders. Which
            // spelling is right for this platform is pinned by
            // `OPEN_ACCEL_XML` above and by `accel`'s own literal-pinned tests;
            // here the subject is coverage of the table, not the transform.
            let shown = crate::accel::for_host(cmd.accels[0]);
            assert!(
                xml.contains(&xml_escape(&shown)),
                "INLINE_ACCEL_CMDS {:?} canonical accelerator {shown:?} is not shown \
                 in the shortcuts window",
                cmd.action,
            );
        }
    }

    /// The same coverage guard for [`POINTER_CMDS`], and it needs its own: those
    /// rows are bound by nothing, so the bind↔display equality test below cannot
    /// see them, and a row tagged with a group heading `interface_xml` does not
    /// render would silently vanish with no other symptom. The **subtitle** is
    /// asserted too — it is the only part of the row that says *wheel*, so a row
    /// that lost it would advertise a bare modifier as though it were a shortcut.
    ///
    /// MUTATION-CHECKED: deleting the `POINTER_CMDS` loop from [`rows_for`] fails it.
    #[test]
    fn every_pointer_cmd_is_displayed_with_its_subtitle() {
        let xml = interface_xml();
        for cmd in POINTER_CMDS {
            for part in [cmd.label, cmd.subtitle] {
                assert!(
                    xml.contains(&xml_escape(part)),
                    "POINTER_CMDS {:?} (group {:?}) is missing {part:?} — is its \
                     group a rendered heading?",
                    cmd.label,
                    cmd.group,
                );
            }
            let shown = crate::accel::for_host(cmd.accel);
            assert!(
                xml.contains(&xml_escape(&shown)),
                "POINTER_CMDS {:?} modifier {shown:?} is not shown",
                cmd.label,
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
    use std::borrow::Cow;

    /// Every accelerator we render must be parseable by GTK, or the
    /// `GtkShortcutsShortcut` shows nothing / warns. Guards EVERY accelerator in
    /// the shared `INLINE_ACCEL_CMDS` table (the Cmd tables are guarded elsewhere)
    /// against a typo'd accel string.
    /// The window must fit a short screen: its height is set by its tallest group and
    /// it cannot be shrunk, so a group over MAX_GROUP_ROWS pushed the page switcher
    /// off a laptop screen (a 22-row group measured 1095 px). Bounded well under a
    /// 1366×768-class screen's usable height, with headroom for font metrics.
    #[gtktest::test]
    fn the_shortcuts_window_fits_a_short_screen() {
        let w = make_shortcuts_window();
        let (min, _, _, _) = w.measure(gtk::Orientation::Vertical, -1);
        assert!(min <= 720, "shortcuts window needs {min} px of height");
    }

    /// The groups reach the reader in the order they are declared, File and Edit first,
    /// read off the section's pages as its page switcher presents them. GTK's own
    /// re-layout reverses page order each time it runs on a multi-page section, which
    /// had put File and Edit on page 4 of 5 (see `BUILD_MAX_HEIGHT`).
    #[gtktest::test]
    fn groups_appear_in_declaration_order() {
        fn titles(w: &gtk::Widget, out: &mut Vec<String>) {
            if let Some(g) = w.downcast_ref::<gtk::ShortcutsGroup>() {
                out.push(g.property::<Option<String>>("title").unwrap_or_default());
                return;
            }
            let mut c = w.first_child();
            while let Some(ch) = c {
                titles(&ch, out);
                c = ch.next_sibling();
            }
        }
        fn section_stack(w: &gtk::Widget) -> Option<gtk::Stack> {
            if let Some(s) = w.downcast_ref::<gtk::Stack>() {
                if s.parent().is_some_and(|p| p.is::<gtk::ShortcutsSection>()) {
                    return Some(s.clone());
                }
            }
            let mut c = w.first_child();
            while let Some(ch) = c {
                if let Some(s) = section_stack(&ch) {
                    return Some(s);
                }
                c = ch.next_sibling();
            }
            None
        }
        let window = make_shortcuts_window();
        let stack = section_stack(window.upcast_ref()).expect("the section's page stack");
        let mut shown = Vec::new();
        let mut pages = 0;
        let mut page = stack.first_child();
        while let Some(pg) = page {
            titles(&pg, &mut shown);
            pages += 1;
            page = pg.next_sibling();
        }
        let declared: Vec<String> = group_rows(&[], &[], &[])
            .into_iter()
            .map(|(t, _)| t.to_string())
            .collect();
        assert_eq!(shown, declared, "groups are out of declaration order");
        assert!(
            pages > 1,
            "expected the real column height to split the groups into pages"
        );
    }

    #[gtktest::test]
    fn inline_accel_cmds_are_parseable() {
        for cmd in INLINE_ACCEL_CMDS {
            assert!(
                !cmd.accels.is_empty(),
                "INLINE_ACCEL_CMDS {:?} has no accelerator",
                cmd.action
            );
            for accel in cmd.accels {
                // Both the declared string and the host's re-spelling of it must
                // parse: the declared one is what the table asserts, the
                // re-spelling is what is actually bound and drawn.
                for form in [Cow::Borrowed(*accel), crate::accel::for_host(accel)] {
                    assert!(
                        gtk::accelerator_parse(form.as_ref()).is_some(),
                        "INLINE_ACCEL_CMDS {:?} has an unparseable accelerator {form:?}",
                        cmd.action
                    );
                }
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
            // Compared in the HOST's spelling, which is what `register_accelerators`
            // binds. That the host's spelling is the right one is pinned separately
            // by `accel`'s literal expectations — this guard's subject is bind ↔
            // display drift, not the transform.
            let expected: Vec<_> = cmd
                .accels
                .iter()
                .map(|a| gtk::accelerator_parse(crate::accel::for_host(a).as_ref()))
                .collect();
            assert_eq!(
                bound, expected,
                "binding for {:?} drifted from its INLINE_ACCEL_CMDS row",
                cmd.action
            );
        }
    }

    /// A [`POINTER_CMDS`] row's modifier and the mask `window::zoomwheel` tests an
    /// event's state against must be the same platform decision. Nothing else
    /// connects them: the row is bound by no accelerator, so no bind↔display guard
    /// covers it, and the failure it prevents is silent and platform-specific —
    /// a window advertising ⌘ beside a gesture that answers only to Ctrl.
    ///
    /// Also proves the modifier-only accelerator PARSES. GTK's shortcut label hands
    /// anything it does not recognise straight to `gtk_accelerator_parse`, and a
    /// parse failure takes the whole label down rather than degrading, so the row
    /// would render blank with nothing logged.
    ///
    /// MUTATION-CHECKED: `primary_modifier(Platform::Other)` → `ALT_MASK` fails it (and
    /// `accel`'s own sibling guard) — the two are the pair that would have to agree for
    /// a wrong modifier to ship silently.
    #[gtktest::test]
    fn pointer_row_modifier_matches_the_gesture_mask() {
        for cmd in POINTER_CMDS {
            let shown = crate::accel::for_host(cmd.accel);
            let (keyval, modifiers) = gtk::accelerator_parse(shown.as_ref())
                .unwrap_or_else(|| panic!("POINTER_CMDS {shown:?} does not parse"));
            assert_eq!(
                glib::translate::IntoGlib::into_glib(keyval),
                0,
                "POINTER_CMDS {shown:?} names a key — it must be modifiers only, or \
                 the row advertises a keystroke that is bound to nothing"
            );
            // The half of the rendering the source trace could only infer: with no
            // keyval, `GtkShortcutLabel`'s `default:` arm appends `gdk_keyval_name(0)`,
            // and the row draws as a bare modifier keycap only because that is NULL.
            assert!(
                keyval.name().is_none(),
                "keyval 0 has a name, so the row would draw a spurious keycap after \
                 the modifier"
            );
            assert_eq!(
                modifiers,
                crate::accel::primary_modifier_for_host(),
                "POINTER_CMDS {:?} advertises a modifier the gesture does not test for",
                cmd.label,
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
