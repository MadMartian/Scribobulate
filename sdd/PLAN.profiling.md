# Plan: CPU and memory profiling

## Problem

The project has a **footprint gate** and no **profiling strategy**, and the two are
not the same instrument. TDD §6 asks *"is this stack viable?"* — VRAM under 50 MiB,
RSS bounded — and that question was answered once, affirmatively, by a viability
spike. Nothing in the tree asks the question a maturing application actually needs
answered: *"did this change make something slower, or make something leak?"*

The gap is not theoretical. This architecture produces two characteristic failures,
and neither has an oracle:

- **A main-loop turn that overruns.** The application owns no threads, so every
  render, parse and highlight runs on the GTK main thread; a turn that takes too
  long *is* a frozen window. This has already happened once in a shipped path — a
  synchronous startup burst of menu-model construction froze the UI for seconds
  (ScrAP-61) — and the general shape is a known GTK trap (GTK4Rs/AP-148: a
  ~150 ms synchronous render leaves even a spinner frozen, because the frame clock
  is a main-loop source a blocked turn never lets dispatch). TDD 1.7 and 1.4c
  encode the contract as *"does not freeze"*, which is a judgement a human makes by
  looking, not a number anything can regress against.
- **A spin at idle.** A large document currently pegs a core at ~100% forever after
  it has finished rendering — measured, reproducible, not platform-specific, and
  **still undiagnosed**. It sits in the debt register as high severity.

Memory is in a different but related state: the knowledge exists and is good, but it
is *scattered* — the toolkit-noise filter, the strand-on-close leak, the per-render
`Rc`-cycle leak and its symbol-free attribution method, the RSS checks in the manual
plan, the sanctioned `valgrind` invocation. There is no recorded **escalation
order**, so every investigation re-derives one, and the cost of getting that wrong is
on record: a previous hunt burned ~72 minutes on a debug-symbol source that was never
going to deliver (ScrAP-141).

Two consequences follow. Performance regressions are invisible until a human notices
one, and the open CPU defect has stalled — in part on an instrument that does not
work here at all, which is the second root cause below.

### Root cause

**1. The footprint gate is a ceiling, not a budget.** A ceiling catches a
catastrophe and is blind to a slope. A render that grows from 40 ms to 150 ms never
approaches the VRAM limit and never touches the RSS ceiling, yet it is exactly the
regression that makes the application feel broken. Ceilings answer *viability*;
budgets and slopes answer *regression*. Only the first was ever written.

**2. The toolkit's own introspection is compiled out of a distribution GTK**
(ScrAP-251, measured on the reference host). Every informational `GTK_DEBUG` /
`GDK_DEBUG` / `GSK_DEBUG` key reports `[unavailable]`; `g_type_get_instance_count()`
is exported and silently returns 0 forever; neither `libgtk-4` nor `libglib-2.0`
links `libsysprof-capture`. So the instruments a GTK profiling plan would naturally
name are all dark — and one of them returns *the number a healthy process returns*.

That second cause has already contaminated the open work: the CPU-spin
investigation's recommended first instrument is a `GTK_DEBUG` key to name the
subtree that re-queues a resize each frame. It cannot run on the reference host as
written, and its empty output reads as *"nothing re-queues resize"* — a false
negative banked into an open, high-severity investigation.

## Measured constraints (reference host, 2026-08-04)

Established with a throwaway C rig against the distribution libraries, so none of it
depends on this application. Ubuntu jammy, **GTK 4.6.9 / GLib 2.72.4**, Xvfb,
`GSK_RENDERER=cairo`.

| Fact | Consequence for this plan |
|---|---|
| All informational `GTK_DEBUG`/`GDK_DEBUG`/`GSK_DEBUG` keys `[unavailable]`; only `interactive` (the Inspector) survives. Warns once on stderr, then runs normally | No verification tier may rest on a debug key. The Inspector is still useful for widget tree / CSS / actions |
| `g_type_get_instance_count()` exported, returns **0** always — verified across ±100 live `GtkLabel`s | Per-GType live counts, **and the Inspector's Statistics tab**, are unusable. Worse than unusable: 0 is a false all-clear |
| No `libsysprof-capture` linkage in either library; no `sysprof` installed | The GTK-native profiling story (frame/layout marks) does not exist here |
| `GdkFrameClock` is ordinary API, not debug instrumentation — `frame_time` returns real monotonic µs (~16–17 ms apart under Xvfb) | **This is the one live toolkit-side signal**, and the foundation for a latency oracle |
| `presentation_time` is `0` under Xvfb (no presentation feedback) | Build the oracle on `frame_time`; any presentation-timing check belongs on the real session |
| `perf` present; `perf_event_paranoid=1` → unprivileged per-process sampling works | External sampling needs no privilege escalation and no host changes |
| `valgrind` present. `heaptrack`, `sysprof`, `samply`, `hotspot` **absent** | The allocation-attribution ladder must be built from valgrind + bespoke tooling |
| Release binary is `strip = "symbols"` — **no symbol table, no `.debug` sections** | Sampling the release build yields addresses, not app function names |
| An unstripped `target/debug` binary exists | Named app frames are available today — unoptimized, so structurally informative and numerically meaningless |
| GTK/GLib debug symbols unobtainable from both official sources for this exact version (ScrAP-141) | Frames inside the toolkit resolve to `lib+offset` at best, on every tier |

## The four failure classes

Framing the strategy around tools produces a tool list; framing it around failures
produces a gate. Everything below is organised by what can go wrong.

| | Class | What it looks like | Existing evidence |
|---|---|---|---|
| **C1** | **Stall** — a single main-loop turn overruns | The window freezes, a spinner stops, input queues | ScrAP-61; GTK4Rs/AP-148; TDD 1.7, 1.4c |
| **C2** | **Spin** — non-zero CPU with no work to do | A core pegged at idle; battery drain | The open large-document spin |
| **C3** | **Throughput** — a core path got slower | Everything feels heavier; no single freeze | Nothing today |
| **C4** | **Growth** — RSS climbs per cycle | Long sessions bloat; eventual OOM | ScrAP-60, ScrAP-155; TDD 6.3; manual §6.3, §8.6 |

C1 and C2 are the classes this architecture is *structurally* prone to, because it is
single-threaded by design. They are also the two with no oracle at all.

## Possible approaches

### 1. A cheapest-first tier ladder with explicit escalation

Four tiers, each with an entry criterion that says when to climb to the next:
**T0** application-owned instrumentation (frame-clock deltas, phase timers) → **T1**
display-free benchmarks on the pure cores → **T2** process-external sampling
(`perf`) → **T3** allocation attribution (RSS slope → weak-ref guard → massif →
`LD_PRELOAD` GType interposer).

**Pros**: each class gets an owning tier; the cheap tiers catch most regressions and
the expensive ones are reached deliberately rather than by panic. T0 is the only tier
that survives the trip to macOS and Windows, which matters for a tri-platform
project. The escalation order is the thing that was missing, and writing it down is
most of the value.
**Cons**: T0 and T1 are code, so the ladder is not free; and a ladder is only real if
its rungs are actually built rather than described.

### 2. External tools only — change nothing in the tree

Run `perf` and `valgrind` against the app as it ships; keep RSS checks in the manual
plan; write no instrumentation and no benchmarks.

**Pros**: zero cost, available today, and genuinely sufficient for *investigating* a
known defect. Nothing to maintain.
**Cons**: cannot gate anything. Every check is a human remembering to run it, which
is the state that produced this plan. Also strictly Linux — `perf` does not travel to
the other two platforms, so the classes most likely to differ per platform stay
unmeasured there. And against the stripped release build it yields library-level
attribution only.

### 3. Restore the toolkit's introspection first (instrumented-GTK sidecar)

Build GTK 4.6.x locally with debug enabled and load it ahead of the distribution
library, unlocking the geometry / size-request keys and the instance counts.

**Pros**: the *only* route to the geometry key the spin investigation wants, and it
would answer "which subtree re-queues a resize" directly rather than by inference.
**Cons**: a real build to own and keep in step; it changes the library under test, so
timing numbers taken against it are not the shipping configuration's numbers; and it
is Linux-only. Best understood as a **diagnostic sidecar for one investigation**, not
a tier of a standing strategy.

### 4. Do nothing — keep the ceiling gate

**Pros**: honest if the conclusion is that a document viewer's performance is
self-evident from using it.
**Cons**: leaves a high-severity defect without an instrument, and leaves C1/C3
regressions to be discovered by users. The freeze that already shipped (ScrAP-61) is
the counter-example.

## Recommendation

**Approach 1, phased, with approach 3 held in reserve for the spin investigation
specifically.**

Phase 0 is deliberately first because it needs no code and unblocks the open defect.

| Phase | Work | Classes | Code? |
|---|---|---|---|
| **0** | Use what exists: `perf` against the unstripped debug binary for the spin's *shape*; RSS-slope runs for growth; the Inspector for tree/CSS questions | C2, C4 | **No** |
| **1** | **T0** — a `profiling` feature carrying a frame-clock stall watchdog and phase timers, routed through the existing forensic log sink | C1, C2 | Yes |
| **2** | **T1** — display-free benchmarks on the pure cores (parse, render, format, copy-as-Markdown, outline, annotations, swap codec) | C3 | Yes |
| **3** | **T2 proper** — a `[profile.profiling]` Cargo profile (release settings, `debug` on, `strip` off) so sampling names app frames at release optimisation | C1–C3 | Manifest only |
| **on demand** | **T3** — the allocation ladder, in order | C4 | Partly |

Two points decide the ordering. First, **T0 before T2**: instrumentation the
application owns is the only tier that answers the same question on all three
platforms, and this project's policy makes Linux the gate but not the only target.
Second, **T3 is an ordered ladder, not a menu** — RSS slope across *scaled* cycle
counts first, because it is free and it is the step that decides whether there is
anything to attribute at all; the interposer last, because it is a tool to write.

Approach 3 stays recorded and unrecommended as a *standing* tier. If the spin
investigation exhausts sampling without naming the driver, it becomes the next step
for that investigation alone — and the researcher question below should be answered
before anyone starts the build.

## What is reachable without touching the tree

Recorded explicitly because it is the first thing anyone will ask, and because
Phase 0 depends on it being accurate.

| Available today | Answers | Caveat |
|---|---|---|
| RSS slope across scaled cycle counts | C4 | None — purely external; already the shape of manual §6.3 and §8.6 |
| `perf` against the **release** binary | C1–C3, coarsely | Library-level attribution only (`lib+offset`); this is how the spin's existing stack sample was taken |
| `perf` against the existing **debug** binary | C1–C3, structurally | Named app frames, but unoptimized — answers *what* is looping, never *how fast* anything is |
| `valgrind` / massif | C4 | Already sanctioned in POLICY § Optional diagnostics; filter every stack for an application frame first (ScrAP-49) |
| GtkInspector (`GTK_DEBUG=interactive`) | widget tree, CSS, actions | The one surviving toolkit channel. **Its Statistics tab is dead** (ScrAP-251) — never read object counts from it |
| The `LD_PRELOAD` GType interposer (ScrAP-155) | C4, by GType and callsite | No application change, but it is a tool to write. Now the *primary* attribution route, since instance counting is dark |

The split is not tier-by-tier: **the diagnostic rungs are largely change-free, and
every regression-gate rung is code.** That follows from what each is for — an
investigation can start today; a budget cannot be locked in without writing
something.

## Sequencing and verification

- **Rubrics before code — the plan-kickoff stop applies.** C1 and C2 need TDD
  rubrics carrying actual numbers (a maximum main-loop turn; an idle-CPU ceiling)
  before any of Phase 1 is written. Today's *"does not freeze"* wording cannot be
  regressed against, and rubrics written after the instrumentation exists will
  describe whatever it happens to measure.
- **A new TDD section ships with its `tests/MANUAL-TEST.md` section in the same
  change** (POLICY). The runbook extends §1.8, which today covers only the VRAM and
  RSS ceilings.
- **Comparability rules are part of the gate, not hygiene.** A pinned fixture corpus
  and a scripted scenario; a release-derived build only; **scale the cycle count and
  read the slope — never trust an absolute** (one-time initialisation noise is flat
  across counts while a real leak grows with them, ScrAP-155); one variable per run;
  never compare across renderer or feature flags.
- **Xvfb versus the real session.** Slopes, allocation counts and structural
  sampling are sound under Xvfb. Anything paint-, compositor- or GPU-dependent is
  not — `presentation_time` reads 0 there, and the footprint gate already requires
  the real session. `tests/MANUAL-TEST.md` §1.10 governs the choice.
- **Prove an instrument emits before trusting its silence** (ScrAP-251). Every
  channel added or relied on gets one positive-control run against a condition known
  to be present.
- **Where this lands in the documents:** POLICY gains *when profiling is owed, the
  tiers, and the escalation order*; TDD gains the C1/C2 rubrics; `tests/MANUAL-TEST.md`
  gains the runbook; ANTI-PATTERNS already carries ScrAP-251.

## Open decisions

1. **The budgets.** A maximum main-loop turn and an idle-CPU ceiling, as TDD
   rubrics. Either the operator sets them, or they are derived by measuring the
   current build and rounding to a defensible margin — but they must exist before
   Phase 1, because they are what the instrumentation is *for*.
2. **New development dependencies** for T1 (a benchmark harness, an allocation
   profiler). POLICY requires dependency justification; this is an operator
   decision, not an implementation detail. Both would be `[dev-dependencies]`,
   linked into no shipping artefact.
3. **The instrumented-GTK sidecar** — worth a researcher round before anyone builds
   it: the cheapest reliable way to obtain a debug-enabled GTK 4.6.x, and whether
   the debug build flag alone restores *all* the informational keys or only some.
   The answer decides whether approach 3 is a half-day or a multi-day commitment.

## Technical details preserved

- **The probe rig.** A ~15-line C program linked against the distribution GTK
  (`pkg-config --cflags --libs gtk4`), run under `xvfb-run`, established every fact
  in the constraints table: allocate 100 labels around a `g_type_get_instance_count()`
  call to prove the counter is dark; `GTK_DEBUG=help` / `GDK_DEBUG=help` /
  `GSK_DEBUG=help` to enumerate availability; a tick callback reading
  `GdkFrameTimings` to prove the frame clock is live and to observe
  `presentation_time = 0`. Reproducing it costs minutes and it depends on nothing in
  this repository — rebuild it rather than trusting this table on a different host or
  a different GTK.
- **Confirmed dead ends — do not re-walk.** Distribution debug symbols for this
  exact GTK/GLib version from either official source (ScrAP-141, ~72 minutes lost);
  `g_type_get_instance_count()` and the Inspector Statistics tab; sysprof marks; any
  informational `GTK_DEBUG`/`GDK_DEBUG`/`GSK_DEBUG` key (ScrAP-251).
- **The stripping consequence is exact.** The release binary has no symbol table and
  no `.debug` sections, and no separate debuginfo file is retained, so there is
  nothing for a sampler to resolve against — not merely inconvenient, *impossible*
  short of the manifest change in Phase 3. Note also that addresses from a stripped
  release build cannot be resolved against a separately-built debug binary: different
  codegen, different addresses.
- **The allocation ladder's order, and why.** RSS slope first (free, and decides
  whether anything is wrong); then a weak-reference finalization test (turns a found
  leak into a permanent guard, and must hold **no** strong reference to the subtree
  itself or it masks the very leak it checks — ScrAP-155); then massif with the
  application-frame filter (ScrAP-49); then the interposer, which tags each
  allocation with the GType under construction and its creation backtrace, keys on
  `lib+offset` because address-space randomisation makes absolute frames
  incomparable across runs, and confirms a leak by scaling the cycle count rather
  than by absolute totals.
- **The frame-clock oracle's shape.** Deltas between successive `frame_time` values
  bound how long the main loop was unavailable, which is the C1 measurement; a
  sustained stream of frames with no input is the C2 signal. Both are ordinary API
  on every platform, which is why T0 travels where `perf` does not.
- **Contention is a profiling subject too.** Document I/O shares one process-wide
  ten-thread GLib pool with the crash-recovery snapshot writer, and the application
  caps its own use of it for that reason (ScrAP-243). A profiling run that saturates
  that pool is measuring the cap, not the code — hold the scenario's I/O
  concurrency fixed, and treat a latency change there as a pool-occupancy question
  before treating it as a code regression.
