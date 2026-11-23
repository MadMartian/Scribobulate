//! The editor's Markdown language: a corrected copy of GtkSourceView's `markdown.lang`.
//!
//! GtkSourceView's own definition matches link text with a lazy `\[(.*?)\]\(`, which
//! still starts at the FIRST `[` on a line, so in `- [ ] caption [link](http://x)` the
//! task box, the caption and the link's text all came out as link text. The copy in
//! `data/language-specs/markdown.lang` changes only the bracketed-text patterns; its
//! header says exactly what, which upstream version it was taken from, and the duty to
//! follow upstream. It keeps the id `markdown`, so the style schemes apply unchanged.
//!
//! **How it is loaded, and why it is a seam.** The file is compiled into the binary as a
//! GResource of its own (no packaging step on any platform) and its `resource://`
//! directory is put FIRST on the default `GtkSourceLanguageManager`'s search path, so the
//! manager finds this `markdown` before the system's. GtkSourceView reads the search path
//! once, when it first lists its languages, and `set_search_path` refuses (a
//! `Gtk-CRITICAL`) once that has happened — so the path must be installed before the
//! FIRST language lookup anywhere in the process. [`markdown`] is that lookup: it installs
//! the path on its first call, and `clippy.toml` bans the manager's lookups everywhere
//! else, so no other caller can list the languages first.
//!
//! If the resource fails to register, the stock language is used and the failure is
//! logged: wrongly coloured link text is a better outcome than an editor with no colour.

use std::cell::Cell;

/// The resource directory `data/language-specs.gresource.xml` compiles to.
const RESOURCE_DIR: &str = "resource:///com/extollit/scribobulate/language-specs";

/// The language id, shared with GtkSourceView's own definition on purpose.
const LANGUAGE_ID: &str = "markdown";

thread_local! {
    /// GTK runs on one thread, so "installed in this process" is "installed on this
    /// thread"; the default language manager is not usable from any other.
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

/// The editor's Markdown language — the corrected copy, or GtkSourceView's own if the
/// copy could not be registered. Every editor buffer takes its language from here.
pub(crate) fn markdown() -> Option<sourceview::Language> {
    install_search_path();
    #[expect(
        clippy::disallowed_methods,
        reason = "the one sanctioned lookup: the search path is installed on the line above"
    )]
    let language = sourceview::LanguageManager::default().language(LANGUAGE_ID);
    language
}

/// Put [`RESOURCE_DIR`] first on the default manager's search path, once.
fn install_search_path() {
    if INSTALLED.with(|installed| installed.replace(true)) {
        return;
    }
    if let Err(err) = gtk::gio::resources_register_include!("language-specs.gresource") {
        log::error!(
            "editor Markdown language: the corrected definition did not register ({err}); \
             falling back to GtkSourceView's own"
        );
        return;
    }
    let manager = sourceview::LanguageManager::default();
    let system = manager.search_path();
    let mut dirs: Vec<&str> = Vec::with_capacity(system.len() + 1);
    dirs.push(RESOURCE_DIR);
    dirs.extend(system.iter().map(|dir| dir.as_str()));
    manager.set_search_path(&dirs);
    log::debug!("editor Markdown language: search path {dirs:?}");
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use sourceview::prelude::*;

    /// The language the editor gets is the vendored copy, not the system's: the
    /// highlighting tests below would also pass against a future GtkSourceView that
    /// fixed the rule upstream, and that must not hide a copy that stopped loading.
    #[gtktest::test]
    fn the_editor_language_is_the_vendored_copy() {
        let language = super::markdown().expect("a markdown language is available");
        assert_eq!(
            language.metadata("scribobulate-vendored-from").as_deref(),
            Some("gtksourceview-5.4.1"),
            "the editor's markdown language is not the vendored copy under {}",
            super::RESOURCE_DIR
        );
        assert_eq!(language.id(), super::LANGUAGE_ID);
    }

    /// The control line every fixture ends with: one inline link and one inline image
    /// whose roles are not in doubt, so the tags GtkSourceView gives link text, a URL and
    /// an image's alternate text are read off the engine rather than named by the test.
    /// The highlighting engine's style tags are anonymous, so identity is the only handle.
    const CONTROL: &str = "\n[ctl](http://ctl) ![cim](http://cim)";

    struct Roles {
        buffer: sourceview::Buffer,
        link_text: Vec<gtk::TextTag>,
        url: Vec<gtk::TextTag>,
        alt_text: Vec<gtk::TextTag>,
    }

    fn tags_at(buffer: &sourceview::Buffer, offset: usize) -> Vec<gtk::TextTag> {
        let offset = i32::try_from(offset).expect("fixture offset fits i32");
        buffer.iter_at_offset(offset).tags()
    }

    /// Build the application's own editor over `line` + [`CONTROL`], highlight it
    /// synchronously, and read the three role tag sets off the control line.
    fn highlighted(line: &str) -> Roles {
        let text = format!("{line}{CONTROL}");
        let (buffer, _view) = crate::window::build_tab_editor(&text);
        let (start, end) = buffer.bounds();
        buffer.ensure_highlight(&start, &end);
        let control = line.chars().count() + 1;
        let link_text = tags_at(&buffer, control + "[c".chars().count());
        let url = tags_at(&buffer, control + "[ctl](h".chars().count());
        let alt_text = tags_at(&buffer, control + "[ctl](http://ctl) ![c".chars().count());
        assert!(
            !link_text.is_empty() && !url.is_empty() && !alt_text.is_empty(),
            "the control line carries no highlighting at all — the markdown language did \
             not load, so nothing below could discriminate"
        );
        assert!(
            link_text != url && link_text != alt_text,
            "link text, URL and alternate text share one tag set — the oracle cannot tell \
             the roles apart"
        );
        Roles {
            buffer,
            link_text,
            url,
            alt_text,
        }
    }

    impl Roles {
        /// Offset of the first `needle` in the fixture line, plus `within` characters.
        fn at(&self, line: &str, needle: &str, within: usize) -> usize {
            let byte = line.find(needle).expect("needle is in the fixture line");
            line[..byte].chars().count() + within
        }

        fn carries(&self, offset: usize, role: &[gtk::TextTag]) -> bool {
            let tags = tags_at(&self.buffer, offset);
            role.iter().all(|tag| tags.contains(tag))
        }
    }

    /// TDD 4.2 — the reported line: a task item whose caption precedes a link. The
    /// caption is plain; the link's own text and URL keep their roles.
    #[gtktest::test]
    fn a_task_caption_before_a_link_is_not_link_text() {
        let line = "- [ ] caption [link](http://x)";
        let roles = highlighted(line);
        let caption = roles.at(line, "caption", 1);
        assert!(
            !roles.carries(caption, &roles.link_text),
            "the caption is coloured as link text: the inline-link rule matched from the \
             task box's `[`"
        );
        assert!(roles.carries(roles.at(line, "link]", 1), &roles.link_text));
        assert!(roles.carries(roles.at(line, "http://x", 1), &roles.url));
    }

    /// TDD 4.2 — the same flaw without a task box: a bracketed word before a link
    /// must not pull the text between them into the link.
    #[gtktest::test]
    fn a_bracketed_word_before_a_link_stays_out_of_the_link() {
        let line = "[a] [b](http://u)";
        let roles = highlighted(line);
        assert!(
            !roles.carries(roles.at(line, "] [", 1), &roles.link_text),
            "the space between `[a]` and `[b](…)` is link text: the link swallowed `[a]`"
        );
        assert!(roles.carries(roles.at(line, "b]", 0), &roles.link_text));
        assert!(roles.carries(roles.at(line, "http://u", 1), &roles.url));
    }

    /// TDD 4.2 — images: a caption before an image is plain, the alternate text keeps
    /// its role, and a bracketed word before an image cannot become part of its alt.
    #[gtktest::test]
    fn an_image_keeps_its_alternate_text_and_nothing_more() {
        let line = "- [ ] caption ![alt](http://i)";
        let roles = highlighted(line);
        let caption = roles.at(line, "caption", 1);
        assert!(!roles.carries(caption, &roles.link_text));
        assert!(!roles.carries(caption, &roles.alt_text));
        assert!(roles.carries(roles.at(line, "alt]", 1), &roles.alt_text));
        assert!(roles.carries(roles.at(line, "http://i", 1), &roles.url));

        let line = "![a] between [c](http://u)";
        let roles = highlighted(line);
        assert!(
            !roles.carries(roles.at(line, "between", 1), &roles.alt_text),
            "`between` is an image's alternate text: the inline-image rule ran from `![a]` \
             to the link's `](`"
        );
        assert!(roles.carries(roles.at(line, "c]", 0), &roles.link_text));
    }

    /// What excluding brackets must NOT cost: one level of nested brackets and an
    /// escaped bracket inside link text both still make a link.
    #[gtktest::test]
    fn nested_and_escaped_brackets_still_make_a_link() {
        for (line, inner) in [("[a [b] c](http://u)", "c]"), (r"[a \] c](http://u)", "c]")] {
            let roles = highlighted(line);
            assert!(
                roles.carries(roles.at(line, inner, 0), &roles.link_text),
                "{line:?}: the link text lost its role"
            );
            assert!(
                roles.carries(roles.at(line, "http://u", 1), &roles.url),
                "{line:?}: the URL lost its role"
            );
        }
    }
}
