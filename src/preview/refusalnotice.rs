//! **A refused image is reported to the document whose render shows it** (TDD 2.23c;
//! Status-notice CAM row 9) — asserted against real renders.
//!
//! Test-only, and carrying `gtk-integration-tests` for the reason `altsuppression`
//! gives: a render needs a `GtkTextBuffer`. The routing from a tab to its window and
//! the notice's wording are pure and tested in `winstate::statusbar`; this asserts the
//! seam between them that no pure test can reach — that the render path carries the
//! tab it is shown in down to the point where the image is refused.

use super::build::build_render_products_with_theme;
use crate::links::{resolve_image, ImageResolution};
use crate::winstate::TabId;

/// An SVG the XInclude screen refuses.
const HOSTILE: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xi="http://www.w3.org/2001/XInclude"/>"#;

fn render_for(tab: Option<TabId>, dir: &std::path::Path) {
    build_render_products_with_theme(
        tab,
        "Before.\n\n![hostile](hostile.svg)\n",
        Some(dir),
        1.0,
        false,
        crate::theme::active(),
        &crate::fold::FoldState::default(),
    );
}

/// The render for a tab tells the screen which tab is showing the refused image; a
/// re-render of the same tab adds nothing; a second tab is told too; and a render no
/// reader sees tells nobody.
#[gtktest::test]
fn a_refused_image_is_reported_to_the_tab_whose_render_shows_it() {
    if !crate::imagedecode::screens_xinclude() {
        println!("SKIPPED [2.23c]: this platform does not screen SVG XInclude");
        return;
    }
    let tmp = tempfile::tempdir().expect("a temp dir");
    let dir = tmp.path().canonicalize().expect("canonical temp dir");
    std::fs::write(dir.join("hostile.svg"), HOSTILE).expect("write the fixture");
    let ImageResolution::Local(path) = resolve_image("hostile.svg", Some(&dir), false) else {
        panic!("precondition: the fixture resolves to a local image");
    };
    let origin = path.display().to_string();
    let told = || crate::imagedecode::told_for_test(&origin);
    // Ids no real tab will have, so a window test sharing the process cannot collide.
    let (first, second) = (
        TabId::from_raw(u64::MAX - 10),
        TabId::from_raw(u64::MAX - 11),
    );
    let document = crate::imagedecode::RefusalTarget::Document;

    render_for(None, &dir);
    assert!(
        crate::imagedecode::svg_refused(&origin),
        "precondition: the render reached the screen and was refused"
    );
    assert!(told().is_empty(), "a render no reader sees reports nothing");

    render_for(Some(first), &dir);
    render_for(Some(first), &dir);
    assert_eq!(
        told(),
        [document(first)],
        "the tab whose render shows it, once"
    );

    render_for(Some(second), &dir);
    assert_eq!(
        told(),
        [document(second), document(first)],
        "a second document is told too"
    );

    crate::imagedecode::forget_for_test(&origin);
}
