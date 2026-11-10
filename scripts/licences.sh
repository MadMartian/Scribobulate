#!/usr/bin/env bash
#
# The licence gate over the shipped Rust dependency graph (pipeline step 8b).
#
# THREE CHECKS, ONE VERDICT. (1) every crate's licence is on deny.toml's allowlist;
# (2) every RustSec advisory against Cargo.lock is satisfied or waived there; (3) the
# committed crate-notice section, notices/50-crates.md, is byte-identical to what
# cargo-about generates from Cargo.lock now. The first two are cargo-deny's; the third
# is this script's, because the build cannot run the generator itself (cargo-about is
# not a build tool and needs registry metadata), so the section is a COMMITTED COPY of a
# derived artefact -- the shape build.rs's own header argues against for a file the
# build can produce, and the honest shape for one it cannot. The drift check is what
# keeps that copy from going stale silently, which is the only way a committed copy
# fails.
#
# `--update` REGENERATES the section in place. It is the one way the file is written:
# a hand edit is overwritten by the next --update and flagged by the next check, so
# "fix the drift" always means "run --update and commit", never "edit the notices".
#
# LINE ENDINGS ARE NORMALISED TO LF ON WRITE. Several crates' licence texts carry CRLF
# inside the crate's own data; git's text=auto would store them as LF and check them out
# as they are, so an un-normalised generate would differ from the committed copy on
# every run by invisible bytes -- the discrepancy that sent one seat chasing a wrong
# explanation for THIRD-PARTY-LICENSES.md (build.rs does the same for the same reason).
#
# Both tools: `cargo install cargo-deny cargo-about --locked --features cli` (the
# feature is cargo-about's; cargo-deny ignores it). RELEASE.md gates 2 and 6 cite this
# script; sdd/TECH.md's "Dependencies <-> Distribution" boundary is the map.

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SECTION="$REPO_DIR/notices/50-crates.md"
TEMPLATE="$REPO_DIR/about.hbs"

usage() {
    echo "usage: $0 [--update]" >&2
    echo "  (no flag)  run the licence allowlist, the advisory check and the notice drift check" >&2
    echo "  --update   regenerate notices/50-crates.md in place (then commit it)" >&2
    exit 2
}

need() {
    if ! cargo "$1" --version >/dev/null 2>&1; then
        echo "licences: cargo-$1 is not installed: cargo install cargo-$1 --locked${2-}" >&2
        exit 1
    fi
}

# Writes the generated section, LF-normalised, to the path in $1.
generate() {
    local out="$1" raw
    raw="$(mktemp)"
    trap 'rm -f "$raw"' RETURN
    (cd "$REPO_DIR" && cargo about generate "$TEMPLATE" -o "$raw")
    tr -d '\r' < "$raw" > "$out"
}

case "${1-}" in
    "") ;;
    --update)
        need about " --features cli"
        generate "$SECTION"
        echo "licences: regenerated $(basename "$SECTION") ($(grep -c '^- `' "$SECTION") crate attributions); commit it"
        exit 0
        ;;
    *) usage ;;
esac

need deny
need about " --features cli"

# All three checks run before the verdict, so one red does not hide another: a reader
# fixing an advisory should learn in the same run that the notices drifted too.
status=0
(cd "$REPO_DIR" && cargo deny check licenses advisories) || status=1

fresh="$(mktemp)"
trap 'rm -f "$fresh"' EXIT
generate "$fresh"
# Compare the committed copy with its own CRLF stripped too: a Windows checkout under
# text=auto may hold CRLF in the working copy while the repository holds LF.
if diff -u <(tr -d '\r' < "$SECTION") "$fresh" > /dev/null; then
    echo "licences: notices/50-crates.md matches Cargo.lock"
else
    echo "licences: notices/50-crates.md is stale against Cargo.lock" >&2
    diff -u <(tr -d '\r' < "$SECTION") "$fresh" | head -40 >&2
    echo "licences: fix: scripts/licences.sh --update, then commit the result" >&2
    status=1
fi
exit $status
