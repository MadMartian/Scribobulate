# Plan: release artifacts for all three platforms, and the hosted execution that earns them

## Problem

**The deliverable is a workflow that produces an installable artefact for each operating
system when the operator publishes a release.** Today the project can build all three by
hand and builds none of them automatically, so every release is a manual pass on three
machines, one of which no contributor but the operator has.

That deliverable rests on a half-finished foundation, and this plan covers both halves
because shipping the second without the first would be untested twice over.

**The first half is done.** CI exists (`.github/workflows/pipeline.yml`): the three ports'
derived step lists and the three platforms' lint scan sets are diffed on every push, so the
parity claim is a build failure rather than an errand somebody remembers.
`scripts/pipeline-parity.sh --self-test` runs inside that job, and both it and the workflow
were shown to fail on a deliberately injected drift rather than trusted on a green run.

**The second half is execution, and it is the prerequisite.** Only Linux actually *runs* the
pipeline in CI. Windows and macOS contribute their step list and their own `--self-test` —
real evidence, and evidence about contract parsing. The distinction is not academic here:
the Windows port passed `-ListSteps` byte-identically, `-SelfTest`, and a twelve-case
mutation battery while an output-stream bug made it report `pipeline PASSED` with exit 0
after a step had failed. It then did it *again* during this work — `-ListSteps` wrote
through `[Console]::Out`, which PowerShell's `>` does not redirect, so the documented parity
procedure produced a zero-byte file at exit 0, and the port's own self-test could not see it
because it rebuilt the printed list inline instead of calling the printer (ScrAP-276).

Twice now, on the same platform, a port has been confidently wrong in a way only execution
or consumption could catch. **A packaging job on a platform whose build has never run in CI
would be that same bet a third time**, with a downloadable artefact as the stake.

**The third half is packaging**, which the previous version of this plan deliberately
excluded — correctly, at the time, because it was scoped to execution alone. Naming the
artefact as the deliverable brings it in.

### Root cause

Nothing on the Windows or macOS side of CI ever *runs* a step, so the only defects reachable
there are defects in reading the contract. A port that parses correctly and executes wrongly
is invisible, and both of this project's Windows pipeline defects have been of exactly that
shape. Nothing anywhere produces an artefact, so every property of the artefact — that it
carries the right version, that it installs, that it runs on a machine that did not build it
— is asserted by hand or not at all.

## Previously attempted

Nothing failed; this is the deliberate second half of an ordering the retired
build-pipelines plan chose on purpose — a contract first, then the machinery that enforces
it. That ordering held, and the work it enabled is recorded above.

One assumption from the first version of this plan **has since been falsified**, and it is
the most useful thing here. That version ranked the remaining work "Linux first, then
Windows, then macOS", on the grounds that macOS is hardest because of `codesign` and
bundling with no interactive session. **That reasoning is about step 10, and step 10 is
`packaging`-class: opt-in, absent from a default run.** An *execution* job that does not
pass `--package` never reaches `codesign` at all. Once packaging is set aside, the two
platforms invert for execution purposes:

- **macOS** needs `brew install gtk4 gtksourceview5 adwaita-icon-theme` — minutes, no build.
- **Windows** needs a gvsbuild GTK, which is a multi-gigabyte source build of the whole
  dependency chain.

So the ascending-difficulty order that plan recommended is, for execution, backwards. Note
what widening the scope does to that finding: it does **not** reinstate the old order. The
`codesign` difficulty returns only in the packaging jobs, which are a separate trigger and a
separate bring-up, so execution still goes macOS first.

## Possible approaches

### Trigger for the packaging jobs

**1. On published release (chosen).** The operator cuts a release when satisfied; the
workflow builds the three artifacts and attaches them to it. Matches how releases actually
get decided, and spends no runner time on commits nobody is shipping.
**Pros**: zero waste, and the artefact set is exactly the release's contents by construction.
**Cons**: the artifacts are built from the tag *after* the decision to ship, so a packaging
failure is discovered at the worst moment. Mitigated by `workflow_dispatch` below.

**2. On every push.** **Rejected**: the Windows gvsbuild dominates wall-clock and nothing
downstream consumes the output.

**3. On tag push.** Nearly the same as (1), but fires on tags that are not releases and has
no release to attach to. Rejected as strictly worse for this operator's workflow.

**`workflow_dispatch` is retained alongside (1) permanently, not as a bring-up crutch.** A
CI gate is trusted only once it has been shown to fail, and that obligation cannot be met by
cutting throwaway public releases. Dispatch is how the packaging jobs get demonstrated,
before first use and after any change to them.

### Signing

**1. Ship unsigned / ad-hoc (chosen for now).** No certificates exist. macOS gets ad-hoc
signing from `bundle.sh`, which Gatekeeper refuses on any machine that did not build it;
Windows gets a SmartScreen warning.
**Pros**: available today, costs nothing.
**Cons**: **step 10's declared intent is not fully met on macOS** — see Technical details.

**2. Apple Developer Program + Azure Trusted Signing.** $99/year and ~$10/month
respectively; Apple individual enrollment is quick, Azure needs an org with 3+ years of
legal existence. **Deferred, not rejected** — the workflow is built so adding signing is a
step inserted into an existing job rather than a redesign.

## Recommendation

**Execution first (macOS, then Windows), then packaging for all three**, in that order,
because each stage retires the risk the next one would otherwise carry blind.

1. `execute-macos`, brought up on `workflow_dispatch`, then moved to the push trigger.
2. `execute-windows`, same bring-up, with a pinned and cached gvsbuild prefix.
3. `release` jobs for all three platforms, triggered by a published release plus dispatch,
   uploading each artefact to the release.

Three rules carry over from the work already done and are not optional:

- **The job invokes the platform's runner and names no step.** Provisioning is the only
  thing an execution or packaging job may contain. A workflow that lists steps is a fourth
  restatement of a contract whose design is derivation (ScrAP-207). The packaging jobs
  invoke the runner with the flag that turns step 10 on; they do not call `dmg.sh` or
  `build-deb.sh` themselves.
- **Show it failing before trusting it.** Every gate added here is demonstrated on an
  injected failure, the way the parity job was. For the packaging jobs this includes the
  vacuous pass their shape invites: a job that uploads nothing must not report success.
- **The artefact is verified as an artefact, not as an exit code.** A packaging step that
  exits 0 having produced a zero-byte file is precisely the defect class this project has
  already shipped twice (ScrAP-276). Each job asserts its output exists, is non-trivial in
  size, and carries the version from `Cargo.toml`.

## Technical details preserved

- **Step 10's intent overstates what an unsigned build delivers.** The contract says "a
  transferable installer artefact a non-developer can install with no toolchain". On macOS,
  an ad-hoc-signed `.dmg` fails that on any machine but the builder's: `dmg.sh` says so in
  its own header. The caveat belongs in the contract beside the step, so the gap is recorded
  where the claim is made rather than discovered by a user. Do not silently narrow the
  intent — the intent is right, the current artefact just does not reach it on one platform.
- **Step 6 (coverage) is contract-declared non-applicable on both new platforms**, for
  semantic reasons that have nothing to do with tooling — so an execution job there runs
  steps 1–5, 8 and 9, and announces 6, 7 and 10 rather than omitting them.
- **A hosted runner is a different MACHINE, not just a different platform.** Linux execution
  landed with a 0.05pt coverage gap traced entirely to `src/config.rs` reading the ambient
  config directory. The floor is now a whole number advancing a whole point at a time, which
  is too coarse for a gap of that size to move (ScrAP-123's extension; POLICY step 6 states
  the rule, `scripts/coverage.sh` holds the value). Expect the same class of finding on each
  new platform, and read it as a defect in the test's environment-independence before
  reading it as a coverage regression — but do not expect it to threaten the gate, and do
  not report sub-point movement as a result.
- **`G_DEBUG=fatal-criticals` must not be set process-wide** over the suite — the workflow
  records why beside the job. A new execution job inherits that decision rather than
  re-litigating it (ScrAP-277).
- **Windows provisioning, already derived once so it need not be again** (the detail lives
  in `packaging/windows/README.md`, which is the authority): cache only the gvsbuild
  *install prefix* (~144 MB), never `C:\gtk-build` (~6.5 GB of sources and intermediates
  that nothing downstream reads); and enable long paths **for git only** (`git config
  --system core.longpaths true`), never the system-wide `LongPathsEnabled` registry key —
  leaving that at its default is what makes a runner exercise the constrained `MAX_PATH`
  case the dev box cannot. Pin the gvsbuild version; an unpinned install silently changes
  which GTK you are testing against.
- **Windows packaging additionally needs Inno Setup 6.** `package.ps1` probes for `ISCC.exe`
  on PATH, then the winget user-scope location, then machine scope, and *fails* rather than
  warns when absent. `winget install --id JRSoftware.InnoSetup` is the documented install.
- **macOS provisioning**: Homebrew `gtk4` does **not** pull in an icon theme —
  `adwaita-icon-theme` is explicit, or roughly half the toolbar renders broken-image
  placeholders. The contract's `cmd.macos integration` deliberately avoids the `--lib`
  target, whose dual-harness bodies abort off the main thread (ScrAP-171), and runs the
  main-thread suite plus the three standalone targets instead.
- **macOS packaging runs `bundle.sh` before `dmg.sh`**, and the `.app` is what carries the
  Dock and Cmd-Tab identity. The version for the `.dmg` is read back out of the built app's
  `CFBundleShortVersionString` rather than re-parsed from `Cargo.toml`, which is the
  contract's rule for that platform.
- **GUI tests on a hosted macOS runner are the remaining unknown.** Step 5 opens real
  windows and the runner's session is not a logged-in desktop. This is the one place the
  execution work may hit something that is neither a port defect nor a provisioning gap, and
  it is why macOS execution is brought up on dispatch first.
- **The repository is public**, so macOS and Windows runner minutes are free. Cost is not an
  argument against any job here; wall-clock on the Windows gvsbuild still is.
- **A workflow file is not scoped by the branch it sits on.** Adding a job is a
  project-level change that takes effect for everyone on merge, which is why the existing
  workflow's triggers are documented in its own header rather than treated as a job detail.

## SESSION HANDOFF — 2026-08-14, read this first

**Branch state.** `ci` was **squashed from 22 commits to one** (`8e45913`) so it rebases onto
`master` in a single conflict pass rather than twenty-two; the two branches overlap on thirteen
files, nearly all append-mostly registers. **The pre-squash history is preserved at tag
`pre-squash-ci-20260814` → `c2f52b3`, which exists both here and on `github`.** Do not delete
it until `ci` has merged. `github/ci` matches local `ci`.

**Other seats.** Four seats worked this session: `linux` (this one, on `ci`), `windows`, `mac`,
and `linux-master` (on `master`). Their `origin` **is this repository**, so a commit here is
immediately visible to them and no GitHub round-trip is needed to collaborate — GitHub carries
CI and durability only. All were told to `git fetch && git reset --hard origin/ci` rather than
pull, because the squash left no common recent ancestor.

**Register allocation is by conversation, not by reading the tail** — see POLICY § SDD register
writes. `master` currently holds `ScrAP-283`, **`ScrAP-284`**, `ISSUES S` and `ISSUES T`; `ci`
holds up to `ScrAP-282` and `ISSUES R`. **`master`'s registers look complete and are behind**;
that misread happened three times this session.

> **2026-08-14, later — `ScrAP-284` is TAKEN on `master`; do not allocate it from `ci`.** The
> `linux-master` seat allocated it (drag-and-drop: per-platform advertise sets) and also extended
> `ScrAP-283` and rescoped the drag-and-drop `ISSUES` entry. Now committed at `master ebcfdd4`,
> with `sdd/scrap-numbers.manifest` ending `…283, 284` — verified from `ci`, not taken on report.
> `ci`'s own manifest still ends at `282` and is correct for `ci`. The next `ScrAP` free for a
> `ci` allocation is therefore **285**, and it must still be claimed by conversation, not by
> reading either tail. Note `ebcfdd4` is **local to the master clone and unpushed**, so it is
> reachable from this repository but not from `github`.
>
> **Why this note was written before that commit existed, which is the part worth keeping:** for
> about forty minutes the allocation lived *only* in an uncommitted worktree and in a chat message
> with a ~660s TTL. Every artefact reachable from `ci` — `git log master`, `master`'s manifest —
> still said `283` was the high-water mark, so a `ci` seat doing the diligent thing would have
> been handed `284` as free. **An allocation held only in a working tree is invisible to every
> other seat until it commits**, so POLICY's "allocation is by conversation" is load-bearing for
> longer than it looks, and the conversation can expire. That is the general hazard; the specific
> `284` window is closed.

### Open, and none of it is blocked on a seat

1. **`windows/bootstrap-vcredist`** (Windows seat) implements the ruled `msvc-runtime` change
   and is **not landed**, pending two operator decisions: whether the **unverified end-to-end
   pair** blocks landing (that seat has no clean machine — every box that can build this
   already has the MSVC runtime, so removing the DLLs and launching proves nothing), and
   whether **+23.7 MB** of embedded redistributable is acceptable versus downloading it and
   requiring a network.
2. **The coverage floor is a two-branch standoff and needs one decision, not two.** `ci`
   requires whole-number floors (POLICY step 6) and sits at a stale `76` against a measured
   77.83; `master` has no such rule and shipped `77.37`. **No single value is correct under
   both rule-sets** — under `master`'s rule, 77 is a forbidden lowering. Both seats froze
   deliberately rather than each moving toward the other. Do not let one seat resolve it alone.
3. **`GApplication::startup` CRITICAL** — root-caused to a missing `app.register()` in one test
   fixture, fix proven and landed on `master`. The transferable half (GTK's guard tests
   `is_registered` while its message names `startup`) went to the `gtk4-rs` skill, not here.

### The failure mode that cost the most time, so the next session recognises it

**An artefact answering a narrower question than the one asked of it** — six instances, four
seats, one day. Two remotes sharing the name `origin`. A ScrAP manifest ending at 268 because a
branch was behind rather than because 269 was free. Non-contiguous ISSUES letters where
"highest present" was never next-free. A `grep` scoped to one checkout, twice. A licence gate
whose string anchor discriminates documents but not *revisions* of one. And
`.claude/settings.json` answering "what does the project configure" when the question was "what
is configured". **In every case the artefact was read correctly and looked complete.**

The remedy that worked was not caution: it was **publishing the command rather than the
conclusion**, so the other seat could re-run it in one line. Several were caught by the seat
that had not made the mistake.

## Outstanding before the first release: licence attribution

**Status: BLOCKING a published release. Execution and packaging are green on all three
platforms; this is what is left.**

**There are TWO independent obligations here, and an earlier version of this section
collapsed them into one and got the scope wrong as a result.** They have different causes,
different scopes, and neither discharges the other.

### Obligation 1 — the bundled GTK runtime. Scope: Windows only.

The Windows installer bundles a complete GTK runtime — 35 binaries in `bin/`, 824 files in
`share/` — and `THIRD-PARTY-LICENSES.md` covers only syntect syntaxes and themes. It has
zero mentions of GTK, GLib, Pango, Cairo, GdkPixbuf, GtkSourceView, HarfBuzz, FreeType,
Adwaita, or the string "LGPL". So the artefact we are one click from publishing
redistributes an LGPL-family runtime and attributes none of it.

Windows-only here **is** a measured fact: Linux `Depends:` on system GTK and bundles no
runtime, and the macOS `.app` links `/opt/homebrew` directly rather than bundling
(`bundle.sh`'s own header says so, and `packaging/macos/README.md` carries the gap list).
The good news is that the gvsbuild prefix already ships upstream's own texts for 28
projects, so most of the remedy is staging a directory that exists — the binary→licence
mapping table is measured and complete.

### Obligation 2 — the statically linked syntax grammars. Scope: ALL THREE platforms.

**`THIRD-PARTY-LICENSES.md` ships in no artefact on any platform.** Measured:

| Artefact | What carries licence text |
|---|---|
| `.deb` / `.rpm` | `payload.sh` stages five things — binary, `.desktop`, icon, `themes.toml`, man page. The deb `copyright` names Apache-2.0 only; the rpm spec's `%files` lists the same five |
| `.exe` | `stage.ps1` never copies it; the Inno script sets `LicenseFile=LICENSE` (Apache-2.0, shown at install) and recurses the stage tree, which has no copy |
| `.app` / `.dmg` | `bundle.sh` has no licence handling at all |
| the release itself | `attach-installers` uploads only `*.deb *.rpm *.dmg *.exe` |

The duty is real and applies everywhere: `two-face` 0.5.1 is an ordinary dependency whose
syntect grammar assets are compiled into the binary on every platform, under MIT,
Apache-2.0, BSD-2-Clause and BSD-3-Clause — all of which require the notice to travel with a
binary distribution. The About dialog already asserts the remedy exists, shipping the line
*"Full notices: THIRD-PARTY-LICENSES.md (in the distribution)"*, and the code comment beside
it states the obligation correctly. The file is simply not in any distribution; it lives in
the repository, which is why the claim reads true from a developer tree and is false in
every artefact a user receives.

**Why the earlier scope conclusion missed it, which is the part worth keeping.** The
Windows-only finding was derived from a measurement about the **GTK runtime** — who bundles
it and who links it — and that measurement was correct. The conclusion then silently
generalised from *which platform bundles GTK* to *which platform owes attribution*. What is
bundled on all three is statically linked into the executable and therefore leaves no file
on disk to notice, so it appears in neither half of that reasoning. That is the identical
mechanism recorded two sections below for librsvg's 359-crate Rust graph — *a statically
linked dependency leaves no file behind to notice* — applied there and missed here, in the
same document. A scope claim is only as wide as the thing it was measured over; state what
was measured, not what the measurement seemed to settle.

**Remedy — DISCHARGED ON ALL THREE PLATFORMS.** Stage `THIRD-PARTY-LICENSES.md` into each artefact:
`/usr/share/doc/scribobulate/` for deb and rpm with the `copyright`/`%files` entries to match,
the stage tree root for Windows alongside the GTK licences, and `Contents/Resources/` for the
`.app`. Obligation 1's mapping table carries a row for it on the Windows side, so the two-way
gate covers it like any other staged licence file.

| Platform | Status |
|---|---|
| **Linux** | **DONE.** `payload.sh`, `install.sh`, `uninstall.sh`, `build-deb.sh` and `build-rpm.sh` all reference it |
| **Windows** | **DONE.** `stage.ps1` copies both to the stage root; verified present in an installed product at 11,061 B and 205,287 B |
| **macOS** | **DONE.** `bundle.sh` stages both into `Contents/Resources/`, beside the `.icns`. Verified in the **mounted `.dmg`**, not the build tree: `LICENSE` 10,865 B, `THIRD-PARTY-LICENSES.md` 201,166 B, both LF, both `diff`-identical to the repo-root originals. No librsvg notice — that row exists only because the Windows installer bundles a statically-linked librsvg, and staging it here would be over-attribution |

**The one-pass ordering this was held for is gone, and should not be reinstated.** The remedy
was deliberately deferred so all three seats changed their platform together; Linux has since
moved on its own, so waiting now buys nothing and leaves red rows describing an absence two
`Copy-Item` lines wide. Land each platform as its seat reaches it.

### REQUIRED FINAL CHECK before any published release: every statically linked dependency

**Enumerate every library statically linked into every binary we ship, on every platform, and
confirm each one's distribution requirements are met. This is a release gate, not a review
suggestion.**

It exists because **this project has missed this exact class twice, and neither miss was
careless** — both were careful reasoning over complete evidence about the wrong thing:

- **The `two-face` syntax grammars.** Compiled into the executable on all three platforms,
  under MIT / Apache-2.0 / BSD-2-Clause / BSD-3-Clause. Missed for months while the About
  dialog *asserted the remedy existed*, because the scope question was measured over *which
  platform bundles the GTK runtime* and the answer was then read as *which platform owes
  attribution*.
- **librsvg's 359-crate Rust graph inside `rsvg-2-2.dll`.** Found only by research. Shipping
  librsvg's own `COPYING.LIB` attributes librsvg and nothing it absorbed.

**The common mechanism, and the reason no amount of looking at the artefact finds it: a
statically linked dependency leaves no file on disk to notice.** Every gate this project has
built — the two-way table, all four conditions, the staged-tree diff — is a predicate over
*files*. Static linkage is invisible to all of them by construction, so this check can never
be replaced by extending them.

**Method** (each item measured, not inferred):

1. **Our own binary** — resolve from `Cargo.lock`, **per target triple**, with
   `[dev-dependencies]` **excluded**. The default `cargo-about` resolve is wrong here: it
   over-attributed librsvg by 132 crates, and a notice claiming the shipped binary contains
   `criterion` is a false statement about the product that looks like diligence.
2. **Every third-party binary we redistribute** — ask what *it* statically links, not what it
   is. A DLL's own licence attributes the DLL. This is the librsvg case and it will recur with
   any Rust- or Go-built component in a prefix.
3. **Confirm the obligation, not just the licence name.** Permissive licences still require the
   notice to travel with a *binary* distribution; that duty is what both misses violated.

**Partly automated as of `src/notices.rs`**: the `two-face` half is now a `cargo test` gate on
all three platforms, so those notices cannot silently go stale on a version bump. **The rest of
this check is manual and stays manual** — nothing in the toolchain knows which of a prefix's
binaries have a static Rust or Go graph inside them.

**Do not narrow this check to the platform that bundles most.** That instinct is precisely what
produced the first miss: Windows carries by far the largest *file* obligation, and obligation 2
was equal on all three platforms the whole time.

### The gaps, researched and mostly resolved

**An empty directory is evidence about gvsbuild's packaging, not about whether attribution
is owed.** That rule was applied rather than assumed, and it paid: of the six items where
the prefix shipped nothing, only one turned out to owe nothing, and two carry duties that a
vendored licence file does **not** discharge. Sourced by the researcher against upstream
repositories; findings doc `researcher-findings-windows-installer-thirdparty-licence-duediligence.md`.

| Item | Verdict | Duty beyond shipping the text |
|---|---|---|
| **FreeType** | `FTL` (upstream offers `FTL OR GPL-2.0-or-later`) → **FTL. RATIFIED by the operator, not inherited** — a dual licence is an election the redistributor makes, and the failure mode is not making it: whichever text happens to get vendored becomes the answer by accident. Only `FTL.TXT` ships; the GPL-2 alternative is deliberately absent, because shipping both would state that we redistribute under either. Recorded in `PROVENANCE.md` beside the file rather than implied by which file is present. **The row records what WE redistribute under, not what upstream offers**, which is why the SPDX expression narrowed to `FTL` once the election was made. | **Of the two duties, one is DISCHARGED and one is OPEN — and ratifying the election discharged neither; landing them did.** (1) **`LICENSE.TXT` is now vendored beside `FTL.TXT`** (2,149 B). It is not a second copy of the licence: it is the sole disclosure of the zlib, X11-style (BDF/PCF), HarfBuzz Old-MIT and public-domain code compiled into the same DLL, which nothing else in the prefix declares. Shipping FTL alone attributes FreeType and nothing FreeType absorbed — the librsvg problem one scale down. (2) **FTL §2 is now DISCHARGED.** It required binary redistribution to state, *"in the distribution documentation"*, that the software is based in part on the work of the FreeType Team — not in a licence file. `notices/10-freetype.md` now carries the canonical sentence, so it ships inside `THIRD-PARTY-LICENSES.md` on **all three platforms** (researcher-confirmed that an installed, discoverable notices file *is* distribution documentation; the licence text ranks no UI surface above another, so an About box is not required). The credit is deliberately **unscoped** — GTK renders through FreeType everywhere, so the sentence is true on every platform and a Windows-only carve-up would understate it. **Upstream's optional preferred credit (*"Portions of this software are copyright © \<year\> The FreeType Project"*) is deliberately NOT shipped, and the reason changed once the year was measured.** The year is **2026** — established from the source tree that built `freetype-6.dll` 2.14.3, where all 417 headers carry 23 different *start* years and a uniform *end* year of 2026. So the obstacle is no longer that the value is unknown. **It is that the value moves.** 2026 is this build's year; a gvsbuild bump changes it, the credit lives in a hand-authored `notices/` section that nothing regenerates, and the year cannot be derived on Linux because the FreeType source tree exists only on the Windows seat. That makes it **a hand-maintained copy of a fact that changes** — precisely the defect class removed from this file when it stopped being versioned. The clause is optional; the mandatory §2 sentence is unaffected. **Ship the required sentence, omit the optional credit, and do not "complete" it later without a way to derive the year.** **The structural point survives the fix and still matters**: all four gate conditions are predicates over staged files, so this row can read green while §2 is unmet — it was, until now, and nothing in the gate noticed. Deliberately not chased with a fifth condition reaching outside the staged tree; that condition could only substring-match prose, going green the moment the string appeared and staying green if the surrounding sentence said the opposite. Trap: FTL §3 "Advertising" is a *name-use* restriction (neither party may use the other's name promotionally), **not** a BSD-4 advertising clause; `LICENSE.TXT`'s own prose calls the FTL BSD-like *"with an advertising clause"*, so the misdirection is upstream's, inside the very file we vendor. There is no advertising-materials obligation — do not add one. |
| **Graphene** | `MIT` | Standard notice. Filename→project confirmed. |
| **libxml2** | `MIT AND ISC` | Standard notice; both texts. |
| **MSVC CRT** | proprietary; app-local deployment **is** licensed | No copyright-notice duty (the 2013-era one is gone). The grant is **folder-scoped, not per-DLL** — "any of the files within `[VisualStudioFolder]\VC\redist`" — which is why grepping for `vcruntime140.dll` finds nothing and why no `.txt` is on disk (Microsoft moved REDIST.txt online, `https://aka.ms/vs/17/redistribution`). **One duty everyone skips: the terms require distributors to bind end users to terms protecting the Distributable Code at least as much as the VS agreement. An Apache-2.0 LICENSE file does not do that — it needs an application EULA.** |
| **`BuilderBlocks.ttf`** | **CLOSED — verdict confirmed by observation, exclusion landed.** Not an attribution question. | It was staged by `stage.ps1`'s recursive copy of `share\gtksourceview-5`, which took `fonts\BuilderBlocks.ttf` in with the language-specs, and it was in every installer built until now — **confirmed present in an INSTALLED product at 500 B**, not merely in the stage tree. The exclusion is surgical: `language-specs`, `snippets` and `styles` kept, `fonts\` removed, 866 → 865 staged files. **Licence-neutral**, checked before being relied on — the `gtksourceview` row matches `^bin\\gtksourceview-5-0\.dll$\|^share\\gtksourceview-5\\`, so the five remaining files keep it satisfied and no condition-2 row appears.<br><br>**It is a denylist, not an allowlist, and the blind spot is deliberate.** Copying only the three known-good subdirectories would silently omit anything upstream adds that the app *does* need, and a missing runtime file fails worse than a stray 500-byte one. The cost is that a future addition under `share\gtksourceview-5\` ships unnoticed, and **the gate cannot catch it** because the `gtksourceview` row claims the whole subtree by pattern rather than file by file. A staging tripwire on an unrecognised entry under that prefix would close it, and was **deliberately not written**: it would turn a benign gvsbuild bump into a red build, which is a decision in its own right rather than a rider on this one.<br><br>**How it was settled, because "install and look" under-specifies it.** An unused font is invisible, so a correct-looking window cannot distinguish *unused* from *used and fine*; looking would have confirmed the row whichever way the truth ran. The discriminating test is **A/B on one install at pinned geometry**: launch, capture, remove the font, relaunch, capture, diff. Font present vs removed — 236 of 1,036,794 pixels differ (0.023%), bounding box 42×16 at x 133..174, y 39..54, which is the word "View" in the menubar carrying a focus underline left by the driving `SendKeys`, i.e. an artefact of the instrument. Re-run from the excluded stage tree against the original installed capture — **zero differing pixels across the entire editor client area**, the remainder localised to a 1px DWM border and desktop bleed-through outside the client rect (a raw total of 14,820 that would have read as a regression unlocalised). The full View menu was captured full-screen, because a window-rect capture truncates it at *Reset Zoom*: **22 items, no minimap/map/overview in any state**, the three greyed entries being a single-tab single-pane state rather than a limit of the observation.

**Closed on TWO independent methods, and the second was nearly thrown away.** Alongside the pixel work, stderr was checked for GtkSourceView's documented `Font loading error:` and was silent. That was initially discarded as worthless, because a control (deleting `gschemas.compiled`, believed to make GTK abort loudly) produced neither a message nor an abort — which looked like a dead channel. **The channel was fine.** Validated afterwards through routes independent of the subject: the binary is `IMAGE_SUBSYSTEM_WINDOWS_GUI` (PE `Subsystem = 2`), a bad CLI option reaches stderr via `g_printerr`, and — the route that actually matters, since a font-load failure is a *warning* — `GTK_DEBUG=no-such-debug-flag` makes `g_parse_debug_string` emit a `g_warning` that arrives on a redirected stderr. So the silence is a measurement through a validated instrument and corroborates the pixel A/B. **A failed control does not implicate the instrument; it suspends the measurement** — reading it as "the channel is dark" would have discarded true evidence and stopped the search, which nothing later corrects.

Mechanism read off the shipped DLL rather than assumed: `gtksourceview-5-0.dll` assembles the path at runtime (hence no single `fonts/BuilderBlocks.ttf` literal) and carries `textview, textview text { font-family: BuilderBlocks; font-size: 4px; line-height: 8px; }` beside `GTK_SOURCE_IS_MAP`/`gtk_source_map_set_view` — the minimap mosaic, whose only consumer is `GtkSourceMap`. **The "we never instantiate `GtkSourceMap`" grep is therefore no longer load-bearing**; the artefact says it. |
| **`share/icons/hicolor`** | **`CC-BY-SA-3.0`** — GtkSourceView's `data/icons/COPYING`, **not** its code licence | Attribution required: licence URI (§4(a)), author/title/URI credit (§4(c)). §4(a) explicitly does **not** subject the surrounding Collection to the licence, so the Apache-2.0 app is unaffected — *provided the SVGs ship unmodified*; any recolour makes an Adaptation and pulls ShareAlike §4(b) in. **CLOSED — `CC-BY-SA-3.0`, read rather than inferred** (researcher, re-confirmed against GtkSourceView **5.20.0**, the version gvsbuild 2026.8.0 pins; text identical 5.4.0→5.20.0 and master). The row covers **15 SVGs** — GtkSourceView's own `lang-*-symbolic`/`completion-*-symbolic` completion-provider set at `data/icons/hicolor/scalable/actions/`, disjoint from Adwaita's artwork. The basis is `data/icons/COPYING`: *"The icons here are licensed under the CC-by-SA 3."* Root `COPYING` is LGPL-2.1 and governs **code, not these icons**. The directory holds 17 files because `index.theme`/`icon-theme.cache` are generated theme metadata, not pictorial works, and carry no claim.<br><br>**Mark this row E, and note WHY it read as I for a whole round: upstream does not install its own COPYING.** `data/icons/meson.build` runs `install_subdir('hicolor', …)` only, so the SVGs land in the prefix **with no licence file beside them**. Reading the installed tree therefore yields "no per-file licence, so the project licence applies" — a careful inference from complete evidence that is nonetheless wrong, because the governing file was never installed. **Consequence: we must supply the CC-BY-SA-3.0 text ourselves; it will never appear in the prefix.**<br><br>**§4(a)**: ship the full CC-BY-SA-3.0 text (or a durable URI plus upstream's COPYING blurb). **§4(c) credit line**, from git history rather than invented — the whole set is one commit, `5470523d` *"icons: add various icons for completion"*, **Christian Hergert**, 2020-09-01, no later authors on that path:<br>*Title*: GtkSourceView completion icons (`lang-*-symbolic`, `completion-*-symbolic`) · *Author*: Christian Hergert · *Source*: GtkSourceView 5.20.0, https://gitlab.gnome.org/GNOME/gtksourceview · *Licence*: CC BY-SA 3.0, http://creativecommons.org/licenses/by-sa/3.0/ |

### The gap the research found that neither of us had listed

**`rsvg-2-2.dll` statically links 359 Rust crates.** librsvg's `deny.toml` admits
MIT / Apache-2.0 / BSD-3-Clause / NCSA / Unicode-3.0 / MPL-2.0 — every one of them
attribution-requiring, and none of them present anywhere in the gvsbuild prefix, because a
statically linked dependency leaves no file behind to notice. Shipping `COPYING.LIB` for
librsvg attributes librsvg and nothing it absorbed.

**RESOLVED — the notice is generated and landed** at
`packaging/windows/licenses/librsvg/THIRD-PARTY-RUST-NOTICES.txt`, which closes this row's
condition 3. Its condition 2 stays open until `stage.ps1` copies it. How it was derived, and
why the obvious method is wrong, is the next section — that reasoning is the valuable part and
outlives the artefact.

### librsvg's static Rust graph — resolved, and the obvious method is wrong

Measured against gvsbuild's own cargo registry rather than reasoned from the lockfile:

- **Policy is clean.** Nothing in the graph is GPL/LGPL/AGPL. Five MPL-2.0 (Servo-lineage
  CSS crates, unmodified), **19 Unicode-3.0** (the `icu_*` family, which needs its *own*
  notice wording and must not be folded into a generic MIT/Apache paragraph), and the rest
  permissive. Nothing forces a source obligation onto Scribobulate itself.
- **DO NOT GENERATE THE NOTICE FROM THE RAW LOCKFILE — it over-attributes by 132 crates.**
  The lock names 360 packages; only **228** were ever downloaded into the registry, and the
  132 absent ones have neither extracted source nor a cached `.crate` archive. Cause
  confirmed rather than inferred: they are librsvg's `[dev-dependencies]` (criterion,
  proptest, assert_cmd, lopdf …) plus their transitive closure. A notice claiming the
  shipped DLL contains `criterion` **is a false statement about the product**, and it is one
  that looks like diligence. This is the same over-attribution failure the two-way staging
  gate exists to prevent, arriving from a completely different direction — and the trap is
  that the DEFAULT `cargo-about` resolve produces the wrong answer, because excluding
  dev-dependencies is a setting someone has to choose deliberately.
- **The ship figure is 198**, against that measured upper bound of 228 — so the resolve did
  prune rather than landing on the bound (the 30 further crates go to target and feature
  filtering). Generated with `cargo-about` 0.9.1, dev-dependencies excluded, target
  `x86_64-pc-windows-msvc`; the notice, its `about.toml` and its template live in
  `packaging/windows/licenses/librsvg/` so the provenance is re-derivable rather than
  asserted. Independently verified on a second seat: the recorded body SHA-256 matches the
  committed bytes, no CR bytes survive, the named dev-dependencies are absent and the
  expected runtime crates present. A raw count reads 200 because `encoding_rs` and
  `unicode-ident` are each correctly listed under two licence headings; 198 is the unique
  figure.
- **Of the 19 Unicode-3.0 entries only 7 are literally `icu_*`** — the rest are the ICU4X
  support crates (`yoke`, `zerovec`, `zerofrom`, `litemap`, `tinystr`, `writeable`,
  `zerotrie`, `potential_utf`) plus `unicode-ident`. Say "the ICU4X family", not "19 `icu_*`
  crates", or the next reader greps for 19 names beginning `icu_` and finds seven.
- **Three things that would have shipped wrong**, all caught on the Windows seat and worth
  more than the artefact: `cargo install cargo-about` **exits 0 while installing nothing**
  without `--features cli`; the default resolve attributes **360** crates rather than 198;
  and the first stated SHA-256 did not match the committed bytes, because the generator
  emitted 202 CR bytes inside embedded licence texts and git normalised them. A provenance
  hash that does not survive the commit is worse than none — it fails for exactly the one
  person who tries to verify it.
- **The claim boundary is sound for CI**: `librsvg-2.0.pc` reports 2.62.3 in *both* the dev
  prefix and the CI prefix, so a notice generated from the local lockfile describes the
  librsvg that actually ships.
- **The provenance line is written and verifies**: generated from `Cargo.lock` of librsvg
  2.62.3 as built by gvsbuild 2026.8.0; dev-dependencies excluded; 198 crates; SHA-256 of
  the lockfile and of the notice body. The body hash deliberately covers everything *below*
  the provenance block — hashing the finished file would be circular — and is taken over LF
  endings so it survives git.

### A row neither the plan nor the inventory had: librsvg complete corresponding source

The replaceable-DLL argument (§6(b)) discharges the *relinking* duty, and static Rust inside
the DLL does **not** create a §6(a) obligation over Scribobulate's own object files. But
distributing the Library in binary form still owes **complete corresponding source for
librsvg 2.62.3, including its lockfile**, so the Rust graph is rebuildable. That is distinct
from the notice row, not a restatement of it. The lockfile half is free — it is already on
the Windows seat's box at 84,387 bytes.

### What LGPL actually requires here, since the stack is mostly LGPL

All the LGPL components in this artefact are `LGPL-2.1-or-later`; there is **no LGPL-3.0
component**. Shipping them triggers **two** clauses, not one:

- **§6 (linking)** — discharged by §6(b): the libraries are separate, replaceable DLLs, so
  the relinking limb is satisfied and no object-code drop of Scribobulate is owed.
- **§4 (conveying the library binaries)** — **not excused by §6(b)**, and this is the half
  that gets missed. LGPL-2.1 §4 has **no three-year written offer** (unlike GPLv2 §3(b)); it
  offers "accompany with source" or the *designated place* route. Practically: publish
  version-pinned upstream source tarballs on the same page as the installer.

That last point is a **release-process obligation, not a packaging one** — it cannot be
discharged by anything `stage.ps1` does, and it is the reason this section blocks a
published release rather than merely a build.

**The component list and versions are now measured, at
`packaging/windows/licenses/SOURCE-AVAILABILITY.md`.** Nine components owe source under §4;
`hicolor-icon-theme` owes none (already ruled — plain text is its own source, §1 not §3);
`adwaita-icon-theme` depends on an unmade election. What remains is the mechanism.

**THE DEV BOX IS TWO GVSBUILD RELEASES BEHIND WHAT SHIPS, AND THREE COMPONENT VERSIONS
DIFFER.** CI pins gvsbuild **2026.8.0** and downloads the prebuilt zip; the operator's box
has **2026.6.0** installed and a prefix built by it. Measured against both recipes' own
`projects/*.py`: **GLib 2.88.1 → 2.88.3, gdk-pixbuf 2.44.6 → 2.44.7, Pango 1.57.1 → 1.58.0.**
The other seven agree.

**BOTH HALVES ARE NOW MEASURED IN THE SHIPPED ZIP, not inferred.**
`GTK4_Gvsbuild_2026.8.0_x64.zip` was downloaded from the URL `pipeline.yml` uses, unpacked,
and both staged from and gated against. Its `.pc` files confirm every version including the
three that differ, and **the gate comes back clean on conditions 1, 2 and 4 against the GTK
that actually ships** — 903 staged files, 35 rows, `msvc-runtime` the only red row. So the
licence table is certified against the artefact rather than against the dev box, which is
the check `packaging/windows/licenses/PROVENANCE.md` records as previously impossible for
either seat to make. Two further findings live there: the one extra Adwaita icon 2026.8.0
adds (attributed automatically, which is the exhaustive table earning its keep), and the
shipped binaries being far larger — `rsvg-2-2.dll` by a factor of six — which is **not** a
debug build, tested rather than assumed.

This is the plan's own recurring failure mode caught before it shipped, and the near miss is
worth more than the correction. **Three independent readings on the dev box — the prefix's
`.pc` files, the tarball filenames in `C:\gtk-build\src\`, and gvsbuild 2026.6.0's pins —
agree with each other perfectly and all describe a build that is not the product.** Worse,
the one component anyone had previously checked across that boundary is `librsvg-2.0.pc`,
and librsvg is one of the seven that match, so the single check on record returned a falsely
reassuring answer about the whole set. **Any measurement taken on the dev box about what the
installer contains is answering the narrower question**, and that now extends past licensing
to anything read out of the local prefix.

### The gate as built — four conditions, not three, and not inside `stage.ps1`

**Built and LANDED on `ci`**: `packaging/windows/licenses.psd1` (34 rows covering all 863
staged files) and `packaging/windows/verify-licenses.ps1` (the gate, with `-SelfTest` and
`-Report`). Mutation battery of 7 subject-side mutants, all 7 killed.

**Condition 4 discriminates DOCUMENTS, not REVISIONS of one — a measured ceiling, recorded
rather than engineered away.** The FTL text vendored first was SPDX's `text/FTL.txt`: the
same licence re-wrapped one line per paragraph, section rules stripped, project URL still
`http://www.freetype.org` where FreeType 2.14.3's own file reads `https://freetype.org` —
5,979 B against 6,743 B, sitting beside a provenance line asserting 2.14.3 was shipped. The
gate passed it and **would pass it again**, because `The FreeType Project LICENSE` occurs in
both. Earlier condition-4 catches (`pcre2/COPYING`, the anti-bot pages) were the wrong
*document*; this was the right document in the wrong *edition*, one notch finer than a string
anchor can resolve. **Not widened on purpose:** a hash anchor would fail every row on any
upstream whitespace change and teach re-baselining rather than reading, converting a check
that catches real substitutions into a ritual. The limit is written into `PROVENANCE.md`
instead. **What caught it was not a check** — it was going to the build tree to collect
`LICENSE.TXT` and finding `docs/FTL.TXT` next to it. Do not let this section imply the gate
found it.

**Vendor from the build tree, not from a tag.** Both FreeType texts now come from
`C:\gtk-build\build\x64\release\freetype`, the tree gvsbuild compiled: a tag asserts upstream
published those bytes under that name, whereas the source tree *is* the bytes the shipped DLL
was built from. Agreement checked both ways — staged `bin\freetype-6.dll` reports FileVersion
2.14.3 and that tree's `freetype.h` defines MAJOR 2, MINOR 14, PATCH 3.

**THE GATE NOW READS A BUILD ARTEFACT, and the protection is accidental — say so before
someone reorders the pipeline.** `licenses.psd1`'s `syntax-grammars` row carries
`Source = 'repo:THIRD-PARTY-LICENSES.md'`, and that file is no longer versioned: `build.rs`
generates it. On a clean checkout with no build, condition 3 would report *"licence text
missing or empty: repo:THIRD-PARTY-LICENSES.md"* — **a red row about ordering that reads as a
row about licensing**, which is the most expensive kind of false failure because it sends the
reader to the wrong subject.

Two orderings prevent it today, and **neither was written for this purpose**:

1. `stage.ps1` throws on a missing `target\release\scribobulate.exe` before doing anything
   else, and the `cargo build --release` that produces that exe is what runs `build.rs`.
2. `packaging/macos/bundle.sh` runs `cargo build --release` at line 38, well before it copies
   the notices at line 85.

So the dependency is real and already enforced, by preconditions that exist for unrelated
reasons. **Anyone who moves the gate ahead of the build, or invokes it standalone, breaks it** —
and the failure will not look like what it is. The staging copy throwing rather than skipping on
a missing source is the second half of the protection: a genuinely absent file fails loudly at
staging instead of becoming a quiet condition-2 row later.

**`packaging/windows/licenses/.gitattributes` (`* -text`) is load-bearing, not tidiness.**
`core.autocrlf` is true on the Windows seat, so under the default `text=auto` a fresh clone
rewrites these files with CRLF — a different file byte for byte from the one that was fetched
and hashed, which silently falsifies every SHA-256 in `PROVENANCE.md` on Windows while leaving
it true on Linux. Measured on a clone of the librsvg notices *without* it: blob 219,581 vs
disk 223,875 bytes, each delta exactly the file's line count. **Any future directory of
byte-verbatim redistributed text needs the same guard, and it must arrive no later than the
files it protects.**

**Two deliberate departures from the design below, both ruled and both kept.**

**It is two files beside `stage.ps1`, not a table inside it.** The design's stated *reason*
for that placement — the gate must run where staging happens, not in the lint gate, so its
setup cannot prevent the condition it guards — is satisfied either way. A 34-row table plus
a four-condition gate inlined into a staging script makes that script one nobody reads. **Do
not inline it later.**

**A fourth condition was added, and it is the one that earns its keep: each row declares a
string that MUST occur in its licence text.** The three designed conditions all reduce to
file *presence*, which is ScrAP-278's exact shape — and it fails here in the direction that
costs most. Measured in the prefix: `pcre2/COPYING` is four lines pointing at a `LICENCE`
file that is not shipped; `cairo/COPYING` is a summary pointing at two files that are not
shipped; and **`gettext/COPYING` is GPL-3.0, the licence of the gettext tools, while the DLL
we ship is LGPL-2.1 `intl.dll`** — staging it would attach a GPL-3 claim to a component not
under it, which is worse than shipping nothing. All three files exist, so all three pass a
presence check.

**Current state: 1 open problem, RE-MEASURED by running the gate from a fresh stage** —
**902 files, 35 rows**. Conditions 1, 2 and 4 clean; condition 3 red on `msvc-runtime`
alone. The previous figure was 2 against 865 files and 34 rows, and it was stale in both
directions: `hicolor-icon-theme` had closed, and the licence texts had not yet been staged.
Obtained by running the gate, not by arithmetic on the old number.

**THE LICENCE TEXTS NOW ACTUALLY SHIP, and until this change none of them did.**
`licenses.psd1` opened by saying *"the installer currently ships not one line of their
licence text"*, and that stayed true through all the vendoring work: the table records where
each text **comes from**, and `verify-licenses.ps1` reads those `Source` paths off the
**build machine** — the repo and the GTK prefix — so all four conditions could pass while the
installed product carried no LGPL text at all. That is the same shape as the defect this
section already records for `LICENSE` and `THIRD-PARTY-LICENSES.md`, one layer further out:
the gate was green about the build tree and silent about the artefact. `stage.ps1` now stages
every row's text to `share\licenses\<row Id>\`, **driven by the manifest rather than by a
second list**, so a component added to the table ships its text without anyone remembering to
copy it. 37 files, +4 MB.

- **Condition 3 (1)** — `msvc-runtime` alone, and it is a decision rather than work: the row may
  be **deleted rather than filled** (see below). `hicolor-icon-theme` is **CLOSED** — its GPL-2
  `COPYING` came from the 0.18 source tree gvsbuild actually built, identified by content rather
  than by name (`meson.build` declares 0.18, and `index.theme` is byte-identical across that
  tree, the GTK prefix and the staged copy at SHA-256 `A02DB5E1…CB9BC5`). The row is now
  `Evidence = M`. **One qualification is recorded in the row rather than inherited silently:**
  upstream ships *no* version-selection statement anywhere in that tree — no per-file header, no
  copyright line, zero hits for `Copyright|Larsson|author` — so the licence *identity* is
  measured while the **`-or-later`** half and the Larsson attribution rest on Debian's reading.
  That distinction is the icon-row lesson arriving a third time: *"the tree contains no
  statement"* is evidence about what upstream ships, not about what the terms are.

### `hicolor-icon-theme` — researched, and a source obligation I raised does NOT exist

**LANDED — this section's remaining instruction is already carried out, and the section is
kept for the reasoning, not the task.** The re-basing demanded below is done: measured at
`packaging/windows/licenses.psd1:192-199`, the row now cites the 0.18 source tree's own
`COPYING` and the `index.theme` SHA-256 agreement, carries `Evidence = 'M'`, and states the
`-or-later` caveat inline. Do not act on the "must be re-based" wording further down; read
it as the argument that produced the current row.

**I was wrong, and the record should say so.** I flagged that shipping `share\icons\hicolor\index.theme`
under `GPL-2.0-or-later` triggers §3's source-availability duty. It does not. **§3 governs
distribution "in object code or executable form"; `index.theme` is plain text shipped verbatim,
so it is already its own source and falls under §1.** No tarball, no written offer, nothing to
publish. I raised it from the licence's *reputation* rather than from its text — the same failure
this plan warns about twice elsewhere, committed by the person writing the warnings.

**What actually governs, MEASURED against the hicolor-icon-theme 0.18 tarball:** the package
`COPYING` is the full GPL-2 text, `index.theme` carries **no per-file header** and is covered by
the package licence, and Debian's `copyright` records *Copyright 2002–2017 Alexander Larsson,
GPL-2+*.

**The row's `Basis` is wrong and must be re-based.** It currently reads *"index.theme declares
Name=Hicolor"*, which is a filename→project inference of exactly the kind the table exists to
avoid — cite `COPYING` and the Debian copyright instead, and the row can stop being `Evidence = I`.

**Do not delete the row on an originality theory.** "A list of directory names may not be
copyrightable" is arguable, and upstream asserts copyright through the GPL package; that is a
question for counsel, not for a packaging decision. **No Apache-2.0 conflict** — GPL-2's
aggregation clause covers it and the file is data GTK reads, not code linked into ours. The duty
is small: ship the GPL-2 text and a short credit.

**Condition 2 went 3 → 0 in one `stage.ps1` change**, as predicted and then measured: `LICENSE`
and `THIRD-PARTY-LICENSES.md` to the stage root, the librsvg notice to
`share\licenses\librsvg\`. No `.iss` edit was needed — `scribobulate.iss:76` installs
`{#StageDir}\*` with `recursesubdirs`, so anything staged is installed.

**Verified in an INSTALLED product, not in the stage tree** — the distinction matters, because
staging and packaging are separate chances to lose a file. Built, installed and inspected from
each side of the change:

| File | Before | After (CRLF, as installed on Windows) |
|---|---|---|
| `LICENSE` | ABSENT | 11,061 B |
| `THIRD-PARTY-LICENSES.md` | ABSENT | 205,287 B |
| `share\licenses\librsvg\THIRD-PARTY-RUST-NOTICES.txt` | ABSENT | 219,581 B |

**`THIRD-PARTY-LICENSES.md` now stages at 201,166 B with zero CR — MEASURED on the Windows seat,
not predicted.** It was 205,287 B while the file was *versioned* as `text: auto`, because an
`autocrlf` checkout rewrote it to CRLF and `stage.ps1` copied the rewritten bytes. `build.rs`
generates it now and normalises to LF, so there is no git round-trip and no platform variance
left; the delta is exactly the 4,121 CRs that are gone. The staged copy was compared to the
generated one by **SHA-256, not by size**.

**`LICENSE` is the control that makes that result mean something**, and it is why the pair is
recorded together: staged from the same repo, by the same `Copy-Item`, on the same seat, under
the same `autocrlf` — and it still carries **196 CRs at 11,061 B**, because it is still
versioned as `text: auto`. Only the generated file came out LF. That isolates the change to the
removed git round-trip rather than to anything about the copy. **State the line-ending
convention beside any byte count for `LICENSE`** — a figure without one is unreproducible, and
the next reader will explain the gap with something plausible rather than measuring it
(ScrAP-289).

**The false-red predicted below was OBSERVED, then observed to clear.** Running the gate on a
fresh checkout *before any build* reproduced it exactly: condition 3 reported *"licence text
missing or empty: repo:THIRD-PARTY-LICENSES.md"*, 3 problems instead of 2. After
`cargo build --release` and `stage.ps1`, back to the two that are red on purpose. **And the
shape is as bad as predicted** — condition **2 passes for that row the whole time**, because a
`THIRD-PARTY-LICENSES.md` from a previous run is sitting in the stage tree, so the gate reports
a licence text missing while the licence is demonstrably shipped.

869 files installed (866 staged plus 3 installer artefacts), **zero size mismatches against the
stage tree**, so the installer is byte-faithful and the fix survives packaging.

**The "before" column is the defect observed rather than argued**: the wizard displayed the
Apache-2.0 text via `scribobulate.iss:61`'s `LicenseFile` and the installed product carried no
licence text of any kind. Shown at install time, absent from the installed tree — a worse gap
than absent-everywhere, because it reads as handled.

**Two lessons about this status line itself, both paid for.** It read 13 for four commits — the
tally at the first gate commit, which neither the vendoring pass nor the librsvg notices moved.
A status line that is not re-measured when the thing it describes changes is worse than none,
because it is read as current. And the 5 → 2 above was **re-measured on a fresh stage after the
rebase** rather than carried over from the pre-rebase run, on the grounds that *"it cannot have
changed"* is the reasoning that produced the 13. **Re-run the gate; never edit the number by
hand.**

**The MSVC row cannot be closed on the Windows seat's own text.** No terms file exists in the
redist directory — only the two `vc_redist` executables — so the terms come from the Visual
Studio licence, and the correct one is the edition that built the *shipped* bytes: CI's VS 18
Enterprise, not the dev box's VS 2022 Community.

### The MSVC row is being DELETED, not closed — RULED by the operator

**Decision: bootstrap Microsoft's redistributable; stop shipping the CRT DLLs.** The
alternative (keep app-local, add a click-through EULA) was assessed and rejected.

**THE RULING'S PREMISE HAS NOW BEEN READ, AND IT HOLDS — MEASURED.** The reasoning is *stop
distributing Distributable Code and the obligation evaporates*, but we redistribute a
**different** Microsoft binary in its place, `vc_redist.x64.exe`, embedded in our own
installer. Both load-bearing propositions were checked against primary text rather than
inherited:

- **Redistributing the unmodified package is explicitly PERMITTED and RECOMMENDED**, not
  merely common. The VS Distributable List (`https://aka.ms/vs/17/redistribution`) names *the
  Redistributable package* as a redistributable artefact, limited to licensed Visual Studio
  users; Learn's *Redistribute Visual C++ Files* says to *"run it as a prerequisite on the
  target system before you install your application"* and recommends it. **Microsoft's own
  deployment walkthrough packs `vc_redist.*.exe` inside a third-party installer**, which is
  exactly what we do.
- **The package path removes the need for our own MSVC click-through.** MEASURED on the docs;
  the discharge of duty (2) is INFERRED from them. Microsoft's walkthrough does not have the
  outer installer present a Microsoft EULA.

**ONE CAVEAT THAT BITES US SPECIFICALLY, and it must not be smoothed over.** We invoke the
package with `/install /passive /norestart`. Under `/passive` — and `/quiet` — **Microsoft's
installer presents no click-through either.** So the comfortable phrase *"Microsoft's UI
collects the agreement"* is true only of an **interactive** run, and ours is not one. This
does **not** revoke the permission to redistribute, and `/passive` is the standard invocation;
it means the discharge rests on the *authorised deployment model*, not on a dialogue the user
actually sees. Recorded rather than relied on silently, and the reason `notices/20-msvc.md`
carries a line naming the component and its licensor.

**Residual, INFERRED and low weight:** a pedant could argue duty (2) still attaches to us as
redistributor of the package. Microsoft's published walkthrough and prerequisite guidance are
the counterweight. **Not grounds to reopen the EULA work.**

**The licence text is not on the build machine, and the artefact that is there would have
passed the gate.** `<VS>\Licenses\` holds SDK, NuGet and .NET EULAs but not the Visual Studio
licence; `Licenses\1033\Redist.txt` is **187 bytes** and is a referral to
`https://aka.ms/vs/17/redist.txt`, not terms. **It contains the `msvc-runtime` row's declared
anchor `Distributable Code`** — because that phrase is in the pointer's own first line — so it
exists, is non-empty, and matches. Had the row ever been closed by vendoring it, **all four
conditions would have gone green on a 187-byte referral.** That is `pcre2/COPYING`'s shape
again, on the row where being wrong would have cost the most.

**The gate's scope narrowed relative to the artefact, and the change did not create the blind
spot — it moved something into an existing one.** `vc_redist.x64.exe` is compiled into
`setup.exe` and never enters the staged tree, so no condition can see it. Neither can Inno
Setup's own code, which has shipped in **every installer this project has produced** and was
never in scope — unnoticed because the gate had never been green before. **A PASS makes an
unlit area read as an empty one.** Remedy chosen: the gate now prints its scope beside the
verdict, naming both non-staged carriers, rather than growing a row for a file that would fail
condition 2 by construction. Extending the gate to reach non-staged inputs was rejected —
the installer's contents are known only to ISCC, so it would be a list checked against another
list. Inno Setup itself owes nothing (its `license.txt` clause 2 is satisfied by not modifying
what it generates; clause 3's acknowledgment is "appreciated but not required").

The elevation cost below is accepted knowingly, and **is narrower than this ruling assumed**:
`PrivilegesRequired=lowest` is unchanged, setup raises no prompt of its own, and `vc_redist`
prompts for itself alone and only on a machine that lacks the runtime.

**Three things this changes, and the third is the one that can be got wrong silently:**

1. `bin\vcruntime140.dll` and `vcruntime140_1.dll` leave the stage tree, and the
   `msvc-runtime` row leaves `licenses.psd1`. The two-way gate makes that self-consistent:
   removing the files without removing the row fails condition 2, and vice versa fails
   condition 1. **Neither half can be forgotten.**
2. `PrivilegesRequired=lowest` can no longer hold unconditionally — `vc_redist.x64.exe`
   installs machine-wide. Whether the installer elevates throughout or only for the
   prerequisite is an implementation choice; **that the no-admin property is being spent is
   not.**
3. **The app now depends on a machine-wide runtime it does not carry.** gvsbuild's GTK DLLs
   are built against the dynamic CRT, so the dependency does not go away — it moves from
   our stage tree to the system.

**VERIFY ON A MACHINE THAT DOES NOT ALREADY HAVE THE RUNTIME.** Every development and CI
box has the CRT installed, so removing the DLLs and launching successfully **proves
nothing** — the app is loading the system copy and would have done so either way. A green
run on a provisioned box is exactly the vacuous pass this project has already shipped: the
condition under test is satisfied by the environment rather than by the change. The
observation that means something is the app **failing** to start with the DLLs removed and
the redistributable absent, then starting after the bootstrapper runs. Without that pair,
"it works" is a statement about the test machine.

**Also settle, before implementing:** what happens when a user declines the UAC prompt.
Silently installing an application that cannot start is worse than refusing to install.

### Why it was not free — the assessment behind the ruling

**Stop shipping `bin\vcruntime140.dll` and `vcruntime140_1.dll`, and the entire obligation
evaporates**, because we would no longer be distributing Microsoft's Distributable Code. Detect
the runtime (`HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64`, or the Wow6432Node
equivalent) and otherwise run `vc_redist.x64.exe /install /passive /norestart` as a prerequisite;
Microsoft's own installer then attaches Microsoft's terms. This is also what Microsoft documents
as preferred for servicing, and it is what peer projects do. **It dissolves the EULA question
below entirely.**

**The cost is real and specific to how this installer is built.** `scribobulate.iss` sets
`PrivilegesRequired=lowest` — a per-user install needing no administrator. **`vc_redist.x64.exe`
installs machine-wide and requires elevation**, so taking this route means either prompting for
admin (`PrivilegesRequiredOverridesAllowed=dialog` already permits it) or abandoning the
no-admin property. That is a product decision about what kind of installer this is, not a
packaging detail, and it is the researcher's own named exception: the bootstrapper argument does
not hold for *"portable / no-admin single-folder installs"*, which is precisely what we ship
today.

**Only if we KEEP app-local deployment does the EULA work begin.** MEASURED from the VS 2022
Community licence: the Distribution Requirements are (1) add significant primary functionality,
and (2) **require distributors and end users to agree to** terms protecting the Distributable
Code at least as much as that agreement does. Two consequences: **an Apache-2.0 `LICENSE` does
not satisfy (2)**, and *"agree to"* means a **click-through**, not a `EULA.txt` sitting on disk.
Any such EULA must explicitly carve out that Apache-2.0 still governs *our* code and that the
Microsoft terms reach only Microsoft's components. There is no separate splash-screen copyright
duty in the current requirements. Community and Enterprise editions impose the same requirement
class for these files, so the earlier worry about *which* edition's licence governs matters less
than it appeared.

**`gschemas.compiled` is GTK's alone** — every schema id in the blob is `org.gtk.gtk4.*`, no
glib, no gtksourceview. It reads like an aggregate and is not.

### How this compares to what everyone else ships — researched, and we are heavier

The approach was derived from first principles without checking prior art. Checked now, against
GTK/Qt desktop apps that bundle a large LGPL runtime into a Windows installer:

- **Bundling GTK on Windows is correct and normal.** There is no system GTK4; GIMP-class apps do
  the same. That part is not over-engineering.
- **A two-way, per-file gate over 865 staged files is rare and heavier than any peer.** GIMP ships
  the huge tree with a composite licence blurb and no per-file gate. HandBrake displays its GPL
  with no forced accept and externalises the .NET runtime to Microsoft. Audacity stopped forcing
  licence acceptance.
- **Load-bearing here, and worth every line:** the high-risk component texts — the LGPL family,
  the FreeType credit, the librsvg Rust notice, the CC-BY-SA icons, and our own Apache-2.0.
- **Called gold-plating relative to peers:** chasing every metadata orphan, and running
  anchor-string checks over *all* rows rather than the risky ones.

**That last judgement is contested by the seat that built it, with evidence, and the objection
holds.** Exhaustiveness is not a separate luxury layered on top of the risky rows — **it is what
makes conditions 1 and 2 possible at all.** A table covering only high-risk components cannot
answer *"is every staged file accounted for"*, and that question is what caught the
GtkSourceView icons being attributed to Adwaita, `gschemas.compiled` being read as an aggregate,
and a notices file that was displayed at install and never installed. **None of those three was
a high-risk component; all three were found by exhaustiveness.** The cost also sits almost
entirely in *building* the table, which is spent, so the marginal cost of keeping it complete is
near zero and shrinking it later would cost more than it saves. Read the peer comparison as
*"do not extend this on momentum"*, not as *"this should be smaller"*.
- **On the MSVC runtime specifically, peers overwhelmingly externalise to Microsoft** rather than
  writing an EULA — which is the section above, and the reason to settle that question before
  drafting any legal prose.

Recording this because the gate is easy to keep extending on momentum. It has caught real
defects and it is worth keeping; it is not worth growing.

### The design, now implemented — the rationale worth keeping

The design was drafted here and built by the Windows seat, because they are the seat that can
run it and a staging gate nobody can execute is the defect this project has already shipped
twice. **Ownership had to be said out loud: naming a design without naming a hand left both
sides waiting on the other once.** What survives implementation is the reasoning, kept because
it governs every row added from here:

The table is **explicit** because filename→project is an *inference*, and an inference belongs
somewhere a human can check rather than in a build-time regex. Rows carry an **E**/**I** mark
for whether the project was confirmed by the binary's own version resource or inferred. GLib
gets its **whole SPDX-named directory** (eleven texts), never a first match —
`LicenseRef-old-glib-tests.txt` sits in it and is *not* GLib's licence. Vendored projects live
under `packaging/windows/licenses/<project>/` with a comment naming the upstream source and
saying why they are vendored.

The gate is **two-way and existence-checking**: it fails on a staged file with no table entry,
on a table entry with no staged file, *and* on a row whose licence source does not exist at
stage time. One-way lets the table rot into over-attribution while looking rigorous, and a
row-presence check stays green while the artefact ships an empty `licenses/` folder. It runs
where staging happens, not in the lint gate, because lint does not stage — a property of the
staged tree asserted by something that never stages is a guard whose setup prevents the
condition it guards. (Where it landed — two files beside `stage.ps1` rather than a table
inside it — and the fourth condition it gained are the two ruled departures recorded under
"The gate as built" above.)

### macOS is deferred by operator decision

The `.dmg` requires the recipient to have Homebrew GTK and therefore fails step 10's stated
intent outright rather than merely falling short on signing. Closing it is ~50 MB of dylib
bundling plus load-path rewriting, the icon theme, GLib schemas and pixbuf loaders — a
separate piece of work, and the macOS seat is occupied. **Until greenlit, macOS must not be
attached to a release.**

It defers **obligation 1** with it — the `.app` bundles no GTK runtime, so there is no
runtime attribution to stage. It does **not** defer **obligation 2**: `bundle.sh` has no
licence handling at all, so the `.app` carries no `THIRD-PARTY-LICENSES.md` even though the
grammars are statically linked into the macOS binary exactly as they are into the other two.
That gap is live whether or not the dylib-bundling work is ever greenlit, and closing it is
one staged file rather than 50 MB of load-path rewriting. The earlier wording here —
*"since the `.app` bundles nothing, this defers no attribution work"* — is the scope error
described under obligation 2, in miniature: true about the runtime, and read as true about
attribution in general.
