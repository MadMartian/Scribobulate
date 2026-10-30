//! This process's memory footprint and huge-page policy, for the memory-growth gates
//! (`memgate::footprint`). Test-only: a production build has no reader for either.

/// This process's resident set size in bytes, read from `/proc/self/status`.
#[cfg(test)]
pub(crate) fn resident_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_vmrss_kb(&text).map(|kb| kb.saturating_mul(1024))
}

/// Parse `VmRSS:` from a `/proc/<pid>/status` blob. Split from the file read so the
/// grammar is unit-tested with no `/proc`.
#[cfg(test)]
fn parse_vmrss_kb(status: &str) -> Option<u64> {
    for line in status.lines() {
        let Some(rest) = line.strip_prefix("VmRSS:") else {
            continue;
        };
        return rest.split_whitespace().next()?.parse::<u64>().ok();
    }
    None
}

/// Set this process's `PR_SET_THP_DISABLE` flag: whether `khugepaged` may collapse its
/// memory into huge pages. See `memgate::footprint::measuring` for why a measurement
/// turns it off.
#[cfg(all(test, feature = "memory-gates"))]
pub(crate) fn set_huge_page_collapse_disabled(disabled: bool) {
    // SAFETY: PR_SET_THP_DISABLE takes one integer flag and no pointers; the trailing
    // arguments are required to be zero.
    let rc = unsafe {
        libc::prctl(
            libc::PR_SET_THP_DISABLE,
            libc::c_ulong::from(disabled),
            0,
            0,
            0,
        )
    };
    assert_eq!(
        rc,
        0,
        "PR_SET_THP_DISABLE refused: {}",
        std::io::Error::last_os_error()
    );
}

/// The address of `symbol` in the librsvg already loaded into this process, or `None`
/// when none is. `RTLD_NOLOAD` makes the lookup load nothing: only a library some
/// loader has already brought in answers, so this names the decoder in use, never one
/// merely installed.
#[cfg(test)]
pub(crate) fn loaded_librsvg_symbol(symbol: &std::ffi::CStr) -> Option<*const libc::c_void> {
    // SAFETY: dlopen with RTLD_NOLOAD returns a handle to an already-loaded image or
    // NULL; dlsym reads that image's symbol table; dlclose drops only the reference
    // this call took, and the loader that loaded librsvg keeps it mapped, so the
    // address stays valid for the life of the process.
    unsafe {
        let handle = libc::dlopen(
            c"librsvg-2.so.2".as_ptr(),
            libc::RTLD_LAZY | libc::RTLD_NOLOAD,
        );
        if handle.is_null() {
            return None;
        }
        let address = libc::dlsym(handle, symbol.as_ptr());
        libc::dlclose(handle);
        (!address.is_null()).then_some(address.cast_const())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_vmrss_reads_the_kb_field() {
        let blob = "Name:\tfoo\nVmPeak:\t999 kB\nVmRSS:\t  1234 kB\nVmData:\t1 kB\n";
        assert_eq!(super::parse_vmrss_kb(blob), Some(1234));
    }

    #[test]
    fn parse_vmrss_none_when_the_field_is_absent() {
        assert_eq!(super::parse_vmrss_kb("Name:\tfoo\nVmPeak:\t9 kB\n"), None);
    }
}
