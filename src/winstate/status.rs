//! The footer status-message stack driving the single status label.

use std::sync::atomic::{AtomicU64, Ordering};

/// How long a transient *error* notice ("Link target not found", "File deleted on
/// disk") stays in the status bar before self-clearing.
///
/// One constant, every such notice: the app's transient-notice lifetime must not vary
/// link to link, and this used to be a per-module `6` with a comment in one of them
/// asserting it matched the other — the shape a drift starts as. The transient *info*
/// notice paired with a visual toast is deliberately shorter and owns its own constant
/// (`window::toast::INFO_STATUS_TIME`), because its lifetime is tied to the toast's.
pub(crate) const ERROR_NOTICE_TIME: std::time::Duration = std::time::Duration::from_secs(6);

/// Identity of one [`StatusStack`], so a [`StatusCtx`] knows which stack issued it.
///
/// Process-unique rather than per-window, so two stacks can never share an id
/// whatever order windows are built and destroyed in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct StackId(u64);

impl StackId {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        StackId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// The base entry's sequence number within its stack. Reserved — [`MessageStack::push`]
/// starts at 1 — so the "seq 0 is the base entry" invariant is spelled once here
/// instead of as a bare `0` at each comparison.
const BASE_SEQ: u64 = 0;

/// A pushed status notice's handle, returned by [`StatusStack::push`] and consumed
/// by [`StatusStack::pop`].
///
/// A newtype rather than a bare `u64`, matching the discipline its sibling
/// [`ids`](super::ids) module already applies to `WindowId`/`TabId` — and for the
/// same reason those exist. `push`/`pop` traded a raw `u64` while `TabId::raw()` and
/// `WindowId::raw()` produce raw `u64`s in the same call sites, so popping a status
/// notice with a tab's id compiled and did nothing: the `retain` simply matched no
/// entry, leaving the notice on screen permanently with no error anywhere. The type
/// makes that unrepresentable instead of merely unlikely.
///
/// It also carries the *issuing stack's* [`StackId`], which the type alone cannot make
/// unrepresentable: every window has its own stack, and a handle popped against a
/// different window's stack matches nothing and silently strands the notice in the
/// window that showed it — the cross-window failure `CAM.md`'s Status-notice matrix
/// (column C) is about. Carrying the id does not prevent that, but it lets
/// [`MessageStack::pop`] *name* it ([`PopOutcome::WrongStack`]) instead of failing the
/// way every other mis-addressed pop does: by doing nothing at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct StatusCtx {
    stack: StackId,
    seq: u64,
}

/// What a [`MessageStack::pop`] actually did — so the one failure that matters is
/// reportable rather than indistinguishable from success.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PopOutcome {
    /// The entry was found in this stack and removed.
    Retracted,
    /// This stack issued the handle but holds no such entry — an already-retracted
    /// notice popped twice. Harmless and expected (a timed pop can race a
    /// condition-driven one), so it is not reported.
    AlreadyRetracted,
    /// The handle was issued by a *different* stack. Always a bug: the notice this
    /// was meant to retract is still up in the window that issued it, and nothing
    /// else will ever take it down.
    WrongStack,
}

/// One entry in the message stack: a context id and its message. The base entry
/// ([`BASE_SEQ`]) is the persistent state; every pushed transient notice gets
/// its own distinct `ctx`. Named fields (not a positional tuple) so the base-entry
/// invariant is legible at every access site (QA D3).
struct StatusEntry {
    ctx: StatusCtx,
    msg: String,
}

/// The display-free push/pop message-stack core (unit-tested headlessly). The
/// base entry ([`BASE_SEQ`]) is the persistent state (e.g. "Unsaved
/// changes"); ephemeral
/// notices are pushed with their own ctx and popped later. [`top`](Self::top) is
/// what should show — the most recently pushed entry, or the base, or "".
struct MessageStack {
    id: StackId,
    entries: Vec<StatusEntry>,
    next_seq: u64,
}

impl MessageStack {
    fn new() -> Self {
        // Handed-out seqs start at 1; BASE_SEQ (0) is the persistent base.
        Self {
            id: StackId::next(),
            entries: Vec::new(),
            next_seq: 1,
        }
    }

    /// This stack's base-entry handle. Never handed out by [`push`](Self::push).
    fn base_ctx(&self) -> StatusCtx {
        StatusCtx {
            stack: self.id,
            seq: BASE_SEQ,
        }
    }

    /// Set the persistent base message ([`BASE_SEQ`]), updating it in place if present
    /// (never stacking a second base). Empty string clears its text.
    fn set_base(&mut self, msg: &str) {
        let base = self.base_ctx();
        if let Some(entry) = self.entries.iter_mut().find(|e| e.ctx == base) {
            entry.msg = msg.to_string();
        } else {
            self.entries.insert(
                0,
                StatusEntry {
                    ctx: base,
                    msg: msg.to_string(),
                },
            );
        }
    }

    /// Push a transient notice; returns its ctx for a later [`pop`](Self::pop).
    fn push(&mut self, msg: &str) -> StatusCtx {
        let ctx = StatusCtx {
            stack: self.id,
            seq: self.next_seq,
        };
        self.next_seq += 1;
        self.entries.push(StatusEntry {
            ctx,
            msg: msg.to_string(),
        });
        ctx
    }

    fn pop(&mut self, ctx: StatusCtx) -> PopOutcome {
        if ctx.stack != self.id {
            return PopOutcome::WrongStack;
        }
        let before = self.entries.len();
        self.entries.retain(|e| e.ctx != ctx);
        if self.entries.len() == before {
            PopOutcome::AlreadyRetracted
        } else {
            PopOutcome::Retracted
        }
    }

    /// The message that should currently show: the topmost entry's, or "".
    fn top(&self) -> &str {
        self.entries.last().map(|e| e.msg.as_str()).unwrap_or("")
    }
}

/// A push/pop message stack driving the single footer status label (GTK4 has no
/// status-bar widget, so the footer is a plain label). Wraps the pure [`MessageStack`] core,
/// re-syncing the label to its [`top`](MessageStack::top) after every mutation.
pub(crate) struct StatusStack {
    label: gtk::Label,
    stack: MessageStack,
}

impl StatusStack {
    pub(crate) fn new(label: gtk::Label) -> Self {
        Self {
            label,
            stack: MessageStack::new(),
        }
    }

    /// What the footer label ACTUALLY reads right now.
    ///
    /// Test-only, and deliberately the widget rather than the model: a notice that
    /// reaches the stack but never the label is the failure a model-level assertion
    /// cannot see.
    ///
    /// Gated to its only callers' cfg (`winstate::busynotice`'s gated tests), not the
    /// broader `cfg(test)` — a bare `cargo test` does not compile them and reported
    /// this as dead.
    #[cfg(all(test, feature = "gtk-integration-tests"))]
    pub(crate) fn label_text(&self) -> String {
        self.label.text().to_string()
    }

    /// Set the persistent base message (the reserved [`BASE_SEQ`] entry). Empty
    /// string clears it.
    pub(crate) fn set_base(&mut self, msg: &str) {
        self.stack.set_base(msg);
        self.sync();
    }

    /// Push a transient notice; returns its ctx for a later [`pop`](Self::pop).
    pub(crate) fn push(&mut self, msg: &str) -> StatusCtx {
        let ctx = self.stack.push(msg);
        self.sync();
        ctx
    }

    /// Retract a pushed notice. A handle issued by a *different* window's stack is
    /// logged, not silently ignored: it means the notice is still up in the window
    /// that showed it and nothing will ever take it down — historically the only
    /// evidence of which was a user noticing a stale footer line hours later.
    /// `warn` is above the forensic threshold (POLICY § Logging), so it reaches a
    /// crash report too.
    pub(crate) fn pop(&mut self, ctx: StatusCtx) {
        if self.stack.pop(ctx) == PopOutcome::WrongStack {
            log::warn!(
                "status stack: refusing to retract {ctx:?} — it was issued by another \
                 window's stack, so the notice it belongs to is stranded there. Push \
                 timed notices through WindowChrome::push_timed_notice (CAM.md, \
                 Status-notice matrix column C)."
            );
            return;
        }
        self.sync();
    }

    /// Whether the label is currently showing nothing — no base message and no
    /// transient notice pushed. Used by tests asserting a silent outcome (e.g.
    /// an unmatched cross-document link fragment) actually stayed silent,
    /// rather than reading the `GtkLabel` text back through the widget tree.
    ///
    /// Gated on the feature as well as `test`, not just `test`: its only caller is
    /// in a `#[cfg(all(test, feature = "gtk-integration-tests"))]` module, so a
    /// bare `#[cfg(test)]` makes it dead code on a plain `cargo clippy
    /// --all-targets` and fails the `-D warnings` gate. Match the caller's gate
    /// exactly rather than reaching for `allow(dead_code)`.
    #[cfg(all(test, feature = "gtk-integration-tests"))]
    pub(crate) fn is_empty(&self) -> bool {
        self.stack.top().is_empty()
    }

    fn sync(&self) {
        self.label.set_text(self.stack.top());
    }
}

#[cfg(test)]
mod tests {
    use super::{MessageStack, PopOutcome};

    #[test]
    fn empty_stack_shows_nothing() {
        assert_eq!(MessageStack::new().top(), "");
    }

    #[test]
    fn base_shows_when_no_transient_pushed() {
        let mut s = MessageStack::new();
        s.set_base("Unsaved changes");
        assert_eq!(s.top(), "Unsaved changes");
    }

    #[test]
    fn set_base_updates_in_place_without_stacking_a_second_base() {
        let mut s = MessageStack::new();
        s.set_base("first");
        s.set_base("second");
        assert_eq!(s.top(), "second");
        assert_eq!(
            s.entries.len(),
            1,
            "the base entry is reused, not re-inserted"
        );
    }

    #[test]
    fn pushed_notice_overrides_base_then_pop_restores_it() {
        let mut s = MessageStack::new();
        s.set_base("base");
        let ctx = s.push("notice");
        assert_eq!(s.top(), "notice");
        s.pop(ctx);
        assert_eq!(s.top(), "base");
    }

    #[test]
    fn push_returns_distinct_nonzero_ctx_ids_and_topmost_shows() {
        let mut s = MessageStack::new();
        let a = s.push("a");
        let b = s.push("b");
        assert_ne!(a, s.base_ctx(), "the base entry's ctx is never handed out");
        assert_ne!(a, b);
        assert_eq!(s.top(), "b");
    }

    /// TDD 16.8 — the cross-window failure this stack cannot prevent, but must
    /// name. A handle belongs to the stack that issued it; popping it against
    /// another window's stack matches nothing there AND leaves the notice up in
    /// the window that showed it, with no error anywhere. `pop` reporting
    /// `WrongStack` is what turns that into a log line.
    #[test]
    fn a_handle_from_another_stack_is_reported_not_silently_ignored() {
        let mut origin = MessageStack::new();
        let mut destination = MessageStack::new();
        let ctx = origin.push("notice");
        destination.set_base("destination base");

        assert_eq!(
            destination.pop(ctx),
            PopOutcome::WrongStack,
            "a foreign handle must be named, not treated as a no-op pop"
        );
        assert_eq!(
            origin.top(),
            "notice",
            "and the notice is still up in the stack that issued it"
        );
        assert_eq!(
            destination.top(),
            "destination base",
            "the destination stack is untouched by a foreign pop"
        );

        // The same handle against its OWN stack retracts, so the check above
        // cannot be passing merely because this handle is unpoppable.
        assert_eq!(origin.pop(ctx), PopOutcome::Retracted);
        assert_eq!(origin.top(), "");
    }

    /// A second pop of an already-retracted notice is expected (a timed pop can
    /// race a condition-driven one) and must stay quiet — it is distinguished
    /// from the foreign-handle case above precisely so only the latter is loud.
    #[test]
    fn a_double_pop_of_this_stacks_own_handle_is_quiet() {
        let mut s = MessageStack::new();
        let ctx = s.push("notice");
        assert_eq!(s.pop(ctx), PopOutcome::Retracted);
        assert_eq!(s.pop(ctx), PopOutcome::AlreadyRetracted);
    }

    #[test]
    fn pop_of_an_earlier_notice_leaves_the_later_one_showing() {
        let mut s = MessageStack::new();
        s.set_base("base");
        let a = s.push("a");
        let _b = s.push("b");
        s.pop(a);
        assert_eq!(s.top(), "b");
    }

    #[test]
    fn base_set_after_a_transient_is_pushed_inserts_beneath_it() {
        let mut s = MessageStack::new();
        let ctx = s.push("notice");
        s.set_base("base");
        // Base is inserted at position 0, so the transient still shows on top.
        assert_eq!(s.top(), "notice");
        s.pop(ctx);
        assert_eq!(s.top(), "base");
    }
}
