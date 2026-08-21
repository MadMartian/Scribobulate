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
    ("Save All", "Save A_ll"), ("Save As…", "Save _As…"), ("Reload", "_Reload"),
    ("Copy Full Path", "Copy Full _Path"), ("Auto-Reload", "A_uto-Reload"),
    // `L` belongs to Save A_ll (which pairs visually with Save _As…), so this item
    // takes `D` — the whole-word key on "Documents", free in the File popover.
    ("Load Unsafe Linked Documents", "Load Unsafe Linked _Documents"),
    ("Close Tab", "_Close Tab"), ("Exit", "E_xit"),
    // File menu: R/e/n are taken (Reload, …, New Document), so Rename takes `m`.
    ("Rename…", "Rena_me…"),
    // File ▸ Export and its two sinks. `E` is free in the File popover (Exit took
    // `x`); `P`/`H` are free inside the Export submenu, which is its own popover, so
    // they do not collide with Copy Full _Path one level up.
    ("Export", "_Export"), ("PDF", "_PDF"), ("HTML", "_HTML"),
    // Edit
    ("Undo", "_Undo"), ("Redo", "_Redo"), ("Copy", "_Copy"), ("Cut", "Cu_t"),
    ("Copy Document", "Copy Docu_ment"),
    ("Copy Link Location", "Copy _Link Location"), ("Delete", "_Delete"),
    ("Select All", "Select _All"), ("Find", "_Find"),
    ("Find & Replace", "Find & Re_place"), ("Insert Emoji", "Insert _Emoji"),
    ("Annotate", "A_nnotate"), ("Change Case", "Chan_ge Case"),
    // The annotation walk (INLINE_ACCEL_CMDS, not an EDIT_CMDS row). `x`/`v` mirror
    // View's Ne_xt Tab / Pre_vious Tab, and N/P are taken here (Annotate, Replace).
    ("Next Annotation", "Ne_xt Annotation"),
    ("Previous Annotation", "Pre_vious Annotation"),
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
    // "B" is Status Bar's and "f" is free in View, so Back takes "k" and Forward
    // "f" (the `unique_per_popover` test below is what holds this).
    ("Back", "Bac_k"), ("Forward", "_Forward"),
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

    /// Every popover the menubar can open, as `(path, marked labels)` pairs.
    ///
    /// A `GtkPopoverMenu` renders ONE menu model: its sections are drawn in the
    /// SAME popover (so their items share one access-key namespace), while a
    /// `submenu` link opens a NEW popover — the submenu's TITLE belongs to the
    /// parent (that is where its key is pressed), its items to the child.
    fn collect_popovers(
        path: &str,
        model: &gtk::gio::MenuModel,
        out: &mut Vec<(String, Vec<String>)>,
    ) {
        let mut labels: Vec<String> = Vec::new();
        // Sections are flattened into `labels`; submenus are queued as their own
        // popovers and walked after this one, so `out` reads in menu order.
        let mut nested: Vec<(String, gtk::gio::MenuModel)> = Vec::new();
        let flatten = |model: &gtk::gio::MenuModel,
                       labels: &mut Vec<String>,
                       nested: &mut Vec<(String, gtk::gio::MenuModel)>| {
            let mut stack = vec![model.clone()];
            while let Some(m) = stack.pop() {
                for i in 0..m.n_items() {
                    let label = m
                        .item_attribute_value(i, "label", None)
                        .and_then(|v| v.str().map(str::to_string));
                    if let Some(sec) = m.item_link(i, "section") {
                        stack.push(sec);
                    }
                    if let Some(label) = label {
                        if let Some(sub) = m.item_link(i, "submenu") {
                            nested.push((format!("{path} ▸ {}", label.replace('_', "")), sub));
                        }
                        labels.push(label);
                    }
                }
            }
        };
        flatten(model, &mut labels, &mut nested);
        out.push((path.to_string(), labels));
        for (child_path, child) in nested {
            collect_popovers(&child_path, &child, out);
        }
    }

    /// Every popover of the REAL menubar model, plus the menubar strip itself
    /// (whose Alt+letter titles are their own namespace).
    fn menubar_popovers() -> Vec<(String, Vec<String>)> {
        let menus = crate::app::menubar::build_top_level_menus().menus;
        let mut out = vec![(
            "menubar".to_string(),
            menus.iter().map(|(title, _)| title.to_string()).collect(),
        )];
        for (title, menu) in &menus {
            collect_popovers(
                &title.replace('_', ""),
                menu.clone().upcast_ref::<gtk::gio::MenuModel>(),
                &mut out,
            );
        }
        out
    }

    /// The popovers whose items are DYNAMIC and therefore deliberately unmarked:
    /// theme names come from `themes.toml` and the Documents list from the open
    /// tabs, so no static table could carry an access key for them.
    fn is_dynamic_popover(path: &str) -> bool {
        path.ends_with("▸ Reading Theme") || path.ends_with("▸ Documents")
    }

    /// Access letters must be UNIQUE within each open popover — a duplicate makes
    /// GTK cycle focus between the clashes instead of activating.
    ///
    /// The popover grouping is DERIVED from the menu models `build_menubar` ships
    /// (`build_top_level_menus`), never mirrored. The mirror this replaces had gone
    /// eight entries out of date — `Save All`, `Rename…`, `Export`, `PDF`, `HTML`,
    /// `Edit Link…`, `Edit Image…` and `Reading Theme` appeared in no list — and so
    /// stayed green over a live collision (`Save A_ll` vs `_Load Unsafe Linked
    /// Documents`, both in the File popover). A guard whose input set is a second
    /// copy of the thing it checks reports on the copy.
    #[test]
    fn menu_access_keys_unique_per_popover() {
        let popovers = menubar_popovers();
        // `File ▸ Export` is named explicitly because `_PDF`/`_HTML` are only free of
        // Copy Full _Path one level up while Export is its OWN popover. That claim was
        // made in a code comment with nothing holding it; this holds it.
        for expected in ["File", "File ▸ Export"] {
            assert!(
                popovers.iter().any(|(path, _)| path == expected),
                "expected the {expected:?} popover among {:?}",
                popovers.iter().map(|(p, _)| p).collect::<Vec<_>>()
            );
        }
        // ScrAP-132 guard-against-the-guard: a walker whose descent is wrong scans
        // nothing and passes forever. Pin that the File popover really does reach
        // through `build_command_menu`'s SECTIONS to the items this issue was about.
        let file = &popovers
            .iter()
            .find(|(path, _)| path == "File")
            .expect("File popover")
            .1;
        for label in ["Save A_ll", mnem("Load Unsafe Linked Documents").as_str()] {
            assert!(
                file.contains(&label.to_string()),
                "File popover missing {label:?} — the section walk is not descending; got {file:?}"
            );
        }
        // Collected, not panicked on at the first hit: this guard's whole purpose is
        // to report the FULL set of gaps in one run, since the failure it exists for
        // is an input set that quietly stopped covering the menu.
        let mut problems: Vec<String> = Vec::new();
        for (path, labels) in &popovers {
            if is_dynamic_popover(path) {
                // Pin the exemption rather than skipping silently: if one of these
                // ever gains a static, markable item, this fails and the item joins
                // the uniqueness check below instead of hiding behind the exemption.
                for label in labels {
                    if access_markup(label).0.is_some() {
                        problems.push(format!(
                            "{path}: {label:?} carries an access key, but this popover is \
                             exempted as dynamic — drop the exemption or the marker"
                        ));
                    }
                }
                continue;
            }
            let mut seen: Vec<(char, &str)> = Vec::new();
            for label in labels {
                let Some(key) = access_markup(label).0 else {
                    problems.push(format!(
                        "{path} popover: {label:?} has no access key — missing mnem()?"
                    ));
                    continue;
                };
                if let Some((_, other)) = seen.iter().find(|(k, _)| *k == key) {
                    problems.push(format!(
                        "{path} popover: access key {key:?} collides — {label:?} vs {other:?}"
                    ));
                }
                seen.push((key, label));
            }
        }
        assert!(problems.is_empty(), "\n{}", problems.join("\n"));
        // The pane context menu (`window/contextmenu.rs`) reuses `mnem()` for every
        // row — its "main" page mirrors the Edit popover and its Change Case page
        // mirrors "Edit ▸ Change Case" — so it inherits both consistency and the
        // uniqueness checked here; no separate group is needed.
    }

    /// QA M-3: exercise the REAL menu-model builders, so a submenu title that
    /// forgets `mnem()` can no longer pass silently — `Format ▸ Heading` did
    /// exactly that (it shipped the bare "Heading") while the old hand-maintained
    /// mirror reserved `H` for it. These builders are pure `gio::Menu` models (no
    /// widgets), so the test runs headlessly.
    #[test]
    fn every_runtime_submenu_title_carries_a_mnemonic() {
        // A submenu's title is the popover path's last segment; its own popover is
        // named after it, so the nested paths ARE the titles.
        let popovers = menubar_popovers();
        let titles: Vec<&str> = popovers
            .iter()
            .filter_map(|(path, _)| path.rsplit(" ▸ ").next().filter(|_| path.contains(" ▸ ")))
            .collect();
        assert!(!titles.is_empty(), "expected at least one nested submenu");
        for title in &titles {
            assert!(
                popovers.iter().any(|(_, labels)| labels
                    .iter()
                    .any(|l| l.contains('_') && l.replace('_', "") == *title)),
                "runtime submenu title {title:?} has no mnemonic marker — missing mnem()?"
            );
        }
        assert!(
            titles.contains(&"Heading"),
            "Format ▸ Heading must be a submenu; got {titles:?}"
        );
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
