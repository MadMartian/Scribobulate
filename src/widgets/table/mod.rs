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

    /// A display-wide CSS provider, removed again when this value drops. A provider on
    /// the display is PROCESS-global state and libtest runs the whole suite in one
    /// process, so it must come off even on a panic (POLICY § Unit tests).
    struct DisplayCss(gtk::CssProvider);

    impl DisplayCss {
        fn install(css: &str) -> Self {
            Self::install_at(css, gtk::STYLE_PROVIDER_PRIORITY_USER)
        }

        /// The same, at a stated priority — for a test whose subject is the CASCADE:
        /// the app's own theme sheet goes on at `APPLICATION + 1` (`app::setup`) and a
        /// desktop GTK theme at `THEME`, so a test that models both needs to say where
        /// each of its providers sits rather than taking one fixed rung.
        fn install_at(css: &str, priority: u32) -> Self {
            let display = gdk::Display::default().expect("this test needs a display");
            let provider = gtk::CssProvider::new();
            provider.load_from_data(css);
            gtk::style_context_add_provider_for_display(&display, &provider, priority);
            Self(provider)
        }
    }

    impl Drop for DisplayCss {
        fn drop(&mut self) {
            if let Some(display) = gdk::Display::default() {
                gtk::style_context_remove_provider_for_display(&display, &self.0);
            }
        }
    }

    /// **A cell keeps the theme's ink when the window goes to the back.** The
    /// regression the operator reported: on a themed page every table cell changed
    /// colour the moment the window lost focus, while the prose around it did not.
    ///
    /// The cause is a cascade fact rather than a GTK one, and it is why no assertion on
    /// the generated sheet's TEXT can see it: a cell is a `GtkLabel`, desktop themes
    /// style that node in the backdrop state (Breeze ships
    /// `label:backdrop { color: @theme_unfocused_text_color }`), and an INHERITED value
    /// — which is all the page's `textview` rule gave a cell — loses to any declaration
    /// that MATCHES the node, from any provider. Provider priority arbitrates rules that
    /// match; it cannot rescue one that does not (GTK4Rs/AP-101, ScrAP-127).
    ///
    /// So the hostile rule here sits *below* this test's own sheet and still wins until
    /// the cell's ink is stated. The control cell is the discriminator: it proves the
    /// backdrop rule really reaches a cell of this shape, so the assertion after it is
    /// about the fix rather than about a rule that never applied.
    ///
    /// **Both priorities are stated relative to the app's own provider rather than at
    /// `PRIORITY_THEME`, where a real desktop theme sits.** `app::setup`'s theme provider
    /// is installed on the display once per PROCESS and never removed, so in a full-suite
    /// run any earlier test that reloaded it leaves a themed sheet — including a
    /// `scribtable .cell` ink of its own — live at `APPLICATION + 1`. A hostile rule
    /// underneath that is simply outranked, and the control then reads the leftover
    /// theme's ink and fails on its own precondition (MEASURED: `#5b4636`, Sepia's, in a
    /// suite run that passed in isolation). Priority is not what this test is about;
    /// stacking both rules above the ambient provider keeps the subject — inheritance
    /// versus a matching rule — the only thing the outcome can turn on.
    ///
    /// **The control is a SECOND cell, not a second reading of the first**, and that is
    /// not tidiness: an UNROOTED widget's computed style is cached at the first read and
    /// a provider added afterwards does not invalidate it (no frame clock to service the
    /// invalidation), so a read/install/read on one label reports the hostile colour
    /// twice and reads as a broken fix. Each reading gets a freshly built cell.
    ///
    /// Mutation: dropping `cell_ink` from `preview::css`'s `.cell` rule fails this.
    /// TDD 18.52.
    #[gtktest::test]
    fn a_cell_keeps_the_themes_ink_in_the_backdrop_state() {
        use crate::palette::Palette;

        // A desktop theme's backdrop ink, in a colour nothing else here can produce.
        const HOSTILE: &str = "label:backdrop { color: #ff0000; }";

        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test("[themes.probe]\nforeground = \"#33ddaa\"\n");
        let theme = themes.resolve("probe");
        let palette = Palette::for_paper(&theme);
        let want = theme
            .foreground
            .expect("the probe theme states a foreground");

        // A plain cell in an unfocused table, built as `renderer::end` builds one.
        // BACKDROP is an inherited state flag, so setting it on the table reaches the
        // cell — which is what a window losing focus does to every widget it holds.
        let backdrop_cell = || {
            let cell = gtk::Label::new(Some("Core GTK4"));
            cell.add_css_class("cell");
            let table = ScribTableWidget::new(vec![vec![cell.clone().upcast()]]);
            table.set_state_flags(gtk::StateFlags::BACKDROP, false);
            (table, cell)
        };

        // +2 and +3: above the app's own provider (+1, see above), hostile below ours.
        let _hostile =
            DisplayCss::install_at(HOSTILE, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 2);
        let (_control_table, control) = backdrop_cell();
        let hijacked = control.style_context().color();
        assert_eq!(
            (hijacked.red(), hijacked.green(), hijacked.blue()),
            (1.0, 0.0, 0.0),
            "fixture no longer discriminates: a desktop theme's `label:backdrop` rule \
             must actually reach this cell, or the assertion below proves nothing"
        );

        let _theme_css = DisplayCss::install_at(
            &crate::preview::theme_css(&theme, &palette),
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 3,
        );
        let (_table, cell) = backdrop_cell();
        let inked = cell.style_context().color();
        assert_eq!(
            (inked.red(), inked.green(), inked.blue()),
            (want.red(), want.green(), want.blue()),
            "an unfocused window's table cell fell back to the desktop theme's backdrop \
             ink — the page's own `color` only INHERITS to a cell, and inheritance loses \
             to a rule that matches the label node"
        );
    }

    /// **The preview's link-cell rules reach the widget** — asserted on the colour the
    /// button node RESOLVES to, which is the only observable that can see whether a
    /// selector matched anything.
    ///
    /// TDD 18.45. Every other test of these rules asserts on the generated stylesheet
    /// TEXT, and that is a check of the same defect one layer up: a blanket rename of the
    /// theme vocabulary turned `scribtable button.cell.link` — where `link` is GTK's own
    /// class on `GtkLinkButton`, not this project's `link_color` key — into
    /// `scribtable button.cell.link_color`, a perfectly well-formed selector matching
    /// nothing. The rule still generated, every text assertion still passed, and a
    /// link-only cell silently reverted to the desktop's link colour beside a mixed cell
    /// that stayed themed.
    ///
    /// Two readings, and the first is what makes the second mean anything: the same
    /// button is read BEFORE the provider goes on, so a colour that happened to match by
    /// coincidence would fail the fixture rather than pass the test.
    ///
    /// **What this cannot reach**, stated rather than papered over: the third selector,
    /// `scribtable button.cell.link label`, exists for `text-decoration-*`, which does
    /// not inherit to the caption and which gtk4-rs exposes no style-context accessor
    /// for. Its effect is covered by the driven pixel comparison recorded at TDD 18.45,
    /// not here — adding another rule-text assertion for it would be the very thing this
    /// test exists to stop.
    #[gtktest::test]
    fn a_link_only_cells_button_wears_the_themes_link_colour() {
        use crate::palette::Palette;

        // A link colour no fallback theme is going to land on by accident, and one that
        // is not the theme's body ink either — so `color` inherited from an ancestor
        // cannot satisfy the assertion.
        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test("[themes.probe]\nlink_color = \"#2de1ff\"\n");
        let theme = themes.resolve("probe");
        let palette = Palette::for_paper(&theme);
        let want = palette.link_fg;

        // Built exactly as `preview::build` builds a pure-link cell.
        let link = crate::widgets::table::link_cell_button(
            "https://example.com/handbook",
            "Handbook",
            crate::mdtable::Align::Left,
        );
        link.set_has_frame(false);
        link.add_css_class("cell");
        let _table = ScribTableWidget::new(vec![vec![link.clone().upcast()]]);

        let before = link.style_context().color();
        assert_ne!(
            (before.red(), before.green(), before.blue()),
            (want.red(), want.green(), want.blue()),
            "fixture no longer discriminates: the ambient theme already paints this \
             button the probe theme's link colour, so the assertion below cannot fail"
        );

        let _css = DisplayCss::install(&crate::preview::theme_css(&theme, &palette));
        let after = link.style_context().color();
        assert_eq!(
            (after.red(), after.green(), after.blue()),
            (want.red(), want.green(), want.blue()),
            "the preview's link-cell rule did not reach a pure-link cell's button — a \
             selector that matches nothing generates and asserts exactly like one that \
             matches, so this is the only place the difference is visible"
        );
    }

    /// **A pure-link cell's border box fills its column** — the live half of the
    /// regression the operator reported: a link cell's `.cell` border shrink-wrapped to
    /// its caption and floated inside the column, so the table's vertical rules moved
    /// from row to row while the text cells beside it stayed flush.
    ///
    /// The cause is not decidable from data — it is what GTK does with a non-Fill
    /// `halign` at allocation time (the widget is given only its natural width), so the
    /// oracle has to be a real allocation. Mutation: restoring
    /// `btn.set_halign(Align::Center)` at the cell's construction fails this.
    #[gtktest::test]
    fn a_link_cells_border_box_fills_its_column() {
        // A wide text cell above a narrow link cell in the SAME column, so the column
        // is fitted much wider than the link caption wants — the only shape in which
        // a shrink-wrapped box is distinguishable from a filled one.
        let wide: gtk::Widget = gtk::Label::builder()
            .label("a deliberately wide header cell")
            .build()
            .upcast();
        let link = crate::widgets::table::link_cell_button(
            "https://example.com/1",
            "#295",
            crate::mdtable::Align::Center,
        );
        // Built exactly as the renderer builds it — a framed button carries the
        // theme's own button chrome, which is not what a table cell is.
        link.set_has_frame(false);
        link.add_css_class("cell");
        let link_w: gtk::Widget = link.clone().upcast();
        let table = ScribTableWidget::new(vec![vec![wide], vec![link_w]]);
        table.set_bound_width(600);

        let (total_w, total_h) = table.imp().layout.borrow().total;
        // The link cell is the only cell in row 1, so its slot IS the column.
        let col_w = table.imp().layout.borrow().rects[1].width();
        table.allocate(total_w, total_h, -1, None);

        // Two oracles, because the ambient theme has a say in the second. The table
        // hands the cell its whole grid slot — that is the decision under test, and it
        // is exact. What the cell's CSS box then does with that slot includes any
        // margin the desktop theme puts on a `button` node (14px under the test
        // environment's fallback theme, 0 under the app's own sheet), so the box is
        // checked against the caption it must NOT be shrink-wrapped to instead.
        let (_, nat_w, _, _) = link.measure(gtk::Orientation::Horizontal, -1);
        assert_eq!(
            link.allocation().width(),
            col_w,
            "the link cell must be allocated its whole grid slot; a non-Fill halign \
             shrinks it to the caption and the column rule breaks"
        );
        assert!(
            link.width() > nat_w,
            "the cell's CSS box ({}px) must span more than its caption wants ({nat_w}px) \
             — a box at the caption's own width IS the shrink-wrapped border",
            link.width()
        );
        // And neither assertion is vacuous: the slot must be genuinely wider than the
        // caption wants, or a shrink-wrapped box would satisfy them too.
        assert!(
            nat_w < col_w,
            "fixture no longer discriminates: the caption's natural width {nat_w} must \
             be under the fitted column width {col_w}"
        );
    }

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
