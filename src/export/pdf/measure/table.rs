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
use super::super::geometry::{pango_to_pt, PT_PER_PX};
use super::super::pdftable;
use super::super::{
    CellWidths, LayoutSpec, LineKind, TableCell, BASE_PT, BLOCK_GAP_PT, PANGO_WEIGHT_NORMAL,
};
use super::Layouter;
use gtk::pango;

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
        quote_depth: u32,
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
        if !head_markup.is_empty() {
            self.table_row(
                &head_markup,
                aligns,
                &grid,
                &chrome,
                indent,
                quote_depth,
                true,
                // The header keeps company with the first body row.
                !rows.is_empty(),
            );
        }
        for row in &body_markup {
            self.table_row(
                row,
                aligns,
                &grid,
                &chrome,
                indent,
                quote_depth,
                false,
                false,
            );
        }
    }

    /// The theme's table chrome in points — no literal, per THEMING.md.
    fn table_chrome(&self) -> pdftable::Chrome {
        let m = &self.theme.metrics;
        pdftable::Chrome {
            padding_h: f64::from(m.table_cell_padding_h) * PT_PER_PX,
            padding_v: f64::from(m.table_cell_padding_v) * PT_PER_PX,
            border: f64::from(m.table_border_width) * PT_PER_PX,
        }
    }

    /// One row's cells as Pango markup, bolded when it is the header.
    fn row_markup(&self, cells: &[Vec<Inline>], doc: &ExportDoc, head: bool) -> Vec<String> {
        cells
            .iter()
            .map(|cell| {
                let markup = inline_markup(cell, doc, self.theme);
                if head {
                    format!("<b>{markup}</b>")
                } else {
                    markup
                }
            })
            .collect()
    }

    /// How wide a cell wants to be, both ways.
    fn cell_widths(&self, markup: &str) -> CellWidths {
        let layout = self.cell_layout(markup, None, Align::None);
        let max = pango_to_pt(layout.extents().1.width());

        // Min-content: squeeze the layout to nothing in `Word` mode, which refuses to
        // break inside a word, so the widest line that comes back IS the widest word.
        // `WordChar` would happily split one and report a meaningless 1pt.
        layout.set_wrap(pango::WrapMode::Word);
        layout.set_width(1);
        let min = pango_to_pt(layout.extents().1.width());

        CellWidths { max, min }
    }

    /// A layout for one cell. `text_width` of `None` measures unconstrained; `Some`
    /// constrains it, which is what makes the text wrap **inside its column**.
    fn cell_layout(&self, markup: &str, text_width: Option<f64>, align: Align) -> pango::Layout {
        self.layout_of(
            markup,
            LayoutSpec {
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
    #[allow(clippy::too_many_arguments)]
    fn table_row(
        &mut self,
        cells: &[String],
        aligns: &[Align],
        grid: &pdftable::Grid,
        chrome: &pdftable::Chrome,
        indent: f64,
        quote_depth: u32,
        is_head: bool,
        keep_with_next: bool,
    ) {
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
            let height = pango_to_pt(layout.extents().1.height());
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
                is_head,
            },
            Fragment {
                height: box_height * grid.scale,
                space_before: if is_head { BLOCK_GAP_PT } else { 0.0 },
                keep_with_next,
            },
            indent,
            box_height * grid.scale,
            quote_depth,
        );
    }
}
