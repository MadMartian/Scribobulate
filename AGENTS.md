# AGENTS.md

Scribobulate: a native GTK4 Markdown viewer/editor for **Linux, macOS and
Windows** that renders on the CPU (zero GPU memory) and live-reloads files as
they change on disk. All three are supported build targets from one source tree;
Linux remains the canonical platform for the POLICY gates.

## SDD skill

This project uses Spec-Driven Development (SDD). If you have access to the SDD
skill, load it before taking any action on this project — it governs how to read,
write, and maintain all project documentation. If the skill is unavailable, read
the files in `sdd/` directly.

## The `gtk4-rs` skill

Scribobulate was built alongside the **`gtk4-rs` skill** — a standing GTK4/Rust
knowledge base of idiomatic patterns and the hard-won anti-patterns behind them.
It is **highly recommended (though not required)** when working here: `sdd/ANTI-PATTERNS.md`
is a compact index whose entries point into the skill for the full lesson, the dead
ends tried, and the GTK C-source tracing. Load it whenever you touch GTK, rendering,
layout, or scrolling code.

Refer to it **by name only, never by a filesystem path** — it may not be installed
on the machine this repo currently lives on, so a path would rot. If it is
unavailable, the `sdd/ANTI-PATTERNS.md` stubs (root cause + citations inline) plus
this repo's git history (which holds the original self-contained essays) carry
enough to proceed. The two registers' citation convention (`ScrAP-N` here vs `AP-N`
in the skill) is under **Task triggers** below.

## Documentation

Project documentation lives in the `sdd/` directory. Every rule and detail lives in
the document that owns it; this page only routes you there.

### If you are exploring this project

Read these files first:
- [`sdd/PRODUCT.md`](sdd/PRODUCT.md) — What this project is and why it exists
- [`sdd/TDD.md`](sdd/TDD.md) — Behavioral contract (test rubrics in Given/When/Then format)

To answer how the project works or how components connect or hand off, read
[`sdd/TECH.md`](sdd/TECH.md)'s module map first, before grepping source — it is a
curated index over the code, so reconstructing the same picture from source is
slower and misses the ownership and boundaries the map records. (When the SDD
skill is loaded this is already enforced; this line carries the rule for the
skill-unavailable fallback above.)

### If you are contributing to this project

Read all of the above, plus:
- [`sdd/POLICY.md`](sdd/POLICY.md) — Development rules, the build pipeline, and constraints you must follow. Authoritative for every rule: the other documents describe, this one prescribes.
- [`sdd/CAM.md`](sdd/CAM.md) — Change accountability matrices: completeness checklists for command-surface, markup/rendering, derived-view, reading-position-preservation, document-reference, deferred-operation and status-notice changes. POLICY makes satisfying every applicable cell binding; this holds the matrices. Read it before adding or altering a command, a markup/rendering feature, a surface that mirrors document state, anything that holds an offset or index into the document across time, anything that perturbs a text pane's geometry or buffer, anything whose completion lands later (any document read or write), or anything that pushes a transient status-bar notice.
- [`sdd/TECH.md`](sdd/TECH.md) — Architecture, dependencies, and module responsibilities. It leads with the embedded `sdd/system-overview.svg` — read the diagram first to orient.
- [`sdd/SCHEMA.md`](sdd/SCHEMA.md) — Exact shapes of what crosses the app's boundaries: the GAction interface, the CriticMarkup annotation storage format, the reading-theme file. TECH.md says where the boundaries fall; this says the structure of what crosses them.
- [`sdd/ANTI-PATTERNS.md`](sdd/ANTI-PATTERNS.md) — Register of the GTK4/Rust pitfalls already hit here, most pointing into the `gtk4-rs` skill for the full lesson. Read the table of contents first, then only the matching entries; scan it *before* troubleshooting, not after getting stuck.

### Additional documents (read when relevant)

- [`sdd/THEMING.md`](sdd/THEMING.md) — The reading-theme rules TECH.md's § Theme awareness points to: resolution order, the `themes.toml` search path and its XDG trap, the three mechanisms a key reaches the screen by, untrusted-input handling, and per-platform change detection. It also holds the **zoom** rules, because the theme/zoom boundary (disjoint CSS properties; the theme owns SCALE, never SIZE) is one invariant about both. Read it before changing any preview colour, typography, or decoration geometry — or anything about zoom.
- [`sdd/ISSUES.md`](sdd/ISSUES.md) — Known unresolved issues. A register: scan its table of contents the moment you hit a bug, before you start searching.
- [`tests/MANUAL-TEST.md`](tests/MANUAL-TEST.md) — the manual/GUI verification plan, for checks that need a running window and so cannot be made by `cargo test`. Read it before any manual/GUI verification, and whenever a change alters user-visible behaviour (POLICY build pipeline step 7). Its own header states how to run it — including its §A "Platform procedures", which is where every OS-specific command lives; on macOS or Windows read that before assuming a check that will not drive is a defect in the app.
- [`packaging/macos/README.md`](packaging/macos/README.md) — How the `.app` bundle is built and what still separates it from a redistributable. Read before touching `packaging/macos/`, or when an icon appears correct in-window but wrong in the Dock (they are two independent icon paths).
- [`packaging/windows/README.md`](packaging/windows/README.md) — how a Windows build and installer are produced, plus the two environment traps that make the toolchain look broken when it isn't. Read it before touching the Windows build pipeline.
- [`scripts/pipeline.steps`](scripts/pipeline.steps) — **the build pipeline is executable, and this file is the contract**: the ordered step list, each step's intent, its verdict rule, its class, and the per-platform command. Every platform's runner (`scripts/pipeline.sh`, `packaging/macos/pipeline.sh`, `packaging/windows/pipeline.ps1`) *derives* its step list from it and prints that list via `--list-steps`/`-ListSteps` so the ports can be diffed. **Read its header before writing or altering any platform's pipeline or packaging** — it carries why the contract pins each step's *intent* rather than its command, why a non-applicable step is declared in the run output, and why derivation rather than comparison is what proves the ports conform. POLICY § Build pipeline stays the authority on *why* each gate exists.
- [`sdd/PLAN.build-pipeline-ci.md`](sdd/PLAN.build-pipeline-ci.md) — deferred: making the cross-platform parity claim self-enforcing. The contract and all three runners exist, but nothing *fails* when they drift — the diff is run when someone remembers, and no single machine can execute all three ports. Read it before adding CI, and before trusting a parity claim that nothing checks.
- [`packaging/linux/`](packaging/linux/) — the `.deb` and `.rpm` builders. `payload.sh` defines what gets installed where **once**, and both packages read it; the per-format metadata is all that differs. Read it before changing what a Linux package contains, and add to `payload.sh` rather than to either builder.
- [`sdd/PLAN.accessibility.md`](sdd/PLAN.accessibility.md) — deferred: structural accessibility (roles/relations on tables, tab rows, sidebars, toasts) and the preview's self-drawn content (task checkboxes, list markers, annotation chips), which has no accessible object at all because it is painted rather than built. **Read it before adding any accessibility markup beyond a control name** — the plan records two GTK-4.6 floor constraints that decide the design rather than the estimate, and one architectural route (widgets at anchors) that is closed to this project. Control *naming* is already done and is not part of it: `src/a11y.rs` is the choke point, `clippy.toml` bans the bare tooltip setter, and TDD §16.7 is the contract.
- [`sdd/PLAN.profiling.md`](sdd/PLAN.profiling.md) — deferred: a CPU and memory profiling strategy. TDD §6 gates *ceilings* (VRAM, RSS) proven once as a viability spike; nothing asks whether a change made a main-loop turn slower or made something leak per cycle, which are the two failures a single-threaded GTK application actually produces. **Read it before profiling anything, before proposing a performance gate, and before reaching for any GTK debug channel** — it records, host-measured, that a distribution GTK has its entire introspection surface compiled out (ScrAP-251), including one channel that reports a healthy-looking `0` while dark, and it carries the tier ladder, the escalation order for leak attribution, and what is reachable with no change to the tree.
- [`sdd/PLAN.details-disclosure.md`](sdd/PLAN.details-disclosure.md) — **shelved** (2026-08-01): HTML `<details>`/`<summary>` collapsible disclosure blocks in the preview. Approved in principle but blocked on one unresolved measurement; nothing is implemented and no TDD rubrics were landed — the plan holds them drafted, for reinstatement on implementation. **Read it before implementing any collapsible or foldable preview construct, and before making a self-drawn affordance keyboard-activatable** — it records, researcher-verified, that a widget copying `GtkExpander`'s `set_activate_signal` gets Space unconditionally but Enter only until the window has a default widget (GTK4Rs/AP-165), that the RTL disclosure triangle is a separate icon name rather than a mirror, and that honouring `<details open>` from source while treating user toggles as ephemeral removes the per-fold-state-across-re-render problem rather than solving it.

### Task triggers

- **Writing or rewriting an anti-pattern citation** — an entry in `sdd/ANTI-PATTERNS.md` is cited `ScrAP-N`; one in the `gtk4-rs` skill is cited `GTK4Rs/AP-N`; **a bare `AP-N` is illegal** and `scripts/lint-references.sh` check 8 fails on it. Never bulk-rewrite a citation's prefix: the two registers number the same lessons differently (79 and 88 hold each other's), so a prefix-only sweep silently re-points citations at real-but-unrelated entries and no lint can see it — re-derive each number against the lesson, per site, and prefer `ScrAP-N` when both registers hold it. ScrAP-231 records what the laxer version of this rule cost. (Inside `sdd/ANTI-PATTERNS.md`'s own body a bare `#N` is the local shorthand for an entry *in that file*; everywhere else write `ScrAP-N` in full.)
- **Changing any preview colour, typography, or decoration geometry** (`palette.rs`, `tags.rs`, `preview/css.rs`, `theme.rs`) — read [THEMING.md](sdd/THEMING.md) and POLICY's "No hard-coded styling" and "One theme key, every application path" rules first.
- **Changing the architecture** — update `sdd/system-overview.svg` in the same change; see POLICY build pipeline step 8 for the validation gate.
- **Fetching anything over the network** — it goes through `src/imagefetch.rs`, and never through a `GFile`. Read that module's header first: `gio::File::for_uri("https://…")` looks dependency-free and resolves only where some backend claims the scheme, which is a Linux daemon (`gvfsd-http`), an in-DLL VFS on Windows (`GWinHttpVfs`), and **nothing on macOS** — so it is a whole feature that silently does not exist on one platform, reporting `NOT_SUPPORTED` that an `.ok()` then swallows (ScrAP-292). Replacing a toolkit transport also means inheriting its configuration surface — trust store, proxy, timeouts — so the client verifies against the machine's own store deliberately; do not "simplify" that to bundled roots. The rule is in POLICY § Architecture rules, the contract is [TDD 14.2](sdd/TDD.md), the check is `tests/MANUAL-TEST.md` §14.2a.
- **Touching crash/panic handling, the logging sink, or anything a crash report must survive** (`src/forensics/`, `src/logging.rs`) — read [TECH.md § Diagnostics and crash forensics](sdd/TECH.md#diagnostics-and-crash-forensics) first. Everything on that path is constrained by what a signal handler may do (no allocation, no locks), and the constraint is invisible from the call site. When investigating an unexplained crash of the installed build, start from the crash report in the state directory; ScrAP-204 is the method for resolving a frame from a kernel log alone. **No report at all is itself evidence** — it means the death was not one of the five signals `forensics::signal::FATAL_SIGNALS` takes, which is a shorter list than the ways a process can die and was one entry short until ScrAP-268 (the pointer here used to be to an ISSUES entry that no longer exists, which is why SDD principle 6 forbids the form).
- **Touching the crash-recovery snapshot write path** (`src/window/swap.rs`, `src/swapfile/`, `src/window/swaprecovery.rs`) — read ScrAP-232 first. `replace_contents_async` destroys the previous snapshot on an ordinary disk-full; the shipped write owns the promote (co-located temp, rename only after a complete write). The contract is [TDD §22](sdd/TDD.md), the format is [SCHEMA.md](sdd/SCHEMA.md) § "Crash-recovery swap file", the surfaces are [CAM.md](sdd/CAM.md) rows 8/10.
- **Renaming a file through GIO, or cancelling/re-attaching a `gio::FileMonitor`** — read the `src/docio/rename.rs` module header first. It records, source-traced, that **no GIO primitive refuses an existing rename destination atomically** (both `set_display_name` and `move_` are `g_lstat` + plain `g_rename`; Windows always passes `MOVEFILE_REPLACE_EXISTING`), so the shipped seam narrows a race rather than closing it; and that a rename of a watched file delivers **three** events on the old monitor on Linux/Windows and **two** on macOS/kqueue — which is why the save path's self-delete guard, consuming one, is insufficient and `g_file_monitor_cancel()` before the rename is the sole mechanism (ScrAP-269). Before trusting any name GIO hands back, note that neither the returned `GFile` nor a `query_info` reads the directory (ScrAP-270) and the `id::file` identity scan that fixes it is not unique per entry (ScrAP-271). The contract is [TDD §24](sdd/TDD.md), the checks are `tests/MANUAL-TEST.md` §24, the surfaces are [CAM.md](sdd/CAM.md) § Document-Identity, and the remaining lessons are ScrAP-272 and ScrAP-274.
- **Writing a GTK test** — the attribute is `#[gtktest::test]`, never `#[gtk::test]`. It is a drop-in, and it registers the body with both harnesses: libtest, and `src/gtk_suite.rs`'s main-thread run, which is the only one available where GTK initialises solely on the main thread. Choosing the old attribute is invisibly wrong (the test still passes on Linux while vanishing from the portable run), so `scripts/lint-references.sh` check 5 rejects it. A check whose assertion is *about* process-global GTK state — icon theme, `GtkSettings`, focus, the default display — needs its own `harness = false` target instead; see POLICY's testing section.
- **Adding a top-level module to `src/lib.rs`** — add it to `src/gtk_suite.rs`'s list too, or every test body inside it silently disappears from the main-thread suite. `lint-references.sh` check 4 is the gate.
- **Retiring a plan, or deleting/renaming any document** — the pointers to it are scattered across file types a sweep does not think of (`Cargo.toml`, `build.rs`, `data/*.xml`, the shell and PowerShell scripts), not just `.md` and `.rs`. Do not sweep by hand and declare it done: `lint-references.sh` check 6 resolves every referenced document path and is the only thing that can tell you the sweep is complete. Where a pointer was carrying a real fact, replace it with the fact or a `ScrAP-N`; a plan is deleted by design, so the citation has to become durable, not relocated.
