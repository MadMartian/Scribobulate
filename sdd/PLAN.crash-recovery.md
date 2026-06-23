# Plan: Crash recovery for unsaved buffers (swap files)

**Status**: **IMPLEMENTED IN FULL, 2026-08-01 — ready to retire on the operator's word.**
`src/swapfile/` (display-free core), `src/window/swap.rs` (write edge + the invariant's
choke point), `src/window/swaprecovery.rs` (startup pass); contract in
[TDD §22](TDD.md) (22.1–22.16, all covered); architecture in
[TECH.md](TECH.md#module-responsibilities), `sdd/system-overview.svg` and
[CAM.md](CAM.md) rows 8/10; manual checks in `tests/MANUAL-TEST.md` §22. Verified on the
operator's live X11 session, not only headlessly.

**⛔ THE WRITE MECHANISM DECISION WAS REVERSED AFTER MEASUREMENT — this is the most
important thing on this page.** The plan adopted option (b), `replace_contents_async`, on
a researcher source-trace. Measurement (2026-08-01) showed that call is **unsafe for
exactly this use**: on a write error it closes the stream and ignores the close result
(`gfile.c:7768-7776`), and for a local file close is where the temp→destination **rename**
happens — so an **ordinary disk-full** promotes a truncated temp over the previous good
snapshot. Measured: a known-good snapshot became a **0-byte file**. `REPLACE_DESTINATION`
is irrelevant to it and the async path is identical.

What shipped instead is option (b′): drive the stream manually — `replace_async()` +
`write_all_async` — and **on a write error close with an already-cancelled
`GCancellable`**, which unlinks the temp instead of renaming it
(`glocalfileoutputstream.c:415` → `:461-462`). This keeps **every** property (b) was chosen for — no
worker thread, the *open as well as the write* off the main thread, owner-only from the
first byte, payload moved not copied — and removes the data loss. The only thing actually
surrendered is the one-liner: (b′) is a nested pair of async callbacks instead of a single
call. (An interim version used the synchronous `replace()`, which reintroduced a blocking
`open(2)` per snapshot — invisible locally, a round trip per debounce on a network or FUSE
filesystem, i.e. a slice of exactly the cost option (c) was rejected for. `replace_async`
closes it; the directory creation is latched to once per process for the same reason.) Full analysis, the corrected route-C matrix
and the honest limits are **ScrAP-232**. Guarded by three tests including one that pins
the GLib branch the mitigation depends on, so an upstream change fails loudly.

**Two consequences worth stating, because they invert what this plan previously said:**

1. **Route C is not the hazard and never was.** It cannot fail for the reason that sent it
   there — `g_close(fd)` precedes the unlink and hands back the denied descriptor. The
   narrowing recorded below got the permissions half right (it does fail safe) and the
   destructive half wrong (EMFILE/ENOSPC *recover*).
2. **TDD 22.15 is therefore implemented, not deferred.** It was held back because a
   failure that had already destroyed the previous snapshot and one that had not needed
   different messages, and the `GError` cannot distinguish them. The mitigation removes the
   condition rather than the ambiguity: the previous snapshot is now genuinely intact, so
   there is one honest message.

**Also corrected during implementation** (the design was sound; three things it asserted
were not):

1. **The header codec's fence invariant was NOT free.** "TOML basic strings escape `\n`, so
   a correct serialiser upholds this automatically" is true of the TOML spec and false of
   the `toml` crate, which emits a **multi-line** string for any value containing a
   newline — forging the closing fence and truncating the recovered document. The test
   written to pin it failed on its first run: **ScrAP-233**.
2. **A recovery must reach `source`, not just the editor buffer.** Every derived view (the
   preview, outline, annotations) renders from `source`; setting only the buffer left the
   preview showing pre-crash content in **Preview mode, the application's default**. The
   whole automated suite passed because its assertions read the editor buffer — found only
   on the live display (ScrAP-56/ScrAP-87).
3. **The recovery notice had to retire with dirtiness.** Found by walking the Derived-view
   CAM's persistence column: left standing after a save, its "Discard recovery" button
   would have reverted work the user had just committed.

Operator decisions from kickoff, all shipped as stated: debounce **3 s** (size-scaled up
for very large documents, 30 s maximum-latency cap); **focus-loss flushes immediately**;
the recovery notice is **not a gate** and discard is revert-then-let-the-invariant-delete;
a failed snapshot is reported on **both** the status bar and a per-tab toast.

**Remaining open items** — none blocking; all are "cheap defensive extras", not gaps:
a defensive age cap on stale swap files (recovery already resolves every swap, so there is
no unbounded growth), and a log line if a swap ever survives two launches (unreachable
under the invariant).

**Requested by**: the operator, 2026-08-01.

**CAM reconciliation (2026-08-01).** Walked against [CAM.md](CAM.md) after the fact —
which was itself a process miss, since the matrices are meant to be read *before and
while* implementing. Three matrices bear on this change:

- **Derived-view CAM** — the recovery notice is now **row 8**, choke point
  `toast::sync_recovery_toast`. The walk found a **real defect in column B**: nothing
  retired the notice when the document stopped being dirty, so after a save it would keep
  offering "Discard recovery" — a button that reverts to disk, i.e. that would have thrown
  away the work just saved. Fixed at the dirtiness choke point, pinned by two
  mutation-tested integration tests.
- **Reading-Position Preservation CAM** — recovery swaps a text pane's buffer, so it is a
  perturbing event and is now **row 10**, recorded `n/a` *by position in the lifecycle*:
  it runs at startup, before anything has scrolled. Three specific future changes end that
  exemption and are named in the row.
- **Document-Reference CAM** — no cell, by construction: a swap file carries whole text
  plus a re-resolved baseline digest, never an offset. Recorded there as a warning, since
  adding a caret or scroll position to the header (an open item below) would create a held
  reference across a process restart.

The **Action CAM does not apply**: "Keep" / "Discard recovery" are toast-local buttons
reachable from exactly one surface by construction, the same shape as the conflict toast's
Reload/Dismiss, so there is no second surface for a cell to catch them missing from.

Two decisions the operator settled at kickoff and that shipped as stated: the debounce is
**3 s** (size-scaled upward for very large documents, with a 30 s maximum-latency cap),
and **leaving the editor pane flushes immediately** rather than waiting the timer out.

**Requested by**: the operator, 2026-08-01.

## Problem

An unclean exit loses every unsaved edit in every open buffer. The application
persists *layout* across sessions — which windows, which tabs, which paths, which
view modes — but deliberately persists **no document content** (`session.rs` module
doc: "No tab content is persisted"), so a restored tab re-reads its file from disk
and an untitled tab restores blank. A clean quit is safe, because the close path
prompts Save/Discard/Cancel for every dirty tab. Nothing else is:

- a SIGSEGV inside GTK/GIO (the process does occasionally die this way with no
  reproduction — the motivation for `forensics/`),
- a SIGKILL from the OOM killer (GTK4Rs/AP-133 documents this happening to *test*
  instances already),
- a power loss, a session-manager kill, a `kill -9`.

In each case the user's work exists only in a `GtkTextBuffer` that is now gone.
The gap is asymmetric with what the project already invests in durability:
`atomic_io` goes to considerable lengths so a crash *mid-write* cannot tear a file,
while a crash *between* writes silently discards everything typed since the last
save.

### Root cause

There is no mechanism at all — this is a missing feature, not a defect. Two
existing properties define its edges:

- **Nothing writes document content except an explicit save.** `window/save.rs` is
  the only path to `atomic_io::write_atomic` for document text, and it is driven by
  a user command.
- **The crash path cannot help.** `forensics/signal.rs` may not allocate or lock,
  so the fatal-signal handler cannot serialize a `GtkTextBuffer` — and a SIGKILL or
  a power loss never reaches a handler at all. Recovery data must therefore already
  be on disk *before* the crash. This rules out the whole "flush on death" family
  and forces a periodic-snapshot design.

## Requirements (from the operator's brief)

1. Unsaved buffer content is written periodically to a central per-user location.
2. The write is debounced, atomic, and must not block the GTK main thread.
3. On restart, after the normal session restore, the directory is scanned and
   recovered content is applied — restoring the pre-crash state, not just the
   pre-crash layout.
4. A canonical save deletes the associated swap file.
5. A swap file whose content matches its twin on disk is deleted.
6. Untitled buffers (never saved, no path) are covered too, and flagged as such.
7. A naming convention that maps documents from anywhere in the filesystem into one
   flat directory.
8. *(added 2026-08-01)* A **frontmatter-style metadata header** in each swap file,
   so a swap file identifies its own companion document without consulting anything
   else — explicitly because the swap directory and the stored application state can
   drift apart, with nothing guaranteeing they stay in sync.
9. *(added 2026-08-01)* A general-purpose **status-bar message** when a recovery
   occurs, alongside the per-tab toast.
10. *(added 2026-08-01)* An **immediate snapshot whenever the editor pane loses
    focus** — switching view mode, opening a menu, moving to another window, or
    leaving the application entirely. The debounce exists to absorb typing; the
    moment the user's attention leaves the buffer is a natural, cheap commit point
    and there is no reason to keep waiting out a timer through it.
11. *(added 2026-08-01)* A **failed snapshot write is reported on both surfaces** —
    the window status bar *and* a tab-specific toast — for the same
    per-window/per-tab reason requirement 9 gives.

Two of these collapse into one invariant, which is the cleanest way to state the
whole mechanism and is proposed as its governing rule:

> **A swap file exists for a document if and only if that document is dirty.**

Requirement 4 is the "became clean by saving" case and requirement 5 is the
"became clean by editing back / undo" case; both are just the invariant's negative
half. Implementing the invariant directly — rather than two special-cased deletion
rules — means every future path that changes dirtiness (reload, discard, revert)
gets the right behaviour without being individually taught it (POLICY § "one path,
not two"; ScrAP-116's choke-point reflex).

## Naming: what to call these

The operator's instinct — "swap file" — is worth keeping, with one caveat recorded
so the name doesn't over-promise. Vim's `.swp` is a *journal* held open for the
whole edit session, and it doubles as a **lock** (a second Vim opening the same
file finds the swap and warns). What is proposed here is a **full-content snapshot
rewritten on a debounce**: simpler, never partially applied, but not a lock and not
incremental.

Recommendation: **"swap file"** as the user-facing and code-facing term (familiar,
short, matches the operator's brief), defined once in the module doc as "a periodic
full-content recovery snapshot of a dirty buffer". Directory `swap/`, extension
`.swap`, module `swapfile.rs`. Alternatives considered and rejected: *autosave*
(implies the user's own file is being written, which is exactly what this must
never do), *backup* (implies a retained history), *recovery snapshot* (accurate but
three syllables too long for the hundred places it will appear).

## Location on disk

**`$XDG_STATE_HOME/scribobulate/swap/`** — i.e. `~/.local/state/scribobulate/swap/`
on a default Linux profile — reached through the **existing**
`session::state_directory()`, which is already documented as the single lookup in
the tree for the user state directory and is already shared by `session.toml`, the
forensic log and crash reports.

This is not a new decision so much as an existing one applied: TECH.md's platform
notes state the rule outright — "configuration should roam between machines;
session state should not" — and swap content is machine-generated, host-local,
short-lived state. Reusing `state_directory()` also inherits the Windows/macOS
fallback (`%LOCALAPPDATA%`) and the ScrAP-167 "warn once when no state directory
resolves" behaviour for free.

The three alternatives in the operator's brief, and why not:

| Candidate | Verdict |
|---|---|
| `~/.config/scribobulate/` | **No.** That is `XDG_CONFIG_HOME` — user configuration, and the one directory the project deliberately arranges to *roam*. Syncing a machine-local crash artefact between machines invites a recovery prompt on a machine where the crash never happened. It would also be the only writer besides `config.toml`, which the app only ever reads. |
| `~/.scribobulate/` | **No.** A bare home dotdir predates the XDG spec; the project already resolves XDG properly everywhere and this would be its one exception. |
| `~/.local/scribobulate/` | **No.** Not an XDG location at all — `~/.local` is only a parent for `share/` (data) and `state/` (state). |

**A central directory rather than a sidecar next to the document** (vim's
`.file.swp`) is the right call here for a project-specific reason worth recording:
each open document has a `gio::FileMonitor` on it, and writing a sibling file into
the watched directory would feed the monitor a stream of events it must then learn
to ignore — a second, harder instance of the ScrAP-54 self-delete guard. A central
directory keeps the document's own directory untouched, so the monitor sees
nothing. It also works for read-only and non-writable document directories, and for
untitled buffers that have no directory at all.

**The directory and its files must be owner-only (`0700` / `0600`).** They hold
verbatim document text, including from documents the user has deliberately
`chmod 600`'d. Note this is *not* what `atomic_io::write_atomic` does today: for a
brand-new file it applies `default_new_file_mode()` (the umask-derived `0666 & !umask`,
typically `0644`) — correct for a user document, wrong for a swap file. A swap
writer must pin `0600` unconditionally — one of the reasons the chosen writer is
GIO's `replace_contents_async` with `PRIVATE` rather than a reuse of `write_atomic`
(§ RESOLVED). The *directory* is still ours to create at `0700`, mirroring
`forensics::private_options`, which already makes crash reports owner-only through
one shared `OpenOptions`.

## File naming and identity

The operator proposed naming each swap file after a sanitized absolute path of its
twin. That is the obvious design and it has four failure modes worth stating before
choosing against it:

1. **Length.** A sanitized `/home/u/Documents/Projects/…/notes.md` easily exceeds
   the 255-byte filename limit on ext4 (and the whole path then bumps Windows'
   `MAX_PATH`, which the `atomic_io` Windows test already has to work around).
   Truncating to fit destroys uniqueness — the exact property the scheme exists for.
2. **Injectivity.** A sanitizer that maps `/` and any other illegal character to a
   single replacement is not injective (`a/b` and `a:b` collide). Percent-encoding
   is injective but makes case 1 worse.
3. **Case folding.** macOS (APFS default) and Windows are case-insensitive, so
   `Notes.md` and `notes.md` name one document but two swap files.
4. **It does not answer requirements 6 or 7's hard half.** An untitled buffer has
   no path to derive a name from, and a document that is renamed or moved on disk
   between snapshots silently orphans its own swap file.

### Proposed scheme: a per-document id, with a human-readable prefix

Give every tab a **`doc_id`** — 128 random bits, lowercase hex — allocated when the
tab is created and carried for the tab's life, including across a save, a Save As,
and a cross-window move. Persist it in `TabSession`, and write it into the swap
file's header.

Filename: **`<sanitized-stem>-<doc_id>.swap`**, e.g.
`notes-3f2a…c91.swap`, or `untitled-3f2a…c91.swap` for a buffer with no path. The
stem is cosmetic only — sanitized to `[A-Za-z0-9._-]`, truncated to 32 bytes, never
parsed. **The `doc_id` is the identity and the header is authoritative**; nothing
downstream ever reconstructs a path from a filename.

What this buys:

- Correlation with a restored tab is exact, for titled *and* untitled documents
  alike, which is what requirement 6 actually needs.
- Two windows with the same file open get two swap files, not one that they
  overwrite alternately.
- A rename or move of the twin between snapshots is harmless.
- Bounded length, no illegal characters, no case-folding hazard, no sanitizer
  injectivity requirement.
- A human listing the directory can still see which document is which.

`doc_id` on `TabSession` is an **additive, non-breaking** schema change — the struct
is `#[serde(default)]`, so an existing v3 session file simply yields `None` and each
such tab gets a fresh id on restore. No v4 bump and no migration function is needed
(contrast the v1→v3 machinery `session.rs` documents).

## File format: a frontmatter header (operator decision, 2026-08-01)

One file per document, one atomic write, no sidecar metadata file (two files can
desync; one cannot). The header uses the **frontmatter idiom** — a delimited
metadata block at the top of the file, the same shape agent skills and static-site
generators use — because it survives the thing this mechanism exists for. Layout:

```
+++scribobulate-swap 1          <- opening fence: magic + format version, one line
doc_id = "3f2a…c91"             <- TOML
path = "/home/u/Documents/notes.md"
untitled = false
…
+++                             <- closing fence, alone on its line
<the buffer's bytes, verbatim, to EOF>
```

### Why frontmatter, and why this spelling

The **decisive argument is the operator's**: nothing transactionally couples the
swap directory to `session.toml`. They are written by different paths at different
moments, and a crash can land between them, so the two *will* drift. A
self-describing file is what makes that survivable — which promotes an
implementation detail into a design principle worth stating plainly:

> **The swap file is self-sufficient. `session.toml` is advisory.**
> Every fact needed to recover a document — which file it belongs to, whether it
> was untitled, what its on-disk baseline was — lives in the swap file's own
> header. The session file only helps decide *which window and tab* to put it back
> into, and recovery must be correct without it.

That principle also decides the recovery algorithm (§ below): **header first,
session as a hint** — never session-first with the header as confirmation.

Frontmatter's second payoff is manual recoverability. If the app won't start, or
the recovery path itself is the thing that's broken, a swap file opens in any
editor — including Scribobulate — and states its own provenance in the first few
lines. A length-prefixed binary-ish header is unambiguous to a parser and a puzzle
to a human holding the only remaining copy of their work.

Three spelling choices, each load-bearing:

- **`+++` fences, not `---`.** The payload is Markdown, where a `---` line is both
  a thematic break and a setext `<h2>` underline — and may itself be a document
  with its own YAML frontmatter. Parsing is safe either way (see the invariant
  below), but `---` makes the file ambiguous to *other* tools and to a human
  skimming it, and a truncated header would leave a body that still looks
  frontmatter-fenced. `+++` is the Hugo/Zola convention for exactly this — TOML
  frontmatter — and is vanishingly rare in prose.
- **TOML inside, not YAML.** The idiom is what's valuable, not the language. TOML
  costs **no new dependency**: `toml` + `serde` are already in the tree and already
  carry `config.toml` and `session.toml`, so the swap header speaks the project's
  existing metadata language rather than introducing a second one. (YAML would mean
  adding a parser, and the obvious crate — `serde_yaml` — was archived upstream in
  2024, so the choice there is between unmaintained and a fork. Not a trade worth
  making for a header of eight scalars.)
- **The magic is on the opening fence.** `+++scribobulate-swap 1` gives file-type
  identification and format version in the line that would otherwise carry no
  information. A file whose first line doesn't match is **not ours: ignore it, log
  it, and never delete it** — a state directory is a shared place and this
  mechanism must not become a file shredder for anything that lands there.

### The one invariant frontmatter needs (the length prefix had it for free)

A delimited format is only unambiguous if the terminator can't be forged. Two
halves:

1. **Bounded terminator search.** Scan for the closing `+++` only within the first
   ~8 KiB / 64 lines. Past that the file is malformed — do not scan 64 MiB of
   payload looking for a fence. (Content *after* the terminator is irrelevant: the
   body may contain `+++`, `---`, or a whole nested frontmatter block, because the
   scan has already stopped.)
2. **No header value may serialize to a bare `+++` line.** The live hazard is
   `path`: on unix a filename may legally contain a newline, so a crafted path could
   otherwise inject a fence and truncate the recovered document. TOML basic strings
   escape `\n`, so a correct serializer upholds this automatically — but it is an
   *invariant of the serializer* rather than a property of the format, which is
   precisely the difference from the length-prefix design, and therefore exactly
   what needs a test: a document whose path contains an embedded newline and `+++`
   must round-trip byte-identically.

Header fields (proposed):

| Field | Why |
|---|---|
| `doc_id` | Identity; correlates to the restored `TabSession`. |
| `path` | The twin's absolute path; absent for an untitled buffer. |
| `untitled` | Explicit flag rather than inferring from a missing `path` — requirement 6, and it keeps a malformed header from silently becoming an untitled recovery. |
| `baseline_hash` | Hash of the twin's content as of the last load/save (`TabState::saved_baseline`). Lets recovery detect that the file changed on disk since the crash. |
| `written_at` | For the recovery prompt's wording and for stale-swap pruning. |
| `pid`, `boot_id`/process-start | Liveness guard — see below. |
| `app_version` | Forensic value; costs one line. |

## Writing: debounce, snapshot, and the thread question

Three stages, and it matters which thread each runs on:

1. **Trigger.** Two, and they are different in kind:
   - **The debounced one**: `editor_buf.connect_changed` — the same signal
     `window/livepreview.rs` already debounces at 300 ms. Guarded by `st.loading`
     exactly as live preview is (a programmatic buffer replacement from a load or an
     external reload is not a user edit and must not arm a snapshot).
   - **The immediate one (operator decision, 2026-08-01)**: the editor pane
     **losing focus** flushes any armed debounce at once, rather than letting the
     timer run out. Switching view mode, opening a menu, activating another window,
     switching application — each is a moment the user has stopped typing *and* has
     signalled it, so the snapshot costs nothing perceptible and closes the window in
     which a crash would lose the last few seconds of work. Mechanically: cancel the
     pending timer and take the snapshot on the same path, never a second parallel
     one (the choke point below). Two details this must respect —
     **(i)** the flush must be idempotent and skip when nothing is armed, because
     focus churns far more often than the buffer changes; and **(ii)** it must not be
     keyed on a raw per-widget focus signal, which flickers mid-interaction — use
     the same window-level focus discipline the action-sensitivity gating already
     uses (GTK4Rs/AP-20), so a menu popover taking focus is a flush and not a
     flapping pair of flushes.
2. **Snapshot.** `TabState::editor_text()` → an owned `String`. **This must be on
   the main thread** — reading a `GtkTextBuffer` off-thread is forbidden (POLICY
   § "All GTK access on the main thread", gtk4-rs guardrail #1). No design can move
   this cost off the main thread; a worker thread can only take the *write*.
3. **Write.** Atomically, to the swap directory.

**Debounce policy**: an idle debounce of **3 s** (operator decision, 2026-08-01 —
fires when typing pauses) plus a **maximum latency cap** of ~30 s (so a user typing
continuously for ten minutes is still snapshotted, rather than starved by a debounce
that never expires). The cap is the part a naive debounce gets wrong and the one that
decides how much work a crash costs. Both values belong in `config.toml` with these
defaults. Note the focus-loss flush above changes what the 3 s actually has to cover:
the debounce now only ever elapses *while the user is still typing into the pane*,
because any departure from it commits early.

**Size policy**: at the 64 MiB ceiling, a snapshot is a 64 MiB `String` copy plus a
64 MiB write, per debounce. Proposal: scale the idle debounce with document size
(e.g. `3s` under 1 MiB, growing to `30s` at the ceiling) and record the chosen curve
in `limits.rs`, the existing single source of truth for input-cost bounds. A flat
3 s on a very large document would be a self-inflicted performance defect. The
focus-loss flush is deliberately **not** size-scaled — it is a one-shot at a moment
the user has stopped interacting, which is exactly when a large copy is affordable.

### RESOLVED: the write mechanism — option (b), no thread (researcher, 2026-08-01)

**Verdict: use `replace_contents_async` with
`FileCreateFlags::REPLACE_DESTINATION | FileCreateFlags::PRIVATE`. Add no worker
thread. The concurrency model does not change.** Source-traced against GLib
**2.72.4** — the jammy host's exact version — and diffed to `main` (2.89.3):
*no deltas that matter*, so both target platforms behave identically and no
version-conditional code is needed. Full write-up:
`~/Documents/Projects/AI/Research/Gtk4Rust/researcher-findings-gio-replace-contents-atomicity-durability-threading.md`.

The three things that had to be true, and how they came out:

- **Off-main-thread: confirmed, genuinely.** `g_file_real_replace_async` →
  `g_task_run_in_thread`, and the write goes via threads because
  `GLocalFileOutputStream` neither overrides `write_async` nor implements
  `GPollableOutputStream` (so `g_output_stream_async_write_is_via_threads` returns
  TRUE). The completion callback returns on the thread-default main context, so it
  is GTK-safe — and gio-rs 0.21 *enforces* both halves (asserts main-context
  ownership, `ThreadGuard`s the callback) rather than merely permitting them.
- **Private from the first byte: confirmed.** `PRIVATE` → mode `0600` passed
  straight to `g_open(2)` via `g_mkstemp_full`, never `chmod`'d afterwards. The
  disclosure hazard that `atomic_io` had to be fixed for twice does not exist here.
- **Zero copy: confirmed.** The bound `replace_contents_async` *takes ownership* of
  a `Vec<u8>`, makes no copy, and **hands the buffer back in the callback** — so the
  64 MiB allocation can be recycled between snapshots. (Do **not** plan around
  `replace_contents_bytes_async`: it is an un-bound TODO in gio-rs 0.21.5.)

**`REPLACE_DESTINATION` is mandatory, not stylistic.** Without it GIO truncates in
place for a symlink or hard-linked target, *and* re-applies the existing file's mode
over our `0600` on every overwrite — so a swap file that ever acquired a laxer mode
would keep it forever. With it, the temp file's mode wins and the privacy is
self-healing.

#### Two costs to design around, not assume away

**1. There is no power-loss durability, at any flag setting.** GIO fsyncs the temp
before the rename *only when the destination already existed* (`sync_on_close` is
set solely on the EEXIST branch), and it **never** fsyncs the parent directory —
one `fsync` call in the whole file, on 2.72.4 and on `main` alike. Consequences,
stated plainly because the mechanism's whole purpose is durability:

| Failure | Outcome |
|---|---|
| SIGSEGV, OOM kill, `kill -9` (kernel survives) | **Identical to our own `write_atomic`** — page cache intact, the rename is visible. This is the primary threat and it is fully covered. |
| Power loss / hard reset | May lose the newest rename, leaving the **previous** snapshot rather than a torn file. |

That is graceful degradation for a debounced periodic snapshot, and it is accepted
— but it is a real narrowing of scope versus the document-save path, which does
fsync both. **If power-loss durability ever becomes a requirement, option (b)
cannot deliver it and the decision reverts to (a).**

**2. Route C: a failed write can destroy the previous good snapshot.** Four code
paths fall back to truncate-in-place; `REPLACE_DESTINATION` closes three (symlink,
hard link, and the fchown/fchmod-mismatch path). It does **not** close the fourth:
if `g_mkstemp_full` fails, GIO takes `fallback_strategy`, and with
`REPLACE_DESTINATION` set that path **`g_unlink`s the destination** before
reopening it — deleting the previous snapshot before failing to write the new one.

This is worse than a torn file, because a torn file is *detectable* (a missing
closing fence) while a deleted one is indistinguishable from "there was nothing to
recover".

⚠ **NARROWED 2026-08-01 — and the narrowing moves the mitigation.** The first
version of this section said the trigger was an *unwritable swap directory*, and
prescribed a startup writability check. That is very probably wrong: **`g_unlink`
needs write permission on the containing directory — exactly what `g_mkstemp` was
just denied** — so a permissions failure should fail the unlink too and leave the
old snapshot intact. The genuinely destructive window is **any failure that stops
`mkstemp` without stopping `unlink`: ENOSPC, EDQUOT, EMFILE/ENFILE.**

Consequences, and this is why the correction is worth more than the original claim:

- **A startup writability check is not a mitigation.** It probes permissions, and
  permissions are the case that fails safe. Free space and fd exhaustion are runtime
  states that a startup probe cannot see, so the check would have bought
  reassurance and no protection — the worst kind of guard.
- **Surfacing the error is the only mitigation that covers the real triggers.** A
  swap write that fails is the one moment the user needs to know their safety net is
  off, and under the true trigger conditions it may already have cost them the
  previous snapshot. This is now the *primary* requirement, not one of two.
  **On both surfaces (operator decision, 2026-08-01)**: the window **status bar**
  *and* a **tab-specific toast**, mirroring the recovery notice's own split — the
  toast answers "*this* document is unprotected", the status message answers
  "something is wrong with saving in this window". Both matter here for a reason
  the recovery case doesn't have: a failure is silent by nature (nothing visibly
  changes when a snapshot *doesn't* happen), so a single surface the user isn't
  looking at is equivalent to no surface. Two constraints on the wording and
  lifetime: it must say the *safety net* is off rather than implying the document
  failed to save (the user's file is untouched and still saveable), and it must not
  re-fire per debounce tick — a full disk would otherwise emit a toast every few
  seconds. Report the transition into the failed state, and clear it on the first
  success.
- If a precondition is still wanted, it has to include a **free-space** element, and
  it has to be re-evaluated at write time rather than once at startup.

*(Source-traced, not probed — and the narrowing above is likewise reasoned, not yet
measured. Two probes are queued: `chmod 500` (expected to fail safe) and a forced
fd-exhaustion case (expected to exhibit the real interleaving), plus capture of the
`GError` domain/code so the destructive failure can be told apart from an ordinary
one. **Do not implement the error-handling branch until those land** — the whole
point of this section is that the two cases need different responses.)*

#### The call site

```rust
file.replace_contents_async(
    payload,                       // Vec<u8>, moved; returned in the callback — recycle it
    None,                          // etag: we are the only writer; WRONG_ETAG is a needless failure mode
    false,                         // make_backup: an extra rename and a ~sibling, for no benefit
    FileCreateFlags::REPLACE_DESTINATION | FileCreateFlags::PRIVATE,
    Some(&cancellable),
    move |res| { /* runs on the main context; GTK-safe */ },
);
```

**We must serialise our own snapshots — GIO will not.** Two in-flight replaces for
the same document can land out of order, which would silently resurrect an older
buffer state. The main-thread debounce is already the natural choke point: gate on
an "in flight" flag per document, and let the completion callback clear it and fire
any snapshot that was coalesced while it ran. This is the same latest-wins
coalescing option (a) would have needed in its slot map, minus the thread.

#### The options as they stood before the answer

Kept because the decision above is contingent on facts that could change (a GLib
that starts fsyncing, or a power-loss requirement), and because (a) is the standing
fallback.

**Operator constraint, 2026-08-01, binding on the choice below**: the UX must not
be degraded by I/O activity on the main thread on every buffer modification. That
rules **(c) out as a shipping answer**, whatever the measurements say, and it means
the fallback if GIO disappoints is **(a)**, not "(c) with a longer debounce". Note
one thing the constraint cannot buy, stated so it isn't assumed away: the *snapshot
copy* (stage 2) is main-thread-bound by GTK itself, so "no main-thread work per
edit" is unreachable — what is reachable is "no main-thread **I/O**", which is the
part that blocks unpredictably. The residual snapshot cost is bounded by the
size-scaled debounce above, not by the thread choice.

**(a) A dedicated writer thread + our own `write_atomic`.** The operator's
suggestion. Main thread snapshots and hands an owned `SwapJob { doc_id, header,
content }` to one long-lived writer thread. **Not** a mutex around shared state —
the mutex protects a *pending-job slot map* (`HashMap<DocId, SwapJob>`, latest wins)
so that a burst of debounce fires coalesces into one write per document, with a
`Condvar` to wake the writer. GTK objects are never visible to that thread; only
owned `String`s cross the boundary.

- **Pros**: reuses `atomic_io`, whose guarantees are already audited to death
  (mode/owner preservation, private-from-first-byte temp, unique temp names, parent
  fsync, `TempFileGuard` cleanup). Fully in our control. A slow filesystem (NFS,
  FUSE, a synced folder) cannot stall the UI.
- **Cons**: **it ends the project's "entirely single-threaded" property**, which
  TECH.md § Concurrency model states as a fact and POLICY leans on ("the app
  currently spawns no worker threads"). Both documents would need revising. It also
  adds shutdown ordering (drain-and-join before exit, without hanging on a stuck
  write) and a new class of test.

**(b) `gio::File::replace_contents_async` on the main thread.** No new thread, no
mutex, no change to the concurrency model; GIO owns the atomicity and the
off-thread dispatch. **Viable only if** GIO's local-file replace genuinely does
temp-file + rename (rather than ever truncating in place), `G_FILE_CREATE_PRIVATE`
gives `0600` from the first byte, and the async really is pooled rather than
main-loop-chunked. Those are precisely the questions in the researcher brief — and
they are exactly the kind of "the API name implies the guarantee" assumption this
project has been burned by before, so **(b) must not be adopted on the strength of
its documentation alone.**

**(c) Main thread, synchronous `write_atomic`, no thread at all.** **Rejected by
the operator** (above); kept here only so a later reader knows it was considered
and why it lost. It is the honest baseline — a few hundred KB on a 2 s debounce is
imperceptible on a local disk, and the project already accepts synchronous
main-thread I/O on the save and load paths as a documented limitation.

- **Pros**: smallest change by a wide margin; nothing new to reason about.
- **Cons**: unlike save/load, this fires *unprompted, forever, while the user
  types*. A user who would tolerate a 200 ms hitch when they press Ctrl+S will not
  tolerate one arriving unbidden every 2 s on a network filesystem. The main-thread
  block also has a known ugly signature (GTK4Rs/AP-148: a synchronous main-thread
  block freezes even the spinner meant to indicate it).

**Outcome**: (b), as detailed above. (a) remains the fallback if the durability
scope ever widens. (c) is rejected.

Regardless of which is in force, the writer must sit behind **one choke-point
function** so a future swap between them is a single-file change
(GTK4Rs/AP-130's enforcement ladder).

## Recovery on restart

Ordering, per the operator's brief: **after** the normal session restore has built
the windows and tabs (`window::restore::restore_session`), **before** the deferred
pre-render pump starts warming background tabs.

1. Scan `swap/`. **A non-empty directory means the last exit was unclean** — that
   property falls straight out of the governing invariant plus the fact that a clean
   quit resolves every dirty tab through Save (deletes the swap) or Discard (deletes
   the swap). This is the whole detection mechanism; no "clean shutdown" marker file
   is needed.
2. For each swap file, parse the header and apply the **liveness guard**: if `pid`
   names a live process that is a Scribobulate started at the recorded time, another
   instance owns this swap — skip it, don't touch it. (Reachable via
   `--new-instance` and on macOS; cheap insurance against two instances fighting.)
3. Correlate — **header-first, session-as-a-hint**, per the self-sufficiency
   principle. The swap file's own header says what the document *is*; matching a
   `doc_id` against a restored tab only says where to *put* it. So the set of
   documents to recover is decided entirely from the headers, and every branch below
   is reachable:
   - **`doc_id` matches a restored tab** → apply into that tab's buffer.
   - **no match, has a path** → the session didn't restore it (drift: the crash
     landed between the swap write and the session write, or the user closed the
     window in a way that never persisted). Open the file and apply — or open a
     recovered tab holding just the swap payload if the file is now gone.
   - **no match, untitled** → create a tab titled *Untitled (recovered)*.
   - **a restored tab with no swap** → nothing to do; it was clean.
   Drift in the *other* direction is the case that makes this ordering matter: a
   session file listing a tab whose swap is absent must never be treated as evidence
   the swap was lost. Absence of a swap means "clean", always.
4. Applying means: set the buffer text from the swap payload, leave
   `saved_baseline` at the on-disk content, and therefore **the tab comes back
   dirty — exactly as it was before the crash**. Nothing is written to the user's
   real file.

### One deviation from the brief, deliberately proposed

The brief says "applies them automatically". Automatic application is proposed
**with a visible, dismissible notice per recovered tab**, not silently:

- Silent application is safe for the *file* (nothing is written without an explicit
  save) but not for the *user*, who has no way to know their buffer differs from
  what is on disk, and no route back to the on-disk version.
- The project already owns the right widget: `window/toast.rs`'s conflict toast, an
  in-window prompt with actions, used for exactly this shape of decision.
- Proposed wording/actions: *"Recovered unsaved changes from <time>"* with
  **Keep** (dismiss) and **Discard recovery**.

**The notice is not a gate (operator decision, 2026-08-01).** The recovery is
already applied by the time the toast appears — there is no branch in which the
buffer is left un-recovered pending an answer, and no "recover?" prompt. That
ordering is what makes the discard action trivial rather than a second recovery
pipeline run backwards: because the tab is a normal dirty tab holding recovered
content, **Discard recovery** is exactly two existing operations in sequence —

1. **reload the file from disk**, which is the ordinary revert path and by itself
   returns the buffer to the on-disk content and clears the dirty flag; then
2. **delete the swap file.**

Note that step 2 is, strictly, already implied by step 1 under the governing
invariant — the reload makes the tab clean, and a clean tab has no swap file — so
the correct implementation is to let the reload flow through the same
dirtiness choke point every other path uses, and *verify* the swap is gone, rather
than to bolt a bespoke deletion onto the discard action. A discard that deletes the
swap by hand is a second deletion path, which is the shape ScrAP-116/ScrAP-219 warn
about; a discard that merely reverts and lets the invariant do the deleting is not.

This keeps "true restoration" — the buffer content and the dirty state are both
exactly as they were — while remaining reversible.

### The status-bar notice (operator decision, 2026-08-01)

Alongside the per-tab toast, a **general-purpose status-bar message** reports that
a recovery happened at all — e.g. *"Recovered unsaved changes in 3 documents"*. The
toast is per-tab and answers "what happened to *this* document"; the status message
is per-window and answers "something happened to this session", which is the fact a
user needs *before* they start clicking through tabs.

Mechanically this must be a **`StatusStack::push`** (a transient notice), **not
`set_base`**. The base entry is already spoken for: a recovered tab is by
construction dirty, so its base message is *"Unsaved changes"*, and a recovery
message written with `set_base` would either overwrite that or be overwritten by
the first `refresh_dirty_status` — the two would fight, silently, with the winner
decided by ordering. `push` stacks the notice above the base and `pop` restores it
untouched, which is exactly the intended lifetime. Retain the returned `StatusCtx`
per window and pop it on the first user interaction (or when the last recovery
toast is dismissed) so it doesn't become permanent furniture.

Two details worth pinning now rather than discovering later: the count is
**per window** (each window reports its own recovered tabs, matching the status
bar's own scope), and a window that recovered nothing shows nothing — an empty
recovery must be silent, not *"Recovered 0 documents"*.

**One case must not auto-apply**: if `baseline_hash` no longer matches the twin's
current on-disk content, the file changed since the crash and the recovery would be
against a stale baseline. That is the existing external-change conflict, and it
should route into the existing conflict flow rather than grow a parallel one
(TDD §5).

## Known traps this design has to respect

Collected here because each one has already cost this project something once:

- **ScrAP-54 (self-delete guard).** Not triggered, *because* the swap directory is
  not the document's directory — but a future "sidecar swap" refactor would trip it
  hard. Recorded so the central-directory choice isn't quietly reversed.
- **ScrAP-152 (deferred-callback zombie).** The debounce timer is a
  `timeout_add_local_once` that must weak-capture, and must be cancelled when the
  tab is torn down. The `window/livepreview.rs` debounce is the pattern to copy.
- **ScrAP-116 / ScrAP-219 (choke points and the enforcement ladder).** Swap deletion must hang off the single
  place dirtiness is recomputed (`refresh_dirty_status`) plus the tab-teardown path
  — never be re-applied by hand at each of `do_save` / `adopt_and_save` /
  `save_and_then` / the discard branch. A per-call-site rule is a regression waiting
  for the next call site.
- **`session::FROZEN` (ScrAP-81).** A coordinated quit freezes session writes across
  a shrinking window set. Swap deletion must be evaluated against *that* sequence
  deliberately — a Discard during a coordinated quit must still delete its swap, or
  the next launch resurrects work the user explicitly threw away.
- **`st.loading`.** Load and external reload replace the buffer programmatically;
  neither may arm a snapshot.
- **The crash path may not allocate or lock.** No part of this mechanism may be
  invoked from `forensics/signal.rs`.
- **Testability (gtk4-rs guardrail #4).** The header codec, the filename
  sanitizer, the dirty↔swap invariant, the correlation decision, and the
  stale/liveness decision must all be pure functions over plain data, unit-testable
  with no display. Only the writer and the buffer read touch GTK/the filesystem.

## Open items to settle at kickoff

- ~~The write mechanism~~ — **settled 2026-08-01**, see § RESOLVED.
- The route-C probe (`chmod 500` the swap directory; confirm whether the previous
  snapshot is unlinked before the write fails). Source-traced, not measured.
- Whether a swap is also written for a **clean** tab. Proposed: **no** (the
  invariant). The cost is that a crash loses nothing that wasn't already on disk,
  which is the correct trade.
- Retention/pruning: what happens to a swap whose twin no longer exists and which
  the user keeps dismissing. Proposed: recovery always resolves a swap (apply or
  discard), so no unbounded growth — but a defensive age cap is cheap.
- Whether recovery is offered for tabs the user has since closed cleanly in a
  *later* session (i.e. a swap file that survived two launches). Should be
  unreachable under the invariant; worth a log line if it ever happens.
- TDD rubrics. Per the SDD plan-kickoff rule these are proposed *before*
  implementation begins, not now — the behaviour to cover is at minimum: the
  dirty↔swap invariant in both directions, untitled recovery, recovery of a tab the
  session did not restore, the changed-on-disk case, "a clean quit leaves the swap
  directory empty", the header round-trip (including a path containing a newline and
  a `+++` line), a foreign file in the swap directory being left strictly alone, and
  the recovery status message appearing and then clearing.
