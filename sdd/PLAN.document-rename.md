# Plan: Renaming an open document

**Status**: **IMPLEMENTED AND TESTED** on all three platforms (Linux, macOS, Windows;
operator-confirmed 2026-08-16). **FULLY DISTILLED. RETIREMENT DEFERRED — the operator
will delete this file in a future commit.**

No further work is owed to this plan and nothing should be added to it. Every durable
fact it carried already has a home elsewhere; it survives only until that deletion
lands:

| What it held | Where it now lives |
|---|---|
| The behavioural contract | TDD §24 (24.1 – 24.14) |
| The manual checks | `tests/MANUAL-TEST.md` §24 |
| The Document-Identity matrix, the toolbar exception, the back-sweep | [`CAM.md`](CAM.md) |
| The `win.rename` action | [`SCHEMA.md`](SCHEMA.md) |
| The two module rows and the recovery seam | [`TECH.md`](TECH.md) |
| The GIO mechanisms, source-traced with line numbers | `src/docio/rename.rs` module header |
| The testing and provenance lessons | ScrAP-269, ScrAP-270, ScrAP-271, ScrAP-272, ScrAP-274 |
| The two **unverified** Save As hypotheses | [`CAM.md`](CAM.md) back-sweep — in full, and still marked unverified |
| The decision narrative, phase log, item numbers | git history; deliberately nowhere else |

**Every original platform gap is closed, and so is the one added later** — 1 and 4
measured on the Windows and mac seats, 2 measured on macOS, and 5 answered
**SOURCE-READ** across nine GLib tags (ScrAP-270). The real NFD cause has now been
driven end-to-end on a purpose-built HFS+ volume, live reload included, so nothing
about this feature rests on a substitute any more. Exactly two things remain open, both
recorded where they will be read rather than here: the Windows monitor **event count**
(ScrAP-269 — set-verified, count-unverified, and the researcher has since shown *why*
it may be unknowable from source at all: the three-event expansion sits behind
`NextEntryOffset != 0`, i.e. behind `ReadDirectoryChangesW`'s batching) and
`renamex_np`/`MoveFileExW(…, 0)`, which are **DOC-ASSERTED** and matter only if the
TOCTOU is ever closed (`src/docio/rename.rs` header).

**Read the rest of this file as history, not as instruction.** The corrections struck
inline below are the reason it is worth a last read before deletion: two of its
load-bearing premises were wrong, and one of its *rejected* alternatives was wronger
than it recorded.

## Problem

A reader who wants to rename the file they are reading has to leave the
application, rename it in a file manager, and come back to a window whose title,
tab label and Documents list all still name a file that no longer exists — with a
live-reload monitor watching a path nothing will ever write to again. The
application knows the document's identity and owns every surface derived from it,
so it is the only thing in a position to change that identity coherently.

### Root cause — why this is not a one-line `rename(2)`

A document's **path is load-bearing state**, not a label. Nine things in this tree
are keyed on it, and the failure when one is missed is uniformly silent: the rename
succeeds, the title updates, everything *looks* finished, and something that reads
the path is now reading the wrong one. The inventory is in the proposed matrix
below.

Two of those nine are genuinely hazardous rather than merely stale:

1. **`std::fs::rename` silently replaces an existing destination.** So the obvious
   implementation — check that the target does not exist, then rename — is a
   check-then-act race whose failure mode is *destroying an unrelated file with no
   error and no log line*. ~~The refusal has to come from a primitive that refuses,
   not from a check we perform ourselves.~~

   > **CORRECTED 2026-08-15 (researcher, SOURCE-TRACED with verbatim quotes).** That
   > last sentence is false, and believing it is how this plan would have shipped a
   > guarantee nobody has. **No GIO primitive refuses atomically.** Both candidates
   > are `g_lstat()` followed by a plain `g_rename()` with *nothing in between* —
   > `glocalfile.c:1158-1191` for `set_display_name`, `:2467-2497` + `:2532` for
   > `move_`. A tree-wide grep for `renameat2`/`RENAME_NOREPLACE`/`renamex_np` finds
   > **zero hits** in 2.72.4 *and* in `main`. GIO **narrows** the window; it does not
   > close it. On Windows it is worse: `gstdio.c:1175` passes
   > `MOVEFILE_REPLACE_EXISTING` **unconditionally**, so that `g_lstat` is the only
   > thing between a rename and destroying an unrelated file.
   >
   > The consequence for this plan is small in code and large in honesty: the
   > primitive choice below stands, but **for different reasons**, and the seam's
   > rustdoc documents a **residual race** rather than claiming a refusal. An atomic
   > path exists on all three platforms (`renameat2(RENAME_NOREPLACE)` — MEASURED
   > refusing over an existing destination on tmpfs and ext4; `renamex_np(RENAME_EXCL)`;
   > `MoveFileExW(…, 0)`) and is **deliberately not taken** — see the Recommendation.
2. **The rename removes the old name from the directory, and the tab's monitor is
   watching it.** That is ScrAP-54's shape (the skill carries it as GTK4Rs/AP-62)
   arriving through a real rename rather than through atomic-save's
   write-temp-then-rename — so without care every rename fires the "File deleted on
   disk — save to restore it" notice and badges the tab with the ⚠ deleted-backing
   marker, immediately after a successful rename.

   > **SHARPENED 2026-08-15 (researcher; Linux MEASURED, other two SOURCE-TRACED).**
   > It is worse than one spurious `Deleted`, and that is what kills choreography (a):
   > on Linux and Windows the **old** monitor reports **three** events for a rename —
   > `DELETED`(old) **+ `CREATED`(new) + `CHANGES_DONE_HINT`(new)** — because
   > `glocalfilemonitor.c:419-450` expands a `RENAMED` into a delete plus a
   > `send_synthetic_created` (`:322-329`) when `WATCH_MOVES` is off, and `queue_event`
   > (`:222-249`) applies **no basename filter**, so the old monitor happily reports the
   > *new* name. `expect_self_delete` consumes only `DELETED`, so the other two fall
   > through to the `Changed|ChangesDoneHint|Created` arm and drive a reload.

## Scope

**In scope**: change the filename of the file backing one tab, within its own
directory, from the File menu and from the tab-strip context menu, with the
application's own view of that document following the change completely.

**Out of scope, deliberately:**

- **Moving the file to another directory.** `TabState::doc_dir()` is the resolution
  base for relative images, local link navigation, and the Insert Link/Image
  relativisation. Holding the directory fixed makes that whole row a provable no-op
  rather than a re-render obligation nobody would think to write. See the matrix
  row 5 note.
- **Rewriting references to the old name in other open documents.** A rename in a
  file manager does not do this either; a Markdown link that pointed at the old name
  is now broken, and that is the user's business.
- **Following an *external* rename.** `FileMonitorEvent::Moved`/`Renamed`/`MovedIn`/
  `MovedOut` are dropped on the floor today (`app/open.rs`, deliberately and with a
  comment). This plan does not change that, but it does record it as the fourth
  member of the identity-change category the matrix below governs.
- **Renaming an untitled document.** It has no file. The honest routing is Save As,
  and the command is simply insensitive.

## Surfaces and choke points — what is free, and what is not

**Free, because the choke point already exists and already contemplates a name
change:**

| Concern | Choke point |
|---|---|
| Window title, tab label + tooltip, View ▸ Documents menu **and** the toolbar combo | `window::tabs::update_window_title` → `retitle_window` + `set_tab_label` + `refresh_documents_menu` → `refresh_documents_button`. `documents.rs` already names "Save-As rename" as one of its triggers. This is Derived-view CAM row 4 in one call. |
| Re-pointing the path, the path-dependent actions, and the live-reload monitor | `crate::app::attach_file_backing(window, tab, new_path)` — sets `tab.path`, cancels the previous monitor, starts a fresh one on the new path, and unconditionally disarms `expect_self_delete`. |
| Back/Forward history | Keyed on `TabId` (`winstate::navhistory::place`), not on a path. Untouched by construction. |
| The crash-recovery swap file's *identity* | `DocId`, not a path — `swapfile/naming.rs` cites rename by name as one of the four reasons it is not path-derived. Its *filename stem* is a separate matter; see matrix row 3. |

**Not free:**

| Concern | Why |
|---|---|
| Which document the command acts on | The tab-strip context menu fires for the **right-clicked** tab, which need not be active, while `win.rename` is a window-scoped action that acts on the active one. Resolved the same way `copy_full_path_for_tab` / `reload_for_tab` already resolve it — operator decision above. |
| Enabled state | The predicate is **per-tab**; the action's `is_enabled()` answers for whichever tab is active *now*, which is the wrong tab for a right-click on an inactive one. |
| "The file has already been deleted" | `backing_missing` is only set if the monitor reported a delete *while this tab was watching*. It is the right gate for the action's sensitivity and is **not sufficient** as the operation's precondition. |
| The filesystem call itself | See Root cause 1. |
| The monitor's self-inflicted `Deleted` | See Root cause 2. |

## Proposed: a Document-Identity CAM

**The category is not "rename". It is *a document's identity (its path) changing
while the document is open*** — and it already has three members before this
feature exists (first save of an untitled buffer, Save As adopting a path, Save As
re-pointing one), plus the external-move non-member above. Rename is the fourth.

**Why it is not covered by the six matrices we have.** [Derived-view](CAM.md#derived-view-cam--surfaces-that-mirror-document-state)
row 4 covers the *display* of a document's name, and its column B already includes
Save As — so the name surfaces are governed. [Document-Reference](CAM.md#document-reference-cam--state-that-points-into-the-document)
governs references pointing *into* a document (offsets, ranges, indices); a path is
not one. [Deferred-operation](CAM.md#deferred-operation-cam--work-whose-completion-lands-later)
column E is literally "the document's identity changes", but it governs that from
the *in-flight operation's* side — what a read or write already out must do when
the path moves under it. **Nothing governs the non-visual, not-in-flight machinery
keyed on the path**, which is where every latent gap in this feature lives.

Rows are the state keyed on a document's path; columns are the three identity
events. Draft, for operator review:

- **A — adopt**: a document that had no path acquires one (first save of untitled;
  Save As from untitled).
- **B — re-point**: a document with a path acquires a different one (Save As to
  another path; **Rename**).
- **C — lose**: the backing file goes away (external delete; a tab discarded).

| # | Path-keyed state | A | B | C | Choke point / anchor |
|---|---|:-:|:-:|:-:|---|
| 1 | `TabState.path` — the truth every other row derives from | ✓ | ✓ | ✓ | `attach_file_backing` |
| 2 | The live-reload `gio::FileMonitor`, the `expect_self_delete` guard and `backing_missing` — **one mechanism, not three** | ✓ | ✓ | ✓ | `attach_file_backing`; ScrAP-54 |
| 3 | The crash-recovery swap file's **stem** and its header `path` | ✓ | ✓ | ✓ | `swapfile::swap_path`; `window::swap::delete_snapshot`; `swaprecovery::retire_source_snapshot` |
| 4 | Name surfaces — window title, tab label + tooltip, View ▸ Documents (menu **and** combo) | ✓ | ✓ | ✓ | *reference row* → Derived-view CAM row 4 |
| 5 | The relative-resource base `TabState::doc_dir()` — images, local link navigation, Insert Link/Image relativisation | ✓ | ✓ | — | `links::resolve_contained_image`; `links::relativize_for_insert` |
| 6 | Same-file identity: the open-tab dedup lookup and the per-path write gate | ✓ | ✓ | ✓ | `app::find_open_tab_for_path`; `winstate::WriteGate` |
| 7 | Operations already in flight over the old path | ✓ | ✓ | ✓ | *reference row* → Deferred-operation CAM column E |
| 8 | The persisted session record and the last-visited dialog directory | ✓ | ✓ | ✓ | `session.rs`; `app::remember_dialog_dir` |

Rules that would give it teeth:

- **A path change is not a content change.** It must not touch the saved baseline,
  the dirty flag, the buffer, the undo stack, the reading position, or the rendered
  preview, and must not re-read the file. Stating this is what keeps an identity
  change *out* of the Reading-Position and Document-Reference matrices instead of
  quietly acquiring cells in both.
- **One choke point re-points the backing, and the monitor and its self-delete
  guard travel with it.** They are one mechanism; a path change that re-points the
  monitor without settling the guard has half-changed the identity
  (GTK4Rs/AP-108 / GTK4Rs/AP-130 shape).
- **Refusal is expressed against the filesystem's own notion of identity, never a
  string compare.** Case-insensitive filesystems and symlinks both make
  `old != new` a wrong answer. See "Case-only rename" below.
- **A move is not a rename**, because only one of them invalidates row 5. Any
  operation that can change the *directory* owes row 5 an answer; one that cannot
  may state that it cannot and stop.
- **A `tests/MANUAL-TEST.md` check per applicable ✓** — the cross-CAM rule.

### The back-sweep this matrix obliges

CAM's own rule: a new row (and *a fortiori* a new matrix) obliges sweeping what
already shipped in the category, or an explicit record that it was not swept. The
category has three prior members, all in `save.rs` / `open.rs`, so the sweep is
affordable and must actually be done. Drafting the rows produced **two candidate
defects in Save As**. Both are **INFERRED from source, not measured** — neither is
a claim until it is checked:

1. **Save As from an untitled dirty buffer probably orphans its swap file.**
   Opening the native chooser deactivates the window, so
   `swap::wire_swap_focus_flush` → `flush_window_swaps` → `write_snapshot` files a
   snapshot as `untitled-<docid>.swap`. `adopt_and_save` then sets `st.path`
   **before** the write, so when the document goes clean the dirty↔swap invariant's
   `delete_snapshot` computes `<newstem>-<docid>.swap` and removes nothing —
   `NotFound` is swallowed by design, and `swap.on_disk` is set `false` while a file
   exists. `recovery::disposition` never asks whether the snapshot's content matches
   what is on disk, so the next launch would take the `ReopenUntitled` branch and
   present a spurious recovered untitled tab holding the pre-save content.
   **Cheap to settle**: `save.rs` already has a `drive_save_as` test helper — a
   short `#[gtktest::test]` asserting the swap directory is empty after Save As
   decides it either way. Note this is *not* the already-recorded Deferred-operation
   CAM 4/B–5/B open cell: that one is a delete racing an in-flight write to the
   *same* filename; this one can never succeed even when it runs, because it
   computes a different name.
2. **Save As to a different directory probably leaves the preview's images
   resolved against the old `doc_dir()`** until something else re-renders. Matrix
   row 5, from the one existing member that can change directory.

## Possible approaches

### The filesystem primitive

**1. `std::fs::rename` after an existence check.**
**Pros**: no GIO, trivially testable, no new async surface.
**Cons**: **rejected on safety.** Documented to silently replace an existing
destination, so the check-then-rename window destroys an unrelated file with no
error. Also bypasses `docio`, which POLICY makes the only route to document I/O.

**2. `gio::File::set_display_name` (`FileExt::set_display_name`).** ← **CHOSEN**
The rename-within-parent primitive; returns a new `GFile` for the renamed file.
**Pros**: exactly the operation's shape. ~~it *cannot* express a move, so the
same-directory constraint is enforced by the primitive rather than by our
validation.~~ Has an async form for `docio`, and that form is **older than our
floor** — `set_display_name_async` predates `g_file_move_async` (`gfile.h:124-125`,
*Since: 2.72*) by a decade, and 2.72 is exactly our floor with zero headroom.
**Cons**: ~~whether its existence refusal is atomic … unknown~~ — answered: it is
**not** atomic (Root cause 1's correction), and the separator guard is **weaker than
it looks**.

> **The separator guard does not enforce "same directory" on Windows.** It is a
> **single-character** test in the public wrapper (`gfile.c:4423-4430`,
> `strchr(display_name, G_DIR_SEPARATOR)`), and `G_DIR_SEPARATOR` is `\` on Windows —
> while GLib's own path machinery treats `/` as a separator there too
> (`gfileutils.h:191`) and resolves `..` lexically below the guard
> (`g_canonicalize_filename`). So on Windows `set_display_name("sub/evil.md")`,
> `"../evil.md"` and forward-slash absolute paths **escape the directory**.
> SOURCE-TRACED (no Windows seat measured it); POSIX is MEASURED as correctly
> rejecting all three with `InvalidArgument` and performing no traversal.
> ⇒ **We validate the name ourselves on every platform** — which the plan already
> proposed for UX reasons (rubric 24.9), and which now has a second, independent
> justification. Two reasons, stated as two.

**3. `gio::File::move_` with `FileCopyFlags::NONE`.** ← rejected
**Pros**: ~~documented to error rather than overwrite without `OVERWRITE`~~ — it errors
via the same non-atomic `g_lstat`; can also express a move, should scope ever widen.
**Cons**: *can* express a move, so the same-directory constraint becomes our
validation's job again — and it carries **no** separator guard at all, so it is
strictly weaker than approach 2 on the one axis approach 2 is imperfect on. Its async
form also sits exactly on our version floor.

### The monitor choreography

**a. Arm `expect_self_delete`, rename, then `attach_file_backing`.** ← **REJECTED as
INSUFFICIENT** (not "weaker" — insufficient)
Mirrors what save already does for its own rename (ScrAP-54).
**Pros**: reuses a mechanism that exists and is understood.
**Cons**: ~~relies on exactly one `DELETED` arriving … If a backend reports the rename
as `MOVED`/`RENAMED` or coalesces it into `CHANGED`, the guard never fires.~~ The real
defect is the opposite of that worry: the guard consumes **only** `DELETED`, and
Linux and Windows deliver **`CREATED` + `CHANGES_DONE_HINT` as well** (Root cause 2's
sharpening). Those two are not swallowed, reach the `Changed|ChangesDoneHint|Created`
arm, and drive a reload. ~~(a) happens to work on **macOS by accident** — kqueue emits
`DELETED` and nothing else — and leaks on the *other two* platforms.~~

> **CORRECTED 2026-08-16 (mac seat, MEASURED, three identical runs).** kqueue emits
> **`DELETED` twice**, not once. The self-delete guard arms and consumes exactly **one**
> event, so the second would leak straight through to the deletion handling and raise a
> false "File deleted on disk" on the very platform this paragraph called safe. **(a)
> was broken on all three platforms, not two** — its single "accidental" success was an
> artefact of a source-traced event count that was one short. Nothing shipped depends on
> this: (b) cancels the monitor before the rename and no event reaches anything. Left
> here, struck rather than deleted, because the *reason* to reject (a) is now stronger
> than the reason recorded, and a future reader tempted back to it should meet the
> corrected version.

MEASURED (Linux,
GLib 2.72.4): with a
60 ms stand-in for an async round trip, all three events were delivered **before** the
completion callback ran, so the old monitor fires before `attach_file_backing` could
cancel it anyway.

**b. Cancel the old monitor *before* the rename, then rename, then re-attach.** ←
**CHOSEN, and the sole load-bearing mechanism**
**Pros**: ~~if cancellation is a hard barrier~~ — **it is one, MEASURED (Linux, GLib
2.72.4; the suppression itself is source-traced and so generalises).** No event
can be delivered, so there is nothing to guard. Uniform across all three platforms.
**Cons**: leaves a brief unwatched window (during which we hold the write gate, so
nothing of ours can write) — and imposes two obligations, below.

> **Mechanically, "hard barrier" is the observable, not the implementation — write
> both down.** `cancel` does **not** drain the pending queue: none of the four backend
> cancel vfuncs touches `event_queue`/`pending_changes` (that is
> `g_file_monitor_source_dispose`, `glocalfilemonitor.c:589-618`, reached from GObject
> dispose and not from cancel). The queued entries survive, are dispatched, and are
> **discarded one at a time at emission** — `gfilemonitor.c:281-295` returns early on
> `priv->cancelled` before `g_signal_emit`, and every local-backend event passes
> through that one function. Rely on the observable; record the mechanism so nobody
> "optimises" against a drain that never happens.
>
> Two footnotes for the seam's docs: on 2.72.4 the flag is set *after* the backend
> cancel vfunc returns, a nominal window that cannot bite us because emission only ever
> happens on the monitor's own main context — the same one we cancel from (GLib ≥ 2.84
> closes it properly with a tri-state atomic set *before* the vfunc). And **use
> `FileMonitorExt::is_cancelled()`, never the `cancelled` property** — below 2.84 the
> property getter is hard-coded `FALSE` with the real value commented out
> (`gfilemonitor.c:105-108`), which is a silent wrong answer on our exact floor.

**Two obligations (b) brings, neither optional:**

1. **A failed rename must re-attach the monitor to the OLD path**, unconditionally
   (a `Drop` guard or an explicit `else` — not a happy-path line). Cancelling first
   means a failed rename otherwise leaves the tab permanently unwatched, which is a
   *silent* loss of live reload. Covered by forcing `Exists` and then asserting a
   subsequent genuine external delete still raises the notice.
2. **Re-verify on re-attach.** The tab is blind for the rename's duration — a shared
   GIO-pool round trip (ScrAP-243) — so re-stat and reconcile once after attaching,
   rather than assuming nothing happened while we were not looking.

`expect_self_delete` **stays**: it is still load-bearing for atomic-save's own rename
(ScrAP-54). It simply has no role in the rename path.

**c. Both.** ← **moot, and the reason is worth keeping**
This was written as the ScrAP-254 trap: two independently sufficient mechanisms are
mutation-proof one at a time, so the guard would grade each as dead code. **The
premise turned out to be false — (a) is not sufficient**, so there was never a set to
mutate against. Kept as a record that the question "which half is load-bearing?"
was answered by discovering there is only one half, which is a better outcome than
the answer the question was fishing for.

### Case-only rename (`notes.md` → `Notes.md`)

On APFS and NTFS the destination "exists" — it *is* the source — so the obvious
existence refusal blocks a legitimate rename on two of three platforms.
`paths_refer_to_same_file` does not rescue this: it canonicalises, and
canonicalisation of a case-variant is not reliably case-preserving.

~~**1. Let GIO decide** (preferred if Q1 says it special-cases this).~~ **RULED OUT.**
GIO does not special-case it. Neither primitive compares `st_dev`/`st_ino` or calls
`g_local_file_equal`; the check is purely "does `lstat` on the destination succeed",
so on APFS/NTFS `lstat("Notes.md")` succeeds when only `notes.md` exists and the
rename is refused with `G_IO_ERROR_EXISTS`. SOURCE-TRACED decisive; the platform half
is INFERRED and is gap 1 below (ext4 cannot reproduce it — a case-sensitive
filesystem has nothing to exhibit).

**2. Detect it ourselves** ← **CHOSEN.** If the new name differs from the old only by
Unicode case folding, skip the existence refusal and let the OS act. Safe in both
directions — on a case-sensitive filesystem the destination genuinely does not
exist; on a case-insensitive one it cannot be a *different* file.
**Implementation note**: the rename must go **two-step through a guaranteed-free temp
name** (`<old>.rename-<nonce>` → `<new>`), because the refusal we are stepping around
is GIO's own and cannot be disabled. Each step is individually atomic with respect to
readers; **the pair is not** — a crash between them leaves the temp name behind, so
`<stem>.rename-*` is an orphan to recognise on next open. **Do not** reach for
`move_` + `FileCopyFlags::OVERWRITE` to dodge the two-step: that disables the only
destination protection we have.
**3. Compare `st_dev`/`st_ino`.** Correct on unix, and Windows has no stable `std`
equivalent — rejected on portability cost.

**Unicode caveat (macOS).** `set_display_name` never re-reads what the filesystem
actually stored. HFS+ normalises to NFD; APFS is normalisation-*insensitive* but
preserving. So the name we asked for and the name a `FileEnumerator` reports need not
be byte-equal — re-query `STANDARD_NAME` where the authoritative spelling matters.

## Recommendation

**Primitive**: `gio::File::set_display_name`, wrapped in a new
`docio::rename_document(path, new_name) -> io::Result<PathBuf>` — the crate's only
sanctioned route to document I/O, off the main thread. **The seam's rustdoc documents
a residual TOCTOU race; it does not claim a refusal** (Root cause 1's correction).
Chosen over `move_` on shape, on carrying *some* separator guard where `move_` carries
none, and on `set_display_name_async` predating our 2.72 floor by a decade where
`g_file_move_async` sits exactly on it.

**The atomic path is deliberately not taken.** All three platforms expose an atomic
no-replace rename (`renameat2(RENAME_NOREPLACE)`, MEASURED refusing over an existing
destination on tmpfs and ext4; `renamex_np(RENAME_EXCL)`; `MoveFileExW(…, 0)` — the
latter two INFERRED from documentation) and GLib uses none of them. Three-platform
unsafe FFI is disproportionate surface for renaming inside the user's own directory,
so `docio` keeps an internal hook for the atomic path and the TOCTOU is **recorded in
the ANTI-PATTERNS entry rather than pretended away**. Revisit if the residual race
ever produces a real report.

**Validation is ours, on every platform** — reject empty, `.`, `..`, `/`, `\`, and
interior NUL; on Windows additionally the reserved device names (`CON`, `PRN`, `AUX`,
`NUL`, `COM1-9`, `LPT1-9`), a trailing `.` or space, and `< > : " | ? *`. Two
independent reasons, and both are load-bearing: GIO's guard lets `sub/x.md` and
`../x.md` escape the directory **on Windows**, and GIO reports `""`, `.` and `..` as
**`Exists`** — "filename already exists", shown to a user who typed nothing.

**Error taxonomy**: discriminate on `err.kind::<gio::IoErrorEnum>()`, never on the
message (every case shares the `g-io-error-quark` domain, and the messages are
translated). Destination exists → `Exists`, with the victim intact. Source vanished →
`NotFound`. Unwritable parent → `PermissionDenied`. Separator on POSIX →
`InvalidArgument`. `InvalidFilename` is essentially FAT-only and cannot be relied on.

**Choreography**: **(b) alone** — cancel the monitor, rename, re-attach. (a) is
insufficient rather than redundant, so there is one load-bearing mechanism and the
ScrAP-254 concern does not arise. Both of (b)'s obligations — unconditional re-attach to
the old path on failure, and a re-stat on re-attach — are part of the deliverable, not
follow-ups.

**Case-only**: (2), via the two-step temp name.

**Subject and enablement**: the operation takes an explicit `Rc<TabState>` resolved
when the user acts and carried across the dialog and the await (the cross-CAM
"name your subject once" rule); the context menu `focus_page`s the clicked tab
first, matching `reload_for_tab`. One pure predicate

```rust
// winstate/decisions.rs — display-free, unit-tested, inside the coverage gate
pub(crate) fn rename_enabled(has_path: bool, dirty: bool, backing_missing: bool, write_in_flight: bool) -> bool
```

read by `update_rename_action_state` for the active tab and by the context-menu
button for the clicked tab. Same predicate, two readers — which is the honest
reading of POLICY's single-`GAction` rule for a per-tab gate, and the same shape
`contextmenu.rs` already documents for Copy Full Path and Reload.

**Preconditions are re-checked at apply time, not trusted from the gate.**
`backing_missing` gates the *command*; the *operation* re-checks that the source
exists off the main thread, and on finding it gone flips `backing_missing`, badges
the tab and enables Save — exactly what the monitor's own `Deleted` arm does, by
calling the same code.

## Command surfaces (Action CAM)

`win.rename`, one `SimpleAction`, in the "Other action" column:

| Obligation | How |
|---|---|
| Menu-bar item | File ▸ `Rena_me…` (mnemonic `m`; `R`, `e`, `n` are taken in that menu by Reload, … , New Document). Built ad-hoc in `menubar::build_file_menu`, **never a `FILE_CMDS` row** — a row auto-generates a toolbar button. Exactly the Close Tab precedent, comment and all. |
| Toolbar section | **Deliberately absent — operator-granted exception, recorded in [`CAM.md` § Granted CAM exceptions](CAM.md#granted-cam-exceptions) as one entry covering Rename, Close Tab, Go To Line and Next/Previous Annotation. Three of those four were deviating unrecorded until this sweep.** |
| Context menu | Tab-strip context menu, `Re_name…` (access key `n`; `C`/`O`/`M`/`F`/`R` are taken there). |
| Accelerator surfaced everywhere it is mirrored | `F2`, declared in `INLINE_ACCEL_CMDS` under group `File`, which puts it in the binding, the menu hint **and** the Keyboard Shortcuts window from one string. `accel::map` leaves a bare F-key untouched, so it is `F2` on all three platforms; nothing in `MAC_RESERVED` claims it. |
| Single `GAction` source of truth | ✓ |
| Consistent enabled state | ✓ via the one predicate above. |

**The toolbar exception, stated plainly.** The Action CAM's "Other action" column
requires a toolbar button and the operator has ruled it out, so this is a deviation
that must be requested and recorded rather than silently taken. While confirming
the mechanism, the same sweep found **Close Tab and Go To Line already carry that
deviation unrecorded**, alongside the already-pending Next/Previous Annotation
entry. Worth resolving as one exception entry covering the family — "a command with
no state a button could usefully show, in a toolbar at its width budget" — rather
than four.

## Draft TDD rubrics — proposed for the plan kickoff

Proposed as a **new §24, Renaming an open document**, with its own
`tests/MANUAL-TEST.md` section, rather than extending §4 (Editing & saving): it is
~12 rubrics of its own and a rename is not a save. Operator authors; these are
drafts.

- **24.1 A clean, titled document can be renamed in place** — *Given* a saved
  document with no unsaved changes, *When* the reader chooses Rename and supplies a
  new filename, *Then* the file on disk carries the new name in the same directory,
  the old name is gone, and the bytes are unchanged.
- **24.2 The rename is not an edit** — *Then* the document stays clean, the buffer,
  the reading position, the undo history and the rendered preview are all
  unchanged, and no re-read of the file occurs.
- **24.3 Every surface that names the document follows** — window title, tab label,
  tab tooltip, View ▸ Documents menu item and the toolbar Documents combo, all
  immediately and without the reader taking any other action.
- **24.4 Live reload follows the file** — *Given* a renamed document, *When*
  something else writes to the **new** path, *Then* the change is picked up; *And*
  re-creating a file at the **old** path changes nothing.
- **24.5 A rename does not look like a deletion** — no "File deleted on disk"
  notice, no ⚠ deleted-backing badge, and Save does not become enabled on a clean
  document.
- **24.6 Rename is unavailable when it cannot be correct** — insensitive for an
  untitled document, for one with unsaved changes, for one whose backing file is
  known to be gone, and while a write to that document is in flight — in every view
  mode and on every surface at once.
- **24.7 An existing file is never overwritten** — *When* the chosen name already
  exists in that directory, *Then* the rename is refused, the reason is reported,
  and both files are untouched.
- **24.8 A vanished source is reported, not papered over** — *When* the file has
  been deleted since the command was enabled, *Then* the rename is refused, the
  document is marked as having lost its backing file, and Save becomes available to
  re-create it.
- **24.9 A name that cannot be a filename is refused before anything happens** —
  empty, `.`, `..`, or containing a path separator; refused in the dialog, with the
  confirm control insensitive, not as an error afterwards.
- **24.10 Changing only the letter case is a rename, not a collision** — on a
  case-insensitive filesystem as on a case-sensitive one.
- **24.11 Renaming from the tab strip acts on the tab that was right-clicked** —
  including when it is not the active tab, and the reader is shown that document.
- **24.12 The rename cannot be aimed at the wrong document** — *Given* the reader
  switches tabs while the rename dialog is open, *Then* the rename still applies to
  the document it was invoked for.

## Test plan

- **Display-free unit tests** (inside the coverage gate): `rename_enabled`'s truth
  table; the new-name validator (empty / separator / `.` / `..` / unchanged /
  case-only); the case-fold comparison.
- **`#[gtktest::test]`**: 24.1–24.5, 24.7, 24.8, 24.11, 24.12. The monitor
  re-point (24.4) is the one that matters and **must be mutation-tested** — delete
  the `attach_file_backing` call and confirm red; a test that only asserts
  `tab.path` would stay green with live reload dead.
- **The no-spurious-notice guard (24.5)**, in the shape the researcher specified —
  and its in-test **control is not optional**: after asserting no notice appeared,
  unlink the *new* file in the same test and assert the notice **does** appear.
  Without it the test passes when the monitor is simply broken, which is ScrAP-209's
  shape (a guard whose setup prevents the thing it guards from existing).
  Timing: events land within ~60 ms on inotify; pump ≥ 1 s.
  **Two mutations, and the second is the interesting one** — removing the
  `monitor.cancel()` before the rename must fail (three delivered events on Linux),
  and removing `expect_self_delete` must **not**, which is the positive proof that
  (b) rather than the flag is carrying the invariant. Not a ScrAP-254 set-mutation:
  there is one mechanism, and the second mutation is what demonstrates that.
- **Rename-failure re-attach**: force `Exists`, then assert a subsequent genuine
  external delete still raises the notice — i.e. that the monitor came back.
- **`tests/MANUAL-TEST.md` §24**: one check per applicable CAM cell, including a
  live-display check for 24.10 on each platform that has a case-insensitive
  filesystem — the one rubric no Linux run can answer.

## Files this touches

| File | Change |
|---|---|
| `src/window/rename.rs` *(new)* | The dialog, the validator, and the operation. |
| `src/window/editoractions.rs` | Register `win.rename`; `update_rename_action_state`. |
| `src/winstate/decisions.rs` | `rename_enabled` — the one predicate. |
| `src/docio/mod.rs` | `rename_document` — the async seam, its **residual-race** semantics (not a refusal claim), the name validator's platform rules, and the internal hook left for an atomic no-replace path. |
| `src/app/open.rs` | **A correction, not a feature**: the `Changed \| ChangesDoneHint \| Created` arm's comment asserts that "some monitor backends coalesce a rename-over into Changed/Created without a separate Deleted". That is refuted and **inverted** — macOS is the backend that delivers `DELETED` and nothing else, and no backend delivers Changed/Created without it. The comment sits in front of a `disarm()` and is load-bearing prose, so it is repaired with the researcher's citations rather than left standing. |
| `src/app/menubar.rs` | The File ▸ Rename… item, beside Close Tab. |
| `src/app/commands.rs` | The `INLINE_ACCEL_CMDS` row for `F2`. |
| `src/app/mnemonics.rs` | `("Rename…", "Rena_me…")`. |
| `src/window/tabs/contextmenu.rs` | The context-menu button and its focus-first driver. |
| `sdd/CAM.md` | The new matrix, and the Granted-exceptions entry. |
| `sdd/SCHEMA.md` | The `win.rename` row in the GAction table. |
| `sdd/TDD.md`, `tests/MANUAL-TEST.md` | §24 and its checks. |
| `sdd/TECH.md` | The `window/rename.rs` module row. |

No `sdd/system-overview.svg` edit: this adds no component and no data flow — it is
a new command over the existing `docio` write edge and the existing
`attach_file_backing` choke point.

## Mechanism questions — resolved (researcher, 2026-08-15)

Full write-up, with the probe rig and verbatim GLib quotes:
`~/Documents/Projects/AI/Research/Gtk4Rust/researcher-findings-gio-rename-refusal-and-filemonitor-cancel-barrier.md`
(rig: `_src/gio-rename-refusal/`, four C probes). **Both questions are closed**; what

> **The measurement envelope, stated once and attached to every MEASURED claim below:
> Linux / GLib 2.72.4-0ubuntu2.9 (jammy), kernel 6.8.0-136, glibc 2.35, ext4 + tmpfs.**
> The **source-traced** half of each answer generalises across platforms and versions
> (it is GLib's own code, and the 2.72.4↔`main` diff was checked); the **measured**
> half does **not** — it is one platform, one GLib. Keep the qualifier attached when
> any of this is quoted onward, including into ANTI-PATTERNS or the `gtk4-rs` skill:
> a label is only worth having if the platform it was established on travels with it.
> Separately, the rig measures **GLib's behaviour on a synthetic file in a tmpdir** —
> not Scribobulate's monitor under a real tab through `attach_file_backing`. Those are
> different claims, and the register entry must land on *our* demonstration with this
> trace as the mechanism behind it, never borrow this evidence across that boundary.
each decided is folded into the sections above rather than left here to be
cross-referenced, and the two answers that *contradicted* this plan are recorded at
the premises they contradict (Root cause 1 and 2) so a reader cannot pick up the old
reasoning by reading only that far.

| Question | Answer | Where it landed |
|---|---|---|
| Q1 — does a GIO primitive refuse an existing destination *atomically*? | **No.** Both are `g_lstat` + plain `g_rename`, nothing between; zero `renameat2`/`RENAME_NOREPLACE` in 2.72.4 **or** `main`. Windows always passes `MOVEFILE_REPLACE_EXISTING`. | Root cause 1 (correction); Recommendation (rustdoc documents a race) |
| Q1 — case-only rename on a case-insensitive FS | **Refused** (`Exists`) — GIO special-cases nothing | Case-only rename → approach (2), two-step |
| Q1 — does the primitive enforce "same directory"? | **Not on Windows.** One-character `strchr` guard on `\`; `/`, `..` and drive-absolute paths escape | Approach 2's box; Recommendation (we validate) |
| Q1 — error taxonomy | `Exists` / `NotFound` / `PermissionDenied` / `InvalidArgument`; **`""`, `.`, `..` all report `Exists`** | Recommendation (discriminate on `kind`, validate first) |
| Q2 — is `cancel()` a barrier against a queued event? | **Yes, MEASURED** — queue is left intact, entries discarded at *emission* (`gfilemonitor.c:281-295`) | Choreography (b); its mechanism box |
| Q2 — per-backend events for a rename of the watched file | Linux **DELETED+CREATED+CHANGES_DONE_HINT** (MEASURED); Windows the same (SOURCE-TRACED); macOS/kqueue **DELETED ×2** (MEASURED 2026-08-16 — the source-traced "DELETED only" was one event short) | Root cause 2 (sharpening); (a) rejected |
| Q2 — does a stale monitor follow the path or the inode? | **Backends disagree**: Linux/Windows path (inert); **macOS inode — actively wrong**, reporting later writes to the new name under the *old* basename | below |

**Two consequences that belong nowhere else:**

- **A stale monitor is not merely useless.** On Linux/Windows it is inert *until* someone
  creates a fresh file at the old name, at which point it reports `CREATED` and the tab
  concludes its file came back. On macOS it holds an open fd and `EVFILT_VNODE` follows
  the **inode**, so writes to the *new* name are reported against the *old* basename —
  live reload silently firing for a path the document no longer has. (b) makes the stale
  state unreachable by construction, which is the strongest argument for it after the
  event counts.
- **A fresh monitor on the destination emits nothing.** MEASURED (Linux, GLib 2.72.4;
  source-traced clean on kqueue and win32): rename, attach a monitor
  to the new path, spin 1.5 s → zero events. No replay and no initial-state scan, so
  re-attaching cannot itself manufacture a spurious `Created`. This is why obligation 2
  above is a **re-stat**, not a "wait and see what the monitor says".

### Remaining gaps — honest, and not to be rounded up

Four claims are SOURCE-TRACED or INFERRED rather than measured, all because this seat has
no access to the platform in question. They are **not** blockers for the Linux
implementation and must not be silently upgraded when it goes green:

1. ~~**Case-only rename on APFS/NTFS**~~ — **CLOSED 2026-08-16**, MEASURED by the
   `windows` seat on Win10 19045 / NTFS / GTK 4.22.4.
2. **macOS monitor event sequences — CLOSED**; **the Windows event COUNT is not, and is
   now specifically suspect.** MEASURED on macOS by the mac seat (26.6.1 / GTK 4.22.4 /
   GLib 2.88.2, three identical runs): kqueue emits **`DELETED` twice**, not once
   (a correction), and the inode-following claim was right and is now measured rather
   than inferred (a confirmation). Source-tracing therefore got the *semantics* right
   and the *multiplicity* wrong — so the Windows row's **`DELETED+CREATED+CHANGES_DONE_HINT`
   is sound as a set and unverified as a count**, on exactly the dimension that just
   failed for kqueue. Do not read this gap as closed for Windows; it wants a probe on
   that seat, not another source trace. Recorded in ScrAP-269.
3. **`renamex_np` / `MoveFileExW(…, 0)` atomicity** — only matters if the TOCTOU is ever closed.
4. ~~**HFS+ NFD round-tripping**~~ — **CLOSED 2026-08-16**, MEASURED both directions by
   the mac seat with `od -c` on the stored entry: **APFS is normalization-preserving**
   (`café.md` → `c a f 303 251`, untouched), a purpose-built HFS+ image **decomposes**
   the same input (`c a f e 314 201`). The suspicion was right — the code comment's
   "HFS+" is correct *for HFS+* and was doing duty for "macOS", which is wrong. Comment
   corrected; scope narrowed in ScrAP-270.
5. **NEW 2026-08-16 — `g_local_file_set_display_name`'s returned `GFile`**: that it is
   built from the `display_name` argument rather than re-read from the directory is
   INFERRED (from a measured `query_info` probe plus the NTFS end-to-end result), not
   source-read. `src/docio/rename.rs`'s `stored_spelling` rests on it. Out with the
   researcher for a `glocalfile.c` trace.

## Technical details preserved

- **`adopt_and_save` sets `st.path` before the write**, which is why the two
  back-sweep hypotheses above are about *Save As* rather than about rename: the
  ordering that makes them possible predates this feature.
- **`delete_snapshot` early-returns on `!tab.swap.on_disk`** and swallows
  `NotFound`, so a swap file filed under a stale stem is invisible from the
  deletion side. `swaprecovery::retire_source_snapshot` is the only thing that
  cleans up a diverged stem, and it only runs on the correlated recovery path.
- **The clean precondition is what keeps matrix row 3 a no-op for rename** — the
  dirty↔swap invariant means a clean document has no snapshot to orphan. That is an
  exemption by *position in the lifecycle*, exactly like Reading-Position CAM row
  10's, and it ends the moment rename is allowed on a dirty document. Record it in
  the row rather than leaving it implicit.
- **`accel::map` returns a bare F-key unchanged** on every platform (it only
  rewrites `<Primary>`/`<Meta>` and the `MAC_RESERVED` pairs), so `F2` needs no
  per-platform spelling and `accel::tests::bindings_are_unique_on_every_platform`
  is what proves it collides with nothing.
- **Mnemonic/access-key availability, measured against the tables**: File menu has
  `N O S l A R P u L C x` taken → `m` free (`Rena_me…`); the tab-strip context menu
  has `C O M F R` taken → `n` free (`Re_name…`).
