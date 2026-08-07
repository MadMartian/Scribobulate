//! Command descriptor tables — the single source of truth for every command's
//! label, accelerator, and icon. All three UI surfaces (menubar model,
//! context-menu buttons, `set_accels_for_action`) are derived from these arrays,
//! so adding or changing a command here keeps every surface in sync (POLICY.md
//! single-source-of-truth rule; ScrAP-9).

use crate::icons::Icon;

pub(crate) const WELCOME: &str = r#"# Scribobulate

A native Markdown viewer/editor. Open a file to begin:

```sh
scribobulate path/to/document.md
```
"#;

// ── command descriptor table (single source of truth for labels/accels/icons) ─
// All three surfaces — menubar model, context-menu buttons, set_accels_for_action
// — are derived from these arrays.  Add/change a command here; nothing else drifts.

/// Platform-correct human label for a GTK accelerator string (e.g. `"<Primary>o"`
/// → `"Ctrl+O"`), or `None` for an empty accel. Wraps `accelerator_parse` +
/// `accelerator_get_label` — the single derivation the pane context menu, the
/// toolbar tooltips, and the keyboard-shortcuts window all share, so a
/// shortcut hint is always rendered the same, platform-correct way (single
/// source of truth, ScrAP-9).
pub(crate) fn accel_hint(accel: &str) -> Option<String> {
    if accel.is_empty() {
        return None;
    }
    gtk::accelerator_parse(accel).map(|(k, m)| gtk::accelerator_get_label(k, m).to_string())
}

/// A toolbar/tooltip string joining a command label with its accelerator hint in
/// parentheses when it has one — `"Open (Ctrl+O)"` (GNOME HIG: toolbar controls
/// carry tooltips, and the tooltip includes the accel — TDD 16.4).
/// Falls back to the bare label for commands with no shortcut.
pub(crate) fn tooltip_with_accel(label: &str, accel: &str) -> String {
    match accel_hint(accel) {
        Some(hint) => format!("{label} ({hint})"),
        None => label.to_string(),
    }
}

pub(crate) struct Cmd {
    pub(crate) action: &'static str,
    pub(crate) label: &'static str,
    /// GTK accelerator format used by set_accels_for_action and the menu "accel"
    /// display attribute.  Empty string = no keyboard shortcut.
    /// The context-menu accel hint is derived at runtime via accelerator_get_label
    /// so it is always platform-correct (here, "Ctrl+C"). Deliberately no example
    /// for a platform this tree does not build for: the previous one asserted a
    /// Command glyph for macOS and was simply wrong — GTK4 dropped GTK3's Quartz
    /// `<Primary>`→Command mapping, so `<Primary>` is Control there too. A port
    /// that needs the other spelling carries it on its own branch.
    pub(crate) accel: &'static str,
    /// The toolbar/menu icon for this command (all symbolic — ScrAP-169).
    pub(crate) icon: Icon,
    /// When true, insert a section separator before this item in both the menu
    /// bar model and the context-menu popup box.
    pub(crate) section_start: bool,
    /// When true, this command is a boolean toggle: the toolbar renders a
    /// `GtkToggleButton` and the menu item is checkable (bound to a stateful
    /// boolean action). False for ordinary one-shot commands.
    pub(crate) is_toggle: bool,
}

pub(crate) const FILE_CMDS: [Cmd; 10] = [
    // Relabeled "New Document" (operator decision) — opens a
    // new TAB in the current window rather than a new window (View ▸ New
    // Window, below, is the old window-spawning behavior). Ctrl+N moves to
    // New Window; Ctrl+T is free for this because Insert Table
    // moved to Ctrl+Shift+T (see FORMAT_CMDS).
    Cmd {
        action: "app.new",
        label: "New Document",
        accel: "<Primary>t",
        icon: Icon::DocumentNew,
        section_start: false,
        is_toggle: false,
    },
    Cmd {
        action: "app.open",
        label: "Open",
        accel: "<Primary>o",
        icon: Icon::DocumentOpen,
        section_start: false,
        is_toggle: false,
    },
    Cmd {
        action: "win.save",
        label: "Save",
        accel: "<Primary>s",
        icon: Icon::DocumentSave,
        section_start: false,
        is_toggle: false,
    },
    // Save every tab in this window that needs writing (dirty or deleted
    // backing file). Same icon as Save — themes' document-save-all is not
    // universal (MustResolve icons cannot miss — GTK4Rs/AP-48); the label
    // differentiates. Accel is layout-stable <Primary><Alt>s (save family),
    // not <Shift>+letter (and not <Primary><Shift>s, which is Save As).
    Cmd {
        action: "win.save-all",
        label: "Save All",
        accel: "<Primary><Alt>s",
        icon: Icon::DocumentSave,
        section_start: false,
        is_toggle: false,
    },
    Cmd {
        action: "win.save-as",
        label: "Save As…",
        accel: "<Primary><Shift>s",
        icon: Icon::DocumentSaveAs,
        section_start: false,
        is_toggle: false,
    },
    // Revert the buffer to the on-disk version; confirms first
    // if there are unsaved edits. Disabled until a file is open.
    Cmd {
        action: "win.reload",
        label: "Reload",
        accel: "",
        icon: Icon::ViewRefresh,
        section_start: false,
        is_toggle: false,
    },
    // Copies the open document's absolute path to the clipboard.
    // Disabled until a file is open.
    Cmd {
        action: "win.copy-path",
        label: "Copy Full Path",
        accel: "<Primary><Alt><Shift>f",
        icon: Icon::EditCopy,
        section_start: true,
        is_toggle: false,
    },
    // Toggle live auto-reload of external file changes.
    // Default on; checkable menu item + toolbar toggle button.
    Cmd {
        action: "win.auto-reload",
        label: "Auto-Reload",
        accel: "",
        icon: Icon::EmblemSynchronizing,
        section_start: false,
        is_toggle: true,
    },
    // A DEDICATED per-tab security toggle for local-link navigation containment
    // (`09b43a2`, `49d21cb`) — deliberately NOT a reuse of "Show Unsafe Images"
    // (that toggle governs image loading; overloading one label onto two
    // unrelated consents would let a user who ticked it for pictures silently
    // grant filesystem navigation). The two remain distinct per-tab consents
    // over different content — but they are deliberately NAMED alike, on the
    // same axis, which is what this label is for. Default off; genuinely
    // per-tab (`TabState.allow_outside_links` — see that field's own doc
    // comment for the full scope/persistence/no-inheritance rationale, which
    // belongs at the field's definition, not this label table).
    //
    // NAMING (operator decision): the label names the RISK ("Unsafe"), not the
    // MECHANISM ("Outside This Document's Folder"), matching its sibling "Show
    // Unsafe Images". This HONOURS rather than overrides the standing "no
    // euphemism" instruction — this is a security control the user is being
    // asked to consent to, and "Unsafe" is if anything blunter than naming the
    // directory rule. Do not "restore" a mechanism-shaped label on the belief
    // that the risk-shaped one is softer; it isn't, and that reading is the
    // reason this paragraph exists.
    Cmd {
        action: "win.allow-outside-links",
        label: "Load Unsafe Linked Documents",
        accel: "",
        icon: Icon::InsertLink,
        section_start: false,
        is_toggle: true,
    },
    Cmd {
        action: "app.quit",
        label: "Exit",
        accel: "<Primary>q",
        icon: Icon::ApplicationExit,
        section_start: true,
        is_toggle: false,
    },
];

// ── VIEW_CMDS — drives View menu radio items and (Phase 3) toolbar toggles ────
// action_target maps to the "s"-typed state of win.view-mode.
// Accelerators are registered with the detailed "win.view-mode::<target>"
// syntax in setup_app so GTK displays the hint in the menu automatically.

pub(crate) struct ViewCmd {
    pub(crate) action_target: &'static str,
    pub(crate) label: &'static str,
    pub(crate) icon: Icon,
    pub(crate) accel: &'static str,
}

pub(crate) const VIEW_CMDS: [ViewCmd; 3] = [
    ViewCmd {
        action_target: "preview",
        label: "Preview",
        icon: Icon::DocumentPageSetup,
        accel: "<Alt><Shift>v",
    },
    ViewCmd {
        action_target: "edit",
        label: "Edit",
        icon: Icon::DocumentEdit,
        accel: "<Alt><Shift>e",
    },
    ViewCmd {
        action_target: "split",
        label: "Side by Side",
        icon: Icon::ViewDual,
        // NOT "<Alt><Shift>2" — a <Shift>+digit accelerator can never fire on a
        // layout where the digit's shifted glyph differs (researcher-verified
        // GDK defect, GTK4Rs/AP-51): gdk_key_event_matches uppercases
        // the spec's keyval for the exact-match check, which is a no-op for
        // digits, so the produced '@' never matches a spec'd '2'.
        accel: "<Alt>2",
    },
];

/// The six per-section toolbar groups, in canonical left-to-right order. The
/// single source of these stable strings for: the `win.show-tbtn-<id>` action
/// names (`window/viewactions.rs`), the `View ▸ Toolbar` submenu build
/// (`build_menubar` below), the section-box array order (`window/toolbar.rs`),
/// and the persisted `session::ToolbarSections` flags — kept in one place so
/// those four sites can never drift (the `FORMAT_CMDS`/`build_format_menu` typo
/// class of bug the codebase has already been bitten by). Order here IS the
/// canonical toolbar order (invariant I7); never
/// reorder without reordering the section boxes to match.
pub(crate) const TBTN_SECTION_IDS: [&str; 6] = ["file", "edit", "format", "view", "split", "zoom"];

pub(crate) const EDIT_CMDS: [Cmd; 10] = [
    Cmd {
        action: "win.undo",
        label: "Undo",
        accel: "<Primary>z",
        icon: Icon::EditUndo,
        section_start: false,
        is_toggle: false,
    },
    Cmd {
        action: "win.redo",
        label: "Redo",
        accel: "<Primary><Shift>z",
        icon: Icon::EditRedo,
        section_start: false,
        is_toggle: false,
    },
    Cmd {
        action: "win.copy",
        label: "Copy",
        accel: "<Primary>c",
        icon: Icon::EditCopy,
        section_start: true,
        is_toggle: false,
    },
    Cmd {
        action: "win.cut",
        label: "Cut",
        accel: "",
        icon: Icon::EditCut,
        section_start: false,
        is_toggle: false,
    },
    // Copies the WHOLE document's raw Markdown source (never the rendered
    // preview text) to the clipboard, regardless of the tab's current view
    // mode — distinct from win.copy, which only ever copies the current
    // *selection* (operator-requested). Bundled
    // "copy-document-symbolic" icon (data/icons) — no standard "copy whole
    // document" icon exists on any common icon theme, researcher-verified.
    Cmd {
        action: "win.copy-document",
        label: "Copy Document",
        accel: "<Primary><Meta><Alt>c",
        icon: Icon::CopyDocument,
        section_start: false,
        is_toggle: false,
    },
    // Copies the URL of the Markdown link (or image) the editor caret sits in —
    // the browser's "Copy Link Location", aimed at the source rather than at a
    // rendered anchor. Enabled only with the editor visible AND a link under the
    // caret (`window::copylink`), so it is greyed everywhere in preview-only mode
    // and everywhere the caret is in ordinary prose. No accelerator: every
    // layout-stable <Primary><Alt>+letter this would plausibly take is already
    // claimed (`l` by Insert Link, `c`/`u`/`o`/`h`/`m`/`n` by the format and
    // annotate commands), and inventing a <Shift>+punctuation one is the
    // GTK4Rs/AP-51 trap. Menu, toolbar and context menu reach it; the Keyboard
    // Shortcuts window correctly omits an unbound command.
    Cmd {
        action: "win.copy-link-location",
        label: "Copy Link Location",
        accel: "",
        icon: Icon::InsertLink,
        section_start: false,
        is_toggle: false,
    },
    Cmd {
        action: "win.delete",
        label: "Delete",
        accel: "",
        icon: Icon::EditDelete,
        section_start: false,
        is_toggle: false,
    },
    Cmd {
        action: "win.select-all",
        label: "Select All",
        accel: "<Primary>a",
        icon: Icon::EditSelectAll,
        section_start: true,
        is_toggle: false,
    },
    Cmd {
        action: "win.find",
        label: "Find",
        accel: "<Primary>f",
        icon: Icon::EditFind,
        section_start: true,
        is_toggle: false,
    },
    Cmd {
        action: "win.find-replace",
        label: "Find & Replace",
        accel: "<Primary>h",
        icon: Icon::EditFindReplace,
        section_start: false,
        is_toggle: false,
    },
];

// ── FORMAT_CMDS — drives the Format menu, toolbar section, and accelerators ────
// All six non-heading commands target the single parameterised win.format action
// (win.format::<target>); Heading 1–6 is built separately (a menu submenu and a
// toolbar combo) but shares the same action with targets h1..h6.  `icon` is tried
// first; if the theme lacks it the toolbar falls back to the short `glyph` label
// so the bar never shows a broken-image placeholder.
pub(crate) struct FmtCmd {
    pub(crate) target: &'static str,
    pub(crate) label: &'static str,
    pub(crate) icon: Option<Icon>,
    pub(crate) glyph: &'static str,
    pub(crate) accel: &'static str,
}

pub(crate) const FORMAT_CMDS: [FmtCmd; 16] = [
    FmtCmd {
        target: "bold",
        label: "Bold",
        icon: Some(Icon::FormatTextBold),
        glyph: "B",
        accel: "<Primary>b",
    },
    FmtCmd {
        target: "italic",
        label: "Italic",
        icon: Some(Icon::FormatTextItalic),
        glyph: "I",
        accel: "<Primary>i",
    },
    FmtCmd {
        target: "strike",
        label: "Strikethrough",
        icon: Some(Icon::FormatTextStrikethrough),
        glyph: "S",
        accel: "<Primary><Shift>x",
    },
    // Highlight (`==text==`, mark). No reliable freedesktop symbolic icon on the
    // common themes, so `icon` is empty and the toolbar falls back to the "H" glyph
    // (same pattern as Code Span / Quote, GTK4Rs/AP-48) — "H" keeps the
    // single-letter grouping with Bold/Italic/Strikethrough.
    FmtCmd {
        target: "highlight",
        label: "Highlight",
        icon: None,
        glyph: "H",
        // <Primary><Alt>h — a layout-stable unshifted letter ("h" for Highlight),
        // avoiding the <Shift>+letter/punctuation matcher traps (GTK4Rs/AP-51/ScrAP-51). Verified free both in ~/.config/kglobalshortcutsrc and among the
        // app's own accelerators; same <Primary><Alt> family as the other
        // non-standard-shortcut format commands.
        accel: "<Primary><Alt>h",
    },
    FmtCmd {
        target: "code-span",
        label: "Code Span",
        icon: None,
        glyph: "`",
        accel: "<Primary>grave",
    },
    FmtCmd {
        target: "sup",
        label: "Superscript",
        icon: Some(Icon::FormatTextSuperscript),
        glyph: "x²",
        accel: "<Primary><Shift>p",
    },
    FmtCmd {
        target: "sub",
        label: "Subscript",
        icon: Some(Icon::FormatTextSubscript),
        glyph: "x₂",
        accel: "<Primary><Shift>b",
    },
    FmtCmd {
        target: "code-block",
        label: "Code Block",
        icon: None,
        glyph: "```",
        // GTK4Rs/AP-51: <Primary><Shift>grave never fires
        // (Shift+` produces `~` on a US layout, and GDK's accelerator matcher
        // only case-maps letters, not punctuation). Swapped Shift for Alt on
        // the same physical key — reliable and layout-stable, per GTK4Rs/AP-51's
        // resolution option 1. No collision in ~/.config/kglobalshortcutsrc.
        accel: "<Primary><Alt>grave",
    },
    FmtCmd {
        target: "quote",
        label: "Quote",
        icon: None,
        glyph: "❝",
        // GTK4Rs/AP-51: <Primary><Shift>period never fires
        // (Shift+. produces `>`). Same Alt-for-Shift swap as Code Block.
        accel: "<Primary><Alt>period",
    },
    // List block commands. Their natural accelerators (Word/Docs use Ctrl+Shift+7/8)
    // are <Shift>+digit, which GDK can never match on a layout where the shifted
    // glyph differs (GTK4Rs/AP-51) — so, like Code Block / Quote / Horizontal
    // Bar, they take the layout-stable <Primary><Alt> family: `u` for unordered
    // (bulleted), `o` for ordered (numbered). Both confirmed free of a WM binding in
    // ~/.config/kglobalshortcutsrc. Breeze ships format-list-{un,}ordered-symbolic;
    // the glyph fallback covers themes that don't (format_button, GTK4Rs/AP-48).
    FmtCmd {
        target: "bulleted-list",
        label: "Bulleted List",
        icon: Some(Icon::FormatListUnordered),
        glyph: "•",
        accel: "<Primary><Alt>u",
    },
    FmtCmd {
        target: "numbered-list",
        label: "Numbered List",
        icon: Some(Icon::FormatListOrdered),
        glyph: "1.",
        accel: "<Primary><Alt>o",
    },
    // GFM task/checkbox list: prefixes every line with `- [ ] ` (toggles off if
    // already a task list). No standard "checklist" symbolic icon exists on the
    // common themes, so `icon` is empty and `format_button` falls back to the ☑
    // glyph (same pattern as Code Span / Quote / Horizontal Bar, GTK4Rs/AP-48).
    // Accelerator <Primary><Alt>c — "c" for Checklist (the sibling list commands use
    // a semantic letter: u=unordered, o=ordered). The more obvious Ctrl+Alt+{K,T,L}
    // are all claimed in ~/.config/kglobalshortcutsrc (layout switch / Konsole /
    // lock), and a <Shift>+digit is an GTK4Rs/AP-51 trap; `c` is an unshifted letter
    // (layout-stable) and verified free both in KDE's globals and among the app's
    // accelerators (Code Block is <Primary><Alt>grave, not c).
    FmtCmd {
        target: "task-list",
        label: "Task List",
        icon: None,
        glyph: "☑",
        accel: "<Primary><Alt>c",
    },
    FmtCmd {
        target: "hr",
        label: "Horizontal Bar",
        icon: None,
        glyph: "—",
        // GTK4Rs/AP-51: <Primary><Shift>minus never fires
        // (Shift+- produces `_`). Same Alt-for-Shift swap as Code Block.
        accel: "<Primary><Alt>minus",
    },
    // Tier-2 insertions: open a field dialog (window.rs insert_link/image/table)
    // rather than wrapping the selection inline. Same win.format::<target> plumbing.
    FmtCmd {
        target: "link",
        label: "Insert Link…",
        icon: Some(Icon::InsertLink),
        glyph: "🔗",
        accel: "<Primary>l",
    },
    FmtCmd {
        target: "image",
        label: "Insert Image…",
        icon: Some(Icon::InsertImage),
        glyph: "🖼",
        accel: "<Primary><Alt>i",
    },
    FmtCmd {
        target: "table",
        label: "Insert Table…",
        icon: Some(Icon::ViewGrid),
        glyph: "▦",
        // Moved off Ctrl+T (operator decision) so app.new (File
        // ▸ New Document) can claim it. Ctrl+Alt+T was the first candidate but
        // is already claimed by the operator's window manager (spawns a new
        // terminal); confirmed unused in FORMAT_CMDS/EDIT_CMDS.
        accel: "<Primary><Shift>t",
    },
];

// ── INLINE_ACCEL_CMDS — the SSOT for commands with NO Cmd-table row ────────────
/// A command whose accelerator is registered *inline* in
/// `setup::register_accelerators` because it has no FILE/EDIT/FORMAT/VIEW `Cmd`
/// row: tab/window navigation, zoom, the outline toggle, Go To Line, and the
/// keyboard-shortcuts-help overlay.
///
/// This table is the **single source of truth** for the FOUR surfaces that would
/// otherwise each hand-copy these accelerator strings (QA M-4):
///   1. the binding — `setup::register_accelerators` calls `set_accels_for_action`
///      from `accels`;
///   2. the menu-bar hint — `menubar` sets each item's `accel` attribute from
///      [`inline_accel`];
///   3. the shortcuts-window display row — `shortcuts::interface_xml` derives its
///      rows from this table;
///   4. the toolbar tooltip — `window::toolbar` builds tooltips via
///      [`inline_tooltip`].
///
/// Before this table those four drifted (the reviewer caught zoom-in bound to
/// `<Primary>plus`+`<Primary>equal` but displayed only `plus`); now every surface
/// reads the one list, so a change is a one-line edit here — the same
/// single-source-of-truth contract the Cmd tables already satisfy (POLICY.md).
pub(crate) struct InlineCmd {
    /// The full `win.*` action name bound via `set_accels_for_action`.
    pub(crate) action: &'static str,
    /// The shortcuts-window group heading this row appears under.
    pub(crate) group: &'static str,
    /// Human label: the shortcuts-window title AND the toolbar-tooltip base label.
    pub(crate) label: &'static str,
    /// Every accelerator bound to the action. The FIRST is canonical — the only
    /// one shown in the menu hint, shortcuts window, and tooltip; the rest are
    /// aliases (Ctrl+= for Ctrl++, Ctrl+Tab for Ctrl+PageDown, Ctrl+? for F1) that
    /// are still bound. Display and binding both derive from this one list, so
    /// they can never disagree.
    pub(crate) accels: &'static [&'static str],
}

pub(crate) const INLINE_ACCEL_CMDS: &[InlineCmd] = &[
    InlineCmd {
        action: "win.close-tab",
        group: "File",
        label: "Close Tab",
        accels: &["<Primary>w"],
    },
    // Back / Forward through the window's document-visit history (TDD §23). The
    // browser bindings, deliberately: Alt+Left/Alt+Right canonical, plus the
    // dedicated Back/Forward keys a keyboard may carry as aliases. Both are
    // layout-stable non-letter keys, so neither is an GTK4Rs/AP-51 <Shift>+glyph trap,
    // and both were verified free of the app's own accelerators and of a
    // ~/.config/kglobalshortcutsrc WM binding (KDE claims Meta+Alt+Left/Right for
    // "Switch Window Left/Right" — a different combination).
    //
    // The two mouse thumb buttons drive the same actions but cannot appear in this
    // table: it is the accelerator SSOT, and buttons 8/9 are not expressible as an
    // accelerator string. They are gestures in `window/navhistory.rs`, and their
    // absence from the Keyboard Shortcuts window is correct — that window
    // describes keys (TDD 23.6).
    InlineCmd {
        action: "win.nav-back",
        group: "View",
        label: "Back",
        accels: &["<Alt>Left", "XF86Back"],
    },
    InlineCmd {
        action: "win.nav-forward",
        group: "View",
        label: "Forward",
        accels: &["<Alt>Right", "XF86Forward"],
    },
    InlineCmd {
        action: "win.outline",
        group: "View",
        label: "Outline",
        accels: &["F9"],
    },
    InlineCmd {
        action: "win.annotations",
        group: "View",
        label: "Annotations",
        accels: &["F8"],
    },
    InlineCmd {
        action: "win.go-to-line",
        group: "View",
        label: "Go To Line",
        accels: &["<Primary>g"],
    },
    // Annotate the preview selection. Ctrl+Alt+M — the Google-Docs
    // "add comment" convention (M for coMMent), and a layout-stable <Primary><Alt>+letter
    // (an unshifted letter sidesteps the GTK4Rs/AP-51 <Shift>+digit trap). Confirmed free of a
    // FORMAT/EDIT/FILE accel and of a ~/.config/kglobalshortcutsrc WM binding. Inline
    // (not a Cmd-table row) while the menu/toolbar surfaces are a follow-up — like
    // Go To Line, it is keyboard- and (soon) menu-reachable without a toolbar button.
    InlineCmd {
        action: "win.annotate",
        group: "Edit",
        label: "Annotate",
        accels: &["<Primary><Alt>m"],
    },
    // Read the NEXT annotation's comment (the a11y read path for margin markers).
    // The margin marker is self-drawn, so it is not in the a11y tree and a pointer
    // click was the only way to reach a comment; this is the parallel keyboard path
    // D1 always owed. Ctrl+Alt+N — "next", same layout-stable <Primary><Alt>+letter
    // family as Annotate's Ctrl+Alt+M (an unshifted letter sidesteps the GTK4Rs/AP-51
    // <Shift>+punctuation trap). Verified free of every FILE/EDIT/VIEW/FORMAT accel
    // and of a ~/.config/kglobalshortcutsrc WM binding.
    InlineCmd {
        action: "win.next-annotation",
        group: "Edit",
        label: "Next Annotation",
        accels: &["<Primary><Alt>n"],
    },
    // Ctrl++ and, as an alias for keyboards where + needs Shift, Ctrl+= — both
    // bound; only Ctrl++ is displayed. (This alias is exactly what used to be
    // dropped from the shortcuts window / tooltip — QA M-4's live drift.)
    InlineCmd {
        action: "win.zoom-in",
        group: "View",
        label: "Zoom In",
        accels: &["<Primary>plus", "<Primary>equal"],
    },
    InlineCmd {
        action: "win.zoom-out",
        group: "View",
        label: "Zoom Out",
        accels: &["<Primary>minus"],
    },
    InlineCmd {
        action: "win.zoom-reset",
        group: "View",
        label: "Reset Zoom",
        accels: &["<Primary>0"],
    },
    InlineCmd {
        action: "win.new-window",
        group: "Windows & Tabs",
        label: "New Window",
        accels: &["<Primary>n"],
    },
    InlineCmd {
        action: "win.move-tab-new-window",
        group: "Windows & Tabs",
        label: "Move Tab to New Window",
        accels: &["<Primary><Shift>n"],
    },
    // Both the GNOME (Ctrl+PageUp/Down) and browser/VS Code (Ctrl+Tab /
    // Ctrl+Shift+Tab) conventions are bound as aliases; the GNOME one is canonical.
    InlineCmd {
        action: "win.previous-tab",
        group: "Windows & Tabs",
        label: "Previous Tab",
        accels: &["<Primary>Page_Up", "<Primary><Shift>Tab"],
    },
    InlineCmd {
        action: "win.next-tab",
        group: "Windows & Tabs",
        label: "Next Tab",
        accels: &["<Primary>Page_Down", "<Primary>Tab"],
    },
    // The keyboard-shortcuts-help overlay: F1 (the universal Help key, freed from
    // app.about which never needed one) plus Ctrl+? (the GtkShortcutsWindow
    // convention, kept for GNOME muscle memory). F1 is canonical.
    InlineCmd {
        action: "win.show-help-overlay",
        group: "Windows & Tabs",
        label: "Keyboard Shortcuts",
        accels: &["F1", "<Primary>question"],
    },
];

/// The inline command descriptor for `action`, or `None` when `action` has no
/// inline-registered accelerator (it's a Cmd-table command or has no shortcut).
pub(crate) fn inline_cmd(action: &str) -> Option<&'static InlineCmd> {
    INLINE_ACCEL_CMDS.iter().find(|c| c.action == action)
}

/// The canonical (first) accelerator string for an inline command, for the
/// menu-bar `accel` attribute — the SSOT the menu hint reads instead of a
/// hand-copied literal (QA M-4).
pub(crate) fn inline_accel(action: &str) -> Option<&'static str> {
    inline_cmd(action).map(|c| c.accels[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure table-lookup tests — no `gtk::init()` needed, so they run under plain
    // `cargo test` / `cargo llvm-cov` (the `commands` module is IN the coverage
    // gate). The label-DERIVING helpers wrap `accelerator_get_label`, which DOES
    // require an initialized GTK, so they're tested in `gtk_integration_tests`.

    #[test]
    fn inline_accel_resolves_canonical_accel_from_the_table() {
        // A known inline command resolves to its canonical (first) accel; a
        // Cmd-table command / unknown action does not.
        assert_eq!(inline_accel("win.zoom-in"), Some("<Primary>plus"));
        assert_eq!(inline_accel("win.show-help-overlay"), Some("F1"));
        assert_eq!(
            inline_accel("win.save"),
            None,
            "save is a Cmd-table command"
        );
        assert_eq!(inline_accel("win.nonexistent"), None);
        assert!(inline_cmd("win.zoom-in").is_some());
        assert!(inline_cmd("win.save").is_none());
    }

    #[test]
    fn inline_table_has_no_duplicate_actions() {
        // Each action appears once, so `inline_cmd`'s first-match lookup is
        // unambiguous and `register_accelerators` binds each exactly once.
        for (i, a) in INLINE_ACCEL_CMDS.iter().enumerate() {
            for b in &INLINE_ACCEL_CMDS[i + 1..] {
                assert_ne!(a.action, b.action, "duplicate inline action {:?}", a.action);
            }
        }
    }
}

/// GTK-object tests for the accelerator-LABEL helpers: `accelerator_get_label`
/// requires an initialized GTK, so — like `shortcuts.rs` — these use `#[gtktest::test]`
/// behind the `gtk-integration-tests` feature (QA M-2; a typo in the label
/// derivation would otherwise surface only in live UI).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    // GTK4's Quartz accelerator label spells the Primary modifier with the
    // Control glyph U+2303 (⌃), never the word "Ctrl" and never a Command glyph
    // — GTK4 dropped GTK3's Quartz `<Primary>`→⌘ map (GTK4Rs/AP-160;
    // measured here on GTK 4.22.4/macOS, 2026-07-30: `accel_hint("<Primary>o")`
    // == `"⌃O"`). Every other platform this tree targets renders the word
    // "Ctrl". Per ScrAP-181, pin the expected substring as a `#[cfg]`'d constant
    // rather than deriving it from the function under test — deriving it that
    // way would just make the test agree with itself.
    #[cfg(target_os = "macos")]
    const PRIMARY_MOD_LABEL: &str = "\u{2303}"; // ⌃ CONTROL — not "Ctrl", not ⌘
    #[cfg(not(target_os = "macos"))]
    const PRIMARY_MOD_LABEL: &str = "Ctrl";

    #[gtktest::test]
    fn accel_hint_none_for_empty_and_platform_labelled_otherwise() {
        assert_eq!(accel_hint(""), None, "empty accel has no hint");
        let hint = accel_hint("<Primary>o").expect("Ctrl+O has a hint");
        // Assert the platform prefix + key without pinning the exact glyph
        // spelling GTK chooses beyond the per-platform constant above (ScrAP-181).
        assert!(
            hint.contains(PRIMARY_MOD_LABEL),
            "hint {hint:?} should name the platform modifier ({PRIMARY_MOD_LABEL:?})"
        );
        assert!(hint.contains('O'), "hint {hint:?} should name the O key");
    }

    #[gtktest::test]
    fn tooltip_with_accel_appends_hint_or_bare_label() {
        assert_eq!(
            tooltip_with_accel("Open", ""),
            "Open",
            "no accel → bare label, no parentheses"
        );
        let tip = tooltip_with_accel("Open", "<Primary>o");
        assert!(
            tip.starts_with("Open ("),
            "tooltip {tip:?} keeps the label first"
        );
        assert!(tip.ends_with(')'), "tooltip {tip:?} closes the parenthesis");
        assert!(
            tip.contains(PRIMARY_MOD_LABEL),
            "tooltip {tip:?} embeds the accel hint"
        );
    }
}
