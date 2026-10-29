//! The right-click context menu attached to the preview/editor text views.

use super::*;
/// Dismiss the per-invocation context popover from inside a child button's `clicked`
/// handler — but DEFERRED to a one-shot idle. The popover is `set_parent`/`popup`'d
/// per right-click and `unparent`s itself on `closed`; calling `popdown()` (→ `closed`
/// → `unparent`) synchronously inside the button's click tears the popover subtree down
/// while the pointer press→release dispatch is still in flight, so GTK then (a) finishes
/// "active"-state accounting on the now-unparented button → `Broken accounting of active
/// state for widget (GtkPopover)`, and (b) drops its still-held event/crossing refs on
/// finalized widgets → `g_object_unref: G_IS_OBJECT` (one per node). Running the popdown
/// on idle moves the whole teardown out of the dispatch. (GTK4Rs/AP-30;
/// researcher-verified, gtk-4-6.)
fn dismiss_context_popover(po: &gtk::Popover) {
    let po = po.clone();
    gtk::glib::idle_add_local_once(move || po.popdown());
}
/// The scaffold every right-click menu shares (the document, tab and toolbar menus): a
/// plain arrowless `GtkPopover` — the convention here, because `GtkPopoverMenu` grows a
/// spurious scrollbar on GTK 4.6 (ScrAP-9) — and one Capture/Local shortcut controller
/// serving bare-letter access keys (GTK4Rs/T-1). A menu builds its rows, registering each
/// row's access key through [`add_access_key`](Self::add_access_key), then
/// [`open`](Self::open)s it, which owns the parts the copies once disagreed on: the
/// scrolling body and the deferred release.
pub(crate) struct ContextMenu {
    pub(crate) popover: gtk::Popover,
    /// Private: every row's access key is registered through
    /// [`add_access_key`](Self::add_access_key), the one place that both reads the `_`
    /// marker and installs the shortcut. Three menus each spelled that pair out, and one
    /// of them once lost an access key silently when its label lost its marker.
    keys: gtk::ShortcutController,
}

impl ContextMenu {
    pub(crate) fn new() -> Self {
        let popover = gtk::Popover::new();
        popover.set_has_arrow(false);
        let keys = gtk::ShortcutController::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.set_scope(gtk::ShortcutScope::Local);
        Self { popover, keys }
    }

    /// Register `row`'s bare-letter access key from its `_`-marked label `marked`, live
    /// while `gate()` holds, and return the label's markup with that key underlined (a
    /// plain popover never shows mnemonics, GTK4Rs/T-1), for the caller to install.
    ///
    /// A label with no usable marker still gets its markup, and is logged: a row that
    /// reaches this has been declared to carry an access key, so losing it would
    /// otherwise show neither a warning nor a failing test. A row meant to have no key
    /// (the Change Case page's Back) does not come here.
    pub(crate) fn add_access_key(
        &self,
        row: &impl IsA<gtk::Widget>,
        marked: &str,
        gate: impl Fn() -> bool + 'static,
    ) -> String {
        let (key, markup) = crate::app::access_markup(marked);
        match key.and_then(|key| crate::app::access_shortcut(row, key, gate)) {
            Some(shortcut) => self.keys.add_shortcut(shortcut),
            None => log::warn!("context menu row {marked:?} has no usable access key"),
        }
        markup
    }

    /// Bind a key that is not a row's access key (the Change Case page's Left arrow) on
    /// the same Capture/Local controller.
    pub(crate) fn add_shortcut(&self, shortcut: gtk::Shortcut) {
        self.keys.add_shortcut(shortcut);
    }

    /// Show `body` as the menu, pointing at `(x, y)` in `parent`'s coordinates.
    /// `on_closed` is the menu's own close work; it runs before the deferred release.
    pub(crate) fn open(
        self,
        parent: &impl IsA<gtk::Widget>,
        (x, y): (f64, f64),
        body: &impl IsA<gtk::Widget>,
        on_closed: impl Fn(&gtk::Popover) + 'static,
    ) -> gtk::Popover {
        let Self { popover, keys } = self;
        popover.set_parent(parent);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        install_menu_body(&popover, body);
        popover.add_controller(keys);
        popover.connect_closed(move |p| {
            on_closed(p);
            release_after_close(p);
        });
        popover.popup();
        popover
    }
}

/// Unparent a closed menu later, never inside its `closed` emission. Every right-click
/// menu releases itself through this, installed by [`ContextMenu::open`] — module-private
/// so a menu cannot hand-roll its own `closed` release and get it wrong again (the
/// document and tab menus once unparented inside the emission).
///
/// `closed` is emitted from the popover's hide, and hiding the HOST hides the popover from
/// inside GTK's own unmap of it: the host's unmap walks its children, the popover's unmap
/// hides its surface, and on GDK-Win32 the surface's `mapped` notify fires synchronously
/// inside that hide (`gdk_win32_surface_hide` → `gdk_surface_set_is_mapped`; the generic
/// path queues it to an idle, which is why X11 never shows this), and the popover answers
/// it with `gtk_widget_set_visible(FALSE)` → `closed`. Unparenting there unrealizes the
/// popover — destroying the surface GDK is still hiding, and freeing the popover if the
/// host held its only reference — in the middle of both. Closing the window while the menu
/// is open is the same chain under `gtk_widget_unrealize`, which emits `unrealize` after
/// the unmap without checking again, so the popover is unrealized twice. Unparenting
/// after the emission has unwound avoids both (GTK4Rs/AP-30). The delay is the tooltip
/// drain, because unrealizing a popover the pointer has just been over is its own fault
/// (GTK4Rs/AP-117). Weak: a popover already gone with its window has nothing to release.
fn release_after_close(popover: &gtk::Popover) {
    let weak = popover.downgrade();
    glib::timeout_add_local_once(crate::widgets::TOOLTIP_HOVER_DRAIN, move || {
        if let Some(popover) = weak.upgrade() {
            if !popover.is_visible() && popover.parent().is_some() {
                popover.unparent();
            }
        }
    });
}

/// Give `popover` its menu `body` inside a vertical scroller, so the menu's MINIMUM
/// height no longer equals its full height.
///
/// On GTK ≥ 4.14 a popover closes itself on the next frame when its surface is given
/// less room than its minimum size (GTK4Rs/AP-86). With the rows set straight in the
/// popover, the minimum was the whole menu (about 500 pt), so a right-click in the band
/// of the screen where the menu fits neither above nor below the pointer opened the
/// menu and closed it within one frame — the reader saw nothing, and the band depends
/// on screen height and window position, not on the document. Inside the scroller the
/// menu keeps its natural height wherever it fits (`propagate_natural_height`) and
/// scrolls where it does not, which is what GTK's own `GtkPopoverMenu` does. Every
/// right-click menu goes through here, from [`ContextMenu::open`] (TDD 9.39).
fn install_menu_body(popover: &gtk::Popover, body: &impl IsA<gtk::Widget>) {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_propagate_natural_width(true);
    scroller.set_child(Some(body));
    popover.set_child(Some(&scroller));
}

/// Attach the right-click context menu to any text view (preview GtkTextView or
/// editor GtkSourceView).  Finds the inner GtkTextView if `container` is a
/// GtkScrolledWindow.  Table-cell label data is read from qdata at click time so
/// this can be called once per view and survives buffer swaps (re_render).
///
/// The capture-phase GestureClick suppresses the view's built-in popup on both
/// the preview and the editor, providing a single consistent menu across all modes.
pub(crate) fn attach_context_menu(container: &gtk::Widget) {
    let Some(view) = find_text_view(container) else {
        return;
    };

    // ctx_cell: the table-cell GtkLabel the user right-clicked on, or None for
    // plain document text.  Read by the Select All handler to select within a cell.
    let ctx_cell: Rc<RefCell<Option<Label>>> = Rc::new(RefCell::new(None));
    let ctx_r = Rc::clone(&ctx_cell);
    let view_weak = view.downgrade();

    let right_click = GestureClick::new();
    right_click.set_button(3);
    right_click.set_propagation_phase(gtk::PropagationPhase::Capture);
    right_click.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        let Some(view) = view_weak.upgrade() else {
            return;
        };

        // Read table labels from qdata at click time so the list stays current
        // after re_render() swaps the buffer without recreating the view widget.
        let table_labels: Rc<RefCell<Vec<Label>>> = crate::preview::scrib_labels(&view)
            .unwrap_or_else(|| Rc::new(RefCell::new(Vec::new())));

        // Determine whether the click landed on a table-cell GtkLabel.
        *ctx_r.borrow_mut() = {
            let mut w = view.pick(x, y, gtk::PickFlags::DEFAULT);
            let mut found = None;
            while let Some(node) = w {
                if let Ok(label) = node.clone().dynamic_cast::<Label>() {
                    if table_labels.borrow().iter().any(|tl| tl == &label) {
                        found = Some(label);
                        break;
                    }
                }
                w = node.parent();
            }
            found
        };

        // Each context-menu row: [label]  [dim accel hint], returned with its name
        // label for the caller to fill — a row with an access key gets its markup from
        // `ContextMenu::add_access_key`, which underlines the key itself because a plain
        // popover never gets mnemonics-visible (GTK4Rs/T-1).  Text-only matches the
        // menu bar (icons ignored — GTK4Rs/AP-11); GtkPopoverMenu has a spurious
        // scrollbar on 4.6, so this stays plain GtkPopover + GtkButton (ScrAP-9), and
        // Change Case is a GtkStack page rather than a nested popover surface
        // (GTK4Rs/AP-69).
        let make_text_btn = |accel: &str| -> (gtk::Button, Label) {
            let name_lbl = Label::new(None);
            name_lbl.set_halign(gtk::Align::Start);
            name_lbl.set_hexpand(true);
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            row.set_hexpand(true);
            row.append(&name_lbl);
            if !accel.is_empty() {
                let hint = Label::new(Some(accel));
                hint.set_halign(gtk::Align::End);
                hint.add_css_class("dim-label");
                hint.set_margin_start(24);
                row.append(&hint);
            }
            let btn = gtk::Button::new();
            btn.set_child(Some(&row));
            btn.add_css_class("flat");
            btn.set_halign(gtk::Align::Fill);
            (btn, name_lbl)
        };

        // Parented to the window's CONTENT, not the window: the release is deferred out of
        // `closed`, so on a window close it runs after the window is gone, and a
        // `GtkWindow` does not unparent children it was handed by `set_parent` — the
        // menu was left behind ("still has children left: GtkPopover") and leaked. The
        // content container's own teardown unparents it, as the tab bar and toolbar do
        // for theirs. Not the text view: a `set_parent` child of a text view never
        // finishes its dispose (GTK4Rs/AP-80).
        let Some(host) = view
            .root()
            .and_then(|r| r.downcast::<gtk::Window>().ok())
            .and_then(|w| w.child())
        else {
            return;
        };
        let menu = ContextMenu::new();
        let popover = menu.popover.clone();

        // Single-surface two-page GtkStack (flat menu + Change Case submenu) and the
        // menu's Capture/Local ShortcutController delivering bare-letter access keys —
        // the researcher-confirmed public recipe for a non-model popover
        // (GTK4Rs/T-1/GTK4Rs/AP-69).
        let stack = gtk::Stack::new();
        stack.set_vhomogeneous(false);
        stack.set_interpolate_size(true);
        stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
        // A row whose bare-letter access key is live only while `page` shows (so `u` is
        // Undo on "main" yet UPPER CASE on "change-case"). The gate holds the stack
        // weakly: the stack owns the row, which owns the shortcut's gate.
        let keyed_btn = |marked: &str, accel: &str, page: &'static str| -> gtk::Button {
            let (btn, name_lbl) = make_text_btn(accel);
            let sw = stack.downgrade();
            let gate = move || {
                sw.upgrade()
                    .is_some_and(|s| s.visible_child_name().as_deref() == Some(page))
            };
            name_lbl.set_markup(&menu.add_access_key(&btn, marked, gate));
            btn
        };

        // Window for action-state lookups; sensitivity then mirrors the menu bar
        // automatically when actions are enabled/disabled (ScrAP-9).
        let win_ref = view
            .root()
            .and_then(|r| r.dynamic_cast::<ApplicationWindow>().ok());

        // Arm Copy Link Location with the link this right-click landed on, BEFORE
        // the rows below read `is_enabled()`. This is the only surface that knows
        // WHICH link the reader means, and it is what makes the row usable in the
        // read-only preview, which has no caret to gate on (`window::copylink`).
        // Disarmed on `closed`, at the bottom of this handler.
        if let Some(w) = win_ref.as_ref() {
            set_context_link(w, link_at_pointer(&view, x, y));
        }

        // Build buttons from EDIT_CMDS; accel hints derived at runtime via
        // accelerator_get_label so they are always platform-correct.
        let edit_btns: Vec<gtk::Button> = EDIT_CMDS
            .iter()
            .map(|cmd| {
                // Same accel-hint derivation the toolbar tooltips use (SSOT).
                let hint = crate::app::accel_hint(cmd.accel).unwrap_or_default();
                keyed_btn(&crate::app::mnem(cmd.label), &hint, "main")
            })
            .collect();

        for (i, cmd) in EDIT_CMDS.iter().enumerate() {
            let bare = cmd
                .action
                .find('.')
                .map_or(cmd.action, |p| &cmd.action[p + 1..]);
            edit_btns[i].set_sensitive(
                win_ref
                    .as_ref()
                    .and_then(|w| w.lookup_action(bare))
                    .is_some_and(|a| a.is_enabled()),
            );

            // Select All has a per-cell branch for table cells; all other commands
            // route through activate_action.
            if cmd.action == "win.select-all" {
                let ctx = Rc::clone(&ctx_r);
                let action = cmd.action;
                // Weak: the popover owns this button (via popup_box), so a strong
                // clone here would form a reference cycle that never drops (leak
                // on every right-click). See ANTI-PATTERNS.md.
                edit_btns[i].connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = v)]
                    view,
                    #[strong]
                    ctx,
                    move |_| {
                        dismiss_context_popover(&po);
                        if let Some(label) = ctx.borrow().as_ref() {
                            label.select_region(0, -1);
                        } else if v.activate_action(action, None).is_err() {
                            // `log`, not `eprintln!`: this is a GUI-only path, and
                            // the Windows release build is `windows_subsystem =
                            // "windows"` with no console attached — a context-menu
                            // item silently doing nothing would leave no trace at
                            // all there (POLICY § Logging).
                            log::error!("activate_action(\"{action}\") failed");
                        }
                    }
                ));
            } else {
                let action = cmd.action;
                edit_btns[i].connect_clicked(glib::clone!(
                    #[weak(rename_to = po)]
                    popover,
                    #[weak(rename_to = v)]
                    view,
                    move |_| {
                        dismiss_context_popover(&po);
                        if v.activate_action(action, None).is_err() {
                            // `log`, not `eprintln!`: this is a GUI-only path, and
                            // the Windows release build is `windows_subsystem =
                            // "windows"` with no console attached — a context-menu
                            // item silently doing nothing would leave no trace at
                            // all there (POLICY § Logging).
                            log::error!("activate_action(\"{action}\") failed");
                        }
                    }
                ));
            }
        }

        // ── "main" page: EDIT_CMDS rows with section separators, then the editor
        //    section (Insert Emoji + the Change Case submenu row). ──
        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (i, btn) in edit_btns.iter().enumerate() {
            if EDIT_CMDS[i].section_start {
                main_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            }
            main_box.append(btn);
        }
        main_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // Annotate — same editor section as the Edit menu. Bind by NAME
        // so GTK drives activation AND sensitivity from the single `win.annotate`
        // action (POLICY SSOT / ScrAP-9). Accel hint from the INLINE_ACCEL
        // table so the shown shortcut matches Ctrl+Alt+M.
        let annotate_marked = crate::app::mnem("Annotate");
        let annotate_hint = crate::app::inline_accel("win.annotate")
            .and_then(crate::app::accel_hint)
            .unwrap_or_default();
        let annotate_btn = keyed_btn(&annotate_marked, &annotate_hint, "main");
        annotate_btn.set_action_name(Some("win.annotate"));
        annotate_btn.connect_clicked(glib::clone!(
            #[weak(rename_to = po)]
            popover,
            move |_| {
                dismiss_context_popover(&po);
            }
        ));
        main_box.append(&annotate_btn);

        let emoji_marked = crate::app::mnem("Insert Emoji");
        let emoji_btn = keyed_btn(&emoji_marked, "", "main");
        // Bind to the action by NAME so GTK manages BOTH activation and the enabled
        // state from the single `win.insert-emoji` source of truth (POLICY). The
        // extra clicked handler only closes the popover.
        emoji_btn.set_action_name(Some("win.insert-emoji"));
        emoji_btn.connect_clicked(glib::clone!(
            #[weak(rename_to = po)]
            popover,
            move |_| {
                dismiss_context_popover(&po);
            }
        ));
        main_box.append(&emoji_btn);

        // Change Case is a real submenu (a second stack page) so its four variants
        // keep the SAME access keys as the menu bar's Change Case submenu (U/l/T/c) —
        // GTK4Rs/AP-69. This row slides to that page (it never dismisses).
        let change_case_marked = format!("{} ▸", crate::app::mnem("Change Case"));
        let change_case_btn = keyed_btn(&change_case_marked, "", "main");
        // Weak, as every other capture of the stack here: the stack owns this row, so a
        // strong clone is the cycle `stack → page → row → closure → stack`, and the whole
        // menu body outlived every right-click (GTK4Rs/AP-63).
        change_case_btn.connect_clicked(glib::clone!(
            #[weak]
            stack,
            move |_| stack.set_visible_child_name("change-case")
        ));
        main_box.append(&change_case_btn);

        // ── "change-case" page: a Back row + the four case variants. ──
        let case_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // No access key: the Left arrow is Back's keyboard route.
        let (back_btn, back_lbl) = make_text_btn("");
        back_lbl.set_text("◀ Back");
        back_btn.connect_clicked(glib::clone!(
            #[weak]
            stack,
            move |_| stack.set_visible_child_name("main")
        ));
        case_box.append(&back_btn);
        case_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // All four variants gate on the same change-case action (disabled together
        // when there is no selection or the editor is not visible).
        let case_enabled = win_ref
            .as_ref()
            .and_then(|w| w.lookup_action("change-case"))
            .is_some_and(|a| a.is_enabled());
        for (marked, variant) in [
            (crate::app::mnem("UPPER CASE"), "upper"),
            (crate::app::mnem("lower case"), "lower"),
            (crate::app::mnem("Title Case"), "title"),
            (crate::app::mnem("tOGGLE cASE"), "toggle"),
        ] {
            let btn = keyed_btn(&marked, "", "change-case");
            btn.set_sensitive(case_enabled);
            let v_str = variant.to_string();
            btn.connect_clicked(glib::clone!(
                #[weak(rename_to = po)]
                popover,
                #[weak(rename_to = v)]
                view,
                move |_| {
                    dismiss_context_popover(&po);
                    let _ = v.activate_action("win.change-case", Some(&v_str.to_variant()));
                }
            ));
            case_box.append(&btn);
        }

        // Left arrow returns from the submenu to the main page (Back's keyboard peer).
        {
            let sw = stack.downgrade();
            let action = gtk::CallbackAction::new(move |_, _| {
                if let Some(s) = sw.upgrade() {
                    if s.visible_child_name().as_deref() == Some("change-case") {
                        s.set_visible_child_name("main");
                        return gtk::glib::Propagation::Stop;
                    }
                }
                gtk::glib::Propagation::Proceed
            });
            if let Some(left) = gtk::gdk::Key::from_name("Left") {
                let trigger = gtk::KeyvalTrigger::new(left, gtk::gdk::ModifierType::empty());
                menu.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
            }
        }

        stack.add_named(&main_box, Some("main"));
        stack.add_named(&case_box, Some("change-case"));
        stack.set_visible_child_name("main");

        let at = view.translate_coordinates(&host, x, y).unwrap_or((x, y));
        // Weak (never a strong clone): this closure is owned by the popover, which
        // lives in the window's widget tree — a strong capture would be the
        // uncollectable cycle GTK4Rs/AP-63 is about.
        let win_weak = win_ref.as_ref().map(|w| w.downgrade());
        menu.open(&host, at, &stack, move |_| {
            // Disarm the right-clicked link with the popover it belongs to, so the
            // pointer target cannot outlive the menu and leave the menu-bar and
            // toolbar surfaces enabled for a link nobody is pointing at.
            if let Some(w) = win_weak.as_ref().and_then(|w| w.upgrade()) {
                set_context_link(&w, None);
            }
        });
    });
    view.add_controller(right_click);
}

/// **Closing a window with one of its right-click menus open releases the menu** — the
/// STATE guard all three menus (document, tab, toolbar) run, so none of them is safe only
/// by the grace of a sweep no test asserts.
///
/// The release is deferred out of `closed` (see `release_after_close`), so on a window
/// close it runs after the window is gone; the menu must therefore hang off a widget
/// whose own teardown unparents it. Parented to the window itself, it was left behind:
/// GTK warned "Finalizing GtkApplicationWindow, but it still has children left:
/// GtkPopover" (MEASURED on Windows 3 of 3, and on Linux). The toolbar and tab menus
/// are released by their hosts' `dispose` sweeps (`ToolbarWrapBox`, `TabBar`), so
/// narrowing either sweep brings that leak back — on Windows visibly, on Linux only
/// here.
///
/// The structural assertion comes first because, with the menu on the window, the
/// window's finalize leaves the menu's parent pointer dangling. For the same reason no
/// widget is read after the close at all: a host whose sweep was narrowed finalizes
/// with the menu still its child, and the menu's parent pointer then names freed
/// memory. The release is therefore RECORDED as it happens (`notify::parent` going to
/// `None`, emitted by `gtk_widget_unparent`), and afterwards only weak references are
/// upgraded: "the menu was unparented, or freed", and "its host was freed".
///
/// `menu` and `win` are taken by value and dropped here, and the caller must hold no
/// other strong reference to the window or the menu's host: one would keep the host
/// alive past the close and fail this for the wrong reason.
///
/// After the assertions it drains past the deferred release (`release_after_close`
/// runs `TOOLTIP_HOVER_DRAIN` after `closed`), so a critical raised by a release that
/// fires against the gone window is raised inside the test that provoked it, not in
/// whichever test runs next.
///
/// NOT asserted: that the popover object is freed. MEASURED on the document menu, it is
/// unparented and still alive after the window and its content are both finalized; what
/// holds it is not established (a popover unparented while still open, GTK4Rs/AP-123?).
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn assert_closing_the_window_releases(
    win: impl IsA<gtk::Window>,
    menu: gtk::Popover,
    what: &str,
) {
    use crate::testpump::{drain_for, until, until_or_for, Clock, TOOLTIP_HOVER_DRAIN};
    use std::{cell::Cell, rc::Rc, time::Duration};
    let unparented = Rc::new(Cell::new(false));
    let (menu, host) = {
        until(Clock::Frame, "the menu to map", || menu.is_mapped());
        let host = menu.parent().expect("an open menu has a parent");
        assert!(
            !host.is::<gtk::Window>(),
            "{what} is parented to the window, whose teardown never unparents a \
             set_parent child"
        );
        let flag = unparented.clone();
        menu.connect_notify_local(Some("parent"), move |m, _| {
            if m.parent().is_none() {
                flag.set(true);
            }
        });
        (menu.downgrade(), host.downgrade())
    };
    // Let the tooltip timeout the menu's map-time enter armed fire while its surface
    // exists (GTK4Rs/AP-117): this guard is about the unparent, not that.
    drain_for(Clock::Frame, TOOLTIP_HOVER_DRAIN);
    win.as_ref().destroy();
    drop(win);
    // Waited for on the host alone, and judged the moment it goes: a sweeping host
    // unparents the menu in its own dispose, before it is freed, so by then the release
    // has been recorded; a host that did not sweep has left the menu's parent pointer
    // dangling, and the deferred release would read it.
    let host_freed = || host.upgrade().is_none();
    until_or_for(
        Clock::Frame,
        TOOLTIP_HOVER_DRAIN + Duration::from_secs(2),
        host_freed,
    );
    assert!(host_freed(), "{what}'s host outlived its window");
    let released = unparented.get() || menu.upgrade().is_none();
    assert!(
        released,
        "{what} is still parented after its window was torn down"
    );
    drain_for(
        Clock::Frame,
        TOOLTIP_HOVER_DRAIN + Duration::from_millis(200),
    );
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// Taller than any monitor, so the popover cannot be given its natural height on
    /// either side of its anchor — the situation a right-click near the middle of a
    /// screen shorter than twice the menu puts the real context menu in.
    const TALLER_THAN_ANY_MONITOR: i32 = 20_000;

    use crate::testpump::{drain_for, until, until_for, until_or_for, Clock, TOOLTIP_HOVER_DRAIN};
    use std::time::Duration;

    /// How long a popover is given to close itself. The self-dismissal runs on a
    /// frame-clock tick after the surface is sized (GTK4Rs/AP-86), so it is WAITED for
    /// up to this bound, never sampled at a fixed moment: a late tick under load would
    /// otherwise read as "this GTK does not self-dismiss" and skip the effect half.
    const SELF_DISMISS_BOUND: Duration = Duration::from_secs(2);

    fn popped_over(
        window: &gtk::Window,
        install: impl Fn(&gtk::Popover, &gtk::Box),
    ) -> gtk::Popover {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.set_size_request(100, TALLER_THAN_ANY_MONITOR);
        let popover = gtk::Popover::new();
        install(&popover, &body);
        popover.set_parent(window);
        popover.popup();
        until(Clock::Frame, "the popover to map", || popover.is_mapped());
        popover
    }

    /// Whether `popover` closes itself within [`SELF_DISMISS_BOUND`].
    fn closes_itself(popover: &gtk::Popover) -> bool {
        until_or_for(Clock::Frame, SELF_DISMISS_BOUND, || !popover.is_visible())
    }

    /// popdown-then-unparent (GTK4Rs/AP-123), with GTK's tooltip hover timeout let run
    /// out in between. The 20000 px popover covers the pointer, so GTK's tooltip binds to
    /// the popover's surface and arms that timeout; on GTK 4.6, if it fires after
    /// `unparent()` has destroyed the surface, it raises a
    /// `gdk_surface_get_device_position` critical, fatal under the suite.
    fn close(popover: &gtk::Popover) {
        popover.popdown();
        drain_for(Clock::Frame, TOOLTIP_HOVER_DRAIN);
        popover.unparent();
    }

    /// A presented window over `md` whose preview has laid out, under a state home of
    /// its own, and the preview the document menu opens on.
    fn document_window(id: &str, md: &str) -> (ApplicationWindow, CodePreviewView) {
        let app = crate::window::testkit::test_app(&format!(
            "com.extollit.scribobulate.integrationtest.{id}"
        ));
        let win = crate::window::new_window(&app, "IT-docmenu", md, None);
        win.present();
        let view = crate::window::zoom::get_preview_view(&win).expect("a preview");
        until(Clock::Frame, "the preview to lay out", || view.height() > 0);
        (win, view)
    }

    /// Right-click `view` through its own secondary-button gesture, as a pointer press
    /// would, and return the document menu once it has mapped.
    fn open_document_menu(win: &ApplicationWindow, view: &CodePreviewView) -> gtk::Popover {
        let press = view
            .observe_controllers()
            .into_iter()
            .filter_map(|c| c.ok().and_downcast::<GestureClick>())
            .find(|g| g.button() == 3)
            .expect("the preview carries the right-click gesture");
        press.emit_by_name::<()>("pressed", &[&1i32, &20.0f64, &20.0f64]);
        until_for(
            Clock::Frame,
            Duration::from_secs(5),
            "the document menu to open",
            || find_popover(win.upcast_ref()).is_some(),
        );
        let menu = find_popover(win.upcast_ref()).expect("the menu is open");
        until(Clock::Frame, "the menu to map", || menu.is_mapped());
        menu
    }

    /// **Closing the window with the document menu open releases the menu.** See
    /// [`assert_closing_the_window_releases`], which all three menus share.
    ///
    /// Mutation check: parenting the menu to the window again fails the helper's first
    /// assertion.
    #[gtktest::test]
    fn closing_the_window_with_the_document_menu_open_releases_the_menu() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let (win, view) = document_window("docmenuclose", "# H\n\ntext\n");
            let menu = open_document_menu(&win, &view);
            drop(view);
            assert_closing_the_window_releases(win, menu, "the document menu");
        });
    }

    /// **Closing the document menu frees its body.** The menu is built afresh on every
    /// right-click, so anything that keeps one body alive keeps one per right-click. The
    /// Change Case and Back rows once captured the `GtkStack` that owns them strongly
    /// (`stack → page → row → closure → stack`), so the stack, both pages and every row
    /// outlived the menu (GTK4Rs/AP-63, GTK4Rs/AP-140's shape). The test holds only a weak
    /// reference to the stack, so nothing but such a cycle can keep it alive.
    ///
    /// Mutation check: restoring a strong `stack.clone()` capture in either row's
    /// `clicked` handler fails the final assertion.
    #[gtktest::test]
    fn closing_the_document_menu_frees_its_body() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let (win, view) = document_window("docmenufree", "# H\n\ntext\n");
            let (menu, stack) = {
                let menu = open_document_menu(&win, &view);
                let stack = menu
                    .child()
                    .and_downcast::<gtk::ScrolledWindow>()
                    .and_then(|s| s.child())
                    .and_downcast::<gtk::Viewport>()
                    .and_then(|v| v.child())
                    .and_downcast::<gtk::Stack>()
                    .expect("the menu body is a stack in a scroller's viewport");
                assert!(
                    stack.child_by_name("change-case").is_some(),
                    "precondition: this is the document menu, with its Change Case page"
                );
                (menu.downgrade(), stack.downgrade())
            };
            // The release waits out GTK's tooltip delay (GTK4Rs/AP-117), so the menu's
            // own map-time enter is let fire first.
            drain_for(Clock::Frame, TOOLTIP_HOVER_DRAIN);
            menu.upgrade().expect("the menu is still open").popdown();
            until_for(
                Clock::Frame,
                TOOLTIP_HOVER_DRAIN + Duration::from_secs(2),
                "the closed menu to be released",
                || menu.upgrade().and_then(|m| m.parent()).is_none(),
            );
            assert!(
                until_or_for(Clock::Frame, Duration::from_secs(2), || stack
                    .upgrade()
                    .is_none()),
                "the document menu's body outlived the menu: something it owns holds it"
            );
            drop(view);
            win.destroy();
        });
    }

    /// The first `GtkPopover` anywhere under `root`.
    fn find_popover(root: &gtk::Widget) -> Option<gtk::Popover> {
        let mut stack = vec![root.clone()];
        while let Some(w) = stack.pop() {
            if let Ok(p) = w.clone().downcast::<gtk::Popover>() {
                return Some(p);
            }
            let mut c = w.first_child();
            while let Some(child) = c {
                c = child.next_sibling();
                stack.push(child);
            }
        }
        None
    }

    /// The context menu must open wherever the reader right-clicks. On GTK ≥ 4.14 a
    /// popover closes itself on the next frame when the screen gives it less room than
    /// its MINIMUM size (GTK4Rs/AP-86), so a menu whose minimum is its full height
    /// opened and vanished in the band of the screen where it fits neither above nor
    /// below the pointer.
    ///
    /// Two halves, because they reach different platforms:
    /// * the **effect** (the menu stays open) is only expressible where GTK
    ///   self-dismisses. Whether it does is MEASURED on a bare popover first, never read
    ///   off the version number: GTK 4.6 clips a too-tall popover, and GTK ≥ 4.23.1, on
    ///   any backend, gives a popover whose minimum equals its natural size its full
    ///   height off-screen, so neither closes it, while 4.14–4.22 does. Where the bare
    ///   popover stays open that half is announced as skipped; where it closes, the
    ///   control has proved the rig can fail;
    /// * the **state** that causes it (the body's minimum height does not grow with
    ///   its content) holds on every GTK and is asserted everywhere.
    #[gtktest::test]
    fn a_context_menu_taller_than_the_screen_still_opens() {
        let window = gtk::Window::new();
        window.set_default_size(400, 300);
        // Something must hold focus, as it always does in the app: a popover opened
        // over a window with no focus widget trips `gtk_widget_is_ancestor` while
        // saving the focus it will restore, a critical that is fatal on Linux.
        let focus_target = gtk::Button::with_label("focus");
        window.set_child(Some(&focus_target));
        window.present();
        focus_target.grab_focus();
        until_for(
            Clock::Frame,
            Duration::from_secs(10),
            "no display: the window never mapped",
            || window.is_mapped(),
        );

        // The control: a bare popover whose minimum exceeds the screen. Its fate decides
        // whether this GTK and backend can express the defect at all.
        let bare = popped_over(&window, |p, b| p.set_child(Some(b)));
        let self_dismisses = closes_itself(&bare);
        if !self_dismisses {
            println!(
                "SKIPPED [9.39]: effect half — GTK {}.{}.{} on this backend keeps a popover \
                 taller than the screen open instead of closing it",
                gtk::major_version(),
                gtk::minor_version(),
                gtk::micro_version()
            );
        }
        close(&bare);

        let menu = popped_over(&window, install_menu_body);
        if self_dismisses {
            assert!(
                !closes_itself(&menu),
                "the context menu closed itself because the screen could not fit it"
            );
        }
        let body = menu.child().expect("the menu has a body");
        let (min_h, _, _, _) = body.measure(gtk::Orientation::Vertical, -1);
        assert!(
            min_h < TALLER_THAN_ANY_MONITOR / 10,
            "the menu body's minimum height ({min_h}) follows its content, so a screen \
             shorter than the menu dismisses it"
        );
        // "Scrolling if it cannot be shown whole": the open menu is fitted to its
        // monitor and its scroller has the rest to scroll to. MEASURED on GTK 4.6.9/X11,
        // where a popover is NOT presented at its natural height once its minimum is
        // smaller: a 1156 px menu opened 879 px tall over a 1024 px screen and scrolled.
        // Asserted, never branched on: when this GTK does not self-dismiss (4.6, the Linux
        // gate) nothing above has checked the menu is still open, and a menu that closed
        // for any other reason would skip every assertion below and pass.
        assert!(
            menu.is_visible(),
            "the context menu closed though this GTK keeps a too-tall popover open"
        );
        let scroller = body
            .downcast_ref::<gtk::ScrolledWindow>()
            .expect("the menu body is its scroller");
        let adj = scroller.vadjustment();
        until(Clock::Frame, "the menu's scroller to be allocated", || {
            adj.page_size() > 0.0
        });
        // Range alone is not scrolling: a scroller that may not scroll still reports
        // the clipped rows as range. The scrollbar is how the reader reaches them.
        assert!(
            adj.upper() > adj.page_size() && scroller.vscrollbar().is_child_visible(),
            "the menu offers no way to the rows the screen cannot fit (upper {}, page \
                 {}, scrollbar shown: {})",
            adj.upper(),
            adj.page_size(),
            scroller.vscrollbar().is_child_visible()
        );
        let monitor = menu
            .native()
            .and_then(|n| n.surface())
            .and_then(|surface| menu.display().monitor_at_surface(&surface))
            .expect("the open menu is on a monitor");
        assert!(
            menu.height() <= monitor.geometry().height(),
            "the menu ({} px) is taller than its monitor ({} px): its bottom is clipped",
            menu.height(),
            monitor.geometry().height()
        );
        close(&menu);
        window.destroy();
    }
}
