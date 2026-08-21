//! What this application puts **on** a clipboard: plain text, never a `GtkTextBuffer`.
//!
//! # Why this module exists
//!
//! By default `GtkTextView` publishes rich content — a `GtkTextBuffer` carrying
//! GtkSourceView's syntax-highlight tags — to both the CLIPBOARD and, at realize, the
//! PRIMARY selection. Pasting that back into the application does **not** insert a
//! string: `gtk_text_buffer_paste_clipboard` takes a local-value shortcut, hands the
//! buffer straight to `insert_range_not_inside_self`, and that walks the copied region
//! with `gtk_text_iter_forward_to_tag_toggle`, emitting **one `insert-text` per
//! tag-delimited run**.
//!
//! That chunking is the root of an entire defect class. It has no line-terminator
//! awareness whatsoever, so a tag toggling between the `\r` and the `\n` of a CRLF
//! splits the pair across two emissions — and any handler that repairs the payload then
//! sees a chunk ending in what looks exactly like a lone `\r`. ScrAP-312 records the
//! measurements; `probes/` holds the rigs.
//!
//! **Publishing plain text removes the mechanism rather than the symptom.** A string on
//! the clipboard is deserialised into a *fresh, untagged* buffer
//! (`gtk_text_buffer_new(NULL)`), which has no tag toggles, so a paste arrives as
//! exactly ONE emission however the source was highlighted. MEASURED end to end: a
//! source with a toggle inside a CRLF pasted as 2 emissions splitting the pair as a
//! buffer, and as `1 emission, 61 62 0d 0a 63 64` as text. It is also the honest model
//! for this application — a `.md` file cannot represent a syntax-highlight tag, so
//! intra-application rich paste was carrying state the document format has no way to
//! express.
//!
//! # The two clipboards are not the same problem
//!
//! **CLIPBOARD** is written only on an explicit copy or cut, so overriding the two
//! signals is enough. Every route reaches them: the context menu, the selection bubble
//! and the keybindings all go through `g_signal_emit_by_name`
//! (`gtktextview.c`'s `gtk_text_view_activate_clipboard_*`), and this window's `win.copy`
//! / `win.cut` actions emit them too.
//!
//! **PRIMARY** is republished continuously as the selection changes, by GTK itself, from
//! a `add_selection_clipboard` call inside `GtkTextView`'s own realize. It therefore has
//! to be taken over rather than overridden — see [`wire_primary_selection`]. It is **not**
//! an X11-only concern: MEASURED on Quartz, `GtkTextView` publishes to PRIMARY there, the
//! content is a `GtkTextBuffer`, it is readable, and a middle-click paste through it
//! works with a three-button mouse. `probes/primary-liveness.c` and
//! `probes/primary-middleclick.c`.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;
    use gtk::gdk;
    use std::cell::RefCell;

    /// A lazy PRIMARY provider: it publishes the *selection*, and builds the text only
    /// when someone actually reads it.
    ///
    /// # Why lazy, and not `gdk_clipboard_set_text` on every selection change
    ///
    /// PRIMARY is republished on every `mark-set`, which for a select-all followed by
    /// shift+arrow is once per keystroke. `set_text` would copy the entire selection
    /// each time; this builds nothing until a paste asks for it. MEASURED: four
    /// selection changes produced `value()` calls = **0** and `formats()` calls = 10,
    /// and the first `value()` call happened at read time.
    ///
    /// It also preserves GTK's *live* semantics for free, which the eager form would
    /// have lost: because the string is built in [`Self::value`] at paste time from
    /// whatever is selected then, a middle-click transfers the current selection rather
    /// than a snapshot taken when the selection was made — exactly as GTK's own
    /// provider behaves.
    ///
    /// # Why the buffer reference is WEAK
    ///
    /// PRIMARY is display-level and outlives any view. A strong reference here would
    /// pin the last-selected tab's entire document for the lifetime of the display.
    /// GTK's own provider takes a strong one, which is half of the leak recorded as a
    /// leak recorded as ScrAP-313 — `GtkTextBufferContent` refs the buffer and never
    /// unrefs it, so a whole document leaks per select-then-deselect.
    #[derive(Default)]
    pub struct ScribSelectionProvider {
        pub(super) buffer: RefCell<Option<glib::WeakRef<gtk::TextBuffer>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ScribSelectionProvider {
        const NAME: &'static str = "ScribSelectionProvider";
        type Type = super::ScribSelectionProvider;
        type ParentType = gdk::ContentProvider;
    }

    impl ObjectImpl for ScribSelectionProvider {}

    impl ContentProviderImpl for ScribSelectionProvider {
        fn formats(&self) -> gdk::ContentFormats {
            gdk::ContentFormatsBuilder::new()
                .add_type(glib::types::Type::STRING)
                .build()
        }

        /// # Two traps, both load-bearing
        ///
        /// 1. The binding does `g_value_copy` into a destination that already holds the
        ///    requested type, so returning anything but exactly `type_` produces a
        ///    critical and silence. Hence the equality test rather than a `matches!`.
        /// 2. **Chaining up for everything else is what makes a paste work at all.**
        ///    The parent returns `G_IO_ERROR_NOT_SUPPORTED`, and that error is the
        ///    signal that makes GDK fall through to the serialize/stream path — which
        ///    is what deserialises our string into a fresh, untagged buffer. Return a
        ///    different error here and the read fails outright instead of falling back.
        fn value(&self, type_: glib::types::Type) -> Result<glib::Value, glib::Error> {
            if type_ != glib::types::Type::STRING {
                return self.parent_value(type_);
            }
            let text = self
                .buffer
                .borrow()
                .as_ref()
                .and_then(|w| w.upgrade())
                .and_then(|buf| {
                    buf.selection_bounds().map(|(s, e)| {
                        crate::saferizer::BufferText::of_range(&buf, &s, &e).into_string()
                    })
                })
                .unwrap_or_default();
            Ok(text.to_value())
        }

        /// # Why this needs a guard, and what happens without one
        ///
        /// `gdk_clipboard_real_claim` installs the NEW content **before** detaching the
        /// old one (identical at GTK 4.6.9, 4.12.5 and 4.22.4). So replacing our own
        /// provider — which happens on every selection change — fires this on the
        /// outgoing instance *after* the incoming one is already installed. Collapsing
        /// the selection unconditionally there makes `mark-set` fire, observe an empty
        /// selection, and **release the clipboard we just claimed**: measured as PRIMARY
        /// going instantly `NULL` and the next paste failing with
        /// `Format text/plain not supported`.
        ///
        /// `content().is_none()` distinguishes the two cases — genuinely losing the
        /// clipboard (a remote claim, or our own release) from being replaced by another
        /// local provider. It is the same test GTK's own provider makes, spelled
        /// differently (`gdk_clipboard_get_content (clipboard) == priv->selection_content`).
        fn detach_clipboard(&self, clipboard: &gdk::Clipboard) {
            let genuinely_lost = clipboard.content().is_none();
            if genuinely_lost {
                if let Some(buf) = self.buffer.borrow().as_ref().and_then(|w| w.upgrade()) {
                    // Match GTK: another application taking PRIMARY collapses our
                    // selection, so the highlight does not linger over content the
                    // reader no longer owns.
                    let insert = buf.iter_at_mark(&buf.get_insert());
                    buf.place_cursor(&insert);
                }
            }
            self.parent_detach_clipboard(clipboard);
        }
    }
}

glib::wrapper! {
    pub struct ScribSelectionProvider(ObjectSubclass<imp::ScribSelectionProvider>)
        @extends gtk::gdk::ContentProvider;
}

impl ScribSelectionProvider {
    fn for_buffer(buffer: &gtk::TextBuffer) -> Self {
        let obj: Self = glib::Object::new();
        let weak = glib::WeakRef::new();
        weak.set(Some(buffer));
        *obj.imp().buffer.borrow_mut() = Some(weak);
        obj
    }
}

/// Publish **plain text** on the CLIPBOARD for copy and cut, instead of GTK's default
/// rich `GtkTextBuffer`.
///
/// Wired onto the editor view. The preview pane already publishes plain text by its own
/// route (`preview::interactions::wire_copy_clipboard`, which resolves a selection back
/// to Markdown source), so this closes the remaining half.
///
/// # Why cut needs more than stopping the emission
///
/// `gtk_text_buffer_cut_clipboard` is `begin_user_action` + `cut_or_copy(delete_region
/// _after = TRUE)` + `end_user_action`, and the deletion happens *inside* the path being
/// suppressed. Stopping the emission therefore copies without cutting. The explicit
/// `begin_user_action`/`end_user_action` pair around the delete is what keeps the whole
/// cut a **single** undo step; without it the deletion becomes its own step and one
/// Ctrl+Z leaves the document half-restored. MEASURED: `"KEEP<CUTME>KEEP"` cuts to
/// `"KEEPKEEP"` with `"<CUTME>"` on the clipboard, and one undo restores the original.
///
/// # Why an empty selection needs no special case here
///
/// `cut_or_copy` returns early when nothing is selected, so an empty payload never
/// reaches this handler — and unlike PRIMARY, CLIPBOARD carries no ownership obligation
/// to release, because it is only ever written by an explicit copy or cut. The
/// `selection_bounds()` test below is what makes that true rather than assumed.
pub(crate) fn wire_plaintext_clipboard(view: &sourceview::View) {
    view.connect_copy_clipboard(|v| {
        let buf = v.buffer();
        if let Some((start, end)) = buf.selection_bounds() {
            v.clipboard()
                .set_text(crate::saferizer::BufferText::of_range(&buf, &start, &end).as_str());
        }
        v.stop_signal_emission_by_name("copy-clipboard");
    });

    view.connect_cut_clipboard(|v| {
        let buf = v.buffer();
        if let Some((start, end)) = buf.selection_bounds() {
            v.clipboard()
                .set_text(crate::saferizer::BufferText::of_range(&buf, &start, &end).as_str());
            // `UndoGroup`, not a raw begin/end pair: it also flushes the redo-merge
            // barrier first, which a hand-rolled pair here would silently omit.
            let _grp = crate::window::undo::UndoGroup::new(&buf);
            buf.delete_selection(true, v.is_editable());
        }
        v.stop_signal_emission_by_name("cut-clipboard");
    });
}

/// Take PRIMARY over from GTK, publishing the selection as plain text.
///
/// # Why the realize/unrealize pairing is asymmetric
///
/// `GtkTextView`'s own realize calls `add_selection_clipboard(buffer, PRIMARY)` and its
/// unrealize calls the matching remove — and the registration is **ref-counted**, so a
/// one-shot removal at startup is unbalanced in both directions. Removing without
/// rebalancing makes GTK's own unrealize handler fail its
/// `g_return_if_fail (selection_clipboard != NULL)`, and then the next realize adds it
/// back and **silently reclaims PRIMARY** — a fix that reverts itself with no error and
/// no failing test.
///
/// The phases are what make the plain connections correct, and they are GtkWidget
/// signal definitions with no windowing code involved, so this is backend-independent:
/// `realize` is `G_SIGNAL_RUN_FIRST`, so the class closure (GTK's add) runs *before*
/// this handler and it can remove; `unrealize` is `G_SIGNAL_RUN_LAST`, so the class
/// closure (GTK's remove) runs *after* this handler and it can pre-add. No
/// `connect_after` is needed — which matters, because gtk4-rs generates no
/// `connect_after_realize` at all.
///
/// # Why an empty selection RELEASES rather than publishing ""
///
/// Other applications key off PRIMARY *ownership*. Publishing an empty string claims it
/// for nothing and breaks every other application's middle-click paste. `set_content(None)`
/// matches `update_selection_clipboards`'s own else-branch.
pub(crate) fn wire_primary_selection(view: &sourceview::View) {
    view.connect_realize(|v| {
        v.buffer()
            .remove_selection_clipboard(&v.primary_clipboard());
    });
    view.connect_unrealize(|v| {
        v.buffer().add_selection_clipboard(&v.primary_clipboard());
    });

    let view_weak = view.downgrade();
    view.buffer().connect_mark_set(move |buf, _iter, mark| {
        // Only the two marks that define a selection; every other mark-set is noise.
        let name = mark.name();
        if name.as_deref() != Some("insert") && name.as_deref() != Some("selection_bound") {
            return;
        }
        let Some(v) = view_weak.upgrade() else {
            return;
        };
        let primary = v.primary_clipboard();
        match buf.selection_bounds() {
            Some(_) => {
                let provider = ScribSelectionProvider::for_buffer(buf);
                let _ =
                    primary.set_content(Some(provider.upcast_ref::<gtk::gdk::ContentProvider>()));
            }
            None => {
                let _ = primary.set_content(None::<&gtk::gdk::ContentProvider>);
            }
        }
    });
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use sourceview::prelude::*;

    fn editor(md: &str) -> (sourceview::Buffer, sourceview::View) {
        let buf = crate::lineendings::new_editor_buffer();
        buf.set_language(
            sourceview::LanguageManager::default()
                .language("markdown")
                .as_ref(),
        );
        buf.set_enable_undo(true);
        buf.set_text(md);
        let view = sourceview::View::with_buffer(&buf);
        view.set_editable(true);
        wire_plaintext_clipboard(&view);
        wire_primary_selection(&view);
        (buf, view)
    }

    fn present(view: &sourceview::View) -> (gtk::Window, glib::MainContext) {
        let win = gtk::Window::new();
        win.set_child(Some(view));
        win.present();
        let ctx = glib::MainContext::default();
        for _ in 0..200 {
            if win.is_mapped() {
                break;
            }
            if !ctx.iteration(false) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        (win, ctx)
    }

    fn pump(ctx: &glib::MainContext, budget: u32) {
        for _ in 0..budget {
            if !ctx.iteration(false) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }

    fn text_of(buf: &sourceview::Buffer) -> String {
        crate::saferizer::BufferText::of(buf).into_string()
    }

    /// **A same-application copy/paste arrives as exactly ONE `insert-text` emission,
    /// and a CRLF survives it.**
    ///
    /// This is the whole point of publishing plain text, stated as the property that
    /// makes the corruption class unreachable rather than as the absence of a symptom.
    /// GTK's default rich content is re-inserted one chunk per syntax-highlight tag
    /// toggle, and a toggle inside a `\r\n` is what lets any payload-repairing handler
    /// turn it into `\n\n` (ScrAP-312). With a string on the clipboard the paste is
    /// deserialised into a fresh, UNTAGGED buffer, so there are no toggles to chunk on.
    ///
    /// **The document is chosen, not incidental.** An unclosed fenced code block leaves
    /// a `no-spell-check` context open at EOF, which is what puts a tag toggle on the
    /// final `\n`; `ensure_highlight` is what makes the tags real. Before this change
    /// this exact document pasted as TWO emissions with the first ending `0d`. If the
    /// emission count ever goes above one again, the mechanism is back whether or not
    /// anything currently exploits it.
    #[gtktest::test]
    fn a_same_application_paste_arrives_as_a_single_emission() {
        use std::cell::Cell;
        use std::rc::Rc;

        let crlf = "# CRLF doc\r\n\r\n```rust\r\nfn main() {}\r\n";
        let (buf, view) = editor(crlf);
        let (win, ctx) = present(&view);
        buf.ensure_highlight(&buf.start_iter(), &buf.end_iter());

        buf.select_range(&buf.start_iter(), &buf.end_iter());
        view.emit_copy_clipboard();
        pump(&ctx, 200);

        let (dest, dest_view) = editor("");
        let (dest_win, _) = present(&dest_view);
        let runs = Rc::new(Cell::new(0usize));
        let counter = Rc::clone(&runs);
        dest.connect_insert_text(move |_, _, _| counter.set(counter.get() + 1));

        dest_view.emit_paste_clipboard();
        for _ in 0..600 {
            if dest.char_count() as usize >= crlf.chars().count() {
                break;
            }
            if !ctx.iteration(false) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }

        assert_eq!(
            runs.get(),
            1,
            "a same-application paste must arrive as ONE emission; more than one means \
             tagged content is back on the clipboard and the CRLF split is reachable again"
        );
        let pasted = text_of(&dest);
        assert_eq!(
            pasted.matches('\r').count(),
            crlf.matches('\r').count(),
            "every carriage return must survive the paste: got {pasted:?}"
        );
        assert_eq!(
            pasted.matches('\n').count(),
            crlf.matches('\n').count(),
            "and no line feed may have been invented in place of one"
        );

        win.destroy();
        dest_win.destroy();
    }

    /// **Lone CRs pasted from outside are repaired, and the CRLF beside them is not.**
    ///
    /// This is the route the defect was originally reported through — text arriving on
    /// the clipboard from a keyboard/mouse sharing tool with classic-Mac-OS line
    /// endings. It is repaired by `lineendings::wire_paste_normalization`, and it is only SAFE to repair here because the paste arrives as a single
    /// emission; the same repair against GTK's default tagged clipboard content is what
    /// corrupted `\r\n` (ScrAP-312). Asserting both in one test is deliberate: a repair
    /// that fixed the lone CRs and ate a CRLF would be the original bug wearing the
    /// fix's clothes.
    #[gtktest::test]
    fn a_foreign_paste_of_lone_crs_is_repaired_without_touching_crlf() {
        let (_buf, view) = editor("");
        let (win, ctx) = present(&view);

        // What a sharing tool's clipboard bridge actually delivers: a plain string.
        view.clipboard()
            .set_text("# Title\rProse.\r\r- alpha\r- beta\r\nkept\r\n");
        pump(&ctx, 100);
        view.emit_paste_clipboard();
        pump(&ctx, 400);

        let got = text_of(&view.buffer().downcast::<sourceview::Buffer>().unwrap());
        assert!(
            !got.contains('\r') || got.contains("\r\n"),
            "no LONE carriage return may survive into the buffer: {got:?}"
        );
        assert_eq!(
            got.matches("\r\n").count(),
            2,
            "both CRLF pairs must survive verbatim — repairing one into \\n\\n is the \
             corruption this whole arc is about: {got:?}"
        );
        assert!(
            got.starts_with("# Title\nProse.\n\n- alpha\n- beta\r\n"),
            "the lone CRs must have become line feeds: {got:?}"
        );

        win.destroy();
    }

    /// **A cut removes the text, puts it on the clipboard, and is a SINGLE undo step.**
    ///
    /// Stopping `cut-clipboard`'s emission suppresses GTK's own deletion, which happens
    /// inside the path being suppressed — so the handler has to perform it. The
    /// `begin_user_action`/`end_user_action` pair is what keeps it one undo step; without
    /// it the deletion becomes its own step and one Ctrl+Z leaves the document
    /// half-restored, which is the assertion at the end here.
    #[gtktest::test]
    fn a_cut_is_one_undo_step_and_leaves_the_text_on_the_clipboard() {
        let (buf, view) = editor("KEEP<CUTME>KEEP");
        let (win, ctx) = present(&view);

        let start = buf.iter_at_offset(4);
        let end = buf.iter_at_offset(11);
        buf.select_range(&start, &end);
        view.emit_cut_clipboard();
        pump(&ctx, 200);

        assert_eq!(
            text_of(&buf),
            "KEEPKEEP",
            "the selection must actually be removed"
        );

        buf.undo();
        assert_eq!(
            text_of(&buf),
            "KEEP<CUTME>KEEP",
            "ONE undo must restore the whole cut, not half of it"
        );

        win.destroy();
    }

    /// **PRIMARY carries our provider while there is a selection, and is RELEASED when
    /// there is not.**
    ///
    /// Releasing rather than publishing an empty string is not tidiness: other
    /// applications key off PRIMARY *ownership*, so claiming it for `""` breaks every
    /// other application's middle-click paste.
    #[gtktest::test]
    fn primary_carries_the_selection_and_is_released_when_it_is_empty() {
        let (buf, view) = editor("hello selected world");
        let (win, ctx) = present(&view);

        buf.select_range(&buf.start_iter(), &buf.end_iter());
        pump(&ctx, 50);
        let primary = view.primary_clipboard();
        assert!(
            primary
                .content()
                .is_some_and(|c| c.is::<ScribSelectionProvider>()),
            "a selection must publish OUR provider to PRIMARY, not GTK's buffer content"
        );

        buf.place_cursor(&buf.start_iter());
        pump(&ctx, 50);
        assert!(
            primary.content().is_none(),
            "an empty selection must RELEASE PRIMARY, never publish an empty string"
        );

        win.destroy();
    }

    /// **The PRIMARY takeover survives an unrealize/re-realize cycle.**
    ///
    /// This is the regression that would otherwise be invisible. GTK's
    /// `add_selection_clipboard` registration is ref-counted and `GtkTextView` adds at
    /// realize and removes at unrealize; a takeover that removes without rebalancing
    /// makes GTK's own unrealize fail its `g_return_if_fail`, and the next realize then
    /// silently reclaims PRIMARY. Nothing errors and nothing else fails — the fix simply
    /// stops working. A tab detach, a window move or any widget reparent is enough to
    /// trigger it.
    #[gtktest::test]
    fn the_primary_takeover_survives_a_re_realize() {
        let (buf, view) = editor("hello selected world");
        let (win, ctx) = present(&view);

        win.set_child(None::<&gtk::Widget>);
        pump(&ctx, 50);
        win.set_child(Some(&view));
        pump(&ctx, 200);

        buf.select_range(&buf.start_iter(), &buf.end_iter());
        pump(&ctx, 50);
        assert!(
            view.primary_clipboard()
                .content()
                .is_some_and(|c| c.is::<ScribSelectionProvider>()),
            "after a re-realize PRIMARY must still be ours; GTK reclaiming it here is \
             silent, which is why this is asserted rather than assumed"
        );

        win.destroy();
    }
}
