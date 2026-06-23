//! Build identity — what, exactly, was running when it died.
//!
//! Hole 6 of the crash-forensics plan: a recovered crash could not be tied to the
//! build that produced it. Nothing in a stripped, unpackaged binary announces
//! itself, so a report has to carry its own provenance.
//!
//! The fields split into two kinds, and both are needed:
//!
//! * **Provenance** — version and git commit, answering *which source*.
//! * **Fingerprint** — the executable's path, size and modification time,
//!   answering *which binary*. This is the half that survives the reference
//!   platform's constraints: the shipped binary is stripped and the distribution's
//!   GTK carries no symbols (ScrAP-141), so an investigator's only route from a
//!   report back to a resolvable artefact is finding the exact file again. A
//!   commit alone cannot do that — an uncommitted build reports its parent commit
//!   truthfully and is still a different binary; size and mtime tell them apart.

/// The identity of the running process, collected once at startup.
///
/// Plain data with no environment access of its own, so [`block`] — the part whose
/// output is read under pressure — is a pure function of it and is unit-testable
/// without a process to inspect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    pub(crate) version: &'static str,
    pub(crate) commit: &'static str,
    pub(crate) profile: &'static str,
    pub(crate) executable: String,
    /// Size in bytes, and last-modified as a Unix millisecond count. `None` when
    /// the executable cannot be stat'd (it was replaced or unlinked under a
    /// running process — itself worth knowing in a crash report).
    pub(crate) executable_stat: Option<ExecutableStat>,
    pub(crate) gtk_runtime: String,
    pub(crate) gtk_crate: &'static str,
    pub(crate) renderer: String,
    pub(crate) pid: u32,
    pub(crate) started: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExecutableStat {
    pub(crate) bytes: u64,
    pub(crate) modified_unix_millis: i64,
}

/// Collect the identity of this process. Called once, from [`super::install`].
pub(crate) fn collect() -> Identity {
    let executable = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".to_owned());
    Identity {
        version: env!("CARGO_PKG_VERSION"),
        commit: env!("SCRIB_GIT_COMMIT"),
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        executable_stat: executable_stat(),
        executable,
        // Read from the loaded library, not from the headers this was compiled
        // against: the whole point is to name the GTK that is *running*, since a
        // distribution upgrade underneath an installed binary is invisible
        // otherwise. Safe before `gtk::init` — these return compiled-in constants.
        gtk_runtime: format!(
            "{}.{}.{}",
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ),
        gtk_crate: env!("SCRIB_GTK4_CRATE_VERSION"),
        // Recorded rather than assumed: the renderer is set in-process by `run()`,
        // but `.cargo/config.toml` and the environment can both reach it, and it
        // decides which GSK code paths the crash could have come from.
        renderer: std::env::var("GSK_RENDERER").unwrap_or_else(|_| "(unset)".to_owned()),
        pid: std::process::id(),
        started: super::timefmt::now_iso8601(),
    }
}

fn executable_stat() -> Option<ExecutableStat> {
    let meta = std::fs::metadata(std::env::current_exe().ok()?).ok()?;
    let modified_unix_millis = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Some(ExecutableStat {
        bytes: meta.len(),
        modified_unix_millis,
    })
}

/// Render the identity as the fixed header block that opens every persistent log
/// and every crash report.
///
/// Pure, and deliberately so: the signal handler writes these bytes verbatim
/// (pre-rendered at install time, since it can neither allocate nor format), so
/// this function's output *is* the crash report's header. Testing it is the only
/// way to test that header at all.
///
/// **The destructure below is the completeness gate, and it is the whole point of
/// writing it that way** (QA round 5, L-5; ScrAP-216). Reading fields off `id.` one at a
/// time means a field added to [`Identity`] is simply never rendered — the struct grows,
/// the crash report does not, and nothing anywhere says so. That is the same
/// checks-existence-where-correspondence-matters shape as the icon audit walking a
/// hand-maintained array. Binding every field by name makes the omission a COMPILE
/// ERROR ("pattern does not mention field `x`"), which needs no test, cannot rot, and
/// puts the diagnostic on the line that caused it.
///
/// So: do not "tidy" this back into `id.field` accesses, and do not add `..` to the
/// pattern — `..` is exactly the escape hatch that turns the gate off.
pub(crate) fn block(id: &Identity) -> String {
    let Identity {
        version,
        commit,
        profile,
        executable,
        executable_stat,
        gtk_runtime,
        gtk_crate,
        renderer,
        pid,
        started,
    } = id;

    let mut out = String::with_capacity(512);
    out.push_str(&format!("scribobulate {version} ({commit}, {profile})\n"));
    out.push_str(&format!("executable: {executable}\n"));
    match executable_stat {
        Some(stat) => {
            let mut modified = String::new();
            let _ = super::timefmt::civil_from_unix_millis(stat.modified_unix_millis)
                .write_iso8601(&mut modified);
            out.push_str(&format!(
                "executable-stat: {} bytes, modified {modified}\n",
                stat.bytes
            ));
        }
        None => out.push_str("executable-stat: (unavailable)\n"),
    }
    out.push_str(&format!(
        "gtk: runtime {gtk_runtime} (gtk4 crate {gtk_crate})\n"
    ));
    out.push_str(&format!("renderer: {renderer}\n"));
    out.push_str(&format!("pid: {pid}\n"));
    out.push_str(&format!("started: {started}\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Identity {
        Identity {
            version: "0.1.0",
            commit: "a24471e",
            profile: "release",
            executable: "/home/jonathan/.local/bin/scribobulate".to_owned(),
            executable_stat: Some(ExecutableStat {
                bytes: 12_345_678,
                modified_unix_millis: 1_785_290_079_000,
            }),
            gtk_runtime: "4.6.9".to_owned(),
            gtk_crate: "0.10.3",
            renderer: "cairo".to_owned(),
            pid: 1_577_312,
            started: "2026-07-29T13:40:00.000Z".to_owned(),
        }
    }

    #[test]
    fn the_header_names_every_field_an_investigator_needs() {
        let text = block(&sample());
        // Each of these answers a distinct question the recorded crashes could not:
        // which source, which binary, which GTK, which renderer, which journald pid.
        for expected in [
            "scribobulate 0.1.0 (a24471e, release)",
            "executable: /home/jonathan/.local/bin/scribobulate",
            "12345678 bytes, modified 2026-07-29T01:54:39.000Z",
            "gtk: runtime 4.6.9 (gtk4 crate 0.10.3)",
            "renderer: cairo",
            "pid: 1577312",
            "started: 2026-07-29T13:40:00.000Z",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
        }
    }

    #[test]
    fn a_missing_executable_stat_says_so_rather_than_omitting_the_line() {
        // "unavailable" is itself evidence — the binary was replaced or unlinked
        // under the running process. A dropped line would read as a formatting bug.
        let mut id = sample();
        id.executable_stat = None;
        assert!(block(&id).contains("executable-stat: (unavailable)"));
    }

    #[test]
    fn every_line_is_a_key_colon_value_pair_or_the_leading_banner() {
        // The header is read by a human under pressure and grepped by an agent;
        // both depend on the shape staying regular.
        let text = block(&sample());
        let mut lines = text.lines();
        assert!(lines.next().unwrap().starts_with("scribobulate "));
        for line in lines {
            assert!(line.contains(": "), "not a key/value line: {line:?}");
        }
    }

    #[test]
    fn the_collected_identity_describes_this_very_process() {
        // Guards the wiring the pure test above cannot: that `collect` reads the
        // real process rather than returning placeholders.
        let id = collect();
        assert_eq!(id.pid, std::process::id());
        assert_eq!(id.version, env!("CARGO_PKG_VERSION"));
        assert!(
            id.gtk_runtime.starts_with('4'),
            "unexpected GTK runtime {:?}",
            id.gtk_runtime
        );
        assert!(!id.executable.is_empty());
    }
}
