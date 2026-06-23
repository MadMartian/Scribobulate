//! A fixed-capacity stack string buffer for the signal handler.
//!
//! `format!` allocates, and `malloc` is **not async-signal-safe**: a SIGSEGV can
//! land while the allocator holds its own lock, and allocating from the handler
//! then deadlocks — the process hangs instead of dying, and the report is never
//! written. That is the failure this type exists to make unrepresentable. It is
//! `unix`-gated because the fatal-signal handler is (see `signal.rs`); every other
//! forensic path runs in ordinary context and uses `String` freely.
//!
//! Writing past capacity **truncates** rather than erroring or growing. A crash
//! report is best-effort by nature: a slightly clipped fault line is worth far more
//! than a handler that gave up. Truncation always lands on a UTF-8 character
//! boundary, so the bytes stay a valid `str` for tests and for any consumer that
//! reads the file back.

use core::fmt;

/// A `[u8; N]` that implements [`core::fmt::Write`] by truncation.
pub(crate) struct RawBuf<const N: usize> {
    bytes: [u8; N],
    len: usize,
    truncated: bool,
}

impl<const N: usize> RawBuf<N> {
    pub(crate) const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
            truncated: false,
        }
    }

    /// The bytes written so far. Always valid UTF-8.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// Whether anything was dropped for want of capacity — so a caller can size a
    /// buffer honestly rather than shipping silent clipping.
    #[cfg(test)]
    pub(crate) fn truncated(&self) -> bool {
        self.truncated
    }

    #[cfg(test)]
    pub(crate) fn as_str(&self) -> &str {
        // Guaranteed by `push_str`'s boundary-respecting truncation.
        core::str::from_utf8(self.as_bytes()).unwrap_or("")
    }

    /// Append as much of `s` as fits, cutting on a character boundary.
    fn push_str(&mut self, s: &str) {
        let room = N - self.len;
        let take = if s.len() <= room {
            s.len()
        } else {
            self.truncated = true;
            super::floor_char_boundary(s, room)
        };
        self.bytes[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
    }
}

impl<const N: usize> fmt::Write for RawBuf<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

/// The largest index `<= limit` that is a character boundary of `s`.
///
/// `str::floor_char_boundary` is still unstable, and this is four lines: UTF-8
/// continuation bytes are exactly those matching `0b10xx_xxxx`, so walking back
/// while one is seen lands on the start of the character that would have been cut.
#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;

    #[test]
    fn writes_and_reads_back_what_fits() {
        let mut buf: RawBuf<32> = RawBuf::new();
        write!(buf, "signal {} at {:#x}", 11, 0x18).unwrap();
        assert_eq!(buf.as_str(), "signal 11 at 0x18");
        assert!(!buf.truncated());
    }

    #[test]
    fn truncates_instead_of_erroring_when_full() {
        let mut buf: RawBuf<8> = RawBuf::new();
        write!(buf, "0123456789").unwrap();
        assert_eq!(buf.as_str(), "01234567");
        assert!(buf.truncated());
    }

    #[test]
    fn truncation_never_splits_a_multi_byte_character() {
        // A path in a breadcrumb can be any UTF-8; splitting it would make the
        // report unreadable as text exactly where it is being read under pressure.
        let mut buf: RawBuf<4> = RawBuf::new();
        write!(buf, "aé€").unwrap(); // 1 + 2 + 3 bytes
        assert_eq!(buf.as_str(), "aé"); // '€' would need byte 4..7
        assert_eq!(buf.as_bytes().len(), 3);
        assert!(buf.truncated());
    }

    #[test]
    fn successive_writes_append() {
        let mut buf: RawBuf<16> = RawBuf::new();
        write!(buf, "a=").unwrap();
        write!(buf, "{}", 42).unwrap();
        assert_eq!(buf.as_str(), "a=42");
    }

    #[test]
    fn an_exactly_full_buffer_is_not_reported_as_truncated() {
        let mut buf: RawBuf<4> = RawBuf::new();
        write!(buf, "abcd").unwrap();
        assert_eq!(buf.as_str(), "abcd");
        assert!(!buf.truncated());
    }
}
