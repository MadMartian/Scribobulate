//! The pure, display-free decision core behind the remote-image cache:
//! URL-keyed LRU eviction under a byte budget, plus short-TTL negative caching
//! for a URL that failed to fetch. Generic over the cached value (`V`) so this
//! module needs no GTK type at all — [`super`] is the only place that
//! instantiates it over `gtk::gdk::Texture` — which is what keeps it inside the
//! coverage gate's scope (POLICY § Build pipeline step 6's "extract the decision
//! core" rule) and lets every rule below be proven by a plain unit test with no
//! display.
//!
//! ## Why a hit can never re-enter the fetch path
//!
//! [`get_or_fetch`] is the single choke point every caller goes through: it
//! calls the supplied `fetch` closure if and only if [`Cache::lookup`]
//! returns [`Lookup::Miss`]. A [`Lookup::Hit`] returns the cached value directly
//! and a live [`Lookup::NegativeHit`] returns `None` directly — neither touches
//! `fetch`. This is a property of the call site, not something every caller has
//! to remember to check, which is the point: the defect this cache exists to
//! remove is a synchronous, uncached fetch running again on every disclosure
//! fold-toggle (ScrAP-34a re-render), so "does a hit call fetch" is the single
//! most load-bearing fact about this module and is proven directly by
//! `a_hit_never_calls_fetch_again` below, by counting calls through the closure
//! rather than by inspecting the code.
//!
//! ## Why negative entries never evict a positive one, and vice versa
//!
//! A negative entry carries no bytes (there is nothing decoded to charge the
//! budget for — it is a marker plus a timestamp) and is never placed in the LRU
//! order, so it can never be the reason a real, decoded image is evicted. It
//! expires purely by [`Cache::negative_ttl`] elapsing, checked on the next
//! lookup. A positive entry is never evicted early by a negative one arriving —
//! recording a failure only ever touches the one key it is about.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The result of asking the cache about a key.
pub(crate) enum Lookup<V> {
    /// A live, decoded value — the LRU order was updated.
    Hit(V),
    /// A remembered failure whose TTL has not yet elapsed. The caller must not
    /// fetch again.
    NegativeHit,
    /// Nothing usable is recorded (never fetched, or a negative entry whose TTL
    /// has elapsed — which this lookup also clears, so the caller's next
    /// `record_*` starts clean).
    Miss,
}

enum Slot<V> {
    Positive { value: V, bytes: usize },
    Negative { inserted_at: Instant },
}

/// A URL-keyed cache with byte-budgeted LRU eviction of successful entries and
/// short-TTL negative caching of failed ones. See the module doc for the two
/// invariants ("a hit never re-fetches", "positive and negative entries cannot
/// evict one another") this type exists to hold.
pub(crate) struct Cache<V> {
    budget_bytes: usize,
    negative_ttl: Duration,
    entries: HashMap<String, Slot<V>>,
    /// LRU order of **positive** entries only, oldest (next eviction candidate)
    /// at the front. Negative entries are never listed here.
    lru: Vec<String>,
    used_bytes: usize,
}

impl<V: Clone> Cache<V> {
    pub(crate) fn new(budget_bytes: usize, negative_ttl: Duration) -> Self {
        Self {
            budget_bytes,
            negative_ttl,
            entries: HashMap::new(),
            lru: Vec::new(),
            used_bytes: 0,
        }
    }

    /// Look up `key` as of `now`. `now` is a parameter, never read from the
    /// system clock internally, so TTL expiry is exercised by unit tests with no
    /// real waiting — a test advances a captured `Instant` by `Duration`
    /// arithmetic instead of sleeping.
    pub(crate) fn lookup(&mut self, key: &str, now: Instant) -> Lookup<V> {
        match self.entries.get(key) {
            Some(Slot::Positive { value, .. }) => {
                let value = value.clone();
                self.touch(key);
                Lookup::Hit(value)
            }
            Some(Slot::Negative { inserted_at }) => {
                if now.duration_since(*inserted_at) >= self.negative_ttl {
                    self.entries.remove(key);
                    Lookup::Miss
                } else {
                    Lookup::NegativeHit
                }
            }
            None => Lookup::Miss,
        }
    }

    /// Record a successful fetch, evicting least-recently-used positive entries
    /// (never negative ones — they carry no bytes) until `bytes` fits the
    /// budget. **A single entry larger than the whole budget is still admitted**
    /// after evicting everything else it can: refusing to cache it would leave
    /// exactly the pathological case this cache exists for — one large image —
    /// re-fetched synchronously on every future toggle, which is worse than
    /// letting the cache run over budget for as long as that one image is the
    /// most recently used entry.
    pub(crate) fn record_success(&mut self, key: String, value: V, bytes: usize) {
        self.remove(&key);
        while self.used_bytes + bytes > self.budget_bytes && !self.lru.is_empty() {
            let victim = self.lru.remove(0);
            self.remove(&victim);
        }
        self.entries
            .insert(key.clone(), Slot::Positive { value, bytes });
        self.lru.push(key);
        self.used_bytes += bytes;
    }

    /// Record that `key` failed to fetch as of `now`, replacing any prior entry
    /// (positive or negative) for the same key — and sweep the negative entries that
    /// have expired since the last failure.
    ///
    /// **The sweep is what BOUNDS this half of the cache.** A negative entry carries no
    /// bytes, so it is deliberately outside the byte budget and outside the LRU — which
    /// left nothing at all limiting how many there could be. `lookup` removes an expired
    /// entry only when someone looks that key up again, so a document whose image URLs
    /// each fail once and are never re-requested (a rename, a reload, a tab closed) left
    /// one entry per URL in a thread-local that lives as long as the process.
    ///
    /// Swept HERE rather than on a timer or on every lookup: a failure is the only event
    /// that can grow this set, so it is the one point where a bound is owed, and it is
    /// already rare — each one costs a connect timeout, so they cannot arrive quickly
    /// enough for an O(entries) pass to matter.
    pub(crate) fn record_failure(&mut self, key: String, now: Instant) {
        self.remove(&key);
        self.sweep_expired_negatives(now);
        self.entries
            .insert(key, Slot::Negative { inserted_at: now });
    }

    /// Drop every negative entry whose TTL has elapsed as of `now`.
    ///
    /// Positive entries are untouched: they expire by eviction under the byte budget,
    /// never by time, and the two policies must not start reaching into each other —
    /// that separation is one of the module's two stated invariants.
    fn sweep_expired_negatives(&mut self, now: Instant) {
        let ttl = self.negative_ttl;
        self.entries.retain(|_, slot| match slot {
            Slot::Negative { inserted_at } => now.duration_since(*inserted_at) < ttl,
            Slot::Positive { .. } => true,
        });
    }

    /// How many negative entries the cache is holding. Test-only: the bound this exists
    /// to prove is invisible from every other observable — an unswept entry changes no
    /// answer `lookup` gives, which is exactly why it went unnoticed.
    #[cfg(test)]
    pub(crate) fn negative_entry_count(&self) -> usize {
        self.entries
            .values()
            .filter(|s| matches!(s, Slot::Negative { .. }))
            .count()
    }

    fn touch(&mut self, key: &str) {
        if let Some(pos) = self.lru.iter().position(|k| k == key) {
            let k = self.lru.remove(pos);
            self.lru.push(k);
        }
    }

    fn remove(&mut self, key: &str) {
        if let Some(Slot::Positive { bytes, .. }) = self.entries.remove(key) {
            self.used_bytes -= bytes;
            if let Some(pos) = self.lru.iter().position(|k| k == key) {
                self.lru.remove(pos);
            }
        }
    }

    #[cfg(test)]
    fn contains_positive(&self, key: &str) -> bool {
        matches!(self.entries.get(key), Some(Slot::Positive { .. }))
    }
}

/// Look `key` up and, only on an outright miss, run `fetch` and record its outcome.
///
/// **Takes the `RefCell`, not `&mut Cache`, and that is the whole reason it exists.**
/// The obvious spelling is a `&mut self` method taking the closure — but a caller then
/// has to hold its borrow across `fetch`, and this cache lives behind a `thread_local!`
/// `RefCell`. A `fetch` that re-entered (directly, or by pumping the main loop, which
/// is what making it async would do) would hit a second `borrow_mut` and ABORT the
/// process rather than fail — ScrAP-53's shape. Owning the borrow here means it is
/// released before `fetch` runs and retaken after, so re-entry is merely a second
/// lookup. Proved by `a_reentrant_fetch_does_not_abort` below, which calls back into
/// the same cache from inside the closure.
pub(crate) fn get_or_fetch<V: Clone>(
    cell: &std::cell::RefCell<Cache<V>>,
    key: &str,
    now: Instant,
    fetch: impl FnOnce() -> Option<(V, usize)>,
) -> Option<V> {
    match cell.borrow_mut().lookup(key, now) {
        Lookup::Hit(value) => return Some(value),
        Lookup::NegativeHit => return None,
        Lookup::Miss => {}
    }
    match fetch() {
        Some((value, bytes)) => {
            cell.borrow_mut()
                .record_success(key.to_string(), value.clone(), bytes);
            Some(value)
        }
        None => {
            cell.borrow_mut().record_failure(key.to_string(), now);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cache, Lookup};
    use std::time::{Duration, Instant};

    /// A `usize` value is its own byte size, which keeps every test's numbers
    /// legible: inserting "a value of size N" is `record_success(key, N, N)`.
    fn cache(budget_bytes: usize, negative_ttl_secs: u64) -> Cache<usize> {
        Cache::new(budget_bytes, Duration::from_secs(negative_ttl_secs))
    }

    #[test]
    fn a_miss_on_an_empty_cache_is_a_miss() {
        let mut c = cache(100, 60);
        assert!(matches!(c.lookup("a", Instant::now()), Lookup::Miss));
    }

    #[test]
    fn a_successful_entry_is_a_hit_afterwards() {
        let mut c = cache(100, 60);
        c.record_success("a".into(), 7, 3);
        match c.lookup("a", Instant::now()) {
            Lookup::Hit(value) => assert_eq!(value, 7),
            _ => panic!("expected a hit"),
        }
    }

    // ── byte-budget enforcement ─────────────────────────────────────────────

    #[test]
    fn eviction_only_fires_once_the_budget_is_exceeded() {
        let mut c = cache(9, 60);
        c.record_success("a".into(), 1, 3);
        c.record_success("b".into(), 1, 3);
        c.record_success("c".into(), 1, 3);
        // Exactly at the budget (9) — nothing evicted.
        assert!(c.contains_positive("a"));
        assert!(c.contains_positive("b"));
        assert!(c.contains_positive("c"));
    }

    #[test]
    fn inserting_past_the_budget_evicts_the_least_recently_used_entry() {
        let mut c = cache(9, 60);
        c.record_success("a".into(), 1, 3);
        c.record_success("b".into(), 1, 3);
        c.record_success("c".into(), 1, 3);
        // 9 used; "d" (3) needs one entry evicted. "a" is the LRU candidate —
        // nothing has touched it since insertion.
        c.record_success("d".into(), 1, 3);
        assert!(
            !c.contains_positive("a"),
            "least-recently-used should evict first"
        );
        assert!(c.contains_positive("b"));
        assert!(c.contains_positive("c"));
        assert!(c.contains_positive("d"));
    }

    #[test]
    fn a_hit_promotes_an_entry_out_of_eviction_order() {
        let mut c = cache(9, 60);
        c.record_success("a".into(), 1, 3);
        c.record_success("b".into(), 1, 3);
        c.record_success("c".into(), 1, 3);
        // Touch "a" so it is no longer the LRU candidate — "b" becomes it.
        assert!(matches!(c.lookup("a", Instant::now()), Lookup::Hit(_)));
        c.record_success("d".into(), 1, 3);
        assert!(
            c.contains_positive("a"),
            "a hit was just promoted, must survive"
        );
        assert!(
            !c.contains_positive("b"),
            "b is now the least-recently-used"
        );
        assert!(c.contains_positive("c"));
        assert!(c.contains_positive("d"));
    }

    #[test]
    fn eviction_walks_lru_order_until_the_new_entry_fits() {
        let mut c = cache(10, 60);
        c.record_success("a".into(), 1, 4);
        c.record_success("b".into(), 1, 4);
        // 8 used; a 6-byte entry only needs "a" evicted — evicting it leaves
        // 4 used + 6 = 10, which fits — so this also proves eviction stops as
        // soon as it fits rather than always draining everything.
        c.record_success("c".into(), 1, 6);
        assert!(!c.contains_positive("a"));
        assert!(
            c.contains_positive("b"),
            "evicting just a already made room"
        );
        assert!(c.contains_positive("c"));
    }

    #[test]
    fn a_single_entry_larger_than_the_whole_budget_is_still_cached() {
        let mut c = cache(10, 60);
        c.record_success("a".into(), 1, 4);
        // "huge" alone exceeds the entire budget; "a" is evicted to make what
        // room exists, and "huge" is admitted anyway rather than refused.
        c.record_success("huge".into(), 1, 50);
        assert!(!c.contains_positive("a"));
        assert!(
            c.contains_positive("huge"),
            "must not refuse a legitimate fetch"
        );
    }

    // ── negative caching ─────────────────────────────────────────────────────

    #[test]
    fn a_failure_is_a_negative_hit_before_its_ttl_elapses() {
        let mut c = cache(100, 60);
        let t0 = Instant::now();
        c.record_failure("dead".into(), t0);
        assert!(matches!(
            c.lookup("dead", t0 + Duration::from_secs(30)),
            Lookup::NegativeHit
        ));
    }

    #[test]
    fn a_failure_expires_to_a_miss_once_its_ttl_elapses() {
        let mut c = cache(100, 60);
        let t0 = Instant::now();
        c.record_failure("dead".into(), t0);
        assert!(matches!(
            c.lookup("dead", t0 + Duration::from_secs(61)),
            Lookup::Miss
        ));
    }

    #[test]
    fn an_expired_negative_entry_is_gone_after_being_observed() {
        let mut c = cache(100, 60);
        let t0 = Instant::now();
        c.record_failure("dead".into(), t0);
        // Observe the expiry once...
        let _ = c.lookup("dead", t0 + Duration::from_secs(61));
        // ...and a fresh success afterwards is not blocked by a stale entry.
        c.record_success("dead".into(), 9, 1);
        assert!(matches!(
            c.lookup("dead", t0 + Duration::from_secs(62)),
            Lookup::Hit(9)
        ));
    }

    #[test]
    fn negative_entries_never_count_against_the_byte_budget() {
        let mut c = cache(5, 60);
        let t0 = Instant::now();
        for i in 0..50 {
            c.record_failure(format!("dead-{i}"), t0);
        }
        // 50 negative entries recorded; a positive one still fits the budget
        // untouched, because negatives carry no bytes and are never evicted for
        // budget pressure.
        c.record_success("ok".into(), 1, 5);
        assert!(c.contains_positive("ok"));
    }

    #[test]
    fn a_success_after_a_negative_hit_replaces_it() {
        let mut c = cache(100, 60);
        let t0 = Instant::now();
        c.record_failure("flaky".into(), t0);
        c.record_success("flaky".into(), 42, 1);
        assert!(matches!(
            c.lookup("flaky", t0 + Duration::from_secs(1)),
            Lookup::Hit(42)
        ));
    }

    // ── get_or_fetch: the "a hit never re-enters the fetch path" contract ────

    #[test]
    fn a_hit_never_calls_fetch_again() {
        let c = std::cell::RefCell::new(cache(100, 60));
        let now = Instant::now();
        let mut fetch_calls = 0;

        let first = super::get_or_fetch(&c, "a", now, || {
            fetch_calls += 1;
            Some((99, 3))
        });
        assert_eq!(first, Some(99));
        assert_eq!(fetch_calls, 1);

        let second = super::get_or_fetch(&c, "a", now, || {
            fetch_calls += 1;
            Some((1234, 3)) // a different value — if this ran, the test below
                            // for staleness would notice.
        });
        assert_eq!(
            second,
            Some(99),
            "must return the cached value, not re-fetch"
        );
        assert_eq!(fetch_calls, 1, "fetch must not run again on a hit");
    }

    #[test]
    fn a_live_negative_hit_never_calls_fetch_again() {
        let c = std::cell::RefCell::new(cache(100, 60));
        let now = Instant::now();
        let mut fetch_calls = 0;

        let first: Option<usize> = super::get_or_fetch(&c, "dead", now, || {
            fetch_calls += 1;
            None
        });
        assert_eq!(first, None);
        assert_eq!(fetch_calls, 1);

        let second = super::get_or_fetch(&c, "dead", now + Duration::from_secs(1), || {
            fetch_calls += 1;
            None
        });
        assert_eq!(second, None);
        assert_eq!(
            fetch_calls, 1,
            "fetch must not run again during the negative TTL"
        );
    }

    #[test]
    fn fetch_runs_again_once_the_negative_ttl_has_elapsed() {
        let c = std::cell::RefCell::new(cache(100, 60));
        let t0 = Instant::now();
        let mut fetch_calls = 0;

        let _: Option<usize> = super::get_or_fetch(&c, "dead", t0, || {
            fetch_calls += 1;
            None
        });
        let retried = super::get_or_fetch(&c, "dead", t0 + Duration::from_secs(61), || {
            fetch_calls += 1;
            Some((7, 1))
        });
        assert_eq!(retried, Some(7));
        assert_eq!(
            fetch_calls, 2,
            "an expired negative entry must allow a retry"
        );
    }

    /// **A `fetch` that re-enters the cache must not abort the process.**
    ///
    /// The guard for the borrow discipline in `get_or_fetch`, and the reason that
    /// function takes the `RefCell` rather than `&mut Cache`. A `RefCell` double
    /// `borrow_mut` panics, and in a GTK signal handler a panic takes the process and
    /// any unsaved work with it — so this cannot be left to "the fetch does not
    /// re-enter today". Re-entry becomes reachable the moment the fetch is made async,
    /// because pumping the main loop lets a second image ask the cache mid-fetch.
    #[test]
    fn a_reentrant_fetch_does_not_abort() {
        let c = std::cell::RefCell::new(cache(100, 60));
        let now = Instant::now();

        let outer = super::get_or_fetch(&c, "outer", now, || {
            // Exactly what an async fetch would allow: another lookup, and another
            // record, while the outer fetch is still in flight.
            let inner = super::get_or_fetch(&c, "inner", now, || Some((1, 1)));
            assert_eq!(inner, Some(1), "the re-entrant call answers normally");
            Some((2, 1))
        });
        assert_eq!(outer, Some(2));
        assert_eq!(
            super::get_or_fetch(&c, "inner", now, || Some((999, 1))),
            Some(1),
            "and what it recorded survives the outer call completing"
        );
    }

    #[test]
    fn expired_negative_entries_are_swept_by_the_next_failure() {
        // The unbounded case: N URLs each fail once and are never looked up again, so
        // `lookup`'s own expiry never runs for any of them. Without the sweep this holds
        // all N for the life of the process.
        let mut c = cache(100, 60);
        let t0 = Instant::now();
        for i in 0..50 {
            c.record_failure(format!("dead-{i}"), t0);
        }
        assert_eq!(
            c.negative_entry_count(),
            50,
            "precondition: all 50 are held"
        );

        // One more failure, a TTL later.
        c.record_failure("fresh".into(), t0 + Duration::from_secs(61));
        assert_eq!(
            c.negative_entry_count(),
            1,
            "the 50 expired entries are gone and only the fresh one remains"
        );
    }

    #[test]
    fn the_sweep_keeps_negative_entries_that_are_still_live() {
        let mut c = cache(100, 60);
        let t0 = Instant::now();
        c.record_failure("still-live".into(), t0);
        c.record_failure("newer".into(), t0 + Duration::from_secs(30));
        assert_eq!(
            c.negative_entry_count(),
            2,
            "an entry inside its TTL survives a sweep"
        );
        assert!(matches!(
            c.lookup("still-live", t0 + Duration::from_secs(31)),
            Lookup::NegativeHit
        ));
    }

    #[test]
    fn the_sweep_never_touches_positive_entries() {
        // The module's second invariant: positive and negative entries cannot evict one
        // another. A time-based sweep is exactly the shape that would breach it.
        let mut c = cache(100, 60);
        let t0 = Instant::now();
        c.record_success("kept".into(), 7, 3);
        c.record_failure("dead".into(), t0);
        c.record_failure("later".into(), t0 + Duration::from_secs(61));
        match c.lookup("kept", t0 + Duration::from_secs(999)) {
            Lookup::Hit(value) => assert_eq!(value, 7, "a positive entry never expires by time"),
            _ => panic!("expected a hit — the sweep took a positive entry"),
        }
    }
}
