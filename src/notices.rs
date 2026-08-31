//! Test-only. Verifies the generated `THIRD-PARTY-LICENSES.md`.
//!
//! **The file is derived and is not versioned** — `build.rs` writes it on every build
//! from `notices/*.md` plus `two_face::acknowledgement::listing()`, and `.gitignore`
//! keeps it out of git. It used to be committed while *claiming* to be generated, which
//! was false: its whole history was one commit and nothing regenerated it, so a
//! `two-face` bump would have left our legal notices describing a grammar set we no
//! longer ship, with nothing to catch it.
//!
//! The first fix was a staleness gate over the committed copy. That worked, and it was
//! the wrong shape — it policed a failure mode instead of removing one, and it dragged in
//! CRLF normalisation on both sides purely because a derived artefact was round-tripping
//! through git's `text=auto`. Generating the file deletes the whole class: it cannot be
//! stale, it cannot be hand-edited, and its bytes no longer depend on the checkout.
//!
//! **So this module asserts properties of the OUTPUT and deliberately does not
//! re-implement the generator.** A second copy of the rendering logic here — to compare
//! against — would be exactly the duplication whose drift the generator exists to
//! prevent. `build.rs` is the single implementation; these tests are the check that it
//! produced something real, and they run on all three platforms under plain `cargo test`
//! (build-pipeline step 4).

use std::path::PathBuf;

/// The generated artefact, at the repository root. Written by `build.rs`.
const OUTPUT: &str = "THIRD-PARTY-LICENSES.md";
/// Repo-relative directory of hand-authored sections. Versioned; the generator's input.
const PARTS_DIR: &str = "notices";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated() -> String {
        let path = repo_root().join(OUTPUT);
        std::fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!(
                "{} is missing ({err}). build.rs generates it on every build; if this \
                 fails, generation is broken or was skipped.",
                path.display()
            )
        })
    }

    /// Both halves must be present. A generator that emitted only the authored preamble,
    /// or only the `two-face` body, would still produce a plausible-looking file — and
    /// this document's entire purpose is to be a true statement about what we ship.
    #[test]
    fn generated_notices_carry_authored_and_generated_halves() {
        let doc = generated();

        assert!(
            doc.starts_with("# Third-Party Licenses"),
            "authored preamble missing — check notices/*.md ordering"
        );
        assert!(
            doc.contains("\n# Syntaxes\n"),
            "generated syntax listing missing from {OUTPUT}"
        );
        assert!(
            doc.contains("\n# Themes\n"),
            "generated theme listing missing from {OUTPUT}"
        );
        // Guards against a truncated or empty render passing the substring checks above.
        assert!(
            doc.len() > 100_000,
            "{OUTPUT} is implausibly small ({} bytes) — the grammar licence texts alone \
             run to ~200 KB, so this is a truncated render, not a shrunken dependency",
            doc.len()
        );
    }

    /// Every authored section must actually reach the output.
    ///
    /// This is what catches a section that was added to `notices/` and silently dropped —
    /// a filename the generator's `*.md` filter does not match, for instance. Without it,
    /// adding the FreeType disclaimer as a new file could fail to ship while every other
    /// check here stayed green.
    #[test]
    fn every_authored_section_reaches_the_output() {
        let doc = generated();
        let dir = repo_root().join(PARTS_DIR);

        let mut checked = 0usize;
        for entry in std::fs::read_dir(&dir)
            .expect("notices/ not found")
            .flatten()
        {
            let path = entry.path();
            // Assert the EXTENSION rather than filtering on it. Filtering here would
            // reuse the generator's own predicate, so a section named `.txt` or
            // `.markdown` would be skipped by both and ship nowhere, silently -- the
            // exact failure this test claims to catch. MEASURED: a mutant section with
            // the wrong extension survived until this was an assertion.
            assert_eq!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("md"),
                "{} is in {PARTS_DIR}/ but is not a .md file, so the generator ignores \
                 it and its content ships nowhere",
                path.display()
            );
            {
                let section = std::fs::read_to_string(&path).expect("read section");
                // Compare on a non-empty line so trailing-whitespace differences in an
                // authored file cannot make this fail for a cosmetic reason.
                if let Some(anchor) = section.lines().find(|line| !line.trim().is_empty()) {
                    assert!(
                        doc.contains(anchor),
                        "authored section {} is absent from {OUTPUT}: its first line \
                         ({anchor:?}) does not appear in the generated document",
                        path.display()
                    );
                }
                checked += 1;
            }
        }
        assert!(
            checked > 0,
            "no authored sections found in {} — a vacuous pass",
            dir.display()
        );
    }

    /// The artefact ships to users on three platforms; its bytes must not depend on the
    /// checkout. `two-face`'s embedded assets are not uniformly LF, so without the
    /// generator's normalisation this file's size differs per platform for invisible
    /// reasons — which already sent one seat chasing a wrong explanation for a 4,121-byte
    /// discrepancy that was purely line endings.
    #[test]
    fn generated_notices_are_lf_only() {
        let doc = generated();
        assert!(
            !doc.contains('\r'),
            "{OUTPUT} contains CR bytes; the generator must normalise CRLF so the \
             artefact is byte-identical on every platform"
        );
    }
}
