# Release Procedure

How a release of Scribobulate is cut. This is a procedure, read top to bottom on the day,
and every step is a gate with named proof — a release is a claim about one commit on
three platforms, and each gate exists because a green run somewhere else does not carry
that claim. The rules it rests on live in [POLICY.md](POLICY.md); this document adds no
rule, it orders the ones that bear on publishing.

**Status: draft, awaiting its first dry run.** The gates and the decisions they rest on
are settled; the dry run is what hardens them. The licence and advisory tooling of
gates 2 and 6 is configured; its pipeline step is not yet delivered.

## Shape of a release

- A release is one commit on `master` — the **target commit** — that has already passed
  the GitHub pipeline on all three platforms *as that commit*, not as an ancestor of a
  branch that later passed.
- The version has one owner, `[package] version` in `Cargo.toml`; every installer, the
  macOS `Info.plist`, the man pages and the Windows setup read it from there (POLICY
  § Build pipeline, "A number has one owner"). The tag and the release title repeat it;
  they never define it.
- Publishing the GitHub release is what builds the artefacts: the workflow's
  `release: published` trigger packages the target commit and attaches the installers,
  so the artefact set is the release's contents by construction
  (`.github/workflows/pipeline.yml`). No installer built on a developer machine is ever
  uploaded.
- The same packaging path runs without a release through `workflow_dispatch` with
  `package: true`. That is the dry run, and it is how the artefacts under test in the
  gates below are obtained.
- Artefacts are unsigned (POLICY § Artefact signing). Every release says so in its notes,
  with what the platform will show the user (Gatekeeper on macOS, SmartScreen on
  Windows), until the change that introduces signing removes the sentence.

## Decisions

- The tag is `v<version>` (`v1.0.0`).
- The change log is a committed `CHANGELOG.md`; the release body is copied from it.
- `Cargo.toml` is bumped to the next patch version in the release's wake (gate 13).
- Manual-test evidence is never published. `tests/reports/` is git-ignored by design
  and stays so; each platform's run-sheet is kept there, on the seat that ran it, and
  its purpose is the operator's review — the operator reads the three reports and
  gives the release a 👍 or 👎 on them (gate 8). `CHANGELOG.md` says only that the
  release was tested on all three platforms against its installers.
- No checksum file is published; GitHub's per-asset digest is the record of what was
  uploaded. A `SHA256SUMS` emitted by the packaging run is a need of a channel beyond
  GitHub, and lives with that question in
  [PLAN.distribution.md](PLAN.distribution.md).
- Distribution channels beyond GitHub are that plan's question, not this document's.
- `cargo about`'s output is committed as `notices/50-crates.md`, one more notice section
  `build.rs` concatenates, because the build cannot run the generator itself (not a
  build tool; needs registry metadata). A Linux-only pipeline step regenerates it and
  fails on drift, so the committed copy cannot go stale unnoticed.
- The seven `cargo deny` advisories found while setting the tool up (five clearable by
  `cargo update`, two unmaintained-crate notices via `syntect` needing waivers) are
  left for the dry run's gate 2, so the procedure is exercised on real findings.


## The readiness checklist

Release readiness spans sessions and may span people, so its state is never held in
an agent's context or a chat transcript. **Before working gate 1, create
`docs/RELEASE.v<version>.md`** — the **release record** every gate below
cites — and keep it current as the only account of where the release stands.
`docs/` is git-ignored, so the file is never versioned; it is handed to whoever
picks the release up by copying it.

Its shape:

- A header naming the version, the target commit hash once chosen (blank until gate
  4, re-stamped whenever the target moves), and who holds the release.
- One section per gate, in the order below, each a Markdown task list (`- [ ]`) with
  one item per bullet of that gate, and one item for the gate's **Proof**.
- A ticked item carries its evidence inline — the run URL, the command output's
  decisive line, the file path, the date — never only the tick. An item with a tick
  and no evidence is unticked by whoever finds it.
- **An item the operator must ratify is marked 👨‍✈️ and is ticked by no one
  else.** The operator's own words are recorded against it (the decision, the
  justification, the 👍 or 👎), with the date. An agent writes the item and the
  material the operator decides on; it never writes the operator's verdict for them,
  and "the operator agreed" in a chat is not a verdict until it is in this file. The
  operator's natural way to write it is a CriticMarkup annotation on the item; the
  agent then resolves the annotation into a plain quotation on the item with the
  date, so the record reads clean once the file is handed on. The
  👨‍✈️ items are gate 2's waivers, gate 3's deferrals, gate 6's licence
  election, gate 8's 👍, gate 11's review of the release body and gate 12's publish.
- A target that moves (gate 4's rule) is recorded as an event — old hash, new hash,
  why — and every item from gate 4 on is unticked.

Whoever picks the release up — a later session, another seat, another person —
reads this file first and resumes at the first unticked item; nothing before it is
re-done unless its evidence fails to hold.

## Gates

Work the gates in order. A gate's proof is what you cite in the release record;
"I ran it" is not proof. A new commit on `master` after gate 4 restarts from gate 4
with the new target — the gates are about *that commit*, and an exception is a
decision the operator records, not one an agent makes.

### 1. Version

- `Cargo.toml` already carries the next patch version from the previous release's gate
  13 (the first release: `1.0.0`). Confirm it is the version being released; if this
  release is a minor or major step instead, bump it on a branch, commit, let the
  pipeline pass, merge to `master` by the usual rule (POLICY § Version control).
  `Cargo.lock` carries the bump too.
- **Proof:** the target commit's `Cargo.toml` shows the release version; nothing else in
  the tree spells that version out (`grep` the old version across the repository; a hit
  outside `Cargo.lock` and `CHANGELOG.md` is a second owner to remove).

### 2. Security: every reported vulnerability satisfied or waived

- Pull the GitHub reports:
  `gh api repos/MadMartian/Scribobulate/{code-scanning,dependabot}/alerts?state=open`
  (secret scanning is off by decision: the application holds no credentials and
  connects to nothing that needs one).
  Every open alert ends in one of two states before the gate passes:
  - **satisfied** — fixed on the target commit, or the alert dismissed on GitHub with a
    reason that is true (false positive, not reachable, not shipped);
  - **waived** — carried into the release with a written justification: why it does
    not affect a user of this release, and what retires it. A waiver is an ISSUES.md
    entry (so it is tracked) and a line in the release notes' Known issues (so the user
    is told). An alert with neither is a failed gate; it is never carried silently.
- Re-read every [CVE.md](CVE.md) entry against what the artefacts will bundle: the
  librsvg floor in `packaging/librsvg-floor` against the Homebrew librsvg the macOS
  bundle stages and the gvsbuild librsvg Windows stages (the gvsbuild version and digest
  are pinned in the workflow; re-derive the digest from the gvsbuild release, never
  from the downloaded file). A workaround that a newer runtime makes unnecessary is
  retired per CVE.md's own rule, not here.
- `cargo deny check advisories` against `Cargo.lock` on the target commit, the same
  satisfied-or-waived rule per RustSec advisory. **Decided:** it becomes a pipeline step
  (`scripts/pipeline.steps`) with the licence check of gate 6; until that lands it is
  run by hand at this gate.
- **Proof:** the two `gh api` outputs and the advisory check's output, and for each
  waiver its ISSUES.md letter and justification.

### 3. Open issues: every entry resolved or explicitly deferred

- Read [ISSUES.md](ISSUES.md)'s table of contents on the target commit. Every open
  entry ends in one of two states:
  - **resolved** — fixed on the target commit and removed from the register (its removal
    is part of the fix, per the register's lifecycle);
  - **deferred** — carried into the release with a justification recorded against the
    entry: why it is acceptable to a user of this release (severity, how rarely it is
    hit, the workaround). A deferred entry that a user can hit is a line in Known
    issues, in the user's terms.
- The decision is the operator's per entry, not a severity threshold applied
  mechanically — a low-severity issue in a headline feature may block, a high one in a
  path no user reaches may not. An agent proposes the split; the operator ratifies it.
- Closed (`CLSD-`) entries are limitations, already settled; they appear in Known issues
  when a user can meet them, and need no new decision.
- **Proof:** the list of deferred letters with their justifications, in the release
  record.

### 4. The pipeline on the target commit

- The GitHub pipeline is green on the target commit itself, every platform, no step
  skipped or declared non-applicable that is not declared so permanently in
  `scripts/pipeline.steps`.
- **Proof:** the run's URL, and the commit hash it reports is the target.

### 5. Packaging dry run

- Dispatch the workflow on the target commit with `package: true`. Download the three
  artefacts: Linux `.deb` and `.rpm`, macOS `.dmg`, Windows `-setup.exe`.
- These artefacts — not a `cargo build` on a developer machine — are what gates 6–9 are
  run against. A dev build proves the code; the release proves the installer.
- **Proof:** the run's URL; each artefact's name carries the release version and its
  size is plausible (the workflow checks both; read the check's output, not the job's
  colour).

### 6. Licences and attribution

Every published artefact carries the notices its dependencies require (POLICY
§ Third-party attribution). Three layers, each with its own check:

- **Statically linked Rust crates.** `THIRD-PARTY-LICENSES.md` is generated from
  `notices/`; confirm it is current against `Cargo.lock` on the target commit — every
  crate that carries an attribution requirement (MIT, BSD, Apache-2.0 with a `NOTICE`
  file, the `two-face` grammars) is represented, and no dependency added since the last
  release carries a licence the project cannot satisfy (copyleft, source-available). A
  dependency review happens per change (POLICY § Dependencies); this is the release-time
  sweep that catches one that slipped. **Decided:** this sweep is tooled, not done by
  hand — `cargo deny check licenses` holds the allowlist and `cargo about` generates the
  crate notices into `notices/50-crates.md`, which `build.rs` already consumes. Both
  are configured (`deny.toml`, `about.toml`, `about.hbs`) and run by hand at this gate
  until the pipeline step that runs them lands.
- **Bundled runtimes.** macOS and Windows redistribute GTK and its dependencies.
  `packaging/macos/verify-licenses.sh` and `packaging/windows/verify-licenses.ps1` prove
  the notices are *staged*; they do not and cannot prove *which licence covers which
  binary* — the manifests mark every row NOT-GATE-ENFORCED for that reason. At release a
  person reads each manifest row (`packaging/macos/license-anchors.txt`,
  `packaging/windows/licenses.psd1`) and confirms the election is still right for the
  binary actually shipped (the worked example is gettext: LGPL for the shipped libintl,
  not GPL for the tools). Linux bundles nothing; its `.deb` copyright file and `.rpm`
  spec licence tag name the project's own licence only.
- **The About dialog's claim.** It says the notices are in the distribution. Install each
  artefact (gate 9) and find them where it says they are.
- **Proof:** the two verify scripts' output from the dry-run artefacts, and a line in the
  release record that the manifest election was reviewed by hand, by whom.

### 7. Footprint

- VRAM ≤ 50 MiB on every platform, measured on the installed artefact from gate 5, by
  the procedure in TDD §6 (POLICY § Footprint verification). RAM at the README's
  advertised order of magnitude, or the README's number changes in the same release.
- **Proof:** the three readings, with the document and window size used.

### 8. Manual testing on all three platforms

- A **full** `tests/MANUAL-TEST.md` pass on each of Linux, macOS and Windows, against
  the installed dry-run artefact of the target commit. Each pass records every item's
  verdict in its run-sheet (`tests/reports/MANUAL-TEST-REPORT.md`, per its §7); a pass
  with an un-verdicted item is not complete.
- **NOT-RUN with a reason** is a legitimate verdict (MANUAL-TEST § header) and is
  listed, not hidden. A **FAIL** is either fixed — which moves the target commit and
  restarts at gate 4 — or becomes an ISSUES.md entry that then goes through gate 3's
  deferral, by the operator's decision.
- Only `windows` and `mac` can run their own platform's pass; `linux` dictates the
  target commit and collects the three reports for the operator.
- **Proof:** the operator's 👍 on the three reports. Each report carries the commit
  hash tested, the artefact's name, the date, the counts (pass / fail / not-run) and
  the NOT-RUN and FAIL item numbers with reasons; a 👎 names what sends the release
  back and to which gate.

### 9. Install, launch, uninstall from the artefact

On each platform, from the gate-5 artefact, by the platform procedure in MANUAL-TEST §A:

- Install on a machine (or account) with no Rust toolchain and no GTK development
  packages — the artefact's whole purpose (`scripts/pipeline.steps` step 9's intent).
- Launch; open a document; About shows the release version and the licence notices
  resolve.
- The file association opens a `.md` from the file manager.
- Uninstall leaves nothing behind that the installer put there.
- **Proof:** one line per platform in the release record.

### 10. Documentation

- `README.md`'s status banner ("pre-release") is rewritten for a released product, and
  its install section points at the release assets rather than at building from source
  first.
- `AI-DILIGENCE.md`'s figures (commit counts, trailer counts) are recomputed; they are
  stated as facts and go stale.
- `cargo xtask lint-references` is clean on the target commit.
- No TDD rubric or MANUAL-TEST item describes behaviour the release does not have — the
  per-change rule (POLICY § Build pipeline) is supposed to guarantee this; the manual
  pass in gate 8 is where it is caught if it did not.
- **Proof:** these land *before* the target commit is chosen; a documentation fix after
  gate 4 moves the target.

### 11. Change log and release notes

- Derive the change log from `git log <previous-tag>..<target>`. The first release has
  no previous tag and no diff to describe: its entry is the product as `README.md`
  presents it, in the same groups. Commits are the operator's conglomerated kind
  (POLICY § Version control), so one commit is usually one line; group by what a user
  sees — new, changed, fixed, platform-specific — and omit what a user cannot see
  (refactors, gate work, register writes).
- Write it as a new top section of `CHANGELOG.md` — `## v<version> — <date>`, newest
  first, headed by the groups above — in the same commit as the version work of gate 1
  or on its own, either way *before* the target commit is chosen, since the change
  log is part of what is released. The release body is copied from that section and
  composed of:
  1. the change log;
  2. **Known issues** — the waivers from gate 2 and the deferrals from gate 3 that a
     user can hit, in the user's terms;
  3. **Requirements** — GTK ≥ 4.6 on Linux (the floor in `Cargo.toml`), macOS ≥ 11.0
     (`Info.plist.in`), Windows x64; the Ubuntu release the Linux packages were built on
     (named in the workflow, which is the owner);
  4. the signing disclosure (above).
- **Proof:** the release body is reviewed by the operator before publishing — notes are
  published under the operator's name, so the operator signs them.

### 12. Publish

- Tag the target commit `v<version>` and create the GitHub release from that tag with
  the body from gate 11. Create it as a **draft** first if the notes need a second look;
  the workflow runs only on `published`.
- Publishing triggers the packaging run. Wait for it; then verify the attached assets by
  hand — the right count, the right names, the right version in each name, sizes
  matching the gate-5 dry run — and that nothing was attached from any other run.
- **Proof:** the release URL and the packaging run's URL.

### 13. After publishing

- Download one asset per platform *from the release page* and confirm its SHA-256
  against the digest GitHub records for that asset (`gh release view --json assets`),
  so the download path is proven to deliver what the packaging run uploaded.
- Re-open `README.md`'s download links in a browser; they resolve to the release.
- Any channel PLAN.distribution.md has delivered is updated from the published assets —
  never from local builds.
- Bump `[package] version` in `Cargo.toml` to the next patch version (`1.0.0` →
  `1.0.1`) and merge it by the usual rule, so no commit after the tag ever reports the
  released version. A later minor or major step overrides it at the next gate 1.
- Open `ISSUES.md` and remove nothing: a release does not close an issue, a fix does.

## What is deliberately not a gate

- **Code review.** It happened per change under POLICY; a release-time re-review of the
  whole tree is a different activity (the audit trails under `docs/`), not a release
  gate.
- **Coverage.** The ratchet is enforced per push (POLICY § Build pipeline); the release
  inherits the floor it stands on.
- **Performance beyond footprint.** PLAN.profiling.md owns whether a performance gate
  exists; until it says so, none is implied here.
