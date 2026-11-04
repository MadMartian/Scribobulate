//! TDD 2.17b: how a document-named anchor is read and what it answers to.

use super::{
    scan_named_anchors, AnchorTarget, HeadingAnchors, HeadingSlugger, NamedAnchor, NamedAnchorWalk,
};
use crate::renderer::frontmatter;
use pulldown_cmark::{Event, Tag};

/// The walk's reading of `md`, each bare position being the event's index in the stream.
fn walk(md: &str) -> Vec<NamedAnchor<usize>> {
    let mut named = NamedAnchorWalk::default();
    for (i, (ev, _)) in frontmatter::events(md, frontmatter::Show::Omitted).enumerate() {
        named.observe(&ev, i);
    }
    named.finish()
}

fn is_attached(md: &str, name: &str, ordinal: usize) -> bool {
    walk(md)
        .iter()
        .any(|a| a.name == name && a.target == AnchorTarget::Heading(ordinal))
}

fn is_bare(md: &str, name: &str) -> bool {
    walk(md)
        .iter()
        .any(|a| a.name == name && matches!(a.target, AnchorTarget::Bare(_)))
}

/// The resolver over `md`'s real walk, each position being the heading's ordinal or, for
/// a bare anchor, 100 plus its event index.
fn page(md: &str) -> HeadingAnchors<usize> {
    let mut slugger = HeadingSlugger::default();
    let mut named = NamedAnchorWalk::default();
    let mut headings = Vec::new();
    let mut text: Option<String> = None;
    for (i, (ev, _)) in frontmatter::events(md, frontmatter::Show::Omitted).enumerate() {
        named.observe(&ev, 100 + i);
        if matches!(ev, Event::Start(Tag::Heading { .. })) {
            text = Some(String::new());
        }
        if let (Event::Text(t), Some(text)) = (&ev, &mut text) {
            text.push_str(t);
        }
        if matches!(ev, Event::End(pulldown_cmark::TagEnd::Heading(_))) {
            if let Some(text) = text.take() {
                let ordinal = headings.len();
                headings.push((slugger.next(&text), ordinal));
            }
        }
    }
    HeadingAnchors::new(headings, named.finish())
}

#[test]
fn an_anchor_is_read_from_id_or_name_on_an_a_open_tag_only() {
    assert_eq!(scan_named_anchors(r#"<a id="x">"#), ["x"]);
    assert_eq!(scan_named_anchors(r#"<a name='x'>"#), ["x"]);
    assert_eq!(scan_named_anchors(r#"<A ID="Mixed">"#), ["Mixed"]);
    assert_eq!(scan_named_anchors(r#"<a id="x" name="x">"#), ["x"]);
    assert_eq!(scan_named_anchors(r#"<a id="x" name="y">"#), ["x", "y"]);
    assert_eq!(scan_named_anchors(r#"<div><a id="x"></a></div>"#), ["x"]);
    for inert in [
        r#"</a>"#,
        r#"<a id="">"#,
        r##"<a href="#x">"##,
        r#"<abbr id="x">"#,
        r#"<span id="x">"#,
        r#"<!-- <a id="x"> -->"#,
        r#"<script><a id="x"></script>"#,
    ] {
        assert!(scan_named_anchors(inert).is_empty(), "{inert}");
    }
}

/// The operator's reported form, and the spellings of the same intent.
#[test]
fn an_anchor_with_nothing_visible_before_a_heading_names_it() {
    assert!(is_attached("<a id=\"f\"></a>\n## 🎲 Flakes\n", "f", 0));
    assert!(is_attached("<a id=\"f\"></a>\n\n## Flakes\n", "f", 0));
    assert!(is_attached("<a id=\"f\">\n</a>\n\n## Flakes\n", "f", 0));
    assert!(is_attached("<div><a id=\"f\"></a></div>\n\n## H\n", "f", 0));
    assert!(is_attached("## <a id=\"f\"></a>Flakes\n", "f", 0));
    assert!(is_attached("## <a name=\"f\">Flakes</a>\n", "f", 0));
    assert!(is_attached("## Flakes <a id=\"f\"></a>\n", "f", 0));
    let two = "<a id=\"a\"></a>\n<a id=\"b\"></a>\n\n# One\n\n# Two\n";
    assert!(is_attached(two, "a", 0) && is_attached(two, "b", 0));
    assert!(is_attached("# One\n\n<a id=\"f\"></a>\n\n# Two\n", "f", 1));
}

#[test]
fn an_anchor_with_something_visible_after_it_is_a_position_of_its_own() {
    for md in [
        "<a id=\"f\"></a>\n\nProse.\n\n## H\n",
        "<a id=\"f\"></a> Prose.\n\n## H\n",
        "Prose <a id=\"f\"></a>\n\n## H\n",
        "<a id=\"f\">Prose</a>\n\n## H\n",
        "<a id=\"f\"></a><img src=\"a.png\">\n\n## H\n",
        "<a id=\"f\"></a>\n\n---\n\n## H\n",
        "<a id=\"f\"></a>\n\n```\ncode\n```\n\n## H\n",
        "- <a id=\"f\"></a>\n\n## H\n",
        "> <a id=\"f\"></a>\n\n## H\n",
        "| a |\n|---|\n| <a id=\"f\"></a> |\n\n## H\n",
        "<a id=\"f\"></a>\n\n<details><summary>S</summary>\n\n## H\n\n</details>\n",
        "<a id=\"f\"></a>\n",
    ] {
        assert!(is_bare(md, "f"), "{md}");
    }
}

#[test]
fn text_that_only_looks_like_an_anchor_declares_nothing() {
    for md in [
        "`<a id=\"f\"></a>`\n\n# H\n",
        "```\n<a id=\"f\"></a>\n```\n\n# H\n",
        "    <a id=\"f\"></a>\n\n# H\n",
        "\\<a id=\"f\"></a>\n\n# H\n",
        "<!-- <a id=\"f\"></a> -->\n\n# H\n",
        "![<a id=\"f\"></a>](x.png)\n\n# H\n",
    ] {
        assert!(walk(md).is_empty(), "{md}");
    }
}

#[test]
fn a_fragment_on_an_attached_anchor_lands_on_the_heading_and_records_its_slug() {
    let anchors = page("# One\n\n<a id=\"flakes\"></a>\n## Flakes And More\n");
    assert_eq!(
        anchors.resolve("flakes"),
        Some(("flakes-and-more", 1)),
        "the two link forms must be one spot to Back/Forward"
    );
    assert_eq!(
        anchors.resolve("flakes-and-more"),
        Some(("flakes-and-more", 1)),
        "the generated slug keeps working: the anchor adds a name, it overrides nothing"
    );
    assert!(!anchors.slugs().contains("flakes"));
}

#[test]
fn a_bare_anchor_is_its_own_canonical_name_so_history_keeps_it() {
    let anchors = page("# One\n\nProse <a id=\"note\"></a> here.\n");
    let (name, pos) = anchors.resolve("note").expect("a bare anchor resolves");
    assert_eq!(name, "note");
    assert!(pos >= 100, "its own position, not a heading's: {pos}");
    assert!(
        anchors.slugs().contains("note"),
        "Back/Forward degrades an entry whose name is not listed (TDD 23.14)"
    );
}

#[test]
fn a_headings_github_slug_beats_an_anchor_wherever_the_anchor_sits() {
    for md in [
        "Prose <a id=\"setup\"></a>.\n\n# Intro\n\n# Setup\n",
        "# Setup\n\nProse <a id=\"setup\"></a>.\n",
        "<a id=\"setup\"></a>\n# Intro\n\n# Setup\n",
    ] {
        let anchors = page(md);
        let (slug, _) = anchors.resolve("setup").expect("the heading resolves");
        assert_eq!(slug, "setup", "{md}");
        let setup_ordinal = usize::from(md.contains("Intro"));
        assert_eq!(anchors.resolve("setup").map(|r| r.1), Some(setup_ordinal));
    }
}

#[test]
fn the_first_anchor_of_a_name_wins() {
    let anchors = page("<a id=\"x\"></a>\n# One\n\n<a id=\"x\"></a>\n# Two\n");
    assert_eq!(anchors.resolve("x"), Some(("one", 0)));
    let anchors = page("Prose <a id=\"x\"></a>.\n\n<a id=\"x\"></a>\n# One\n");
    assert_eq!(anchors.resolve("x").map(|r| r.0), Some("x"));
}

/// An author's stated name is stronger than a slug derived for convenience.
#[test]
fn an_anchor_beats_an_emoji_free_alias() {
    let anchors = page("# 🚨 Escalations\n\n<a id=\"escalations\"></a>\n# Other\n");
    assert_eq!(anchors.resolve("escalations"), Some(("other", 1)));
    assert_eq!(anchors.resolve("-escalations"), Some(("-escalations", 0)));
}

#[test]
fn an_anchor_does_not_disturb_github_numbering() {
    let anchors = page("<a id=\"a\"></a>\n# Same\n\n# Same\n");
    assert_eq!(anchors.resolve("same"), Some(("same", 0)));
    assert_eq!(anchors.resolve("same-1"), Some(("same-1", 1)));
    assert_eq!(anchors.resolve("a"), Some(("same", 0)));
}
