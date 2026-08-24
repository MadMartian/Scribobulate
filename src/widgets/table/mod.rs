//! `ScribTableWidget` — a custom, churn-free `GtkWidget` that lays out a Markdown
//! table's cells, anchored in the preview `GtkTextView` at a `GtkTextChildAnchor`.
//!
//! Why not a `GtkGrid`? An anchored *height-for-width* widget (a grid of wrapping
//! cells) re-arms the "snapshot … without a current allocation" blank: `GtkTextView`
//! validates the anchor line lazily and measures the child **at the child's own
//! minimum width, never the viewport** (`gtk_widget_get_preferred_size` →
//! `measure(V, min_width)`); any size delta from a re-measure `queue_resize`s →
//! propagates `alloc_needed` up → snapshot bails → blank. A width-clamp can never fix
//! it (the viewport width never enters that measure). See GTK4Rs/AP-23.
//!
//! This widget satisfies the only invariant that works (researcher-verified against
//! gtk-4-6 source): **report a size independent of GTK's `for_size`, and never
//! `queue_resize` at a steady bound width.** It does so by:
//!   - `request_mode = ConstantSize` and a `measure()` that returns a **cached**
//!     total (min == nat) for any `for_size`, **without measuring the cells**;
//!   - `size_allocate()` that reuses **cached cell rectangles** (an unchanged
//!     allocation is skipped by GTK, so the wrapping cell labels never re-wrap →
//!     never `queue_resize`);
//!   - a layout that is recomputed **once per real bound-width change**, driven by
//!     `set_bound_width()` (called from the view as the viewport column changes),
//!     not by GTK's validation `for_size`.
//!
//! The layout arithmetic itself (column fit + cell placement) lives in the pure,
//! GTK-free [`layout`] submodule so it can be unit-tested without a display; this
//! file owns only the GObject glue and the `measure()` calls that feed it.
//!
//! Cells stay real, selectable `GtkLabel`s with working `<a href>` links — so
//! selection (per-cell; tables are anchored islands) and links are kept,
//! unlike the rejected `GdkPaintable` route.

// `pub(crate)` for one reason worth stating: `export::pdftable` shares this module's
// column-fitting RULE (Document Rendering CAM row 17 — an export shows what the preview
// showed), and its test cross-checks the two implementations against each other. The
// widget half of this directory stays private.
pub(crate) mod layout;
mod linkcell;

pub(crate) use linkcell::{
    cell_markup_label, link_cell_button, link_cell_caption, link_markup_open, LINK_MARKUP_CLOSE,
};

use gtk::prelude::*;
use gtk::{gdk, glib};

mod imp {
    use super::*;
    use gtk::subclass::prelude::*;
    use std::cell::RefCell;

    /// One cell and its grid position.
    pub(crate) struct Cell {
        pub(crate) widget: gtk::Widget,
        pub(crate) row: usize,
        pub(crate) col: usize,
    }

    /// The whole layout result, recomputed once per real bound-width change.
    #[derive(Default)]
    pub(crate) struct Layout {
        /// The content-column width this layout was computed for; the idempotent
        /// guard that makes a re-`set_bound_width` at the same width a no-op.
        pub(crate) bound_w: i32,
        /// The widget's total (width, height) — returned verbatim by `measure`.
        pub(crate) total: (i32, i32),
        /// Per-cell allocation rectangle, parallel to `cells`.
        pub(crate) rects: Vec<gdk::Rectangle>,
    }

    #[derive(Default)]
    pub(crate) struct ScribTableWidget {
        pub(crate) cells: RefCell<Vec<Cell>>,
        pub(crate) ncols: std::cell::Cell<usize>,
        pub(crate) nrows: std::cell::Cell<usize>,
        pub(crate) layout: RefCell<Layout>,
        /// Left inset (px) this table inherits from an enclosing list item and/or
        /// blockquote — the view bounds every anchored child to `content − 1` as if it
        /// started at the content edge, but a table nested in a list/quote actually
        /// starts `inset` px further right, so `set_bound_width` subtracts this or the
        /// table overflows the viewport by `inset` px → spurious Automatic h-scrollbar →
        /// GTK4Rs/AP-22/23 churn/blank (GTK4Rs/AP-23a). Set once by the renderer at build time.
        pub(crate) inset: std::cell::Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ScribTableWidget {
        const NAME: &'static str = "ScribTableWidget";
        type Type = super::ScribTableWidget;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            // A css name so the cell-border stylesheet can target this subtree.
            klass.set_css_name("scribtable");
        }
    }

    impl ObjectImpl for ScribTableWidget {
        fn dispose(&self) {
            // Children are parented directly (no layout manager owns them).
            crate::widgets::unparent_all_children(&*self.obj());
        }
    }

    impl WidgetImpl for ScribTableWidget {
        // (1) Size is independent of `for_size` — that is the whole point.
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::ConstantSize
        }

        // (4) Return the cached total; NEVER measure cells here (a cell re-measure at
        // validation `for_size` is exactly what re-arms the blank — GTK4Rs/AP-23).
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let (w, h) = self.layout.borrow().total;
            let v = if orientation == gtk::Orientation::Horizontal {
                w
            } else {
                h
            };
            (v, v, -1, -1) // min == nat ⇒ the TextView allocates exactly this
        }

        // (5) Reuse cached rects. At a steady bound width every validation pass lands
        // here with the cached size; allocating each cell its identical rect is a
        // no-op in GTK (skipped), so the wrapping labels never re-wrap → never
        // `queue_resize` → `alloc_needed` stays FALSE → the blank cannot re-arm.
        fn size_allocate(&self, _width: i32, _height: i32, baseline: i32) {
            let cells = self.cells.borrow();
            let layout = self.layout.borrow();
            for (cell, rect) in cells.iter().zip(layout.rects.iter()) {
                cell.widget.size_allocate(rect, baseline);
            }
        }
    }
}

glib::wrapper! {
    pub(crate) struct ScribTableWidget(ObjectSubclass<imp::ScribTableWidget>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ScribTableWidget {
    /// Build a table widget from `rows` (row-major; each inner `Vec` is one row's
    /// cell widgets). The cells are parented immediately; the layout stays empty
    /// (size 0×0) until the first `set_bound_width`, after which the view's
    /// `size_allocate` gives it the viewport column width.
    pub(crate) fn new(rows: Vec<Vec<gtk::Widget>>) -> Self {
        use gtk::subclass::prelude::*;
        let obj: Self = glib::Object::new();
        let imp = obj.imp();
        let nrows = rows.len();
        let mut ncols = 0usize;
        let mut cells = Vec::new();
        for (r, row) in rows.into_iter().enumerate() {
            ncols = ncols.max(row.len());
            for (c, widget) in row.into_iter().enumerate() {
                widget.set_parent(&obj);
                cells.push(imp::Cell {
                    widget,
                    row: r,
                    col: c,
                });
            }
        }
        imp.ncols.set(ncols);
        imp.nrows.set(nrows);
        *imp.cells.borrow_mut() = cells;
        obj
    }

    /// Set the content-column width the table must fit into (from the view's live
    /// viewport). Idempotent: a no-op when unchanged (so it is safe to call on every
    /// `size_allocate`); on a real change it recomputes the column/row layout **once**
    /// and `queue_resize`s **once**. This is the ONLY place cells are measured.
    pub(crate) fn set_bound_width(&self, px: i32) {
        use gtk::subclass::prelude::*;
        let imp = self.imp();
        // Subtract the list/blockquote inset (GTK4Rs/AP-23a): the caller passes the
        // content-column width as if the table started at the content edge, but a table
        // nested in a list item / blockquote actually starts `inset` px further right, so
        // fitting it into the full column leaves it `inset` px over-wide → spurious
        // Automatic h-scrollbar → GTK4Rs/AP-22/23 churn/blank. `inset` is 0 for a top-level
        // table, so this is a no-op there.
        let effective = (px - imp.inset.get()).max(1);
        if px <= 0 || imp.layout.borrow().bound_w == effective {
            return; // unchanged ⇒ no queue_resize ⇒ no churn
        }
        let layout = self.recompute(effective);
        *imp.layout.borrow_mut() = layout;
        self.queue_resize(); // exactly once, for the real width change
    }

    /// Record the left inset (px) this table inherits from an enclosing list item and/or
    /// blockquote, so [`set_bound_width`](Self::set_bound_width) fits it into
    /// `content − inset` rather than the full column. Set once by the renderer at build
    /// time, before the first `set_bound_width` (GTK4Rs/AP-23a). Idempotent; clamped ≥ 0.
    pub(crate) fn set_bound_inset(&self, px: i32) {
        use gtk::subclass::prelude::*;
        self.imp().inset.set(px.max(0));
    }

    /// Compute column widths, cell heights (height-for-width, measured ONCE here),
    /// per-cell rectangles, and the total size — for the given content width. The
    /// GTK `measure()` calls live here; the fit and placement arithmetic they feed
    /// is delegated to the pure [`layout`] module.
    fn recompute(&self, bound_w: i32) -> imp::Layout {
        use gtk::subclass::prelude::*;
        let imp = self.imp();
        let cells = imp.cells.borrow();
        let ncols = imp.ncols.get();
        let nrows = imp.nrows.get();
        if ncols == 0 || nrows == 0 {
            return imp::Layout {
                bound_w,
                total: (0, 0),
                rects: Vec::new(),
            };
        }

        // 1. Each column's MINIMUM and NATURAL width — the max over its cells. Both
        //    include the cell's own CSS padding + border (measure accounts for them).
        //    The minimum is load-bearing: a column allocated less than a cell's minimum
        //    makes that cell OVERFLOW its column, so the table ends up a few pixels
        //    wider than the bound → the view goes over-wide → the outer Automatic h-bar
        //    appears and churns → blank (GTK4Rs/AP-23). `fit_columns` never allocates below it.
        //    Measured as PAIRS rather than two parallel vectors, so nothing downstream
        //    can transpose them — see `layout::ColumnWant`.
        let mut wants = vec![
            layout::ColumnWant {
                natural: 0,
                minimum: 0,
            };
            ncols
        ];
        for cell in cells.iter() {
            let (min_w, nat_w, _, _) = cell.widget.measure(gtk::Orientation::Horizontal, -1);
            let want = &mut wants[cell.col];
            want.minimum = want.minimum.max(min_w).max(layout::MIN_COL_WIDTH);
            want.natural = want.natural.max(nat_w);
        }

        // 2. Fit the columns into `bound_w` (the three-case water-fill — pure).
        let col_w = layout::fit_columns(&wants, bound_w);

        // 3. Each row's height — the max cell height measured AT that cell's assigned
        //    column width (height-for-width, but done once, here, not at validation).
        let mut row_h = vec![0i32; nrows];
        for cell in cells.iter() {
            let cw = col_w[cell.col];
            let (_, nat_h, _, _) = cell.widget.measure(gtk::Orientation::Vertical, cw);
            row_h[cell.row] = row_h[cell.row].max(nat_h);
        }

        // 4. Prefix sums, per-cell rects, and total size (pure).
        let positions: Vec<layout::CellPos> = cells
            .iter()
            .map(|cell| layout::CellPos {
                row: cell.row,
                col: cell.col,
            })
            .collect();
        let placement = layout::place_cells(&col_w, &row_h, &positions);

        imp::Layout {
            bound_w,
            total: placement.total,
            rects: placement
                .rects
                .iter()
                .map(|r| gdk::Rectangle::new(r.x, r.y, r.w, r.h))
                .collect(),
        }
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use gtk::subclass::prelude::*;

    /// `set_bound_inset` narrows the fit target: after `set_bound_width(px)` the table's
    /// measured width is `≤ px − inset`, so an indented table fits inside the column its
    /// enclosing list/blockquote leaves it (GTK4Rs/AP-23a). Wide cells force the fit to
    /// fill the column, so the inset is what keeps it in-bounds. Mutation: dropping the
    /// `− inset` in `set_bound_width` makes the width equal `px` and fails this.
    #[gtktest::test]
    fn set_bound_inset_shrinks_the_fit_target() {
        // Wrapping cells (like the real ones) so their MINIMUM width is small and
        // fit_columns fills the column rather than overflowing on minimums (case 2).
        let long = "A fairly long cell whose natural width exceeds a narrow column bound";
        let cell = || -> gtk::Widget {
            gtk::Label::builder()
                .label(long)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::Char)
                .build()
                .upcast()
        };
        let rows = || vec![vec![cell(), cell()], vec![cell(), cell()]];

        // No inset: the table fills the whole bound.
        let table = ScribTableWidget::new(rows());
        table.set_bound_width(400);
        let (w0, _) = table.imp().layout.borrow().total;
        // `w0 <= 400` alone cannot fail: `fit_columns` returns widths summing to at
        // most the bound by construction in both cases this setup can reach (the
        // minimums-overflow case, which CAN exceed the bound, needs minimums summing
        // past 400 — these wrapping cells sit at MIN_COL_WIDTH). So the load-bearing
        // half is the LOWER bound: the no-inset table must actually consume more than
        // the inset case's target, or the `w1 <= 350` assertion below is satisfied by
        // a table that was never wide enough to be constrained, and the pair proves
        // nothing about the inset.
        assert!(
            (351..=400).contains(&w0),
            "with no inset the table must fill its bound and exceed the inset target, \
             else the inset assertion below is vacuous (got {w0})"
        );

        // With an inset the SAME bound yields a narrower table: it must fit `400 − 50`.
        let table2 = ScribTableWidget::new(rows());
        table2.set_bound_inset(50);
        table2.set_bound_width(400);
        let (w1, _) = table2.imp().layout.borrow().total;
        assert!(
            w1 <= 350,
            "an inset of 50 must fit the table into bound−inset = 350, got {w1}"
        );
        // And it must be a genuine REDUCTION against the same bound, not merely a
        // number under 350: an implementation that ignored the bound argument and
        // always fitted to some small constant would satisfy the assertion above.
        assert!(
            w1 < w0,
            "the inset must narrow the fit target: same bound of 400, inset 50 gave \
             w1 {w1} against w0 {w0} with no inset"
        );
    }
}
