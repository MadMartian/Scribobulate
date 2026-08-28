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

## Where this stands — read this first

**Ten of the eighteen entries this map opened with are gone; eight remain.** Batches 1–5
are done and each landed as one commit on **`mitigations/various`**, which is this
campaign's integration branch — **not `master`**, which is behind it and contains none
of this work. (This paragraph said `master` until 2026-08-28, when the batch-5 seat went
looking for batch 4 there and did not find it. A resume note naming the wrong branch
fails in the one situation it exists for.) Nothing is in flight: no branch, no
uncommitted work, all gates green (fmt, clippy `-D warnings`, both test harnesses,
coverage at its ratchet, `lint-references`).

**Every batch is now closed. What is left is the standalone column below**, and it is led
by `D` — the only High, and the largest thing outstanding.

**What a session resuming here should know, and would otherwise re-derive:**

- **`D` is the only High and the only open-ended item.** It is the one place where the
  timebox matters more than the fix, and it is worth agreeing the budget with the operator
  before starting rather than after.
- **`R` is not an engineering task.** It is a decision the operator owns, a palette
  question about Pixel Quest, so do not start it without an answer.
- **Four of the five batches closed at least one issue WITHOUT writing a fix** — by
  disproving a recorded root cause, by finding the contract already stated, or by the
  operator ruling something out of scope. Read an entry sceptically before building on it:
  **three** of the root causes recorded in this register have now been measured and found
  wrong, and batch 5 added a fourth kind — an entry whose stated worry was unfounded while
  a *different*, real defect sat underneath it, reachable only because the reproduction was
  built anyway.
- **A "measure it first" batch pays even when the fix it predicted is not the fix it
  finds.** Batch 5 was scoped as a reproduction pass for two Low entries. One did not
  reproduce at all; the other's stated concern was already structurally guaranteed. The
  rig built to establish both then found a live rendering defect neither entry described.

## The batches

| Batch | Issues | Severity | Why these coalesce | State |
|---|---|:-:|---|---|
| **1 — View-state handoff** | *(U, V — deleted)* | Medium | One missing document position, seen from two ends | ✅ **DONE**, one commit |
| **2 — A theme change that never lands** | *(T — deleted)* | Medium | A reading-theme switch missing a background tab. C was grouped here at first and turned out to be a different mechanism entirely; the operator then dropped it as upstream (below) | ✅ **DONE**, verified on the live display |
| **3 — Find highlight rendering** | *(Y, X — deleted)* | Medium, Low | Y did not reproduce and its recorded root cause was disproven; X was dropped as never having been a defect | ✅ **DONE** |
| **4 — Theme fidelity across export sinks** | *(W, S — deleted)* | Low, Low | Both were violations of contracts that already existed (TDD 18.43, 25.9), so neither needed a new rubric. The parity guard the batch was supposed to buy turned out to be already built — `theme::tests::sinks`'s registry sweep — and it earned its keep immediately by failing on a THIRD surface neither issue mentioned | ✅ **DONE** |
| **5 — Preview render diagnostics** | *(N, P — deleted)* | Low, Low | Neither was a fix, and neither turned out to be a defect as filed. **N did not reproduce**: the preview cannot produce an over-wide line at all under `WrapMode::Char`, measured 0px overflow across four pane widths and nine constructs with a positive control (flipping to `Word` overflows by up to 1300px). Its original "700×1000" geometry was never reached — the outline sidebar's longest heading label floors the window near 1220px and `xdotool windowsize` is clamped in silence. **P's stated worry was already structurally impossible**: `renderer::end` closes one `BufferSpan` per whole quote, so the bar is continuous across every span shape by construction. But the rig built to prove that found a real defect neither entry named — a sprite-tiled bar anchored to the viewport instead of the document (ScrAP-333), fixed in the same commit | ✅ **DONE** |

## Standalone — real work, but no batch to join

| Issue | Severity | Why it stands alone |
|---|:-:|---|
| **D** | **High** | The only High in the register, and the largest thing outstanding: a large document pegs a CPU core at ~100% while idle. Shares a mechanism with nothing here. **[PLAN.profiling.md](PLAN.profiling.md) is a PREREQUISITE, not a companion** (operator, 2026-08-28): this class of defect has no oracle, so the instrument is built and its cost agreed BEFORE the hunt starts, otherwise the open-ended part of D is the measuring rather than the fixing |
| **B** | Low | A nesting defect in strikethrough parsing — the parser, touching nothing else here |

| **G** | Low | `Test` scope, not `Production`: two wall-clock growth-ratio guards go red on a loaded machine. A gate-design problem, not an application one — the fix is to stop measuring an exponent with a stopwatch on a small baseline |
| **H** | Low | macOS-only and intermittent; needs the Mac seat's hands, not a mechanism |

## Not work waiting to be scheduled

| Issue | Why it is not on the list |
|---|---|
| **A** | `Closed` — intractable, source-verified, and deliberately retained so nobody reopens the dead end. Not actionable by design |
| ~~C~~ | **Dropped by the operator, 2026-08-28**, on evidence the register could not have held: the same failure to repaint on a KDE/X11 desktop dark↔light switch occurs in an unrelated GNOME application, so it is upstream and outside this project's scope. Recorded here rather than nowhere because the investigation it invites is expensive — a new `src/platform/linux/` portal seam — and the reason not to do it is not visible from the code |
| ~~E~~ | **Dropped by the operator, 2026-08-28**, and documented as expected UI behaviour instead (TDD 2.24a, MANUAL-TEST 2.24a). A press landing inside an existing selection is claimed by `gtk_text_view_click_gesture_pressed` for a possible drag-and-drop, and that claim denies the application's own gesture; `DENIED` is terminal and is not a cancellation, so the app sees `pressed` and then neither `released` nor `cancel`. The one workaround was priced and rejected: claiming the sequence ourselves is the only way to out-rank GTK, and it trades one self-correcting wasted click for the silent loss of a selection drag begun over an affordance, with intent unknowable at the moment the claim must be made. Recorded here because the entry was well-traced and reads like actionable work right up until the pricing |
| ~~X~~ | **Dropped by the operator, 2026-08-28**, as never having been a defect. Its confirmed behaviour was that a click collapses the selection which *is* the current-match indicator, re-asserting on the next Next/Prev — ordinary text-buffer behaviour, and the remedy once proposed for it would have taken the caret back from the user |
| **F**, **I** | `Upstream` — the defect is in a third-party library and the repair is not ours to make. A workaround may exist; the fix does not belong to this project |
| **R** | Not an engineering task at all: an operator decision about Pixel Quest's palette (retune both inks, retune one, or settle both permanently and close). Blocked on the operator, not on effort |
| ~~M~~ | **Dropped by the operator, 2026-08-28**, as never having been a defect: the entry's own premise was false. It claimed `sprite::scaled`'s cache "only ever grows, nothing evicts an entry or bounds the cache's size"; both halves are wrong. `sprite::clear_cache()` drops all three caches and `theme::set_active()` calls it on every theme change (a production path, with the reasoning written at both ends), so the cache's lifetime is ONE THEME rather than the process. The size is bounded too, by a clamp at each call site: `round(13·zoom).max(9)` gives 9 keys and `clamp(w,6,14) × clamp(h,8,18)` gives at most 99. And the memoisation is load-bearing rather than incidental, since the module docs state `scaled` is REQUIRED at this project's GTK floor: handing GSK a correctly-sized texture is the only way pixel art stays crisp (`append_scaled_texture` is 4.10). Recorded here rather than nowhere because the entry read as a plausible latent leak for months, and the thing that refutes it is a function in a different module |

## Recommendation

**`D` is the largest thing left, and it is no longer unblocked.** It leads on severity, the
only High and the one entry a user feels as the application being broken rather than as a
detail being wrong, but [PLAN.profiling.md](PLAN.profiling.md) is now its stated
prerequisite (operator, 2026-08-28), so the order is **profiling, then D**. The reason is
the shape of D rather than its size: every mitigation it lists opens with "take several
samples", the one GTK key that would answer it is dark on this host, and an open-ended
budget spent on ad-hoc measurement is the failure mode. Build the instrument, agree what it
costs, then aim it.

`B` and `G` are the cheap standalone remainder and need none of that; `H` needs the
Mac seat's hands. `R` is a decision rather than a task and can be taken at any point.

**A note on what this ordering is not.** It is not a commitment to do them all: the
register is meant to *empty*, but several of these entries are correctly parked. A batch
that measures its way to "this is fine" has done its job and should delete its issues
rather than invent a fix — which is exactly what batch 5 did for both of its entries,
while the rig it built to reach that verdict earned the batch a fix nobody had filed.
