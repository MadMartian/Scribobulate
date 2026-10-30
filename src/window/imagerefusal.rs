//! Tells the reader why an image is missing when the XInclude screen refused it
//! (TDD 2.23c; Status-notice CAM row 9).
//!
//! The screen runs deep inside a decode, which knows no document. The render that shows
//! the image's placeholder does know its tab, so it hands the screen that tab's id
//! (`imagedecode::report_shown_refusal`) and the screen calls [`schedule`]. One idle
//! later every refusal queued since is reported here, each in the window that holds the
//! document it was refused in — never simply the active window, which may be showing a
//! different document with nothing blocked in it. The window is resolved NOW, from the
//! tab's id, not when the image was refused: a tab can move between windows in between,
//! and a tab closed in between has nobody left to tell, so its notice is dropped.
//!
//! A theme sprite is shown by no document, so its refusal goes to the window in front
//! (resolved now too), said to be the theme's.
//!
//! Refusals coalesce per window: one notice for the window's documents — named when a
//! single document behind the front one, counted when there are several — and one for
//! theme images. The screen records who was told about which content, so a re-render
//! (each keystroke, a zoom step, a reload, a theme reload) reports nothing new; the
//! placeholder's tooltip keeps saying why for as long as it is shown.

use crate::imagedecode::RefusalTarget;
use crate::winstate::statusbar::{xinclude_blocked_notices, PlacedRefusal, RefusalSubject};
use crate::winstate::{WindowChrome, ERROR_NOTICE_TIME};
use gtk::prelude::*;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether a report is already queued, so a render refusing several images queues one
/// idle rather than one each. Atomic, and the idle queued with the thread-safe
/// `idle_add_once`, because the screen is a pure function a test thread may call: the
/// report must still run on the thread that owns GTK, whichever thread refused.
static SCHEDULED: AtomicBool = AtomicBool::new(false);

/// Route the screen's refusals here. Called once, at application start-up.
pub(crate) fn install() {
    crate::imagedecode::on_refusal(schedule);
}

fn schedule() {
    if SCHEDULED.swap(true, Ordering::AcqRel) {
        return;
    }
    gtk::glib::idle_add_once(report);
}

/// A window's chrome compared by identity — the key notices are coalesced on.
#[derive(Clone)]
struct SameWindow(Rc<WindowChrome>);

impl PartialEq for SameWindow {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

fn report() {
    SCHEDULED.store(false, Ordering::Release);
    let front = gtk::gio::Application::default()
        .and_downcast::<gtk::Application>()
        .and_then(|app| app.active_window())
        .and_downcast::<gtk::ApplicationWindow>();
    report_to(front.as_ref());
}

/// Report every queued refusal, `front` being the window in front — where a theme
/// image's notice goes. Split from [`report`] so a test can say which window that is.
fn report_to(front: Option<&gtk::ApplicationWindow>) {
    let mut placed = Vec::new();
    for (target, origin) in crate::imagedecode::take_unreported_refusals() {
        let Some((window, subject)) = place(target, front) else {
            log::info!("blocked image {origin} not reported: no window to report it in");
            continue;
        };
        placed.push(PlacedRefusal {
            window: SameWindow(window),
            subject,
            origin,
        });
    }
    for (SameWindow(chrome), text) in xinclude_blocked_notices(&placed) {
        // A timed notice through the chrome that issued it holds Status-notice CAM
        // columns B and C (`WindowChrome::push_timed_notice`).
        log::debug!("blocked-image notice: {text}");
        chrome.push_timed_notice(&text, ERROR_NOTICE_TIME);
    }
}

/// Where `target`'s notice goes, resolved now: a document's tab's current window (and
/// its name, if it is not in front there), or for the theme the window in front.
/// `None` when there is nobody to tell — the tab has closed, or no window is open.
fn place(
    target: RefusalTarget,
    front: Option<&gtk::ApplicationWindow>,
) -> Option<(Rc<WindowChrome>, RefusalSubject<crate::winstate::TabId>)> {
    match target {
        RefusalTarget::Document(id) => {
            let (chrome, in_front) = crate::winstate::tab_placement(id)?;
            let tab = crate::winstate::tab_by_id(id)?;
            let background_name = (!in_front).then(|| super::tabs::tab_display_name(&tab));
            Some((
                chrome,
                RefusalSubject::Document {
                    id,
                    background_name,
                },
            ))
        }
        RefusalTarget::Theme => Some((crate::winstate::chrome(front?)?, RefusalSubject::Theme)),
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::report_to;
    use crate::imagedecode::{report_shown_refusal, RefusalTarget};
    use crate::window::{create_tab_in_window, new_window};
    use crate::winstate::{chrome, rehome_tab, remove_tab, set_active_tab, state, tab_by_id};
    use gtk::gio::ApplicationFlags;
    use gtk::prelude::*;
    use std::rc::Rc;

    const HOSTILE: &[u8] =
        br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xi="http://www.w3.org/2001/XInclude"/>"#;
    const DIR: &str = "/imagerefusal-test";

    /// Have the screen refuse `name` (under [`DIR`]) as a decode would, and return the
    /// origin it was refused as.
    fn refuse(name: &str) -> String {
        let origin = format!("{DIR}/{name}");
        assert!(
            crate::imagedecode::decode(HOSTILE, &origin).is_none(),
            "precondition"
        );
        origin
    }

    fn footer(window: &gtk::ApplicationWindow) -> String {
        chrome(window).expect("chrome").status.borrow().label_text()
    }

    /// TDD 2.23c / Status-notice CAM row 9 — the notice follows the document: it lands
    /// in the window holding the tab whose render refused the image, names that tab's
    /// document when it is not in front, counts several documents in one notice, follows
    /// a tab that moved windows before the notice was shown, is dropped for a tab that
    /// closed, and reports a theme image in the window in front.
    #[gtktest::test]
    fn the_blocked_image_notice_follows_the_document() {
        if !crate::imagedecode::screens_xinclude() {
            println!("SKIPPED [2.23c]: this platform does not screen SVG XInclude");
            return;
        }
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.imagerefusal"),
            ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE).expect("register");
        let first = new_window(&app, "first", "# Front\n", None);
        let second = new_window(&app, "second", "# Other\n", None);
        let front = state(&first).expect("a front tab").id;
        let back = create_tab_in_window(&first, "# Back\n", None, false, false).expect("a tab");
        set_active_tab(&first, front);
        drop(crate::imagedecode::take_unreported_refusals());
        let doc = RefusalTarget::Document;

        // A document behind the one in front: its own window, named. The SECOND window
        // is the one said to be in front, so a notice that went "to the active window"
        // would land there.
        report_shown_refusal(doc(back), &refuse("a.svg"));
        report_to(Some(&second));
        assert!(
            footer(&first).starts_with("Untitled: Blocked image a.svg: "),
            "{}",
            footer(&first)
        );
        assert_eq!(footer(&second), "", "never the window in front");

        // Two documents of one window in one flush: ONE notice counting both.
        report_shown_refusal(doc(back), &refuse("b1.svg"));
        report_shown_refusal(doc(back), &refuse("b2.svg"));
        report_shown_refusal(doc(front), &refuse("f.svg"));
        report_to(Some(&second));
        assert!(
            footer(&first).starts_with("Blocked 3 images in 2 documents: "),
            "{}",
            footer(&first)
        );

        // A theme image: no document shows it, so it goes to the window in front.
        report_shown_refusal(RefusalTarget::Theme, &refuse("rule.svg"));
        report_to(Some(&second));
        assert!(
            footer(&second).starts_with("Blocked theme image rule.svg: "),
            "{}",
            footer(&second)
        );

        // A theme image and the front document's image in one flush, in that window:
        // one notice naming both, or the second pushed would hide the first throughout.
        let other = state(&second).expect("a front tab").id;
        report_shown_refusal(RefusalTarget::Theme, &refuse("dot.png"));
        report_shown_refusal(doc(other), &refuse("o.svg"));
        report_to(Some(&second));
        assert!(
            footer(&second).starts_with("Blocked image o.svg and theme image dot.png: "),
            "{}",
            footer(&second)
        );

        // The tab moves to the second window before its notice is shown: the notice
        // goes where the tab is NOW, unnamed there because the move puts it in front.
        report_shown_refusal(doc(back), &refuse("m.svg"));
        let moved = tab_by_id(back).expect("the tab");
        moved.set_chrome(Rc::clone(&chrome(&second).expect("chrome")));
        rehome_tab(&second, back);
        report_to(Some(&first));
        assert!(
            footer(&second).starts_with("Blocked image m.svg: "),
            "{}",
            footer(&second)
        );
        assert!(!footer(&first).contains("m.svg"), "{}", footer(&first));

        // The tab closes before its notice is shown: nobody is left to tell.
        report_shown_refusal(doc(back), &refuse("c.svg"));
        remove_tab(&second, back);
        report_to(Some(&first));
        assert!(!footer(&first).contains("c.svg") && !footer(&second).contains("c.svg"));

        for name in ["a", "b1", "b2", "f", "rule", "m", "c"] {
            crate::imagedecode::forget_for_test(&format!("{DIR}/{name}.svg"));
        }
        first.destroy();
        second.destroy();
    }
}
