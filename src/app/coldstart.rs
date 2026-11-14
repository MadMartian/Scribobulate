//! "Is this activation the process's cold start?" — claimed once, synchronously.
//!
//! Its own module, and a measured one, because it is a pure decision that the two
//! GApplication entry points share: `setup::on_activate` (a bare launch) and
//! `openbatch::on_open` (a launch carrying file arguments). Both of those files are
//! GTK wiring and sit outside the coverage gate; the rule below is not wiring, and
//! the defect it replaces was a rule nobody could test.

/// Whether this activation is the process's cold start — claimed **synchronously**,
/// and `true` for exactly one caller per process.
///
/// # Why a latch and not `app.windows().is_empty()`
///
/// That predicate was read as "has this process started up yet", justified by TDD 8.3:
/// a running process can never reach zero windows, so zero windows means brand new.
/// True of a process in *steady state* — and startup is the one interval that premise
/// does not cover. The answer was decided synchronously and then falsified
/// asynchronously, by the very work it authorised.
///
/// Session restore reads each tab's document off the main thread, one at a time. For
/// as long as that takes, no window exists yet. A second bare launch arriving inside
/// that window is handed to the running instance as `activate`, still sees zero
/// windows, and **restores the entire session a second time**: every document in two
/// tabs, each with its own baseline, file monitor and swapfile, so a save in one raises
/// an external-change decision in the other. Crash recovery ran twice for the same
/// reason.
///
/// It is reachable precisely when it hurts. Restore is slowest on slow storage or a
/// large session, and the symptom of a slow start is *nothing happening* — which is
/// what makes a person click the launcher again.
///
/// Every entry point that can start a cold process claims through here, so the claim
/// and the work it authorises cannot be separated by an await.
pub(super) fn claim() -> bool {
    CLAIMED.with(Claim::take)
}

/// How long after a file launch claimed the cold start a bare launch is still owed the
/// session it asked for. Two launches made in one instant start up side by side and
/// finish about together, so the loser's forwarded activation lands moments after the
/// winner's claim whatever the machine's speed; seconds is a generous bound on "moments",
/// and short enough that a launch the user makes later reads as a re-activation.
pub(super) const BARE_LAUNCH_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// A bare launch that lost the race to a launch carrying files — the file launch became
/// primary, so the bare one arrives as a re-activation — is still owed the saved session.
///
/// A launch carrying files restores no session (it opens its files), so without this the
/// session was never restored and the next quit overwrote it with the file window and a
/// blank one. The two launches asked, between them, for the session and the files, and
/// both are owed. Only the first bare launch inside the grace is owed it, so the session
/// is restored once however many land (TDD 8.2a).
pub(super) fn bare_launch_owed_restore() -> bool {
    LATE_RESTORE.with(|l| l.request(std::time::Instant::now()))
}

/// Note that a launch carrying files has just claimed the process's cold start.
pub(super) fn note_open_cold_start() {
    LATE_RESTORE.with(|l| l.begin(std::time::Instant::now()));
}

/// The state behind [`bare_launch_owed_restore`], as a value with the clock passed in so
/// the rule is testable without the process's own copy or any waiting.
struct LateRestore {
    began: std::cell::Cell<Option<std::time::Instant>>,
    spent: std::cell::Cell<bool>,
}

impl LateRestore {
    const fn new() -> Self {
        Self {
            began: std::cell::Cell::new(None),
            spent: std::cell::Cell::new(false),
        }
    }

    fn begin(&self, now: std::time::Instant) {
        self.began.set(Some(now));
        self.spent.set(false);
    }

    /// `true` to the first request inside the grace of a file launch's claim, and to no
    /// other.
    fn request(&self, now: std::time::Instant) -> bool {
        let inside = self
            .began
            .get()
            .is_some_and(|at| now.saturating_duration_since(at) <= BARE_LAUNCH_GRACE);
        inside && !self.spent.replace(true)
    }
}

thread_local! {
    static LATE_RESTORE: LateRestore = const { LateRestore::new() };
}

thread_local! {
    /// Main-thread-only: both callers are GApplication signal handlers, which GTK
    /// emits on the main thread. A process starts up once, so production never resets
    /// it — but a TEST BINARY is one process running many launches, which is what
    /// [`ClaimResetGuard`] exists for.
    static CLAIMED: Claim = const { Claim::new() };
}

/// Put the latch into the state this test requires, and restore it on drop.
///
/// **The production latch is deliberately unresettable, and that is what broke the
/// suite.** Every `#[gtktest::test]` runs on the single main thread of one binary, so
/// the first test to drive `activate` or `open` claimed the process's cold start and
/// every later one saw `false`. The predicate this replaced — "no windows exist yet" —
/// was per-`gtk::Application`, so tests were independent by construction; the fix traded
/// a production race for a suite-ordering coupling, and the guard protecting the `open`
/// recovery route passed only because of where it happened to sort.
///
/// **It takes the answer it wants rather than merely resetting**, and that distinction
/// is the whole fix. A bare reset makes every launching test a cold start, which is just
/// as wrong in the other direction: a test that builds a window and then calls `open` is
/// modelling a launch into a RUNNING instance, and forcing a cold start there runs crash
/// recovery it never expected. Both were observed — resetting unconditionally reddened
/// two `documents` tests that had been green. So a test states which launch it is
/// modelling, and no test's verdict depends on which ran first.
///
/// A guard rather than a setter: a test that set state on the way in and panicked would
/// leave it set for everything after it, which is the same order-dependence one step
/// removed.
#[cfg(all(test, feature = "gtk-integration-tests"))]
#[must_use = "the forced state only holds while the guard is alive"]
pub(crate) fn force_for_test(is_cold_start: bool) -> ClaimGuard {
    // `claim` returns `!claimed`, so "the next claim answers `is_cold_start`" means
    // the flag must hold its negation.
    LATE_RESTORE.with(|l| {
        l.began.set(None);
        l.spent.set(false);
    });
    let previous = CLAIMED.with(|c| c.0.replace(!is_cold_start));
    ClaimGuard { previous }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) struct ClaimGuard {
    previous: bool,
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
impl Drop for ClaimGuard {
    fn drop(&mut self) {
        CLAIMED.with(|c| c.0.set(self.previous));
    }
}

/// The claim itself, as a value rather than as the process's one global.
///
/// Split out so the rule — *first caller wins, every later one loses, for good* — is
/// testable without consuming the process's own claim, which is by design unresettable
/// and so could otherwise be asserted on only once per test binary.
struct Claim(std::cell::Cell<bool>);

impl Claim {
    const fn new() -> Self {
        Self(std::cell::Cell::new(false))
    }

    /// `true` to the first caller only. Named `take` rather than `is_cold_start`
    /// because asking IS claiming: there is deliberately no way to read the answer
    /// without consuming it, which is what stops a caller checking early and acting
    /// after an await.
    fn take(&self) -> bool {
        !self.0.replace(true)
    }
}

#[cfg(test)]
mod tests {
    use super::{Claim, LateRestore, BARE_LAUNCH_GRACE};

    /// The whole contract: one `true`, then `false` for good.
    ///
    /// The second and third calls are what matter. A predicate derived from observable
    /// state — "no windows exist yet" — answers `true` repeatedly for as long as that
    /// state lasts, and that was the defect: the state stayed true across the await
    /// session restore occupies, so a second launch arriving inside it restored the
    /// whole session again.
    #[test]
    fn the_first_caller_claims_the_cold_start_and_no_later_one_can() {
        let claim = Claim::new();
        assert!(claim.take(), "the first caller is the cold start");
        assert!(
            !claim.take(),
            "a second caller is not, even immediately after"
        );
        assert!(!claim.take(), "and it never becomes available again");
    }

    /// A bare launch landing within the grace of a file launch's claim is owed the
    /// session, once; one outside it, or before any file launch, is a re-activation.
    #[test]
    fn a_bare_launch_is_owed_the_restore_only_within_the_grace_and_once() {
        let t0 = std::time::Instant::now();
        let near = t0 + std::time::Duration::from_millis(400);
        let far = t0 + BARE_LAUNCH_GRACE + std::time::Duration::from_millis(1);
        let s = LateRestore::new();
        assert!(!s.request(t0), "no file launch has claimed anything");
        s.begin(t0);
        assert!(
            s.request(near),
            "the first bare launch inside the grace is owed"
        );
        assert!(!s.request(near), "a second one is a re-activation");
        s.begin(t0);
        assert!(!s.request(far), "past the grace it is a re-activation");
    }

    /// Two claims are independent, so the rule above is a property of the claim and
    /// not of some shared global the first test happened to reach first.
    #[test]
    fn two_claims_do_not_share_a_verdict() {
        let a = Claim::new();
        let b = Claim::new();
        assert!(a.take());
        assert!(b.take(), "b's verdict is its own");
        assert!(!a.take());
        assert!(!b.take());
    }
}
