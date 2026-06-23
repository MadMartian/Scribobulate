//! The content digest a swap file records for its twin's on-disk baseline.
//!
//! **This is change detection, not integrity.** The digest answers one question at
//! recovery time: *is the file on disk still the same file this snapshot was taken
//! against?* If it is not, the recovery would apply against a stale baseline and must
//! route into the existing external-change conflict flow instead of applying silently.
//!
//! Three consequences of that narrow purpose, each of which decides something here:
//!
//! - **A cryptographic hash would be the wrong tool, not merely an expensive one.** The
//!   adversary this project models is a *document* (untrusted content, POLICY § Input
//!   limits), not a filesystem racing us to forge a digest. A collision here costs a
//!   suppressed conflict prompt on a file the user themselves changed — the same
//!   outcome as the coarse-clock mtime check this project already rejected in favour of
//!   content comparison, and strictly better than it.
//! - **It must be stable across builds.** A swap file is written by one run and read by
//!   the next, which may be a different build of this application or a different Rust
//!   toolchain. `std::collections::hash_map::DefaultHasher` is explicitly documented as
//!   unstable across releases, so using it would make every recovery after an upgrade
//!   report a spurious conflict. This is spelled out because reaching for `DefaultHasher`
//!   is the obvious move and it is silently wrong: nothing fails until months later, in
//!   the one code path whose whole job is to be trustworthy after a crash.
//! - **It must not need a dependency.** POLICY § Dependencies asks for justification
//!   before adding a crate; twenty lines of FNV-1a is cheaper than that conversation and
//!   has no supply chain.
//!
//! FNV-1a at 128 bits, hex-encoded. The length is folded in as well, so two contents
//! that collide in the hash still differ unless they are also the same length.

/// FNV-1a 128-bit offset basis.
const OFFSET_BASIS: u128 = 0x6c62272e_07bb0142_62b82175_6295c58d;
/// FNV-1a 128-bit prime.
const PRIME: u128 = 0x00000000_01000000_00000000_0000013b;

/// The digest of `bytes`, as 32 lowercase hex characters followed by the byte length.
///
/// The length suffix is not decoration: it makes an accidental collision require
/// agreement on both the hash and the size, and it makes a truncated file — the
/// realistic corruption here — differ from its intact original even in the vanishing
/// case where the hash agreed.
pub(crate) fn content_digest(bytes: &[u8]) -> String {
    let mut hash = OFFSET_BASIS;
    for &byte in bytes {
        hash ^= byte as u128;
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:032x}-{}", bytes.len())
}

#[cfg(test)]
mod tests {
    use super::content_digest;

    #[test]
    fn identical_content_digests_identically() {
        assert_eq!(content_digest(b"# Notes\n"), content_digest(b"# Notes\n"));
    }

    #[test]
    fn a_single_byte_change_changes_the_digest() {
        assert_ne!(content_digest(b"# Notes\n"), content_digest(b"# Notet\n"));
    }

    #[test]
    fn truncation_changes_the_digest() {
        assert_ne!(content_digest(b"# Notes\n"), content_digest(b"# Notes"));
    }

    #[test]
    fn empty_content_has_a_digest_rather_than_an_empty_string() {
        // A brand-new untitled document's baseline is empty, and it still has to
        // compare equal to itself on the next run.
        let digest = content_digest(b"");
        assert!(!digest.is_empty());
        assert_eq!(digest, content_digest(b""));
    }

    #[test]
    fn the_digest_has_the_documented_shape() {
        let digest = content_digest(b"abc");
        let (hash, len) = digest
            .split_once('-')
            .expect("digest is <hex>-<len> so recovery can compare it as one opaque string");
        assert_eq!(hash.len(), 32, "128 bits as hex");
        assert!(hash
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(len, "3");
    }

    #[test]
    fn contents_of_equal_length_still_differ() {
        // The length suffix must not be doing all the work.
        assert_ne!(content_digest(b"ab"), content_digest(b"ba"));
    }

    #[test]
    fn the_digest_is_pinned_so_a_refactor_cannot_silently_change_it() {
        // A swap file written by one build is read by the next. If this value ever
        // changes, every swap file in the wild reports a spurious conflict — so this
        // assertion exists to make that a deliberate, visible decision (bump the swap
        // format version with it) rather than an accident of "tidying" the constants.
        assert_eq!(
            content_digest(b"scribobulate"),
            "bb25fab0adf2c0b582400780b9600d20-12"
        );
    }
}
