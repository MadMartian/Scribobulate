//! Wall-clock formatting for forensic records — allocation-free and
//! dependency-free, because the crash handler needs it.
//!
//! Every timestamp the forensics machinery writes is **UTC**, rendered as
//! `YYYY-MM-DDThh:mm:ss.sssZ`. Two properties decide that, and both are about the
//! reader of a crash report rather than about elegance:
//!
//! * **It correlates with `journalctl`.** The recorded crashes were reconstructed
//!   from kernel and journal lines; a report whose clock cannot be lined up against
//!   those is a report that has to be re-derived by hand.
//! * **It sorts.** Crash report filenames embed this stamp, so lexicographic order
//!   *is* chronological order — which is the whole mechanism behind "notice the
//!   report I have not seen yet" ([`super::report::unread_report`]).
//!
//! Local time is deliberately not offered. Deriving one needs the timezone
//! database, which means `open`/`read`/`malloc` — none of them async-signal-safe,
//! all of them reached from a handler that runs *after* memory is already known to
//! be corrupt. UTC is pure arithmetic on an integer, so the same function serves
//! the log sink and the signal handler with no second implementation to drift.

/// A broken-down UTC timestamp. Plain data: [`civil_from_unix_millis`] computes it,
/// [`Utc::write_iso8601`] renders it, and nothing else in the module knows how a
/// calendar works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Utc {
    pub(crate) year: i64,
    pub(crate) month: u32,
    pub(crate) day: u32,
    pub(crate) hour: u32,
    pub(crate) minute: u32,
    pub(crate) second: u32,
    pub(crate) millis: u32,
}

/// Split a Unix-epoch millisecond count into calendar fields.
///
/// Pure integer arithmetic — no leap seconds (Unix time has none by definition),
/// no timezone lookup, no allocation. Negative inputs (pre-1970) are handled by
/// flooring the division rather than truncating it, so the function is total: a
/// nonsense clock produces a nonsense-but-well-formed date instead of a panic in a
/// crash handler, which is the one place a panic would destroy the evidence.
pub(crate) fn civil_from_unix_millis(unix_millis: i64) -> Utc {
    const MS_PER_DAY: i64 = 86_400_000;
    // Euclidean (flooring) division: `-1 / MS_PER_DAY` must be day -1, not day 0.
    let days = unix_millis.div_euclid(MS_PER_DAY);
    let ms_of_day = unix_millis.rem_euclid(MS_PER_DAY);

    let CivilDate { year, month, day } = civil_from_days(days);
    Utc {
        year,
        month,
        day,
        hour: (ms_of_day / 3_600_000) as u32,
        minute: (ms_of_day / 60_000 % 60) as u32,
        second: (ms_of_day / 1_000 % 60) as u32,
        millis: (ms_of_day % 1_000) as u32,
    }
}

/// A calendar date, so [`civil_from_days`] returns named fields rather than a
/// tuple the caller has to destructure positionally (POLICY § Code style).
struct CivilDate {
    year: i64,
    month: u32,
    day: u32,
}

/// Days since 1970-01-01 → proleptic Gregorian calendar date.
///
/// Howard Hinnant's `civil_from_days` (the algorithm the C++20 `<chrono>` calendar
/// is specified against), transcribed with its era arithmetic intact. It shifts the
/// year to start in March so the leap day lands last and the month-length table
/// collapses into the `(5*doy + 2)/153` expression — which is why there is no table
/// here and no special case for February.
fn civil_from_days(days_since_epoch: i64) -> CivilDate {
    // Re-base onto 0000-03-01, the start of the 400-year era cycle.
    let z = days_since_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097); // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365; // [0, 399]
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100); // [0, 365]
    let month_prime = (5 * day_of_year + 2) / 153; // [0, 11], March = 0
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32; // [1, 31]
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32; // [1, 12]
    CivilDate {
        // January and February belong to the following calendar year.
        year: year + i64::from(month <= 2),
        month,
        day,
    }
}

/// Longest string [`Utc::write_iso8601`] can produce for a four-digit year.
pub(crate) const ISO8601_LEN: usize = 24; // YYYY-MM-DDThh:mm:ss.sssZ

impl Utc {
    /// Render as `YYYY-MM-DDThh:mm:ss.sssZ` into `out`.
    ///
    /// Takes a [`core::fmt::Write`] rather than returning a `String` so the signal
    /// handler can render into a stack buffer ([`super::rawbuf::RawBuf`]) while the
    /// log sink renders into a `String` — one implementation, both callers, no
    /// allocation on the path that cannot afford it.
    pub(crate) fn write_iso8601<W: core::fmt::Write>(&self, out: &mut W) -> core::fmt::Result {
        write!(
            out,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second, self.millis
        )
    }

    /// Render as `YYYYMMDDThhmmssZ` — the same instant with the separators dropped,
    /// for embedding in a filename. Second resolution: two crash reports from one
    /// process in the same second is not a case worth a longer name, and the pid is
    /// in the filename too.
    pub(crate) fn write_compact<W: core::fmt::Write>(&self, out: &mut W) -> core::fmt::Result {
        write!(
            out,
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// The current instant in Unix milliseconds, or `0` if the system clock is set
/// before the epoch. Total by construction — a forensic timestamp is never worth
/// failing a write for.
pub(crate) fn now_unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `now()` rendered as `YYYY-MM-DDThh:mm:ss.sssZ`.
pub(crate) fn now_iso8601() -> String {
    let mut s = String::with_capacity(ISO8601_LEN);
    // Infallible: `String`'s `fmt::Write` never errors.
    let _ = civil_from_unix_millis(now_unix_millis()).write_iso8601(&mut s);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iso(ms: i64) -> String {
        let mut s = String::new();
        civil_from_unix_millis(ms).write_iso8601(&mut s).unwrap();
        s
    }

    #[test]
    fn epoch_renders_as_the_start_of_1970() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn renders_a_known_instant_including_millis() {
        // 2026-07-29T01:54:39.123Z — the shape of the crash that motivated the plan.
        assert_eq!(iso(1_785_290_079_123), "2026-07-29T01:54:39.123Z");
    }

    #[test]
    fn handles_leap_day_and_the_day_either_side_of_it() {
        // 2024-02-28T00:00:00Z, and the two days following it.
        let feb28 = 1_709_078_400_000;
        assert_eq!(iso(feb28), "2024-02-28T00:00:00.000Z");
        assert_eq!(iso(feb28 + 86_400_000), "2024-02-29T00:00:00.000Z");
        assert_eq!(iso(feb28 + 2 * 86_400_000), "2024-03-01T00:00:00.000Z");
    }

    #[test]
    fn nineteen_hundred_is_not_a_leap_year_but_two_thousand_is() {
        // The century rule and its 400-year exception — the two cases a naive
        // `year % 4` implementation gets wrong, and both are reachable from a
        // wrong system clock.
        assert_eq!(iso(-2_203_891_200_000), "1900-03-01T00:00:00.000Z");
        assert_eq!(iso(951_868_800_000), "2000-03-01T00:00:00.000Z");
    }

    #[test]
    fn a_pre_epoch_clock_renders_a_real_date_rather_than_panicking() {
        // A crash handler must not panic on a nonsense clock; flooring division is
        // what makes this total. -1ms is the last millisecond of 1969.
        assert_eq!(iso(-1), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn the_rendered_length_never_exceeds_the_declared_bound() {
        // The signal handler's stack buffer is DERIVED from this constant
        // (`signal::STAMP_CAP`), with a `const` assertion that fails the build if the
        // format outgrows it. That sentence used to be a claim about a hardcoded 32
        // and was simply false — see `STAMP_CAP`'s doc comment. Stating a coupling in
        // prose is not a coupling; this line is now a description of a compile-time
        // dependency rather than a substitute for one.
        assert_eq!(iso(1_785_290_079_123).len(), ISO8601_LEN);
    }

    #[test]
    fn the_compact_form_sorts_chronologically() {
        // Lexicographic order over filenames must equal chronological order —
        // `unread_report` depends on exactly this.
        let mut earlier = String::new();
        let mut later = String::new();
        civil_from_unix_millis(1_785_290_079_000)
            .write_compact(&mut earlier)
            .unwrap();
        civil_from_unix_millis(1_785_290_080_000)
            .write_compact(&mut later)
            .unwrap();
        assert_eq!(earlier, "20260729T015439Z");
        assert!(earlier < later);
    }

    #[test]
    fn millisecond_boundaries_do_not_bleed_between_fields() {
        assert_eq!(iso(999), "1970-01-01T00:00:00.999Z");
        assert_eq!(iso(1_000), "1970-01-01T00:00:01.000Z");
        assert_eq!(iso(59_999), "1970-01-01T00:00:59.999Z");
        assert_eq!(iso(60_000), "1970-01-01T00:01:00.000Z");
    }
}
