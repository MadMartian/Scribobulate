//! Exporting a document to a presentation format — HTML to share, PDF to record
//! (TDD §25).
//!
//! # An export is a function of the document SOURCE, not of the preview widget
//!
//! Three independent reasons, any one of them sufficient:
//!
//! 1. **A tab may have no preview.** Deferred tabs build one on first activation and
//!    edit-only mode never builds one at all, so a widget-reading exporter would
//!    silently depend on what the reader happened to do beforehand (TDD 25.2).
//! 2. **The widget tree is only correct where it is visible.** Off-screen anchored
//!    children are parked at negative coordinates and painted that way
//!    (GTK4Rs/AP-166); line heights validate lazily with no completion signal
//!    (ScrAP-260); a geometry read taken before allocation answers the buffer's
//!    *last* line, silently (GTK4Rs/AP-263).
//! 3. **Only a display-free pipeline is inside the coverage gate** (POLICY § Build
//!    pipeline step 6). The suite runs without a display, so a widget-reading
//!    exporter would sit permanently outside the coverage number with its
//!    correctness resting on a human opening the file.
//!
//! The precedent is [`crate::copymap`], a pure second consumer of the same event
//! stream for exactly these reasons (GTK4Rs/AP-5).
//!
//! # One pipeline, two sinks
//!
//! [`doc`] enters through the conditions the preview enters through — CriticMarkup
//! extraction, [`crate::renderer::md_options`], `normalize_inline_tabs`, the
//! pulldown event stream, plus `scan_script_spans` for the four constructs a second
//! tokeniser owns — and builds an [`ExportDoc`]. **Both sinks consume that one
//! model**, so they agree with the preview, and with each other, by construction
//! rather than by vigilance. Every decision about *what a construct is* is made once,
//! upstream of all three consumers.
//!
//! # What an export is allowed to contain
//!
//! A Markdown document is untrusted content (TDD 2.7), and an export **removes the
//! containment the screen provides**, because the artefact is opened by software this
//! project neither controls nor sandboxes. So the obligation is stricter here, never
//! looser:
//!
//! * Raw HTML is **dropped**, exactly as the preview drops it — not escaped (which
//!   would put text on the page the preview never showed) and certainly not passed
//!   through. The permitted set is [`crate::renderer::RENDERED_HTML_ELEMENTS`], read
//!   from its one owner rather than reproduced here (TDD 25.4).
//! * A link's URL passes [`crate::links::is_allowed_url`]'s scheme allowlist.
//! * A local image passes the containment gate before its bytes are embedded; a
//!   remote one is referenced and not fetched unless the document's own "Show Unsafe
//!   Images" consent is on (TDD 25.12).
//!
//! # File layout
//!
//! * [`doc`] — display-free: event stream + annotations → [`ExportDoc`].
//! * [`html`] — display-free: [`ExportDoc`] + resolved style → `String`.
//! * `paginate` — display-free: [`ExportDoc`] + page metrics → pages.
//! * `pdf` — the only GTK-touching file, a thin adapter with no logic of its own.
//!   If it grows past that, logic has leaked into it.

use pulldown_cmark::HeadingLevel;
use std::path::Path;

pub(crate) mod doc;
pub(crate) mod html;
pub(crate) mod markup;
pub(crate) mod paginate;
pub(crate) mod pdf;
pub(crate) mod walk;

/// What the builder needs to know about the document beyond its text: where it sits
/// (for resolving relative image and link references) and whether its reader has
/// consented to unsafe images.
///
/// `allow_unsafe_images` is **this document's** consent, per tab (TDD §14). It is
/// never a global preference and never inherited from another tab, so it is passed
/// in rather than read from anywhere.
#[derive(Debug, Clone, Default)]
pub(crate) struct RenderOptions {
    pub(crate) doc_dir: Option<std::path::PathBuf>,
    pub(crate) allow_unsafe_images: bool,
}

/// Which sink an export is headed for. The `win.export` action's `s` parameter
/// parses to this once, at the action boundary, rather than passing a string down
/// (POLICY § Code style — a closed set of values gets an enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExportTarget {
    Html,
    Pdf,
}

impl ExportTarget {
    /// The action target string this sink is named by, in SCHEMA.md's `win.export`
    /// row and in the menu model.
    pub(crate) const fn target(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
        }
    }

    /// Parse an action parameter. `None` for anything else, so an unknown target is
    /// refused at the boundary rather than defaulting to a sink the reader did not
    /// ask for.
    pub(crate) fn from_target(s: &str) -> Option<Self> {
        match s {
            "html" => Some(Self::Html),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    /// The filename extension an exported artefact carries, **without** the dot.
    pub(crate) const fn extension(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
        }
    }

    /// The menu label for this sink.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Html => "HTML",
            Self::Pdf => "PDF",
        }
    }
}

/// The name the destination chooser is seeded with: the document's **stem** plus the
/// target's extension.
///
/// Pure, so the guarantee above is a unit test on every platform rather than a
/// property observed on one. An untitled document falls back to a fixed stem — a
/// buffer with no path still exports (TDD 25.5), so it still needs a name.
pub(crate) fn default_export_name(path: Option<&Path>, target: ExportTarget) -> String {
    const UNTITLED_STEM: &str = "untitled";
    let stem = path
        .and_then(Path::file_stem)
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| UNTITLED_STEM.to_string());
    // Validate the name we PROPOSE. A document whose stem is not a legal filename on
    // this platform — a reserved device name, say — would otherwise have us seed the
    // dialog with something the platform then rewrites or refuses.
    let stem = if crate::docio::rename::is_valid_filename(&stem) {
        stem
    } else {
        UNTITLED_STEM.to_string()
    };
    format!("{stem}.{}", target.extension())
}

/// A resolved image, ready for a sink to place.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ImageSource {
    /// Local bytes that passed the containment gate, to be embedded.
    Embedded {
        /// The image file's bytes.
        bytes: Vec<u8>,
        /// The IANA media type inferred from the bytes' magic number — never from
        /// the filename extension, which an untrusted document chooses.
        mime: &'static str,
    },
    /// A remote URL, referenced rather than fetched. HTML can point at it; a PDF
    /// cannot, which is the gap TDD 25.12 requires be communicated.
    Remote(String),
    /// Nothing renderable — refused by the containment gate, absent, or undecodable.
    /// Carries the same reason string the preview's placeholder tooltip shows, so the
    /// artefact says why rather than showing an unexplained gap.
    Missing(String),
}

/// An image as the document referred to it, plus what it resolved to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImageRef {
    pub(crate) alt: String,
    pub(crate) title: Option<String>,
    pub(crate) source: ImageSource,
}

/// A column alignment from a GFM table's delimiter row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    None,
    Left,
    Center,
    Right,
}

/// One inline run. The tree mirrors what the preview shows, not what the source
/// said — the four constructs `scan_script_spans` owns
/// (`^sup^`, `~sub~`, `~~strike~~`, `==highlight==`) appear here as their own
/// variants even though pulldown-cmark delivered them as plain `Text`, which is
/// precisely the blindness that makes a second parse the wrong answer (ScrAP-66,
/// ScrAP-195).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Inline {
    /// A literal text run, with the half-open **rendered char** range it occupies.
    /// The range is what lets an annotation's claim be placed over exactly the
    /// characters it covers even where the rendered text is shorter than its source
    /// (Document Rendering CAM row 3's extent obligation).
    Text {
        text: String,
        span: (i32, i32),
    },
    Code(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Superscript(Vec<Inline>),
    Subscript(Vec<Inline>),
    /// `==marked==`.
    Highlight(Vec<Inline>),
    Link {
        href: String,
        title: Option<String>,
        inner: Vec<Inline>,
    },
    Image(ImageRef),
    /// A soft or hard line break.
    Break,
    /// An annotated claim — the index into [`ExportDoc::annotations`] plus the run it
    /// covers. Produced by a post-pass over the built tree, never by the walk, so the
    /// claim's extent is decided by the one mapper the preview uses.
    Claim(usize, Vec<Inline>),
}

/// One list item. `task` is `Some(checked)` for a task-list item.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListItem {
    pub(crate) task: Option<bool>,
    pub(crate) blocks: Vec<Block>,
}

/// One block-level construct.
// The `CodeBlock`/`BlockQuote` names echo the enum's own, which clippy flags as a
// naming smell. They are kept: these are the constructs' names in CommonMark, in
// pulldown-cmark's `Tag`, and in HTML, and renaming them to satisfy a lint would make
// every mapping between the three read as a translation rather than an identity.
#[allow(clippy::enum_variant_names)] // CommonMark's own names; see above
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Block {
    Heading {
        /// 1–6 as written. The preview folds h6 into h5's *tag*, but the export keeps
        /// the level the document stated: an HTML `<h6>` is a real element and a PDF
        /// scale is a number, so neither has the preview's five-tag constraint.
        level: u8,
        /// The heading's anchor slug, from [`crate::links::slugify`] and made unique
        /// the same way the preview makes it — so an in-document link that works on
        /// screen works in the artefact (TDD 25.1).
        id: String,
        inlines: Vec<Inline>,
    },
    Paragraph(Vec<Inline>),
    CodeBlock {
        lang: Option<String>,
        text: String,
    },
    BlockQuote(Vec<Block>),
    List {
        /// `Some(start)` for an ordered list, `None` for a bullet list.
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Table {
        aligns: Vec<Align>,
        head: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Rule,
}

/// One comment-bearing annotation, in document order.
///
/// Membership is [`crate::annotate::Annotation::is_listed`]'s — the same predicate the
/// margin chips and the annotations viewer use — so the export cannot disagree with
/// either about what an annotation is.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExportAnnotation {
    /// The reviewer's comment.
    pub(crate) comment: String,
    /// The claim's text, for a sink that shows the comment away from its claim.
    pub(crate) claim: String,
}

/// The heading level as the document wrote it. The preview folds h6 into h5's *tag*
/// because it has five heading tags; an export has no such constraint — `<h6>` is a
/// real element and a PDF scale is a number — so the level is kept as stated.
pub(super) fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

pub(super) fn align_of(a: pulldown_cmark::Alignment) -> Align {
    match a {
        pulldown_cmark::Alignment::None => Align::None,
        pulldown_cmark::Alignment::Left => Align::Left,
        pulldown_cmark::Alignment::Center => Align::Center,
        pulldown_cmark::Alignment::Right => Align::Right,
    }
}

/// The plain text of an inline run — for a heading's anchor slug, an image's alt, and
/// an annotation's claim summary.
pub(crate) fn plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    collect_text(inlines, &mut out);
    out
}

fn collect_text(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text { text, .. } => out.push_str(text),
            Inline::Code(c) => out.push_str(c),
            Inline::Break => out.push(' '),
            Inline::Image(img) => out.push_str(&img.alt),
            Inline::Emphasis(v)
            | Inline::Strong(v)
            | Inline::Strikethrough(v)
            | Inline::Superscript(v)
            | Inline::Subscript(v)
            | Inline::Highlight(v)
            | Inline::Claim(_, v) => collect_text(v, out),
            Inline::Link { inner, .. } => collect_text(inner, out),
        }
    }
}

/// A whole document, display-free, ready for either sink.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ExportDoc {
    /// The document's first level-1 heading, if it has one — the artefact's title.
    pub(crate) title: Option<String>,
    pub(crate) blocks: Vec<Block>,
    pub(crate) annotations: Vec<ExportAnnotation>,
    /// True when at least one image was left as a URL rather than embedded. The PDF
    /// sink cannot reference a URL the way HTML can, so this is what lets the export
    /// tell the reader about the gaps rather than let them find them (TDD 25.12).
    pub(crate) has_unembedded_remote_images: bool,
    /// Content-run records `(cleaned_start, cleaned_end, rendered_before,
    /// rendered_after)` from the walk, consumed by the claim mapper and empty
    /// afterwards. Kept on the model rather than threaded through every call site;
    /// no sink reads it.
    pub(crate) content_evs: Vec<(usize, usize, i32, i32)>,
}

#[cfg(test)]
mod export_name_tests {
    use super::{default_export_name, ExportTarget};
    use std::path::Path;

    #[test]
    fn the_default_name_is_the_stem_plus_the_target_extension() {
        // TDD 25.10 — asserted on EVERY platform, because the derivation that is
        // silently fine on macOS is destructive on Windows.
        assert_eq!(
            default_export_name(Some(Path::new("/docs/notes.md")), ExportTarget::Pdf),
            "notes.pdf"
        );
        assert_eq!(
            default_export_name(Some(Path::new("/docs/notes.md")), ExportTarget::Html),
            "notes.html"
        );
    }

    #[test]
    fn the_documents_own_filename_is_never_proposed() {
        // The whole point: a chooser that appends the filter's extension only to a
        // suffix-less name returns `notes.md` unchanged, and the export then writes
        // over the reader's source.
        for target in [ExportTarget::Pdf, ExportTarget::Html] {
            let name = default_export_name(Some(Path::new("/docs/notes.md")), target);
            assert!(
                !name.ends_with(".md"),
                "{name} still carries the document's own extension"
            );
            assert!(name.ends_with(target.extension()));
        }
    }

    #[test]
    fn a_stem_that_already_looks_like_the_target_is_not_doubled() {
        assert_eq!(
            default_export_name(Some(Path::new("/docs/report.pdf")), ExportTarget::Pdf),
            "report.pdf"
        );
    }

    #[test]
    fn a_multi_dot_name_keeps_everything_before_its_last_suffix() {
        assert_eq!(
            default_export_name(Some(Path::new("/docs/v1.2.notes.md")), ExportTarget::Html),
            "v1.2.notes.html"
        );
    }

    #[test]
    fn an_untitled_buffer_still_gets_a_name() {
        // An untitled document exports (TDD 25.5), so it needs a proposal.
        assert_eq!(
            default_export_name(None, ExportTarget::Html),
            "untitled.html"
        );
    }

    #[test]
    fn a_stem_this_platform_would_refuse_falls_back_rather_than_being_proposed() {
        // Validate the name we PROPOSE, never the one the chooser returns (TDD 25.11).
        // The expectation is a LITERAL per platform, never re-derived by calling
        // `is_valid_filename` on this function's own output. That derivation passed
        // whichever branch ran — reject `CON` → `untitled.pdf` → `is_valid_filename
        // ("untitled")` is true; accept `CON` → `CON.pdf` → `is_valid_filename("CON")`
        // is also true — so it was green on both platforms and pinned neither
        // (GTK4Rs/AP-160's self-agreeing shape; found by the Windows seat). `cfg!`
        // rather than `#[cfg]` so both arms compile everywhere and nothing is silently
        // deleted from a platform's run, per POLICY's "never `#[cfg(platform)]` a test".
        let name = default_export_name(Some(Path::new("/docs/CON.md")), ExportTarget::Pdf);
        let expected = if cfg!(windows) {
            // A reserved device name: refused, so the stem must not survive.
            "untitled.pdf"
        } else {
            // An ordinary name on POSIX: it must survive untouched.
            "CON.pdf"
        };
        assert_eq!(
            name, expected,
            "the proposed name must fall back exactly when THIS platform refuses the stem"
        );

        // A stem every platform refuses, so the validation step is pinned on POSIX too.
        // Mutation-checked: deleting the `is_valid_filename` guard above leaves the
        // `CON` case green on Linux — `CON` is a perfectly ordinary POSIX name, so both
        // behaviours agree there and no Linux assertion can see the guard go missing.
        // A dot-entry stem is refused by BOTH rule sets, so this case fails on every
        // platform if the guard is removed.
        assert_eq!(
            default_export_name(Some(Path::new("/docs/...")), ExportTarget::Pdf),
            "untitled.pdf",
            "a dot-entry stem is refused everywhere and must never be proposed"
        );
    }
}
