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
  now measured rather than only designed: 147/147 suite cases pass, and all three
  standalone targets pass.
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
5. `xvfb-run -a cargo test --features gtk-integration-tests` — all tests must pass.
   Needs a display; `xvfb-run` supplies one headlessly, and these tests present real
   windows and pump the frame clock. Do NOT skip this step when no display is handy:
   skipping is how step 2's failure mode arises.
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
6. **Coverage gate** — `scripts/coverage.sh` must pass. Scoped line coverage is a
   no-regression **ratchet**, not a target: the script owns both the floor (`FLOOR`)
   and the scope (`IGNORE`), and is the only place either is written down. **Do not
   restate either value here** — a second copy is exactly how the floor silently fell
   ~2pt behind the code and stopped protecting it. When new tests raise coverage, raise
   `FLOOR` in the script in the same change; the aspiration is 80%.
   **Scope rule:** GTK signal-wiring that cannot be exercised headlessly is excluded;
   pure decision logic is always in. So **when adding logic to an excluded file, extract
   the decision core into its own logic module** (as `winstate` does) rather than letting
   it hide behind the exclusion — that extraction is the mechanism by which the floor
   rises. The excluded set and its per-module rationale live beside the regex, in the
   script.
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
9. **Cross-reference gate** — `scripts/lint-references.sh` must pass (Windows: the
   equivalent `scripts/lint-references.ps1`, which `packaging/windows/pipeline.ps1`
   runs as this same step). **A gate is its pattern *and* the set of files it runs
   over, and parity is required in both.** The two scripts share (a) one pattern and
   one `--self-test`/`-SelfTest` corpus, string-for-string, and (b) one file
   enumeration, `scripts/lint-references.scan`, which both read rather than restate —
   *one* enumeration, which **every** check consumes and which `--list-scan` prints
   exactly, ordinal-sorted so a clean parity diff is an empty one. The contract's
   `maxdepth` is a tripwire, not a filter: a file past the budget makes both gates
   refuse to run and name it, because a budget that silently truncated the set would
   make a check leniently incomplete without saying so.
   Neither half is optional, and (b) is here because it was learned the hard way: this
   step once claimed the shared corpus alone meant "neither can drift into being the
   lenient one", while the corpus pinned only the check-1 regex and the two scripts had
   *already* drifted on enumeration — `.agents/`, `docs/` and `THIRD-PARTY-LICENSES.md`
   were linted on Linux and invisible on Windows, so a dangling link in any of them
   failed one gate and passed the other. **No automated test can compare the two**
   (neither platform has the other's shell), so when either script's scanning changes,
   run `--list-scan` / `-ListScan` on both platforms and diff the output. A claim of
   parity that nothing checks is worse than no claim, because the next author trusts
   it. It enforces
   eight rules mechanically — three over citations into the SDD registers, two over
   the test architecture, both of whose failure modes are silent (that
   `src/gtk_suite.rs`'s duplicated module list has not drifted from `src/lib.rs`'s — a
   module missing there drops every test body inside it from the main-thread run, with
   nothing failing — and that `#[gtk::test]` has not returned in place of
   `#[gtktest::test]`, check 5, nor been PRESCRIBED in the documents a developer acts
   on, check 5b — a lint's input set is source, so until 5b existed nothing in the
   toolchain could read the prose telling someone to write the banned attribute, and
   this file did exactly that for as long as check 5 had existed, ScrAP-222), and one
   over document paths: every file the tree points at must
   exist. That last is what a plan retirement breaks, since a `PLAN.*.md` is deleted by
   design once implemented and every pointer written while it existed dangles at once —
   including the bare `PLAN.<topic>` **section** citations code comments actually write
   (`PLAN.<topic> D3`, no `.md`), which is the form that let 21 danglers survive a sweep
   that believed itself complete. It deliberately ignores a bare document name used as a
   *mention* in prose, which resolves against nothing. The seventh is over the
   **application ID**: `src/icons.rs` is its source of truth and Rust derives it from
   there, but the desktop entry, GResource manifest, `Info.plist` template and the
   install/uninstall scripts each restate the literal, and a change to one of them
   fails no build while breaking a different platform's icon or Launch Services
   registration. The eighth is over the **citation FORM**, and it is the one rule here
   that bans a spelling rather than checking a target: an entry in
   `sdd/ANTI-PATTERNS.md` is cited `ScrAP-N`, one in the `gtk4-rs` skill is cited
   `GTK4Rs/AP-N`, and **a bare `AP-N` is illegal anywhere in the tree** (check 8).
   Illegal, not "means the skill" — it was this project's spelling historically and the
   skill's later, so its correct and incorrect uses are textually identical and no
   reader can tell a deliberate citation from one a sweep missed. Both legal forms are
   deliberately **single tokens**: a two-word form is split by any Markdown or
   `rustfmt` wrap, and since a `GTK4Rs/AP-N` can only ever be checked for *form* (the
   skill need not be installed), its whole value is that a grep can **enumerate** the
   set a human must audit — which a wrapped citation silently drops. When a lesson is
   held by both registers, cite `ScrAP-N`; this one is always resolvable. ScrAP-231
   records what the previous, laxer version of this rule cost. Checks 4, 5, 6, 7 and 8
   were each mutation-tested when written.
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
   are right or wrong. The script owns the check definitions and the
   PLAN exclusion; do not restate them here. **A PASS does not mean the citations are
   correct** — check 2 proves an entry exists, never that it is the right one; a real
   number naming the wrong lesson passes. That residue is a review obligation, and
   the reason it arises is documented in the script.

Do not skip any step. If `clippy` emits a warning, fix it — do not suppress it
with `#[allow(...)]` unless there is a documented reason in a comment on the same
line. A `disallowed-methods` rejection is **not** in that category: it is a routing
instruction with its own rule — see § "Typed GTK seams".

**Run the pipeline after every change, not just at session end.** Treat it as
part of completing each task: write code → fmt → clippy → build → test → done.
Do not report a task complete until every step passes. Running these steps only at
cleanup time lets broken changes pile up, making it harder to attribute which
change introduced the problem.

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
  **Never `#[gtk::test]`** — it is superseded, `lint-references.sh` check 5
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
run. `lint-references.sh` check 5 therefore rejects the attribute outright — a lint is
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
platform-specific items rather than assuming parity.

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
  command's accelerator is declared **once** in its descriptor (`Cmd`/`FmtCmd`) and
  registered from there via `set_accels_for_action`; the *same* string drives the
  menu/context-menu accel **hint**, so the active binding and its displayed hint can
  never diverge. This holds for both the editor/view actions and the formatting
  actions. Adding or changing a command — or its shortcut — means editing its single
  descriptor, not each surface and not a separate accel-registration list.
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
  exempting geometry would make this rule mean less than it says. Bounds: a theme
  sets the *metric of a decoration*; it does not change *what is drawn* or *how
  layout is computed* (a bounded/centred "measure" is layout behaviour, and out).
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

## SDD register writes

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

## Prohibited actions

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
becomes a hard failure instead of a silent log line.

Log messages must be self-contained — an agent reading them should understand
what the application was doing without needing to cross-reference source code.
Include the file path, event type, and relevant state in every message where
applicable.

Temporary debug output added during development must be prefixed with
`[🐛DEBUG]` and removed before committing. Use `eprintln!` for ad-hoc output
when the GLib log machinery is not yet initialised (e.g., before `Application`
is built).
