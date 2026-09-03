# Development Policy

## Build

- Requires a Rust 2021 toolchain and the GTK4 + GtkSourceView 5 development
  libraries (`libgtk-4-dev`, `libgtksourceview-5-dev`). GTK 4.6 works; a
  defensive XCompose-size workaround (`workaround_gtk46_compose_crash()`) is
  included and is a no-op when no large `~/.XCompose` is present (see
  ScrAP-3).
- **macOS builds** take GTK4/GtkSourceView 5 from Homebrew and run on the Quartz
  backend; `packaging/macos/bundle.sh` produces the `.app`, which is required
  before the app has any Dock or Cmd-Tab identity. Homebrew's `gtk4` does **not**
  pull in an icon theme — `brew install adwaita-icon-theme` explicitly, or roughly
  half the toolbar renders broken-image placeholders. Step 5's literal command
  does not apply here — there is no Xvfb, and `--lib`'s dual-harness bodies abort
  the process off the main thread (ScrAP-171; measured 2026-07-30, GTK 4.22.4/Quartz:
  `cargo test --features gtk-integration-tests --lib` SIGABRTs after ~95 non-GTK
  tests). Run `cargo test --features gtk-integration-tests --test gtk_suite` plus
  the three standalone targets instead — see "Verifying a change on macOS" below,
  now measured rather than only designed: the suite passes clean on that platform,
  as do all three standalone targets. **Deliberately not a case count.** The number
  written here (147, when this was first measured) is the one thing about a suite
  guaranteed to be wrong by the next commit, and a stale one reads as a *deficit*
  to the next reader — the seat that measured 259 passing had to establish that the
  figure it was being compared against was merely old. The verdict is the claim;
  the count belongs to the run that produced it.
  **Build it ON a Mac — cross-compiling from Linux is a closed dead end.** The native
  build is routine and produces the `.app` and `.dmg`. The *cross* build is not:
  installing the `x86_64-apple-darwin` target and running `cargo check --target` fails
  inside `gtk4-sys` on pkg-config cross-compilation, and the target was removed again
  (verified in QA round 1). These two are easy to conflate and the distinction is the
  whole point — a successful native build says nothing about the cross one, so a report
  that "it builds fine on the Mac" does not reopen this. The consequence is a standing
  constraint on how the project is verified, not a preference: **no seat but the macOS
  one can produce or check a macOS artefact**, which is why the manual suite carries
  platform-specific items instead of assuming parity.
- **Windows builds** use MSVC plus a gvsbuild-produced GTK4/GtkSourceView 5
  runtime; the full pipeline and its pitfalls are in
  `packaging/windows/README.md`. `src/workaround.rs` is `#[cfg(unix)]`-gated
  there, so the XCompose workaround above is Linux-only by construction.
  **Linux remains the canonical platform for the gates below** — in particular
  the coverage ratchet (step 6), whose scoped percentage legitimately differs on
  Windows because `atomic_io.rs`'s unix-only code and tests are not compiled.
  Never lower the Linux floor to make a Windows run pass. The same principle
  applies on macOS for the opposite reason: `src/platform/mac/*.rs` (the
  appearance and single-instance seams) compiles only there, is mostly
  GTK/CoreFoundation-wired glue with no headless test, and measurably drags the
  scoped total below the Linux-calibrated floor (measured 2026-07-30: 72.25%, against
  the floor `scripts/coverage.sh` held that day). The floor is a ratchet and moves; it
  is deliberately NOT restated here, because the number written at this spot had
  already drifted from the script's once — which is the failure step 6 below warns
  about, committed by this very step. Read `FLOOR` from
  `scripts/coverage.sh`. Do not chase that gap by lowering the floor or by excluding the
  seam files from `IGNORE` — the floor's job is to gate Linux, where these files
  do not exist at all.
- Release builds are the reference for both behavior and footprint — the VRAM
  gate (TDD §6) is defined against release binaries.

## Build pipeline

**The pipeline is executable, and `scripts/pipeline.steps` is the contract.** Run
`scripts/pipeline.sh` (Windows: `packaging/windows/pipeline.ps1`) rather than working
through the list below by hand. Each runner *derives* its step list from that contract
instead of restating it, and `--list-steps` prints the derived list so the ports can be
diffed against each other — comparison proves the ports agree, derivation proves they
conform, and only the second is worth having (ScrAP-207).

The steps below remain the authority on *why* each gate exists and what it is defending;
the contract owns the ordered list, each step's verdict rule, and the per-platform
command. **Do not restate a command or a step's applicability here** — same rule, and same
reason, as the coverage floor and the input limits: a second copy is how the first one
silently stops matching. A step that does not apply to a platform is declared in that
platform's *run output* with its reason, never omitted and never left in a source comment
where the pipeline's user cannot see it.

Before any change is considered valid, run these steps in order:

1. `cargo fmt --check` — formatting must be clean
2. `cargo clippy --all-targets --features gtk-integration-tests -- -D warnings` —
   zero warnings permitted. **The feature flag is not optional here.** Without it
   the `gtk-integration-tests` modules are not compiled at all, so neither clippy
   nor `cargo test` sees them and they rot unnoticed — they reached the point of
   **not compiling** (a `BufferSpan` refactor changed a type they destructure) while
   every gate stayed green, because a suite outside the gate protects nothing.
   ScrAP-124.
3. `cargo build --release` — must compile cleanly
4. `cargo test` — all tests must pass
5. `scripts/run-integration.sh` — all tests must pass. That helper IS the step; the
   contract points `cmd.linux integration` at it, and its header is the authority on
   what it owns that a command line cannot carry. **No count here** — the header
   enumerates them and the list has already grown once.
   Needs a display; `xvfb-run` supplies one headlessly, and these tests present real
   windows and pump the frame clock. Do NOT skip this step when no display is handy:
   skipping is how step 2's failure mode arises.
   **RUNNING THE UNDERLYING COMMAND BY HAND: `xvfb-run` GOES OUTSIDE, `dbus-run-session`
   INSIDE.**

   ```sh
   xvfb-run -a dbus-run-session -- cargo test --features gtk-integration-tests
   ```

   The reverse — `dbus-run-session -- xvfb-run …` — is the order that reads more
   naturally and it LEAKS. The bus then starts before the display exists, inherits the
   ambient `DISPLAY`, and hands it to every service it activates: portal, gvfs, a11y and
   the rest attach to the DEVELOPER'S REAL X SERVER rather than the Xvfb, and they
   outlive the bus because systemd reaps them as a subreaper. Measured on this project:
   ~20 leaked connections per run, accumulating unnoticed across a week until Xorg hit
   its 256-client limit and no application on the desktop could start. It fails
   favourably — with slots to spare every test passes — so a green run is not evidence
   the nesting is right.
   **Run it through Cargo, so `.cargo/config.toml`'s `[env]` applies.** These tests
   close real windows, which runs the production session-save path, and that path
   resolves `XDG_STATE_HOME` from the environment — so without that override the
   suite rewrites the tester's own `session.toml` (open tabs, geometry, theme). The
   `[env]` table supplies a scratch directory under `target/`; a runner that invokes
   the test binary directly, or that scrubs the environment, must set
   `XDG_STATE_HOME` itself.
   This step runs **two** GTK harnesses over the same bodies: the lib test target
   (libtest) and `--test gtk_suite`, whose `main()` runs them on the process main
   thread. Both must pass, and their GTK counts should match — a divergence means a
   body registered with one harness and not the other. Write bodies as
   `#[gtktest::test]`; see the testing section below for that rule, for when a check
   needs its own `harness = false` target instead, and for why the choice matters.
   **In a session with no accessibility bus — any agent session, and any session
   whose `at-spi-dbus-bus.service` has gone stale — this step SIGTRAPs before it
   tests anything.** GTK emits `Unable to connect to the accessibility bus` as a
   `Gtk-CRITICAL`, and `G_DEBUG=fatal-criticals` promotes it. The failure names
   accessibility and is not about accessibility, is not about the change under test,
   and reproduces identically on an untouched tree — so it costs a control build to
   disbelieve, every time, unless it is written down. **The runner now handles this** —
   `scripts/run-integration.sh`, which the contract points step 5 at on Linux, wraps the
   suite in `dbus-run-session` — INSIDE `xvfb-run`, per the nesting rule above — so
   at-spi autolaunches on a bus of its own, on the throwaway display rather than yours. This paragraph
   used to instruct the reader to do that by hand, with nothing in the toolchain doing it,
   so every caller either supplied the wrapper themselves or ate the SIGTRAP; the helper's
   header records what else it owns and why none of it fits on a command line. On the operator's live session the
   other repair is `systemctl --user restart at-spi-dbus-bus.service`, which is needed
   when the unit reports `active (running)` while its socket no longer exists —
   `org.a11y.Bus.GetAddress` then returns an address for a socket that is not there,
   so the service looks healthy from every angle except use. MEASURED twice this way,
   once after a nested `dbus-run-session` in a test rig unlinked
   `/run/user/1000/at-spi/bus_0` on teardown: a rig that claims that path takes the
   operator's session down with it.
6. **Coverage gate** — `scripts/coverage.sh` must pass. Scoped line coverage is a
   no-regression **ratchet**, not a target: the script owns the floors (`FLOOR`,
   `FLOOR_FULL`) and the scope (`IGNORE`), and is the only place any of them is written
   down. **Do not restate any value here** — a second copy is exactly how the floor
   silently fell ~2pt behind the code and stopped protecting it.
   **The gate measures one scope TWICE and renders a floor verdict on each.** Leg A runs
   the unit tests alone (`FLOOR`); leg B runs them plus the GTK integration suite
   (`FLOOR_FULL`), under the same throwaway-session wrapper step 5 uses. Two legs exist
   because a unit-only number cannot see code that `#[gtktest::test]` bodies exercise
   thoroughly — landing such code *dropped* the figure, and the only lever a single
   number offers is to lower the floor, which happened twice. Leg B turns that event
   round: tested GTK-wired code raises `FLOOR_FULL`, untested GTK-wired code lowers it,
   and neither reads as ordinary drift. Leg A stays because it is the only leg that can
   see whether a decision core was extracted at all.
   **`FLOOR` may only be lowered in a change that RAISES `FLOOR_FULL`.** That is the
   rule that makes the pair a ratchet rather than two numbers, and no script can enforce
   it — a script cannot see history. It is stated in `scripts/coverage.sh`'s header, next
   to the numbers, and repeated here only because it is a rule about how to *work*, which
   is this document's job.
   **Both floors are WHOLE NUMBERS, and each advances one whole point at a time.** It rises
   only once measured coverage *reliably* reaches the next integer — on every host that
   runs the gate, not on the machine that happened to measure it. Coverage sitting at
   76.8 keeps a floor of 76; the floor becomes 77 when the figure reaches 77 with room
   to spare. Deriving a floor to the second decimal is what made it track the tester
   rather than the tree, and a whole number is wide enough that the residual
   host-dependence in the scoped set cannot move it.
   **Sub-point movement is not a finding.** Coverage drifting by fractions of a percent
   — between hosts, between runs, or across a change — is normal measurement noise and
   the expected consequence of ordinary work. Do not raise it with the operator, do not
   adjust `FLOOR` for it, and do not treat a change that moves it as owing an
   explanation. What is worth reporting is the gate going **red**: a whole point lost
   means real coverage was removed, and that is the event this ratchet exists to catch.
   **Scope rule:** GTK signal-wiring that cannot be exercised headlessly is excluded;
   pure decision logic is always in. So **when adding logic to an excluded file, extract
   the decision core into its own logic module** (as `winstate` does) rather than letting
   it hide behind the exclusion — that extraction is the mechanism by which the floor
   rises. The excluded set and its per-module rationale live beside the regex, in the
   script.
   **The measured scope is itself gated, and it is reported FIRST.** The set of files
   the gate measures is recorded in `scripts/coverage.scope`, re-derived on every run
   from the same invocation that produces the percentage, and compared. When it differs
   the gate names the files that entered or left, states that the SCOPE changed, and
   **withholds the floor verdict entirely** — a ratchet compared across two different
   scopes measures nothing. This exists because every `IGNORE` term names a directory
   *depth*, so a new subdirectory under an excluded tree used to pull its files into
   scope at 0% and the ratchet then failed as *"your change reduced coverage"*, sending
   the reader after untested code they had just written; when the entering files were
   small it cleared the floor and said nothing at all, which is worse. It catches the
   opposite direction too — a floor that climbs because an exclusion was widened is now
   a failure rather than a success. **Leg B is scope-checked against the same manifest**,
   which is what makes its number comparable to anything: enabling the feature compiles
   test scaffolding as whole files, `IGNORE_TESTONLY` removes the ones known to be
   scaffolding, and the equality check names anything else rather than absorbing it at
   whatever coverage a test file happens to have. Update the manifest with
   `scripts/coverage.sh --update-scope`, after deciding which side each named file
   belongs on, in the same commit as the layout change, exactly as raising `FLOOR` is.
   The manifest records what IS measured; `IGNORE` remains the policy about what should
   be, and the manifest is never passed to llvm-cov.
   Tooling is `cargo-llvm-cov`, a dev subcommand rather than a crate dependency:
   `rustup component add llvm-tools-preview && cargo install cargo-llvm-cov`.
7. **UI-behaviour coverage alignment** — if the change adds, alters, or removes any
   user-visible behaviour (a command, action-sensitivity, rendering, layout, dialog,
   shortcut, view mode, tab/window behaviour, toast, status text), **add, modify, or
   remove the matching check(s) in `tests/MANUAL-TEST.md`**; where the behavioural
   *contract* changed and not just its wording, update the `sdd/TDD.md` rubric the
   check traces to. A behaviour change landed with no MANUAL-TEST.md edit is
   **incomplete**. This step edits the plan *template* only — it never runs the plan
   or records results in it. The converse obligation (every rubric needs a check) is
   in § Testing → Manual integration testing.
8. **Architecture-diagram alignment** — if the change alters the architecture (a
   new top-level component, a changed data flow), **edit `sdd/system-overview.svg`
   in the same change**. It is the map the rest of [TECH.md](TECH.md) hangs off, and
   a stale diagram misleads faster than stale prose. It is plain hand-authored SVG
   (shapes/lines/`<text>` plus a `prefers-color-scheme` block). **After editing it,
   run `xmllint --noout sdd/system-overview.svg`** — this is a gate, not a nicety:
   a clean render in Inkscape or a browser does not mean the file is valid, and
   librsvg rejects outright what those recover from, so the app shows its own
   diagram as a broken-image placeholder (ScrAP-130). Then check it on a
   light **and** a dark background; most rasterisers — librsvg included — ignore
   `prefers-color-scheme`, so the light defaults must read on their own.
9. **Cross-reference gate** — `cargo xtask lint-references` must pass. One command on
   every platform, and that is the point of it being a `cargo` subcommand: this gate was
   a bash script plus a hand-synced PowerShell port, ~3,400 lines implementing one rule
   twice, and the premise that forced the split — "neither platform has the other's
   shell" — was measured false, while a single QA round found seven defects that existed
   only because there were two ports. `cargo` is on PATH wherever this repository builds.
   The gate crate is a workspace **default member**, so steps 1, 2 and 4 format, lint and
   test it like any other code; its corpora are ordinary `#[test]` cases rather than a
   bespoke `--self-test` mode, which is where those defects concentrated.
   **A gate is its pattern *and* the set of files it runs over.** The set is a data file,
   `scripts/lint-references.scan`, which the binary reads rather than restates — *one*
   enumeration, which **every** check consumes. Its `maxdepth` is a tripwire, not a
   filter: a file past the budget makes the gate refuse to run and name it, because a
   budget that silently truncated the set would make a check leniently incomplete without
   saying so. That the set is half the gate was learned the hard way — the two ports had
   drifted on enumeration while POLICY claimed a shared pattern meant neither could be
   the lenient one, so `.agents/`, `docs/` and `THIRD-PARTY-LICENSES.md` were linted on
   Linux and invisible on Windows (ScrAP-207).
   It enforces its rules mechanically — the crate owns the check definitions and **the
   count is deliberately not restated here**, for the same reason as the coverage floor
   and the input limits: this sentence said "nine" while the gate implemented fourteen,
   and no reader could tell. The run output enumerates every check by number and title as
   it executes. The classes are citations into the SDD registers; two over the test
   architecture, both of whose failure modes are silent (that `src/gtk_suite.rs`'s
   duplicated module list has not drifted from `src/lib.rs`'s — a module missing there
   drops every test body inside it from the main-thread run, with nothing failing — and
   that `#[gtk::test]` has not returned in place of `#[gtktest::test]`, check 5, nor been
   PRESCRIBED in the documents a developer acts on, check 5b: a lint's input set is
   source, so until 5b existed nothing in the toolchain could read the prose telling
   someone to write the banned attribute, and this file did exactly that for as long as
   check 5 had existed, ScrAP-222); one over document paths, that every file the tree
   points at must exist. That last is what a plan retirement breaks, since a `PLAN.*.md`
   is deleted by design once implemented and every pointer written while it existed
   dangles at once — including the bare `PLAN.<topic>` **section** citations code
   comments actually write (no `.md`), which is the form that let 21 danglers survive a
   sweep that believed itself complete. It deliberately ignores a bare document name used
   as a *mention* in prose, which resolves against nothing. Another is over the
   **application ID**: `src/icons.rs` is its source of truth and Rust derives it from
   there, but the desktop entry, GResource manifest, `Info.plist` template and the
   install/uninstall scripts each restate the literal, and a change to one of them fails
   no build while breaking a different platform's icon or Launch Services registration.
   Another is over the **citation FORM**, and it is the one rule here that bans a
   spelling rather than checking a target: an entry in `sdd/ANTI-PATTERNS.md` is cited
   `ScrAP-N`, one in the `gtk4-rs` skill `GTK4Rs/AP-N` and one of its techniques
   `GTK4Rs/T-N`, one in the `general-engineering-principles` skill `GEP-N`, and **a bare
   `AP-N` or `T-N` is illegal anywhere in the tree** (check 8). Illegal, not "means the skill" — it was this
   project's spelling historically and the skill's later, so its correct and incorrect
   uses are textually identical and no reader can tell a deliberate citation from one a
   sweep missed. **`sdd/ANTI-PATTERNS.md`'s own header is the authority on this list, and
   this paragraph is a summary of it** — read that one before concluding a form is
   unsanctioned. Said because this sentence enumerated only the first two for as long as
   it existed, and a seat holding the register's pen answered from it rather than from the
   register, told a peer that `GEP-N` was illegal, and was corrected by the peer quoting
   the header back with line numbers — with about eighty `GEP-N` tombstones already in the
   file. Every legal form is deliberately a **single token**: a two-word form is
   split by any Markdown or `rustfmt` wrap, and since a `GTK4Rs/AP-N` can only ever be
   checked for *form* (the skill need not be installed), its whole value is that a grep
   can **enumerate** the set a human must audit — which a wrapped citation silently
   drops. When a lesson is held by both registers, cite `ScrAP-N`; this one is always
   resolvable. ScrAP-231 records what the previous, laxer version of this rule cost.
   Checks 4, 5, 6, 7 and 8 were each mutation-tested when written, and so were the
   corpora that now stand in for the old self-test.
   One check is over **tracked PATH LEGALITY**: `< > : " | ? *` and a trailing dot or
   space are illegal in a Win32 filename, so **one such path makes `git checkout` refuse
   the entire tree** — not that file, the whole tree — blocking every Windows clone and
   anyone bisecting through the commit. MEASURED: an unquoted `sed -i 's|a|b|'` had its
   `|` eaten by the shell, wrote the replacement half to disk as a filename, and
   `git add -A` committed it; fmt, clippy, the whole suite and every other check all
   passed, and only the Windows seat could see it, one fetch later, by being blocked.
   **Its input set is `git ls-files`, deliberately NOT `lint-references.scan`** — the
   offending path landed in the repository root, outside the curated scan, and a check
   whose input is narrower than its hazard is ScrAP-132's species.
   The tenth is over **`.ps1` source encoding**: every `.ps1` in the scan set must carry
   a UTF-8 BOM **or** contain no byte above `0x7F` (check 13). Windows PowerShell 5.1
   decodes a BOM-less `.ps1` as the ANSI codepage, so a UTF-8 curly quote inside a string
   literal terminates the literal early and the parse dies naming an unrelated brace
   hundreds of lines away; pwsh 7 defaults to UTF-8 and is immune, which is the hazard —
   the contract-checking job runs pwsh 7 and certifies the break GREEN while the job that
   actually executes the pipeline runs 5.1. A disjunction rather than "keep them ASCII"
   because a BOM removes the hazard instead of dodging it, MEASURED on a real Windows
   host. It replaces an unwritten rule ("non-ASCII in comments only, never in a literal")
   whose enforcement depended on which side of a quote mark a character sat on.
   The three citation rules: no `sdd/ISSUES.md` entry may be cited from outside that
   file (SDD principle 6 — issue IDs are ephemeral, so every such pointer dangles
   when the fix lands, and *lies quietly* if IDs are ever compacted); every
   `ScrAP-N` cited in `src/` must exist in `sdd/ANTI-PATTERNS.md`; and every
   `ScrAP-N` cited *inside* that register must have a body in it, which is where a
   cross-branch transfer breaks and where nothing else is looking. **This
   gate exists because nothing else can see a wrong reference:** the first two classes
   were found in a single 103-line change *after* fmt, clippy `-D warnings` and 625
   passing tests had all gone green, and the third was found by hand during a
   cross-branch transfer — every other gate passes identically whether the citations
   are right or wrong. The crate owns the check definitions and the PLAN exclusion; do
   not restate them here. **A PASS does not mean the citations are correct** — check 2
   proves an entry exists, never that it is the right one; a real number naming the wrong
   lesson passes. That residue is a review obligation, and the reason it arises is
   documented at the check.

10. **Installer artefact** — OPT-IN (`--package` / `-Package`), and the only step that is.
    Every other gate answers "is this change valid?" and belongs after every edit; this
    one answers "can a stranger install this?", takes minutes, and answers the same way
    whether or not the last edit touched packaging. It is a gate rather than a chore
    because the property it defends is invisible from inside the repo: `packaging/linux/install.sh`
    builds from source into `~/.local` and needs cargo plus the `-dev` libraries, so a
    tree can look perfectly installable to everyone who already has a toolchain and be
    unusable by the audience the artefact exists for. Each platform's command is in the
    contract; all three read the version from `Cargo.toml`, because a hand-maintained
    version string in a second place is the same defect class as everything else here.
    A skipped packaging step is **announced** in the run output, never omitted — the
    same rule as a non-applicable step, for the same reason.

Do not skip any step. If `clippy` emits a warning, fix it — do not suppress it
with `#[allow(...)]` unless there is a documented reason in a comment on the same
line. A `disallowed-methods` rejection is **not** in that category: it is a routing
instruction with its own rule — see § "Typed GTK seams".

**Run the pipeline after every change, not just at session end.** Treat it as
part of completing each task: write code → fmt → clippy → build → test → done.
Do not report a task complete until every step passes. Running these steps only at
cleanup time lets broken changes pile up, making it harder to attribute which
change introduced the problem.

## Continuous integration

`.github/workflows/pipeline.yml` runs on every push. It exists because the parity between
the three ports was an assurance nothing checked: no single machine has all three, so the
diff was performed when someone thought to perform it, and the Windows port's step list was
*inferred* rather than measured. CI is the only machine where the comparison can happen.

- **The workflow invokes the runners and names no step.** There is not one `cargo` or
  `clippy` invocation in it; `execute-linux` runs `scripts/pipeline.sh` whole and takes its
  verdict. **Adding a step to `scripts/pipeline.steps` must not require editing the
  workflow** — a workflow that listed steps would be a fourth restatement of a contract
  whose entire design is derivation, and a clean diff between restatements proves only
  that two people copied the same list (ScrAP-207). Provisioning is the one thing the
  workflow may gain when a step is added.
- **Two jobs, two different claims, and the first does not imply the second.** `parity`
  proves the ports *agree* about the derived step list and the lint scan set; `execute-linux`
  proves a port can actually *run* a step. Keeping both is not belt-and-braces: the Windows
  port once passed `-ListSteps` byte-identically, `-SelfTest`, and a twelve-case mutation
  battery while an output-stream bug made it report `pipeline PASSED` with exit 0 after a
  step had failed. Contract-parsing evidence is evidence about contract parsing.
- **A CI gate is trusted only once it has been shown to FAIL.** A gate that reports success
  while something failed is the defect a gate exists to prevent, and this project has
  already produced one. `scripts/pipeline-parity.sh --self-test` runs inside the `parity`
  job on every run rather than once when it was written, and its battery includes the
  vacuous pass the job's own shape invites — a port whose job died before uploading leaves
  a directory whose survivors agree. Any new CI job carries the same obligation: demonstrate
  the failure, do not infer it from a green run.
- **THE ARTEFACT IS VERIFIED AS AN ARTEFACT, NOT AS AN EXIT CODE.** A packaging step that
  exits 0 having produced a zero-byte file is a defect class this project has shipped twice,
  and a third instance was measured on macOS: `codesign --sign` printed an error, wrote no
  signature at all, and RETURNED ZERO — defeating a guard written for precisely that risk.
  So each packaging job asserts its output EXISTS, is NON-TRIVIAL IN SIZE, and carries the
  version from `Cargo.toml`; an acting verb's exit status is a claim the tool makes about
  itself, and where a false green is expensive, follow it with a verifying one
  (`codesign --verify --deep --strict`) and check the file. ScrAP-329 carries the boundary.
- **Execution runs on all three platforms**, brought up in ascending order of difficulty as
  planned — Linux, then macOS, then Windows with a pinned and cached gvsbuild prefix. All
  three `execute-*` jobs run the platform's own runner and name no step, and all three
  produce an installer under `workflow_dispatch` with `package: true`. The contract jobs run
  on all three as well, because `--list-steps` and `--self-test` exit before any runner
  touches its environment, which is what keeps that matrix affordable on every push.
- **`G_DEBUG=fatal-criticals` is not set process-wide over the suite**, notwithstanding the
  recommendation in [§ Logging](#logging) — see the exception recorded there. The reasoning
  is in the workflow beside the job it applies to.

Direct pushes to a scratch branch are how the workflow itself is iterated on; a workflow
cannot be verified any other way, and it is worth knowing that **a workflow file is not
scoped by the branch it sits on** — it runs against the shared repository the moment
anything merges, which is why its triggers are a project-level decision rather than a
detail of whoever adds a job.

## Third-party attribution

**Every artefact this project publishes must carry the notices its dependencies require.**
Two obligations, and they have different scopes, so neither substitutes for the other.

1. **THE STATICALLY LINKED GRAMMARS — ALL THREE PLATFORMS.** `two-face`'s syntect grammar
   assets are compiled INTO the binary under MIT, Apache-2.0, BSD-2-Clause and
   BSD-3-Clause, every one of which requires the notice to travel with a binary
   distribution. A statically linked dependency leaves no file of its own in the installed
   tree, **which is exactly why this was missed on all three platforms at once**: nothing
   was absent that anyone could see. `THIRD-PARTY-LICENSES.md` is generated at build time
   from `notices/*.md` and must reach every artefact — Linux via `payload.sh`, Windows via
   `stage.ps1`, macOS into `Contents/Resources/`.
2. **THE BUNDLED RUNTIME — WHEREVER ONE IS BUNDLED.** An artefact that ships an
   LGPL-family GTK stack must attribute it. This is Windows AND macOS: Windows has always
   bundled, and the macOS `.app` began bundling when self-containment landed, so THE SCOPE
   OF THIS OBLIGATION FOLLOWED THE BUNDLING RATHER THAN BEING RE-DECIDED. Linux is exempt
   as a measured fact, not an assumption — the packages `Depends:` on the system GTK and
   bundle no runtime. Detail lives beside the artefact it describes:
   `packaging/windows/licenses.psd1` and its `PROVENANCE.md` / `SOURCE-AVAILABILITY.md`.

**THE IN-APP CLAIM IS PART OF THE OBLIGATION, not a description of it.** The About dialog
tells every user "Full notices: THIRD-PARTY-LICENSES.md (in the distribution)". That
sentence is only true because something stages the file, so deleting a staging step does
not lose a file — **it falsifies a claim the running product makes about itself**. Treat
those `cp` lines as load-bearing.

**GATE IT WHERE YOU CAN, AND SAY SO WHERE YOU CANNOT.** Presence, non-emptiness and a
content anchor are file-shaped and belong in a gate — `packaging/windows/verify-licenses.ps1`
is the worked example, and its anchor tests the TEXT'S IDENTITY rather than the file's
existence, because a licence file containing the wrong licence satisfies every
presence check. The DETERMINATION — which licence covers which binary — is a claim we make
and is **not** derivable: an SPDX `OR` needs an election and an `AND` does not say which
part covers the artefact we shipped. Where a determination is unmade, mark it
NOT GATE-ENFORCED in the table rather than letting a green gate read as a licensing verdict
(ScrAP-278).

## Artefact signing

**No published artefact is signed with a trusted identity today, and the intent line in
`scripts/pipeline.steps` is deliberately NOT narrowed to match.** Step 10 states the
property an artefact must have — installable by a non-developer with no toolchain — and one
platform not reaching it is a gap to close, not a standard to lower.

| Platform | State | What the recipient meets |
|---|---|---|
| Linux | `.deb` / `.rpm` unsigned | Ordinary for a direct download; not a Gatekeeper-class barrier. Intent holds. |
| Windows | Installer unsigned | SmartScreen warns on a downloaded file and the publisher shows as unknown. Annoying, passable. |
| macOS | **Ad-hoc signed** | Gatekeeper REFUSES it on any machine that did not build it, and macOS reports that as *"is damaged and can't be opened"* — a trust verdict in the vocabulary of corruption. Step 10's intent is **partially met**. |

**DEFERRED, NOT REJECTED.** Closing the macOS gap needs a Developer ID Application
certificate, `codesign --options runtime`, `notarytool submit` and `stapler staple` — a paid
enrolment bound to a legal identity, which is an operator decision and not a technical one.
The workflow is built so signing inserts into an existing job rather than forcing a
redesign.

**UNTIL THEN THE LIMITATION IS ANNOUNCED BY THE THING THAT PRODUCES THE ARTEFACT, and again
where the recipient meets it.** `bundle.sh` prints it on SUCCESS, unconditionally, because a
limitation mentioned only in a README is one the person holding the `.app` has already
walked past; `packaging/macos/README.md` carries the override (`xattr -dr
com.apple.quarantine`, or right-click ▸ Open) stated AS an override of a security decision
the recipient's OS made for them. **A green step 10 must not be readable as "a stranger can
install this"** when today it means "a stranger is told it is damaged".

**WHEN NOTARIZATION LANDS, THE ANNOUNCEMENT COMES OUT IN THE SAME CHANGE AS THE STAPLING.**
An artefact that is notarized and still warns that it is not is this same defect pointed the
other way.

## Optional diagnostics

- `cargo valgrind test` / `cargo valgrind run` (`cargo install cargo-valgrind`)
  can be run on demand to check for genuine memory-safety bugs in application
  code. Not part of the required pipeline above and not run automatically.
  A raw run against the GUI reports a large baseline of toolkit-internal
  "errors" (GTK/Pango/Fontconfig caches GTK never frees before exit) — when
  interpreting results, filter every error's stack trace for a `scribobulate::`
  frame before investigating it as an app bug; see ScrAP-49.

## Testing

### Regression coverage — two independent areas, both required

Any fix for a **regression** (a user-visible behaviour that previously worked, or
was expected to, and broke — whether caught in review, in manual testing, or
reported by the operator) must be locked in by coverage in **two independent
areas**, so it cannot silently return:

1. **An automated test** — a unit test and/or a `gtk-integration-tests` test.
   Prefer a unit test on an extracted pure decision core (as `winstate::copy_target`
   pins the split focused-pane fix, TDD 9.25); reach for a GTK-object integration
   test only when the behaviour is genuinely about live widget/signal state that
   cannot be decided from data. At least one automated test is required.
2. **A `tests/MANUAL-TEST.md` check — non-optional.** Add or extend the check that
   drives the *exact* broken scenario, tracing it to its `sdd/TDD.md` rubric (add
   the rubric if the regression exposed a behavioural contract that was never
   pinned down), **then actually run that check** via the automated UI loop below
   and read the after-state — proving the fix on the same path the manual plan
   describes.

Both areas are mandatory, not either/or: the automated test proves the decision
logic; the manual run proves the *running app* behaves (a correct decision never
wired to the on-screen widget passes the unit test yet still fails the user — the
two catch different failure modes). A regression fix landed with only one area
covered is **incomplete**. This specialises build-pipeline step 7 and the
live-verification rule below for the regression case, and — because the fix now
has a rubric plus a test on both the logic and the live path — guards it from
silently returning in a future change.

### Unit tests

- Unit-test the logic that does not require a live display: the Markdown
  AST → widget mapping decisions, the document model (dirty flag, the
  content-gated save guard), and the conflict/reconcile policy. Run with
  `cargo test`.
- **Never `#[cfg(platform)]` a test.** A cfg'd-out test is not skipped, it is
  deleted: not compiled, not reported, not counted, and no harness distinguishes
  "never built" from "passed". Give the test a platform-appropriate
  implementation and skip at *runtime* where the platform genuinely refuses,
  printing `SKIPPED [<rubric>]: <reason>` so the build pipeline can grep and
  report it. A test whose subject is a **symlink** goes through
  `testsymlink::symlink_or_skip`, which does this and also asserts the fixture is
  really a symlink before the test trusts its own verdict.
- **A test that installs PROCESS-global state restores it before it returns.** libtest
  runs the whole suite in one process, so a signal disposition, an alternate signal
  stack, an environment variable or any process-wide hook left armed by one test
  silently reconfigures every test after it — and the failure then surfaces in an
  unrelated test, later, and intermittently, which is the shape that reads as "the gate
  is flaky" rather than "the suite is broken". Restore from an RAII guard that also
  holds whatever mutex serialises the installers, so the restore cannot be lost to an
  early return or a panic, and so nothing can re-arm between the restore and the
  assertion that checks it (`forensics::signal::tests::ArmedHandler`). This is at its
  worst when the hijacked state is *diagnostic* machinery: it does not fail silently, it
  manufactures authoritative-looking evidence about the wrong subject — ScrAP-265, where
  the artefact was a full crash report naming an application that had not crashed.

### GTK-object integration tests

- For a regression that is genuinely about live GTK object identity/signal
  wiring OR about the real GTK runtime (not decidable as pure data — e.g. "is
  this signal still connected to the widget actually on screen after a rebuild",
  or "do the buffer offsets a feature captures match the live `GtkTextBuffer`"),
  a real `gtk::init()` + widget test is preferable to leaving it uncovered. Gate
  any such test behind the `gtk-integration-tests` Cargo feature (empty feature,
  no extra deps) so plain `cargo test` — and any headless CI runner without a
  provisioned display — is unaffected. Run explicitly with `cargo test
  --features gtk-integration-tests`; this requires a live GDK display
  (X11/Wayland/broadway — Xvfb in CI). Prefer this over silently skipping when
  no display is found — fail loudly so a missing display reads as a CI/env
  problem, not a silently-vacuous pass.
- Keep these tests in their owning module (`#[cfg(all(test, feature =
  "gtk-integration-tests"))]`), not a `tests/` directory file — a `tests/*.rs`
  integration test links the crate externally and so sees only `pub` items, not
  the `pub(crate)` tree these tests reach into. The main-thread runner that does
  need that reach is a second crate root (`src/gtk_suite.rs`), not a `tests/`
  file, for exactly this reason.
- **A helper that exists only for these tests carries their cfg, not a bare
  `#[cfg(test)]`.** An item gated `#[cfg(test)]` whose only callers are gated
  `#[cfg(all(test, feature = "gtk-integration-tests"))]` still compiles under a
  plain `cargo test`, with nothing to use it — so step 4 reports it as dead code
  while step 2, which passes the feature, stays silent. Gate the item to the same
  cfg as its callers and the `-D warnings` gate stays satisfiable without an
  `#[allow]`, for the same reason platform seams are `#[cfg]`-gated at the module
  declaration.
- **Use `#[gtktest::test]`, not `#[test]` + a manual `gtk::init()`.** GTK is
  single-threaded (gtk4-rs skill guardrail #1) and libtest runs each test on its
  own thread, so a plain `#[test]` that calls `gtk::init()` works only for the
  *first* GTK test in the binary — the next thread's init panics ("Attempted to
  initialize GTK from two different threads"). The attribute runs every such
  test body serialized with a single init, so many GTK-object tests coexist and
  **no `--test-threads=1` is needed**. The test body then needs no `gtk::init()`
  of its own.
  **Never `#[gtk::test]`** — it is superseded, `cargo xtask lint-references` check 5
  rejects it outright, and the reason it must be the *portable* attribute is in
  [Verifying a change on macOS](#verifying-a-change-on-macos). That section is
  the authority on this rule; it is cross-linked from here because a reader
  working on Linux has no reason to open a macOS section, which is exactly how
  this bullet went on prescribing the banned attribute while the gate rejected
  it (ScrAP-222).
- This is a narrow exception, not a general pattern — reach for the plain
  unit-test style (extract a pure decision function, per the scope rule in the
  [coverage gate](#build-pipeline) above) whenever the logic in question can be
  decided from data rather than live widget state.

### Manual integration testing

- After changes to rendering, live reload, or conflict handling, verify end-to-end
  against the relevant TDD.md rubrics. **Integration tests trace back to TDD.md
  rubrics** — review the rubric before writing or changing a test; a test that maps to
  no rubric is probably a unit-test concern.
- **Every bug fix or behaviour-affecting change must be proved against the running
  app before it is done — `cargo test` passing is not sufficient.** It proves the
  pure-logic core; it cannot see a signal that never fires, a dialog that never
  appears, or a widget that silently fails to update. Drive the *exact* scenario the
  fix describes and read the after-state (a screenshot, a status message, a
  window/tab count) — not "the process didn't crash". This is **per fix**, not once
  per session: N fixes need N verified scenarios, though they may share one app
  instance. A refactor with no intended behaviour change still needs its main paths
  re-driven.
- **How to run it:** `tests/MANUAL-TEST.md` §1 "Dev loop" for this app's specifics
  (that section is the *Linux* loop; on macOS or Windows read its §A "Platform
  procedures", which answers the same mechanics per platform),
  and the `gtk4-rs` skill's automated-UI-testing module for the loop itself
  (launch/PID discipline, window lookup, input delivery, capture, Xvfb). Neither
  procedure is restated here — this document says *when* verification is owed, not
  how to perform it.
- **A new `sdd/TDD.md` section ships with its `tests/MANUAL-TEST.md` section in the
  same change.** Build-pipeline step 7 is change-triggered and maps a check up to its
  rubric; nothing ever asks the converse — *does every rubric have a check?* — so a
  feature can land with its rubric written and its check never created, and no gate
  notices, because a missing section looks exactly like a passing one. When a plan is
  retired or a feature debriefed, diff the two lists (`grep '^## ' sdd/TDD.md` vs
  `grep '^### §' tests/MANUAL-TEST.md`) and close any gap. A rubric with no check is
  unverified, not verified-and-passing — record it as such.

### Footprint verification (TDD §6 gate)

**VRAM ceiling: 50 MiB — hard limit, no exceptions.**

This is the project's reason to exist. Exceeding it is not a warning — it is a
go/no-go failure. If a significant change causes VRAM to exceed 50 MiB, the
change must be reverted and the team must pivot: change the approach, replace
the offending dependency, or reconsider the feature. Do not ship a workaround
that keeps the number under the ceiling in a contrived measurement scenario
while letting it climb in realistic use.

**What counts as a significant change:**
- Adding or changing a rendering dependency
- Adding a new rendering path (new widget type, new compositing approach)
- Adding a new process or subprocess
- Any change to how `GSK_RENDERER` is set or when GTK is initialised

**After every such change, measure it** — the procedure is `tests/MANUAL-TEST.md`
§1.8, run against a **release** build with a representative document open, and on the
operator's real session (a bare `Xvfb` has no GPU driver stack). Two rules that are
easy to get wrong: **on Linux** `nvidia-smi` must show **no GPU client process** for the
app at all (any VRAM reading above 0 means the Cairo renderer is not active), and RSS
must not climb across repeated live-reload cycles.

**The "above 0" rule is Linux/`nvidia-smi` accounting — do not apply it on Windows.**
There, "no GPU client at all" describes the *absence of a process row*, not a byte
count. Windows composites every visible window through the GPU, so a healthy
Cairo-rendered build still reports a few MiB of fixed per-process driver overhead;
reading the rule literally would fail a build that passes the actual gate. The Windows
criterion is **TDD 6.5**: the reading must not grow with window area or document
complexity, and GPU engine utilisation must stay near zero. The 50 MiB ceiling is
unchanged and applies on every platform.

**Do not apply the "above 0 means broken" test literally off Linux.** macOS
composites every visible window through the GPU, so a non-zero reading there is
the window server's fixed overhead, not evidence that the Cairo renderer is
inactive. The macOS formulation of the gate is TDD 6.4, and it discriminates on
*scaling* rather than on a byte count: the reading must not grow with window area
or with document size, and GPU engine utilisation must stay near zero. A reading
that scales with either is the macOS signal that the footprint contract has
broken. The 50 MiB ceiling itself is unchanged on every platform.

If VRAM exceeds 50 MiB: **stop, revert, pivot.** Do not continue feature work
on a stack that has broken the footprint contract.

### Verifying a change on macOS

**Write every GTK test body as `#[gtktest::test]`, never `#[gtk::test]`.** The
attribute is a drop-in — it takes no arguments and the body needs no change — and it
registers the body with *both* harnesses: libtest as before (same test name), and
`src/gtk_suite.rs`, the `harness = false` target whose `main()` runs bodies on the
process **main** thread. That second run is the only one available where GTK
initialises solely on the main thread, because gtk4-rs dispatches `#[gtk::test]`
bodies onto a `glib::ThreadPool` worker (`--test-threads=1` does not help; it governs
libtest's concurrency, not which thread the binding's own pool uses).

Choosing `#[gtk::test]` over `#[gtktest::test]` is **invisibly** wrong: the test
passes on Linux, so nothing fails, while the body is silently absent from the portable
run. `cargo xtask lint-references` check 5 therefore rejects the attribute outright — a lint is
the strongest available rung, since a clippy `disallowed-methods` ban cannot reach an
attribute macro. Check 5b rejects *prescribing* it in these documents, which is the
same failure one level up (ScrAP-222).

A body still needs its **own** `harness = false` target, not the suite, when its
assertion is *about* process-global GTK state — the icon theme or resource
registration, `GtkSettings`, the theme name or variant, focus/window state, the
default display — because GTK cannot be un-initialised and the suite shares one
process across every body. `tests/icon_resolution.rs` (a pristine icon theme, plus its
own `--render` argv), `tests/macos_dark_mode.rs` (drives `prefer-dark` both ways) and
`tests/popover_deferred_focus.rs` (a clean focus curve) are the three standing cases.
Record the reason in the target's doc comment so it is not later "tidied" into the
suite. Do not reach for `examples/` instead — an example also owns `main()`, but
`cargo test` never runs one, so the check silently stops being a gate.

So a change on a platform whose GTK is main-thread-only is verified by the non-GTK
unit tests, the main-thread suite, the standalone targets above, and a manual pass at
the physical machine for anything none of them can reach.

**Linux remains the gate for GTK-level regressions.** A macOS-only defect is
therefore discoverable only by the above, which is why the manual suite carries
platform-specific items rather than assuming parity. **And the converse now has a measured
instance**: a Linux-era guard can be *inert* on the macOS GTK without being broken —
ScrAP-157's collapse-all guard passes there with the fix it guards removed, because
`GtkListView` 4.22.4 does not exhibit the 4.6 defect at all. So a green macOS suite is not
evidence that a Linux-era fix is still required; the guard is live only where the defect is.

**A locked macOS session makes that suite RED, and correctly so.** `codeview::markers`'s
a11y focus test panics on its own precondition — "the stand-in can hold the focus (a mapped,
active toplevel)" — rather than passing when no active toplevel exists. That is a positive
control doing its job, not a defect: a locked screen has no active toplevel. Do not chase it,
do not "fix" it by relaxing the precondition, and do not report the suite as passing on a
locked machine. Unlock and re-run.

**Before filing a defect as macOS-specific, have the Linux counterpart try to
reproduce it.** Behaviour found here is not platform-specific until a peer fails
to reproduce it — an idle CPU spin found during this port reproduced on Linux and
would otherwise have been filed against the wrong platform, under the wrong cause.

### Platform seams live in `src/platform/<os>/`

Code that exists only because a platform lacks something the others get for free
goes under `src/platform/<os>/`, `#[cfg]`-gated **at the module declaration** and
never internally, so another platform's build compiles none of it and the
`-D warnings` gate stays satisfiable without an `#[allow]`. Prefer hand-rolled
FFI over a new binding crate for a handful of calls.

The bar is narrow, and it is about *plumbing, not behaviour*: a platform seam
supplies a source or a transport the toolkit does not, and feeds it into the
machinery every platform already shares. It must never own application behaviour
of its own, or the platforms drift. Both existing seams follow this — the
single-instance handoff emits the same `open`/`activate` the D-Bus path does, and
the appearance module writes the same `GtkSettings` properties a desktop would,
re-theming nothing itself.

## Cross-platform by default

**This project ships on Linux, macOS and Windows, and all three are first-class.**
Linux is the canonical platform for the gates (§ Build), which is a statement about
where verification happens — not about which platform the code is written for. Write
every line as portable unless it lives in a platform seam (§ Platform seams).

- **Reach for the standard library's portable abstraction before the platform's own.**
  `std::path::Path`/`PathBuf` over string concatenation with a separator; `std::fs`
  over an OS call; a portable IPC choice over a hand-picked transport. Where the
  toolkit or the standard library already spans the three platforms, use it and do not
  re-derive the difference.
- **Never hardcode a path separator, a path shape, or a filesystem root.** Not in
  production code, and — the case that actually bites — **not in a test fixture**. A
  POSIX-shaped literal like `/etc/passwd` reads as a constant and is not one: it is an
  absolute path on unix and a rooted, drive-relative path on Windows, so a test using it
  exercises a *different* case on each platform while looking identical everywhere. A
  fixture that encodes one platform's grammar must be `#[cfg]`-selected per platform, so
  each tests its own shape (a drive-qualified path and a UNC path on Windows, the POSIX
  one on unix) — not one platform's literal inherited by the others.
- **A predicate over a path is a question about the HOST's grammar, not about the
  string.** `Path::is_absolute` is the standing example: on Windows it requires a volume
  prefix, so a rooted path with no drive answers `false` there and `true` on unix.
  Reason about what the platform means by the term, not about what the method name
  suggests.
- **Filesystem and IPC facilities are not universal.** A unix domain socket is a named
  pipe elsewhere; a FIFO has no Windows form at all; POSIX mode bits are ACLs on
  Windows; a symlink needs privilege there while a directory junction does not; case
  sensitivity, path length limits and legal filename characters all differ (§ Build
  pipeline's path-legality gate exists because one illegal character blocks every
  Windows clone of the tree). Where a facility is genuinely absent, that is a platform
  seam, not an `#[cfg]` sprinkled through shared code.
- **Line endings are a property of the DOCUMENT, not of the host.** A CRLF file opens
  on Linux and an LF file opens on Windows, so never branch on the platform to decide
  what a line separator is, and never assume the host's convention for text this
  application did not write. The one rule the project holds about the shape a buffer may
  take is `lineendings.rs`'s — applied at the doors documents arrive by and never at a
  parse site — so do not re-derive it at a third site. Code downstream of that may depend
  on a separator being exactly `"\n"`, but **only where the code that EMITTED it is in
  view** (the preview renderer's own `newline()` is the standing case). Say so at the
  site and pin it with a test that renders the CRLF twin and compares: without one, the
  next reader cannot tell a measured invariant from an unexamined platform assumption,
  and neither can the seat asked to ratify it.
- **A capability a test needs may be unavailable rather than absent.** Skip loudly
  through the project's one skip marker so the run reports the limb as unverified — never
  let a guard compile to an empty passing function on the platform that most needs it.
  Before accepting a skip, ask whether a different mechanism reaches the same guarantee
  on that platform.
- **Verification is per platform, and the peer seats own it.** A change that touches
  the filesystem, IPC, packaging or a path is not ratified by passing on Linux; it is
  ratified when the macOS and Windows seats have run it (§ Verifying a change on macOS,
  § Cross-machine seat branches). Never weaken a shared rule — a lint, a gate, a floor —
  to make one platform pass.

## Code style

- Format with `rustfmt`; keep `cargo clippy` clean. Use `Result`/`?`; no
  `unwrap`/`expect` outside tests and startup invariants.
- **Soft limit 500 lines per file**, checked *before* the edit that would exceed it —
  decompose at that moment (extracting a decision core also brings it inside the
  coverage gate), not later.
- **Keep functions small and shallow.** Logic trapped in a long, deeply nested function
  can only be exercised end-to-end. GTK signal-wiring bodies are the pragmatic
  exception; hoist real computation out of them even so.
- **No magic numbers or magic strings.** A value carrying domain meaning gets a named
  `const`, a config value, or a live measurement; a closed set of values gets an
  `enum`, parsed once at whatever string boundary an external API forces
  (`FormatCmd::from_target`). Trivial values and a stable external identifier at its
  single boundary are exempt.
- **Widget-owned closures capture weakly** — `glib::clone!(#[weak] …)`, never a strong
  `self.clone()`: an uncollectable cycle that leaks the subtree on window close
  (ScrAP-60; the site shapes that must stay hand-rolled are in ScrAP-154).
- Destructure tuples by name, never positional `.0`/`.1`.

## Dependencies

- **Do not add a dependency without justification.** The current crate set and each
  crate's role are in [TECH.md § Rust crates](TECH.md#rust-crates) — check there for
  something that already covers the need before reaching for a new crate.
- **Never add a web engine or HTML-rendering dependency** (WebKitGTK, Servo,
  litehtml, etc.). The native-widget decision is deliberate; the cost of the
  alternatives is documented in ScrAP-1.

## Input limits

A Markdown document is **untrusted content** (TDD 2.7), and cost is part of the threat
model, not separate from it.

- **Every read of a document path goes through `limits::is_regular_file_within_limit`,
  never a comparison against the constants itself.** Both halves are required and
  neither implies the other: a size test alone admits a FIFO (whose reported length is
  zero) and then blocks the main thread forever, and a type test alone admits a 40 GiB
  regular file. A caller that reimplements one half has reimplemented the bug.
- **The numbers live in `src/limits.rs` and are not restated here**, for the same
  reason the coverage floor is not restated in the build-pipeline step above — a second
  copy is how the first one silently stops matching. That module records how each value
  was measured; re-measure rather than re-reason when changing one.

This section exists because its absence was the defect. Four unrelated
denial-of-service paths were found in four subsystems written by different hands, and
they were not four oversights — nothing in the tree said the project had an opinion
about input size at all, so each author reasonably assumed someone upstream did. The
full account is ScrAP-225; the rule above is what stops it recurring.

## Architecture rules

- **Render Markdown into native GTK widgets only.** No embedded web engine, no
  GPU-compositing UI stack. This is the project's reason to exist (PRODUCT.md,
  ScrAP-1).
- **Force the GSK Cairo software renderer in-process** (set `GSK_RENDERER=cairo`
  before GTK initialises). The app must never hold a GL/GLES context. The
  GTK integration suite never runs `main.rs`, so it pins the same
  renderer via `.cargo/config.toml`'s `[env]` table — every `cargo` invocation
  (tests and `cargo run`) inherits it, keeping the test harness on the exact
  stack the app ships on. Do not remove that pin (ScrAP-153).
- **On Windows, take the native Win32 frame** (set `GTK_CSD=0` before GTK
  initialises, `#[cfg(windows)]`, beside the renderer pin above). GDK-Win32
  defaults to client-side decorations; without this the app draws its own
  titlebar and loses native resize borders, the Alt+Space system menu, and Snap
  Layouts. Pinned in `main.rs` rather than in a launcher so the frame does not
  depend on how the binary was started.
  **Two constraints that are easy to break by accident:**
  1. **Never add a `GtkHeaderBar` or call `set_titlebar()`.** `GTK_CSD=0` only
     yields a native frame for an app with no custom titlebar — adding one puts
     CSD back silently, on every platform.
  2. **`GTK_CSD` must stay `cfg(windows)` and must NOT go in
     `.cargo/config.toml`'s `[env]` table** the way `GSK_RENDERER` does. That
     table is platform-unconditional, and `GTK_CSD` is a GTK-wide variable, not
     a Windows one: on Wayland, decorations are negotiated with the compositor,
     so forcing them off can leave a window undecorated. The renderer pin's
     precedent deliberately does **not** extend here.
  This was adopted on measurement, not preference: with `GTK_CSD=0` the window
  carries `WS_CAPTION`/`WS_SYSMENU`/`WS_MAXIMIZEBOX`/`WS_THICKFRAME`, all six
  resize edges hit-test *and* drag natively, the maximize button reports
  `HTMAXBUTTON` (which is what makes Snap Layouts work, with no manifest opt-in),
  and `GetSystemMenu` returns a real menu — against none of that under CSD. The
  rejected alternatives were an installer-set variable (the frame would then depend
  on how the app was launched), restyled CSD imitating Win11 buttons, and a forked
  chrome widget tree; the realistic ceiling is "native frame, GTK interior", since
  GTK4 has no maintained Windows theme.
- **`src/platform/win32/` is the ONLY place that may talk to Win32 directly.** Taking the
  native frame above hands the caption to DWM and leaves two things GTK does not
  do on Windows: it never asks DWM for a dark caption, and — verified against the
  shipped GTK 4.22.4, whose `gtk-4-1.dll` contains no reference to
  `AppsUseLightTheme` or `Themes\Personalize` — it never reads the system
  light/dark setting at all. That module supplies both, and is deliberately the
  whole of the project's Win32 surface: **one module**, `#[cfg(windows)]`-gated at its
  single declaration in `platform/mod.rs` (never internally), FFI declared by hand
  rather than by pulling in `windows-sys`/`gdk4-win32`. **New OS-level calls go in it or
  nowhere.** A second site reaching past GTK is how a portable codebase quietly becomes
  two.
  **The same module also holds the Windows repairs that need no OS call** —
  `track_maximized_size`, which keeps a maximized window from collapsing when a
  popover opens. The organising principle is the *cause*, not the mechanism: if
  something is broken because Windows rather than GTK draws the frame, it belongs
  here even when the fix is pure gtk-rs. Splitting on mechanism would scatter one
  decision's consequences across the tree.
  **It is a directory, and the children split by cause.** It was a single file until it
  passed 1100 lines against the 500-line soft limit below, at which point the two rules
  pointed opposite ways; the operator ruled for decomposition. What the one-place rule
  protects is that every call past GTK crosses **one** module boundary with **one**
  `#[cfg]` gate, and a directory preserves that exactly — `platform/mac/` has always been
  one. Every name stayed re-exported from `win32/mod.rs`, so no caller outside the module
  could tell. The same cause-not-mechanism principle governs the split itself: children
  are `frame` (the native frame's consequences), `appearance` (the light/dark source GTK
  does not read), `privacy` (an owner-only state directory, which `std::fs` cannot
  express here) and `process` (pid liveness, which Windows has no `/proc` for). There is
  deliberately no `dwmapi.rs` or `advapi32.rs` — splitting on which DLL a call lands in
  is the exact scattering this rule forbids.
  Note the decomposition corrected this clause rather than amending it: `platform/mod.rs`
  has said from the start that a platform gets "a directory where the platform needs more
  than one file, a single file where it does not", so **the rule was the outlier, not the
  code** — it had frozen the shape the Win32 surface happened to have while it was small
  into a property that read as load-bearing, and a reader obeying it literally would have
  been blocked by a second reader obeying the size rule. The general form, worth applying
  to every rule in this document: **when a rule names an artefact's *form*, establish
  whether the form or the boundary underneath it is the thing being protected before
  treating the form as binding.** Here the boundary — one module, one `#[cfg]` gate, one
  reviewable surface — was the whole point, and "file" was incidental to it.
- **The desktop's lightness has exactly one source: `palette::desktop_is_dark()`.**
  Anything that must follow light/dark — the editor scheme, the theme sheet, the
  Windows caption — reads that probe, and anything that *detects* a change writes
  into `GtkSettings`' `gtk-application-prefer-dark-theme` so
  `app::setup::connect_theme_change` propagates it. Never re-theme a surface
  directly from a platform signal: that builds a second channel that will drift
  from the first. The Windows registry poll is a *source* for the existing
  channel, not a bypass of it, and the KDE/X11 live-toggle gap must be closed the
  same way when it is.
- **One process, many windows.** A new document opens a window in the existing
  process; never spawn a second process for another document (TDD §8). The
  single-instance `GtkApplication` (`HANDLES_OPEN`) delivers this **only where
  GIO has a transport for it** — it negotiates over a D-Bus session bus, so on a
  platform that runs none the registration still succeeds and every launch
  silently becomes its own primary. Treat the rule as the requirement and the
  `GtkApplication` as one implementation of it: a platform without the transport
  needs an app-side substitute feeding the same `open`/`activate` handlers
  (`src/platform/mac/single_instance.rs`), not an exemption from the rule.
- **Never silently overwrite unsaved edits.** External changes to a file with a
  dirty buffer are resolved by notifying the user and letting them choose
  (TDD §5). Clean buffers reload automatically (TDD §3).
- **One `GAction` is the single source of truth for every command.** A command that
  appears in more than one surface — the menu bar, a toolbar/button-bar button, and a
  context-menu item — must have all of its manifestations reference the **same**
  `SimpleAction` **by name** (`set_action_name` / `gio::Menu` `action`), so GTK drives
  their behaviour AND their enabled/disabled (sensitivity) state from that one action.
  **Never** set a menu item's, button's, or context-menu item's sensitivity — or wire
  its activation — independently in a way that could diverge from the action. Drive the
  action's `enabled` state from one place (e.g. a single mode/selection gate) and let
  every surface mirror it automatically. This is a hard rule: independent per-surface
  enablement is how the surfaces drift out of sync (ScrAP-9). A context
  menu rebuilt per-invocation that *reads* `action.is_enabled()` is acceptable (it
  derives from the action); a per-surface `set_sensitive` computed from its own
  precondition check is not. The **set** of surfaces differs by command family, but
  the rule is identical for all of them:
    - *Editor/view commands* (`win.copy`, `win.undo`, `win.view-mode`, …) appear in
      the **menu bar**, the **toolbar/button bar**, and the **right-click context
      menu**.
    - *Formatting commands* (the parameterised `win.format`, targets `bold`…`hr`,
      `h1`…`h6`) appear in the **Format menu**, the **Format toolbar section**, and
      the **caret formatting overlay**.

    In both cases there is one `SimpleAction`, every surface binds to it by name, and
    one place gates its `enabled` state (the mode/selection gate for editor actions;
    the editor-focus gate for formatting). Never special-case one surface. These
    per-surface obligations are the checklist face of the **Action CAMs** in
    [`CAM.md`](CAM.md) — the two SSOT rows of those matrices *are* this
    rule, not a second copy of it.
- **Keyboard accelerators are part of the single-source-of-truth contract.** Each
  command's accelerator is declared **once** in its descriptor (`Cmd`/`FmtCmd`/
  `InlineCmd`) and registered from there via `set_accels_for_action`, which is what
  every displayed hint derives from — so the active binding and its displayed hint can
  never diverge. This holds for both the editor/view actions and the formatting
  actions. Adding or changing a command — or its shortcut — means editing its single
  descriptor, not each surface and not a separate accel-registration list.
  **The menubar model therefore sets NO `accel` attribute, and adding one is a
  regression even though it looks like an improvement.** GTK already draws the hint
  from the registered accelerator (`gtk_menu_tracker_item_get_accel` falls through to
  `gtk_action_muxer_get_primary_accel`, i.e. `accels[0]`) — on the `GtkPopoverMenuBar`
  *and* on the macOS system menu bar, which is a different renderer calling the same
  accessor. So an attribute cannot add a hint; it can only restate one, and where the
  two disagree **the attribute wins silently**. MEASURED: removing the whole mechanism
  left the View menu pixel-identical (AE=0), and a build that set `<Primary><Alt>F12`
  on Zoom In displayed exactly that while `Ctrl++` went on working. `menubar.rs`'s
  `no_menu_item_declares_its_own_accel_attribute` holds it. The context menu is the
  deliberate exception — it is hand-built widgets rather than a `GMenu`, gets no
  fallback, and so reads the descriptor itself.
- **Prefer extending an existing code path over adding a parallel one.** Before
  introducing a new function, action, save/write path, or render pass, scan the
  codebase for a path that already performs the equivalent work and extend it.
  Two near-duplicate paths carry a variable, compounding cost: every later change
  that touches the behaviour must account for *all* of them, and the gap between
  them is exactly where bugs and drift breed (the same reasoning behind the
  single-source-of-truth action rule above). A new path is justified only when
  reuse would force the existing one to serve two masters badly — and when you do
  add one, state why in the code and, if architectural, in TECH.md.
- **Every document read and write goes through `docio`, and none of them run on the
  main thread.** A `std::fs` call in a signal handler freezes the window for as long
  as the filesystem takes to answer, which on a stalled network mount is indefinite —
  the prohibition below has always said so, and `docio` is the place that makes
  obeying it the path of least resistance rather than a thing to remember. Its
  blocking halves are private, so the `async fn`s are the only route in; that
  encapsulation IS the enforcement (a `clippy.toml` ban on `std::fs::read_to_string`
  was assessed and rejected on the true-positive test in § Typed GTK seams).
  The boundary is **documents, not everything**: the application's own small state
  files — the session file, the config file, the swap-directory scan — stay
  synchronous, because they are ours, bounded, and their contents decide whether any
  window is built at all, so there is nothing on screen to keep responsive. Two
  consequences bind every caller: an operation that spans one of these awaits must
  resolve its subject **once** and carry it (ScrAP-244), and concurrent operations on
  one document must be serialised by the caller, because GIO orders neither the
  renames nor the completions (ScrAP-243).
- **Network access goes through `imagefetch`, and never through a `GFile`.** The
  application makes exactly one kind of outbound connection — a remote image on the
  opt-in "Show Unsafe Images" path — and it is the app's own HTTP client, bounded by
  the timeouts and byte cap declared there. Two rules follow, and neither is a
  preference. A URI handed to GIO (`gio::File::for_uri("https://…")`) resolves only
  where some backend claims the scheme, which is a property of the *host's desktop
  stack* and not of anything this project depends on: a daemon on Linux, an in-DLL VFS
  on Windows, nothing at all on macOS — so that route is a feature that silently does
  not exist on a platform (ScrAP-292). And a second network path would be a second set
  of timeouts, a second trust-store decision and a second proxy rule, of which the
  parts nobody reproduces fail only on machines nobody here runs. If a future feature
  needs the network, extend that module; if it needs it *not* to block the main thread,
  that is the same module's problem to solve once.
- **All GTK access on the main thread.** GTK is single-threaded. File watching uses
  `gio::FileMonitor`, whose `changed` signal is delivered on the main context (main
  thread); the app currently spawns no worker threads *of its own* — the one write that
  leaves the main thread (a crash-recovery snapshot) is dispatched to GLib's thread pool
  by `replace_contents_async`, which hands only an owned `Vec<u8>` across and returns its
  completion on the main context, so no GTK object ever crosses. Prefer that shape: a
  GIO async call whose payload is plain owned data costs no concurrency model, whereas a
  hand-rolled worker costs shutdown ordering and a new class of test. If future work needs
  background computation, do the heavy work off-thread but **apply every
  widget/`GtkTextBuffer`/`GtkAdjustment` mutation back on the main thread** via
  `glib::MainContext::spawn_local`, `glib::idle_add_local`, or a `glib` async channel
  — never touch GTK objects from a `std::thread` directly. Off-main-thread mutation
  races the main loop's layout→draw cycle and can leave a resize queued at snapshot
  time, which re-arms the same `alloc_needed`/"snapshot without a current allocation"
  blank as ScrAP-22/ScrAP-23 (a third, threading-borne path to that warning).
- **List styling is symmetric across list types.** Any change to list rendering —
  marker layout, hanging indent, per-depth indentation, or inter-item spacing —
  must apply equally to **unordered (bulleted)** and **ordered (numbered)** lists,
  and to **task-list (checkbox)** items, across every nesting depth. They
  deliberately share the `li-{depth}` hanging-indent tag family (`tags.rs`) so
  styling stays common in one place; never adjust one list type in a way that
  leaves the others behind. This is the same no-drift reasoning as the
  single-source-of-truth action rule and the CAMs — one shared mechanism, no
  per-type forks — but the axis (list types) is narrow enough that it is a plain
  rule here, not a matrix.
- **No hard-coded styling.** Every styling value the preview renders with —
  **colour, typography, and decoration geometry** — is sourced from the active
  theme, resolving in order: **the selected theme's key → `[themes.system]`'s key
  → the system GTK theme probe + derivation**. A literal in rendering code is a
  defect, whatever its type: `[themes.system]` (in `themes.toml`) is the register
  where an otherwise-hardcoded value lives, and it is an ordinary theme — **no key
  is system-only and no theme is second-class**. The rule covers geometry
  deliberately: a value that is hardcoded is hardcoded regardless of its type, and
  exempting geometry would make this rule mean less than it says. **Bounds:
  a theme selects from a closed vocabulary of decorations the engine already knows
  how to draw, and states their appearance and metric. It does not describe new
  drawing, and it does not change how layout is computed** (a bounded/centred
  "measure" is layout behaviour, and out). Two invariants make the wider bound safe,
  and a decoration that breaks either is out of the vocabulary regardless of how it
  is spelled: the engine holds **no per-theme knowledge** (TDD 18.14 — a new theme
  needs no code change), and every decoration is either **inert** (absent unless a
  theme asks for it, leaving System byte-identical — TDD 18.2) or occupies space the
  engine reserves for it **unconditionally**. An unset key means *not present*, never
  *guess*. The previous, narrower bound — "a theme sets the metric of a decoration;
  it does not change what is drawn" — is superseded; it is recorded here because
  code and comments written under it are still correct, just no longer the limit.
  Themed geometry is a **design-time value at zoom 1.0**, applied through the
  existing `px(n) = (n * zoom).round()` path — pixel metrics are widget/Pango
  properties and do **not** follow the CSS `font-size` rule, so they must be scaled
  explicitly on every render/zoom. Validate and clamp: a malformed theme must never
  break layout, on the same principle that a malformed config never prevents
  startup. Prefer types over sanitisers — geometry keys are integers (injection is
  then impossible by construction), whereas colour/font strings are interpolated
  into generated CSS and **must** be sanitised (a stray `}` or `;` injects CSS;
  ScrAP-127).
- **One theme key, every application path.** A surface that appears both in the
  body buffer and inside a table cell is fed by **one** theme key through both
  paths. The representations necessarily differ — `GtkTextTag` RGBA, Pango markup
  plus a separate `bgalpha`, a `u16` triple, generated CSS — but the *source* does
  not. Two literals for one surface is precisely the drift this rule exists to
  prevent, and it is already the shape of a live defect: the annotation and find
  highlights each carry two independent hardcoded copies (body tag + cell
  representation) that nothing keeps in sync. The same applies to geometry shared
  by a tag and a drawn marker — `LIST_STEP`/`LI_GAP` are defined once and imported
  by both `tags.rs` and `codeview/gutter.rs` *so the tag and its marker cannot
  drift*; a themed metric must preserve that single resolution point or it
  re-creates ScrAP-121. Same no-drift reasoning as the single-source-of-
  truth action rule and the list-symmetry rule above. **Verify themed geometry by
  the resolved pixel** (`iter_location().x()` on a realized view), never by
  tag-property equality — that bug class is invisible to tag-level tests
  (ScrAP-121).
- **Bundled decoration art is an original design.** A sprite or glyph shipped in
  `data/themes.toml` may evoke an idiom but must not reproduce another work's
  expression: no franchise names, no copied character, block or tile art, and no
  edited derivative of one — a trace is still a copy. A bare colour is the stated
  exception, carrying no copyrightable expression. This is a merge gate rather than
  a taste note, and it is not recoverable after the fact: art produced from a
  reference has to be discarded and redrawn, not sanitised.
- **Every menubar nested-submenu action must call `window::dismiss_stray_menubar_popovers`**
  after it fires (on the active window, for an app-scoped action). Otherwise GTK
  4.6–4.12 leaks the activation onto a top-level menu (ScrAP-116). It is
  opt-in and unenforced, so a new submenu that omits it silently regresses — as
  Reading Theme did.

## Typed GTK seams

GTK's runtime contracts — allocation timing, coordinate spaces, popover parenting
lifetime, offset spaces, weak capture — are enforced by nothing at compile time.
Documented in prose alone, each one is re-learned, re-violated, and re-debugged by
whoever writes the next call site; that repetition is the project's single largest
avoidable cost. The standing answer is to **promote a recurring contract into a
type or a choke point** that makes the wrong call impossible or non-compiling, and
to back it with a mechanism nobody has to remember. `src/saferizer/` is that
module; `clippy.toml`'s `disallowed-methods` is its primary teeth (`-D warnings`
in build-pipeline step 2 makes a bypass a build failure). TECH.md lists what has
landed; the rules for adding to it are here.

- **Seam-first when a task would re-invoke a typeable contract.** If the code you
  are about to write would re-enact a contract the register already records, and a
  seam exists, call it. If a seam does not exist and the contract is typeable,
  introduce or extend one as part of that change rather than obeying the prose one
  more time. Marginal cost at the moment of the change is small; the cost of not
  doing it recurs at every future call site.
- **A ban is a routing instruction, never an obstacle. Never `#[allow]` your way
  past one.** When clippy rejects a call, its `reason` names the sanctioned route —
  take it. Sanctioned callers are defined by that reason string, not by living in
  `saferizer/` (the undo-group guard and the typed tag sink legitimately hold their
  `#[allow]` elsewhere). Adding a **new** allow site means amending the ban's reason
  in `clippy.toml` in the same change, so the exception is declared where the rule
  is, not buried at the call site. An undeclared allow silently demotes the
  strongest enforcement tier back to prose, which is the failure the seams exist to
  end.
- **No promotion without its enforcement mechanism in the same change.** Decide it
  when the seam lands: a `clippy.toml` ban, encapsulation (a newtype whose only
  constructor is the safe path), or — where neither fits — an explicit note that the
  seam is convention-only. A seam whose mechanism is "remember to call it" is a
  latent regression, not a fix.
- **Ban a raw method only when the ban's true-positive rate justifies it.** A ban
  that fires mostly on legitimate calls trains everyone to reach for `#[allow]`, and
  costs more than it saves; prefer encapsulation there. Ban paths use the **crate
  name** (`gtk4::prelude::…`), never the local `gtk::` alias — an aliased path
  silently never fires, so the ban fails open and looks installed.
- **Choose the seam's shape per seam; there are two correct answers.** *Total with a
  safe fallback* when a coarser but correct answer exists (it removes a branch from
  every call site, which is itself a prevention measure); *`Option` with forced
  handling* when there is no meaningful fallback and a wrong answer is worse than no
  answer. Neither is the house style.
- **Prove the wrapped call, not just the type.** The type is the easy half. Each
  promotion carries the verification the raw call would have needed: a `#[gtktest::test]`
  where possible, a driven Xvfb check for anything allocation/geometry/focus-timing
  dependent, and the operator's real session only where a compositor is genuinely
  required (seat grabs, autohide popovers). **Mutation-test the guard** — a test that
  pumps to full allocation before exercising the code will pass even when the seam is
  broken, so neuter the guard and confirm the test fails.
- **Link the seam back to its lesson.** A promoted contract's ANTI-PATTERNS entry
  gains a one-line "now enforced by `<type>` (`<mechanism>`)" pointer, so prose and
  type cannot drift apart. If that pointer becomes the entry's only remaining
  content, delete the entry and let the seam's rustdoc carry it — the register
  shrinking is the point, not a side effect.
- **Do not force the resistant ones.** Contracts that are runtime state-machine
  invariants — main-loop and signal ordering, allocation-readiness finality,
  adjustment settling — are not reachable by types at proportionate cost. They stay
  prose in ANTI-PATTERNS.md deliberately: the assessed-and-rejected set is ScrAP-109
  (anchored-child allocation readiness), ScrAP-13 (adjustment settling — `page_size > 0`
  is a legitimate *gate* but no principled "authoritative now" signal exists), ScrAP-53,
  ScrAP-107, and ScrAP-108's runtime semantics. Attempting to encode them is churn, and a seam
  that merely *looks* like a guarantee is worse than the prose it replaced. This is a
  ceiling of the binding, not of effort — gtk4-rs #819 records it, and no ecosystem
  binding has cleared it except Qt, whose meta-object system solves it structurally
  rather than by types.
  **One of those rejections has since been partly overturned, and the way it fell is
  the reusable part.** ScrAP-13's "no principled *authoritative now* signal exists" is
  now false for one subset — `GtkTextView` line-height validation — because the toolkit
  schedules its own deferred work at a *published priority*, so a source below that
  priority is a precise "it has finished" event (`farscroll::after_line_heights_validated`).
  Adjustment settling in general remains rejected; only this subset moved. The general
  lesson for a future assessment: before recording a runtime invariant as unreachable,
  check whether the toolkit's own deferred work sits at a priority you can order against
  — the main loop is an observable the type system is not.

## Change accountability matrices (CAM)

The CAMs — completeness checklists for whole *categories* of change (command
surfaces, markup/rendering features, derived views, reading-position preservation,
state that points into the document, and work whose completion lands later) — live in their own document, [`CAM.md`](CAM.md), because they are long and
consulted as a unit.

**Why they matter:** a CAM catches the *latent* gaps a change leaves when the
happy path works but a surface, context, or mirror is silently missed — a command
that works from the menu but was never added to the formatting overlay or given an
accelerator; markup that renders in the body but not inside a table cell; a
derived view (outline, title, status bar) that agrees with the document only until
the next tab switch. None of these fail loudly; they ship looking fine and surface
as bug reports later. The matrix is the checklist that forces every such cell to
be considered up front.

**When to read it:** before *and while* implementing any change that adds or
alters a command, a markup/rendering feature, a surface that mirrors document
state, that holds an offset or index into the document across time, that
perturbs a text pane's geometry or buffer, or that reads or writes a document
(whose completion therefore lands later) — a change in a CAM's
category must **account for every applicable cell** in
that matrix, and derive its `tests/MANUAL-TEST.md` checks from the cells (build
pipeline step 7). Operator-granted exceptions are recorded in `CAM.md` too.

## Cross-machine seat branches

The platform seats' `origin` **is the Linux seat's working clone**, not a neutral server. A
seat's push therefore writes directly into the integration machine's repository, and that
topology has one sharp edge worth stating before it is discovered.

**Push a seat branch with an explicit refspec** — `git push origin
mac/feature:refs/heads/mac/feature` — and set tracking deliberately afterwards. Do **not**
rely on a bare `git push -u origin <branch>`.

The reason is not style. A branch created with `git checkout -b <name> origin/<shared>` has its
upstream set to the **shared** branch, and under `push.default = tracking` a bare push sends
the current branch to *its upstream's name* — aiming a seat branch straight at the shared
integration branch. **Measured, and it was one fast-forward away from succeeding silently**: the
push was rejected only because the integration branch had moved on. Had it been fast-forwardable
the seat's work would have been written onto the shared branch with no error, and nobody would
have learned of it until something looked wrong downstream. This is a property of the topology,
not of any seat's carelessness, and the failure mode is a **silent write to a shared branch
rather than an error** — which is why it belongs here rather than in a seat's habits.

Corollaries already paid for elsewhere in this document's registers: **verify integration by
diffing the trees, not by trusting a record of what was picked** (`git diff HEAD <seat-branch>
-- <paths>` before deleting a branch), because a seat that is still working moves the target
under a confirmation that was accurate when it was sent.

## One commit per batch

**A batch of debt-register work lands as exactly one commit on the integration branch, or it
has not landed.** A batch is a set of issues sharing a mechanism, a fixture or a verification
rig — they are fixed together or not at all — so develop on a `feature/<batch>` branch with as
many working commits as the work wants, then land with `git merge --squash` and one commit
whose message names the batch and enumerates what it closes. Never fast-forward a batch
branch, and never cherry-pick half of one.

The reason is what a *published* revision is for. Splitting a batch into several commits
publishes intermediate states in which the mechanism is half-replaced — the old hand-off gone
and the new one not yet read by every consumer — and those are exactly the revisions a
`git bisect` lands on and a peer seat fetches mid-flight. One commit per batch keeps every
revision on the branch a state the whole batch's tests describe.

**A group of issues that merely shares a SUBSYSTEM is not a batch.** The test is a shared
mechanism, fixture or verification rig; two issues in one file that need two different
mechanisms are two batches. Stated because the subsystem reading is the tempting one and it
produces batches that cannot land as one commit.

**A batch that will not fit one commit is not one batch** — that is the signal to re-cut it,
not to relax the rule. Two mechanisms wearing one batch's name is the thing this prevents.
Seat branches (§ Cross-machine seat branches) are unaffected: a seat's branch is integrated
into the batch branch and the squash carries it, so its work reaches the integration branch
inside the batch's single commit rather than beside it.

## SDD register writes

**Route the lesson before you write it, and there are three destinations.** A lesson
about **gtk4-rs itself** is woven into the `gtk4-rs` skill and stubbed here citing
`GTK4Rs/AP-N`. A **general engineering-discipline** lesson — verification and gate
design, experiment method, claims and relay hygiene, cross-platform toolchain hazards,
trust-boundary design — goes to the **`general-engineering-principles`** skill and is
stubbed citing `GEP-N`; send the content to the `gep` member in the `skills` ToasterTalk
room, which allocates the number. **Everything else** — this project's internals and
every dependency that is not gtk4-rs — is a full entry in `sdd/ANTI-PATTERNS.md`.

The routing decision is made **at minting time**, not deferred to a later migration: an
entry written full and moved later costs the rewrite twice, and in practice is never
moved. This rule is stated here, in the prescriptive document, because it previously
lived *only* inside the register it governs — so an agent about to file an entry read
the register's own note, and when that note fell behind the practice (claiming the
general-lesson destination was undecided while 59 entries already cited `GEP-N`), nine
consecutive general lessons were filed as project entries with nothing to catch it.
A rule that lives only in the artefact it governs is one nobody consults before acting.

`sdd/ANTI-PATTERNS.md` and `sdd/ISSUES.md` have **one writer**. When work is split
across machines, every other seat sends **entry content** — symptom, root cause,
what was tried, the corrective, and its citations — and the owning seat allocates
the number or letter and lands it. A seat that is not the writer does not edit
either file, does not pick an ID, and does not cite an ID it has not been given.

This is not a courtesy about style; it is the only mechanism that prevents a
lost-update race. Two seats reading a register's tail and appending both pick the
same next ID, and the registers are append-mostly permanent files, so the
collision surfaces as a merge conflict in the one place a conflict is most
expensive to resolve correctly.

**The measured justification, recorded because a working protocol produces no
evidence of itself:** the campaign before this rule existed had **three** ID
collisions (`ScrAP-205`, then `ScrAP-211` and an `ISSUES` letter concurrently).
Every one was caught by a human or by a merge conflict; **none** was caught by a
gate, and no gate can catch them — a duplicate-ID check would only *detect* the
race after both writers had already done the work. The campaign after it had
none. Anyone considering relaxing this will see a register that has never
collided and no check enforcing it; that appearance is the protocol working, and
the three-collision rate above is the only defence it will ever have.

Content authored elsewhere keeps its author's reasoning and its
MEASURED/INFERRED labelling; the allocating seat edits for the register's format,
not for its argument, and says so in the entry.

**Cite an entry by READING it, not from a remembered gloss of it.** `lint-references` check 2
proves a cited `ScrAP-N` exists and never that it is the right one, so the whole weight of
correctness sits on the author — and the way it fails in practice is specific enough to name:
a seat cites a number it *does* hold, for a claim the entry does not make, because it is
working from a note *about* the entry rather than from the entry. MEASURED: a seat cited
`ScrAP-157` for "a guard that is inert on this platform", which is an observation recorded in
that seat's own working memory *about* the entry; the entry itself is a `GtkTreeListModel`
collapse defect and makes no such claim. The gate passed, as it must. This is the documentary
twin of trusting an instrument's reading without checking what it measured — and it is one
step past the rule below, which covers a number a seat has *not* been given.

**A number a seat has been TOLD is not a number that seat can CITE.** The writing
seat allocates an ID in its own clone, so until that clone's register reaches the
other seat, an entry cited there resolves to nothing — and `lint-references` check
2 fails, correctly, on a citation whose body is not in the tree. So an instruction
to cite a freshly allocated `ScrAP-N` is implicitly an instruction to wait for it:
the citing seat leaves a one-line note at the site and adds the citation once the
register lands, rather than committing a forward reference that breaks its own
gate. Measured, not anticipated — the macOS seat hit exactly this and the gate
caught it, which is the gate working, not friction to route around.

## Prohibited actions

- **Never run `git checkout` against a FILE or a path.** Not to revert a mutation, not
  to undo an experiment, not to "clean up" a scratch edit. It overwrites the working tree
  from the index and there is **no way back**: no reflog entry, no stash, no dangling
  object, nothing. Every other destructive git operation this project might reach for
  leaves a recovery path; this one does not.
  **MEASURED 2026-08-28**, and it cost real work: a seat mid-way through a multi-file
  change used `git checkout src/tags.rs` to back out a one-line *mutation test*, and
  discarded the entire uncommitted implementation in that file along with it — twice, in
  two files, before noticing. The mutation run that followed then reported the guard as
  PASSING, because what it was actually exercising was the reverted build (the ScrAP-239
  family: a control run silently driving the wrong artefact). The work was only
  recoverable because the patches happened to still be readable in that session's
  transcript, which is luck, not a procedure.
  **Instead: copy the file aside first** (`cp src/x.rs /tmp/x.rs.good`) and restore with
  `cp`. It is one extra command, it is reversible, and it does not care whether the file
  had other uncommitted work in it. The same goes for reverting a whole experiment: prefer
  a copy or an explicit reverse edit over asking git to erase a file you have not committed.
- Do not use `sudo` in build, test, or run commands — the agent cannot enter a
  password and the command will hang. If a system development library is
  missing, ask the human to install it (e.g. via a `!`-prefixed session command).
- Do not block the GTK main thread with synchronous file I/O on large files;
  keep the window responsive (TDD §1.4).

## Logging

Use the Rust `log` facade as the single sink, rendered by `env_logger` and
controlled at runtime by `RUST_LOG`. glib/GTK's own diagnostics are bridged into
the same sink by `logging::init()` (a glib *writer* function), so application logs
and GTK/glib messages flow through one configurable place. `logging::init()` runs
first in `main()`. Rationale: `RUST_LOG` gives per-module, per-level filtering that
`G_MESSAGES_DEBUG` cannot, and keeps call sites GTK-agnostic — so instrumentation
stays in the code permanently, gated by level, instead of being added and ripped
out. Do **not** use the glib-native `g_*!` macros for app logging, and never
register `glib::GlibLogger` alongside the writer bridge (opposite direction →
stack overflow; see ScrAP-18).

Call sites use the plain `log` macros; let `target` default to the module path
(free per-module filtering), or pass an explicit subsystem target for hot paths.

- `log::error!` — operation failed but the app **continues** (non-fatal): save
  failed, parse error surfaced to the user.
- `log::warn!` — recoverable anomaly: file not found on reload, conflict detected,
  watcher error, fell back to a default.
- `log::info!` — lifecycle events: file opened, reloaded, saved, window realized.
  **`info` is the forensic threshold**: every `info`-or-worse record is written to
  the persistent log in the state directory *and* into the breadcrumb ring a crash
  report dumps, regardless of `RUST_LOG` (TECH.md § Diagnostics and crash forensics).
  Two rules follow, and both are binding:
  - **A lifecycle boundary logs at `info`, once, at its choke point.** Open, save,
    reload, file-monitor event, tab close, window close-request, native dialog
    shown/answered, session restore. A boundary that logs at `debug` is invisible in
    a crash report — which is the case the record exists for.
  - **Never log document content.** Buffer text, selections, clipboard contents and
    excerpts of any of them are out, at every level: these records persist to disk
    and are handed to whoever is debugging a crash. Log the path, the byte count,
    the tab id, the decision — never the bytes (TDD 21.10).
- `log::debug!` — per-operation developer detail: render triggered, mode switch,
  signal wiring, conflict policy evaluated.
- `log::trace!` — ultra-verbose / hot paths (per-frame, per-event). Give these an
  explicit subsystem target (e.g. `target: "scribobulate::scroll"`) so they can be
  toggled independently and stay off by default.

Fatal conditions are handled in Rust, not via glib's aborting `g_error!`: use
`panic!`/`expect` for programmer errors and invariant violations (unwinds, runs
`Drop`, honours `RUST_BACKTRACE`), or `std::process::exit` for a controlled
shutdown after logging an `error!`. **Do not set `panic = "abort"`** — the panic
hook that writes a crash report relies on unwinding, and `Cargo.toml` records the
decision explicitly. Reserve `[🐛DEBUG]`-prefixed `eprintln!` for
genuinely throwaway debugging, and delete those before committing — permanent
instrumentation goes through `log` (the level and target already convey origin and
severity, so permanent logs carry no `[🐛DEBUG]` prefix).

Runtime control (one knob, app + GTK):

```text
RUST_LOG=warn                              # default
RUST_LOG=info,scribobulate=debug           # app at debug, GTK at info
RUST_LOG=warn,scribobulate::scroll=trace   # just the scroll hot path
```

CI: run the test binary with `G_DEBUG=fatal-criticals` so any `Gtk-CRITICAL`
becomes a hard failure instead of a silent log line. **This is armed on Linux, in
build-pipeline step 5's contract line — and it was a dead letter until 2026-08-21.**
The prescription had stood for a long time with **nothing in the toolchain setting the
flag**, which is the failure this document keeps recording in other people's code: a gate
that is never armed cannot fail, and looks identical to one that passes. When it was
finally run, the suite was carrying three latent criticals while reporting `ok` — the
worst of them a `change_action_state` on a **stateless** action, silently no-opping a
guard's setup, inside the very test that cites ScrAP-209 for that species. (The guard's
*reach* was never affected — that part of the story was inferred rather than measured, and
ScrAP-277 records the correction.) **Do not prescribe a flag without wiring it to a
runner.** Arming it also cost one test a one-line change, and the change made that
test STRONGER, not narrower: `saferizer::popover_anchor`'s round-trip read an unset anchor
on an **unparented** popover, which drives GTK's own fallback through the bounds of a NULL
parent and fires `GTK_IS_WIDGET` by design — benign noise, fatal under the flag. Parenting
the popover moves the fallback from a zeroed rect to the parent's own bounds, so the seam
is now shown discarding a *plausible-looking* rectangle rather than an obviously empty one.
ScrAP-277 records the run that found both this and the `change_action_state` defect, and
corrects its own first reading of the trade. Windows carries it too, having confirmed its own suite clean **and mutation-tested the
gate** — reverting one of the three fixes above makes the suite pass with the flag off and
die with it on, which is the only evidence that distinguishes an armed gate from a quiet
one.

**LINUX IS NOW MUTATION-TESTED TOO, and it was not when this paragraph first claimed the
flag was armed here.** That gap is worth naming rather than quietly closing: this document
states mutation testing to be the only admissible evidence that a gate can fail, Windows
supplied it, and the CANONICAL platform did not — so the strongest claim in this section
rested on the platform with the least evidence behind it. MEASURED 2026-08-21, on the
`a11y` walk's setup: reverting `activate_action(&window, "find-replace", None)` to the
`change_action_state` call it replaced gives `test result: ok. 13 passed` with the flag
off, and `signal: 5, SIGTRAP` with `G_DEBUG=fatal-criticals` set. Restored, the same suite
passes under the flag. Passing, dying, and passing again is the three-state evidence; a
green run alone would have been the thing this paragraph warns about. Note the **death code differs by platform**: a promoted critical is `SIGTRAP` (exit
133) on Linux and `0xC0000409 STATUS_STACK_BUFFER_OVERRUN` under MSVC, where the harness
reports only "test exited abnormally".

**macOS CANNOT carry it, and this is settled rather than pending.** Its suite emits
`gdk_surface_thaw_updates: assertion 'surface->update_freeze_count > 0' failed` — nine
occurrences across eight tests, every one opening or dismissing a popover or annotation
card. The cause is upstream in GDK's macOS backend (one freeze per surface *construction*,
one thaw per *map*, and `gdk_macos_surface_hide` never freezes, so the second map thaws at
zero), it is present from 4.22.4 through `main`, and `gdk_surface_freeze_updates` /
`thaw_updates` are private and absent from the `gdk4` bindings, so **no application-side
fix exists**.

⛔ **Do NOT arm it there by masking `Gdk-CRITICAL`.** That mask would also silence a
*latent* defect in the same file at the version macOS ships — a leaked surface
update-freeze that stops a surface repainting, ScrAP-318 — which is the one signal anybody
would get if that ever happened.
⛔ **And do NOT reach for a `g_log_set_writer_func` allowlist without reading ScrAP-268
first.** Installing a writer replaces `g_log_writer_default`, which is *where GLib
consults `g_log_always_fatal`* for structured logs — so the naive allowlist **disarms the
gate entirely and silently**, and a disarmed gate is indistinguishable from a passing one.
Any such writer must delegate to `g_log_writer_default` for everything it does not
allowlist, and must be **mutation-tested** by planting a second, different critical and
confirming the suite still fails on it. **The test binary, and only
it** — the flag is a no-op against the *running application*, MEASURED (GLib 2.72.4):
for structured logs, which GTK's own diagnostics are, GLib consults
`g_log_always_fatal` inside `g_log_writer_default`, and `logging::init` replaces that
writer with the bridge — so a promoted `Gtk-WARNING`/`Gtk-CRITICAL` is recorded and
survived. This holds for every promotion route, not just this one: `fatal-warnings`,
`fatal-criticals` and a programmatic `g_log_set_always_fatal` are all defused the same
way, so there is no variant of the flag that arms the app. The suite runners install no bridge (`gtk_suite.rs` says why), which is what
keeps the line above true where it is claimed. `g_error` is unaffected either way: its
fatality is decided after the writer returns, and it kills the process with `SIGTRAP`
rather than `abort()` (ScrAP-268).

Log messages must be self-contained — an agent reading them should understand
what the application was doing without needing to cross-reference source code.
Include the file path, event type, and relevant state in every message where
applicable.

Temporary debug output added during development must be prefixed with
`[🐛DEBUG]` and removed before committing. Use `eprintln!` for ad-hoc output
when the GLib log machinery is not yet initialised (e.g., before `Application`
is built).
