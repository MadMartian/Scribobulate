# Plan: CI as the enforcement mechanism for build-pipeline parity

## Problem

The three platforms now share one step contract (`scripts/pipeline.steps`) and
each has a runner that *derives* its step list from it, with a
`--list-steps`/`-ListSteps` artefact that can be diffed between them. That is
approach 3 of the retired build-pipelines plan, and it works: the Linux and
macOS runners print byte-identical output from independently written code.

**But the parity claim is still enforced by a human remembering to check it.**
Nothing fails when the three drift. The diff is run when someone thinks to run
it, on a machine that happens to have two of the three ports reachable — and no
single machine has all three, because a Linux box has no PowerShell and no
Quartz. The Windows port's 11-step parity is, as this is written, *inferred*
rather than measured for exactly that reason.

This is the same shape as every other assurance this project has had to learn
about the hard way: the claim is true today, nothing checks it, and the next
author trusts it. `lint-references.scan` spent a long time proving the two lint
ports agreed with each other while proving neither honoured the contract
(ScrAP-207), and it was only ever caught because someone went looking.

### Root cause

There is no machine that can execute all three pipelines, so there is no place
the comparison can be made a *gate* rather than an errand.

## Previously attempted

Nothing — this was deferred deliberately, not retreated from. The retired
build-pipelines plan chose approach 3 first and recorded why:

- CI is the only mechanism that makes the parity claim self-enforcing, and every
  unenforced assurance in this repository has eventually turned out to be false.
- But it should follow a working contract rather than precede it, because the
  version of this that fails is the one where the machinery lands before the
  thing it is meant to check.

That ordering has now been satisfied. The contract exists, all three runners
consume it, and two of the three are verified against each other.

## Possible approaches

### 1. A CI matrix that runs each platform's pipeline

Three hosted runners; each executes its own platform's pipeline on every push.

**Pros**: catches a broken runner, not just a drifted step list.
**Cons**: GTK on three hosted runners is real work. The macOS runner hits the
`codesign` and bundling questions with no interactive session to debug them, and
the Windows runner needs a gvsbuild GTK — neither is a small provisioning job.

### 2. A parity job only

One runner (any platform) fetches all three ports' `--list-steps` output and
diffs them. Cheaper, and it targets the specific claim that is currently
unenforced.

**Cons**: a port can print a correct step list and still be incapable of running
a step — measured, not hypothetical: the Windows port passed `-ListSteps`
byte-identically, `-SelfTest`, and a twelve-case mutation battery while a
PowerShell output-stream bug made it report `pipeline PASSED` with exit 0 after a
step had failed. Contract-parsing evidence is evidence about contract parsing.

### 3. Both — parity job now, execution matrix incrementally

Land the parity job first, because it is cheap and closes the claim that is
actually unguarded. Add per-platform execution as each platform's provisioning is
solved, Linux first (Xvfb, already used locally), then Windows, then macOS.

## Recommendation

**Approach 3.** Take the parity job first: it is a small job that converts the
one claim nothing currently checks into a build failure, and it does not depend
on solving GTK provisioning on any hosted runner. Then add execution per
platform as provisioning allows, in ascending order of difficulty.

Do not let the parity job stand alone indefinitely. On its own it proves the
three ports agree about the step list, which the Windows port has already
demonstrated is compatible with being unable to execute a single step. The
execution matrix is what closes that, and approach 2 alone would re-create the
"comparison without conformance" failure one level up.

## Technical details preserved

- **`.github/` does not exist** in this repository, so this is a larger change
  than the pipelines themselves were.
- **No single machine can run all three ports.** A Linux box has no PowerShell
  and cannot satisfy the macOS runner's `uname` guard; cross-compiling to macOS
  from Linux is a closed dead end (POLICY § Build). This is the constraint that
  makes CI the only real answer rather than a convenience.
- **The runners are already CI-shaped.** Each takes `--list-steps` for the
  artefact, `--self-test` for contract validation, `--skip-integration` for an
  unattended run, and `--package` for the installer — so a CI job consumes the
  same entry points a developer does, with nothing CI-specific to maintain.
- **`G_DEBUG=fatal-criticals`** is the standing recommendation for a CI test run,
  so a `Gtk-CRITICAL` becomes a hard failure rather than a silent log line
  (POLICY § Logging).
- **The verification bar for a CI gate is injection, not a green run.** A gate
  that reports success while a step failed is the defect a gate exists to
  prevent, and it has already occurred once in this campaign inside a runner that
  passed every parsing check. Any CI job added here must be shown to fail on a
  deliberately injected failure before it is trusted.
