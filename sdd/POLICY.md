# Development Policy

The operator's deliberate rules for working on this project. Lessons learned the hard
way belong in [ANTI-PATTERNS.md](ANTI-PATTERNS.md); procedures belong to the script or
test plan that performs them; architecture is described in [TECH.md](TECH.md). Do not add
a rule here on your own initiative.

## Build

- Rust 2021 toolchain with the GTK4 and GtkSourceView 5 development libraries. The
  oldest supported GTK is 4.6.
- Linux, macOS and Windows are all first-class. Each is built natively on its own
  platform — macOS cannot be cross-compiled. Platform setup is in
  `packaging/<os>/README.md`.
- Linux is the canonical platform for the gates. Never weaken a shared gate — a lint, a
  test, a coverage floor — to make another platform pass.
- Release builds are the reference for behaviour and footprint.
- Every clone runs `git config core.hooksPath scripts/git-hooks` once. Its pre-push hook
  runs the pipeline's format and lint steps on the commits being pushed (not the working
  tree) and refuses a push that fails them.

## Build pipeline

After every change lands run formatting, lint and any affected tests. Then before signing off on a branch, 
in preparation for merging to master, run the following, typically (but not always) at session end:
- **Linux**: `scripts/pipeline.sh`
- **macOS**: `packaging/macos/pipeline.sh`
- **Windows**: `packaging/windows/pipeline.ps1`
A task is not complete until every step passes, and the remote Github CI is the final ratifying gate for all three platforms.

The steps are the contract's, `scripts/pipeline.steps`; `scripts/pipeline.sh --list-steps`
prints them. What the contract cannot enforce, and this document owns:
- Clippy runs with every feature enabled and zero warnings; an `#[allow]` carries its
  reason in a comment on the same line.
- Coverage (Linux) is a no-regression ratchet, not a target. Floors are whole numbers,
  and one drops only in a change that raises the other. Fractional drift is noise: do
  not report it or adjust for it. Logic added to a file excluded from coverage (GTK
  wiring) is extracted into a pure module so it is measured.
- A change to user-visible behaviour updates `tests/MANUAL-TEST.md` in the same change,
  and the TDD rubric too if the contract itself changed.
- A change to the architecture updates the system diagram in the same change.
- The installer step is the one opt-in step.
- Linux runs the GTK tests on a throwaway display and session bus; macOS and Windows run
  them against their real windowing system.

- The step list lives in one contract file that every runner and the CI workflow derive
  from. Change a step there — never in a runner, and never by naming steps in CI.
- Never skip a step. A step that does not apply on a platform is announced in the run
  output, never silently omitted.
- **A number has one owner.** A floor, limit, count or version is written in exactly one
  place and cited from nowhere else. Usually that place is the code or script enforcing
  it; where nothing enforces it mechanically — a product commitment, a limit a person
  checks before an edit — this document is the owner. Either way the ban is on the
  SECOND copy, which goes stale silently while reading as current.

Clean-up any leftover processes from test runs after testing is completed or before signing-off on a task or during debriefing.

## Continuous integration

- CI invokes the platform runners whole and names no step, so adding a step never means
  editing the workflow.
- A packaging job verifies its artefact as a file — it exists, has a plausible size and
  carries the right version — not by the packaging tool's exit code.
- A workflow runs against the shared repository whichever branch it sits on, so its
  triggers are the operator's decision.
- The remote Github pipeline is slow, do not rely upon it when targeted / surgical testing suffices for verification of a change mid-task.

## Third-party attribution

- Every published artefact carries the licence notices its dependencies require:
  statically linked code on every platform, a bundled GTK runtime wherever one is
  bundled.
- The About dialog promises those notices are in the distribution — removing a staging
  step falsifies that claim.
- Which licence covers which binary is a determination we make, not something a gate
  can derive. Where it is unmade, say so rather than letting a green gate imply it.

## Artefact signing

No artefact is signed with a trusted identity yet; obtaining one is the operator's
decision. Until then the tool that builds an artefact announces the limitation on
success, and that announcement is removed in the same change that introduces signing.

## Testing

### Unit tests

- Pure logic gets unit tests. Prefer extracting a decision core and unit-testing it over
  writing a GTK test.
- Never `#[cfg]` a test away by platform. Skip at runtime and print
  `SKIPPED [<rubric>]: <reason>`.
- A test that installs process-global state restores it before returning.
- A new gate or guard is mutation-tested: break what it protects and watch it fail.
- After any change to the XInclude screen (`src/imagedecode/xinclude.rs`), and in a
  review round of it, run the differential test's long run on each platform as well
  as the pipeline's bounded one (TDD 2.23d–2.23i):
  `SCRIBOBULATE_XINCLUDE_DIFFERENTIAL_CASES=1000000 cargo test --release --lib
  imagedecode::differential::the_screen -- --nocapture`. It draws a fresh seed and
  prints it; `SCRIBOBULATE_XINCLUDE_DIFFERENTIAL_SEED` replays one. A leak it finds
  becomes a fixed case in `xinclude/tests.rs` and a "Leaks found and closed" line in
  ScrSEC-1, in the fixing commit.

### GTK-object integration tests

- The exception, for behaviour that is genuinely about live widget or signal state.
- They sit in their owning module behind the integration-test feature, and helpers used
  only by them carry the same gate.
- Use `#[gtktest::test]` — never `#[gtk::test]` or a manual `gtk::init()` — so the test
  also runs where GTK is main-thread-only.
- A new top-level module in `src/lib.rs` is also added to `src/gtk_suite.rs`'s list, or
  its GTK tests silently drop out of the main-thread run.
- Fail loudly when there is no display; never pass vacuously.
- A test that asserts on process-global GTK state gets its own standalone target, says
  why in its doc comment, and is added to the macOS integration step, which runs an
  enumerated list.

### Per-render memory-growth class

A separate test class and pipeline step, outside the coverage ratchet. It bounds memory
growth per render after warm-up — never absolute memory, and never a single
render-free-measure, which a correct implementation cannot pass.

### Manual integration testing

- Integration tests trace back to TDD rubrics; one that maps to no rubric is probably a
  unit test.
- **Every fix or behaviour change is proved against the running app** — per fix, by
  driving the exact scenario and reading the after-state. Passing tests are not
  sufficient. The procedure is in `tests/MANUAL-TEST.md`.
- **A regression fix needs both** an automated test and a manual-test check that was
  actually run. One without the other is incomplete.
- A new TDD section lands together with its manual-test section.

### Footprint verification

**VRAM ceiling: 50 MiB on every platform — hard limit, no exceptions.** It is the
project's reason to exist.

- Measure on a release build after any change to a rendering dependency, a rendering
  path, the process model, or how the renderer is selected. How to read the measurement
  on each platform is in TDD §6.
- Over the ceiling: stop, revert, pivot. Never keep the number down in a contrived
  measurement while it climbs in realistic use.

## Cross-platform by default

- Write every line as portable unless it lives in a platform seam.
- Never hardcode a path separator, path shape or filesystem root, **test fixtures
  included**. A fixture that encodes one platform's path grammar is selected per
  platform.
- Line endings are a property of the document, not the host. Never branch on the
  platform to decide what a line separator is.

### Platform seams

- Code that exists only because one platform lacks something lives in that platform's
  module under the platform directory, `#[cfg]`-gated at the module declaration and
  never inside shared code. Nothing else calls an OS API directly.
- A seam supplies plumbing — a source or a transport — feeding machinery all platforms
  share. It never owns application behaviour.
- Group a platform's code by the cause it answers, not by the API it happens to call.
- Prefer hand-rolled FFI over a new binding crate for a handful of calls.

### Verifying on every platform
- Verify changes on each of the three platforms, locally in your environment, before merging your branch
- The pipeline is not sufficient to test changes that require a **real user desktop** rather than a display (other applications
  running, real input devices and input methods, clipboard readers, focus and
  fullscreen behaviour), a look at the rendered UI, interactive diagnosis (sampling,
  bisecting, probes, many reruns), or timing on real hardware. The runners do have a
  display and run real windows; what they lack is everything a person's machine has
  around the application.

### When a UI change needs a check on macOS or Windows, and when it does not

**Moving existing widgets around does not re-open a ratified feature.** The one thing a
rearrangement reliably breaks is machine-gated on the Linux host:
`window::gtk_integration_tests::no_chrome_sets_the_windows_width_floor_above_the_backstop`
asserts the window's minimum is exactly `MIN_WINDOW_WIDTH` with every toolbar section
shown, both sidebars open, the find bar open with its replace row, and a document whose
headings stretch anything that stretches. A control that raises the floor fails there.
That is worth more than a look on another platform, because the macOS failure mode is silent — the
window is **not** grown to meet a risen minimum, so the control is simply not drawn (TDD
9.38). Accessibility naming is gated the same way: `clippy.toml` bans the bare tooltip
setter, so a control not named through `a11y::` fails the build.

So a pure rearrangement needs a green pipeline and nothing else. **These are not
rearrangements, and each owes a narrow check** — narrow, not a repeat of a full pass:

| Change | Check on | Why a Linux run cannot answer it |
|---|---|---|
| A new glyph or non-ASCII character in a label | macOS and Windows | Different font stacks. A `▾` once used on the history buttons had to be confirmed as not tofu in the bundled macOS font. |
| A new ICON NAME | nowhere else, **if it is bundled** | `tests/icon_resolution.rs` answers per platform, but only for the theme that machine has. Bundling under the requested name settles it before anyone sees a placeholder, and the host theme still wins where it has one. ⚠️ MEASURED consequence: when every host theme *does* carry the name, the bundled copy is shadowed everywhere and **no machine exercises the fallback**. Know that before "simplifying" one away. |
| A new control, or a changed widget CLASS | any one platform, by looking | Construction details do not show in a layout test. `set_label` builds `box[label, arrow]`, which drew a second chevron beside the toolkit's own — identical on every platform and caught only by looking. A widget-tree dump plus a screenshot on any platform is the check. ⚠️ The tree alone can mislead: a `GtkMenuButton` arrow node reports present, visible and allocated while painting nothing. **The render is the witness, not the tree.** |
| A new KEY BINDING, or a window-level event controller | macOS and Windows | **Not because the toolkit's propagation differs — it was MEASURED not to.** A bubble-phase window controller for Escape behaved identically on X11/GTK 4.6, GDK-Win32/GTK 4.22 and Quartz: every in-toolkit claimant (popover, drop-down, in-window card, GTK dialog) answered first on all three. Two *other* things did differ and are what the macOS and Windows checks are for: a **native** surface may swallow the key before the toolkit sees it (macOS's native menu bar consumes Escape entirely — same visible outcome, different mechanism), and **where focus lands after an action is platform-specific** (after Enter in the find field, Windows returns focus to the document and macOS keeps it in the entry), which decides whether a key even reaches the controller under test. The two rules themselves are platform-INDEPENDENT: take a window-level key on the **bubble** phase so every other claimant answers first, and never report a key handled on the assumption that the handler's action worked — check that it did, or a failed handler swallows that key for the rest of the process. |
| Anything touching the titlebar | Windows | Windows requires a NATIVE frame; a `GtkHeaderBar` or `set_titlebar()` silently defeats `GTK_CSD=0` (MANUAL-TEST §7.0a). |

⚠️ **If a floor is ever re-measured by hand rather than by that test, converge first.**
It was established independently on both platforms that a single resize reports the *pre-wrap* floor —
macOS by a coarse drag that stops short, Windows by a `SetWindowPos` refused at the
unwrapped minimum — and the window reports the short width faithfully, so nothing looks
wrong. Repeat the resize until it stops changing before reading it. Per-platform recipes
are in MANUAL-TEST §A.2 and §A.3; the automated test avoids this entirely by measuring
rather than dragging, which is the third reason to lean on it.

⚠️ **A screenshot does not mean the same thing on every platform.** On Win32 a popover
and a dialog are each their own toplevel and a window capture cannot see them, while an
in-window overlay is visible only in the capture; on Quartz a screen-region capture sees
both. Judging "the popover did not open" under the wrong platform's rule produces a
confident falsehood. MANUAL-TEST §A.2 and §A.3 each state their own rule.

## Code style

- `rustfmt` and clippy clean. Use `Result`/`?`; no `unwrap`/`expect` outside tests and
  startup invariants.
- Soft limit of 1,200 lines per file, checked before the edit that would exceed it.
- Keep functions small and shallow; hoist computation out of signal-wiring bodies.
- No magic numbers or strings. A closed set of values is an `enum`, parsed once at the
  boundary.
- Widget-owned closures capture weakly.
- Destructure tuples by name, never `.0`/`.1`.

## Dependencies

- No new dependency without justification; check the crate list in TECH.md first.
- Never add a web engine or HTML renderer.
- The in-house image decoder crate holds no GTK type and never reaches into the
  application. Its decoder dependencies are pinned exactly.

## Input limits

- Documents and images are untrusted, and cost is part of the threat. Every read of a
  user-supplied path first passes the one shared admission check — a regular file
  **and** within the size limit. Never reimplement either half.
- Decoded image size is bounded separately from file size.

## Architecture rules

- **Render Markdown into native GTK widgets only.** No web engine, no GPU-composited
  UI.
- **Force the software (Cairo) renderer before GTK initialises.** The app never holds a
  GL context. The test configuration pins the same renderer; do not remove it.
- **Native window frame on Windows.** Never add a header bar or custom titlebar on any
  platform, and keep the setting that disables client-side decorations Windows-only.
- **One source of truth for desktop light/dark.** Whatever detects a change writes GTK's
  prefer-dark setting; never re-theme a surface directly from a platform signal.
- **One process, many windows.** A platform without GTK's single-instance transport gets
  a substitute feeding the same open/activate handlers, not an exemption.
- **Never silently overwrite unsaved edits.** An external change to a dirty buffer asks
  the user; a clean buffer reloads.
- **One action per command.** Every surface showing a command — menu bar, toolbar,
  context menu, overlay — binds the same action by name, and its enabled state is driven
  from one place. Never set sensitivity or wire activation per surface. A command's
  accelerator is declared once, in its descriptor; menu models set no accelerator
  attribute of their own.
- **Extend an existing code path rather than adding a parallel one.** When a new path
  is justified, say why in the code.
- **All document I/O goes through the async document-I/O module**, never on the main
  thread. Only the app's own small state files may be read synchronously.
- **All network access goes through the one image-fetch module.** Never hand a URL to
  GIO.
- **All GTK access happens on the main thread.** The app starts no threads of its own;
  off-thread work goes to GLib's pool, carries only plain owned data, and has a stated
  concurrency bound.
- **List styling is symmetric** across bulleted, numbered and task lists at every depth.
- **No hard-coded styling.** Colour, typography and geometry come from the active
  theme, resolving selected theme → system theme → GTK probe. A theme chooses from a
  closed vocabulary of decorations; the engine holds no per-theme knowledge; an unset key
  means absent. Scale themed geometry by zoom explicitly, clamp it, and sanitise any
  string interpolated into CSS.
- **One theme key, every application path.** A surface rendered by more than one path
  takes its value from a single theme key.
- **Bundled decoration art is an original design.** Nothing copied, traced or derived
  from another work.

## Typed GTK seams

GTK's runtime contracts are enforced by nothing at compile time, so a contract that keeps
recurring is promoted into a type or choke point that makes the wrong call impossible.

- If a seam exists for what you are about to write, call it. If none exists and the
  contract is typeable, add one in that change.
- A seam lands with its enforcement — a lint ban or encapsulation — in the same change,
  and with a test proving the wrapped call, not just the type.
- A lint ban is a routing instruction: take the route it names, never `#[allow]` past
  it, and declare any new exception in the ban itself.
- Ban a raw method only when most of its hits would be real mistakes; otherwise
  encapsulate.
- A seam either returns a safe fallback or forces the caller to handle absence. Choose
  per seam.
- Link the seam from the lesson it enforces; delete the lesson once the seam says it
  all.
- Do not force contracts that are runtime ordering invariants. Those stay prose in
  ANTI-PATTERNS.md.

## Change accountability matrices (CAM)

A change in a [CAM](CAM.md) category — commands, rendering features, views derived from
the document, anything holding a position in it, document I/O, anything wired to a signal
that fires continuously, a setting offered on more than one surface — accounts for every
applicable cell of that matrix, and derives its manual-test checks from them.  It is mandatory to ensure CAM consistency whenever making
functional modifications and/or feature additions.  Failure to do this can result in:
- Stale code paths
- Outright functional gaps
- Incorrect or inconsistent cache invalidation
- Missing or incorrect UI elements
- Even data loss and/or corruption

## Version control
When making Git commits, larger conglomerated commits with multiple disjoint (not disjunct) changes are
preferable to a long string of small and isolated commits because the former is easier for the operator
to follow and make sense of. 

Roll documentation changes into the latest commit instead of committing them,
separately, unless there are code changes that are significantly disjoint, which is defined as having
less than 20% overlap in terms of both code and UX, at which point a separate commit is warranted. 
Keep documentation changes in the working copy if they pertain to new work that is about to commence.

Work that shares a mechanism, fixture or test rig lands as **one squashed commit** on the branch. 
Never fast-forward or cherry-pick part of it. Sharing a subsystem does not make a batch; work that will 
not fit one commit is more than one batch.

- Work happens on a branch — `feature/<name>`, `bug/<name>`, or `mitigations` for
  unrelated small tasks — never directly on `master`.
- Merge a branch to `master` only once the GitHub pipeline has passed on it in full.
  Every push triggers it. When a commit needs fixing, amend it and force-push the branch
  rather than stacking follow-up commits, so each commit on `master` is one that passed.
- When merging a branch to master, just do a fast-forward merge if there's only one commit on the branch

## SDD register writes

- A lesson learned the hard way on this project is recorded in ANTI-PATTERNS.md when it
  is learned, not at the end of the work.
- Read an entry before citing it.
- **Citation forms.** `ScrAP-N` is an entry in `sdd/ANTI-PATTERNS.md`; `ScrSEC-N` is an entry
  in `sdd/CVE.md`. `GTK4Rs/AP-N` and
  `GEP-N` are entries in two private knowledge bases (the `gtk4-rs` and
  `general-engineering-principles` skills). They are not in this repository and not
  published anywhere, deliberately, so a contributor without them cannot look these up. A
  bare `AP-N` is not allowed. Never rewrite a citation's prefix in bulk: the registers number the same lessons differently,
  so re-derive each number at each site.
- **Never cite a git commit hash** — squashing orphans them, and an orphaned hash still
  resolves in the clone that wrote it while resolving nowhere else, so the citation looks
  sound exactly where it is checked. Cite the fact itself, a register entry, or the
  commit subject plus date.
- **Two things are not citations and the rule does not reach them.** A generated
  build stamp that ties a crash report back to the revision it came from is machine
  output, read by a maintainer holding the report, and it is emitted by the build
  rather than written by anyone. A transient instruction about the working tree — *"fetch
  and look at what I just pushed"* — names a commit in order to act on it now, not to
  cite it later. Anything that enforces this rule mechanically carries both as
  exceptions; a lint without them deletes the crash reporter's build stamp, and an agent
  reading only the flat prohibition does the same by hand.

## Prohibited actions

- **Never `git checkout` a file or path.** It discards uncommitted work
  unrecoverably. Copy the file aside and restore with `cp`.
- No `sudo` in any command an agent runs — it hangs on the password prompt. Ask the
  operator to install what is missing.
- Never block the GTK main thread on file I/O.
- Do not set `panic = "abort"`; the crash reporter relies on unwinding.

## Logging

- Log through the Rust `log` facade only, filtered by `RUST_LOG`. GTK's own messages
  are bridged into the same sink. Never use the glib logging macros.
- `error`: an operation failed and the app continues. `warn`: a recoverable anomaly.
  `info`: a lifecycle event. `debug`: per-operation detail. `trace`: hot paths, under an
  explicit target.
- `info` and above persist to disk and into crash reports, so every lifecycle boundary
  (open, save, reload, close, dialog, session restore) logs at `info`, once.
- **Never log document content** — not text, selections or clipboard, at any level. Log
  the path, the size, the decision.
- Repeated identical records are collapsed with a count, never capped.
- Messages are self-contained: path, event and relevant state.
- Programmer errors `panic!`; a controlled shutdown logs an `error!` and exits.
- Throwaway debug output is `eprintln!` prefixed `[🐛DEBUG]`, removed before commit.
- The test suites run with GTK criticals fatal on Linux and Windows. macOS cannot, for
  an upstream reason — do not arm it there by masking a log domain.
