# AGENTS.md

Scribobulate: a native GTK4 Markdown viewer/editor for **Linux, macOS and Windows** that
renders on the CPU (zero GPU memory) and live-reloads files as they change on disk.

## SDD skill

This project uses Spec-Driven Development (SDD). If you have access to the SDD skill, load
it before taking any action on this project — it governs how to read, write, and maintain
all project documentation. If the skill is unavailable, read the files in `sdd/` directly.

## Documentation

Project documentation lives in the `sdd/` directory.

### If you are exploring this project

- [`sdd/PRODUCT.md`](sdd/PRODUCT.md) — what this project is and why it exists
- [`sdd/TDD.md`](sdd/TDD.md) — the behavioural contract (Given/When/Then rubrics)
- [`sdd/TECH.md`](sdd/TECH.md) — read its module map before grepping source to answer how
  something works or where it lives

### If you are building, running, or testing this project

- [`sdd/POLICY.md`](sdd/POLICY.md) — build commands, the build pipeline, testing rules and
  prohibited actions. Read it in full before running anything; do not infer the build from
  `Cargo.toml`.
- [`scripts/pipeline.steps`](scripts/pipeline.steps) — the executable step list every
  platform's pipeline runner derives from. Read its header before changing any runner,
  packaging or the CI workflow ([`.github/workflows/pipeline.yml`](.github/workflows/pipeline.yml)).
- [`tests/MANUAL-TEST.md`](tests/MANUAL-TEST.md) — checks that need a running window; read
  its header, and §A on macOS or Windows, before any manual or GUI verification.
- Packaging: [`packaging/linux/`](packaging/linux/), [`packaging/macos/README.md`](packaging/macos/README.md),
  [`packaging/windows/README.md`](packaging/windows/README.md).

### If you are changing code in this project

Read all of the above, plus:
- [`sdd/CAM.md`](sdd/CAM.md) — the change accountability matrices POLICY makes binding. Its
  opening lists the kinds of change each matrix covers; check it before any change to
  commands, rendering, derived views, positions held in the document, document I/O,
  status notices, or handlers on continuously firing signals.
- [`sdd/SCHEMA.md`](sdd/SCHEMA.md) — exact shapes of what crosses the app's boundaries.
- [`sdd/ANTI-PATTERNS.md`](sdd/ANTI-PATTERNS.md) — lessons from past mistakes. A register:
  read its table of contents, then only the matching entries; scan it before
  troubleshooting, not after.

### Additional documents (read when relevant)

- [`sdd/THEMING.md`](sdd/THEMING.md) — before changing any preview colour, typography,
  decoration geometry or zoom.
- [`sdd/ISSUES.md`](sdd/ISSUES.md) — known unresolved issues. A register: scan its table of
  contents the moment you hit a bug, and read its header before picking work off it.
- [`sdd/PLAN.accessibility.md`](sdd/PLAN.accessibility.md) — before adding accessibility
  markup beyond a control name.
- [`sdd/PLAN.profiling.md`](sdd/PLAN.profiling.md) — before profiling, proposing a
  performance gate, or reaching for a GTK debug channel.
- [`sdd/PLAN.spell-check.md`](sdd/PLAN.spell-check.md) — **Planned, not yet implemented.** The spell checker: a toggleable, context-menu-driven checker over the editor pane. Read it before touching spell checking, the editor's tag set, or `contextmenu.rs`'s row construction. It records four facts measured on this machine's GTK/GtkSourceView (a foreign `GtkTextTag` survives re-highlighting; which regions the `no-spell-check` context class already covers; that `GtkTextIter`'s word API splits `don't`; that a whole-buffer re-tag does not move the viewport) — each of which is expensive to re-derive and one of which refutes an otherwise plausible assumption.

## Task triggers

Read the named document before doing any of these:

| When you are… | Read first |
|---|---|
| Writing an anti-pattern citation, or a lesson worth recording | POLICY § SDD register writes |
| Holding an offset, line number or index into the document in a field, closure, widget or idle | CAM § Document-Reference CAM, and `src/docref.rs` |
| Fetching anything over the network | `src/imagefetch.rs`'s header (POLICY § Architecture rules) |
| Touching crash handling or logging (`src/forensics/`, `src/logging.rs`) | `src/forensics/mod.rs`'s header |
| Touching the crash-recovery snapshot (`src/window/swap.rs`, `src/swapfile/`) | TDD §22 and SCHEMA § Crash-recovery swap file |
| Renaming a file through GIO, or cancelling a `gio::FileMonitor` | `src/docio/rename.rs`'s header |
| Building a second representation of a rendered document (export) | TECH's `export/` entry |
| Touching image decoding, animation or the memory-growth step | TECH's `imagedecode/` and `animation/` entries; POLICY § Per-render memory-growth class |
| Adding window chrome | TDD §9.38 (no control may set the window's width floor) |
| Writing a GTK test, or adding a module to `src/lib.rs` | POLICY § GTK-object integration tests |
| Retiring a plan, or deleting/renaming any document | run `cargo xtask lint-references`; a hand sweep is never complete |
| Changing the architecture | update `sdd/system-overview.svg` in the same change |
