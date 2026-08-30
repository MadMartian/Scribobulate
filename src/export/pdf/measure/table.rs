//! Table measurement — the one construct that needs a measured column grid.
//!
//! Split from the rest of the measurement pass by CAUSE, not by size: every other block
//! is measured once at a width the page hands it, whereas a table must be measured
//! TWICE — once unconstrained, to learn what each column would naturally want, and again
//! at whatever [`pdftable::fit`] rules each column actually gets. Nothing else in the
//! sink has that shape, and the two-pass structure is what makes this the longest and
//! most easily-broken part of the pass.
//!
//! **The geometry decision is not here.** How wide each column ends up, and whether the
//! table must be scaled down at all, belongs to [`pdftable::fit`] and is settled by unit
//! test — including the property sweep that pins `scale > 0` at every nesting depth
//! (F-PDF-001). What happens in this file is measurement, and a decision appearing here
//! is the leak the parent module's doc warns about.

use super::super::super::markup::inline_markup;
use super::super::super::paginate::Fragment;
use super::super::super::{Align, ExportDoc, Inline};
use super::super::decide::table_column_count;
use super::super::geometry::{pango_to_pt, px_to_pt};
use super::super::pdftable;
use super::super::{
    CellWidths, LayoutSpec, LineKind, QuoteRef, TableCell, BASE_PT, BLOCK_GAP_PT,
    PANGO_WEIGHT_NORMAL,
};
use super::Layouter;
use gtk::pango;

/// Which row this is, and whether it must stay with the next one.
///
/// A NAMED value, not two adjacent `bool`s at the end of a nine-parameter signature.
/// The two were transposable — same type, same position class — in the ONE module that
/// introduces three dedicated structs (`Grid`, `Chrome`, `CellWidths`) specifically to
/// remove that hazard, and it carried an `#[allow(clippy::too_many_arguments)]` to say
/// so. `is_head` and `keep_with_next` are not independent in practice either: a header
/// always keeps company with the row under it, and folding them into one value is what
/// makes the two callers state their intent rather than their flags.
/// The measured grid and the theme's chrome, which always travel together.
///
/// One borrow rather than two parameters: `fit` computes the grid FROM the chrome, so a
/// row laid out against one and drawn with the other is a table whose reserved space and
/// drawn space disagree. Bundling them is also what brings `table_row` inside clippy's
/// argument limit without an `#[allow]` — the limit was flagging a real thing, which is
/// that a nine-parameter signature in this module was carrying decisions that belong in
/// values.
#[derive(Clone, Copy)]
pub(super) struct TableGeometry<'a> {
    pub(super) grid: &'a pdftable::Grid,
    pub(super) chrome: &'a pdftable::Chrome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowKind {
    /// The header row. Carries `keep_with_next` because a header alone at the foot of a
    /// page is the break this exists to prevent — `false` only where the table has no
    /// body for it to keep company with.
    Head { keep_with_next: bool },
    /// An ordinary body row: never kept with the next, so a long table breaks between
    /// any two of its rows.
    Body,
}

impl RowKind {
    fn is_head(self) -> bool {
        matches!(self, RowKind::Head { .. })
    }

    fn keep_with_next(self) -> bool {
        matches!(
            self,
            RowKind::Head {
                keep_with_next: true
            }
        )
    }
}

impl Layouter<'_> {
    /// A table is laid out one **row** at a time on a measured column grid, each row an
    /// indivisible fragment, so a page break falls between rows and never through one.
    ///
    /// The geometry decision — how wide each column is, and whether the table must be
    /// scaled at all — belongs to [`pdftable::fit`] and is settled by unit test. What
    /// happens here is measurement and ink, which is the only reason this file exists.
    pub(super) fn table(
        &mut self,
        aligns: &[Align],
        head: &[Vec<Inline>],
        rows: &[Vec<Vec<Inline>>],
        doc: &ExportDoc,
        indent: f64,
        quote: Option<QuoteRef>,
    ) {
        let column_count = table_column_count(head, rows);
        if column_count == 0 {
            return;
        }
        let chrome = self.table_chrome();

        // Pass 1 — measure every cell unconstrained, so a column's natural width is
        // its widest cell's own idea of how much room it wants.
        let head_markup = self.row_markup(head, doc, true);
        let body_markup: Vec<Vec<String>> = rows
            .iter()
            .map(|row| self.row_markup(row, doc, false))
            .collect();

        // One `ColumnWant` per column rather than two parallel slices: the pair travels
        // as a pair, so nothing downstream can transpose it.
        let mut wants = vec![
            pdftable::ColumnWant {
                natural: 0.0,
                minimum: 0.0,
            };
            column_count
        ];
        for row in std::iter::once(&head_markup).chain(body_markup.iter()) {
            for (index, markup) in row.iter().enumerate() {
                let CellWidths { max, min } = self.cell_widths(markup);
                if max > wants[index].natural {
                    wants[index].natural = max;
                }
                if min > wants[index].minimum {
                    wants[index].minimum = min;
                }
            }
        }

        // Pass 2 — the grid decides, then every row is built against it.
        let grid = pdftable::fit(&wants, self.printable_width(indent), &chrome);
        let geometry = TableGeometry {
            grid: &grid,
            chrome: &chrome,
        };
        if !head_markup.is_empty() {
            self.table_row(
                &head_markup,
                aligns,
                geometry,
                indent,
                quote,
                // The header keeps company with the first body row — where there is one.
                RowKind::Head {
                    keep_with_next: !rows.is_empty(),
                },
            );
        }
        for row in &body_markup {
            self.table_row(row, aligns, geometry, indent, quote, RowKind::Body);
        }
    }

    /// The theme's table chrome in points — no literal, per THEMING.md.
    ///
    /// Through `px_to_pt` rather than by multiplying `PT_PER_PX` here. The value is the
    /// same; the point is that it is the SAME ROUTE every other `Metrics` read in this
    /// sink takes, so "is this key converted?" has one answer to look up instead of one
    /// per key. It was coherent per key and wrong overall — `blockquote_bar_width`,
    /// `rule_space` and `heading_band_padding` were read straight as points while these
    /// three converted, so a reader checking any one metric concluded the sink was right.
    fn table_chrome(&self) -> pdftable::Chrome {
        let m = &self.theme.metrics;
        pdftable::Chrome {
            padding_h: px_to_pt(m.table_cell_padding_h),
            padding_v: px_to_pt(m.table_cell_padding_v),
            border: px_to_pt(m.table_border_width),
        }
    }

    /// One row's cells as Pango markup, bolded when it is the header.
    ///
    /// `<b>` is Pango's own "bolder than the base", which is a different number from the
    /// one the theme stated — so a theme setting `bold_weight = 800` got 800 for
    /// `**bold**` on this very page and Pango's default bold in the header beside it.
    /// `Typography::bold_attr` is the one spelling of that key, already shared by every
    /// surface for inline bold (F-BOLD-001).
    fn row_markup(&self, cells: &[Vec<Inline>], doc: &ExportDoc, head: bool) -> Vec<String> {
        cells
            .iter()
            .map(|cell| {
                let markup = inline_markup(cell, doc, self.theme);
                if head {
                    format!("<span{}>{markup}</span>", self.theme.typography.bold_attr())
                } else {
                    markup
                }
            })
            .collect()
    }

    /// How wide a cell wants to be, both ways.
    fn cell_widths(&self, markup: &str) -> CellWidths {
        let layout = self.cell_layout(markup, None, Align::None);
        // `extents()` answers (ink, logical); the LOGICAL rect is the one a column
        // width is measured from — ink stops at the glyphs and would drop a trailing
        // space's advance.
        let (_ink, logical) = layout.extents();
        let max = pango_to_pt(logical.width());

        // Min-content: squeeze the layout to nothing in `Word` mode, which refuses to
        // break inside a word, so the widest line that comes back IS the widest word.
        // `WordChar` would happily split one and report a meaningless 1pt.
        layout.set_wrap(pango::WrapMode::Word);
        layout.set_width(1);
        let (_ink, logical) = layout.extents();
        let min = pango_to_pt(logical.width());

        CellWidths { max, min }
    }

    /// A layout for one cell. `text_width` of `None` measures unconstrained; `Some`
    /// constrains it, which is what makes the text wrap **inside its column**.
    fn cell_layout(&self, markup: &str, text_width: Option<f64>, align: Align) -> pango::Layout {
        self.layout_of(
            markup,
            LayoutSpec {
                family: None,
                width_pt: text_width,
                size_pt: BASE_PT,
                // A cell's weight comes from its markup (the header row is bolded there),
                // so the descriptor stays at normal — stated because the paragraph path
                // passes a weight and the difference used to be silent.
                weight: PANGO_WEIGHT_NORMAL,
                align: match align {
                    Align::Center => pango::Alignment::Center,
                    Align::Right => pango::Alignment::Right,
                    Align::None | Align::Left => pango::Alignment::Left,
                },
            },
        )
    }

    /// Build and push one row: every cell laid out in its column, the row's height the
    /// tallest of them, and the whole thing **one** fragment.
    fn table_row(
        &mut self,
        cells: &[String],
        aligns: &[Align],
        geometry: TableGeometry<'_>,
        indent: f64,
        quote: Option<QuoteRef>,
        kind: RowKind,
    ) {
        let TableGeometry { grid, chrome } = geometry;
        let mut drawn = Vec::with_capacity(cells.len());
        let mut text_height = 0.0_f64;
        for (index, markup) in cells.iter().enumerate() {
            let Some(column) = grid.columns.get(index) else {
                // More cells than the delimiter row declared columns. GFM says the
                // surplus is dropped, and the preview drops it too — so dropping it
                // here is agreement, not loss.
                break;
            };
            let align = aligns.get(index).copied().unwrap_or(Align::None);
            let layout = self.cell_layout(markup, Some(column.text_width), align);
            let (_ink, logical) = layout.extents();
            let height = pango_to_pt(logical.height());
            if height > text_height {
                text_height = height;
            }
            drawn.push(TableCell {
                layout,
                column: index,
            });
        }

        // The row's box is the tallest cell plus this theme's vertical padding; the
        // fragment the paginator sees is that box at the grid's scale, so what it
        // reserves and what gets drawn are the same number by construction.
        let box_height = text_height + chrome.padding_v * 2.0;
        self.push_line(
            LineKind::TableRow {
                cells: drawn,
                columns: grid.columns.clone(),
                chrome: *chrome,
                scale: grid.scale,
                box_height,
                is_head: kind.is_head(),
            },
            Fragment {
                space_after: 0.0,
                height: box_height * grid.scale,
                space_before: if kind.is_head() { BLOCK_GAP_PT } else { 0.0 },
                keep_with_next: kind.keep_with_next(),
            },
            indent,
            box_height * grid.scale,
            quote,
        );
    }
}

#[cfg(test)]
mod row_kind_tests {
    use super::RowKind;

    /// `is_head` and `keep_with_next` were two adjacent `bool` parameters at the end of
    /// a nine-argument signature — transposable, same type, and behind an
    /// `#[allow(clippy::too_many_arguments)]`. Folding them into one value makes the
    /// transposition unrepresentable; this pins the mapping the two callers rely on.
    ///
    /// The pairing is not free either way: a body row that kept company with the next
    /// would make a long table indivisible, and a header that did not would let a page
    /// break fall between the header and its first row — which is the break the flag
    /// exists to prevent.
    #[test]
    fn a_header_keeps_company_with_its_body_and_a_body_row_never_does() {
        let with_body = RowKind::Head {
            keep_with_next: true,
        };
        assert!(with_body.is_head() && with_body.keep_with_next());

        // A header with no body under it: still a header, but there is nothing to keep.
        let alone = RowKind::Head {
            keep_with_next: false,
        };
        assert!(alone.is_head() && !alone.keep_with_next());

        assert!(!RowKind::Body.is_head() && !RowKind::Body.keep_with_next());
    }
}
