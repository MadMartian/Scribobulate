# Plan: Consistent build pipelines and installers across Linux, macOS and Windows

## Problem

The three platforms do not have comparable build pipelines, and only one of them
can hand a working application to somebody who does not already have a Rust
toolchain. Measured against the tree at `671040a`:

| | Linux | macOS | Windows |
|---|---|---|---|
| Executable gated pipeline | **none** — prose steps in POLICY, run by hand | **none** | `packaging/windows/pipeline.ps1`, 9 gated steps |
| Package/bundle step | none | `packaging/macos/bundle.sh` → `Scribobulate.app` | `stage.ps1` → redistributable tree |
| Installer artifact | none — `install.sh` builds **from source** | **none** — a `.app` with no disk image | `scribobulate.iss` → `Scribobulate-<ver>-x64-setup.exe` |
| Uninstall | `uninstall.sh` | none | provided by Inno Setup |
| Can a non-developer install it? | **no** | **no** — nothing to hand them | **yes** |

Three consequences, in increasing order of cost:

1. **Only Windows can be released.** `install.sh` runs `cargo build --release`
   and installs into `~/.local`, so it is a *developer* convenience, not a
   distribution: it requires the Rust toolchain and the GTK development
   packages. macOS produces a bundle but no `.dmg`, so there is no artifact to
   transfer. Linux produces nothing packaged at all, despite already carrying
   the desktop-integration inputs (`data/scribobulate.desktop`, the hicolor
   icon, `data/themes.toml`).

2. **The gates are not equally enforceable.** POLICY's build pipeline is nine
   numbered steps. On Windows they are a script that fails the run; on Linux
   and macOS they are prose that a person is trusted to have run in order. A
   prose pipeline cannot be verified, cannot be run in CI, and cannot report —
   which is not hypothetical: `pipeline.ps1` step 4b surfaces tests that printed
   `SKIPPED [...]`, because libtest captures a *passing* test's output and
   silences a body announcing "I verified nothing" exactly when it passes.
   **Linux and macOS have nowhere for that report to go**, so a skipped test is
   invisible on two of three platforms. Today that is masked, because the tests
   in question run rather than skip on Linux — "invisible because currently
   inapplicable", which is the shape this project has now recorded four times
   (ScrAP-206, 209, 211, 212).

3. **Alignment will be claimed and will not exist.** This is the expensive one,
   and it is not speculative: it has already happened in this repository. POLICY
   asserted that the two `lint-references` ports "share one pattern and one
   self-test corpus, string-for-string, so neither can drift into being the
   lenient one". The claim was false when written — the pattern was pinned and
   the *file set* was not (ScrAP-207). Three pipelines authored independently
   on three machines, none of which can execute the other two, is that failure
   with two more copies.

### Root cause

There is no artefact that says what a build pipeline *is* for this project.

POLICY §Build pipeline is the closest thing, but it is written as instructions
to a reader, not as a contract a program can consume. So each platform's
pipeline — where one exists — encodes its own reading of it. `pipeline.ps1`
numbers its steps 1-5, 9, plus unnumbered staging and installer steps, and
inserts a 4b that POLICY does not mention at all. Nothing detects that
divergence, because nothing has an opinion about what the step list should be.

The same defect one level down produced R2-01 and R2-02 this round: a contract
that pinned *some* of what a gate is (the pattern, the directory roots) and not
the rest (the file-type classification, the depth bound), so the two ports
agreed where they had been pinned and diverged everywhere else. The lesson
recorded then applies verbatim here: **a gate is its pattern *and* the set it
runs over**, and by extension a pipeline is its step list *and* the verdict
rule for each step.

## Previously attempted

Nothing has been shelved; this is a gap rather than a retreat. But three pieces
of relevant history should not be re-derived:

- **`build.bat` exists because MSYS2's GTK is not discoverable by default.** It
  sets four environment variables (`PKG_CONFIG_PATH` and friends) without which
  `cargo build` cannot find pkg-config and `cargo run` cannot find the GTK DLLs
  — the app builds and then fails to start. `pipeline.ps1` sets the same four.
  Any Windows pipeline work must preserve that, and any *shared* pipeline design
  must accommodate a platform needing environment setup before step 1.

- **macOS cannot be cross-compiled from Linux.** Verified during QA round 1:
  installing the `x86_64-apple-darwin` target and running `cargo check --target`
  still fails inside `gtk4-sys` on pkg-config cross-compilation. The target was
  removed again. No seat but the macOS one can produce or verify a macOS
  artifact, which is a hard constraint on how this plan can be validated.

- **`bundle.sh` ad-hoc codesigns and treats failure as fatal**, with the reason
  recorded at the site: a bundle whose signature `codesign` refused to write is
  not "slightly worse", it will likely refuse to launch. Any packaging step that
  wraps it must not soften that into a warning.

## Possible approaches

### 1. Three independent pipelines, aligned by review

Each platform writes its own script. Alignment is maintained by the three agents
reading each other's work and by POLICY describing the intended steps.

**Pros**: no new machinery; each script is idiomatic for its platform; fastest
to land.
**Cons**: this is the status quo's failure mode with more surface area. It is
precisely what produced ScrAP-207 — reviewers diff the parts that *look* like
the rule and never the enumeration. No mechanism detects divergence, and two of
the three reviewers cannot execute what they are reviewing.

### 2. One cross-platform pipeline in a portable runner

Replace all three with a single implementation — a `cargo xtask`, or `just`, or
a Rust binary in the workspace — that runs everywhere and branches internally on
target OS.

**Pros**: one step list by construction; no parity problem because there is only
one artefact; testable on any platform for the parts that are not
platform-specific.
**Cons**: introduces a dependency and a build-before-you-can-build step; the
platform-specific halves (MSYS2 environment, `codesign`, Inno Setup) still
cannot be exercised except on their own platform, so the *hard* part of the
parity problem survives inside the branches. It also discards `pipeline.ps1`,
which is working, gated, and now carries several hard-won corrections.

### 3. A shared step contract, three thin ports (mirrors `lint-references.scan`)

Define the pipeline as data in one file that all three implementations *read*
rather than restate: the ordered step list, each step's command per platform,
its verdict rule (exit code vs. output marker), and whether it is required or
informational. Each platform keeps its own thin runner. Add a `--list-steps`
mode that prints the resolved step list, one per line, so the three can be
diffed against each other.

**Pros**: the invariant is pinned where it has proven to matter, and the
comparison artefact exists — this is the design that actually caught a real
divergence this round (`--list-scan` found a 231-vs-232 mismatch on
byte-identical trees). Keeps each platform's runner idiomatic. Incremental:
`pipeline.ps1` becomes a consumer rather than being rewritten.
**Cons**: a third artefact that can itself drift from its consumers if nothing
proves they honour it — which is exactly what `--list-scan` did for a long time
(ScrAP-207), one level down. The contract must therefore ship with the parity
artefact from the start, not as a follow-up, and with a self-test asserting the
artefact prints what the consumers actually consume.

### 4. Contract + runners, with CI as the enforcement mechanism

Approach 3, plus a CI matrix (three runners) that executes each platform's
pipeline on every push and diffs the three `--list-steps` outputs as a job.

**Pros**: converts the parity claim from a promise into a build failure, which
is the only thing that has worked in this project so far. Removes the "no seat
can verify another's platform" constraint entirely — the CI runner can.
**Cons**: there is no CI configuration in this repository today (`.github/`
absent), so this is a larger change than the pipelines themselves; GTK
dependencies on three hosted runners is real work, and the macOS runner will hit
the same `codesign` and bundling questions with no interactive session to debug
them.

## Recommendation

**Approach 3 now, designed so that approach 4 is a later addition rather than a
rewrite.**

Take 3 because it is the design this project has already proven, in the same
week, against the same failure mode: the shared-contract-plus-comparison-artefact
pattern found a genuine cross-platform divergence that two independently correct
implementations had produced from the same spec. Take it *with* the comparison
artefact from day one, because the version of this that fails is the one where
the contract lands and the proof is deferred.

Reject 1 outright: it is the arrangement that produced the defect this plan
exists to prevent, and its cost is paid silently and later.

Reject 2 for now on the grounds that it deletes working, gated, recently
corrected Windows tooling to solve the *easy* half of the problem. The hard half
— platform-specific packaging that only one seat can execute — is unchanged by
it.

Defer 4 rather than rejecting it. CI is the only mechanism that would make the
parity claim self-enforcing, and every unenforced assurance in this repository
has eventually turned out to be false. But it should follow a working contract,
not precede it.

### What "consistent alignment" has to mean concretely

Since the three variants land independently on three machines, alignment cannot
mean "the scripts look alike" — that is unverifiable and is the trap. It must
mean these five things, each checkable:

1. **One ordered step list**, defined in the contract, identical on all three
   platforms. Steps that genuinely do not apply to a platform are declared
   **in the run output**, with the reason, rather than silently omitted — an
   unexplained asymmetry is what the next contributor "fixes" into a bug. "In
   the run output" is the operative part and was learned by measuring: the
   Windows pipeline declares step 6 at runtime with its reason, but declares
   steps 7 and 8 only in a *source comment*, so someone reading the run sees
   `1, 2, 3, 4, 4b, 5, 6, 9` with no indication that 7 and 8 exist. A
   declaration only a code reader can find is not a declaration to the
   pipeline's user; without this clause "declared" degrades back into
   "documented somewhere", which is the state this plan exists to leave.

   A non-applicable declaration must also record the **kind** of
   non-applicability, because the two kinds are acted on differently:

   - **permanent by nature** — true even with perfect tooling;
   - **absent tooling** — fixable by installing something.

   Step 6 (the coverage ratchet) is the worked example and shows why this
   matters. It is non-applicable on Windows for *both* reasons: semantically,
   because unix-only code shifts numerator and denominator so a Windows figure
   can legitimately fall below the floor with no regression; and incidentally,
   because `cargo llvm-cov` is not installed there. **Record the semantic
   reason and not the tooling one.** If the contract cites the tooling, the
   first person to run one `cargo install` enables a gate that produces a
   meaningless number — and a meaningless gate that runs is worse than one
   honestly declared off. This is ScrAP-210's "an unmeasured trigger propagates
   further than an unmeasured symptom" in another costume: a plausible-but-wrong
   reason attached to a correct decision is a trap for whoever revisits it.

2. **Each step's verdict rule is explicit.** Exit code, or a named output
   marker. Not "ambient preference behaviour", which differs by shell, host and
   version — the defect that made a cargo *success* line fatal on Windows.

3. **The contract pins each step's INTENT, with the command supplied per
   platform.** Do not pin commands. Measured on Windows: `xmllint`, `bash` and
   `cargo llvm-cov` are all absent from the PowerShell `PATH`, so POLICY step
   8 as written — `xmllint --noout sdd/system-overview.svg` — cannot run there.
   But the step's *intent* ("the architecture SVG parses as well-formed XML")
   is satisfiable with a built-in and no new dependency:

   ```powershell
   $doc = New-Object System.Xml.XmlDocument
   $doc.Load('sdd/system-overview.svg')     # well-formed: root <svg>, 146 children
   ```

   and it discriminates — fed `<svg><g></svg>` it rejects with a parse error,
   which was mutation-tested rather than assumed, because an XML check that
   accepts anything is worse than none. A contract of *commands* would have
   forced Windows to install `xmllint` or file a false non-applicable
   declaration, and both are wrong. A contract of *intents* makes the step
   portable today.

4. **A `--list-steps` parity artefact on every runner**, printing the
   **derived** step list one per line, and the runners must DERIVE that list
   from the contract rather than restate it. The distinction is load-bearing and
   comes from this project's own experience: `--list-scan` proved the two
   enumerations agreed *with each other*, and for a long time never proved either
   honoured `lint-references.scan` — it printed a set some checks did not even
   consume (ScrAP-207). It worked only because both ports genuinely read the
   contract, so agreement and conformance happened to coincide. Where a port restates instead of deriving, a clean diff
   proves only that two restatements match, which two people copying the same
   wrong list also achieve. **Comparison proves agreement; derivation proves
   conformance.** Prefer derivation, and fall back on comparison only where
   derivation is impossible — which is precisely the per-platform command
   bodies.

5. **Skips are surfaced identically.** The marker is pinned by the Windows
   implementation as literal `SKIPPED`, space, `[`, a **rubric** label, `]`,
   colon, space, reason — e.g. `SKIPPED [TDD 19.2 symlink]: <reason>`. Runners
   grep `SKIPPED \[` — **today that is one runner, `packaging/windows/pipeline.ps1`,
   and the plural above is the plan's intent rather than a description of what
   exists** (QA round 5, F-SEC5-002). Worth stating because of which one it is: the
   Windows runner is the platform where the emitter mostly *cannot* fire the
   interesting way, so the reader that exists is watching the case least likely to
   occur, and the cases most likely to skip have no reader at all. A convention with
   one consumer is a convention with one consumer, whatever the spec says. The label is the rubric and not the test name, so renaming
   a test does not break the grep. A skip nobody sees is a pass.

6. **Per-platform setup is a phase, not a step.** Windows needs four
   environment variables before *any* step: `PATH` must carry the GTK `bin` for
   the runtime (a built binary will not start without it), `PKG_CONFIG_PATH` for
   the build probe, and `LIB`/`INCLUDE` for the MSVC link — the last two being
   conditional concatenations, because prepending onto an unset variable leaves
   a trailing `;`, and an empty entry in those lists means "the current
   directory" to the toolchain. This produces environment, has no verdict, and
   cannot be skipped or reordered, so modelling it as "step 0" would force
   Linux and macOS to declare a non-applicable step 0 — noise that dilutes the
   declarations that carry real information. Keep it a distinct per-platform
   section, empty for Linux and macOS.

7. **Test carve-outs are contract data, per platform.** Windows' step 5 carries
   a by-name skip list (`$skippedTests`, currently empty, feeding `--skip`). The
   mechanism must survive while the list is empty — that is what lets the step
   print "no carve-outs" and have it *mean* something rather than be silence.
   Express carve-outs as data in the contract rather than letting each runner
   invent its own.

8. **Every step is verified against a real injected failure**, not a
   before/after comparison. A pipeline that buys quiet by swallowing failures
   passes before/after and is worse than the bug it replaces. Each platform's
   author must demonstrate, per step, that a genuine failure still fails the
   run — the check that caught the `ErrorActionPreference` fix not being a
   suppression.

### Installer scope, per platform

The target is: an artefact a non-developer can install, with no toolchain.

- **Windows** — already there. `stage.ps1` + `scribobulate.iss` produce
  `Scribobulate-<version>-x64-setup.exe`, version read from `Cargo.toml`. This
  is the reference implementation; the other two should match its *properties*,
  not its mechanism.
- **macOS** — `bundle.sh` already produces a signed `.app`. Missing is a
  transferable container: a `.dmg` (`hdiutil`) is the minimum, and the
  quarantine/Gatekeeper story for an ad-hoc signature must be documented
  honestly rather than assumed, since an ad-hoc-signed bundle from the internet
  will be refused by default on a machine that did not build it.
- **Linux** — nothing exists. `install.sh` is a from-source developer install
  and should stay as that, unchanged. The distributable form is a separate
  question with real trade-offs (AppImage: single file, bundles GTK, no root;
  Flatpak: sandboxed, handles the GTK runtime, needs a manifest and a portal
  story for file access; `.deb`/`.rpm`: native, but one per distribution and
  they inherit the host's GTK version). **This choice is not made in this plan**
  and should not be made by whoever writes the Linux pipeline as a side effect
  of writing it — it is a product decision about who the Linux user is.

Version is already single-sourced from `Cargo.toml` on Windows. All three
installers must read it the same way; a hand-maintained version string in a
second place is the same class of defect as everything else in this plan.

## Technical details preserved

- **The step list as it exists today**, from `pipeline.ps1` (numbering is the
  script's own): 1 `cargo fmt --check`; 2 `cargo clippy --all-targets --features
  gtk-integration-tests -- -D warnings`; 3 `cargo build --release`; 4 `cargo
  test`; 4b skip-report (informational, never fails the run); 5 GTK integration
  tests, with a carve-out; 9 static reference lint. POLICY additionally
  specifies 6 coverage gate (`scripts/coverage.sh`), 7 UI-behaviour coverage
  alignment, 8 architecture-diagram alignment (`xmllint --noout
  sdd/system-overview.svg`). Of those three, **step 6 IS declared by the Windows
  script at runtime, with its reason** ("coverage ratchet — NOT RUN
  (Linux-canonical, by design)", and a header explaining that unix-only code
  shifts both numerator and denominator, ending "Never lower the Linux floor to
  make a Windows run pass"). **Steps 7 and 8 are declared only in a source
  comment** and never reach the run output. So the POLICY-versus-script
  divergence is narrower than a step-count comparison suggests, and its
  interesting part is not "missing" but "declared where the pipeline's user
  cannot see it" — which is why property 1 above says *in the run output*.

  Corrected here after being measured on the Windows host; the first draft of
  this plan asserted all three were absent, which was wrong for 6 and half-wrong
  for 7 and 8. Recorded rather than quietly fixed, because a plan that
  overstates the divergence would have justified changes to a script that was
  already doing the right thing for step 6.

- **Linux's GTK integration tests need a display.** POLICY step 5 is
  `xvfb-run -a cargo test --features gtk-integration-tests`. Any Linux runner
  must provide that wrapper; on macOS and Windows the equivalent step runs
  against the real windowing system, which is a genuine per-platform difference
  the contract must express rather than paper over.

- **`Case`-level test attributes now differ from stock libtest behaviour.** The
  main-thread suite honours `ignored` and `should_panic` itself
  (`src/suite_registry.rs`, `src/gtk_suite.rs`); a runner that greps test output
  must account for both harnesses reporting, since the second one is
  `harness = false` and formats its own results.

- **Ad-hoc `codesign` failure is fatal by design** in `bundle.sh`. Do not
  demote it to a warning when wrapping it in a pipeline step.

- **`build.bat`'s four environment variables** are a precondition for *any*
  Windows step, not part of any single step. The contract needs a notion of
  per-platform setup that runs before step 1, or every Windows step has to
  restate it.
