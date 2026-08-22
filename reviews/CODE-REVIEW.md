# Consolidated Code Review: `mitigations` (the `bugs/various` merge) — Scribobulate

**Branch:** `mitigations` (== `master`), HEAD `fd83582`
**Last updated:** 2026-08-21
**Review scope:** Round 1 — the `bugs/various` merge, diff-anchored to `fd83582^1..fd83582^2`
(13 commits, `b890c9c..98fc41c`; 58 files, +8009 / −518). Working tree MEASURED clean
(`git status --porcelain | wc -l` → 0).

**Pipeline:** 14 reviewers (3× spec/SDD, 3× DRY/abstraction, 3× anti-pattern, 3× testability,
1× security, 1× link integrity; 10 conceptual groups → 3 agents per scalable type, file order
alternated A→Z / Z→A to cancel positional attention bias) → nitpick filter (1 Haiku scorer;
81 Low/Tidy scored, 35 dropped) → line-reference verification (14 Haiku batches, 330 claims) →
orchestrator verify-then-include gate → this document.

**Raw → consolidated:** 178 findings raised, 35 dropped by the nitpick filter, **145 carried
forward**; the 27 High findings deduplicate to **19 distinct defects**.

> **Note on the previous campaign.** `docs/code-review.md` previously held the terminal Round 5
> of the `mitigations/linux` campaign. That campaign's consolidated review, audit trail and 34
> uncleaned per-round scratch files were archived to
> `docs/archive/campaign-mitigations-linux-r1-r5/` before this round began. Nothing was deleted.

---

## 🔒 Security Review

**One HIGH-confidence finding, two Needs Verification.** The security reviewer cleared four of
the five nominated attack surfaces *with stated reasons* rather than silence, which is worth
recording: the lone-CR/CRLF byte scans are provably char-boundary-safe and length-preserving;
the clipboard rework is a strict **reduction** in what leaves the process (plain `STRING` only,
weak buffer ref); the PDF path emits no hand-written PDF object syntax at all (cairo owns
escaping — the real surface is Pango markup, where every attacker-controlled string passes
`escape_pango` and every markup *attribute* originates in the theme); and both lint scripts
enumerate via `git ls-files -z` on a NUL boundary with fully quoted expansions and no
`Invoke-Expression`. Path handling was **tightened** by this merge, not loosened — the admission
check now runs on every re-read rather than once at open time.

### [SEC-01] Negative printable width reaches the table grid unclamped — Low (merged into F-PDF-001)  ✅ **Done** (via F-PDF-001)
Attacker-supplied deep nesting drives `indent` past the printable width; `pdf.rs:430` omits the
`.max()` clamp its sibling image path applies at `pdf.rs:246`. Held to Low by the draw-time
`scale > 0.0` gate and a single-pass `paginate` (no hang, no OOB, no allocation blow-up).
**Escalated to High** on consolidation — see F-PDF-001, where three other reviewers independently
established a spec violation and executable evidence.

### [VERIFY-01] Per-cell Pango layout allocation in table export — resource amplification
### [VERIFY-02] Attacker-influenced path interpolated into placeholder Markdown notices
Both MEDIUM confidence, reported as Needs Verification per the security skill's methodology.
Neither was escalated; both want a decision from `linux` rather than a fix on spec.

---

## 🔴 Critical — Must Fix

**None.** No finding in this round meets the Critical bar (security vulnerability, data-loss
risk, or broken core functionality reachable in normal use).

---

## 📊 LIVE STATUS — 2026-08-22

| Tier | Total | Closed | Remaining |
|------|-------|--------|-----------|
| **High** | 19 | **18** | 1 — F-GATE-009, PARKED (superseded by the `cargo xtask` unification) |
| **Medium** | 69 | **68** | 1 — M64, PARKED with F-GATE-009 (both retire together under the `cargo xtask` unification); **every actionable Medium is done** |
| **Low** | 45 | **42** | 3 — L29, L36, L37, each AWAITING AN OPERATOR RULING (all three are structural, not cleanup) |
| **Tidy** | 4 | **4** | 0 — done anyway; three were misleading documentation, which costs the next reader real time |

**Verification standard applied throughout**: every fix carries an automated test; anything
that changes behaviour carries a `tests/MANUAL-TEST.md` check; and the ones that could go
wrong quietly were mutation-tested — the guard is broken deliberately and the test must fail.
Where a fix was declined as costing more than the defect, the reasoning is recorded at the
finding rather than the finding silently dropped.

**Four findings have now been CORRECTED rather than implemented as written**, each after
measurement contradicted the review: F-GATE-010's premise, F-DRY-002's severity rationale,
**M36** (its "two menu items show no key" defect is refuted — GTK supplies the hint; the fix
went the other way and DELETED the mechanism the finding wanted tidied) and **M34** (the
preferred water-fill lift refused on a worked counterexample: the two implementations are
different arithmetic, not one rule twice). **One objection of mine was refuted the same way**
— I claimed M28's fix would reintroduce ScrAP-313's leak, and a controlled probe showed the
leak is identical either way.

**Fifteen of the 69 Mediums were ALREADY CLOSED by the High work** and were verified against
the tree before any were scheduled. Planning against the raw count would have been planning
against a number nobody had checked.

### Round-2 mitigation pass — 2026-08-21/22

Thirteen Mediums closed in this pass — **the whole actionable Medium tail**: M02, M11, M29,
M31, M34, M36, M38, M39, M40, M42, M46, M58, M68.

Three things are worth carrying forward more than the fixes themselves:

1. **A stale delegated worktree nearly landed a silent revert.** A sub-agent's branch was five
   commits behind; copying its file back reverted 79 lines of newer work and looked clean.
   Caught by diffing the tree against the agent's own reported diffstat. Every later
   delegation was told to check its base first, and every merge was verified by diff rather
   than by report — POLICY § Cross-machine seat branches, at a sub-agent rather than a seat.
2. **A harness `pgrep -x` killed the operator's own running instance.** Restored by relaunch
   (its session brought back 10 windows). The drive loop now runs through a helper that
   resolves and kills only PIDs whose `/proc/<pid>/environ` names the private display, and
   whose refusal was demonstrated against the operator's PID before reuse. GTK4Rs/AP-132 and
   `MANUAL-TEST` §1.6 both say exactly this; knowing the rule was not enough.
3. **Two findings were wrong in the direction that flatters the reviewer**, and both were only
   caught by driving the real app with a pre-change control binary beside the fixed one.
   A review reads code; it cannot see a toolkit supplying what the code omits.

---

## 🟠 High

Nineteen distinct defects. Where several reviewers converged, the merged entry names all of
them — convergence from independent lenses is the strongest signal in this round and is
recorded rather than flattened away.

**Status — 18 of 19 resolved and FULLY VERIFIED on all three platforms; the 19th (F-GATE-009) is PARKED, superseded by the `cargo xtask` unification logged as debt** (marked ✅ **Done** below), landed on `mitigations` in four
mitigation rounds plus one merged seat branch. Each fix carries an automated test and, where
it changes behaviour, a `tests/MANUAL-TEST.md` check; the four orchestrator-verified findings
were additionally mutation-tested, and F-CLIP-001 was reproduced live before being fixed.

The one that is not resolved is **F-GATE-009**, and it is PARKED rather than open: the
operator ruled to replace both `lint-references` ports with a single `cargo xtask` binary,
which retires that finding and M64 with it. Logged as debt; not started, because the Medium
and Low tails are the priority.

Two findings were **corrected rather than implemented as written**, and the corrections are
recorded in the code: F-GATE-010's premise (the macOS runner already derived from the
contract; the real defect was thirteen duplicated functions) and F-DRY-002's severity
rationale (a transposed pair does not reach the grid — `fit` clamps it back).

---

## ✅ CLOSED — the Windows-offline TODO block

> This section used to carry W1–W4, the work parked while the `windows` seat was
> unreachable. **All of it landed.** Kept as a heading rather than deleted so anyone who
> remembers being pointed here finds the outcome instead of a gap.
>
> - **W1** — the `.ps1` edits made blind from the Linux seat were verified on Windows, and
>   the seat found FOUR defects in them, three of them mine. The worst: my `ExitCode` fix
>   fell through and printed `PASS` on the line below its own `FAIL`, on the one check whose
>   subject is that an empty input is indistinguishable from a clean tree.
> - **W2** — F-GATE-003, -004, -005 and -008 all closed. F-GATE-005's root cause turned out
>   to be structural rather than weak cases: PowerShell defines functions as it executes and
>   the `-SelfTest` dispatch sat above the definitions, so the self-test *could not* reach
>   the executor.
> - **W3** — the `disarm.` contract grammar landed readers-first across three ports, and no
>   port went red at any point. F-GATE-006 closed with the announcement confirmed in a real
>   macOS run.
> - **W4** — F-GATE-009 is superseded rather than pending: the operator ruled to unify both
>   ports on a `cargo xtask` binary, and that is logged as debt.
>
> **A note on this file that outlived the section**: `docs/` is gitignored, so this document
> reaches no other seat. Findings are dispatched by sending CONTENT over ToasterTalk, citing
> repo paths and symbol names. Line numbers throughout were taken at `fd83582` and many have
> since moved — locate by symbol.

---

### F-GATE-001 — `check 12` is a permanent false green on macOS ⚠️ **ORCHESTRATOR-VERIFIED**  ✅ **Done**
**Found by 4 reviewers independently:** `antipattern-C H1`, `dry-C H1`, `spec-C H1`, `testability-C T1`.
**Where:** `scripts/lint-references.sh:1444` (new in this diff), constraint stated at `:140`,
dispatch at `scripts/pipeline.steps:335`.

Verified directly, on all four legs:
1. `:1444` uses `grep -qP` with PCRE-only constructs — `(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])` and `\x01-\x1f`.
2. **The same file states the constraint at `:140`:** *"`grep -P` is absent from the BSD grep macOS ships, and this file already refuses `\s` for that reason."*
3. `git diff` confirms the line is `+` new in this merge.
4. `pipeline.steps:335` — `cmd.macos references scripts/lint-references.sh` — macOS runs the **bash** port.

BSD grep exits 2 on `-P`. The call sits inside an `if … || grep …`, so the failure reads as
"no match", `set -e` is suppressed, and the check prints PASS over every path on macOS.
A rule was written down in the same file and broken 1300 lines below it.

**Aggravating:** the check shipped with **no `--self-test` / `-SelfTest` corpus in either port**
(`spec-C M4`, `antipattern-C M1`) — which is the mechanism that would have caught this.

**Fix:** replace the PCRE with the file's own established two-stage POSIX-ERE idiom; add the
missing self-test corpus including a case that fails on a BSD-grep host.

### F-CLIP-001 — PRIMARY is claimed-and-emptied, not released, on every caret move ⚠️ **ORCHESTRATOR-VERIFIED against GTK source**  ✅ **Done**
**Found by 2 reviewers:** `antipattern-A A1`, `spec-A A-1` (TDD 1.11).
**Where:** `src/clipboard.rs:272`; ineffective guard at `:476`; author intent stated at `:243`.

`clipboard.rs:272` runs `let _ = primary.set_content(None::<&ContentProvider>)` on the
no-selection arm of every `mark-set`. I read GDK at `4.6.9-5-g492b44f20c` rather than trust the
inference — `gdk_clipboard_set_content()`:

```c
else /* provider == NULL */
  {
    if (priv->content == NULL && priv->local)
      return TRUE;                    /* early-out ONLY if we already own it locally */
    formats = gdk_content_formats_new (NULL, 0);
  }
result = gdk_clipboard_claim (clipboard, formats, TRUE, provider);
```

When a **foreign** application owns PRIMARY, `priv->local` is FALSE, the early-out does not fire,
and the call falls through to `gdk_clipboard_claim(..., local=TRUE, provider=NULL)` — taking the
selection and emptying it. Moving the caret in Scribobulate destroys the X11 primary selection
you copied from another application.

The bug is doubly sharp because the code's **own doc comment at `:243`** and its test both state
that claiming PRIMARY for `""` "breaks every other application's middle-click paste" — the intent
was right and the API does the opposite of what the author expected.

**Fix:** test ownership before releasing — only clear when we are the local owner
(`is_local()` / track our own claim); otherwise leave PRIMARY alone.

### F-PDF-001 — `pdftable::fit` returns `scale <= 0`, and the guard that exists to honour TDD 25.17 is what breaks it ⚠️ **ORCHESTRATOR-VERIFIED**  ✅ **Done**
**Found by 4 reviewers:** `testability-B B-1` (High, *measured*), `antipattern-B 3` (High),
`spec-B B-1` (Medium), `security SEC-01` (Low). Severity resolves to **High** — higher wins.
**Where:** `src/export/pdf.rs:430` (missing clamp) vs `src/export/pdf.rs:246` (sibling clamp);
`src/export/pdftable.rs:129` (`fit`).

Verified by direct read:
- `pdf.rs:246` — `let available = (self.width_pt - indent).max(1.0);`
- `pdf.rs:430` — `pdftable::fit(&natural, &minimum, self.width_pt - indent, &chrome)` — **no clamp**
- `fit()` guards only `count == 0`; `text_available = available - total_chrome` may go negative.

`testability-B` extracted `fit` verbatim into a standalone crate and ran it:
`available=0.0 → scale=0.0`, `available=-5.0 → scale=-0.085`. Reachable — indent grows 18pt per
blockquote/list level and `MAX_NEST_DEPTH` is enforced only in `copymap`, so 26 nested `>` on a
468pt page hits it exactly. Downstream, `draw_table_row`'s `scale > 0.0` guard *skips the
transform*, so the row draws unscaled and clipped at the margin — **failing TDD 25.17 through the
code written to honour it**.

**Why the tests missed it:** `pdftable`'s property sweep asserts exactly this `(0,1]` invariant
but starts its input list at `0.5`, and **no test anywhere passes a non-zero indent to
`Layouter::table`**.

**Fix:** clamp at the `table()` call site to match `image()`; extend the property sweep to
non-positive widths; add a nested-table export case.

### F-GATE-002 — Both ports of `check 12` report PASS when `git ls-files` fails  ✅ **Done**
**Found by:** `antipattern-C H2`.
Bash process substitution discards the enumerator's exit status; the PowerShell port never reads
`$proc12.ExitCode`. A failed enumeration is indistinguishable from a clean tree — the exact
defect the script's own preflight block at `:700-714` exists to prevent.

### F-GATE-003 — `pipeline.ps1` neither applies nor validates carve-outs, and its comment misdescribes the shell port  ✅ **Done** (`1aca1bc`)
**Found by 2 reviewers:** `antipattern-C H3`, `dry-C H3`.
**Where:** `packaging/windows/pipeline.ps1:655` (`ANNOUNCE-ONLY`), `:671`; contract at
`scripts/pipeline.steps:103-112`; `scripts/pipeline.sh:391` applies them.
The parity comment at `:655` is now factually false. Windows also skips the cargo-test validation
the contract requires of "the runners".

### F-GATE-004 — The packaging provenance warning is missing on Windows  ✅ **Done** (`1aca1bc`)
**Found by 2 reviewers:** `dry-C H4`, `antipattern-C M5`.
Required by `scripts/pipeline.steps:128-138`; implemented in both shell runners, absent from
`packaging/windows/pipeline.ps1`.

### F-GATE-005 — `-SelfTest` proves the helpers and never proves the executor uses them  ✅ **Done** (`0a66025`)
**Found by:** `antipattern-C H4` (with `testability-C T6`).
The new `-SelfTest` cases call `Split-EnvPrefix` / `Invoke-WithEnv` directly and never
`Invoke-ContractCommand`. **Revert the executor and the whole self-test stays green** — a
self-test that cannot detect the deletion of its own subject.

### F-GATE-006 — macOS runs step 5 with the criticals gate disarmed, and the output cannot say so  ✅ **Done** (`d57c402`, announcement confirmed in a real macOS run)
**Found by:** `antipattern-C H5`. Related: `spec-C M3` — Linux's newly armed
`G_DEBUG=fatal-criticals` was never mutation-tested, though POLICY declares mutation testing the
only admissible evidence that a gate can fail.
macOS's absence is declared only in a source comment, and its run output is byte-identical to
Linux's — so nothing downstream can distinguish armed from unarmed.

### F-GATE-007 — The coverage floor moved on a scope narrowing and the record does not say so  ✅ **Done**
**Found by 3 reviewers:** `testability-C T3` (High, *measured*), `spec-C M5`, `antipattern-C M2`.
**Where:** `scripts/coverage.sh:181`, `:193`, `:245`.
`testability-C` ran `cargo llvm-cov` with `clipboard` removed from `IGNORE`: the exclusion is
worth **+0.40 pt** (79.52% → 79.92%), against a floor that moved +0.40 pt in `b890c9c` with no
ratchet note. The log records "79.40 → 79.60" over a floor that was 79.00. `testability-A T-5`
adds that the exclusion is argued from an undercount — the stated "one branch" is four.

### F-GATE-008 — Check 10's predicate diverges between the twins  ✅ **Done** (`d3a9eec`) — ruled: the PowerShell predicate was correct
**Found by:** `dry-C H2`. Shell glob `'**'*Scribobulate*` (`lint-references.sh:1330`) vs regex
`^\*\*[^*]*Scribobulate` (`.ps1:1377`). The reviewer **emulated both algorithms** and got
opposite verdicts on a four-line input; the triggering line shape is already live at
`sdd/ANTI-PATTERNS.md:1370`, `:1761`, `:1225`.

### F-GATE-009 — The shell/PowerShell twin: the shared rule is not shared  ⏸ **PARKED — awaiting operator ruling (see TODO W4)**
**Found by:** `dry-C H6`. `.scan` shares only the *file set*; ~120 lines of patterns, six
self-test corpora, file lists, thresholds and exclusions are hand-synced literals in each port —
and `.ps1:569-572` has been reduced to instructing a human to keep them identical.
**Fix (proposed in full by the reviewer):** a `scripts/lint-references.rules` data file reusing
`pipeline.steps`' already-validated grammar, plus a ~10-line reader per port. The reviewer also
names the two things that must **not** move.

### F-GATE-010 — `packaging/macos/pipeline.sh` is a copy of `scripts/pipeline.sh`, already drifted twice  ✅ **Done**
**Found by:** `dry-C H5`. ~320 identical bash lines. Drifts: no carve-out precondition check;
first-word-only script existence check vs every-token.

### F-PLAT-001 — `macwordnav` is `cfg`-deleted at its module declaration, removing all its tests from CI  ✅ **Done** — and the macOS run half CLOSED: unlocked suite 278 passed / 0 failed, all 7 `macwordnav` GTK bodies green
**Found by 3 reviewers:** `testability-C T2` (High, *measured*), `testability-B B-2`, `antipattern-C M4`.
**Where:** `src/lib.rs:71-72`; `src/macwordnav.rs:102` (`word_movement`).
`cargo test --lib -- --list` → **1020 cases, zero under `macwordnav`**. The decision core is
platform-free and display-free, yet its tests never compile on the platform POLICY names as
canonical. `pipeline.steps:215-217` forbids exactly this as **AP-212 — in a file this change
edited** — and the same merge applies the correct `cfg!` idiom in `export/mod.rs`, citing that
very rule.
**Fix:** compile the pure logic unconditionally; gate only the platform wiring.

### F-VIEW-001 — The outline reads the document through a different prologue than the page  ✅ **Done**
**Found by:** `dry-B H-1`; supported by `dry-B H-2`.
The merge claims outline, annotation balancer and page now share "the same reading of the
document". They do not — there are **four** hand-assembled readers, and the outline's omits the
CriticMarkup strip the page and the export both perform, so CriticMarkup leaks into heading
labels. `H-2` adds that the pairing is an unenforced per-call-site convention repeated four times.

### F-DRY-001 — The tab context menu's access keys are a hand-maintained mirror, in three copies  ✅ **Done**
**Found by 2 reviewers:** `dry-B H-3`, `antipattern-B 10`.
The menubar guard genuinely derives from `build_top_level_menus()` — **the commit got that
right**, confirmed independently by `spec-B`. The tab context menu did not follow.

### F-DOC-001 — `load_into_editor`'s rustdoc states the opposite of what this merge shipped  ✅ **Done**
**Found by 3 reviewers:** `antipattern-A A3` (High), `dry-A D-9`, `testability-A T-7`.
**Where:** `src/window/actions.rs:618`. The doc says there is "deliberately no clipboard-side
half"; commit `f1efa3b` added one three commits later in the same merge. A doc that contradicts
its own module is worse than no doc — the next agent will trust it.

### F-VIEW-002 — The promote gate counts callbacks, not drawn pages  ✅ **Done**
**Found by:** `antipattern-B 2`.

### F-DRY-002 — Two implementations of one rule, with their arguments in opposite order  ✅ **Done**
**Found by:** `antipattern-B 1`; related to `dry-B M-2` (water-fill rule implemented twice,
guarded by a single example) and `dry-B M-1` (two Pango layout constructors in the PDF sink that
already disagree on weight and size origin).

### F-SDD-001 — A code comment cites an `ISSUES.md` entry from outside the register, and check 1 cannot see it  ✅ **Done**
**Found by 2 reviewers:** `spec-C H2`, `links 1.1`.
**Where:** `probes/native-chooser-rss.m:3`. SDD principle 6 / `POLICY.md:204` forbid this
absolutely. Check 1's regex matches only letter IDs, so the gate reports PASS over it (verified
against `$ISSUES_RX`). Companion: `links 1.2` — `sdd/TECH.md:161` cites `ScrAP-277`, which exists
in neither `ANTI-PATTERNS.md` nor the manifest.

---

## 🟡 Medium

**69 findings.** Fix recommended; verification is spot-check, not full audit. Grouped by originating reviewer; convergences already merged into the High section are not repeated here.

**AUDIT, 2026-08-21 — 15 of the 69 were ALREADY CLOSED by the High work and the `pipeline-lib.sh`
audit, verified against the tree rather than assumed** (marked ✅ below): M13, M15, M16, M17, M18,
M19, M32, M43, M44, M48, M50, M53, M54, M61, M66. Several are the same defect a High finding
described from a different reviewer's angle, which is why the raw count overstates the backlog.

**STATUS, live — 55 of 69 Medium closed.** Fourteen remain: M02, M11, M29, M31, M34, M36,
M38, M39, M40, M42, M46, M58, M64, M68. Of those, **M64 is PARKED** — the hand-copied corpus
is the same subject as F-GATE-009, and both are superseded by the decision to unify the two
`lint-references` ports on a `cargo xtask` binary, which is logged as debt rather than
started. So **thirteen are actionable**.

**[M01]** A4 — Swap recovery is a fourth ingress door: it repairs the buffer and not the source  ✅ **Done** (`8e86a67`)
  - **Where:** `src/window/swaprecovery.rs:300`, `src/lineendings.rs:185-187`, `src/renderer/normalize.rs:1-7`, `src/swapfile/codec.rs:264`
  - **Reviewer:** `antipattern-A`
  - **Fix:** , two parts:

**[M02]** A5 — Invariant 1 ("every insertion arrives as ONE emission") is not held process-wide  ✅ **Done — the gap is REFUTED; the statement was corrected**
  - Same defect as M46 from a second reviewer's angle, and both rest on `wire_primary_selection` being wired only on the editor. **That function does not exist.** It and `src/primarysel.rs` were deleted in `4b97c84` ("delete the take-over that never could"); the only trace left was a dangling doc reference, now removed.
  - **The substantive claim is disproven by measurement, not by argument.** The preview really does publish rich `GtkTextBufferContent` to PRIMARY (nothing removes GTK's own `add_selection_clipboard`) — but that never reaches Invariant 1, because the editor's middle-click paste is a CONSUMER-side route reading `read_text_async`, never `GTK_TYPE_TEXT_BUFFER`. GDK satisfies a text read from any publisher's content, tagged or not, so `insert_range_not_inside_self`'s per-tag-toggle chunking is never entered whichever pane the selection came from.
  - **Fix:** `lineendings.rs`'s Invariant 1 now NAMES the preview pane as the case that looks like a hole and is not, with both measurements cited, so the next reader does not re-derive this worry. Two new tests pin it: `a_preview_selection_still_publishes_gtks_default_rich_content_to_primary` and `a_preview_selection_pastes_into_the_editor_as_one_plain_text_emission` (a CRLF-bearing preview selection middle-clicked into the editor, asserting exactly one `insert-text` emission with every `\r\n` byte-exact).
  - **Where:** `src/lineendings.rs:237-245`, `src/clipboard.rs:245`, `src/window/tabs/lifecycle.rs:171`, `src/codeview/mod.rs:329`
  - **Reviewer:** `antipattern-A`
  - **Fix:** , cheapest first:

**[M03]** A6 — Invariant 2's enforcement is "it's greppable", with no gate  ✅ **Done** (`HEAD`)
  - **Where:** `src/lineendings.rs:246-250`, `src/lineendings.rs`, `src/clipboard.rs:361`
  - **Reviewer:** `antipattern-A`
  - **Fix:** — add to `clippy.toml`:

**[M04]** A7 — `value()` reports "no buffer / no selection" as a successful empty paste  ✅ **Done** (`1e5c82e`)
  - **Where:** `src/clipboard.rs:109-125`, `src/clipboard.rs:71-77`
  - **Reviewer:** `antipattern-A`
  - **Fix:** fail the read instead of serving an empty payload, and let GDK report it.

**[M05]** A8 — `RefCell` borrow held across a synchronously re-entrant GTK setter  ✅ **Done** (`1e5c82e`)
  - **Where:** `src/clipboard.rs:145-151`, `src/clipboard.rs:255`, `src/clipboard.rs:168`, `src/clipboard.rs:113-122`
  - **Reviewer:** `antipattern-A`
  - **Fix:** — take the value out and drop the borrow before touching GTK:

**[M06]** A9 — Primitive obsession: "text with normalised line endings" is not a type  ✅ **Done** (`b82dd8e`)
  - **Where:** `src/lineendings.rs:128`, `src/lineendings.rs:151`, `src/docio/mod.rs:189`, `src/docio/mod.rs:98`
  - **Reviewer:** `antipattern-A`
  - **Fix:** — a minimal newtype produced only inside `lineendings`:

**[M07]** 4. Medium — A bounds guard defeated three lines later  ✅ **Done** (`3a09e0a`)
  - **Where:** `src/export/pdf.rs:607-610`
  - **Reviewer:** `antipattern-B`
  - **Fix:** make the pairing structural instead of parallel-by-convention:

**[M08]** 5. Medium — A doc comment that GUARANTEES what its own branch violates  ✅ **Done** (`HEAD`)
  - **Where:** `src/widgets/table/layout.rs:42-43`
  - **Reviewer:** `antipattern-B`
  - **Fix:** restate the contract as what the code does, and name the exception where a caller will meet it:

**[M09]** 6. Medium — Hardcoded rule geometry in a file that forbids literals  ✅ **Done** (`3a09e0a`)
  - **Where:** `src/export/pdf.rs:642`
  - **Reviewer:** `antipattern-B`
  - **Fix:** ```rust LineKind::Rule => { let rule = theme.rule.unwrap_or(palette.rule); // Span the printable column the rule sits in, exactly as the paragraph // beside it does — never a fixed length, which over- or under-runs the // margin depending on the page setup and the nesting depth. let width = (content_width_pt - line.indent).max(0.0); let thickness = f64::from(theme.metrics.rule_thickness); … cr.rec

**[M10]** 7. Medium — A user-supplied string reaches a GMenu label unescaped  ✅ **Done** (`HEAD`)
  - **Where:** `src/app/menubar.rs:146-155`, `src/theme.rs:817-828`
  - **Reviewer:** `antipattern-B`
  - **Fix:** ```rust // A theme name comes from a user-editable themes.toml, so it is dynamic content in a // mnemonic context — double its underscores (documents_item_label's rule) and do NOT // route it through mnem(), whose table is keyed on static command labels and would // inject a marker into any theme that happens to share one of their names. let label = crate::theme::Themes::chooser_label(&name, symbo

**[M11]** 8. Medium — A live-bound `GMenu` mutated straight from signal handlers  ✅ **Done**
  - **The finding's dilemma was real and is now resolved the safe way.** `View ▸ Documents` deferred its rebuild to a coalesced idle with an emphatic "never call it directly from a signal handler"; `Format ▸`'s insert section — an equally live-bound child of the SAME `GtkPopoverMenuBar` model — was relabelled synchronously from three signal handlers, the sharpest being `win.format`'s `notify::enabled`, which fires exactly when focus moves into a menu popover.
  - **Fix:** one choke point, `menubar::defer_live_menu_mutation`, taking the menu's own coalescing flag. BOTH submenus now go through it — the point is not that the Format menu gained an idle, but that neither site decides for itself any more.
  - **Coalescing done properly:** requested kind (`format_menu_pending`) and displayed kind (`format_menu_kind`) are now separate fields. Folding them, which is what the synchronous version did, makes an A→B→A toggle inside one turn record B as applied while the menu still shows A.
  - **Guards:** `the_format_relabel_lands_on_idle_and_not_in_the_callers_turn` asserts the TIMING — unchanged immediately after the call, changed after the loop runs — because "it eventually shows the right label" is equally true of the unsafe version and cannot discriminate. **Mutation-tested:** restoring the synchronous body makes it fail on exactly that assertion. Plus `a_there_and_back_relabel_in_one_turn_settles_on_what_is_shown`.
  - **Live, with a positive control:** two binaries driven identically — caret selection over an exact link, Format menu opened — both render `Edit Link…`, `compare -metric AE` = **0**. A safety fix with no user-visible change, which is what it should be.
  - ⚠ **A harness mistake during this verification killed the operator's own running instance** — a `pgrep -x scribobulate | head -1` matched their live copy on `:0` (GTK4Rs/AP-132 / MANUAL-TEST §1.6, both of which say exactly this). Restored by relaunch; its session restored 10 windows. The drive loop now goes through `/tmp/scrib-harness/lib.sh`, which resolves and kills only PIDs whose `/proc/<pid>/environ` names the private display, and whose refusal was demonstrated against the operator's PID before reuse.
  - **Where:** `src/app/menubar.rs:64-85`, `src/window/mod.rs:677`, `src/window/tabs/lifecycle.rs:104`, `src/window/editbar/relabel.rs:23`
  - **Reviewer:** `antipattern-B`
  - **Fix:** give `update_format_menu_labels` the same coalescing shape:

**[M12]** 9. Medium — The a11y walk's structural downcast, and the coverage claim it does not support  ✅ **Done** (`HEAD`)
  - **Where:** `src/a11y.rs:144-149`
  - **Reviewer:** `antipattern-B`
  - **Fix:** 1. Ask the semantic question. GTK already answers it: a widget owes a name iff it is focusable / interactive and has no accessible label. Replace the downcast cascade with:

**[M13]** 10. Medium — The tab-context-menu guard is a hand-maintained mirror  ✅ **Done** — closed by F-DRY-001
  - **Where:** `src/app/mnemonics.rs:393-430`, `src/window/tabs/contextmenu.rs:84-85`
  - **Reviewer:** `antipattern-B`
  - **Fix:** 1. **Fuse the pair** so the mismatch is unrepresentable (this project's `AP-156`/`AP-108`/`AP-130` reflex — "fuse mutate+retarget into ONE method so the wrong order is unrepresentable"):

**[M14]** 11. Medium — Every cairo status discarded, and none checked  ✅ **Done** (`3a09e0a`)
  - **Where:** `src/export/pdf.rs`
  - **Reviewer:** `antipattern-B`
  - **Fix:** check the status once per page, at the one place that owns the outcome:

**[M15]** M1. `check 12` is the one check in the file with no `--self-test` corpus, and the two ports do not share its pattern  ✅ **Done** — closed by F-GATE-001
  - **Where:** `scripts/lint-references.sh:1443-1444`, `scripts/lint-references.ps1:1465-1469`, `scripts/lint-references.sh:684`, `src/lib.rs`
  - **Reviewer:** `antipattern-C`
  - **Fix:** Hoist the three classes into shared named patterns beside `BARE_AP_RX` / `$bareApPattern`, add a check-12 corpus to the shared self-test (must-flag: `a|b.txt`, `CON`, `nul.md`, `dir/COM1.txt`, `x.`, `x `, a path with `\x01`; must-not-flag: `src/lib.rs`, `CONtinue.md`, `a-CON/b.txt`), and drive it through the same predicate the check calls — the idiom `is_bare_ap` / `Test-BareAp` already establishe

**[M16]** M2. The coverage ratchet's narrative has a hole exactly where it claims to be complete  ✅ **Done** — closed by F-GATE-007
  - **Where:** `scripts/coverage.sh:181`, `scripts/coverage.sh:193`, `scripts/coverage.sh:202`
  - **Reviewer:** `antipattern-C`
  - **Fix:** Insert the missing entry for `b890c9c` naming what it banked and the measured total behind it, and reconcile the two same-date headings (the second is presumably a later day). If the `79.40` step cannot be reconstructed, say so in one line rather than leaving the chain to read as continuous.

**[M17]** M3. `clipboard.rs` enters `IGNORE` in the same change that raises the floor, and nothing else measures it  ✅ **Done** — closed by F-GATE-007
  - **Where:** `scripts/coverage.sh:280`, `scripts/coverage.sh:245-256`, `src/clipboard.rs`
  - **Reviewer:** `antipattern-C`
  - **Fix:** Either extract the one predicate the note itself identifies (`selection_bounds().is_some()` plus whatever decides the realize/unrealize rebalance) into a gated pure function, or — cheaper and adequate — add the four behaviour names to `lint-references.sh` as a check that `src/clipboard.rs` still contains four `#[gtktest::test]` bodies. An exclusion whose justification is "it is covered elsewhere" 

**[M18]** M4. `macwordnav` is `cfg`-deleted at its module declaration, taking nine display-free unit tests off two platforms  ✅ **Done** — closed by F-PLAT-001
  - **Where:** `src/lib.rs:71-72`, `src/gtk_suite.rs:70-71`, `src/macwordnav.rs:102`, `src/macwordnav.rs:243-335`
  - **Reviewer:** `antipattern-C`
  - **Fix:** Drop the cfg at `src/lib.rs:71` and `src/gtk_suite.rs:70`, leave the five call sites gated as they already are, and carry `#[cfg_attr(not(target_os = "macos"), allow(dead_code))]` on `wire_word_navigation` / `wire_field_word_navigation` with a one-line reason. Cleaner still: move `word_movement` into `keynav`, which is already cross-platform and already holds `document_movement` that this function

**[M19]** M5. The Windows runner omits the packaging provenance warning the contract requires  ✅ **Done** — closed by F-GATE-004
  - **Where:** `packaging/windows/pipeline.ps1:836-840`, `packaging/windows/pipeline.ps1:725-729`, `scripts/pipeline.steps:128-142`, `scripts/pipeline.sh:381-388`
  - **Reviewer:** `antipattern-C`
  - **Fix:** Add `$script:Overridden = @()`, append at `:838`, and emit the warning in `Invoke-ContractStep`'s packaging path before the command runs, matching `pipeline.sh`'s wording so the two logs stay diffable.

**[M20]** M6. `native-chooser-rss.m`'s GObject instance counter reads 0 when it is unavailable, and the comment claims otherwise  ✅ **Done** (`67133a7`)
  - **Where:** `probes/native-chooser-rss.m:219-221`, `probes/native-chooser-rss.m:365-370`, `probes/README.md`
  - **Reviewer:** `antipattern-C`
  - **Fix:** Canary the instrument: count a type known to be live (the probe's own `GtkApplicationWindow`, or `GtkWindow`) once at startup and print `instance counting: UNAVAILABLE (set GOBJECT_DEBUG=instance-count)` and suppress the whole `--instances` section when it reads 0. Then correct `:220-221`.

**[M21]** M7. `--cancel` silently degrades to `hide()` mid-run while the run header still claims the user's path  ✅ **Done** (`67133a7`)
  - **Where:** `probes/native-chooser-rss.m:514-523`, `probes/native-chooser-rss.m:678-679`
  - **Reviewer:** `antipattern-C`
  - **Fix:** Count the fall-throughs and print them in the final summary (`cancel=%d hide_fallback=%d`), and make a nonzero fallback count invalidate the run — `exit(2)` the way `watchdog()` already does at `:404`, since a mixed-dismissal figure is not the measurement anyone asked for.

**[M22]** M8. The first checkpoint bucket divides nine cycles' growth by ten and mislabels its range  ✅ **Done** (`67133a7`)
  - **Where:** `probes/native-chooser-rss.m:556-558`, `probes/native-chooser-rss.m:581-583`
  - **Reviewer:** `antipattern-C`
  - **Fix:** Track `int prev_cycle` alongside `prev`, pass `done_cycles - prev_cycle` as the divisor, and label the span from `prev_cycle`.

**[M23]** M9. `SelfDeleteGuard::swallows` — the one cell where a real deletion is suppressed is neither tested nor logged  ✅ **Done** (`HEAD`)
  - **Where:** `src/winstate/selfdelete.rs:69-71`, `src/winstate/selfdelete.rs:125-131`, `src/app/open.rs:259-262`
  - **Reviewer:** `antipattern-C`
  - **Fix:** Two small changes, neither behavioural:

**[M24]** M10. `allowed-file-types-empty.m` has no positive control — four "NO exception" lines with nothing proving it can print anything else  ✅ **Done** (`67133a7`)
  - **Where:** `probes/allowed-file-types-empty.m:33-40`, `probes/allowed-file-types-empty.m:49-55`
  - **Reviewer:** `antipattern-C`
  - **Fix:** One line, before the four cases:

**[M25]** M11. `accessory-view-dealloc.m`'s cross-probe corroboration compares two rigs that differ in three ways  ✅ **Done** (`67133a7 (INFERRED, not aligned)`)
  - **Where:** `probes/accessory-view-dealloc.m:13-17`, `probes/accessory-view-dealloc.m:86`, `probes/appkit-panel-control.m:130`
  - **Reviewer:** `antipattern-C`
  - **Fix:** Name the exact invocation behind 518.5 in `accessory-view-dealloc.m:14` (`--dismiss orderout`, `releasedWhenClosed=YES`, n=?) and either align `setReleasedWhenClosed:` between the two rigs or state in one clause why the difference cannot move the residual. If it has not been re-measured under matched conditions, say `INFERRED` rather than presenting it as the number that "makes it a mechanism".

**[M26]** M12. `ids.rs`'s CSS-suffix assertion is a tautology over a string the test builds itself  ✅ **Done** (`HEAD`)
  - **Where:** `src/winstate/ids.rs:106-114`
  - **Reviewer:** `antipattern-C`
  - **Fix:** Read the real thing: after the window is built and the CSS scope is applied, assert `window.css_classes().iter().any(|c| c == &format!("scrib-win-{}", a.raw()))`. If the scope is applied by a function, call it. Failing that, delete `:106-114` and keep only the stability assertion at `:102-104`, which is genuine — a tautology dressed as a cross-module contract is worse than no test, because the com

**[M27]** D-1 — `renderer/normalize.rs`: the differential test compares two copies of *test* code, so the production loop is unguarded  ✅ **Done** (`HEAD`)
  - **Where:** `src/renderer/normalize.rs:136`, `src/renderer/normalize.rs:424`
  - **Reviewer:** `dry-A`

**[M28]** D-2 — The PRIMARY takeover is applied to one of the tab's two `GtkTextView`s, with no recorded reason for the other  ✅ **Done** (`45948c0`)
  - **Where:** `src/window/tabs/lifecycle.rs:170-171`, `src/codeview/mod.rs:1031-1033`
  - **Reviewer:** `dry-A`
  - **Fix:** Either wire `wire_primary_selection` on the preview view at `src/preview/render.rs:148`'s neighbourhood (`ScribSelectionProvider` is already generic over `gtk::TextBuffer`, so no change to `clipboard.rs` is needed), or record the reason it must not be, beside the existing CLIPBOARD note at `clipboard.rs:176`.

**[M29]** D-3 — The inline-tab pre-pass is a four-site convention with two different disciplines, held only by one test  ✅ **Done**
  - **Fix:** `NormalizedMd<'a>` in `src/renderer/normalize.rs` — a `Cow<str>` newtype whose only constructor runs the pre-pass, with `as_str()` for the sites that must feed CriticMarkup extraction first and `parse()` for the one that parses directly.
  - **The caller/callee split is gone, resolved toward "the caller normalises"** — already the majority shape, and callee-normalises was actively wasteful: `copymap::balance_source_span` was re-running the structural code-span pre-parse on every one of up to 32 fixpoint passes. Normalisation moved out to its single caller.
  - **Enforcement: encapsulation, and the clippy ban was REJECTED on POLICY's true-positive test.** All 12 existing raw-`Parser::new_ext` callers are legitimate (the pre-pass's own scan, which must read un-normalised text; three sites parsing seam-derived `cleaned` text; eight tests exercising the tokenizer). A ban would force 12 `#[allow]`s to catch nothing that exists — the `docio` precedent exactly. `normalize_inline_tabs` is now module-private instead, so the bypass two of the four sites once took is unreachable. The residual gap (a brand-new site calling the external crate's constructor directly) is documented rather than papered over.
  - **Mutation-tested at compile time:** reverting `outline.rs` to the old direct call yields `error[E0425]: … exists but is inaccessible`. Fails → passes → fails again.
  - **ANTI-PATTERNS ScrAP-75 gained its "now enforced by `NormalizedMd`" pointer**, per POLICY § Typed GTK seams.
  - **Live:** a document with an inline tab in prose renders it normalised, with outline, table, task list, code block, blockquote and annotation all intact.
  - **Where:** `src/preview/build.rs:184`, `src/export/doc.rs:29`, `src/outline.rs:162`, `src/copymap.rs:1029`
  - **Reviewer:** `dry-A`

**[M30]** D-4 — `docio::repaired` throws the `Cow` away and allocates unconditionally, thirty lines below a sibling that deliberately does not  ✅ **Done** (`HEAD`)
  - **Where:** `src/docio/mod.rs:190`
  - **Reviewer:** `dry-A`
  - **Fix:** — same shape as its neighbour, no signature change:

**[M31]** D-8 — Twenty-two hand-rolled main-loop pump helpers, eight signatures; this merge adds the twenty-third and weakest  ✅ **Done**
  - **Inventory verified before acting:** 24 hits across 21 files (the finding said 19 — file count stale, hit count right), 22 once `pump_strip`/`pump_find_entry` are excluded as not predicate pumps. Classified by the CLOCK each caller actually depends on: 12 idle, 6 frame/timer, 3 worker-thread, plus one fixed-span and one dual-use.
  - **Fix:** `src/testpump.rs`, declared in BOTH crate roots (the `testsymlink` precedent — a module missing from `gtk_suite.rs` silently drops its bodies from the main-thread run). Its rustdoc carries the GTK4Rs/AP-261 three-clocks lesson and GTK4Rs/AP-79's timeout-source rule, and `Clock::{Idle, Frame, Worker}` is a MANDATORY argument with no default — the clock choice is the thing the copies lost. `docio::settle` now delegates rather than duplicating.
  - **19 sites migrated; 4 deliberately left**, each with a one-line reason at the site: a fixed-span `pump_for` with no predicate, a dual-use helper called both as a predicate wait and as an unconditional drain, and the two that were never predicate pumps. Scope discipline was the point — a mechanical unification that changes what a timing-sensitive test waits on is worse than the duplication.
  - **Two live defects fixed in passing:** `window/reload.rs` cited GTK4Rs/AP-122 backwards, and `window/linknav.rs::pump_until_tabs` blocked on `iteration(true)` with **no watchdog source at all** — a standing AP-79 hang risk.
  - **Mutation-tested:** a migrated test with its predicate forced false fails in 20.08s with `pump watchdog (20s, Idle) fired waiting for: the viewport to reach the top` — a clean failure rather than a hang. Reverted, passes in 0.26s.
  - **Test counts unchanged either side** (1044 unit / 1325 lib-integration / 282 main-thread), which is the check that matters for a pure test-infrastructure change.
  - ⚠ `src/platform/win32/appearance.rs` was migrated but is not compiled on this platform — **`windows` must verify it on fetch.**
  - **Where:** `src/clipboard.rs`, `src/window/editbar/newline.rs:227`
  - **Reviewer:** `dry-A`

**[M32]** D-9 — `window::actions::load_into_editor`'s rustdoc asserts the opposite of what this merge shipped  ✅ **Done** — closed by F-DOC-001
  - **Where:** `src/window/actions.rs:618-621`, `src/lineendings.rs:256`, `src/window/tabs/lifecycle.rs:136`
  - **Reviewer:** `dry-A`
  - **Fix:** Replace the paragraph with a pointer to `crate::lineendings`'s module doc, which is the single owner of this narrative, keeping only the sentence that is still true of *this* function: it is the file-side door, and the repair sits inside `begin_irreversible_action` so a load never reaches the undo stack.

**[M33]** M-1 — Medium — Two Pango layout constructors in the PDF sink that can (and already do) disagree  ✅ **Done** (`HEAD`)
  - **Where:** `src/export/pdf.rs:304`
  - **Reviewer:** `dry-B`

**[M34]** M-2 — Medium — The water-fill rule is implemented twice, guarded by one example  ✅ **Done — the MINIMUM, and the finding's preferred fix was REJECTED with a counterexample**
  - **Why the lift was refused:** the two implementations are not the same arithmetic dressed differently. `pdftable::distribute` divides once in `f64`; `fit_columns`'s water-fill divides per step with **integer truncation**. Worked case — three tied columns, `want=4`, `pool=11`, `remaining=3` — `fit_columns` yields `[+3,+4,+4]` while an exact-then-floor-with-residual reconstruction yields `[+3,+3,+5]`: a *different column* absorbs the extra pixel, not merely a different rounding. Lifting would have forced the widget's pinned per-column contract to serve a continuous-division master, which is precisely POLICY's "two masters" clause. The counterexample is now in `pdftable.rs`'s module doc so the lift is not re-proposed from scratch.
  - **What was actually wrong is fixed:** the single-example cross-check became a **sweep** — 2–5 columns × 4 shapes × every bound in the water-fill regime — asserting column count, the bound never exceeded, no column below its floor, per-column agreement within 1.0 pt, and determinism, for BOTH implementations. Each file also gained an invariant sweep of its own across all three regimes. Agreement is now a property rather than a coincidence.
  - **Mutation-tested:** dropping the floor clamp in `distribute` turns 5 tests red including both new sweeps; restored, all 17 green.
  - **Coverage ratchet honoured:** `FLOOR` 79.70 → 79.75, with the reasoning recorded at the value.
  - **Live:** exported a PDF containing a wide-column table — column widths correct, no regression.
  - **Where:** `src/export/pdftable.rs:214`, `src/widgets/table/layout.rs:59`, `src/export/pdftable.rs:338`
  - **Reviewer:** `dry-B`

**[M35]** M-3 — Medium — The cairo colour idiom is written out ten times in `pdf.rs`, and four of those writes are dead  ✅ **Done** (`3a09e0a`)
  - **Where:** `src/export/pdf.rs:600`
  - **Reviewer:** `dry-B`

**[M36]** M-4 — Medium — The menu-item idiom is repeated ~12 times in `menubar.rs`; the extraction already exists but is scoped to one section — and two items silently lost their accel hint as a result  ✅ **Done — CORRECTED, not implemented as written**
  - **The finding's defect claim is REFUTED.** "Previous Tab and Next Tab are the only two of the eighteen whose menu items carry no key hint" is true of the MODEL and false of the SCREEN. Two release binaries driven identically (GTK 4.6.9/X11, Xvfb+kwin) both showed `Ctrl+Page Up`/`Ctrl+Page Down`; the `mac` seat read `⌘PageUp`/`⌘PageDown` off the live macOS system menu bar through the Accessibility API. GTK supplies the hint from the registered accelerator when the model declares none.
  - **Mechanism, researcher-verified at 4.6.9 / 4.12.0 / 4.22.4:** `gtk_menu_tracker_item_get_accel` returns an explicit `accel` attribute if present, else `gtk_action_muxer_get_primary_accel` = `accels[0]` as passed to `set_accels_for_action`. Backend-independent (every file in the chain is core `gtk/`); the macOS system menu bar is a different renderer calling the SAME accessor (`gtkapplication-quartz-menu.c` `didChangeAccel`), with a real muxer as its observable, and `<Meta>` → `NSEventModifierFlagCommand`.
  - **So the fix went the other way: the whole `accel`-attribute mechanism was DELETED** (`set_accel`, `set_inline_accel`, and every attribute write), not tidied. `accelerator_bindings_for` already re-spells for the host before registration, so the attribute could only restate what GTK was about to say — and where they disagree the attribute WINS silently (measured: a build setting `<Primary><Alt>F12` on Zoom In rendered exactly that while `Ctrl++` kept working). Four hand-rolled item idioms collapsed to one `item(label, action)`.
  - **Evidence the deletion strips nothing:** View menu pixel-identical before/after, `compare -metric AE` = **0**; File/Edit/Format/Heading menus captured and read item by item, every declared accelerator present and every command with no accelerator correctly blank.
  - **Guards (both mutation-tested):** `every_menu_command_with_a_shortcut_is_registered` — registration is now the sole source of every hint, so an unregistered accelerator is the one state that yields a hintless item; `no_menu_item_declares_its_own_accel_attribute` — the attribute is one method call away and reads like an improvement. Plus `every_inline_command_has_a_menu_item`.
  - **Recorded:** TDD 16.9 (new), `tests/MANUAL-TEST.md` 16.9 (new — and it says PRESS the keys, because reading alone can no longer fail), POLICY § accelerator SSOT.
  - **Open:** `windows` has not yet confirmed the premise on its port. Expected to match (same `GtkPopoverMenuBar`, no backend code in the chain) but unmeasured.
  - **Where:** `src/app/menubar.rs:307`, `src/app/menubar.rs:220`, `src/app/commands.rs:704`, `src/app/commands.rs:526`
  - **Reviewer:** `dry-B`

**[M37]** M-5 — Medium — Four definitions of `test_app` plus ~20 inline copies, in a module whose own doc comment predicts exactly this  ✅ **Done** (`HEAD`)
  - **Where:** `src/window/mod.rs:752`, `src/window/outline_nav.rs:643`, `src/window/save.rs:871`, `src/window/navhistory/testkit.rs:14`
  - **Reviewer:** `dry-B`

**[M38]** M-6 — Medium — "The document currently shown" is a three-copy match, two of which sit in one file  ✅ **Done**
  - **Fix:** `TabState::shown_source(mode)` owns the mode→source rule; `refresh_outline`, `current_heading_levels` and `refresh_annotations` all call it. The prose comment saying the annotations viewer "matches `refresh_outline`" — a comment doing a function's job — is gone with the copy it described.
  - **Guards (mutation-tested, both fail when the Edit arm is flipped to the stored source):** `shown_source_reads_the_buffer_in_editor_modes_and_the_stored_source_in_preview`, seeded with a buffer that DIFFERS from the stored source since that is the only state where the arms are distinguishable; and `the_outline_and_the_annotations_viewer_read_one_document`, asserted through both consumers rather than by calling the helper twice.
  - **Live:** typed a heading and an annotation into the editor buffer and watched both the outline and the annotations panel pick them up (refactor, so the main paths were re-driven per POLICY).
  - **Where:** `src/window/outline_nav.rs:18`, `src/window/outline_nav.rs:515`, `src/window/annotations_nav.rs:25`
  - **Reviewer:** `dry-B`

**[M39]** M-7 — Medium — The text-field construction pairing is opt-in at four call sites, in a file that names that exact hazard  ✅ **Done**
  - **Fix:** new `src/widgets/textfield.rs` — `named_entry` / `named_search_entry`, taking the accessible name as a REQUIRED argument so omission is non-compiling rather than invisible. All four sites routed through it (`comment_entry`, the find bar's two fields in `chrome`, the prompt form in `editbar/dialog`).
  - **The drifted site was real:** `editbar/dialog` wired macOS word navigation and named nothing, relying on the adjacent `GtkLabel`. `a11y`'s tree-walk guard cannot arbitrate — it walks WINDOWS and the prompt form is a transient dialog. So the fields now name themselves, which is a screen-reader-observable change: TDD 16.7 extended to cover dialog controls, and `tests/MANUAL-TEST.md` gained its **missing 16.7 check** (the rubric had none at all — a POLICY § Testing violation predating this finding).
  - **Guards (both mutation-tested):** `a_constructed_field_carries_its_accessible_name` on the constructors, and `every_prompt_field_carries_an_accessible_name` in situ on the assembled dialog — the latter because this site's failure was never "the constructor is wrong", it was that the site never called one.
  - **Enforcement decided, per POLICY § Typed GTK seams: convention, not a clippy ban.** The raw constructor's remaining callers are all tests whose subject IS the bare widget, and they outnumber the production sites — a ban would fire mostly on legitimate calls. Recorded in the module rather than left open.
  - **Live:** Insert Link dialog and find bar driven and captured; layout unchanged by the refactor.
  - **Where:** `src/widgets/comment_entry.rs:54`, `src/window/chrome.rs:280`, `src/window/editbar/dialog.rs:105`, `src/widgets/comment_entry.rs:55`
  - **Reviewer:** `dry-B`

**[M40]** M-8 — Medium — GFM column alignment exists only on the export side of a rule that claims parity  ✅ **Done — IMPLEMENTED in the preview, not recorded as a CAM deviation**
  - **Confirmed live before fixing, in one document:** the exported PDF right-aligned a `---:` column and centred a `:---:` one while the preview rendered every cell flush left. The renderer discarded `Tag::Table`'s payload outright and hardcoded `set_xalign(0.0)`.
  - **Fix:** `Align` and `align_of` moved out of `export::` into a new display-free `src/mdtable.rs` — while they were export-private the widget half could not name them, which is *why* the two diverged. `TableState` now carries the delimiter row's alignments and a column index; both cell shapes honour it, a pure-link cell via `halign` on the button (a `GtkButton` fills its cell, so `xalign` on its inner label would leave the button flush left).
  - **`Align::None` kept distinct from `Align::Left`** though both render flush left today: the difference is "the author said left" vs "the author said nothing", and `column_align` is total so a row with more cells than the delimiter declared — legal input, and untrusted per TDD 2.7 — takes `None` rather than panicking.
  - **Guards (both mutation-tested; restoring the `0.0` hardcode fails both):** `a_delimiter_rows_alignment_reaches_the_preview_cells` asserts the resolved `xalign` on the built labels, not the threaded values — a vector carried correctly and never applied is indistinguishable at every level above the widget. And `the_preview_and_the_export_align_the_same_columns` compares the export's own alignment vector against what the preview applied, which is the actual CAM row 17 contract rather than two independent "looks reasonable" assertions.
  - **Recorded:** TDD 2.2d (new), `tests/MANUAL-TEST.md` 2.2d (new — checks preview, PDF and HTML together, and notes the regression is one-sided, so checking the export alone passes).
  - **Live, with a pre-change control beside it:** the fixture table now renders left / right / centred / left across its four columns, link cell included. **The control also exposed a second defect the review did not report** — with no `halign` a `GtkLinkButton` defaults to Fill and centres its own caption, so a pure-link cell rendered CENTRED while the text cells beside it in the same column rendered flush left. One table disagreeing with itself, in every document containing a link-only cell. Fixed by the same line, and only visible because the control was driven rather than reasoned about.
  - **Where:** `src/export/mod.rs:183`, `src/export/pdf.rs:509`, `src/export/html.rs:190`, `src/widgets/table/`
  - **Reviewer:** `dry-B`

**[M41]** M1 — `exclude` prefix matching diverges (Medium)  ✅ **Done** (`d76ff2c`)
  - **Where:** `scripts/lint-references.sh:248`, `scripts/lint-references.ps1:445`
  - **Reviewer:** `dry-C`
  - **Fix:** `-like` is the wrong operator for a contract-driven prefix. Use the same ordinal, case-sensitive comparison the depth tripwire two blocks down already uses (`.ps1:516`):

**[M42]** M2 — coverage.sh states its scope twice and the two have drifted (Medium)  ✅ **Done**
  - **Fix:** justification paragraphs written for `tags`, `logging`, `outline_view` — each derived by reading the module, each naming where the pure half lives and that it is gated. The `[/\\]` separator comment now says it guards a HAND-RUN invocation, since `scripts/pipeline.steps` declares step 6 permanently non-applicable on Windows.
  - **A FOURTH unjustified term was found and also closed:** `window/editbar/`, named in the regex but never described. Its decisions live wholesale in the gated `src/format/` — the floor-raising direction POLICY's scope rule names — which is exactly the kind of thing the prose exists to record.
  - **Audited by enumeration, not by trusting the finding:** every term in `IGNORE` was listed and checked against the prose. Two of the finding's claims were stale — `clipboard` IS in `IGNORE` and IS justified, and the `pipeline.steps` line number was wrong (245, not 293).
  - **⚠ INTEGRATION HAZARD HIT AND REPAIRED, recorded because it nearly landed silently.** This was delegated to a sub-agent in a git worktree whose branch was **five commits stale**; `scripts/coverage.sh` had gained 79 lines in between. Copying its file back wholesale reverted all of them, and the copy looked clean. Caught only by diffing the tree against the agent's own reported diffstat (32 insertions, 0 deletions) and finding 113/87 instead — POLICY § Cross-machine seat branches' "verify integration by diffing the trees, not by trusting a record of what was picked", at a sub-agent rather than a seat. Repaired by a three-way merge (base = the worktree's actual base commit); the one conflict was the `IGNORE` line itself, which is what had gained `clipboard`. **Verify a delegated diffstat against the tree, and check the worktree's base commit before copying anything out.**
  - **Where:** `scripts/coverage.sh:206`, `scripts/coverage.sh:280`, `scripts/pipeline.steps:293`
  - **Reviewer:** `dry-C`
  - **Fix:** The cheap correct one is to add the three missing paragraphs — this is documentation of a *decision*, and a decision nobody wrote down cannot be re-derived. The structural one, if you want the drift to be impossible rather than merely visible, is to carry the scope as data:

**[M43]** 1.1 — `ISSUES.md` entry cited from outside `ISSUES.md` (hard rule)  ✅ **Done** — closed by F-SDD-001
  - **Where:** `probes/native-chooser-rss.m:3`, `sdd/ISSUES.md`, `sdd/ISSUES.md:733`, `sdd/POLICY.md:204-206`
  - **Reviewer:** `links`
  - **Fix:** drop the parenthetical, or replace it with the durable fact being probed (e.g. "the macOS per-invocation RSS growth of `GtkFileChooserNative`") with no register pointer at all.

**[M44]** 1.2 — Dangling `ScrAP-277` citation (no corresponding register entry)  ✅ **Done** — closed by F-SDD-001
  - **Where:** `sdd/TECH.md:161`, `sdd/ANTI-PATTERNS.md`, `sdd/scrap-numbers.manifest`, `scripts/lint-references.sh`
  - **Reviewer:** `links`
  - **Fix:** either write the `ANTI-PATTERNS.md` entry for this mechanism and append `277` to the manifest, or drop the parenthetical if the lesson isn't meant to be a registered anti-pattern.

**[M45]** A-2 — TDD 1.10's crash-recovery route is uncovered, and the module doc names a function that route never calls · Severity: **Medium**  ✅ **Done** (`8e86a67`)
  - **Where:** `src/lineendings.rs:184-186`, `src/window/swaprecovery.rs:300`, `src/window/swaprecovery.rs:316`, `sdd/TDD.md:119`
  - **Reviewer:** `spec-A`

**[M46]** A-3 — `clipboard.rs`'s invariant is stated at application scope and implemented at editor scope — TECH.md § Module map · Severity: **Medium**  ✅ **Done — option (a), and (b) was rejected on evidence rather than caution**
  - **Option (b), extending the PRIMARY takeover to the preview, is a design already measured to destruction.** `4b97c84`'s own rationale for deleting the publisher-side takeover: the preview swaps its buffer on every re-render, so removing GTK's `add_selection_clipboard` registration raises a `selection_clipboard != NULL` critical from inside GTK's `set_buffer` on the very next re-render, and the overwrite-content alternative collapses the selection it is publishing. Re-enacting it would buy nothing: **neither** consequence the finding names is actually closed by it — ScrAP-312 is unreachable anyway (consumer-side read, measured), and ScrAP-313's own text says a custom provider does not fix that leak either.
  - **Fix:** the prose now states the true scope — CLIPBOARD on both panes by two different routes; PRIMARY as an editor-only takeover of the *paste*, never the *publish*, and why that suffices. `sdd/TECH.md`'s "Covers both clipboards" corrected, and the dangling `wire_primary_selection` reference removed.
  - **ScrAP-313's Scribobulate line was stale and is corrected** (landed by `linux`, per POLICY § SDD register writes — the sub-agent proposed the content and did not edit the register): it claimed a PRIMARY provider holding a `WeakRef` that has not existed since `4b97c84`.
  - **Mutation-tested:** removing `wire_middle_click_paste` from the test helper turns the new paste test red; restored, all seven clipboard integration tests pass.
  - **Where:** `sdd/TECH.md:226`, `src/clipboard.rs:1`, `src/window/tabs/lifecycle.rs:169-170`, `src/clipboard.rs:176`
  - **Reviewer:** `spec-A`

**[M47]** A-4 — The `new_editor_buffer` choke point landed without the mechanism POLICY § Typed GTK seams requires · Severity: **Medium**  ✅ **Done** (`b82dd8e`)
  - **Where:** `sdd/POLICY.md:817`, `src/lineendings.rs:201`, `src/lineendings.rs:256`, `src/lineendings.rs:183-184`
  - **Reviewer:** `spec-A`

**[M48]** B-1 — `pdftable::fit` can return a zero or negative scale, so a deeply indented table is not contained within the printable width  ✅ **Done** — closed by F-PDF-001
  - **Where:** `src/export/pdf.rs:430`, `src/export/pdftable.rs:171`, `src/export/pdftable.rs:103`, `src/export/pdf.rs:751`
  - **Reviewer:** `spec-B`

**[M49]** B-2 — `pdftable`'s "this file uses the preview's rule … exactly one thing differs" is falsified three ways, and the cross-check test is a single data point  ✅ **Done** (`HEAD`)
  - **Where:** `src/export/pdftable.rs:37`, `src/export/pdftable.rs:41`, `src/export/pdftable.rs:42`, `src/export/pdftable.rs:221`
  - **Reviewer:** `spec-B`

**[M50]** M1. `tests/MANUAL-TEST.md`'s new 15.22a cites `ScrAP-62`, which is an unrelated entry  ✅ **Done** — closed by F-SDD-001
  - **Where:** `tests/MANUAL-TEST.md:898`, `scripts/lint-references.sh:20-27`, `sdd/ANTI-PATTERNS.md:488`, `sdd/ANTI-PATTERNS.md:818`
  - **Reviewer:** `spec-C`
  - **Fix:** `GTK4Rs/AP-62`.

**[M51]** M2. ScrAP-318's implementation line is already wrong at the merge commit  ✅ **Done** (`a8fada2`)
  - **Where:** `sdd/ANTI-PATTERNS.md:4347`, `scripts/pipeline.steps`
  - **Reviewer:** `spec-C`

**[M52]** M3. Linux's newly armed `G_DEBUG=fatal-criticals` was never mutation-tested, by POLICY's own standard  ✅ **Done** (`HEAD`)
  - **Where:** `sdd/POLICY.md:1029-1043`, `scripts/pipeline.steps:250-263`
  - **Reviewer:** `spec-C`
  - **Fix:** revert one of the three fixes on Linux, confirm the suite passes with the flag off and dies with it on (exit 133 / `SIGTRAP`), and record it in the same paragraph.

**[M53]** M4. Check 12 shipped with no `--self-test` / `-SelfTest` case in either port  ✅ **Done** — closed by F-GATE-001
  - **Where:** `scripts/lint-references.sh:415-684`, `scripts/lint-references.ps1`, `scripts/lint-references.sh:29-35`, `scripts/lint-references.sh:1414-1419`
  - **Reviewer:** `spec-C`

**[M54]** M5. `scripts/coverage.sh`'s ratchet log records a "from" value the floor never held  ✅ **Done** — closed by F-GATE-007
  - **Where:** `scripts/coverage.sh:193`, `scripts/coverage.sh:181-192`, `scripts/coverage.sh:280`
  - **Reviewer:** `spec-C`

**[M55]** M6. ISSUES entry S is 56.6 KB — 55% of the register — and is an investigation dossier, not a debt entry  ✅ **Done** (`a8fada2`)
  - **Where:** `sdd/ISSUES.md:733-1563`, `sdd/ISSUES.md`, `scripts/lint-references.sh:1357`, `sdd/ANTI-PATTERNS.md`
  - **Reviewer:** `spec-C`

**[M56]** M7. Entry S states three times that all its figures are from a locked session, while presenting unlocked figures and declaring the unlocked gate passed  ✅ **Done** (`a8fada2`)
  - **Where:** `sdd/ISSUES.md:803-805`, `sdd/ISSUES.md:1389`
  - **Reviewer:** `spec-C`

**[M57]** M8. The shared scan contract's parity proof is stale at the merge commit  ✅ **Done** (`HEAD`)
  - **Where:** `scripts/lint-references.scan:182-185`
  - **Reviewer:** `spec-C`
  - **Fix:** either re-run both ports and re-record, or drop the numbers and keep the *procedure* sentence, which is the durable half.

**[M58]** T-1 — The production assembly seam is untested, and every test builds its own stand-in  ✅ **Done**
  - **Fix:** `every_editor_this_application_builds_arrives_fully_wired` in `window/tabs/lifecycle.rs` — one body asserting the ASSEMBLY, one assertion per wire so a mutation names which line died. Before it, `build_tab_editor` was covered by exactly one test (`farscroll`'s, which asserts only scroll behaviour) while every other test in the area built its own editor by hand.
  - **Mutation-tested per wire:** swapping `new_editor_buffer()` for a bare `sourceview::Buffer::new(None)` fails at the arming assertion; dropping `wire_middle_click_paste` fails at the gesture assertion — distinct lines, so the message names the defect. Restored, green.
  - **The finding's third wire does not exist** — it names `wire_primary_selection`, deleted in `4b97c84` (see M02/M46). Replaced with the middle-click gesture, which is what actually took over that job.
  - ⚠ **The CLIPBOARD half is deliberately NOT asserted here, and the reason is recorded at the site because the obvious assertion looks like it works.** MEASURED: `view.emit_by_name::<()>("copy-clipboard", &[])` leaves the clipboard reporting `gchararray GtkTextBuffer text/plain;…` — GTK's default handler publishes its rich content despite `wire_plaintext_clipboard`'s `stop_signal_emission_by_name`. A real Ctrl+C does **not**: driven under Xvfb, the X CLIPBOARD offers `UTF8_STRING COMPOUND_TEXT TEXT STRING text/plain;charset=utf-8 text/plain` and no buffer-contents target at all. So that assertion would fail against correct production code, and "fixing" it would mean changing the application to satisfy a harness artefact. **I nearly filed this as a live defect**; the live drive is the only thing that distinguished the two.
  - **Where:** `src/window/tabs/lifecycle.rs:131`, `src/clipboard.rs:283`, `src/lineendings.rs:392`, `src/window/actions.rs:656`
  - **Reviewer:** `testability-A`

**[M59]** T-2 — The undo-soundness precondition is held by convention, not by construction or a gate  ✅ **Done** (`b82dd8e`)
  - **Where:** `src/lineendings.rs:439`, `src/lineendings.rs:182-188`, `src/lineendings.rs:182-184`
  - **Reviewer:** `testability-A`

**[M60]** T-3 — The hook's second invariant names its own enforcement, and the enforcement does not exist  ✅ **Done** (`b82dd8e`)
  - **Where:** `src/lineendings.rs:246-250`, `scripts/lint-references.sh`, `src/window/tabs/lifecycle.rs:157-163`
  - **Reviewer:** `testability-A`

**[M61]** B-2 — the whole `macwordnav` module is `#[cfg(target_os = "macos")]`, deleting 16 tests from the canonical gate  ✅ **Done** — closed by F-PLAT-001
  - **Where:** `src/lib.rs:71–72`, `src/macwordnav.rs:102`, `src/macwordnav.rs:173`, `src/macwordnav.rs:243–244`
  - **Reviewer:** `testability-B`

**[M62]** B-3 — the export↔preview column-rule agreement (CAM row 17) is asserted at exactly one point, in one of three regimes  ✅ **Done** (`HEAD`)
  - **Where:** `src/export/pdftable.rs:338`, `src/widgets/table/layout.rs:59`, `src/widgets/table/mod.rs`, `src/widgets/table/layout.rs:74`
  - **Reviewer:** `testability-B`

**[M63]** B-4 — `build_top_level_menus()` — the new derivation seam the mnemonics guards depend on — reads the host filesystem  ✅ **Done** (`HEAD`)
  - **Where:** `src/app/menubar.rs:537`, `src/app/menubar.rs:146`, `src/theme.rs:857`, `src/theme.rs:803`
  - **Reviewer:** `testability-B`

**[M64]** T4 — The lint ports share an enumeration contract but hand-copy the corpus (Medium)  ⏸ **PARKED — retires with F-GATE-009**
  - Same subject as F-GATE-009: the hand-copied corpus is one half of the two-port duplication, and the operator's ruling was to replace both `lint-references` ports with a single `cargo xtask` binary rather than keep them in step. Logged as debt (ISSUES T); not started, because the Medium and Low tails were the priority. Not a fix declined — a fix superseded.
  - **Where:** `scripts/lint-references.scan`, `scripts/pipeline.steps:11-19`, `sdd/POLICY.md`, `scripts/lint-references.corpus`
  - **Reviewer:** `testability-C`

**[M65]** T5 — Check 12's pattern is blind to non-final path components (Medium)  ✅ **Done** (`HEAD`)
  - **Where:** `scripts/lint-references.sh:1444`, `scripts/lint-references.ps1:1466-1467`
  - **Reviewer:** `testability-C`

**[M66]** T6 — The shell port has no env-prefix self-test, and neither port executes a step (Medium)  ✅ **Done** — closed by AUDIT-3 + F-GATE-005
  - **Where:** `scripts/pipeline.sh:273-299`, `scripts/pipeline.sh:393`, `scripts/pipeline.steps:65-88`, `scripts/pipeline.steps:277`
  - **Reviewer:** `testability-C`

**[M67]** T7 — `swallows`'s `write_in_flight` is only ever a literal (Medium)  ✅ **Done** (`HEAD`)
  - **Where:** `src/winstate/selfdelete.rs:69`, `src/winstate/writegate.rs:83`, `src/app/open.rs:260`, `tests/MANUAL-TEST.md:898`
  - **Reviewer:** `testability-C`

**[M68]** T8 — `crlf-run-boundary.c` computes its precondition and throws it away (Medium)  ✅ **Done**
  - **Fix:** `expect_split` added to `Case`; `finish_case` now consumes the stashed `case-bad` and turns it into a verdict, and `main` returns three distinguishable states — `rig_broken ? 2 : any_corruption ? 1 : 0` — so a blind rig is no longer indistinguishable from a clean run. `probes/README.md` gained the exit-code table and the updated expected output.
  - **The reviewer's fixture list was CORRECTED:** they proposed `expect_split` true for 1, 3 and 5. Fixture 5 is `false` — the README states it removes the *chunking-by-toggle* rather than the toggle, so its byte-identical verdict is meaningful whether or not a dangerous toggle exists. Marking it true would have made the guard fire on a fixture whose validity never depended on the precondition.
  - **Mutation-tested twice, independently.** Forcing the stashed value to 0: exit 0 → exit 2 with `RIG BROKEN` on fixtures 1 and 3 only (not 2, 4, 5 — confirming `expect_split` scopes the guard) → exit 0 restored. Passing, firing, and passing again.
  - **Where:** `probes/crlf-run-boundary.c:75`, `probes/README.md:36-42`, `probes/README.md:491-493`, `probes/binding-shape-rs/src/main.rs:158`
  - **Reviewer:** `testability-C`
  - **Fix:** Consume `bad` in `finish_case` and make it a verdict:

**[M69]** T9 — `native-chooser-rss.m` silently accepts unknown flags and echoes only some modes (Medium)  ✅ **Done** (`67133a7`)
  - **Where:** `probes/native-chooser-rss.m:650-671`, `probes/README.md:599-600`, `probes/native-chooser-rss.m:314-321`
  - **Reviewer:** `testability-C`
  - **Fix:** ```c else { fprintf(stderr, "unknown argument: %s\n", argv[i]); return 2; } ```


## 🟢 Low

**45 findings** (survived the nitpick filter). **STATUS: 42 CLOSED, 3 awaiting an operator
ruling** (L29, L36, L37 — see the LIVE STATUS table). Run as one batch, per the instruction
below, verified by a full green pipeline rather than per-item.

**Nine were ALREADY CLOSED by the Medium-tail work and one was MOOT**, verified against the
tree before any were scheduled — the same lesson the Medium pass recorded, applied earlier
this time: L01 (moot), L03, L04, L12, L22, L23, L25, L30, L32, L42.

**Four findings were CORRECTED rather than implemented as written**, each because a
measurement contradicted the review — and in two cases the review's own proposed fix was
the thing that failed:
1. **L33's proposed fix breaks the gate.** It asked for `34a`/`34b` in the number manifest;
   doing so makes check 9 FAIL, because they are sub-labels inside entry 34, not headings
   (measured, then reverted). Fixed the other way — the citations were disambiguated,
   since `ScrAP-23a` (a real entry) and `ScrAP-34a` (a paragraph) were textually identical
   and named different things, which is check 8's own argument against a bare `AP-N`.
2. **VERIFY-02's suggested `escape_debug` does not close the hole.** It handles the newline
   half and not the backtick half, and a backslash escape is not honoured inside a Markdown
   code span — so the injection survived the suggested fix. Caught by the guard written for
   it, then closed with a CommonMark dynamic delimiter.
3. **L31's premise undercounted in the safe direction.** The script's header said eight and
   POLICY said nine; the script implements **fourteen**. Corrected by removing the count
   from both places rather than by writing a third number that will also go stale.
4. **L34 and L11 both undercounted their own subject** — six undocumented probes, not two;
   three `GSourceFunc` cast sites, not one.

**L44 was not cosmetic.** The probe printed `GtkSourceView 5.20.0` while running against
**5.4.1** — a banner misreporting the version its results applied to, which is the one line
a reader checks a probe's conclusion against.

**Every guard added was mutation-tested** — broken deliberately, confirmed failing, restored:
the menu-order walk (L05), the bytes-twin repair (L40), the code-span delimiter (VERIFY-02),
the parse-site closure (L28), the `is_busy` ban (L45), and both arm-R3 bans (L43).

**Batch these into a single cleanup pass — do not round-trip them individually.** If a fix costs more than the defect costs, say so and close the finding; that is a correct outcome, not a failure. QA will batch-verify that the cleanup landed, not audit each item.

**[L01]** A11 — PRIMARY is claimed from an unrealized view / a background tab  ✅ **MOOT — the PRIMARY takeover it targets was deleted in `4b97c84`**
  - **Where:** `src/clipboard.rs:255-275`, `src/clipboard.rs:254`
  - **Reviewer:** `antipattern-A`
  - **Fix:** gate on the view, which the handler already holds weakly at `src/clipboard.rs:254`:

**[L02]** A13 — The clipboard takeover is two opt-in calls, while its sibling in the same change was deliberately fused  ✅ **Done — fused as `clipboard::wire_editor_clipboards`, both halves demoted to private**
  - **Where:** `src/clipboard.rs:196`, `src/clipboard.rs:245`, `src/window/tabs/lifecycle.rs:170-171`, `src/lineendings.rs:86-91`
  - **Reviewer:** `antipattern-A`
  - **Fix:** expose one `crate::clipboard::wire_editor_clipboards(&view)` that does both, and demote the two halves to module-private — the `GTK4Rs/AP-130` "seal the exit API" rung, and the same shape as `new_editor_buffer`/`wire_paste_normalization` next door.

**[L03]** 12. Low — `fit`'s two parallel slices: primitive obsession and asymmetric bounds handling  ✅ **Done — subsumed by `ColumnWant`**
  - **Where:** `src/export/pdftable.rs:129`
  - **Reviewer:** `antipattern-B`
  - **Fix:** subsumed entirely by finding 1's `ColumnWant` struct. If that is deferred, at minimum add the same `debug_assert_eq!(natural.len(), minimum.len())` at `pdftable.rs:130` so the two twins agree about their own precondition.

**[L04]** 13. Low — The cross-implementation agreement test covers one sample in one of three regimes  ✅ **Done — per-regime split plus a sweep**
  - **Where:** `src/export/pdftable.rs:338-356`
  - **Reviewer:** `antipattern-B`
  - **Fix:** sweep the three regimes and assert the agreement or the documented divergence per regime:

**[L05]** 15. Low — `collect_popovers` walks LIFO while claiming menu order  ✅ **Done — walks in menu order now, with a mutation-tested ordering guard**
  - **Where:** `src/app/mnemonics.rs:221`
  - **Reviewer:** `antipattern-B`
  - **Fix:** `stack.remove(0)`, or a `VecDeque` with `pop_front()`, or drop the sentence from the comment. Given the comment exists to help someone debug a collision report, fixing the walk is the better half.

**[L06]** L1. `packaging` is a legal class in both runners and absent from the contract's grammar  ✅ **Done — `packaging` added to the grammar and the CLASSES paragraph**
  - **Where:** `scripts/pipeline.steps:56`, `scripts/pipeline.steps:361`, `packaging/windows/pipeline.ps1:262`, `scripts/pipeline.sh:133`
  - **Reviewer:** `antipattern-C`
  - **Fix:** add `packaging` to `:56` and one sentence to the CLASSES paragraph.

**[L07]** L2. Stale contradicting comment about what `-close` balances  ✅ **Done — marked as the refuted hypothesis it is**
  - **Where:** `probes/native-chooser-rss.m:293`
  - **Reviewer:** `antipattern-C`
  - **Fix:** delete `:291-294`'s second sentence, or mark it as the hypothesis `:327` refuted.

**[L08]** L3. Dead code presenting as an instrument  ✅ **Done — real `--verbose` flag**
  - **Where:** `probes/appkit-panel-control.m:112-115`
  - **Reviewer:** `antipattern-C`
  - **Fix:** delete it, or put it behind a `--verbose` flag alongside the others at `:151-158`.

**[L09]** L4. `--no-spy` prints `deallocs=0`, indistinguishable from an armed spy that saw none  ✅ **Done — `deallocs=n/a` when unarmed; `windowlist_panels`**
  - **Where:** `probes/appkit-panel-control.m:118-120`
  - **Reviewer:** `antipattern-C`
  - **Fix:** print `deallocs=n/a` when `use_spy == 0`, and rename `live_panels` to `windowlist_panels`.

**[L10]** L5. Private ivars read at an assumed type  ✅ **Done — `ivar_getTypeEncoding` checked before dereferencing**
  - **Where:** `probes/appkit-panel-control.m:102-108`
  - **Reviewer:** `antipattern-C`
  - **Fix:** `const char *enc = ivar_getTypeEncoding(b); if (!enc || (enc[0] != 'B' && enc[0] != 'c')) { printf(" %s=<enc %s>", bools[i], enc ? enc : "?"); continue; }`.

**[L11]** L6. `(GSourceFunc)gtk_window_destroy` — the callback's "return value" is garbage  ✅ **Done — `destroy_parent` wrapper, at all THREE sites (the finding named one)**
  - **Where:** `probes/native-chooser-rss.m:540`
  - **Reviewer:** `antipattern-C`
  - **Fix:** a two-line `static gboolean destroy_parent(gpointer p) { gtk_window_destroy(p); return G_SOURCE_REMOVE; }`.

**[L12]** L7. A live ISSUES citation that check 1 structurally cannot see  ✅ **Done — citation removed, subject named instead**
  - **Where:** `probes/native-chooser-rss.m:3`, `sdd/ISSUES.md`, `scripts/lint-references.scan:183`, `scripts/lint-references.sh:729-730`
  - **Reviewer:** `antipattern-C`
  - **Fix:** replace with the fact the pointer carries ("every `GtkFileChooserNative` invocation grew RSS ~1 MB with no plateau over 20 cycles"), per check 1's own advice at `scripts/lint-references.sh:729-730`. Do **not** extend the pattern to catch title citations — that is the vocabulary-enumeration losing game `:58-66` argues against.

**[L13]** L8. Argument-parsing and arithmetic edges in the two smaller probes  ✅ **Done — both probes reject unknown flags and guard `--cycles 1`**
  - **Where:** `probes/accessory-view-dealloc.m:34`, `probes/accessory-view-dealloc.m:76`, `probes/accessory-view-dealloc.m:102`, `probes/appkit-panel-control.m:154`
  - **Reviewer:** `antipattern-C`
  - **Fix:** a terminal `else { fprintf(stderr, "unknown flag %s\n", argv[i]); return 2; }` in both, a `total < 2` guard, and `#include <stdlib.h>`.

**[L14]** L9. Two probes have no README section and no build recipe  ✅ **Done — see L34**
  - **Where:** `probes/accessory-view-dealloc.m`, `probes/appkit-panel-control.m`, `probes/README.md`
  - **Reviewer:** `antipattern-C`
  - **Fix:** add the two `clang -ObjC` lines and a short section each. The README's own thesis is that a claim should be re-runnable by whoever doubts it.

**[L15]** D-7 — `clipboard.rs`: "the current selection as plain text" is derived three times  ✅ **Done — one `publish_selection` helper (two sites, not three; the third was deleted)**
  - **Where:** `src/clipboard.rs:120`
  - **Reviewer:** `dry-A`

**[L16]** D-10 — `clipboard.rs`'s rustdoc describes the implementation that was rejected  ✅ **Done — rustdoc no longer prescribes the rejected `begin/end_user_action` pair (two copies fixed)**
  - **Where:** `src/clipboard.rs:185-186`
  - **Reviewer:** `dry-A`

**[L17]** L-1 — Low — Magic layout numbers in a file whose doc comment forbids literals  ✅ **Done — last bare `400` is `PANGO_WEIGHT_NORMAL`**
  - **Where:** `src/export/pdf.rs:195`, `src/export/pdf.rs:642`, `src/export/pdf.rs:16`
  - **Reviewer:** `dry-B`

**[L18]** L-4 — Low — Theme/palette colour resolution repeated four times, twice inside a per-line loop  ✅ **Done — both inks resolved once per page, not per line**
  - **Where:** `src/export/pdf.rs:616`
  - **Reviewer:** `dry-B`

**[L19]** M3 — check 9's heading extraction differs in case sensitivity (Low)  ✅ **Done — `-CaseSensitive` on check 9's PowerShell port**
  - **Where:** `scripts/lint-references.sh:1274`, `scripts/lint-references.ps1:1334`
  - **Reviewer:** `dry-C`

**[L20]** M4 — check 6a's `tests/reports/` filter is scoped differently (Low)  ✅ **Done — 6a's exclusion is target-side and anchored, matching 6b and the twin; divergence demonstrated**
  - **Where:** `scripts/lint-references.sh:90`, `scripts/lint-references.ps1:1145`, `tests/reports/`, `packaging/tests/reports/a.md`
  - **Reviewer:** `dry-C`

**[L21]** L1 — an avoidable `.expect()` in non-test code (Low)  ✅ **Done — the monitor is wired BEFORE it is stored, so the `expect` is gone rather than justified**
  - **Where:** `src/app/open.rs:219`, `src/saferizer/file_monitor.rs:45`
  - **Reviewer:** `dry-C`

**[L22]** [SEC-01] Negative printable width reaches the table grid unclamped, producing a negative scale factor (Low)  ✅ **Done via F-PDF-001 — verified against the tree**
  - **Where:** `src/export/pdf.rs:430`, `src/export/pdftable.rs:143`, `src/export/pdftable.rs:170-175`, `src/export/pdf.rs:564`
  - **Reviewer:** `security`

**[L23]** A-5 — `load_into_editor`'s rustdoc still says the clipboard-side half does not exist · Severity: **Low**  ✅ **Done**
  - **Where:** `src/window/actions.rs:618`, `src/window/tabs/lifecycle.rs:136`, `src/clipboard.rs:405`, `sdd/TECH.md:227`
  - **Reviewer:** `spec-A`

**[L24]** A-6 — `expect` in production code — POLICY § Code style · Severity: **Low**  ✅ **Done — built as a `String` from the start; the UTF-8 question cannot arise**
  - **Where:** `sdd/POLICY.md:507`, `src/lineendings.rs:142`, `src/renderer/normalize.rs:121`
  - **Reviewer:** `spec-A`

**[L25]** A-7 — Every document read now copies the whole document on the no-op path — POLICY § Input limits · Severity: **Low**  ✅ **Done**
  - **Where:** `src/docio/mod.rs:189-191`, `src/docio/mod.rs:156-162`, `sdd/POLICY.md`
  - **Reviewer:** `spec-A`

**[L26]** A-8 — TDD 1.11 landed with no human-authorship marker, unlike the sibling rubric added beside it · Severity: **Low**  ✅ **Done — markers added to 1.11 and 1.11a**
  - **Where:** `sdd/TDD.md`, `sdd/TDD.md:119`, `sdd/TDD.md:130`, `sdd/TDD.md:184`
  - **Reviewer:** `spec-A`

**[L27]** B-3 — `ScrAP-128` is cited for a claim that entry does not make  ✅ **Done — the popover claim cites ScrAP-90; the three config-dir citations were correct**
  - **Where:** `sdd/POLICY.md`, `src/saferizer/popover_anchor.rs:419`, `sdd/ANTI-PATTERNS.md:276`, `sdd/ANTI-PATTERNS.md:238`
  - **Reviewer:** `spec-B`

**[L28]** B-4 — the "every parse site reads one document" contract is convention-only; nothing stops a fifth site  ✅ **Done — the parse-site SET is now closed by a mutation-tested enumeration guard**
  - **Where:** `src/renderer/normalize.rs:67`, `src/renderer/normalize.rs:169`
  - **Reviewer:** `spec-B`

**[L29]** B-5 — `src/export/pdf.rs` is 1640 lines, well past POLICY's 500-line soft limit, and grew 545 lines in this change
  - **Where:** _see reviewer report_
  - **Reviewer:** `spec-B`

**[L30]** B-6 — the dynamic-popover exemption covers access keys but not a literal `_` in a theme name  ✅ **Done**
  - **Where:** `sdd/TDD.md:2230`, `src/app/menubar.rs:146`, `src/app/menubar.rs:150`, `src/theme.rs:438`
  - **Reviewer:** `spec-B`

**[L31]** L1. POLICY step 9 miscounts its own gate, twice, in the same paragraph  ✅ **Done — CORRECTED: the count is no longer restated in either place (script said eight, POLICY nine, reality fourteen)**
  - **Where:** `sdd/POLICY.md:160`, `scripts/lint-references.sh:3`, `probes/README.md:11-16`, `scripts/lint-references.sh:2-3`
  - **Reviewer:** `spec-C`

**[L32]** L2. `ScrAP-277` is cited in `sdd/TECH.md` and resolves to nothing  ✅ **Done**
  - **Where:** `sdd/TECH.md:161`, `sdd/ANTI-PATTERNS.md`, `sdd/scrap-numbers.manifest`, `sdd/TECH.md`
  - **Reviewer:** `spec-C`

**[L33]** L3. `ScrAP-34a` / `ScrAP-34b` are cited from `src/` but are not in the number manifest  ✅ **Done — CORRECTED: the proposed manifest fix BREAKS check 9 (measured); citations disambiguated instead**
  - **Where:** `src/limits.rs:109`, `src/imagefetch.rs:27`, `src/renderer/start.rs:412`, `src/links.rs:312`
  - **Reviewer:** `spec-C`

**[L34]** L4. Two of the newly added probes have no section in `probes/README.md`  ✅ **Done — all SIX undocumented probes given sections and build recipes (the finding named two)**
  - **Where:** `probes/accessory-view-dealloc.m`, `probes/appkit-panel-control.m`, `probes/README.md`, `probes/README.md:3-6`
  - **Reviewer:** `spec-C`

**[L35]** L5. TDD §4.13's surface enumeration omits the Rename dialog, and the code comment beside it undercounts the choke point  ✅ **Done — Rename added to the rubric and all three code enumerations (five commands, not four)**
  - **Where:** `sdd/TDD.md:509`, `src/macwordnav.rs:58`, `src/window/editbar/dialog.rs:106`, `src/window/editbar/insert.rs:28`
  - **Reviewer:** `spec-C`

**[L36]** L6. ISSUES entry D carries a ~180-line upstream GTK filing for a different defect, at a colliding heading level
  - **Where:** `sdd/ISSUES.md:126`, `sdd/ISSUES.md:9-15`, `sdd/ISSUES.md:20`
  - **Reviewer:** `spec-C`

**[L37]** L7. `sdd/ANTI-PATTERNS.md` gained seven entries while already past its soft ceiling, with no migration offer
  - **Where:** `sdd/ANTI-PATTERNS.md`, `scripts/lint-references.sh:1355`, `scripts/lint-references.sh:1361`
  - **Reviewer:** `spec-C`

**[L38]** L8. Entry S's four preconditions are introduced as three  ✅ **Done — fixed where the text actually lives now (`probes/native-chooser-rss-investigation.md`)**
  - **Where:** `sdd/ISSUES.md:1083-1096`
  - **Reviewer:** `spec-C`

**[L39]** L9. Issue G is classified `Any` on two-platform evidence  ✅ **Done — G's evidence basis recorded; `Any` justified by inherence, not by a third reproduction**
  - **Where:** `sdd/ISSUES.md:22`, `sdd/ISSUES.md:3-8`, `sdd/ISSUES.md:21`
  - **Reviewer:** `spec-C`

**[L40]** T-4 — `normalize_lone_cr_bytes` has no direct test, including of the one thing that makes it a separate function  ✅ **Done — three direct tests including the non-UTF-8 case that makes it a separate function**
  - **Where:** `src/lineendings.rs:151-158`, `src/lineendings.rs:277`, `src/docio/mod.rs:194-196`, `sdd/TDD.md`
  - **Reviewer:** `testability-A`

**[L41]** T-6 — The clipboard tests bypass `docio::settle`, and one load-bearing assertion passes on an empty buffer  ✅ **Done — the assertion now uses the production predicate; the old form passed on `"a\rb\r\nc"`**
  - **Where:** `src/clipboard.rs:299-313`, `src/docio/mod.rs:457-470`, `src/clipboard.rs:417-420`
  - **Reviewer:** `testability-A`

**[L42]** T-7 — `load_into_editor`'s rustdoc states a contract this same merge reversed  ✅ **Done**
  - **Where:** `src/window/actions.rs:614-621`, `src/lineendings.rs:256`, `src/window/tabs/lifecycle.rs:136`, `sdd/TDD.md`
  - **Reviewer:** `testability-A`

**[L43]** T10 — The probe findings that did *not* become guards (Low)  ✅ **Done — half (a) closed by retraction; half (b) now enforced by two mutation-tested clippy bans**
  - **Where:** `probes/README.md:322-323`, `src/lineendings.rs:258`, `src/clipboard.rs:361`, `src/lineendings.rs:237-251`
  - **Reviewer:** `testability-C`

**[L44]** T11 — Two probes print a GtkSourceView version they never measure (Low)  ✅ **Done — MEASURED at runtime; the probe had been printing 5.20.0 while running on 5.4.1**
  - **Where:** `probes/undo-replay.c:71`, `probes/crlf-run-boundary.c:183-184`, `probes/README.md:36-42`, `probes/binding-shape.c:48`
  - **Reviewer:** `testability-C`
  - **Fix:** ```c g_print("GTK %d.%d.%d / GtkSourceView %u.%u.%u / %s\n", gtk_get_major_version(), gtk_get_minor_version(), gtk_get_micro_version(), gtk_source_get_major_version(), gtk_source_get_minor_version(), gtk_source_get_micro_version(), G_OBJECT_TYPE_NAME(gdk_display_get_default())); ```

**[L45]** T12 — `WriteGate::is_busy`'s "ONE SANCTIONED CALLER" rule has no enforcement (Low)  ✅ **Done — `clippy.toml` ban, one sanctioned `#[allow]`, mutation-tested**
  - **Where:** `src/winstate/writegate.rs:66-85`, `src/app/open.rs:260`
  - **Reviewer:** `testability-C`


## 🧹 Tidy (Optional)

**4 findings.** No functional impact. **Terminal** — no verification round follows, and you owe no explanation for skipping any of them. Three were retained by orchestrator discretion over a nitpick score of 1 because they are *misleading* documentation rather than merely untidy: a doc that states the opposite of the code costs the next agent real time.

**[T01]** A16 — `normalize_lone_cr_bytes` accepts arbitrary bytes but is only sound for UTF-8  ✅ **Done — UTF-8 precondition stated**
  - **Where:** `src/lineendings.rs:151-158`, `src/docio/mod.rs:194`
  - **Reviewer:** `antipattern-A`

**[T02]** D-11 — Idiom sweep: two discarded `Result`s, one dangling issue citation, one duplicated clause, one pass-through  ✅ **Done — the pass-through's reason for existing is recorded**
  - **Where:** `src/clipboard.rs:268`, `src/clipboard.rs:75-76`, `src/clipboard.rs:76`, `sdd/ANTI-PATTERNS.md:4266`
  - **Reviewer:** `dry-A`

**[T03]** A-9 — `sdd/TDD.md`'s own index no longer covers section 1 · Severity: **Tidy**  ✅ **Done — index row reads 1.1 – 1.11a**
  - **Where:** `sdd/TDD.md:5`
  - **Reviewer:** `spec-A`

**[T04]** A-10 — TECH.md calls `lineendings.rs` "display-free" when half of it is GTK-bound · Severity: **Tidy**  ✅ **Done — TECH.md row now separates the display-free repair from the GTK-bound arming half**
  - **Where:** `sdd/TECH.md:227`, `scripts/coverage.sh`
  - **Reviewer:** `spec-A`


---

## ✅ Resolved

Nothing yet — this is Round 1 of this campaign. Findings resolved in later rounds move here with
the commit that fixed them.

---

## ✅ Positives

Recorded because they are load-bearing, and because a reviewer that only ever reports faults is
one you learn to discount.

- **The undo-divergence property is already properly tested.** The brief predicted this would be
  the hardest thing in the change to verify and probably need a human at a keyboard.
  `testability-A` found a working seam asserted in *both* directions at `lineendings.rs:411` and
  `:439`, with the precondition break deliberately staged at `:441`. Nine headless unit tests over
  a pure `&str → Cow` function.
- **The menubar access-key guard is genuinely derived from the live menu model**, not a mirror —
  confirmed independently by `spec-B` and `dry-B`. The commit message's claim is true.
- **Word navigation was properly extracted**, not copy-pasted: one pure `word_movement`, one
  `key_target`, two thin wirings, five one-line call sites (`dry-C`, verified negative).
- **`winstate/` contains no duplicated path or identity logic** — path equality exists exactly
  once in the tree, at `src/app/open.rs:75` (`dry-C`, verified negative).
- **The clipboard rework is a net security reduction** — plain `STRING` only, weak buffer ref, and
  the merge *tightened* path admission (it now re-checks on every re-read).
- **The four production `Parser::new_ext` sites really do all share the normalisation pre-pass** —
  `spec-B` re-enumerated them itself rather than trusting the commit message.
- **`selfdelete.rs`'s short-circuit order is correct** and the `is_busy` exception is properly
  declared at the type; the fix does *not* trade a false positive for a false negative
  (`antipattern-C`, answering a question posed adversarially).
- **Several reviewers filed deliberate anti-recommendations** rather than padding: don't
  de-duplicate the ObjC probe boilerplate (`accessory-view-dealloc.m:17` rests its conclusion on
  the two probes sharing no code path — the duplication is load-bearing); don't build a trait seam
  over `gdk::Clipboard`; don't add a measurement trait for Pango.

---

## Checklist Assessment

| Area | Status | Notes |
|------|--------|-------|
| Functionality | ⚠️ | Two user-visible defects: PRIMARY selection destruction (F-CLIP-001), clipped/unscaled PDF tables at depth (F-PDF-001). Neither is Critical; both are reachable in normal use. |
| Code Quality | ⚠️ | Large duplication surface across the shell/PowerShell twins and the macOS pipeline copy; four document readers where the merge claims one. |
| Testing | ❌ | The dominant theme of this round. Armed gates with no evidence they can fail; a self-test blind to its own subject; 1020 test cases with zero covering `macwordnav`; property sweeps whose input ranges exclude the violating case. |
| Security | ✅ | One Low finding, merged upward on other grounds. Four of five attack surfaces cleared with reasons. Path handling tightened. |
| Performance | ✅ | Nothing raised beyond one Needs-Verification resource-amplification item (VERIFY-01). |
| Documentation | ⚠️ | Three bad SDD citations in a change that added the policy against them; a rustdoc that contradicts its own merge; a self-refuting "this is greppable" invariant. |

---

## Verification status of findings in this document

| Class | Status |
|---|---|
| F-GATE-001, F-CLIP-001, F-PDF-001, F-PLAT-001, F-GATE-007 | **Orchestrator-verified by direct measurement** (source read, GDK source read, `git diff`, `cargo llvm-cov`, `cargo test --list`) |
| Remaining High | Verified by ≥1 reviewer with a grep-confirmed location; convergent findings cross-checked between reviewers |
| Medium / Low / Tidy | Reviewer-verified locations; orchestrator spot-check pending (see `docs/audit-trail.md`) |

**Honest limitation:** the batch line-reference pass (330 claims, 14 Haiku agents) proved to be
the wrong instrument for roughly half of this round's findings and its aggregate accuracy figure
is **not reported**, because it would be measuring my batching rather than the reviewers. Details
and the correction in `docs/audit-trail.md`.
