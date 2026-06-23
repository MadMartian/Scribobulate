//! The Tier-2 Insert↔Edit Link/Image command kind.

/// The two Tier-2 insert commands that also EDIT an existing one when the selection is
/// exactly that markup — drives their Insert↔Edit surface relabel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FmtInsertKind {
    Link,
    Image,
}

impl FmtInsertKind {
    /// Parse a `FORMAT_CMDS` target (the GAction string boundary) into a kind.
    pub(crate) fn from_target(target: &str) -> Option<Self> {
        match target {
            "link" => Some(Self::Link),
            "image" => Some(Self::Image),
            _ => None,
        }
    }

    /// The `win.format` action target string for this kind (the GAction boundary).
    pub(crate) fn target(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Image => "image",
        }
    }

    /// The surface label/tooltip for this command in insert vs edit mode.
    pub(crate) fn label(self, editing: bool) -> &'static str {
        match (self, editing) {
            (Self::Link, false) => "Insert Link…",
            (Self::Link, true) => "Edit Link…",
            (Self::Image, false) => "Insert Image…",
            (Self::Image, true) => "Edit Image…",
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::winstate::*;

    #[test]
    fn fmt_insert_kind_target_round_trips_and_rejects_junk() {
        // The GAction target string is the boundary between the menu/toolbar
        // wiring and the typed kind — a round-trip mismatch would silently route
        // an Insert Link command to the Image handler (or vice versa).
        for kind in [FmtInsertKind::Link, FmtInsertKind::Image] {
            assert_eq!(FmtInsertKind::from_target(kind.target()), Some(kind));
        }
        assert_eq!(
            FmtInsertKind::from_target("link"),
            Some(FmtInsertKind::Link)
        );
        assert_eq!(
            FmtInsertKind::from_target("image"),
            Some(FmtInsertKind::Image)
        );
        assert_eq!(FmtInsertKind::from_target("bogus"), None);
    }

    #[test]
    fn fmt_insert_kind_label_reflects_insert_vs_edit() {
        // The full 2×2 relabel matrix (link/image × insert/edit) that drives the
        // command's surface text as the caret enters/leaves an existing markup span.
        assert_eq!(FmtInsertKind::Link.label(false), "Insert Link…");
        assert_eq!(FmtInsertKind::Link.label(true), "Edit Link…");
        assert_eq!(FmtInsertKind::Image.label(false), "Insert Image…");
        assert_eq!(FmtInsertKind::Image.label(true), "Edit Image…");
    }
}
