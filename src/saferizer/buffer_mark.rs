//! The persisted-`GtkTextMark` resolution seam.

use gtk::glib;
use gtk::prelude::*;

/// A `GtkTextMark` paired with the buffer it was created on, so that resolving it
/// back to an iter can be gated on the mark still belonging to the buffer being
/// resolved against (ScrAP-104).
///
/// # Contract
///
/// GTK 4.6's `gtk_text_buffer_get_iter_at_mark` performs **no** mark∈buffer check:
/// resolving a foreign or orphaned mark against the wrong buffer drives it into an
/// uninitialised iter, and the first geometry read on that iter aborts the process
/// (`gtk_text_btree_line_number couldn't find line`). A mark is orphaned whenever a
/// `set_buffer` swap finalizes the buffer it lived on — which is exactly the window
/// between scheduling a deferred scroll and its idle firing.
///
/// These marks are **persisted deliberately** — a left-gravity `GtkTextMark`
/// survives edits at its anchor position where a plain char offset would drift — so
/// the fix is to carry the mark, not to downgrade it to an offset. [`resolve`] (and
/// [`scroll_mark`], its mark-returning sibling for the `scroll_to_mark` path) are the
/// **only** safe way to turn a persisted mark back into a usable iter/mark: each
/// returns `None` when `against` is not the mark's current owning buffer, and only
/// then performs the otherwise-fatal resolution.
///
/// [`resolve`]: BufferMark::resolve
/// [`scroll_mark`]: BufferMark::scroll_mark
pub(crate) struct BufferMark {
    mark: gtk::TextMark,
    /// The owning buffer, held **weakly** on purpose: a strong clone would keep the
    /// old buffer alive across a `set_buffer` swap, resurrecting it and masking the
    /// mark orphaning this seam exists to detect — the guard would then never see the
    /// finalize it guards against. A weak ref lets the buffer finalize normally and
    /// `upgrade()` to `None`, which membership treats as "not current".
    buffer: glib::WeakRef<gtk::TextBuffer>,
}

impl BufferMark {
    /// Capture a mark together with the buffer that owns it.
    pub(crate) fn new(mark: gtk::TextMark, buffer: &gtk::TextBuffer) -> Self {
        Self {
            mark,
            buffer: buffer.downgrade(),
        }
    }

    /// True only when the mark still lives in `against` and `against` is the buffer
    /// captured at construction. `mark.buffer()` re-queries the mark's **live** owner
    /// (`None` once a `set_buffer` finalize has orphaned it), which is the exact
    /// predicate the hand-copied inline guards used; the captured-buffer identity is
    /// confirmed too (its `WeakRef` likewise `upgrade`s to `None` once the buffer
    /// finalizes), since a mark that no longer belongs to that buffer must not be
    /// resolved against it.
    fn is_current(&self, against: &gtk::TextBuffer) -> bool {
        self.mark.buffer().as_ref() == Some(against)
            && self.buffer.upgrade().as_ref() == Some(against)
    }

    /// Resolve the mark to an iter in `against`, or `None` if `against` is not the
    /// mark's current owning buffer. The `None` arm is the safe fallback: the caller
    /// skips work rather than aborting on a foreign/orphaned mark (ScrAP-104).
    pub(crate) fn resolve(&self, against: &impl IsA<gtk::TextBuffer>) -> Option<gtk::TextIter> {
        let against = against.upcast_ref::<gtk::TextBuffer>();
        self.is_current(against)
            .then(|| against.iter_at_mark(&self.mark))
    }

    /// The mark itself, but only when it is safe to resolve against `against` — for
    /// the `scroll_to_mark` path, which needs the `GtkTextMark` (not an iter) because
    /// `scroll_to_mark` re-applies its target across GtkTextView's lazy height
    /// validation where a one-shot `scroll_to_iter` would land pre-validation (ScrAP-22).
    /// `None` carries the same membership guarantee as [`resolve`](Self::resolve):
    /// `scroll_to_mark` internally resolves the mark via the same unchecked path.
    pub(crate) fn scroll_mark(
        &self,
        against: &impl IsA<gtk::TextBuffer>,
    ) -> Option<&gtk::TextMark> {
        let against = against.upcast_ref::<gtk::TextBuffer>();
        self.is_current(against).then_some(&self.mark)
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// A mark created in buffer A, resolved against a DIFFERENT buffer B, must return
    /// `None` — never abort — and against its own buffer A must resolve to the mark's
    /// offset. This is the ScrAP-104 crash (`gtk_text_btree_line_number couldn't find
    /// line`) reduced to a unit: removing the `is_current` membership check makes the
    /// cross-buffer arm abort the process (verified by mutation per GTK4Rs/AP-78).
    #[gtktest::test]
    fn resolve_is_none_across_buffers_and_some_within() {
        let a = gtk::TextBuffer::new(None);
        a.set_text("line 0\nline 1\nline 2\n");
        let b = gtk::TextBuffer::new(None);
        b.set_text("other\ntext\n");

        let iter = a.iter_at_line(2).unwrap();
        let mark = a.create_mark(Some("test-mark"), &iter, true);
        let bmark = BufferMark::new(mark, &a);

        // Foreign buffer: the fatal case — must be a clean None, not an abort.
        assert!(
            bmark.resolve(&b).is_none(),
            "resolving a mark from buffer A against buffer B must return None"
        );
        assert!(
            bmark.scroll_mark(&b).is_none(),
            "scroll_mark must withhold the mark from a foreign buffer"
        );

        // Owning buffer: resolves to the mark's own offset.
        let resolved = bmark
            .resolve(&a)
            .expect("resolving against the owning buffer must yield the iter");
        assert_eq!(
            resolved.offset(),
            a.iter_at_line(2).unwrap().offset(),
            "the resolved iter must sit at the mark's offset"
        );
        assert!(
            bmark.scroll_mark(&a).is_some(),
            "scroll_mark must return the mark for its owning buffer"
        );
    }
}
