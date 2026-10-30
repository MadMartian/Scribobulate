//! `ToolbarWrapBox` — a small custom `GtkWidget` that lays its children out
//! left-to-right, wrapping onto additional rows when a child would overflow the
//! allocated width, and packing every row flush against the LEFT edge.
//!
//! Built to replace an earlier `GtkFlowBox`-based attempt at this (the
//! behavioural contract is TDD 9.38): `GtkFlowBox` computes a shared column grid
//! across every line it lays out, so a later row whose items differ in size
//! from the row(s) that set that grid gets offset/padded rather than packed
//! tightly left — reported as odd gaps opening up on the left of a wrapped
//! row. This widget does the placement itself with a plain greedy
//! left-to-right, top-to-bottom bin-pack (no shared grid, no per-line
//! justification), so there is nothing to misalign: `size_allocate` runs the
//! exact same arithmetic `measure` used to decide the required height, so the
//! two can never disagree.
//!
//! Children keep their own natural size — never stretched, never shrunk. A
//! child that would overflow the current row moves whole to the next one,
//! never squeezed to fit. Invisible children (`set_visible(false)`) are
//! skipped entirely and take no space, exactly like a plain `GtkBox`, so
//! hiding a toolbar section still costs it nothing here. A popover parented here
//! (the toolbar's right-click menu) is skipped the same way — it is drawn on its own
//! surface — and is presented on each allocation so it follows the bar.
//!
//! A child SHORTER than its row is placed by its own `valign`: `Start` at the
//! row's top, `End` at its bottom, and anything else — including GTK's default
//! `Fill` — centred. `Fill` centres rather than stretching because this widget
//! never stretches a child; that is the same rule as the width, stated for the
//! other axis.
//!
//! This is the widget's job rather than the caller's, and the reason is that
//! `gtk_widget_set_valign` has nothing to work with here on its own: a child is
//! allocated exactly its natural height, so GTK is given no slack to place it
//! in, and the property is inert unless this code reads it. Every toolbar row
//! was uniform-height buttons for the life of this widget, so none of it
//! mattered until the find bar gained controls of two different heights — a
//! `GtkCheckButton` shorter than a button, then a three-row option grid beside
//! a one-row entry. Centring is the default rather than baseline alignment
//! because the rows here are not all text (icons, entries, a check indicator),
//! and a row with no baseline to share still has a middle.

use gtk::prelude::*;
use gtk::{gdk, glib};

mod imp {
    use super::*;
    use gtk::subclass::prelude::*;
    use std::cell::Cell;

    #[derive(Default)]
    pub(crate) struct ToolbarWrapBox {
        pub(crate) column_spacing: Cell<i32>,
        pub(crate) row_spacing: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ToolbarWrapBox {
        const NAME: &'static str = "ScribToolbarWrapBox";
        type Type = super::ToolbarWrapBox;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("toolbarwrap");
        }
    }

    impl ObjectImpl for ToolbarWrapBox {
        fn dispose(&self) {
            // Children are parented directly (no layout manager owns them) — same
            // contract as `ScribTableWidget`; see `widgets::unparent_all_children`.
            crate::widgets::unparent_all_children(&*self.obj());
        }
    }

    impl WidgetImpl for ToolbarWrapBox {
        // Height depends on the width we're given (more width ⇒ fewer wrapped
        // rows ⇒ less height) — the opposite of `ScribTableWidget`'s
        // `ConstantSize`, which exists specifically to defeat this negotiation.
        // Here the negotiation IS the wrap.
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let children = visible_children(&self.obj());
            let col_sp = self.column_spacing.get();
            let row_sp = self.row_spacing.get();
            match orientation {
                gtk::Orientation::Horizontal => {
                    // Minimum: the widest single child — with unlimited height,
                    // every child could get its own row. Natural: every child on
                    // one row (what a wide-enough window shows), matching what a
                    // non-wrapping GtkBox would have reported.
                    let mut min_w = 0;
                    let mut sum_w = 0;
                    for (i, child) in children.iter().enumerate() {
                        let (_, nat_w, _, _) = child.measure(gtk::Orientation::Horizontal, -1);
                        min_w = min_w.max(nat_w);
                        if i > 0 {
                            sum_w += col_sp;
                        }
                        sum_w += nat_w;
                    }
                    // Given a height, the minimum is the narrowest width whose rows fit
                    // in it, or the height-for-width answer contradicts this one. GTK
                    // 4.24 checks the pair and warns ("Widget reports min width of …
                    // for height of …, but min height of … for width of …").
                    if for_size > 0 {
                        min_w =
                            narrowest_fitting(&children, for_size, min_w, sum_w, col_sp, row_sp);
                    }
                    (min_w, sum_w.max(min_w), -1, -1)
                }
                gtk::Orientation::Vertical => {
                    let h = if for_size <= 0 {
                        // No width to wrap against yet — the natural, one-row case.
                        children
                            .iter()
                            .map(|c| c.measure(gtk::Orientation::Vertical, -1).1)
                            .max()
                            .unwrap_or(0)
                    } else {
                        pack(&children, for_size, col_sp, row_sp).1
                    };
                    (h, h, -1, -1)
                }
                _ => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, _height: i32, baseline: i32) {
            let children = visible_children(&self.obj());
            let col_sp = self.column_spacing.get();
            let row_sp = self.row_spacing.get();
            let (rects, _) = pack(&children, width.max(1), col_sp, row_sp);
            for (child, rect) in children.iter().zip(rects.iter()) {
                child.size_allocate(rect, baseline);
            }
            // A popover parented here (the toolbar's right-click menu) is laid out on
            // its own surface, never by the pack above; GTK asks the parent to present
            // it on each allocation so it follows the parent.
            let mut next = self.obj().first_child();
            while let Some(child) = next {
                if let Some(popover) = child.downcast_ref::<gtk::Popover>() {
                    if popover.is_visible() {
                        popover.present();
                    }
                }
                next = child.next_sibling();
            }
        }
    }

    fn visible_children(widget: &super::ToolbarWrapBox) -> Vec<gtk::Widget> {
        let mut out = Vec::new();
        let mut next = widget.first_child();
        while let Some(child) = next {
            // `should_layout`, not `is_visible`: it is also false for a `GtkNative`
            // child — a popover parented here — which must take no slot in the pack.
            if child.should_layout() {
                out.push(child.clone());
            }
            next = child.next_sibling();
        }
        out
    }

    /// The narrowest width in `widest..=one_row` at which [`pack`] fits in `height`, or
    /// `one_row` when even a single row does not. Fewer rows never need more height, so
    /// the fit only improves with width and a binary search finds the edge.
    fn narrowest_fitting(
        children: &[gtk::Widget],
        height: i32,
        widest: i32,
        one_row: i32,
        col_spacing: i32,
        row_spacing: i32,
    ) -> i32 {
        let fits = |width: i32| pack(children, width, col_spacing, row_spacing).1 <= height;
        if fits(widest) || !fits(one_row) {
            return if fits(widest) { widest } else { one_row };
        }
        let (mut too_narrow, mut wide_enough) = (widest, one_row);
        while wide_enough - too_narrow > 1 {
            let mid = too_narrow + (wide_enough - too_narrow) / 2;
            if fits(mid) {
                wide_enough = mid;
            } else {
                too_narrow = mid;
            }
        }
        wide_enough
    }

    /// Greedy left-to-right, top-to-bottom pack at `avail_width`: place each
    /// child at its natural size, starting a new row whenever the next child
    /// would overflow. Returns each child's allocation (parallel to `children`)
    /// and the total height used.
    ///
    /// The one seam `measure(Vertical, for_size)` and `size_allocate` share, so
    /// the height `measure` promises and the rows `size_allocate` actually lays
    /// out can never drift apart from each other.
    fn pack(
        children: &[gtk::Widget],
        avail_width: i32,
        col_spacing: i32,
        row_spacing: i32,
    ) -> (Vec<gdk::Rectangle>, i32) {
        let mut rects: Vec<gdk::Rectangle> = Vec::with_capacity(children.len());
        let mut x = 0;
        let mut y = 0;
        let mut row_h = 0;
        // Where the row being built starts in `rects`. A row's height is only
        // known once the row is closed, so the centring is applied then rather
        // than as each child is placed.
        let mut row_start = 0usize;

        // Place each child of a finished row within the row's height, by its own
        // `valign`. A child exactly as tall as its row does not move whatever it
        // asks for, which is every child of a row of uniform buttons.
        fn place_row(rects: &mut [gdk::Rectangle], children: &[gtk::Widget], row_h: i32) {
            for (rect, child) in rects.iter_mut().zip(children) {
                let slack = row_h - rect.height();
                let dy = match child.valign() {
                    gtk::Align::Start => 0,
                    gtk::Align::End => slack,
                    // Center, Fill and Baseline all land here. Fill is GTK's
                    // default and is treated as Center deliberately — see the
                    // module header.
                    _ => slack / 2,
                };
                rect.set_y(rect.y() + dy);
            }
        }

        for child in children {
            let (_, nat_w, _, _) = child.measure(gtk::Orientation::Horizontal, -1);
            let (_, nat_h, _, _) = child.measure(gtk::Orientation::Vertical, -1);
            let would_overflow = x > 0 && x + col_spacing + nat_w > avail_width;
            if would_overflow {
                place_row(&mut rects[row_start..], &children[row_start..], row_h);
                y += row_h + row_spacing;
                x = 0;
                row_h = 0;
                row_start = rects.len();
            }
            let cx = if x == 0 { 0 } else { x + col_spacing };
            rects.push(gdk::Rectangle::new(cx, y, nat_w, nat_h));
            x = cx + nat_w;
            row_h = row_h.max(nat_h);
        }
        place_row(&mut rects[row_start..], &children[row_start..], row_h);
        let total_h = if rects.is_empty() { 0 } else { y + row_h };
        (rects, total_h)
    }
}

glib::wrapper! {
    pub(crate) struct ToolbarWrapBox(ObjectSubclass<imp::ToolbarWrapBox>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ToolbarWrapBox {
    pub(crate) fn new(column_spacing: i32, row_spacing: i32) -> Self {
        use gtk::subclass::prelude::*;
        let obj: Self = glib::Object::new();
        obj.imp().column_spacing.set(column_spacing);
        obj.imp().row_spacing.set(row_spacing);
        obj
    }

    /// Append `child` as the next flow item, in order — parented directly
    /// (this widget has no layout manager of its own; see `imp::dispose`).
    pub(crate) fn append(&self, child: &impl IsA<gtk::Widget>) {
        child.set_parent(self);
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::ToolbarWrapBox;
    use gtk::prelude::*;

    /// A child too wide to share a row with the previous one drops to its own
    /// row, flush against the LEFT edge — never centered/offset the way the
    /// `GtkFlowBox` predecessor was reported doing.
    #[gtktest::test]
    fn a_child_that_does_not_fit_wraps_to_a_new_row_at_x_zero() {
        let wrap = ToolbarWrapBox::new(2, 0);
        let a = gtk::Button::with_label("AAAA");
        let b = gtk::Button::with_label("BBBB");
        wrap.append(&a);
        wrap.append(&b);

        let (_, a_nat_w, _, _) = a.measure(gtk::Orientation::Horizontal, -1);
        let (_, b_nat_w, _, _) = b.measure(gtk::Orientation::Horizontal, -1);
        // Wide enough for `a` alone, too narrow for both side by side.
        let width = a_nat_w + 2;
        assert!(
            width < a_nat_w + 2 + b_nat_w,
            "precondition: the two buttons must not fit on one row at this width"
        );

        let win = gtk::Window::new();
        win.set_child(Some(&wrap));
        win.present();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(200),
        );
        // Drive the wrap box's own allocation directly at the exact tight
        // width under test, rather than trusting the window to shrink that
        // far on its own (its own chrome/minimum can keep it wider).
        wrap.size_allocate(&gtk::Allocation::new(0, 0, width, 100), -1);

        let a_alloc = a.allocation();
        let b_alloc = b.allocation();
        assert_eq!(a_alloc.x(), 0, "the first row's item starts at x=0");
        assert_eq!(
            b_alloc.x(),
            0,
            "a wrapped row starts flush at x=0 — no inherited offset from the \
             previous row's column grid (the exact `GtkFlowBox` symptom this \
             widget replaces)"
        );
        assert!(
            b_alloc.y() > a_alloc.y(),
            "the overflowing child must land on a NEW row, below the first"
        );
    }

    /// **The width it asks for at a height fits in that height.** Given one row's height,
    /// the minimum width is the whole row, not the widest child: at the widest child's
    /// width the children wrap and need two rows. The two answers are checked against
    /// each other here, on every GTK; GTK 4.24 checks the same pair itself and warns.
    #[gtktest::test]
    fn the_minimum_width_for_a_height_fits_in_that_height() {
        let wrap = ToolbarWrapBox::new(2, 0);
        for label in ["AAAA", "BBBB", "CCCC"] {
            wrap.append(&gtk::Button::with_label(label));
        }
        let (widest, one_row_w, _, _) = wrap.measure(gtk::Orientation::Horizontal, -1);
        let (one_row_h, _, _, _) = wrap.measure(gtk::Orientation::Vertical, one_row_w);
        let (stacked_h, _, _, _) = wrap.measure(gtk::Orientation::Vertical, widest);
        assert!(
            stacked_h > one_row_h,
            "precondition: at the widest child's width the children wrap"
        );

        for height in [one_row_h, stacked_h, stacked_h + 10] {
            let (min_w, nat_w, _, _) = wrap.measure(gtk::Orientation::Horizontal, height);
            let (min_h, _, _, _) = wrap.measure(gtk::Orientation::Vertical, min_w);
            assert!(
                min_h <= height,
                "min width {min_w} for height {height}, but min height {min_h} at that width"
            );
            assert!(min_w <= nat_w, "minimum {min_w} above natural {nat_w}");
        }
        let (min_w, _, _, _) = wrap.measure(gtk::Orientation::Horizontal, one_row_h);
        assert_eq!(
            min_w, one_row_w,
            "one row's height needs the whole row's width"
        );
        let (min_w, _, _, _) = wrap.measure(gtk::Orientation::Horizontal, stacked_h);
        assert_eq!(
            min_w, widest,
            "room for every row needs only the widest child"
        );
    }

    /// A hidden child is skipped entirely — no gap, no reserved row.
    #[gtktest::test]
    fn a_hidden_child_takes_no_space() {
        let wrap = ToolbarWrapBox::new(2, 0);
        let a = gtk::Button::with_label("AAAA");
        let hidden = gtk::Button::with_label("HIDDEN");
        hidden.set_visible(false);
        let b = gtk::Button::with_label("BBBB");
        wrap.append(&a);
        wrap.append(&hidden);
        wrap.append(&b);

        let (_, nat_w, _, _) = wrap.measure(gtk::Orientation::Horizontal, -1);

        let win = gtk::Window::new();
        win.set_child(Some(&wrap));
        win.present();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(200),
        );
        wrap.size_allocate(&gtk::Allocation::new(0, 0, nat_w, 100), -1);

        let a_alloc = a.allocation();
        let b_alloc = b.allocation();
        assert_eq!(
            a_alloc.y(),
            b_alloc.y(),
            "both visible children share one row"
        );
        assert_eq!(
            b_alloc.x(),
            a_alloc.x() + a_alloc.width() + 2,
            "the hidden middle child must not leave a gap or shift `b` further \
             than `a`'s own width plus spacing"
        );
    }

    /// A popover parented to the box — the toolbar's right-click menu — takes no slot,
    /// open or closed: it adds nothing to the box's width and moves no sibling.
    #[gtktest::test]
    fn a_parented_popover_takes_no_space() {
        let wrap = ToolbarWrapBox::new(2, 0);
        let a = gtk::Button::with_label("AAAA");
        wrap.append(&a);
        let (_, alone_w, _, _) = wrap.measure(gtk::Orientation::Horizontal, -1);

        let popover = gtk::Popover::new();
        popover.set_child(Some(&gtk::Label::new(Some("a wide popover body"))));
        popover.set_parent(&wrap);
        let b = gtk::Button::with_label("BBBB");
        wrap.append(&b);

        let win = gtk::Window::new();
        win.set_child(Some(&wrap));
        win.present();
        popover.popup();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(200),
        );
        assert!(popover.is_visible(), "precondition: the popover is open");
        let (_, nat_w, _, _) = wrap.measure(gtk::Orientation::Horizontal, -1);
        let (_, b_w, _, _) = b.measure(gtk::Orientation::Horizontal, -1);
        assert_eq!(
            nat_w,
            alone_w + 2 + b_w,
            "the open popover must not be measured as a pack item"
        );
        wrap.size_allocate(&gtk::Allocation::new(0, 0, nat_w, 100), -1);
        assert_eq!(
            b.allocation().x(),
            a.allocation().width() + 2,
            "the popover between `a` and `b` must not shift `b`"
        );
        popover.popdown();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(600),
        );
        popover.unparent();
    }

    /// A child shorter than its row sits in the MIDDLE of it, not at the top.
    ///
    /// The subject is a `GtkCheckButton` rather than a contrived short widget,
    /// because the check button is what revealed this: it is the first control
    /// in any toolbar row here that is not a button, and its label rode above
    /// the toggle labels beside it. Asserting on centres rather than on `y`
    /// keeps the check passing if either control's natural height changes with
    /// a theme — what must hold is that they agree, not what they measure.
    #[gtktest::test]
    fn a_short_child_is_centred_in_its_row_rather_than_riding_at_the_top() {
        let wrap = ToolbarWrapBox::new(4, 4);
        let tall = gtk::Button::with_label("Reg-Ex");
        let short = gtk::CheckButton::with_label("Search in selection");
        wrap.append(&tall);
        wrap.append(&short);

        let win = gtk::Window::new();
        win.set_child(Some(&wrap));
        win.present();
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(200),
        );
        let (_, nat_w, _, _) = wrap.measure(gtk::Orientation::Horizontal, -1);
        wrap.size_allocate(&gtk::Allocation::new(0, 0, nat_w, 100), -1);

        let tall_alloc = tall.allocation();
        let short_alloc = short.allocation();
        assert!(
            short_alloc.height() < tall_alloc.height(),
            "precondition: the check button must be the shorter of the two — \
             if a theme ever makes them equal this test proves nothing, so it \
             says so rather than passing vacuously"
        );
        let tall_centre = tall_alloc.y() + tall_alloc.height() / 2;
        let short_centre = short_alloc.y() + short_alloc.height() / 2;
        assert!(
            (tall_centre - short_centre).abs() <= 1,
            "the short child's centre ({short_centre}) must line up with the \
             row's ({tall_centre}); off by more than rounding means it is \
             riding at the row's top edge"
        );
    }
}
