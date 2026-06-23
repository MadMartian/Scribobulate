//! Compile-checked icon names (ScrAP-39).
//!
//! Icon names passed to `Button::from_icon_name` / `Widget::set_icon_name` /
//! `Image::from_icon_name` are stringly-typed: a typo, or a name absent from the
//! active icon theme and the bundled fallback set, silently renders the
//! broken-image placeholder with **no** compile error. This enum centralises the
//! icon-name literals the app hands to those calls, so a typo becomes a compile
//! error and the whole set is enumerable and testable (see the resolution test
//! at the bottom of this file).
//!
//! Scope: this covers BOTH the direct-literal call sites AND the icon names
//! carried as data in the command-descriptor tables (`crate::app::commands` —
//! `Cmd`, `ViewCmd`, `FmtCmd`). `Cmd`/`ViewCmd` carry an [`Icon`]; `FmtCmd` carries
//! an `Option<Icon>` (`None` = "no icon, fall back to the glyph" — the old
//! empty-string sentinel). Every variant has a real call site, so the whole set is
//! enumerated by [`Icon::every`] — derived from an exhaustive `match`, so a new
//! variant cannot be added without joining it — and resolution-checked in one place.

/// A compile-checked icon name. Call [`Icon::name`] to get the freedesktop icon
/// name string to hand to a GTK icon-consuming constructor/setter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Icon {
    /// The application's own icon, named by application ID (GTK resolves a
    /// window's icon and the About dialog's logo by app ID, not by filename).
    /// This literal is the source of truth: `crate::APP_ID` is *defined as*
    /// `Icon::App.name()`, so this arm must not refer back to it.
    /// Bundled in the GResource: before that it resolved ONLY where
    /// `install.sh` had dropped it into `hicolor`, so any uninstalled run — every
    /// `cargo run`, and any packaging path that does not install into an icon
    /// theme — put the broken-image placeholder in the About dialog. Note this
    /// masks itself on a developer box: once `install.sh` has run the icon
    /// resolves from hicolor forever after. The same gap is *unmaskable* on
    /// **Windows**, which has no `install.sh` step at all (the staged tree ships
    /// only gvsbuild's stock Adwaita + hicolor), so the title bar, taskbar and
    /// Alt+Tab all fell back to GTK's generic default until this was bundled.
    /// Not `-symbolic`: it keeps its own palette instead of being recoloured to
    /// the theme foreground.
    App,
    /// GTK's built-in broken-image placeholder (not a `-symbolic` name); always
    /// present on every GTK4 install. Used for a failed inline image.
    ImageMissing,
    /// Tab-strip "scroll left" chevron.
    GoPrevious,
    /// Tab-strip "scroll right" chevron.
    GoNext,
    /// Close affordance (tab close ×, sidebar-pane close, find-bar close).
    WindowClose,
    /// Outline "expand all" header button (bundled fallback for Adwaita).
    ExpandAll,
    /// Outline "collapse all" header button (bundled fallback for Adwaita).
    CollapseAll,
    /// Find-bar "previous match".
    GoUp,
    /// Find-bar "next match".
    GoDown,
    /// Conflict toast warning glyph.
    DialogWarning,
    /// Info toast: reload / generic-refresh notice. Also File ▸ Reload.
    ViewRefresh,
    /// Info toast: successful-save notice. Also File ▸ Save.
    DocumentSave,
    /// Toolbar "move tab to new window".
    SendTo,
    /// Toolbar outline-sidebar toggle.
    ViewList,
    /// Toolbar annotations-viewer toggle.
    MailMarkImportant,
    /// Toolbar "go to line".
    GoJump,
    /// Toolbar "show unsafe images" toggle. `emblem-photos-symbolic`, not
    /// `image-x-generic-symbolic`: the latter is a *mimetypes*-category icon
    /// absent from Breeze/Breeze-dark (the operator's theme), whose fallback
    /// chain never reaches Adwaita's copy, so it rendered the broken-image
    /// placeholder there (ScrAP-39 / GTK4Rs/AP-48). `emblem-photos-symbolic` is present
    /// in breeze, breeze-dark, and the Adwaita installed on Linux — but NOT in
    /// gvsbuild's Adwaita 50.0, so it showed the placeholder on Windows until it
    /// was bundled. Now resolves from the GResource on every platform; the swap
    /// above is why a *third* name was not tried instead — see
    /// `data/resources.gresource.xml`.
    EmblemPhotos,
    /// Toolbar split "swap panes" toggle.
    ObjectFlipHorizontal,
    /// Toolbar split "vertical split" toggle.
    ObjectFlipVertical,
    /// Toolbar zoom-in.
    ZoomIn,
    /// Toolbar zoom-reset (original size).
    ZoomOriginal,
    /// Toolbar zoom-out.
    ZoomOut,

    // ── command-descriptor-table icons (crate::app::commands) ─────────────────
    // Toolbar/menu buttons built from FILE_CMDS / EDIT_CMDS / VIEW_CMDS /
    // FORMAT_CMDS. All SYMBOLIC on purpose (ScrAP-169: a symbolic icon recolours to
    // the theme foreground, so it stays visible on a dark variant). The nine
    // former full-colour names — document-new/open/save/save-as, edit-copy/cut/
    // delete/select-all, application-exit — were swapped to their `-symbolic`
    // variants (Wave 13); all verified present in breeze, breeze-dark, and Adwaita.
    /// File ▸ New Document.
    DocumentNew,
    /// File ▸ Open.
    DocumentOpen,
    /// File ▸ Save As.
    DocumentSaveAs,
    /// File ▸ Auto-Reload toggle. Bundled fallback for the same reason as
    /// [`Icon::EmblemPhotos`] — absent from gvsbuild's Adwaita 50.0.
    EmblemSynchronizing,
    /// File ▸ Load Unsafe Linked Documents toggle; also Format ▸ Insert Link.
    InsertLink,
    /// File/Edit ▸ Exit (menu only — Exit is not on the toolbar).
    ApplicationExit,
    /// View ▸ Preview toggle.
    DocumentPageSetup,
    /// View ▸ Edit toggle.
    DocumentEdit,
    /// View ▸ Side by Side toggle.
    ViewDual,
    /// Edit ▸ Undo.
    EditUndo,
    /// Edit ▸ Redo.
    EditRedo,
    /// Edit ▸ Copy; also File ▸ Copy Full Path.
    EditCopy,
    /// Edit ▸ Cut.
    EditCut,
    /// Edit ▸ Copy Document (bundled fallback — no standard whole-document-copy
    /// icon exists on the common themes; ships in `data/icons`).
    CopyDocument,
    /// Edit ▸ Delete.
    EditDelete,
    /// Edit ▸ Select All.
    EditSelectAll,
    /// Edit ▸ Find.
    EditFind,
    /// Edit ▸ Find & Replace.
    EditFindReplace,
    /// Format ▸ Bold.
    FormatTextBold,
    /// Format ▸ Italic.
    FormatTextItalic,
    /// Format ▸ Strikethrough.
    FormatTextStrikethrough,
    /// Format ▸ Superscript.
    FormatTextSuperscript,
    /// Format ▸ Subscript.
    FormatTextSubscript,
    /// Format ▸ Bulleted List.
    FormatListUnordered,
    /// Format ▸ Numbered List.
    FormatListOrdered,
    /// Format ▸ Insert Image.
    InsertImage,
    /// Format ▸ Insert Table.
    ViewGrid,
}

/// The application ID — also the app's icon name, which is why it lives here
/// beside the literal rather than at a crate root. The two roles genuinely are one
/// string: GTK resolves a window's icon and the About dialog's logo by app ID.
///
/// **Defined once, in the module both crate roots already share.** It used to be
/// declared separately in `src/lib.rs` AND `src/gtk_suite.rs` — the second crate
/// root the main-thread GTK suite needs — where the suite's copy was dead code
/// masked by that root's `#![allow(dead_code)]`, so nothing would have reported the
/// two drifting apart. The cross-reference gate's check 4 compares the roots' module
/// lists and does not see constants, so it could not have caught it either. Both
/// roots now re-export this one definition.
///
/// `allow(dead_code)` is narrow and load-bearing, exactly as `suite_registry::Case`'s
/// is: `tests/icon_resolution.rs` pulls this file in with `#[path]` to check icon-name
/// resolution and has no application to identify, so the constant is genuinely unread
/// in that one compilation. The alternative — leaving the definition in the crate
/// roots so that target never sees it — is the duplication this moved here to remove.
#[allow(dead_code)]
pub(crate) const APP_ID: &str = Icon::App.name();

impl Icon {
    /// The freedesktop icon name for this icon.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Icon::App => "com.extollit.scribobulate",
            Icon::ImageMissing => "image-missing",
            Icon::GoPrevious => "go-previous-symbolic",
            Icon::GoNext => "go-next-symbolic",
            Icon::WindowClose => "window-close-symbolic",
            Icon::ExpandAll => "expand-all-symbolic",
            Icon::CollapseAll => "collapse-all-symbolic",
            Icon::GoUp => "go-up-symbolic",
            Icon::GoDown => "go-down-symbolic",
            Icon::DialogWarning => "dialog-warning-symbolic",
            Icon::ViewRefresh => "view-refresh-symbolic",
            Icon::DocumentSave => "document-save-symbolic",
            Icon::SendTo => "send-to-symbolic",
            Icon::ViewList => "view-list-symbolic",
            Icon::MailMarkImportant => "mail-mark-important-symbolic",
            Icon::GoJump => "go-jump-symbolic",
            Icon::EmblemPhotos => "emblem-photos-symbolic",
            Icon::ObjectFlipHorizontal => "object-flip-horizontal-symbolic",
            Icon::ObjectFlipVertical => "object-flip-vertical-symbolic",
            Icon::ZoomIn => "zoom-in-symbolic",
            Icon::ZoomOriginal => "zoom-original-symbolic",
            Icon::ZoomOut => "zoom-out-symbolic",
            Icon::DocumentNew => "document-new-symbolic",
            Icon::DocumentOpen => "document-open-symbolic",
            Icon::DocumentSaveAs => "document-save-as-symbolic",
            Icon::EmblemSynchronizing => "emblem-synchronizing-symbolic",
            Icon::InsertLink => "insert-link-symbolic",
            Icon::ApplicationExit => "application-exit-symbolic",
            Icon::DocumentPageSetup => "document-page-setup-symbolic",
            Icon::DocumentEdit => "document-edit-symbolic",
            Icon::ViewDual => "view-dual-symbolic",
            Icon::EditUndo => "edit-undo-symbolic",
            Icon::EditRedo => "edit-redo-symbolic",
            Icon::EditCopy => "edit-copy-symbolic",
            Icon::EditCut => "edit-cut-symbolic",
            Icon::CopyDocument => "copy-document-symbolic",
            Icon::EditDelete => "edit-delete-symbolic",
            Icon::EditSelectAll => "edit-select-all-symbolic",
            Icon::EditFind => "edit-find-symbolic",
            Icon::EditFindReplace => "edit-find-replace-symbolic",
            Icon::FormatTextBold => "format-text-bold-symbolic",
            Icon::FormatTextItalic => "format-text-italic-symbolic",
            Icon::FormatTextStrikethrough => "format-text-strikethrough-symbolic",
            Icon::FormatTextSuperscript => "format-text-superscript-symbolic",
            Icon::FormatTextSubscript => "format-text-subscript-symbolic",
            Icon::FormatListUnordered => "format-list-unordered-symbolic",
            Icon::FormatListOrdered => "format-list-ordered-symbolic",
            Icon::InsertImage => "insert-image-symbolic",
            Icon::ViewGrid => "view-grid-symbolic",
        }
    }
}

/// What happens when an icon name fails to resolve — the contract the resolution audit
/// holds each name to.
///
/// `allow(dead_code)` for the same narrow reason [`APP_ID`] carries it: the app binary
/// never reads this — `tests/icon_resolution.rs` does, pulling this file in with
/// `#[path]`, and the completeness gate below reads it under `cfg(test)`. Compiled
/// unconditionally so that target checks the real partition rather than a second copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum Resolution {
    /// A miss renders the broken-image placeholder (ScrAP-39). The audit must fail.
    MustResolve,
    /// `format_button` gives this one a short text glyph (B, I, •, 1., …), so a miss
    /// degrades rather than breaking. The audit reports it and carries on.
    GlyphFallback,
}

#[allow(dead_code)] // read by tests/icon_resolution.rs (see `Resolution`)
impl Icon {
    /// The first icon in the enumeration order — the head of [`Icon::next`]'s chain.
    pub(crate) const FIRST: Icon = Icon::App;

    /// The next icon after `self`, or `None` at the end of the chain.
    ///
    /// **This exists so the audit ranges over the enum instead of over a list somebody
    /// maintains** (QA round 5, M-5; ScrAP-216). It replaces the `ALL` and
    /// `GLYPH_FALLBACK` arrays, which were two hand-written partitions of the variant
    /// set with nothing tying them to it: adding a 50th variant and forgetting to list
    /// it left `icon_resolution` reporting *"audited 49 names"* and exiting 0 — an audit
    /// that silently stops covering the thing it was added to cover. MEASURED: the two
    /// arrays did in fact still cover all 49 variants, so this closes an enforcement
    /// gap rather than a live miss, which is the moment to close it.
    ///
    /// Rust has no reflection over variants, so completeness has to be manufactured.
    /// A `match` is exhaustive, so writing an arm is compulsory; chaining each variant
    /// to the next makes the enumeration *derived from* those compulsory arms rather
    /// than from a separate list. A new variant cannot compile without an arm, and
    /// [`tests::every_variant_is_reachable_and_audited`] catches the one remaining
    /// evasion — an arm that returns `None` and orphans the tail.
    pub(crate) const fn next(self) -> Option<Icon> {
        match self {
            Icon::App => Some(Icon::ImageMissing),
            Icon::ImageMissing => Some(Icon::GoPrevious),
            Icon::GoPrevious => Some(Icon::GoNext),
            Icon::GoNext => Some(Icon::WindowClose),
            Icon::WindowClose => Some(Icon::ExpandAll),
            Icon::ExpandAll => Some(Icon::CollapseAll),
            Icon::CollapseAll => Some(Icon::GoUp),
            Icon::GoUp => Some(Icon::GoDown),
            Icon::GoDown => Some(Icon::DialogWarning),
            Icon::DialogWarning => Some(Icon::ViewRefresh),
            Icon::ViewRefresh => Some(Icon::DocumentSave),
            Icon::DocumentSave => Some(Icon::SendTo),
            Icon::SendTo => Some(Icon::ViewList),
            Icon::ViewList => Some(Icon::MailMarkImportant),
            Icon::MailMarkImportant => Some(Icon::GoJump),
            Icon::GoJump => Some(Icon::EmblemPhotos),
            Icon::EmblemPhotos => Some(Icon::ObjectFlipHorizontal),
            Icon::ObjectFlipHorizontal => Some(Icon::ObjectFlipVertical),
            Icon::ObjectFlipVertical => Some(Icon::ZoomIn),
            Icon::ZoomIn => Some(Icon::ZoomOriginal),
            Icon::ZoomOriginal => Some(Icon::ZoomOut),
            Icon::ZoomOut => Some(Icon::DocumentNew),
            Icon::DocumentNew => Some(Icon::DocumentOpen),
            Icon::DocumentOpen => Some(Icon::DocumentSaveAs),
            Icon::DocumentSaveAs => Some(Icon::EmblemSynchronizing),
            Icon::EmblemSynchronizing => Some(Icon::InsertLink),
            Icon::InsertLink => Some(Icon::ApplicationExit),
            Icon::ApplicationExit => Some(Icon::DocumentPageSetup),
            Icon::DocumentPageSetup => Some(Icon::DocumentEdit),
            Icon::DocumentEdit => Some(Icon::ViewDual),
            Icon::ViewDual => Some(Icon::EditUndo),
            Icon::EditUndo => Some(Icon::EditRedo),
            Icon::EditRedo => Some(Icon::EditCopy),
            Icon::EditCopy => Some(Icon::EditCut),
            Icon::EditCut => Some(Icon::CopyDocument),
            Icon::CopyDocument => Some(Icon::EditDelete),
            Icon::EditDelete => Some(Icon::EditSelectAll),
            Icon::EditSelectAll => Some(Icon::EditFind),
            Icon::EditFind => Some(Icon::EditFindReplace),
            Icon::EditFindReplace => Some(Icon::FormatTextBold),
            Icon::FormatTextBold => Some(Icon::FormatTextItalic),
            Icon::FormatTextItalic => Some(Icon::FormatTextStrikethrough),
            Icon::FormatTextStrikethrough => Some(Icon::FormatTextSuperscript),
            Icon::FormatTextSuperscript => Some(Icon::FormatTextSubscript),
            Icon::FormatTextSubscript => Some(Icon::FormatListUnordered),
            Icon::FormatListUnordered => Some(Icon::FormatListOrdered),
            Icon::FormatListOrdered => Some(Icon::InsertImage),
            Icon::InsertImage => Some(Icon::ViewGrid),
            Icon::ViewGrid => None,
        }
    }

    /// Every icon, in declaration order. Derived from [`Icon::next`], so it cannot
    /// drift from the enum.
    pub(crate) fn every() -> impl Iterator<Item = Icon> {
        std::iter::successors(Some(Icon::FIRST), |icon| icon.next())
    }

    /// Which resolution contract this icon has.
    ///
    /// Exhaustive on purpose (same reasoning as [`Icon::next`]): a new variant must be
    /// classified, and classifying it is a decision with a visible justification rather
    /// than an omission from an array nobody re-reads. This replaces the `ALL` /
    /// `GLYPH_FALLBACK` split; the reasoning behind that split is preserved here,
    /// because it is the reasoning a new variant has to be classified against.
    ///
    /// **`MustResolve`** — every variant used at a call site with **no** fallback (a
    /// bare `from_icon_name` / `set_icon_name`: the direct-literal sites plus the
    /// `FILE`/`EDIT`/`VIEW` toolbar `Cmd`/`ViewCmd` buttons). A miss renders the
    /// broken-image placeholder (ScrAP-39).
    ///
    /// [`Icon::App`] is in this class for the same reason but with a quieter symptom:
    /// the window's `icon_name` has no fallback either, and a miss there hands the
    /// title bar, taskbar and Alt+Tab to GTK's generic default rather than drawing a
    /// placeholder — which is exactly why it went unnoticed on Windows until someone
    /// looked at the title bar. It resolves from the bundled GResource, so the
    /// assertion holds on every platform and without `install.sh`.
    ///
    /// `InsertLink` is `MustResolve` despite being a format icon, because it is ALSO
    /// used without a fallback (the File ▸ Load Unsafe Linked Documents toolbar
    /// toggle).
    ///
    /// **`GlyphFallback`** — the nine `FmtCmd`-only icons (Format toolbar:
    /// bold/italic/strike/super/sub, the two list icons, insert-image, insert-table).
    /// `format_button` verifies each with its own `has_icon` check and falls back to a
    /// short glyph, so a non-resolving format icon degrades gracefully instead of
    /// showing a placeholder. Four of them (super/subscript, the two lists)
    /// legitimately do NOT resolve on Adwaita/hicolor — they ship in Breeze — which is
    /// exactly why they carry a glyph fallback.
    pub(crate) const fn resolution(self) -> Resolution {
        match self {
            Icon::App
            | Icon::ImageMissing
            | Icon::GoPrevious
            | Icon::GoNext
            | Icon::WindowClose
            | Icon::ExpandAll
            | Icon::CollapseAll
            | Icon::GoUp
            | Icon::GoDown
            | Icon::DialogWarning
            | Icon::ViewRefresh
            | Icon::DocumentSave
            | Icon::SendTo
            | Icon::ViewList
            | Icon::MailMarkImportant
            | Icon::GoJump
            | Icon::EmblemPhotos
            | Icon::ObjectFlipHorizontal
            | Icon::ObjectFlipVertical
            | Icon::ZoomIn
            | Icon::ZoomOriginal
            | Icon::ZoomOut
            | Icon::DocumentNew
            | Icon::DocumentOpen
            | Icon::DocumentSaveAs
            | Icon::EmblemSynchronizing
            | Icon::InsertLink
            | Icon::ApplicationExit
            | Icon::DocumentPageSetup
            | Icon::DocumentEdit
            | Icon::ViewDual
            | Icon::EditUndo
            | Icon::EditRedo
            | Icon::EditCopy
            | Icon::EditCut
            | Icon::CopyDocument
            | Icon::EditDelete
            | Icon::EditSelectAll
            | Icon::EditFind
            | Icon::EditFindReplace => Resolution::MustResolve,
            Icon::FormatTextBold
            | Icon::FormatTextItalic
            | Icon::FormatTextStrikethrough
            | Icon::FormatTextSuperscript
            | Icon::FormatTextSubscript
            | Icon::FormatListUnordered
            | Icon::FormatListOrdered
            | Icon::InsertImage
            | Icon::ViewGrid => Resolution::GlyphFallback,
        }
    }
}

// RESOLUTION for every `Icon` name (ScrAP-39) is gated by the
// `tests/icon_resolution.rs` integration target, not by a test body here. It walks
// `Icon::every()` and classifies with `Icon::resolution`, which is why both are
// compiled unconditionally rather than gated to `cfg(test)`.
//
// It stays its OWN `harness = false` target — rather than joining the main-thread
// suite in `src/gtk_suite.rs`, which could now reach these tables directly — because
// its assertion is *about* process-global state. It registers a GResource and an icon
// theme search path and then asks whether every name resolves; in a process shared
// with 147 other bodies that answer becomes order-dependent, and can go wrong in the
// dangerous direction: a name resolving because a *sibling* body put something on the
// search path, while the shipped app misses it and renders the broken-image
// placeholder. GTK cannot be un-initialised, so the isolation has to come from having
// its own process. Its `--render <DIR>` evidence mode also needs its own argv, which a
// shared runner owns. See POLICY's testing section.

// NOTE the imports live inside each body rather than at module scope. This file is
// compiled TWICE — once as `crate::icons`, and once more as a `#[path]` module of the
// `tests/icon_resolution.rs` target — and a module-scope `use super::*` reads as unused
// in the second compilation, which is a hard error under the pipeline's `-D warnings`.
// Same dual-compilation quirk `APP_ID`'s narrow `allow(dead_code)` exists for.
#[cfg(test)]
mod tests {

    /// Every variant is reachable from [`Icon::FIRST`], so the audit really does range
    /// over the enum (QA round 5, M-5).
    ///
    /// **What the compiler already guarantees, and the one hole it leaves.** `name`,
    /// `next` and `resolution` are exhaustive matches, so a new variant cannot compile
    /// without an arm in each — that is what makes this enumeration enforced rather
    /// than remembered. The evasion left is `Icon::NewOne => None`: a legal arm that
    /// terminates the chain early and orphans every variant after it, silently
    /// shrinking the audited set exactly as a forgotten array entry used to.
    ///
    /// So the count is checked against a source that cannot lie about it: the arm count
    /// of `name`, which the compiler proves is the whole variant set. Reading this
    /// file's own text is the same tactic `forensics::signal.rs` uses, and it inherits
    /// that module's lesson about boundaries — the region is delimited by items the
    /// language guarantees are unique (`const fn name`, and the first `\n    }` closing
    /// it), and a failure to find them is an assertion rather than a silent fallback.
    #[test]
    fn every_variant_is_reachable_and_audited() {
        use super::Icon;
        let source = include_str!("icons.rs");
        let start = source
            .find("pub(crate) const fn name(self) -> &'static str {")
            .expect("icons.rs must define `name` — the exhaustive match this counts");
        let body = &source[start..];
        let end = body.find("\n    }").expect("`name`'s match must close");
        let arms = body[..end].matches("Icon::").count();

        let reachable: Vec<Icon> = Icon::every().collect();
        assert_eq!(
            reachable.len(),
            arms,
            "{} variants are reachable from Icon::FIRST but `name` has {arms} arms — a \
             variant exists that `next` does not chain to, so the resolution audit \
             would report success while never looking at it. Link it into the chain.",
            reachable.len()
        );

        // Names are distinct, so a mis-chained variant cannot hide behind a duplicate.
        let mut names: Vec<&str> = reachable.iter().map(|i| i.name()).collect();
        names.sort_unstable();
        let distinct = names.len();
        names.dedup();
        assert_eq!(names.len(), distinct, "two variants share an icon name");
    }

    /// The partition is total and the classes are non-empty — a `resolution` that
    /// answered `GlyphFallback` for everything would make the audit unfailable.
    #[test]
    fn the_resolution_partition_covers_both_classes() {
        use super::{Icon, Resolution};
        let must = Icon::every()
            .filter(|i| i.resolution() == Resolution::MustResolve)
            .count();
        let glyph = Icon::every()
            .filter(|i| i.resolution() == Resolution::GlyphFallback)
            .count();
        assert_eq!(must + glyph, Icon::every().count());
        assert!(
            must > 0 && glyph > 0,
            "{must} must-resolve, {glyph} fallback"
        );
        assert_eq!(
            Icon::App.resolution(),
            Resolution::MustResolve,
            "the window/About icon has no fallback either — see `resolution`"
        );
        assert_eq!(
            Icon::FormatTextBold.resolution(),
            Resolution::GlyphFallback,
            "format-bar icons degrade to a glyph rather than a placeholder"
        );
    }
}
