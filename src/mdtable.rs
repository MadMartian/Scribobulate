//! Shared GFM table vocabulary: what a delimiter row said about each column.
//!
//! Display-free and dependency-free, because it has **two** consumers that must not
//! answer differently — the preview renderer and the export pipeline. It lives here
//! rather than inside `export::` for exactly that reason: while [`Align`] was an
//! `export`-private type, the widget half could not name it, and the two halves diverged
//! in the direction nobody checks.
//!
//! MEASURED, on one document containing `|:-----|----:|:-----:|`: the PDF right-aligned
//! the numeric column and centred the last one, while the preview left-aligned every
//! cell. Document Rendering CAM row 17 says an export shows what the preview showed;
//! this was the same rule failing in reverse, with the *export* the richer of the two.

/// A GFM table column's alignment, as the delimiter row stated it.
///
/// [`None`](Self::None) is a column whose delimiter carried no colon — it is not a
/// synonym for [`Left`](Self::Left), even though both render flush left today: the
/// distinction is between "the author said left" and "the author said nothing", and a
/// future default (a numeric column auto-aligning right, say) would need to tell them
/// apart. Keep them separate for that reason and not because the pixels differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    None,
    Left,
    Center,
    Right,
}

impl Align {
    /// The horizontal alignment fraction a GTK widget wants: `0.0` flush left, `0.5`
    /// centred, `1.0` flush right.
    ///
    /// One conversion, so the preview's cell labels and any future drawn cell cannot
    /// disagree about what `Center` means.
    pub(crate) fn xalign(self) -> f32 {
        match self {
            Self::None | Self::Left => 0.0,
            Self::Center => 0.5,
            Self::Right => 1.0,
        }
    }
}

/// Translate pulldown-cmark's own alignment enum into [`Align`].
///
/// The single crossing point from the parser's vocabulary to the application's, so a
/// change in the parser is one edit here rather than one per consumer.
pub(crate) fn align_of(a: pulldown_cmark::Alignment) -> Align {
    match a {
        pulldown_cmark::Alignment::None => Align::None,
        pulldown_cmark::Alignment::Left => Align::Left,
        pulldown_cmark::Alignment::Center => Align::Center,
        pulldown_cmark::Alignment::Right => Align::Right,
    }
}

/// The alignment for column `col`, for a table whose delimiter row produced `aligns`.
///
/// Total by design: a row with more cells than the delimiter row declared is legal input
/// (GFM truncates, but the renderer sees the events either way), and a cell past the end
/// takes [`Align::None`] rather than panicking on a document the user typed. Untrusted
/// content (TDD 2.7) reaches this function.
pub(crate) fn column_align(aligns: &[Align], col: usize) -> Align {
    aligns.get(col).copied().unwrap_or(Align::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xalign_maps_each_alignment_to_its_gtk_fraction() {
        assert_eq!(Align::None.xalign(), 0.0);
        assert_eq!(Align::Left.xalign(), 0.0);
        assert_eq!(Align::Center.xalign(), 0.5);
        assert_eq!(Align::Right.xalign(), 1.0);
    }

    #[test]
    fn a_cell_past_the_declared_columns_takes_no_alignment_rather_than_panicking() {
        let aligns = [Align::Right, Align::Center];
        assert_eq!(column_align(&aligns, 0), Align::Right);
        assert_eq!(column_align(&aligns, 1), Align::Center);
        assert_eq!(column_align(&aligns, 2), Align::None);
        assert_eq!(column_align(&[], 0), Align::None);
    }

    /// The delimiter row's four spellings survive the crossing from pulldown-cmark.
    #[test]
    fn every_parser_alignment_crosses_into_the_application_vocabulary() {
        use pulldown_cmark::Alignment as P;
        assert_eq!(align_of(P::None), Align::None);
        assert_eq!(align_of(P::Left), Align::Left);
        assert_eq!(align_of(P::Center), Align::Center);
        assert_eq!(align_of(P::Right), Align::Right);
    }
}
