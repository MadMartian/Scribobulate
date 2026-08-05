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
- [`sdd/PLAN.build-pipelines.md`](sdd/PLAN.build-pipelines.md) — deferred: bringing all three platforms to a comparable gated pipeline and a redistributable installer. Only Windows currently has either; Linux's pipeline is prose in POLICY and its installer builds from source, and macOS has neither a pipeline nor a transferable artefact. **Read it before writing or altering any platform's build pipeline or packaging**, because the three variants are authored independently on three machines and the plan defines the alignment contract they must all satisfy — the point being that "the scripts look alike" is exactly the unverifiable claim this project has already been burned by.
- [`sdd/PLAN.accessibility.md`](sdd/PLAN.accessibility.md) — deferred: structural accessibility (roles/relations on tables, tab rows, sidebars, toasts) and the preview's self-drawn content (task checkboxes, list markers, annotation chips), which has no accessible object at all because it is painted rather than built. **Read it before adding any accessibility markup beyond a control name** — the plan records two GTK-4.6 floor constraints that decide the design rather than the estimate, and one architectural route (widgets at anchors) that is closed to this project. Control *naming* is already done and is not part of it: `src/a11y.rs` is the choke point, `clippy.toml` bans the bare tooltip setter, and TDD §16.7 is the contract.
- [`sdd/PLAN.profiling.md`](sdd/PLAN.profiling.md) — deferred: a CPU and memory profiling strategy. TDD §6 gates *ceilings* (VRAM, RSS) proven once as a viability spike; nothing asks whether a change made a main-loop turn slower or made something leak per cycle, which are the two failures a single-threaded GTK application actually produces. **Read it before profiling anything, before proposing a performance gate, and before reaching for any GTK debug channel** — it records, host-measured, that a distribution GTK has its entire introspection surface compiled out (ScrAP-251), including one channel that reports a healthy-looking `0` while dark, and it carries the tier ladder, the escalation order for leak attribution, and what is reachable with no change to the tree.

### Task triggers

- **Writing or rewriting an anti-pattern citation** — an entry in `sdd/ANTI-PATTERNS.md` is cited `ScrAP-N`; one in the `gtk4-rs` skill is cited `GTK4Rs/AP-N`; **a bare `AP-N` is illegal** and `scripts/lint-references.sh` check 8 fails on it. Never bulk-rewrite a citation's prefix: the two registers number the same lessons differently (79 and 88 hold each other's), so a prefix-only sweep silently re-points citations at real-but-unrelated entries and no lint can see it — re-derive each number against the lesson, per site, and prefer `ScrAP-N` when both registers hold it. ScrAP-231 records what the laxer version of this rule cost.
- **Changing any preview colour, typography, or decoration geometry** (`palette.rs`, `tags.rs`, `preview/css.rs`, `theme.rs`) — read [THEMING.md](sdd/THEMING.md) and POLICY's "No hard-coded styling" and "One theme key, every application path" rules first.
- **Changing the architecture** — update `sdd/system-overview.svg` in the same change; see POLICY build pipeline step 8 for the validation gate.
- **Touching crash/panic handling, the logging sink, or anything a crash report must survive** (`src/forensics/`, `src/logging.rs`) — read [TECH.md § Diagnostics and crash forensics](sdd/TECH.md#diagnostics-and-crash-forensics) first. Everything on that path is constrained by what a signal handler may do (no allocation, no locks), and the constraint is invisible from the call site. When investigating an unexplained crash of the installed build, start from the crash report in the state directory and ISSUES.md's SIGSEGV entry; ScrAP-204 is the method for resolving a frame from a kernel log alone.
- **Touching the crash-recovery snapshot write path** (`src/window/swap.rs`, `src/swapfile/`, `src/window/swaprecovery.rs`) — read ScrAP-232 first. `replace_contents_async` destroys the previous snapshot on an ordinary disk-full; the shipped write owns the promote (co-located temp, rename only after a complete write). The contract is [TDD §22](sdd/TDD.md), the format is [SCHEMA.md](sdd/SCHEMA.md) § "Crash-recovery swap file", the surfaces are [CAM.md](sdd/CAM.md) rows 8/10.
- **Writing a GTK test** — the attribute is `#[gtktest::test]`, never `#[gtk::test]`. It is a drop-in, and it registers the body with both harnesses: libtest, and `src/gtk_suite.rs`'s main-thread run, which is the only one available where GTK initialises solely on the main thread. Choosing the old attribute is invisibly wrong (the test still passes on Linux while vanishing from the portable run), so `scripts/lint-references.sh` check 5 rejects it. A check whose assertion is *about* process-global GTK state — icon theme, `GtkSettings`, focus, the default display — needs its own `harness = false` target instead; see POLICY's testing section.
- **Adding a top-level module to `src/lib.rs`** — add it to `src/gtk_suite.rs`'s list too, or every test body inside it silently disappears from the main-thread suite. `lint-references.sh` check 4 is the gate.
- **Retiring a plan, or deleting/renaming any document** — the pointers to it are scattered across file types a sweep does not think of (`Cargo.toml`, `build.rs`, `data/*.xml`, the shell and PowerShell scripts), not just `.md` and `.rs`. Do not sweep by hand and declare it done: `lint-references.sh` check 6 resolves every referenced document path and is the only thing that can tell you the sweep is complete. Where a pointer was carrying a real fact, replace it with the fact or a `ScrAP-N`; a plan is deleted by design, so the citation has to become durable, not relocated.
- **Citing an anti-pattern** — this project's register (`sdd/ANTI-PATTERNS.md`) is cited as **`ScrAP-N`**; the `gtk4-rs` skill's as **`AP-N`** (or `skill AP-N` where extra clarity helps). The prefix is the whole disambiguation: `ScrAP-` is unique, so it means the same thing in a code comment, in `sdd/`, and inside the register itself. Always use it for a new citation. (Inside `sdd/ANTI-PATTERNS.md`'s own body a bare `#N` is the local shorthand for an entry *in that file*; everywhere else write `ScrAP-N` in full.)
