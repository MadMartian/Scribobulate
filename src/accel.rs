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
//! **collides** for any command that declares `<Meta>` beside `<Primary>`: a command
//! declared `<Primary><Meta><Alt>c` next to another declared `<Primary><Alt>c` would
//! both become `<Meta><Alt>c` — two commands, one keystroke, whichever GTK reaches
//! first. Nothing warns. No descriptor declares `<Meta>` today; the swap is what keeps
//! a future one from colliding.
//!
//! The two modifiers therefore swap, which is also what they *mean*: each
//! platform has a primary command modifier (Ctrl on Linux/Windows, Command on
//! macOS) and a rarely-used extra one (Super on Linux, Control on macOS — the
//! `Ctrl+Cmd+F` fullscreen idiom). `<Primary>`↔`<Meta>` maps each role onto the
//! key that plays it. `<Shift>` and `<Alt>` (Option) need no rewrite **as
//! modifiers** — but two specific `<Alt>`-held keystrokes are, like the
//! `MAC_RESERVED` entries below, overridden outright: see `<Alt>Left`/`<Alt>Right`
//! there. That is not a rewrite of the `<Alt>` token itself (which still means
//! nothing special to `map`), only of the two whole strings that happen to use it.
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
///
/// **`<Alt>Left`/`<Alt>Right` are a different kind of reservation, entered here
/// for the same *effect* on a different *mechanism*.** GTK's Quartz backend does
/// not itself claim these two — there is no `gtkapplication-quartz.c` entry for
/// them, and nothing measures a collision the way Cmd+H/Cmd+Option+H do above. What
/// actually claims them, measured, is `GtkSourceView`'s own `move-words` class
/// keybinding — a *word-transposition* editing command, default-bound to
/// Alt+Left/Right by GtkSourceView itself (`gtksourceview.c:953`) — which fires on
/// the focused editor **ahead of** `win.nav-back`/`win.nav-forward` (TDD §23.6)
/// declaring the identical keystroke: measured with this reservation reverted,
/// Alt+Left silently reordered the buffer's words rather than walking history or
/// moving the caret. See `macwordnav`'s module doc comment for the full mechanism
/// and the fix (a key controller that pre-empts `move-words`); freeing this
/// keystroke here is what lets that fix claim it instead. Back/Forward itself moves
/// to Safari/Finder's own spelling, Cmd+[ / Cmd+] — a keystroke it was never
/// actually reachable on from the editor, but every native macOS text field still
/// binds Option+Left/Option+Right to word movement at the AppKit layer, and Back/
/// Forward has no business contesting a key the platform owns just because GTK's
/// own binding happened to win the contest first. `XF86Back`/`XF86Forward` and the
/// two mouse thumb buttons are unaffected (TDD §23.6 declares them platform-neutral
/// aliases already).
const MAC_RESERVED: [(&str, &str); 6] = [
    // Cmd+H hides the application on every Mac. Find & Replace moves to the
    // macOS convention for it, Cmd+Option+F.
    ("<Primary>h", "<Meta><Alt>f"),
    // Cmd+Option+H hides all other applications. Highlight keeps its H, one
    // modifier over.
    ("<Primary><Alt>h", "<Meta><Shift>h"),
    // Frees Option+Left for the editor's own word-navigation (`macwordnav`).
    ("<Alt>Left", "<Meta>bracketleft"),
    // Frees Option+Right for the editor's own word-navigation (`macwordnav`).
    ("<Alt>Right", "<Meta>bracketright"),
    // Increase / Decrease Indent declare Ctrl+] / Ctrl+[, which on a Mac would be
    // Cmd+] / Cmd+[ — Back/Forward's keys just above. They take Cmd+Ctrl instead:
    // Option or Shift on a bracket changes the character typed, which GDK's matcher
    // does not see through (GTK4Rs/AP-51). Tab / Shift+Tab on a list item, the
    // macOS convention for list levels, works there as everywhere.
    ("<Primary>bracketright", "<Meta><Control>bracketright"),
    ("<Primary>bracketleft", "<Meta><Control>bracketleft"),
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

/// The modifier `<Primary>` denotes on `platform`, as an event mask.
///
/// The same fact [`map`] expresses about accelerator *strings*, asked for by a
/// caller that has no string to re-spell: a pointer gesture carries a
/// [`gdk::ModifierType`], not an accelerator. It lives here, beside the swap,
/// because the two must agree — a Ctrl+wheel on a platform whose menus all read ⌘
/// is a command the user cannot find. `mask_matches_the_respelled_accelerator`
/// pins them together against GTK's own parse.
pub(crate) const fn primary_modifier(platform: Platform) -> gtk::gdk::ModifierType {
    match platform {
        // GTK spells macOS's Command `<Meta>`, and the Quartz backend reports it as
        // `GDK_META_MASK` in an event's modifier state.
        Platform::Mac => gtk::gdk::ModifierType::META_MASK,
        Platform::Other => gtk::gdk::ModifierType::CONTROL_MASK,
    }
}

/// [`primary_modifier`] for the platform this binary runs on.
pub(crate) const fn primary_modifier_for_host() -> gtk::gdk::ModifierType {
    primary_modifier(host())
}

/// The shortcut that presses a dialog button whose label marks `letter` as its mnemonic
/// (`B_rowse…` → `r`), on `platform` — or `None` where the toolkit's own Alt+letter
/// mnemonic already does it.
///
/// On macOS the Alt key is Option, and Option+letter **types a character** (Option+S is
/// `ß`, Option+R is `®`): GTK's mnemonic never sees a plain letter, so the keystroke
/// lands in the focused field instead of pressing the button. Command+letter is the
/// platform's own way to press a dialog button, and it types nothing. Spelled `<Primary>`
/// and passed through [`map`], so it is Command by the same swap every other shortcut
/// uses rather than by a second spelling of the same fact.
pub(crate) fn dialog_button_accel(letter: char, platform: Platform) -> Option<String> {
    match platform {
        Platform::Mac => Some(
            map(
                &format!("<Primary>{}", letter.to_ascii_lowercase()),
                platform,
            )
            .into_owned(),
        ),
        Platform::Other => None,
    }
}

/// The mnemonic letter a label marks with `_` (`B_rowse…` → `r`), lower-cased; `None`
/// when it marks none. A doubled `__` is a literal underscore, not a marker.
pub(crate) fn mnemonic_letter(label: &str) -> Option<char> {
    let mut chars = label.chars();
    while let Some(c) = chars.next() {
        if c == '_' {
            match chars.next() {
                Some('_') => continue,
                Some(m) => return Some(m.to_ascii_lowercase()),
                None => return None,
            }
        }
    }
    None
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
    /// A one-way `<Primary>`→`<Meta>` rename produces precisely that pair from a
    /// `<Primary><Meta><Alt>c` and a `<Primary><Alt>c` declaration — which is why the
    /// guard must normalise before it compares.
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
        // <Alt>Left/Right are the MAC_RESERVED exception, not the general rule —
        // see `mac_reserves_option_arrow_for_word_navigation` for that one. A
        // key `<Alt>` combination unrelated to Back/Forward still passes through.
        assert_eq!(map("<Alt><Shift>2", Platform::Mac), "<Alt><Shift>2");
        assert_eq!(map("F9", Platform::Mac), "F9");
        assert_eq!(map("", Platform::Mac), "");
        // The collision case the swap exists for: a one-way rename would fold a
        // `<Primary><Meta><Alt>c` declaration onto a `<Primary><Alt>c` one.
        assert_eq!(
            map("<Primary><Meta><Alt>c", Platform::Mac),
            "<Meta><Primary><Alt>c"
        );
        assert_ne!(
            map("<Primary><Meta><Alt>c", Platform::Mac),
            map("<Primary><Alt>c", Platform::Mac)
        );
    }

    /// Back/Forward moves off Option+Left/Right on macOS, freeing the keystroke
    /// for the editor's own word navigation (`macwordnav`), and lands on
    /// Safari/Finder's own spelling rather than colliding with it.
    #[test]
    fn mac_reserves_option_arrow_for_word_navigation() {
        assert_eq!(map("<Alt>Left", Platform::Mac), "<Meta>bracketleft");
        assert_eq!(map("<Alt>Right", Platform::Mac), "<Meta>bracketright");
        // Unaffected on every other platform — the declared, browser-convention
        // spelling is still exactly what's bound.
        assert_eq!(map("<Alt>Left", Platform::Other), "<Alt>Left");
        assert_eq!(map("<Alt>Right", Platform::Other), "<Alt>Right");
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
    /// It only sees the descriptors as they are: no command declares `<Meta>` today, so
    /// deleting the `"<Meta>" => "<Primary>"` arm from [`map`] does not fail it. That arm
    /// is pinned by `mac_swaps_the_command_modifier_with_the_spare_one` instead, and this
    /// test catches the collision the day a descriptor does declare `<Meta>`.
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

    #[test]
    fn a_dialog_button_is_pressed_with_command_on_mac_and_by_the_toolkit_elsewhere() {
        assert_eq!(
            dialog_button_accel('R', Platform::Mac).as_deref(),
            Some("<Meta>r")
        );
        assert_eq!(dialog_button_accel('s', Platform::Other), None);
    }

    #[test]
    fn a_mnemonic_letter_is_the_one_after_the_marker() {
        assert_eq!(mnemonic_letter("B_rowse…"), Some('r'));
        assert_eq!(mnemonic_letter("_Section"), Some('s'));
        assert_eq!(mnemonic_letter("snake__case"), None);
        assert_eq!(mnemonic_letter("a__b_Xy"), Some('x'));
        assert_eq!(mnemonic_letter("Plain"), None);
        assert_eq!(mnemonic_letter("trailing_"), None);
    }
}

/// GTK-object tests (`gtk::accelerator_parse` needs an initialised GTK, hence the
/// `gtk-integration-tests` feature and `#[gtktest::test]`).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// The mask a pointer gesture tests must be the modifier GTK actually parses out
    /// of `<Primary>` **as this platform spells it**. The two are separate statements
    /// of one fact — [`map`] re-spells a string, [`primary_modifier`] answers with a
    /// mask — and nothing but this connects them, so a swap corrected in one and not
    /// the other would leave Ctrl+wheel live on a Mac whose every menu reads ⌘.
    ///
    /// Both platforms are checked from either host: the masks are compared against
    /// GTK's parse of the string `map` produces for that platform, which is a pure
    /// transform of a literal rather than a `cfg`.
    ///
    /// MUTATION-CHECKED: `primary_modifier(Platform::Other)` → `ALT_MASK` fails it.
    #[gtktest::test]
    fn mask_matches_the_respelled_accelerator() {
        for platform in [Platform::Mac, Platform::Other] {
            let spelled = map("<Primary>a", platform);
            let (_key, modifiers) = gtk::accelerator_parse(spelled.as_ref())
                .expect("<Primary>a re-spells to something GTK can parse");
            assert_eq!(
                modifiers,
                primary_modifier(platform),
                "{platform:?}: the gesture mask disagrees with the accelerator {spelled:?}"
            );
        }
    }

    /// The Mac dialog-button spelling is one GTK parses into the modifier `<Primary>`
    /// means there, so the binding and every other Command shortcut agree.
    #[gtktest::test]
    fn the_mac_dialog_button_accel_parses_to_command() {
        let accel = dialog_button_accel('s', Platform::Mac).expect("mac binds");
        let (key, mods) = gtk::accelerator_parse(&accel).expect("GTK parses it");
        assert_eq!(key, gtk::gdk::Key::s);
        assert_eq!(mods, primary_modifier(Platform::Mac));
    }
}
