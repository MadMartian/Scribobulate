# Plan: How the open debt register is batched

> ## ⚠️ This file cites `sdd/ISSUES.md` entries by letter — an OPERATOR-GRANTED, ONE-TIME EXCEPTION
>
> **Granted 2026-08-28. Do not "fix" the citations in this file, and do not treat it as
> precedent.** SDD principle 6 bans pointing at an issue from outside the register, because
> issue letters are ephemeral: an entry is deleted the moment it is fixed, so every pointer
> is born with an expiry date, and if the letters are ever compacted the pointer does not
> break loudly — it quietly names a different issue. That reasoning is unchanged and still
> governs every other file in the tree.
>
> The exception was granted for one reason: without the letters this document cannot do its
> job, which is to let the operator see *what is left and in what order*. A batch map that
> describes issues without naming them is not a map.
>
> `cargo xtask lint-references` check 1 already exempts `sdd/PLAN.*.md`, so this costs no
> gate change and trips nothing.
>
> **The obligation this creates:** when an issue is fixed and deleted from the register,
> **delete its row here in the same change**, and when the alphabet is compacted, re-derive
> every letter below from the register rather than carrying it across. This file is the one
> place in the tree that has to be swept, and it is why the ban exists everywhere else.

## Problem

The register holds eighteen entries of mixed severity, platform and scope. Read top to
bottom it says what is broken; it does not say what to *do*, because the entries that share
a mechanism are scattered through the alphabet and the ones that are not ours to fix sit
beside the ones that are. Work picked off it one letter at a time invites three different
answers to one question — which is how a batch-1 defect (one missing reading position seen
from two ends) had been filed as two unrelated issues and nearly fixed twice.

This is the sequencing map. It is a **decision record about grouping and order**, not a
tracker: the issue bodies stay in the register and are not duplicated here.

## What a batch is

A batch is a set of issues that share a **mechanism**, a **fixture**, or a **verification
rig** — so fixing them together costs materially less than fixing them apart, and lands as
**one commit** (POLICY § One commit per batch).
A group of issues that merely share a subsystem is not a batch; if they need two mechanisms,
they are two batches.

## The batches

| Batch | Issues | Severity | Why these coalesce | State |
|---|---|:-:|---|---|
| **1 — View-state handoff** | *(U, V — deleted)* | Medium | One missing document position, seen from two ends | ✅ **DONE**, one commit on master |
| **2 — A theme change that never lands** | *(T — deleted)* | Medium | A reading-theme switch missing a background tab. C was grouped here at first and turned out to be a different mechanism entirely; the operator then dropped it as upstream (below) | ✅ **DONE**, verified on the live display |
| **3 — Find highlight rendering** | *(Y, X — deleted)* | Medium, Low | Y did not reproduce and its recorded root cause was disproven; X was dropped as never having been a defect | ✅ **DONE** |
| **4 — Theme fidelity across export sinks** | **W**, **S** | Low, Low | Both fail TDD 25.9 the same way — a value that does not resolve through the theme engine. Coalescing buys a real deliverable neither gets alone: a **cross-sink parity guard** that the two sinks agree on a given key | 💡 Ready |
| **5 — Preview render diagnostics** | **N**, **P** | Low, Low | Neither is a fix yet; both need a deliberate reproduction before anyone can act, and both want the same instrument — a fixture matrix driven at several pane widths with screenshot comparison. One pass covers both; two passes duplicate the setup | 💡 Ready |

## Standalone — real work, but no batch to join

| Issue | Severity | Why it stands alone |
|---|:-:|---|
| **D** | **High** | The only High in the register, and the largest thing outstanding: a large document pegs a CPU core at ~100% while idle. Shares a mechanism with nothing here. Related in *instrument* to [PLAN.profiling.md](PLAN.profiling.md), which exists because this class of defect has no oracle |
| **B** | Low | A nesting defect in strikethrough parsing — the parser, touching nothing else here |
| **E** | Low | A click inside an existing selection never reaches the pane's affordances. Interaction/gesture routing; no sibling |
| **G** | Low | `Test` scope, not `Production`: two wall-clock growth-ratio guards go red on a loaded machine. A gate-design problem, not an application one — the fix is to stop measuring an exponent with a stopwatch on a small baseline |
| **H** | Low | macOS-only and intermittent; needs the Mac seat's hands, not a mechanism |

## Not work waiting to be scheduled

| Issue | Why it is not on the list |
|---|---|
| **A** | `Closed` — intractable, source-verified, and deliberately retained so nobody reopens the dead end. Not actionable by design |
| ~~C~~ | **Dropped by the operator, 2026-08-28**, on evidence the register could not have held: the same failure to repaint on a KDE/X11 desktop dark↔light switch occurs in an unrelated GNOME application, so it is upstream and outside this project's scope. Recorded here rather than nowhere because the investigation it invites is expensive — a new `src/platform/linux/` portal seam — and the reason not to do it is not visible from the code |
| ~~X~~ | **Dropped by the operator, 2026-08-28**, as never having been a defect. Its confirmed behaviour was that a click collapses the selection which *is* the current-match indicator, re-asserting on the next Next/Prev — ordinary text-buffer behaviour, and the remedy once proposed for it would have taken the caret back from the user |
| **F**, **I** | `Upstream` — the defect is in a third-party library and the repair is not ours to make. A workaround may exist; the fix does not belong to this project |
| **R** | Not an engineering task at all: an operator decision about Pixel Quest's palette (retune both inks, retune one, or settle both permanently and close). Blocked on the operator, not on effort |
| **M** | Inert today — every current call site resolves at a bounded set of sizes, so the missing cache eviction cannot fire. Either a small LRU cap or a documented constraint at the call sites; confirm it is worth spending anything on before doing so |

## Recommendation

**Order: D, then 2, then 3, then 4, then 5.**

D goes first on severity alone — it is the only High, it is the one entry a user feels as
the application being broken rather than as a detail being wrong, and nothing else in the
register competes with it. It is also the one most likely to need the researcher and the
profiling instrument, so starting it early overlaps its waits with other work.

Batch 2 next because both its issues are already-measured live-display reproductions and
the ink defect is visibly wrong on a themed page. Batches 3–5 are ordered by severity within
"cheap and ready". R and M are decisions rather than tasks and can be taken at any point.

**A note on what this ordering is not.** It is not a commitment to do them all: the register
is meant to *empty*, but several of these entries are correctly parked, and 5 in particular
may resolve to "accept the limitation" once its reproduction runs. A batch that measures its
way to "this is fine" has done its job and should delete its issues, not invent a fix.
