# Release Procedure

How a release of Scribobulate is cut. This is a procedure, read top to bottom on the day,
and every step is a gate with named proof — a release is a claim about one commit on
three platforms, and each gate exists because a green run somewhere else does not carry
that claim. The rules it rests on live in [POLICY.md](POLICY.md); this document adds no
rule, it orders the ones that bear on publishing.

**Status: under its first dry run (1.0.0).** The gates and the decisions they rest on
are settled; the dry run is what hardens them, and its findings are folded in as they
surface.

## Shape of a release

- A release is one commit — the **target commit**, the head of the release branch
  (next section) — that has already passed the GitHub pipeline on all three platforms
  *as that commit*, not as an ancestor of a branch that later passed. It becomes an
  ancestor of `master` when the branch is merged back after publishing.
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

## The release branch

A release is prepared on a branch, `release/<version>`, cut from the `master` commit
the release is meant to be, and the tag is placed on that branch's head. The branch is
what lets a release be prepared while work on `master` goes on, and what lets a fix the
release needs be taken without taking everything `master` has gained since the cut.

- **Cut** the branch from `master` before working gate 1, and name it in the release
  record's header. Everything readiness produces — the version bump if gate 1 needs one,
  a register entry filed from a finding, a correction to this procedure, the change log
  of gate 11 — is committed on it, by POLICY's branch rules (work on a `bug/` or
  `feature/` branch cut *from the release branch*, pipeline green, merge in). Nothing
  readiness produces goes to `master` directly.
- **A fix the release needs** lands on the release branch *and* on `master`, as soon as
  it is green: merge its branch into `release/<version>`, then merge it into `master` by
  the usual rule. `master` must not wait for the release to learn of a defect its own
  head carries, and a fix that reached only the release branch is lost to the next
  release the moment the branch is deleted. Whether a defect found during readiness is a
  fix the release needs or a deferral is gate 3's decision, recorded there.
- **A fix that touches `tests/MANUAL-TEST.md` or `sdd/`** passes `cargo xtask
  lint-references` before it is handed back, beside fmt, clippy and its tests: a manual
  item added under a number already taken, or a register citation left dangling, fails
  the pipeline's references step on all three platforms and costs a whole run.
- **Fixes are batched.** Findings accumulate until the pass that found them ends, or
  until a checkpoint declared in the record beforehand; then the fixes the release needs
  land as one batch, and the target moves once for all of them. The exceptions are fixed
  at once, each moving the target on its own: data loss, a blocker that stops the passes
  from continuing, and a security defect.
- **Every commit on the branch moves the target.** The head is the target, so a fix —
  or a register entry — re-stamps the record's header and unticks gate 4 onward (the
  checklist's rule). A commit that touches no build input (only `sdd/`, `docs/`,
  `CHANGELOG.md`) still needs gate 4's run on the new head, which the push gives for
  free; whether gates 5–9 are re-done for it is the question below.
- **The branch lives until the release is published.** After gate 12 it is merged back
  into `master` (gate 13, before the version bump, so the tagged commit is an ancestor
  of `master` and the bump does not conflict with it) and then deleted, local and
  remote. A release branch left behind after its release is a stale branch like any
  other.
- **Nothing on `master` is cherry-picked onto the branch.** A change `master` gained
  after the cut that the release turns out to need is a fix the release needs — it goes
  through the bullet above from its own branch, not by picking a commit out of
  `master`'s history (POLICY § Version control: never cherry-pick part of a batch).

- **A build-neutral commit does not re-run the artefact gates.** A commit on the
  release branch that touches no build input — a register entry, a correction to this
  file, the change log — moves the target hash but not the artefacts. Gate 4 re-runs
  by itself on the push, and with it gates 2 and 6's automated halves (step 8b runs on
  every push, and that is wanted: `Cargo.lock` pins what a dependency *is*, but the
  advisory database and the licence detection can change under an unchanged lock).
  Gates 5, 7, 8 and 9 stand on the evidence already recorded when
  `git diff --stat <old> <new> -- . ':!sdd' ':!docs' ':!CHANGELOG.md'` is empty, and
  the record says so against the moved target. (Decided 2026-11-12.)

## Release kinds

POLICY § Version numbers makes the version say what kind of release this is, so the
kind is read, not decided: compare `[package] version` on the release branch with the
last tag. **PATCH only** (`v1.0.0` → `1.0.1`) is a patch release — bug fixes, no
rubric added or rewritten. **MINOR or MAJOR** moved — a feature release. The record's
header names the kind, and gate 1 checks the reading against the change list (a rubric
added since the tag under a patch number is a version bump owed, not a patch release).

A **feature release** runs every gate below in full, gate 8 at the tier gate 8 names.

A **patch release** runs every gate, but four of them smaller, because what a patch
can change is bounded by what its fixes touched — and each gate says what "touched"
means so the reduction is read off the diff, not argued:

- **Gate 7 (footprint)**: skipped unless the diff since the last tag touches a
  rendering dependency, a rendering path, the process model or how the renderer is
  selected (POLICY § Footprint verification's own trigger list); the record cites the
  diff and the tag's readings stand.
- **Gate 8 (manual testing)**: the smoke tier (gate 8), plus every item of every
  section whose rubric a fix cites (each fix names the rubric it makes hold — that is
  what makes it a patch). The report lists the items run and why; the operator's 👍 is
  still the proof.
- **Gate 9 (install, launch, uninstall)**: by gate 9's own rule — full when the diff
  touches what that rule lists, lite otherwise.
- **Gate 10 (documentation)**: `cargo xtask lint-references` and the README's install
  examples carry the new version; the status banner and AI-DILIGENCE figures stand.


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

- A header naming the version, the release kind (patch or feature — Release kinds,
  above), the release branch, the target commit hash once chosen (blank until gate 4,
  re-stamped whenever the target moves), and who holds the release.
- One section per gate, in the order below, each a Markdown task list (`- [ ]`) with
  one item per bullet of that gate, and one item for the gate's **Proof**. A gate's
  section holds its proof lines and its decisions, nothing else.
- **One findings table** for the whole release — a row per finding: its id, the
  platforms it was seen on, its class (fix, defer, accept, reword) and its disposition
  (the fix's branch, the register entry, the rubric changed). A finding seen on two
  platforms is one row, not two.
- The passes' chunk-by-chunk narratives stay in each seat's `tests/reports/`; the record
  cites a report, it does not carry one.
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
  👨‍✈️ items are gate 2's waivers, the findings gate 3 reserves for the operator,
  gate 6's licence election, gate 8's 👍, gate 11's review of the release body (which
  is where the operator sees every deferral) and gate 12's publish.
- A target that moves (every commit on the release branch moves it) is recorded as
  an event — old hash, new hash, why — and every item from gate 4 on is unticked.

Whoever picks the release up — a later session, another seat, another person —
reads this file first and resumes at the first unticked item; nothing before it is
re-done unless its evidence fails to hold.

## Gates

Work the gates in order. A gate's proof is what you cite in the release record;
"I ran it" is not proof. A new commit on `master` after gate 4 restarts from gate 4
with the new target — the gates are about *that commit*, and an exception is a
decision the operator records, not one an agent makes.

### 1. Version

- `Cargo.toml` already carries the version being released: the next patch number from
  the previous release's gate 13, or the MINOR/MAJOR step the feature work bumped it to
  when it landed (POLICY § Version numbers; the first release: `1.0.0`). Confirm it
  against the change list since the last tag — a rubric added or rewritten under a
  patch number means a bump is owed: make it on a branch cut from the release branch,
  commit, let the pipeline pass, merge it into the release branch and into `master`
  (The release branch, above). `Cargo.lock` carries the bump too. Record the release
  kind (Release kinds, above) in the record's header.
- **Proof:** the target commit's `Cargo.toml` shows the release version; nothing else in
  the tree spells that version out (`grep` the old version across the repository; a hit
  outside `Cargo.lock` and `CHANGELOG.md` is a second owner to remove).

### 2. Security: every reported vulnerability satisfied or waived

- Pull the GitHub reports:
  `gh api repos/MadMartian/Scribobulate/{code-scanning,dependabot}/alerts?state=open`
  (secret scanning is off by decision: the application holds no credentials and
  connects to nothing that needs one).
  Every open alert ends in one of two states before the gate passes:
  - **satisfied** — by one of three routes: **upgrade** the dependency so the target
    commit carries the fix; **narrow the dependency's shape** — a feature set dropped so
    the vulnerable crate leaves the graph, often the cheapest route (the 1.0.0 dry run
    cleared three advisories at once by dropping `syntect` loaders the app never used);
    or **dismiss** the alert on GitHub with a reason that is true (false positive, not
    reachable, not shipped);
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
- **Decided by default.** The release seat classifies every finding and entry as it
  arrives. A cosmetic one, a Low one, and a rubric that drifted from the behaviour are
  filed in ISSUES.md or reworded in TDD.md and MANUAL-TEST.md with no decision asked
  for. The operator decides the rest, one 👨‍✈️ item each: a High entry, anything that
  loses data, a security defect, and a reader-facing defect seen on more than one
  platform. Severity is where the classification starts, not where it ends — a Low
  entry in a headline feature goes to the operator, and the seat says why.
- The operator sees the default deferrals once, as the release body's Known issues at
  gate 11, and can pull any of them back into a fix there.
- Closed (`CLSD-`) entries are limitations, already settled; they appear in Known issues
  when a user can meet them, and need no new decision. A `Test`-scope entry (the
  register's own scope column) needs no user-facing decision and no Known-issues line:
  no user meets it.
- An entry filed from a CI flake records the runner's toolchain (the macOS runner's
  Homebrew GTK is not pinned); a local 0-of-N measured on a different version is weaker
  evidence than it reads, and the same version makes it like-for-like.
- **Proof:** the list of deferred letters with their justifications, in the release
  record.

### 4. The pipeline on the target commit

- The target is the release branch's head. The GitHub pipeline is green on that commit
  itself, every platform, no step
  skipped or declared non-applicable that is not declared so permanently in
  `scripts/pipeline.steps`.
- Read a red job's check-run annotations before its log. A hosted macOS job that ends
  cancelled with **zero steps** ("not acquired by Runner … capacity constraints") is
  runner capacity, not a crash and not the code: re-run the failed job
  (`gh run rerun <id> --failed`). A job that ran steps and died is a different shape,
  read from its log.
- **Proof:** the run's URL, and the commit hash it reports is the target.

### 5. Packaging dry run

- Dispatch the workflow on the target commit with `package: true` — **after** gate 4's
  run on that ref has finished, never beside it: the workflow's concurrency group is per
  ref with cancel-in-progress, so a dispatch while a push run (or a re-run of its failed
  job) is in flight cancels one of the two. Download the three artefacts: Linux `.deb`
  and `.rpm`, macOS `.dmg`, Windows `-setup.exe`.
- A red dry run on a tree the push pipeline already passed is usually a flake: re-run
  the failed job, and file the flake in ISSUES.md before moving on — gate 3 then has a
  new letter to decide. A red that reproduces is a fix the release needs.
- Stage the downloaded artefacts, with a `SHA256SUMS.txt` beside them, where every seat
  can reach them without GitHub credentials (`../Scribobulate-artefacts/<version>/`
  beside the tree, which the Windows VM sees through its shared folder, and the
  operator's `/srv/pub/…` mirror of it) — the sums file lists bare filenames, not the
  download tree's subpaths, since the staging directory is flat and `shasum -c` fails
  otherwise — and have each seat verify the sum before using one.
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
  are configured (`deny.toml`, `about.toml`, `about.hbs`) and pipeline step 8b
  (`scripts/licences.sh`) runs them on every push, so gate 4's green run on the target
  is this layer's proof; run the script by hand only to read its output.
- **Bundled runtimes.** macOS and Windows redistribute GTK and its dependencies.
  `packaging/macos/verify-licenses.sh` and `packaging/windows/verify-licenses.ps1` prove
  the notices are *staged*; they do not and cannot prove *which licence covers which
  binary* — the manifests mark every row NOT-GATE-ENFORCED for that reason. At release a
  person reads each manifest row (`packaging/macos/license-anchors.txt`,
  `packaging/windows/licenses.psd1`) and confirms the election is still right for the
  binary actually shipped (the worked example is gettext: LGPL for the shipped libintl,
  not GPL for the tools). An election the bundle must make — `lzo`, plain GPL with
  no alternative, taken under GPL-3.0 as the version coherent with Apache-2.0 — is
  recorded by `stage-licenses.sh` as `ELECTION.txt` beside the formula's texts, with the
  Corresponding Source offer. Linux bundles nothing; its `.deb` copyright stanza and
  `.rpm` spec licence tag name every licence family the binary carries (the project's,
  the grammars' and the crates'), and `scripts/licences.sh` fails when the crate notices
  gain a family those lines do not name.
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

- A `tests/MANUAL-TEST.md` pass on each of Linux, macOS and Windows, against the
  installed dry-run artefact of the target commit, at one of two tiers:
  - **the full plan** — the baseline, run for a MAJOR release or on the operator's word;
  - **the smoke tier** otherwise — the items MANUAL-TEST's header marks `[smoke]`, on
    every platform, plus the items the diff touches: for each change, the rows of the
    [CAM.md](CAM.md) matrices it falls under name the surfaces it reaches, and every
    item testing one of those surfaces runs.

  Each pass records every item's verdict in its run-sheet
  (`tests/reports/MANUAL-TEST-REPORT.md`, per its §7); a pass with an un-verdicted item
  of its tier is not complete.
- **A re-drive after a fix round** runs only the items that failed and the legs never
  run. Nothing that passed is repeated: a fix is answerable for the items it names, and
  its own tests cover the mechanism it changed.
- **NOT-RUN with a reason** is a legitimate verdict (MANUAL-TEST § header) and is
  listed, not hidden. A **FAIL** is either fixed — in the batch the release branch's
  rule sets, which moves the target commit and restarts at gate 4 — or becomes an
  ISSUES.md entry or a reworded rubric, classified as gate 3 says.
- Only `windows` and `mac` can run their own platform's pass; `linux` dictates the
  target commit and collects the three reports for the operator.
- A brief to a seat is sent at the maximum message TTL with an acknowledgement asked
  for: a seat mid-chunk reads its inbox late, and a brief that expires unread is
  indistinguishable, from the sender's side, from one that was read. The release seat
  keeps one consolidated current brief per seat — target, artefact and sum, what to run,
  what to skip — and re-sends that, not a chain of amendments.
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
- **Full once per campaign, lite after that.** The steps above run in full once per
  release campaign, and again only when the diff since the last full run touches
  `packaging/`, `.github/workflows/`, `scripts/pipeline.steps`, `Cargo.toml` or
  `Cargo.lock`, `build.rs`, the resources (desktop file, icons, MIME, `Info.plist`,
  entitlements, the licence bundle) or startup and lifecycle code (`src/main.rs`,
  `src/app/`, `src/logging.rs`, `src/forensics/`, settings and session migration). Any
  other change cannot alter whether the package installs or the binary launches, and
  takes **gate 9 lite**: verify the artefact's sum, install it over the previous build,
  prove the version (`--version`) and the commit, launch once, quit once. The commit
  proof is the log's identity line on Windows and macOS; on Linux, whose log carries
  none, it is the commit found in the installed binary (`strings`) and the previous
  one absent. On macOS the lite run strips quarantine from its scratch copy
  (`xattr -d com.apple.quarantine`), so it costs the operator no Gatekeeper click —
  the full run already proved what Gatekeeper shows.
- **Proof:** one line per platform in the release record, saying full or lite.

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
- Merge `release/<version>` into `master` — a merge commit, so the tagged head stays
  reachable as itself — and delete the branch, local and remote. Every fix it carries is
  already on `master` by the release-branch rule; this merge brings the rest (the
  change log, register entries, procedure corrections).
- Then bump `[package] version` in `Cargo.toml` to the next patch version (`1.0.0` →
  `1.0.1`) on a branch and merge it by the usual rule, so no commit after the tag ever
  reports the released version. A later minor or major step overrides it at the next
  gate 1.
- Open `ISSUES.md` and remove nothing: a release does not close an issue, a fix does.

## Rig notes

What the 1.0.0 dry run's rigs got wrong, each once and at a cost:

- **pdftotext builds differ in where the page-break form feed lands** — one appends it
  to the page's last text line, another puts it on a line of its own. Strip `\f` before
  comparing extracted text.
- **Never pipe a test or app run through `tail`** (or anything that holds the pipe
  open). A D-Bus-activated portal started by the run inherits the pipe, and the command
  never returns.
- **A strong reference that exists only under `#[cfg(test)]` hides release-only
  lifetimes.** A test-only clone that keeps an object alive makes every test pass while
  the release build frees it; a release-only defect is proved on a release binary.
- **gtk-rs checks interior NULs only in debug builds** (`debug_assertions`). A debug test
  of text containing a NUL panics in the binding before the code under test runs, and
  passes for the wrong reason; the release build hands the text to GTK.
- **A `dbus-run-session` rig must not take over the user's at-spi socket.** The rig's
  private bus, and whatever it starts, stays the rig's; the operator's desktop keeps its
  own accessibility bus.
- **Inno Setup's `unins000.exe` returns before it has finished.** `Start-Process … -Wait`
  returns while the uninstaller's `%TEMP%` copy is still deleting; a scripted
  uninstall-then-reinstall waits for that copy to exit first, or the reinstall loses its
  own uninstaller.
- **macOS Open Anyway expires.** The System Settings button lapses after some hours; a
  relaunch regenerates it.

## What is deliberately not a gate

- **Code review.** It happened per change under POLICY; a release-time re-review of the
  whole tree is a different activity (the audit trails under `docs/`), not a release
  gate.
- **Coverage.** The ratchet is enforced per push (POLICY § Build pipeline); the release
  inherits the floor it stands on.
- **Performance beyond footprint.** PLAN.profiling.md owns whether a performance gate
  exists; until it says so, none is implied here.
