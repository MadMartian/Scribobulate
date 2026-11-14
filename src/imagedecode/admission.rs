//! Admission of a local image path — the same two-part test a document gets (POLICY §
//! Input limits), applied here for the first time to a local image.
//!
//! Before this module existed, a local image went straight to `GdkTexture`, which read
//! the whole file with no cap and no file-type check of its own. Sniffing by content
//! means THIS project's own code reads the file first, so it inherits the same
//! obligation every other untrusted-path read in this crate already carries.

use std::path::Path;
use std::sync::Arc;

/// Why a local image path was refused before a single content byte was decoded.
///
/// Distinct from [`crate::limits::LoadRefusal`] only in wording — `Display` here talks
/// about an *image*, not a *document* — which is exactly why `limits::is_regular_file_within`
/// takes the cap as a parameter rather than being called on our behalf: "a caller
/// passing a different cap matches the variant and words its own diagnostic" (that
/// function's own doc comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// A regular file, but larger than the configured local-image byte cap.
    TooLarge { bytes: u64 },
    /// Not a regular file — a FIFO, socket, block/character device, or the path could
    /// not be `stat`ed at all. Reading it could block the main thread forever or never
    /// end, so it is never opened.
    NotARegularFile,
    /// The path could not be `stat`ed, opened or read — missing, permission denied, an
    /// I/O error. Not the same fact as [`NotARegularFile`](Self::NotARegularFile): the
    /// kind is kept so the log names what actually failed. (The kind, not the OS text,
    /// so the record is the same on every platform and locale.)
    Unreadable { kind: std::io::ErrorKind },
}

impl From<crate::limits::LoadRefusal> for Refusal {
    fn from(r: crate::limits::LoadRefusal) -> Self {
        match r {
            crate::limits::LoadRefusal::TooLarge { bytes } => Refusal::TooLarge { bytes },
            crate::limits::LoadRefusal::NotARegularFile => Refusal::NotARegularFile,
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::TooLarge { bytes } => write!(
                f,
                "image is {:.1} MiB, over the configured {:.0} MiB limit",
                *bytes as f64 / (1024.0 * 1024.0),
                local_file_limit_bytes() as f64 / (1024.0 * 1024.0)
            ),
            Refusal::Unreadable { kind } => write!(f, "could not be read: {kind}"),
            Refusal::NotARegularFile => {
                // A DIRECTORY is the case this reaches on Windows, which has no FIFO
                // form at all — the Windows seat measured `metadata()` succeeding on a
                // directory named `x.gif` with `is_file = false` and `len = 0`, i.e. the
                // same two properties a FIFO has and the same limb refusing it. Naming
                // only pipes and sockets made the one string this platform can produce
                // wrong about itself.
                write!(
                    f,
                    "not a regular file (a directory, pipe, socket or device)"
                )
            }
        }
    }
}

/// The operator-configured local-image byte cap, in bytes. Defaults to
/// [`crate::limits::MAX_LOCAL_IMAGE_BYTES`]; `config.toml`'s `[images]` section
/// overrides it (already clamped to 1..=64 MiB by `config::ImagesConfig::clamped`).
fn local_file_limit_bytes() -> u64 {
    const BYTES_PER_MIB: u64 = 1024 * 1024;
    crate::config::config().images.local_file_limit_mib * BYTES_PER_MIB
}

fn unreadable(err: std::io::Error) -> Refusal {
    Refusal::Unreadable { kind: err.kind() }
}

/// Admit and read a local image file — a REGULAR file, within the configured byte
/// cap, exactly as a document is admitted (POLICY § Input limits), and for the same
/// reason: a FIFO named `x.gif` must never block the main thread, and a huge regular
/// file must never be read whole just to learn it should have been refused.
///
/// Two separate defences against two separate races. The type-plus-size check runs
/// against `stat`-time metadata, **before any byte is read** — a TOCTOU on the file's
/// *type* (swapped for a FIFO between `stat` and `open`) is an accepted residual, the
/// same one `crate::limits`'s own doc comment names, and not one this function can
/// close without `openat2(RESOLVE_BENEATH)`. But a TOCTOU on the file's *size* — it
/// grows after the `stat` and before the read completes — is closed here: the read
/// itself is bounded with `Read::take(cap + 1)`, so a file that grows mid-read still
/// cannot exceed the cap by more than the one byte needed to detect it.
pub(crate) fn read_local(path: &Path) -> Result<Arc<[u8]>, Refusal> {
    use std::io::Read;

    let cap = local_file_limit_bytes();
    let meta = std::fs::metadata(path).map_err(unreadable)?;
    crate::limits::is_regular_file_within(&meta, cap).map_err(Refusal::from)?;

    let file = std::fs::File::open(path).map_err(unreadable)?;
    // `+ 1`: enough to tell "exactly at the cap" from "over it" without ever reading
    // more than one byte past the limit, whatever the file grows to mid-read.
    let mut limited = file.take(cap + 1);
    let mut buf = Vec::new();
    limited.read_to_end(&mut buf).map_err(unreadable)?;
    if buf.len() as u64 > cap {
        return Err(Refusal::TooLarge {
            bytes: buf.len() as u64,
        });
    }
    Ok(Arc::from(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_regular_file_is_read_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.png");
        std::fs::write(&path, b"not really a png, just bytes").unwrap();
        let bytes = read_local(&path).expect("within every default cap");
        assert_eq!(&*bytes, b"not really a png, just bytes");
    }

    #[test]
    fn a_missing_file_is_refused_as_unreadable_not_as_a_non_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.png");
        let refusal = read_local(&path).unwrap_err();
        assert_eq!(
            refusal,
            Refusal::Unreadable {
                kind: std::io::ErrorKind::NotFound
            }
        );
        assert!(!refusal.to_string().contains("regular"), "{refusal}");
    }

    /// TDD 6.8 / the logged reason: a regular file the process may not open fails to
    /// OPEN, and the record says so rather than calling it a pipe or device.
    #[test]
    fn a_file_without_read_permission_is_logged_as_unreadable() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("locked.png");
            std::fs::write(&path, b"x").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
            if std::fs::File::open(&path).is_ok() {
                crate::testsymlink::skipped(
                    "TDD 6.8",
                    "the process ignores file permissions (root)",
                );
                return;
            }
            let refusal = read_local(&path).unwrap_err();
            assert_eq!(
                refusal,
                Refusal::Unreadable {
                    kind: std::io::ErrorKind::PermissionDenied
                }
            );
            let text = refusal.to_string();
            assert!(text.contains("permission denied"), "{text}");
            assert!(!text.contains("regular"), "{text}");
        }
        #[cfg(not(unix))]
        crate::testsymlink::skipped("TDD 6.8", "permission bits are unix-only here");
    }

    /// A DIRECTORY carrying an image name — the non-regular file every platform can
    /// produce, and the ONLY one Windows can: it has no FIFO form, so the unix-only
    /// FIFO test below skips there and this limb would otherwise go unchecked on the
    /// platform whose users are most likely to meet it. It stands in for a FIFO on
    /// both halves rather than one: the Windows seat measured `metadata()` succeeding
    /// with `is_file = false` AND `len = 0`, so a size-only admission check would sail
    /// past it exactly as it would past a pipe.
    #[test]
    fn a_directory_named_like_an_image_is_refused_on_every_platform() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notregular.gif");
        std::fs::create_dir(&path).unwrap();
        assert_eq!(read_local(&path), Err(Refusal::NotARegularFile));
    }

    /// The byte cap must be established BEFORE the read, using a sparse file so the
    /// test does not need to write 16 MiB+ of real data (same technique as
    /// `limits::tests::an_oversized_regular_file_is_refused_and_says_by_how_much`).
    #[test]
    fn an_oversized_local_file_is_refused_before_the_read_completes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bomb.gif");
        let cap = local_file_limit_bytes();
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(cap + 1).unwrap();
        drop(f);
        assert_eq!(read_local(&path), Err(Refusal::TooLarge { bytes: cap + 1 }));
    }

    #[test]
    fn a_file_exactly_at_the_cap_is_admitted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("edge.gif");
        let cap = local_file_limit_bytes();
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(cap).unwrap();
        drop(f);
        let bytes = read_local(&path).expect("exactly at the cap is admitted");
        assert_eq!(bytes.len() as u64, cap);
    }

    /// TDD 2.23b: a FIFO named `.gif` must yield a refusal and must NEVER block the
    /// main thread waiting for a writer that will never come.
    ///
    /// Compiled on every platform (never `#[cfg(unix)]` — a cfg'd-out test is deleted,
    /// not skipped); FIFO admission is inherently a unix-only scenario (Windows named
    /// pipes are not filesystem entries `metadata` reports on the way a FIFO is), so
    /// elsewhere this prints a runtime `SKIPPED [TDD 2.23b]: …` rather than vanishing.
    #[test]
    fn a_fifo_named_gif_is_refused_and_does_not_block() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("x.gif");
            let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
                .expect("path has no interior NUL");
            // SAFETY: `c_path` is a valid NUL-terminated string that outlives the
            // call; `mkfifo` only reads it.
            let rc = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
            assert_eq!(rc, 0, "mkfifo failed: {}", std::io::Error::last_os_error());
            let meta = std::fs::metadata(&path).unwrap();
            assert!(meta.file_type().is_fifo(), "precondition: it is a FIFO");

            // If `read_local` ever called `File::open` on this path, opening for
            // read would block forever (no writer will ever connect) and this test
            // would hang rather than fail — so completing at all is part of what is
            // being asserted, not just the return value.
            assert_eq!(read_local(&path), Err(Refusal::NotARegularFile));
        }
        #[cfg(not(unix))]
        crate::testsymlink::skipped(
            "TDD 2.23b",
            "FIFO admission is unix-only: Windows named pipes are not filesystem \
             entries in the sense `std::fs::metadata` reports one",
        );
    }
}
