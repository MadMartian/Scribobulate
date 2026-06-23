//! An owner-only state directory — a privacy mechanism `std::fs` cannot express here.
//!
//! On unix the state directory is created `0700` and re-tightened if an earlier build
//! left it open. Windows has no mode bits, so the `#[cfg(not(unix))]` branch was a bare
//! `create_dir_all` and the directory's permissions were WHATEVER IT INHERITED. This
//! module is the Windows half of that one behavioural rubric, not a second policy.

use std::ffi::c_void;

use super::{wide, CloseHandle, GetLastError};

// ── Private state directory ───────────────────────────────────────────────────
//
// `std::fs` cannot express this, which is the whole reason the seam exists. On unix
// the state directory is created `0700` and re-tightened if an earlier build left it
// open; the `#[cfg(not(unix))]` branch was a bare `create_dir_all`, so the directory's
// permissions were WHATEVER IT INHERITED.
//
// MEASURED, and it is not theoretical: under the default `%LOCALAPPDATA%` the inherited
// ACL is already owner+SYSTEM+Administrators and nothing is wrong. Point
// `XDG_STATE_HOME` at a second volume and the same code inherits that volume's root
// ACL — `NT AUTHORITY\Authenticated Users:(I)(M)` and `BUILTIN\Users:(I)(RX)` — on the
// state directory, on `swap/`, and on every `.swap` file inside it.
//
// `(M)` is MODIFY, and that is what makes this worse than a disclosure bug. Another
// local user cannot merely READ the unsaved prose in a snapshot; they can REWRITE one,
// and the next launch replays it into the user's document. Crash recovery is a path
// whose entire purpose is to be trusted and replayed without the user re-reading it, so
// a tamperable snapshot is worse than no snapshot at all.
//
// THE FIX IS ON THE DIRECTORY, NOT THE FILES, deliberately. A private file inside a
// traversable directory still advertises its own name, and the directory is the one
// seam that also covers files written by paths that never went through a per-file
// helper. It is also what lets the behavioural rubric be stated once for both platforms
// ("on a platform whose privacy mechanism is not POSIX mode bits, the containing
// directory carries it") rather than forked per OS.

/// `TOKEN_QUERY` — all we need; we read the token's user and nothing else.
const TOKEN_QUERY: u32 = 0x0008;
/// `TokenUser` class for `GetTokenInformation`.
const TOKEN_USER_CLASS: u32 = 1;
/// `SDDL_REVISION_1`.
const SDDL_REVISION_1: u32 = 1;
/// `SE_FILE_OBJECT` — the object type `SetNamedSecurityInfoW` is addressing.
const SE_FILE_OBJECT: u32 = 1;
/// `DACL_SECURITY_INFORMATION`.
const DACL_SECURITY_INFORMATION: u32 = 0x0000_0004;
/// `PROTECTED_DACL_SECURITY_INFORMATION` — **this constant is the fix.** Without it the
/// directory keeps inheriting the parent's ACEs and the permissive ones survive
/// alongside ours; with it, inheritance is severed and only the ACEs below apply.
const PROTECTED_DACL_SECURITY_INFORMATION: u32 = 0x8000_0000;
/// `ERROR_ALREADY_EXISTS` from `CreateDirectoryW`.
const ERROR_ALREADY_EXISTS: u32 = 183;

#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
    fn GetTokenInformation(
        token: *mut c_void,
        class: u32,
        info: *mut c_void,
        len: u32,
        ret_len: *mut u32,
    ) -> i32;
    fn ConvertSidToStringSidW(sid: *mut c_void, out: *mut *mut u16) -> i32;
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        sddl: *const u16,
        revision: u32,
        descriptor: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
    fn GetSecurityDescriptorDacl(
        descriptor: *mut c_void,
        present: *mut i32,
        acl: *mut *mut c_void,
        defaulted: *mut i32,
    ) -> i32;
    fn SetNamedSecurityInfoW(
        name: *mut u16,
        object_type: u32,
        info: u32,
        owner: *mut c_void,
        group: *mut c_void,
        dacl: *mut c_void,
        sacl: *mut c_void,
    ) -> u32;
}

// Read-back half, used only by the privacy test. Kept in its own block so the shipped
// binary declares nothing it does not call — the `-D warnings` gate rejects an unused
// `extern` just as it would an unused function.
#[cfg(test)]
#[link(name = "advapi32")]
extern "system" {
    fn GetNamedSecurityInfoW(
        name: *const u16,
        object_type: u32,
        info: u32,
        owner: *mut *mut c_void,
        group: *mut *mut c_void,
        dacl: *mut *mut c_void,
        sacl: *mut *mut c_void,
        descriptor: *mut *mut c_void,
    ) -> u32;
    fn ConvertSecurityDescriptorToStringSecurityDescriptorW(
        descriptor: *mut c_void,
        revision: u32,
        info: u32,
        out: *mut *mut u16,
        len: *mut u32,
    ) -> i32;
}

/// Copy a NUL-terminated UTF-16 string out of a `LocalAlloc`'d buffer and free it.
///
/// # Safety
/// `raw` must be a non-null, NUL-terminated wide string allocated with `LocalAlloc`,
/// and must not be used again afterwards.
unsafe fn take_local_wide(raw: *mut u16) -> String {
    let mut len = 0usize;
    while *raw.add(len) != 0 {
        len += 1;
    }
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(raw, len));
    LocalFree(raw.cast::<c_void>());
    text
}

/// The DACL of `dir`, as an SDDL string, or `None` if it cannot be read.
///
/// **Exists so a test can assert what the OS STORED, not what we passed in.** Reading
/// back our own inputs would prove nothing — the same argument as
/// `the_caption_follows_the_desktop_theme_and_dwm_stores_it` below, which reads the
/// caption attribute back out of DWM rather than trusting the setter's return code. A
/// DACL that was silently not applied, or applied without the protected bit, is exactly
/// the failure this has to be able to see.
#[cfg(test)]
pub(crate) fn directory_dacl_sddl(dir: &std::path::Path) -> Option<String> {
    let path = wide(&dir.to_string_lossy());
    let mut descriptor: *mut c_void = std::ptr::null_mut();
    // SAFETY: `path` is NUL-terminated and outlives the call; every out-parameter we do
    // not want is null, which the API permits, and `descriptor` is live.
    let status = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 || descriptor.is_null() {
        return None;
    }
    let mut raw: *mut u16 = std::ptr::null_mut();
    // SAFETY: `descriptor` is a valid SD returned by the call above; `raw` is live.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut raw,
            std::ptr::null_mut(),
        )
    };
    // SAFETY: `descriptor` came from GetNamedSecurityInfoW, which allocates with
    // LocalAlloc and documents LocalFree as the release.
    let text = if ok != 0 && !raw.is_null() {
        // SAFETY: `raw` is a NUL-terminated LocalAlloc'd wide string we now own.
        Some(unsafe { take_local_wide(raw) })
    } else {
        None
    };
    // SAFETY: as above; nothing reads `descriptor` after this.
    unsafe { LocalFree(descriptor) };
    text
}

// `CloseHandle` and `GetLastError` are not here: they have no cause of their own and
// live in the parent module, which is why this block is the ones that do.
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn CreateDirectoryW(path: *const u16, security: *const SecurityAttributes) -> i32;
    fn LocalFree(mem: *mut c_void) -> *mut c_void;
}

/// `SECURITY_ATTRIBUTES`. Only `CreateDirectoryW` sees it, and only for the duration of
/// that call, so it never needs to outlive the descriptor it points at.
#[repr(C)]
struct SecurityAttributes {
    length: u32,
    descriptor: *mut c_void,
    inherit_handle: i32,
}

/// The current process user's SID in string form (`S-1-5-21-…`), or `None` if the token
/// cannot be read.
///
/// There is no SDDL alias for "the user running this process" — `CO` (CREATOR OWNER)
/// applies to objects created *later*, not to the directory itself — so the SID has to
/// be resolved and interpolated.
fn current_user_sid() -> Option<String> {
    let mut token: *mut c_void = std::ptr::null_mut();
    // SAFETY: `GetCurrentProcess` returns a pseudo-handle that needs no closing, and
    // `token` is a live out-parameter.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    // Two-call idiom: ask for the size, then read into a buffer of that size.
    let mut needed: u32 = 0;
    // SAFETY: a null buffer with zero length is the documented way to query the size;
    // the call is expected to fail with ERROR_INSUFFICIENT_BUFFER and set `needed`.
    unsafe {
        GetTokenInformation(
            token,
            TOKEN_USER_CLASS,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    let mut buffer = vec![0u8; needed.max(1) as usize];
    // SAFETY: `buffer` is `needed` bytes and outlives the call.
    let got = unsafe {
        GetTokenInformation(
            token,
            TOKEN_USER_CLASS,
            buffer.as_mut_ptr().cast::<c_void>(),
            needed,
            &mut needed,
        )
    };
    if got == 0 {
        // SAFETY: `token` is a valid handle from the successful OpenProcessToken above.
        unsafe { CloseHandle(token) };
        return None;
    }
    // TOKEN_USER is `{ SID_AND_ATTRIBUTES { PSID Sid; DWORD Attributes; } }`, so the SID
    // pointer is the first pointer-sized field.
    // SAFETY: the buffer was filled by GetTokenInformation with a TOKEN_USER, whose
    // first member is the PSID we read here.
    let sid = unsafe { *buffer.as_ptr().cast::<*mut c_void>() };
    let mut raw: *mut u16 = std::ptr::null_mut();
    // SAFETY: `sid` points into `buffer`, which is still alive.
    let converted = unsafe { ConvertSidToStringSidW(sid, &mut raw) };
    // SAFETY: valid handle; nothing below reads through it.
    unsafe { CloseHandle(token) };
    if converted == 0 || raw.is_null() {
        return None;
    }
    // SAFETY: `raw` is a NUL-terminated LocalAlloc'd UTF-16 string owned by us now.
    Some(unsafe { take_local_wide(raw) })
}

/// A self-relative security descriptor carrying an owner-only, protected DACL, plus its
/// SDDL for diagnostics. The caller must `LocalFree` the descriptor.
///
/// `D:PAI` — **P** severs inheritance (the fix), **AI** marks the DACL auto-inherited so
/// the ACEs propagate to `swap/` and to every `.swap` file rather than having to be
/// re-applied per object. `(OICI)` is object+container inherit, `FA` is full access.
///
/// THE THREE PRINCIPALS ARE NOT AN INVENTION. They are exactly what Windows itself puts
/// on a private per-user directory, measured on `%LOCALAPPDATA%\scribobulate` before any
/// of this existed: the user, `SY` (Local System) and `BA` (Administrators), all full.
/// Matching a shape the OS demonstrably considers private beats designing one — and
/// dropping SYSTEM in particular would break backup, indexing and anti-malware in ways
/// that surface later as unrelated bugs rather than as an ACL problem.
fn private_descriptor() -> Result<(*mut c_void, String), String> {
    let sid =
        current_user_sid().ok_or_else(|| "cannot resolve the current user's SID".to_string())?;
    let sddl = format!("D:PAI(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");
    let wide_sddl = wide(&sddl);
    let mut descriptor: *mut c_void = std::ptr::null_mut();
    // SAFETY: `wide_sddl` is NUL-terminated and outlives the call; `descriptor` is a
    // live out-parameter. A null size out-parameter is permitted.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide_sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 || descriptor.is_null() {
        // SAFETY: no allocation to release on this path.
        return Err(format!(
            "building the security descriptor failed (error {}) for {sddl}",
            unsafe { GetLastError() }
        ));
    }
    Ok((descriptor, sddl))
}

/// Create `dir` with an owner-only protected DACL, or tighten it if it already exists.
///
/// **Both halves are load-bearing and neither substitutes for the other**, which is the
/// same split the unix branch makes and for the same reason:
///
/// * **Creation** carries the descriptor into `CreateDirectoryW`, so the directory is
///   never, for any instant, visible to other users. A create-then-tighten leaves a
///   window — and that window is wider here than on unix, because what it would be open
///   *as* is the inherited ACL, which off the profile volume is genuinely permissive
///   rather than merely `0755`.
/// * **Migration** reaches installations that already ran. Anyone who has used an
///   earlier build has an open directory today, and a creation-time-only fix reaches
///   none of them — the exact trap the unix branch's comment calls out.
///
/// Only the LEAF is protected. Ancestors are created with `create_dir_all` and left
/// alone: they are the user's own `XDG_STATE_HOME`, not ours to re-permission.
///
/// **Failure is an error, not a warning.** The unix side treats tightening as
/// best-effort because a `0700`-created directory is the floor underneath a failed
/// chmod. Windows has no such floor: if this fails the directory may be genuinely
/// writable by other local users, and that is precisely the state in which we would
/// otherwise write the user's unsaved prose. Callers that cannot proceed safely should
/// decline rather than continue.
pub(crate) fn create_private_directory(dir: &std::path::Path) -> std::io::Result<()> {
    let io = |msg: String| std::io::Error::new(std::io::ErrorKind::PermissionDenied, msg);

    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (descriptor, sddl) = private_descriptor().map_err(io)?;
    let existed = dir.exists();

    let result = if existed {
        // Migration. `PROTECTED_DACL_SECURITY_INFORMATION` is what severs the inherited
        // ACEs; `DACL_SECURITY_INFORMATION` alone would merge ours alongside the
        // permissive ones and leave the exposure exactly where it was.
        let mut present: i32 = 0;
        let mut acl: *mut c_void = std::ptr::null_mut();
        let mut defaulted: i32 = 0;
        // SAFETY: `descriptor` is a valid SD from `private_descriptor`; all three
        // out-parameters are live.
        let got = unsafe {
            GetSecurityDescriptorDacl(descriptor, &mut present, &mut acl, &mut defaulted)
        };
        if got == 0 || present == 0 {
            Err(format!("the built descriptor carries no DACL ({sddl})"))
        } else {
            let mut path = wide(&dir.to_string_lossy());
            // SAFETY: `path` is NUL-terminated and outlives the call; `acl` points into
            // `descriptor`, which is still alive.
            let status = unsafe {
                SetNamedSecurityInfoW(
                    path.as_mut_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    acl,
                    std::ptr::null_mut(),
                )
            };
            if status == 0 {
                Ok(())
            } else {
                Err(format!("SetNamedSecurityInfoW failed with error {status}"))
            }
        }
    } else {
        let attributes = SecurityAttributes {
            length: std::mem::size_of::<SecurityAttributes>() as u32,
            descriptor,
            inherit_handle: 0,
        };
        let path = wide(&dir.to_string_lossy());
        // SAFETY: both `path` and `attributes` outlive the call, and `attributes.length`
        // matches the struct's real size.
        let created = unsafe { CreateDirectoryW(path.as_ptr(), &attributes) };
        if created != 0 {
            Ok(())
        } else {
            // SAFETY: called immediately after the failing call on this thread.
            let err = unsafe { GetLastError() };
            if err == ERROR_ALREADY_EXISTS {
                // Raced with another instance between `exists()` and here. The other
                // instance created it through this same function, so it is already
                // protected.
                Ok(())
            } else {
                Err(format!("CreateDirectoryW failed with error {err}"))
            }
        }
    };

    // SAFETY: `descriptor` came from ConvertStringSecurityDescriptorToSecurityDescriptorW,
    // which allocates with LocalAlloc, and nothing reads it after this point.
    unsafe { LocalFree(descriptor) };
    result.map_err(io)
}
