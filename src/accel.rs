//! The **one** place a command's declared accelerator is rewritten into the
//! spelling the host platform actually uses.
//!
//! POLICY's single-source-of-truth rule says a command's accelerator is declared
//! once, in its `Cmd`/`FmtCmd`/`InlineCmd` descriptor, and that the *same* string
//! drives the binding and every displayed hint. That rule is what makes a
//! per-platform difference dangerous rather than merely fiddly: the moment two
//! surfaces spell a shortcut differently, one of them is lying. So the platform
//! difference is expressed as a single **transform applied to the declared
//! string**, and every consumer — `set_accels_for_action`, the menu item's
//! `accel` attribute, the toolbar tooltip, the shortcuts window, the annotation
//! card's local re-offer — routes through it. There is no per-surface spelling.
//!
//! ## Why a transform is needed at all
//!
//! GTK4 dropped GTK3's Quartz mapping of `<Primary>` onto the Command key, so on
//! macOS `<Primary>` is **Control** — every one of this app's shortcuts landed on
//! Ctrl there, where the platform's own command modifier is Command. GTK itself
//! spells Command `<Meta>` on that backend: `gtk_application_impl_quartz_set_default_accels`
//! binds `app.quit` to `<Meta>q` and `clipboard.copy` to `<Meta>c`, and
//! `GNSMenuItem didChangeAccel` maps `GDK_META_MASK` → `NSEventModifierFlagCommand`
//! (both in GTK 4.22.4's `gtkapplication-quartz.c` / `gtkapplication-quartz-menu.c`,
//! lines 204-224 and 291-292).
//!
//! ## Why it is a SWAP and not a one-way rename
//!
//! The naive rewrite — `<Primary>` → `<Meta>` and nothing else — silently
//! **collides**. `win.copy-document` is declared `<Primary><Meta><Alt>c`, where
//! `<Meta>` is the Super key used purely to hold that command apart from
//! `win.format::task-list`'s `<Primary><Alt>c`. Rewrite only `<Primary>` and both
//! become `<Meta><Alt>c`: two commands, one keystroke, whichever GTK reaches
//! first. Nothing warns.
//!
//! The two modifiers therefore swap, which is also what they *mean*: each
//! platform has a primary command modifier (Ctrl on Linux/Windows, Command on
//! macOS) and a rarely-used extra one (Super on Linux, Control on macOS — the
//! `Ctrl+Cmd+F` fullscreen idiom). `<Primary>`↔`<Meta>` maps each role onto the
//! key that plays it. `<Shift>` and `<Alt>` (Option) need no rewrite.
//!
//! `tests::bindings_are_unique_on_every_platform`
//! is the guard that keeps this true as commands are added; it is the reason the
//! mapping takes an explicit [`Platform`] rather than reading `cfg!` internally —
//! a pure function over plain data can be checked for *both* platforms from
//! either one, which a `#[cfg]`-gated one never can (POLICY forbids `#[cfg]`ing a
//! test precisely because it deletes it).

use std::borrow::Cow;

/// Which platform an accelerator is being spelled for. Passed explicitly so the
/// mapping is a pure function testable for every platform from any host.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Platform {
    /// macOS / the GDK Quartz backend.
    Mac,
    /// Linux and Windows, where the declared spelling is already correct.
    Other,
}

/// The platform this binary was built for.
pub(crate) const fn host() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Mac
    } else {
        Platform::Other
    }
}

/// Keystrokes the macOS platform itself owns, and what the command that declared
/// them takes there instead. Keyed by the **declared** accelerator, which every
/// consumer of this module already holds — keying by action name would leave the
/// display surfaces (menu hint, tooltip, shortcuts window) unable to look the
/// override up, and a surface that cannot see the override is a surface that
/// advertises the wrong key. `accel::tests::bindings_are_unique_on_every_platform`
/// guarantees no two commands declare the same accelerator, so this key is as
/// unique as an action name would be.
///
/// These are not conveniences. GTK's Quartz backend binds its own
/// `gtkinternal.*` actions at application startup
/// (`gtkapplication-quartz.c:202-225`) and inserts them as a real application
/// action group, so they resolve from the window's action muxer and **always
/// win** over a `win.*` action on the same key. MEASURED on GTK 4.22.4: before
/// these overrides, Cmd+H hid the application and Cmd+Option+H hid every other
/// application — Find & Replace and Highlight were simply unreachable from the
/// keyboard, with nothing logged and nothing failing.
///
/// The other halves of GTK's default set — `text.undo`, `text.redo`,
/// `clipboard.cut/copy/paste`, `selection.select-all` — are deliberately NOT
/// listed, and that is a measurement rather than an omission: they are
/// `gtk_widget_class_install_action` entries on the text widgets, not an
/// application action group, so they do not resolve from the window and the
/// app's own command wins. MEASURED: Cmd+A then Cmd+C in the preview yields
/// `# Code blocks` — `win.copy`'s copy-as-Markdown — not the plain buffer text
/// `clipboard.copy` would have produced. Do not "complete" this table with them.
const MAC_RESERVED: [(&str, &str); 2] = [
    // Cmd+H hides the application on every Mac. Find & Replace moves to the
    // macOS convention for it, Cmd+Option+F.
    ("<Primary>h", "<Meta><Alt>f"),
    // Cmd+Option+H hides all other applications. Highlight keeps its H, one
    // modifier over.
    ("<Primary><Alt>h", "<Meta><Shift>h"),
];

/// `accel` as `platform` spells it. Borrowed unchanged whenever nothing moves,
/// so the non-macOS build allocates nothing at all.
pub(crate) fn map(accel: &str, platform: Platform) -> Cow<'_, str> {
    if platform != Platform::Mac {
        return Cow::Borrowed(accel);
    }
    if let Some((_, replacement)) = MAC_RESERVED.iter().find(|(declared, _)| *declared == accel) {
        return Cow::Borrowed(replacement);
    }
    if !(accel.contains("<Primary>") || accel.contains("<Meta>")) {
        return Cow::Borrowed(accel);
    }
    let mut out = String::with_capacity(accel.len());
    let mut rest = accel;
    // Walk complete `<…>` tokens. `rest` is left untouched on any malformed tail
    // so the trailing push below cannot duplicate what was already emitted.
    while let Some(open) = rest.find('<') {
        let Some(close) = rest[open..].find('>') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(match &rest[open..=open + close] {
            "<Primary>" => "<Meta>",
            "<Meta>" => "<Primary>",
            other => other,
        });
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// [`map`] for the platform this binary runs on — what every call site outside a
/// test wants.
pub(crate) fn for_host(accel: &str) -> Cow<'_, str> {
    map(accel, host())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{accelerator_bindings_for, EDIT_CMDS, FILE_CMDS, FORMAT_CMDS, VIEW_CMDS};

    /// Every modifier token the command descriptors are allowed to declare. A
    /// token outside this set is not *wrong* to GTK, but it is unreviewed by
    /// [`map`] — so `only_known_modifiers_are_declared` fails rather than letting
    /// a new modifier reach macOS untranslated and silently keep the Linux key.
    const KNOWN_MODIFIERS: [&str; 4] = ["<Primary>", "<Shift>", "<Alt>", "<Meta>"];

    /// The **keystroke** an accelerator string denotes, as plain data: the *set*
    /// of modifiers (sorted and deduplicated) plus the key name.
    ///
    /// Comparing accelerator *strings* cannot answer "do these two commands share
    /// a keystroke", and getting that wrong is not hypothetical — it is the exact
    /// mistake the collision guard below was first written with, and the mutation
    /// test that should have caught the naive one-way rename passed because of
    /// it. `gtk_accelerator_parse` ORs each modifier into one `GdkModifierType`
    /// mask, so a repeated modifier collapses and order is irrelevant:
    /// `<Meta><Meta><Alt>c` and `<Meta><Alt>c` are two strings and one keystroke.
    /// A one-way `<Primary>`→`<Meta>` rename produces precisely that pair from
    /// `win.copy-document` and `win.format::task-list` — which is why the guard
    /// must normalise before it compares.
    ///
    /// Display-free by design: it models GTK's parse rather than calling it, so
    /// the guard stays a plain `cargo test` unit test with no display and no
    /// `gtk::init` (POLICY § Unit tests).
    fn keystroke(accel: &str) -> (Vec<&str>, &str) {
        let mut modifiers = Vec::new();
        let mut rest = accel;
        while let Some(open) = rest.find('<') {
            let Some(close) = rest[open..].find('>') else {
                break;
            };
            modifiers.push(&rest[open..=open + close]);
            rest = &rest[open + close + 1..];
        }
        modifiers.sort_unstable();
        modifiers.dedup();
        (modifiers, rest)
    }

    #[test]
    fn keystroke_collapses_a_repeated_modifier_and_ignores_order() {
        assert_eq!(keystroke("<Meta><Meta><Alt>c"), keystroke("<Meta><Alt>c"));
        assert_eq!(keystroke("<Alt><Meta>c"), keystroke("<Meta><Alt>c"));
        assert_ne!(keystroke("<Meta><Alt>c"), keystroke("<Meta><Primary>c"));
        assert_ne!(keystroke("<Meta>c"), keystroke("<Meta>x"));
        assert_eq!(keystroke("F9"), (vec![], "F9"));
    }

    #[test]
    fn other_platforms_get_the_declared_string_back_untouched() {
        for accel in ["<Primary>s", "<Primary><Meta><Alt>c", "F9", ""] {
            assert_eq!(map(accel, Platform::Other), accel);
            assert!(
                matches!(map(accel, Platform::Other), Cow::Borrowed(_)),
                "the non-macOS path must not allocate"
            );
        }
    }

    #[test]
    fn mac_swaps_the_command_modifier_with_the_spare_one() {
        assert_eq!(map("<Primary>s", Platform::Mac), "<Meta>s");
        assert_eq!(map("<Primary><Shift>s", Platform::Mac), "<Meta><Shift>s");
        assert_eq!(map("<Alt>Left", Platform::Mac), "<Alt>Left");
        assert_eq!(map("F9", Platform::Mac), "F9");
        assert_eq!(map("", Platform::Mac), "");
        // The collision case this whole module exists for: a one-way rename would
        // fold this onto task-list's `<Primary><Alt>c`.
        assert_eq!(
            map("<Primary><Meta><Alt>c", Platform::Mac),
            "<Meta><Primary><Alt>c"
        );
        assert_ne!(
            map("<Primary><Meta><Alt>c", Platform::Mac),
            map("<Primary><Alt>c", Platform::Mac)
        );
    }

    /// Every accelerator the descriptors declare is built from modifiers [`map`]
    /// has actually been reasoned about. A new one (`<Super>`, `<Hyper>`, a bare
    /// `<Control>`) would otherwise reach macOS untranslated and silently keep
    /// the Linux key.
    #[test]
    fn only_known_modifiers_are_declared() {
        let mut declared: Vec<String> = Vec::new();
        for c in FILE_CMDS.iter().chain(EDIT_CMDS.iter()) {
            declared.push(c.accel.to_string());
        }
        for c in VIEW_CMDS.iter() {
            declared.push(c.accel.to_string());
        }
        for c in FORMAT_CMDS.iter() {
            declared.push(c.accel.to_string());
        }
        for c in crate::app::INLINE_ACCEL_CMDS {
            for a in c.accels {
                declared.push((*a).to_string());
            }
        }
        for accel in &declared {
            let mut rest = accel.as_str();
            while let Some(open) = rest.find('<') {
                let close = rest[open..]
                    .find('>')
                    .unwrap_or_else(|| panic!("unterminated modifier token in {accel:?}"));
                let token = &rest[open..=open + close];
                assert!(
                    KNOWN_MODIFIERS.contains(&token),
                    "{accel:?} declares {token:?}, which `accel::map` does not translate — \
                     add it to the match (and to KNOWN_MODIFIERS) or use a known modifier"
                );
                rest = &rest[open + close + 1..];
            }
        }
    }

    /// **The guard the swap exists to satisfy.** No keystroke may drive two
    /// different actions, on either platform — a collision is silent (GTK simply
    /// activates whichever shortcut it reaches first) and platform-specific, so
    /// nothing on Linux would ever reveal one introduced on macOS.
    ///
    /// Runs both platforms from whichever host executes it, which is only
    /// possible because [`map`] and `accelerator_bindings_for` take the platform
    /// as data rather than reading `cfg!`.
    ///
    /// Compares [`keystroke`]s, never strings — see that function for why the
    /// string comparison this started as was permanently green.
    /// MUTATION-CHECKED: deleting the `"<Meta>" => "<Primary>"` arm from [`map`]
    /// (i.e. reverting to a one-way `<Primary>`→`<Meta>` rename) fails this test
    /// on `Mac` with `win.copy-document` against `win.format::task-list`.
    #[test]
    fn bindings_are_unique_on_every_platform() {
        for platform in [Platform::Other, Platform::Mac] {
            let bindings = accelerator_bindings_for(platform);
            for (i, (action, accel)) in bindings.iter().enumerate() {
                for (other_action, other_accel) in &bindings[i + 1..] {
                    if keystroke(accel) == keystroke(other_accel) {
                        assert_eq!(
                            action, other_action,
                            "{platform:?}: {accel:?} and {other_accel:?} are the same \
                             keystroke but drive {action:?} and {other_action:?} — \
                             two commands, one key"
                        );
                    }
                }
            }
        }
    }

    /// Every keystroke GTK's Quartz backend claims for itself at application
    /// startup, transcribed from `gtkapplication-quartz.c:202-225` (GTK 4.22.4)
    /// in GTK's own spelling. `app.quit` is deliberately absent: GTK binds it to
    /// the same `<Meta>q` this app's own `app.quit` takes, so that one is an
    /// agreement rather than a collision.
    const GTK_QUARTZ_RESERVED: [&str; 4] = [
        "<Meta>comma",    // app.preferences
        "<Meta><Alt>h",   // gtkinternal.hide-others
        "<Meta>h",        // gtkinternal.hide
        "<Meta><Shift>q", // guards against a near-miss on quit
    ];

    /// **No macOS binding may land on a keystroke GTK's Quartz backend has
    /// already claimed.** Those actions live in a real application action group,
    /// so they resolve from the window's muxer and win — the app's own command
    /// then does nothing at all, with no warning, no log line and no failing
    /// test anywhere else.
    ///
    /// This is the guard `MAC_RESERVED` exists to satisfy, and the one that will
    /// catch the *next* command to declare a `<Primary>h`-shaped shortcut.
    /// MUTATION-CHECKED: emptying `MAC_RESERVED` fails this test on
    /// `win.find-replace` and `win.format::highlight`.
    #[test]
    fn no_mac_binding_lands_on_a_keystroke_gtk_reserves() {
        for (action, accel) in accelerator_bindings_for(Platform::Mac) {
            for reserved in GTK_QUARTZ_RESERVED {
                assert_ne!(
                    keystroke(&accel),
                    keystroke(reserved),
                    "{action:?} binds {accel:?}, which GTK's Quartz backend already \
                     claims ({reserved:?}) — GTK's action wins and {action:?} becomes \
                     unreachable from the keyboard. Add an entry to MAC_RESERVED."
                );
            }
        }
    }

    /// A reserved-keystroke override must actually be reachable: it has to name a
    /// declared accelerator, or it silently governs nothing.
    #[test]
    fn every_mac_override_names_a_declared_accelerator() {
        let declared: Vec<String> = FILE_CMDS
            .iter()
            .chain(EDIT_CMDS.iter())
            .map(|c| c.accel.to_string())
            .chain(FORMAT_CMDS.iter().map(|c| c.accel.to_string()))
            .chain(VIEW_CMDS.iter().map(|c| c.accel.to_string()))
            .chain(
                crate::app::INLINE_ACCEL_CMDS
                    .iter()
                    .flat_map(|c| c.accels.iter().map(|a| (*a).to_string())),
            )
            .collect();
        for (key, _) in MAC_RESERVED {
            assert!(
                declared.iter().any(|d| d == key),
                "MAC_RESERVED overrides {key:?}, which no command declares — \
                 the override governs nothing"
            );
        }
    }

    /// Every binding a macOS build makes is reachable from the declared table,
    /// and the two platforms bind the same *set of actions* — the transform may
    /// re-spell a shortcut but must never add or drop a command.
    #[test]
    fn both_platforms_bind_the_same_commands() {
        let mut linux: Vec<String> = accelerator_bindings_for(Platform::Other)
            .into_iter()
            .map(|(action, _)| action)
            .collect();
        let mut mac: Vec<String> = accelerator_bindings_for(Platform::Mac)
            .into_iter()
            .map(|(action, _)| action)
            .collect();
        linux.sort();
        mac.sort();
        assert_eq!(linux, mac);
    }
}
