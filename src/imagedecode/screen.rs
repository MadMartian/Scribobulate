//! The gate between this module and gdk-pixbuf's loader chain: every hand-off of
//! encoded bytes to a loader takes a [`Screened`], and the only way to get one is to
//! pass [`super::xinclude`]'s screen. A refusal is logged here, and remembered so the
//! window can say why an image is missing (TDD 2.23c).
//!
//! The screen knows the image, never the document: a decode carries no document. So the
//! notice is fed by whoever SHOWS the image ([`report_shown_refusal`]) — the render that
//! anchors its placeholder, which knows the tab it renders for, or the theme, for a
//! sprite — and this module keeps the ledger of who has already been told about which
//! content.

use super::xinclude;
use crate::winstate::TabId;
use std::collections::{BTreeMap, HashMap};

/// Who is shown a refused image, and so who its notice is for: an open document, by its
/// tab's id (the window holding it is resolved only when the notice is shown), or the
/// theme, whose sprites no document shows.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum RefusalTarget {
    Document(TabId),
    Theme,
}
use std::sync::{Mutex, OnceLock};

/// Bytes that passed the XInclude screen (or a build that does not screen). The loader
/// calls in this module take this rather than a slice, so a new loader call cannot be
/// written without screening first.
#[derive(Clone, Copy)]
pub(super) struct Screened<'a>(&'a [u8]);

impl<'a> Screened<'a> {
    pub(super) fn bytes(self) -> &'a [u8] {
        self.0
    }
}

/// Screen `bytes` decoded on behalf of `origin`, logging and recording a refusal.
pub(super) fn screen<'a>(bytes: &'a [u8], origin: &str) -> Option<Screened<'a>> {
    if !xinclude::enforced() {
        return Some(Screened(bytes));
    }
    match xinclude::inspect(bytes) {
        Ok(()) => {
            refusals::forget(origin);
            Some(Screened(bytes))
        }
        Err(reason) => {
            log::warn!(
                "image {origin} not loaded: refused before decoding because {reason} — \
                 SVG XInclude is blocked while librsvg carries CVE-2026-96889"
            );
            refusals::record(origin, bytes);
            None
        }
    }
}

/// Whether the screen refuses `bytes` decoded on behalf of `origin` — logged and
/// recorded exactly as a decode's refusal is — for a caller whose header probe already
/// failed and needs to know whether this is why (a theme sprite: TDD 2.23c).
pub(crate) fn screen_refuses(bytes: &[u8], origin: &str) -> bool {
    screen(bytes, origin).is_none()
}

/// Screen `bytes` for a header probe, which has no origin to report. A refusal is
/// logged at debug only: every probe is followed by a decode through [`screen`],
/// which reports it once.
pub(super) fn screen_quietly(bytes: &[u8]) -> Option<Screened<'_>> {
    if !xinclude::enforced() {
        return Some(Screened(bytes));
    }
    match xinclude::inspect(bytes) {
        Ok(()) => Some(Screened(bytes)),
        Err(reason) => {
            log::debug!("image header probe refused: {reason}");
            None
        }
    }
}

/// Whether the image decoded as `origin` was last refused by the screen — what the
/// broken-image placeholder's tooltip asks.
pub(crate) fn svg_refused(origin: &str) -> bool {
    refusals::lock().by_origin.contains_key(origin)
}

/// Install the function told, once per refusal newly queued for a notice, that
/// [`take_unreported_refusals`] has something. Set once, at application start-up.
pub(crate) fn on_refusal(listener: fn()) {
    let _ = LISTENER.set(listener);
}

/// `target` is being shown the image decoded as `origin` — a render's placeholder, or a
/// theme sprite. If the screen refused that image's current content and `target` has
/// not been told about that content yet, queue it for a notice and wake the listener.
/// Anything else — an image refused for another reason, content already reported to
/// this target — does nothing, so a re-render (a keystroke, a zoom step, a theme
/// reload) reports nothing new.
pub(crate) fn report_shown_refusal(target: RefusalTarget, origin: &str) {
    // Called with the lock released: the listener may drain the queue at once.
    if refusals::queue_for(target, origin) {
        if let Some(listener) = LISTENER.get() {
            listener();
        }
    }
}

/// Every `(target, origin)` queued since the last call, oldest first, each once.
pub(crate) fn take_unreported_refusals() -> Vec<(RefusalTarget, String)> {
    std::mem::take(&mut refusals::lock().unreported)
}

static LISTENER: OnceLock<fn()> = OnceLock::new();

mod refusals {
    use super::*;

    /// How many refused origins are remembered before the record is reset. A reset
    /// only means an image refused again is reported again.
    const MAX_TRACKED: usize = 256;

    pub(super) struct Refusals {
        /// Origin → a hash of the refused bytes, so the same content is reported once
        /// however often it is re-rendered, and changed content is reported afresh.
        pub(super) by_origin: BTreeMap<String, u64>,
        /// `(target, origin)` → the content hash that target was told about. Per
        /// target, because the notice is per document: a second document showing the
        /// same refused image is told too, and a re-render of the first is not told again.
        pub(super) told: Option<HashMap<(RefusalTarget, String), u64>>,
        pub(super) unreported: Vec<(RefusalTarget, String)>,
    }

    static REFUSALS: Mutex<Refusals> = Mutex::new(Refusals {
        by_origin: BTreeMap::new(),
        told: None,
        unreported: Vec::new(),
    });

    pub(super) fn lock() -> std::sync::MutexGuard<'static, Refusals> {
        REFUSALS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn record(origin: &str, bytes: &[u8]) {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        let hash = hasher.finish();
        let mut r = lock();
        if r.by_origin.len() >= MAX_TRACKED {
            r.by_origin.clear();
        }
        r.by_origin.insert(origin.to_owned(), hash);
    }

    /// Queue `(target, origin)` for a notice if the screen refused `origin`'s current
    /// content and `target` has not been told about it. Returns whether it was queued.
    pub(super) fn queue_for(target: RefusalTarget, origin: &str) -> bool {
        let mut r = lock();
        let Some(&hash) = r.by_origin.get(origin) else {
            return false;
        };
        let told = r.told.get_or_insert_with(HashMap::new);
        if told.len() >= MAX_TRACKED {
            told.clear();
        }
        if told.insert((target, origin.to_owned()), hash) == Some(hash) {
            return false;
        }
        // Drained every idle once start-up installs the listener; the cap only matters
        // where nothing drains it (a test process), so it cannot grow without bound.
        if r.unreported.len() >= MAX_TRACKED {
            r.unreported.clear();
        }
        r.unreported.push((target, origin.to_owned()));
        true
    }

    pub(super) fn forget(origin: &str) {
        let mut r = lock();
        if r.by_origin.remove(origin).is_some() {
            r.unreported.retain(|(_, o)| o != origin);
            if let Some(told) = r.told.as_mut() {
                told.retain(|(_, o), _| o != origin);
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn forget_for_test(origin: &str) {
    refusals::forget(origin);
}

/// Every target told about `origin`'s refusal — the ledger [`report_shown_refusal`]
/// keeps, documents by tab id and the theme last. Read rather than the queue, which an
/// installed listener may drain at any idle.
#[cfg(test)]
pub(crate) fn told_for_test(origin: &str) -> Vec<RefusalTarget> {
    let r = refusals::lock();
    let mut told: Vec<RefusalTarget> = r
        .told
        .iter()
        .flatten()
        .filter(|((_, o), _)| o == origin)
        .map(|((target, _), _)| *target)
        .collect();
    told.sort_by_key(|t| match t {
        RefusalTarget::Document(tab) => tab.raw(),
        RefusalTarget::Theme => u64::MAX,
    });
    told
}

#[cfg(test)]
mod tests {
    use super::*;

    const XINCLUDE_SVG: &[u8] =
        br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xi="http://www.w3.org/2001/XInclude"/>"#;

    /// A test binary runs outside any macOS bundle, so on every platform it loads a
    /// decoder this project did not deliver and the screen is on (TDD 2.23c). This is
    /// also what keeps the other 2.23c tests from skipping on macOS, where a
    /// compile-time exemption once left a non-bundle run with no screen at all.
    #[test]
    fn a_process_outside_a_macos_bundle_is_screened() {
        assert!(xinclude::enforced());
        assert_eq!(
            screen(XINCLUDE_SVG, "test:screen:outside-bundle.svg").map(Screened::bytes),
            None
        );
        crate::imagedecode::forget_for_test("test:screen:outside-bundle.svg");
    }

    #[test]
    fn a_refusal_is_remembered_by_origin_and_forgotten_when_the_content_passes() {
        if !xinclude::enforced() {
            println!("SKIPPED [2.23c]: this platform does not screen SVG XInclude");
            return;
        }
        let origin = "test:screen:remembered.svg";
        assert!(screen(XINCLUDE_SVG, origin).is_none());
        assert!(svg_refused(origin));
        assert!(screen(b"<svg/>", origin).is_some());
        assert!(
            !svg_refused(origin),
            "content that passes clears the record"
        );
        forget_for_test(origin);
    }

    /// TDD 2.23c — the notice ledger: each document is told about a refused image's
    /// content once, a second document is told too, changed content is told afresh, and
    /// content that later passes clears it so a relapse is reported again.
    #[test]
    fn each_document_is_told_about_refused_content_once() {
        if !xinclude::enforced() {
            println!("SKIPPED [2.23c]: this platform does not screen SVG XInclude");
            return;
        }
        let origin = "test:screen:ledger.svg";
        let first = RefusalTarget::Document(TabId::from_raw(u64::MAX - 1));
        let second = RefusalTarget::Document(TabId::from_raw(u64::MAX - 2));
        assert!(
            !refusals::queue_for(first, origin),
            "an image the screen never refused is never reported"
        );
        assert!(screen(XINCLUDE_SVG, origin).is_none());
        assert!(
            refusals::queue_for(first, origin),
            "first sight is reported"
        );
        assert!(
            !refusals::queue_for(first, origin),
            "a re-render is not reported again"
        );
        assert!(
            refusals::queue_for(second, origin),
            "a second document is told too"
        );
        assert!(
            refusals::queue_for(RefusalTarget::Theme, origin),
            "the theme is told too"
        );
        assert_eq!(told_for_test(origin), [second, first, RefusalTarget::Theme]);

        let changed = br#"<svg xmlns:a="http://www.w3.org/2001/XInclude"><a:include/></svg>"#;
        assert!(screen(changed, origin).is_none());
        assert!(
            refusals::queue_for(first, origin),
            "changed content is reported afresh"
        );

        assert!(screen(b"<svg/>", origin).is_some());
        assert!(
            told_for_test(origin).is_empty(),
            "passing content clears the ledger"
        );
        assert!(screen(XINCLUDE_SVG, origin).is_none());
        assert!(
            refusals::queue_for(first, origin),
            "a relapse is reported again"
        );
        forget_for_test(origin);
    }

    #[test]
    fn a_header_probe_refuses_without_recording() {
        if !xinclude::enforced() {
            println!("SKIPPED [2.23c]: this platform does not screen SVG XInclude");
            return;
        }
        assert!(screen_quietly(XINCLUDE_SVG).is_none());
        assert!(screen_quietly(b"<svg/>").is_some());
    }
}
