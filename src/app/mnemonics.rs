//! Menu mnemonics (Alt+letter to open a menu; bare-letter access keys inside).
//!
//! GTK menu-model labels honor a `_` as a mnemonic/access-key marker (the same
//! mechanism `window/tabs/documents_item_label` escapes for dynamic
//! filenames). We DON'T put the `_` in the shared `Cmd.label` / `FmtInsertKind`
//! fields, because those same strings drive the toolbar tooltips and the custom
//! context menu, where a literal `_` (or a hidden access key) would be wrong
//! (ScrAP-9 single source of truth — the label stays literal there).
//! Instead we inject the marker ONLY at menubar-build time, via a lookup keyed on
//! the plain label. Access keys are unique within each popover.
//!
//! Each entry's marked form must equal its plain form with the `_` removed, and
//! every Cmd-table label that reaches the menubar must have an entry — the
//! `menu_mnemonics_wellformed` test enforces both so the two can't drift.

use gtk::prelude::*;

#[rustfmt::skip]
const MENU_MNEMONICS: &[(&str, &str)] = &[
    // File
    ("New Document", "_New Document"), ("Open", "_Open"), ("Save", "_Save"),
    ("Save As…", "Save _As…"), ("Reload", "_Reload"),
    ("Copy Full Path", "Copy Full _Path"), ("Auto-Reload", "A_uto-Reload"),
    ("Load Unsafe Linked Documents", "_Load Unsafe Linked Documents"),
    ("Close Tab", "_Close Tab"), ("Exit", "E_xit"),
    // Edit
    ("Undo", "_Undo"), ("Redo", "_Redo"), ("Copy", "_Copy"), ("Cut", "Cu_t"),
    ("Copy Document", "Copy Docu_ment"), ("Delete", "_Delete"),
    ("Select All", "Select _All"), ("Find", "_Find"),
    ("Find & Replace", "Find & Re_place"), ("Insert Emoji", "Insert _Emoji"),
    ("Annotate", "A_nnotate"), ("Change Case", "Chan_ge Case"),
    ("UPPER CASE", "_UPPER CASE"), ("lower case", "_lower case"),
    ("Title Case", "_Title Case"), ("tOGGLE cASE", "tOGGLE _cASE"),
    // Format
    ("Bold", "_Bold"), ("Italic", "_Italic"), ("Heading", "_Heading"),
    ("Strikethrough", "_Strikethrough"), ("Highlight", "Hi_ghlight"),
    ("Code Span", "_Code Span"),
    ("Superscript", "Su_perscript"), ("Subscript", "S_ubscript"),
    ("Code Block", "Code Bl_ock"), ("Quote", "_Quote"),
    ("Bulleted List", "Bull_eted List"), ("Numbered List", "_Numbered List"),
    ("Task List", "Tas_k List"), ("Horizontal Bar", "Hori_zontal Bar"),
    ("Insert Link…", "Insert _Link…"), ("Edit Link…", "Edit _Link…"),
    ("Insert Image…", "Insert I_mage…"), ("Edit Image…", "Edit I_mage…"),
    ("Insert Table…", "Insert _Table…"),
    // View
    ("Preview", "_Preview"), ("Edit", "_Edit"), ("Side by Side", "_Side by Side"),
    ("New Window", "_New Window"),
    ("Move Tab to New Window", "_Move Tab to New Window"),
    ("Previous Tab", "Pre_vious Tab"), ("Next Tab", "Ne_xt Tab"),
    ("Documents", "_Documents"), ("Outline", "_Outline"),
    ("Annotations", "_Annotations"),
    ("Go To Line…", "_Go To Line…"), ("Toolbar", "_Toolbar"),
    // "R"/"e"/"d"/"g" are all taken in View (Reset Zoom, Edit, Documents, Go To
    // Line), so Reading Theme takes "h". Its ITEMS are deliberately absent from this
    // table: theme names come from themes.toml, so a static list could never cover a
    // user's own theme — they go unmarked, like the dynamic Documents filenames.
    ("Reading Theme", "Reading T_heme"),
    ("Status Bar", "Status _Bar"), ("Show Unsafe Images", "Show _Unsafe Images"),
    ("Zoom In", "Zoom _In"), ("Zoom Out", "_Zoom Out"),
    ("Reset Zoom", "_Reset Zoom"), ("Swap Panes", "S_wap Panes"),
    ("Vertical Split", "Vertical Sp_lit"),
    // View ▸ Toolbar submenu (per-section toggles; "Edit" reuses the entry above)
    ("Show", "_Show"), ("File", "_File"), ("Format", "Fo_rmat"),
    ("View", "_View"), ("Split", "S_plit"), ("Zoom", "_Zoom"),
    // Help
    ("Keyboard Shortcuts", "_Keyboard Shortcuts"),
    ("Markdown Reference", "_Markdown Reference"), ("About", "_About"),
];

/// The mnemonic-marked form of `label` for the menubar, or `label` unchanged when
/// it has no entry (e.g. the dynamic Documents filenames, which are `_`-escaped at
/// their own source, or a `Heading _{n}` literal that already carries its marker).
/// Also reused by the context menus (`window/contextmenu.rs`, `window/tabs/`) so
/// a shared command's access key matches the menubar's.
pub(crate) fn mnem(label: &str) -> String {
    MENU_MNEMONICS
        .iter()
        .find(|(plain, _)| *plain == label)
        .map_or(label, |(_, marked)| marked)
        .to_string()
}

/// Turn a `_`-marked label into `(access_char, pango_markup)` for the CONTEXT menus,
/// which are plain `GtkPopover`+`GtkButton` (not model menus): the char after the
/// first `_` is the access key (lowercased — GTK keyvals for bare letters are
/// lowercase), and the markup wraps that char in `<u>…</u>` so the underline renders
/// WITHOUT the mnemonics-visible/Alt gating a real `use-underline` label depends on
/// (researcher-confirmed: a plain popover never sets mnemonics-visible on its own).
/// The rest of the text is Pango-escaped (labels like "Find & Replace" contain `&`).
/// No `_` ⇒ `(None, escaped_label)`.
pub(crate) fn access_markup(marked: &str) -> (Option<char>, String) {
    if let Some(us) = marked.find('_') {
        let rest = &marked[us + 1..];
        if let Some(ch) = rest.chars().next() {
            let before = &marked[..us];
            let after = &rest[ch.len_utf8()..];
            let markup = format!(
                "{}<u>{}</u>{}",
                gtk::glib::markup_escape_text(before),
                gtk::glib::markup_escape_text(&ch.to_string()),
                gtk::glib::markup_escape_text(after),
            );
            return (Some(ch.to_ascii_lowercase()), markup);
        }
    }
    (None, gtk::glib::markup_escape_text(marked).to_string())
}

/// Build a bare-letter access-key `GtkShortcut` for a context-popover button:
/// `KeyvalTrigger(key, no modifiers)` → activate the button when `gate()` holds and
/// the button `is_sensitive()`. Returns `None` if `key` has no keyval.
///
/// The gate lets one Capture-phase controller serve a multi-page (`GtkStack`)
/// popover: a key on the hidden page returns `Proceed` so the same physical key can
/// belong to a different item on each page (e.g. `u` = Undo on the main page but
/// UPPER CASE on the Change-Case page). `activate()` does NOT itself check
/// sensitivity, so we gate on `is_sensitive()` explicitly (researcher-confirmed).
pub(crate) fn access_shortcut(
    button: &gtk::Button,
    key: char,
    gate: impl Fn() -> bool + 'static,
) -> Option<gtk::Shortcut> {
    let keyval = gtk::gdk::Key::from_name(key.to_string())?;
    let btn = button.downgrade();
    let action = gtk::CallbackAction::new(move |_, _| {
        if !gate() {
            return gtk::glib::Propagation::Proceed;
        }
        if let Some(b) = btn.upgrade() {
            if b.is_sensitive() {
                b.activate();
                return gtk::glib::Propagation::Stop;
            }
        }
        gtk::glib::Propagation::Proceed
    });
    let trigger = gtk::KeyvalTrigger::new(keyval, gtk::gdk::ModifierType::empty());
    Some(gtk::Shortcut::new(Some(trigger), Some(action)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{EDIT_CMDS, FILE_CMDS, FORMAT_CMDS, VIEW_CMDS};

    /// Guards the mnemonic table (`MENU_MNEMONICS`) against drift:
    /// - each marked form must equal its plain form with the `_` removed, and
    ///   must actually carry a marker (so no entry is a silent no-op);
    /// - every Cmd-table label that reaches the menubar must have an entry, so a
    ///   renamed command can't quietly lose its access key.
    #[test]
    fn menu_mnemonics_wellformed() {
        for (plain, marked) in MENU_MNEMONICS {
            assert_eq!(
                marked.replace('_', ""),
                *plain,
                "mnemonic marked form drifted from plain label for {plain:?}"
            );
            assert!(marked.contains('_'), "{plain:?} has no mnemonic marker");
        }
        let has = |label: &str| MENU_MNEMONICS.iter().any(|(p, _)| *p == label);
        for c in FILE_CMDS.iter() {
            assert!(
                has(c.label),
                "no mnemonic entry for FILE_CMDS {:?}",
                c.label
            );
        }
        for c in EDIT_CMDS.iter() {
            assert!(
                has(c.label),
                "no mnemonic entry for EDIT_CMDS {:?}",
                c.label
            );
        }
        for c in VIEW_CMDS.iter() {
            assert!(
                has(c.label),
                "no mnemonic entry for VIEW_CMDS {:?}",
                c.label
            );
        }
        for c in FORMAT_CMDS.iter() {
            assert!(
                has(c.label),
                "no mnemonic entry for FORMAT_CMDS {:?}",
                c.label
            );
        }
    }

    /// The access letter of a marked label = the char right after the first `_`,
    /// lowercased (GTK matches mnemonics case-insensitively).
    fn access_key(plain: &str) -> char {
        let marked = mnem(plain);
        let after = marked.find('_').map(|i| &marked[i + 1..]);
        after
            .and_then(|s| s.chars().next())
            .unwrap_or_else(|| panic!("no access key derivable for {plain:?} ({marked:?})"))
            .to_ascii_lowercase()
    }

    /// Access letters must be UNIQUE within each open popover — a duplicate makes
    /// GTK cycle focus between the clashes instead of activating (a silent bug the
    /// `wellformed` test above can't see, since it doesn't know popover grouping).
    /// Each slice below mirrors one actual popover in `build_menubar`. Submenu
    /// TITLES belong to their PARENT popover (that's where you press their key);
    /// the submenu's own items are a separate popover. Heading 1–6 (digit keys,
    /// trivially unique) and the dynamic Documents list are omitted.
    #[test]
    fn menu_mnemonics_unique_per_popover() {
        // (Insert/Edit Link… and Insert/Edit Image… are the same slot relabelled at
        // runtime — list only the Insert form; both share the L / m key.)
        let popovers: &[(&str, &[&str])] = &[
            (
                "File",
                &[
                    "New Document",
                    "Open",
                    "Save",
                    "Save As…",
                    "Reload",
                    "Copy Full Path",
                    "Auto-Reload",
                    "Load Unsafe Linked Documents",
                    "Close Tab",
                    "Exit",
                ],
            ),
            (
                "Edit",
                &[
                    "Undo",
                    "Redo",
                    "Copy",
                    "Cut",
                    "Copy Document",
                    "Delete",
                    "Select All",
                    "Find",
                    "Find & Replace",
                    "Annotate",
                    "Insert Emoji",
                    "Change Case",
                ],
            ),
            (
                "Edit ▸ Change Case",
                &["UPPER CASE", "lower case", "Title Case", "tOGGLE cASE"],
            ),
            (
                "Format",
                &[
                    "Bold",
                    "Italic",
                    "Heading",
                    "Strikethrough",
                    "Highlight",
                    "Code Span",
                    "Superscript",
                    "Subscript",
                    "Code Block",
                    "Quote",
                    "Bulleted List",
                    "Numbered List",
                    "Task List",
                    "Horizontal Bar",
                    "Insert Link…",
                    "Insert Image…",
                    "Insert Table…",
                ],
            ),
            (
                "View",
                &[
                    "Preview",
                    "Edit",
                    "Side by Side",
                    "New Window",
                    "Move Tab to New Window",
                    "Previous Tab",
                    "Next Tab",
                    "Documents",
                    "Outline",
                    "Annotations",
                    "Go To Line…",
                    "Toolbar",
                    "Status Bar",
                    "Show Unsafe Images",
                    "Zoom In",
                    "Zoom Out",
                    "Reset Zoom",
                    "Swap Panes",
                    "Vertical Split",
                ],
            ),
            (
                "View ▸ Toolbar",
                &["Show", "File", "Edit", "Format", "View", "Split", "Zoom"],
            ),
            (
                "Help",
                &["Keyboard Shortcuts", "Markdown Reference", "About"],
            ),
        ];
        for (menu, labels) in popovers {
            let mut seen: Vec<(char, &str)> = Vec::new();
            for label in *labels {
                let key = access_key(label);
                if let Some((_, other)) = seen.iter().find(|(k, _)| *k == key) {
                    panic!("{menu} popover: access key {key:?} collides — {label:?} vs {other:?}");
                }
                seen.push((key, label));
            }
        }
        // The pane context menu (`window/contextmenu.rs`) reuses `mnem()` for every
        // row — its "main" page mirrors the Edit popover above and its Change Case
        // page mirrors "Edit ▸ Change Case" — so it inherits both consistency and the
        // uniqueness checked here; no separate group is needed.
    }

    /// The tab context menu (`window/tabs/`) builds access keys from these
    /// `_`-marked literals rather than the `MENU_MNEMONICS` table (three of the
    /// five labels are context-only; Reload's literal happens to match the File
    /// menu's own anyway). Guard: internally unique, and the labels that mirror a
    /// menu-bar command keep its access key (File ▸ Close Tab = C; View ▸ Move Tab
    /// to New Window = M; File ▸ Reload = R) so context and menu bar agree. Copy
    /// Full Path deliberately does NOT mirror the File menu's `P` — it uses `F` to
    /// match the `win.copy-path` accelerator's own letter, so it is checked only
    /// for internal uniqueness, not menu-bar agreement.
    #[test]
    fn tab_context_menu_access_keys_consistent() {
        let key = |m: &str| {
            access_markup(m)
                .0
                .expect("marked literal has an access key")
        };
        let (close, close_others, move_win, copy_path, reload) = (
            "_Close Tab",
            "Close _Other Tabs",
            "_Move to New Window",
            "Copy _Full Path",
            "_Reload",
        );
        let keys = [
            key(close),
            key(close_others),
            key(move_win),
            key(copy_path),
            key(reload),
        ];
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(keys[i], keys[j], "tab context-menu access-key collision");
            }
        }
        assert_eq!(
            key(close),
            key(&mnem("Close Tab")),
            "Close Tab key ≠ File menu"
        );
        assert_eq!(
            key(move_win),
            key(&mnem("Move Tab to New Window")),
            "Move to New Window key ≠ View menu"
        );
        assert_eq!(key(reload), key(&mnem("Reload")), "Reload key ≠ File menu");
    }
}
