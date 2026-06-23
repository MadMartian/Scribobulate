//! The broken-image placeholder reason string — pure and unit-tested. The GTK side
//! (image anchoring in [`super::start`]) uses it to decide whether to show the
//! picture or a `image-missing` icon with this tooltip.

use crate::links::ImageResolution;

/// The tooltip for the broken-image placeholder shown when an image cannot be
/// displayed, or `None` when it loaded and no placeholder is needed.
///
/// Every non-renderable image gets a VISIBLE placeholder (with this tooltip)
/// rather than silently degrading to bare alt text — so the reader always sees
/// that an image was expected AND why it isn't there. The three reasons are
/// distinguished (they were previously conflated: Refused showed an icon while
/// Missing / failed-to-decode silently fell back to alt text, so toggling
/// "Show Unsafe Images" on an image whose file was absent removed the icon and
/// left only the alt string, looking like the toggle did nothing).
///
/// `has_texture` is whether a `GdkTexture` actually loaded: a `Local`/`Remote`
/// resolution with no texture is a decode/read failure, distinct from a path
/// that could not be resolved at all (`Missing`).
pub(super) fn image_placeholder_tooltip(
    resolution: &ImageResolution,
    has_texture: bool,
    src: &str,
) -> Option<String> {
    if has_texture {
        return None;
    }
    Some(match resolution {
        ImageResolution::Refused => {
            format!("Blocked image (enable Show Unsafe Images to load): {src}")
        }
        ImageResolution::Missing => format!("Image not found: {src}"),
        // Resolved to a path/URI, but the texture never loaded — the file exists
        // (or the URL was fetched) yet could not be decoded as an image.
        ImageResolution::Local(_) | ImageResolution::Remote(_) => {
            format!("Could not load image: {src}")
        }
    })
}

#[cfg(test)]
mod image_placeholder_tests {
    use super::image_placeholder_tooltip;
    use crate::links::ImageResolution;
    use std::path::PathBuf;

    #[test]
    fn loaded_image_has_no_placeholder() {
        // A texture loaded → the picture stands in, no placeholder tooltip.
        assert!(image_placeholder_tooltip(
            &ImageResolution::Local(PathBuf::from("/x/y.png")),
            true,
            "y.png"
        )
        .is_none());
        assert!(image_placeholder_tooltip(
            &ImageResolution::Remote("https://h/i.png".into()),
            true,
            "https://h/i.png"
        )
        .is_none());
    }

    #[test]
    fn refused_reports_the_policy_block() {
        let t =
            image_placeholder_tooltip(&ImageResolution::Refused, false, "../secret.svg").unwrap();
        assert!(t.contains("Show Unsafe Images"), "{t}");
        assert!(t.contains("../secret.svg"), "{t}");
    }

    #[test]
    fn missing_reports_not_found_with_the_src() {
        // The regression: a Missing image (file absent at render time) must now
        // show a "not found" placeholder rather than silently degrading to alt
        // text — so toggling Show Unsafe Images on an absent file no longer
        // leaves a bare alt string that looks like the toggle did nothing.
        let t =
            image_placeholder_tooltip(&ImageResolution::Missing, false, "./target-structure.svg")
                .unwrap();
        assert!(t.contains("not found"), "{t}");
        assert!(t.contains("./target-structure.svg"), "{t}");
    }

    #[test]
    fn resolved_but_undecodable_reports_load_failure() {
        // Path resolved / URL fetched, but no texture → decode failure, distinct
        // from Missing.
        let local = image_placeholder_tooltip(
            &ImageResolution::Local(PathBuf::from("/x/bad.png")),
            false,
            "bad.png",
        )
        .unwrap();
        assert!(local.contains("Could not load"), "{local}");
        assert!(local.contains("bad.png"), "{local}");
        let remote = image_placeholder_tooltip(
            &ImageResolution::Remote("https://h/bad.png".into()),
            false,
            "https://h/bad.png",
        )
        .unwrap();
        assert!(remote.contains("Could not load"), "{remote}");
    }
}
