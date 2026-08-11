# Manual Test Plan

Exhaustive manual/GUI verification checklist for Scribobulate. This complements
`cargo test` (logic-only, no display) — everything here requires a running,
visible window and is **not** automatable through the unit-test suite (POLICY.md
"Manual integration testing").

Audience: a human tester, or an agent driving the app — on Linux via
`xdotool`/`import` (§1 "Dev loop"), on macOS via the calibrated-click procedure in
**§A.2**. Every checklist item traces to a `sdd/TDD.md` rubric number where one
exists; items in the final section are proposed additions not yet in TDD.md —
promote them there if the operator agrees they're worth a permanent behavioral
contract.

**The checks are platform-neutral; the mechanics are not. §A holds the
difference.** Every instruction that varies by operating system — how to drive the
app, launch, kill, install it, read its logs, what the session must provide — lives
in **§A "Platform procedures"**, under `§A.1 Linux` / `§A.2 macOS` / `§A.3 Windows`.
Items themselves say only *what* to do and point there with
**`(platform procedure: §A)`** for *how*; none of them carry a platform-specific
command inline.

That separation is not tidiness, and it is worth understanding before running the
plan anywhere but Linux:

- **A check that cannot be driven on your platform has not failed.** The most
  expensive mistake available here is reading a harness gap as an application
  defect — chasing a bug that does not exist because the click never landed, or the
  screenshot captured the wrong window. That is not hypothetical; it is the whole
  substance of GTK4Rs/AP-163, where four separate "the app is broken" diagnoses on
  macOS all turned out to be gaps in the driving mechanism. §A exists so that
  "there is no procedure for this here" is a *finding you can look up*, recorded
  as **NOT-RUN with a reason**, rather than something you discover by debugging the
  wrong layer for an hour.
- **Each subsection answers the same seven headings, in the same order**, and a
  platform that cannot do one answers it **"not available"** with the reason — never
  by omitting the heading. So a reader can always distinguish *"no route exists
  here"* from *"nobody wrote it down"*, which are very different states and look
  identical when a section is simply silent.
- **One command, one home.** A command appears in exactly one place, so adding a
  platform is an edit to §A rather than a hunt through this file, and a command
  that changes cannot rot in a copy nobody remembered. The cost is one lookup for a
  Linux runner; §1 remains the Linux loop written out in full, precisely because
  most runs are there.

Whether an item *applies* at all is a separate question and stays at the item, as a
declarative gate (`7.10 (X11 only)`) — that is a statement about the behaviour under
test, not about the harness.

**This file is an immutable template + instruction set — do NOT mark it up during
a run.** The `- [ ]` boxes are the canonical list of checks, not a mutable
checklist: never tick them, annotate them, or record PASS/FAIL/dates inline here.
To run the plan, derive a **separate** mutable run-sheet from this file (copy the
item list into a scratch file, or track status directly in
`tests/reports/MANUAL-TEST-REPORT.md`) and record every verdict there. This keeps this
document a clean, diffable contract, keeps results with the report/findings, and
means a wedged run can be **resumed from a checkpoint** (the run-sheet's last
completed item) instead of re-running the whole pass. See §7 for how a pass
records results.

Reports and audit trails are **deliberately non-versioned**: `tests/reports/`
is git-ignored (`.gitignore`) because these files are temporal run artifacts,
not permanent project documentation — they record the state of one pass
against one binary, and go stale the moment the code changes again. Any
finding that should persist gets dispersed into the appropriate SDD document
(`sdd/ISSUES.md`, `sdd/ANTI-PATTERNS.md`) or a skill, as directed by the human
operator; the raw report/audit files themselves are not meant to survive
past the session that produced them.

---

## 1. Dev loop (how to run every test below)

Canonical source: **'Automated UI Testing' from the GTK4-Rs skill** — the
actively-maintained home for the whole loop (launch/PID discipline, window-ID
lookup, input delivery, capture, Xvfb, troubleshooting). This section is a
condensed, copy-pasteable quick reference plus the parts specific to this app;
load the skill if anything here is unclear or a step misbehaves. `sdd/POLICY.md`
states *when* live verification is obligatory, not how to perform it.

**This section is the LINUX loop.** It is `§A.1`'s drive-loop answer, written out
in full because Linux is the reference platform and most runs are here. On macOS or
Windows, read **§A** instead — and note that anything in this document tagged
`(platform procedure: §A)` is answered there for all three platforms, not here.

**Precondition:** X11 session (`echo $XDG_SESSION_TYPE`). On native Wayland,
prefix with `GDK_BACKEND=x11` to force XWayland (screenshot capture needs the
X server) and note that compositing-path bugs won't be caught this way.
`DISPLAY` is already set in the environment — never prefix commands with it —
**unless you're on a private `Xvfb` display, the default for routine runs; see
§1.10 before your first launch.**

### 1.1 Build

```bash
cargo build --release
```

Release is the reference build for both behavior and footprint (POLICY.md).

### 1.2 Launch (always `-n`, always tracked PID)

```bash
cargo run --release -- -n [file.md ...] &
```
Use the harness's background-task mechanism (`run_in_background: true` for an
agent; a plain `&` + note the job for a human) — never a bare `&` inside one
synchronous call, it gets reaped when the call returns.

`-n`/`--new-instance` is mandatory: without it a second launch is forwarded to any
already-running instance (TDD 8.5), and you silently test a stale binary. This
applies on **both** platforms — over D-Bus on Linux, over `src/platform/mac/single_instance.rs`'s socket
on macOS — so a habit built on "macOS always starts its own process" is now wrong
and will quietly hand your test to yesterday's build.

Find the **actual** GTK process PID (not `cargo`'s, since `cargo run` spawns a
child):
```bash
pgrep -a scribobulate
```

⚠️ **Never `pgrep -f '<path fragment>'` for this.** `-f` matches the entire command line of
every process on the host — **including the shell that is running the `pgrep`**, whose command
line contains the pattern you just typed. It returns that shell, every later
`xdotool search --pid` finds no windows, and a perfectly healthy app reads as a capture
failure. Prefer the PID the launch gives you (`$!` when backgrounding the binary directly) or
a before/after `pgrep -x` diff. Same hazard, stated per-case, in items 8.2m and §1.10.

### 1.3 Find the toplevel window

```bash
for _ in $(seq 1 20); do
    WID=$(for id in $(xdotool search --pid <PID> 2>/dev/null); do
        i=$(xwininfo -id "$id" 2>/dev/null)
        w=$(echo "$i" | awk '/Width:/{print $2}')
        m=$(echo "$i" | awk '/Map State:/{print $3}')
        o=$(echo "$i" | awk '/Override Redirect State:/{print $4}')
        [ "${w:-0}" -gt 100 ] && [ "$m" = "IsViewable" ] && [ "$o" = "no" ] \
            && { echo "$id"; break; }
    done)
    [ -n "$WID" ] && break
    sleep 0.5
done
```
Confirm it's really your process: `xdotool getwindowpid $WID` must equal `<PID>`.

**All three conditions are load-bearing — width alone is not enough.** One PID owns
several X windows: besides the toplevel there are 1×1 helpers and, once any menu or
popover has been opened, **override-redirect popup surfaces** which can be *larger*
than 100px and are usually `IsUnMapped`. Selecting on size alone picks whichever the
server lists first, so the same command works early in a run and silently picks a
popover later. The failure is quiet and misleading: `xdotool getwindowpid` still
matches (it really is your process), `windowsize`/`windowmove` "succeed" against the
invisible surface while the real window never moves, and the capture fails with
`import: unable to read X window image ... Resource temporarily unavailable` — which
reads like a screenshot-tool problem rather than "you selected an unmapped window".

### 1.4 Screenshot (no focus stolen)

```bash
import -window $WID /tmp/scrib_<step>.png
```
Reads the server-side backing store — window can be unfocused/obscured.
**Open menus/popovers are a separate X surface** and won't appear in this
capture — use `import -window root` (whole screen) to see an open menu, then
`xdotool key Escape` (untargeted) to dismiss it before continuing.
**Do NOT judge an action's enabled/disabled state from a screenshot** — sensitivity
often has no pixel signal at all (GTK4Rs/AP-67). Read the enabled bit over
`org.gtk.Actions`, or verify functionally: invoke the action and observe its effect —
e.g. for Save, click it and `cat` the file. A disabled action-button silently ignores
the click; an enabled one acts.

### 1.5 Drive input

```bash
# Click (explicit down/sleep/up — bare `click` is sometimes too fast for GTK's
# click-gesture recognizer and silently no-ops). SEPARATE the move from the press
# and let it settle: chaining `mousemove … mousedown` in ONE xdotool invocation has
# been measured to deliver a press the app's gesture never sees at all (no handler
# call, yet a popover still appeared — so the capture looks like a real result for
# the PREVIOUS pointer position; GTK4Rs/AP-252):
xdotool windowactivate --sync $WID
xdotool mousemove --window $WID <x> <y>
sleep 0.3
xdotool mousedown 1
sleep 0.15
xdotool mouseup 1

# Type:
xdotool windowactivate --sync $WID type "text"

# Key/chord — NEVER --window-target a key send (a popover/menu grab silently
# swallows it); use the untargeted form so it reaches whatever holds the grab:
xdotool key ctrl+s
```
Re-focus (`windowactivate --sync`) at the start of every input call — focus is
not durable across separate shell invocations.

### 1.6 Cleanup

```bash
kill <PID>          # never pkill by name — may kill the user's own instance
ps -p <PID>          # confirm gone
```
Never `pkill -f <pattern>`: it matches full command lines including the invoking
shell's own argv, so a multi-line call whose later lines mention the pattern kills
itself (GTK4-Rs skill, "self-kill hazard"). In a shared single-instance session with
multiple test windows, close and recount one at a time.

### 1.7 Is a defect a regression? (also: the positive control for a fix)

```bash
git stash && cargo build --release          # build the pre-change binary
cp target/release/scribobulate /tmp/pre     # KEEP it under its own name
git stash pop && cargo build --release      # rebuild — see the warning below
# ...drive BOTH binaries through the same steps, capture, compare...
```

**`git stash pop` does not invalidate `target/`.** Popping restores the source and
nothing else, so `target/release/scribobulate` is still the *pre-change* build until
something rebuilds it — and running the binary by path (rather than through `cargo
run`) never does. Every drive after the pop then exercises the old code while the shell
and the file name both say otherwise, so a **working fix reads as broken**, identically
in the "before" and "after" columns. That symmetry is the tell: if your fixed binary
behaves exactly like the control, suspect the build before the fix. Copy the control out
under its own name, `cargo build --release` immediately after the pop, and prefer
comparing two explicitly-named binaries over one path that means different things at
different times (ScrAP-239).

Two binaries, driven identically, is also how a fix earns a **positive control**
(ScrAP-217): "the notice cleared" is only evidence once the pre-change binary is shown
*not* to clear it on the same steps. Keep both captures.

### 1.8 Footprint measurement (§6 gate — run after any rendering/dependency change)

```bash
cargo build --release
# launch with a representative doc, as above, then as SEPARATE commands:
nvidia-smi                                    # must show no GPU client for the app
grep VmRSS /proc/<PID>/status                 # sum across all app processes
```
Never `pkill` a helper process in the same command that captures output — a
child core-dump silently discards stdout (POLICY.md).

### 1.9 Detecting a wedged or crashed run (for unattended / orchestrated runs)

Recovery is cheap — `kill` the tracked PID and relaunch with `-n` — but you must
first **detect** the failure. Two distinct modes need different checks.

**Crash (process died) — detection is nearly free:**
- The background launch task reports an **exit code** when the process exits;
  exit ≠ 0 and ≠ your own SIGTERM ⇒ crash (e.g. `134` = SIGABRT/panic).
- `kill -0 <PID>` (or `ps -p <PID>`) before each step as a backstop.
- For the *reason*: `grep -E 'panicked at|Aborted|non-unwinding' <launch-log>`.

**⚠ WARNING — an external SIGTERM under memory pressure (earlyoom / kernel OOM) is
NOT an app crash.** If the process exits by a *signal* (SIGTERM ⇒ code 143, SIGKILL
⇒ 137) with an **empty** stderr/launch-log (no `panicked at` / `Aborted`), it was
killed from *outside* the app — most often `earlyoom` (or the kernel OOM killer)
reclaiming memory. This is not deterministic (it depends on host memory pressure),
but it is **reliably** provoked by running several test instances at once: each is
its own `Xvfb` + window manager + release binary (~180–210 MB RSS), so a few in
parallel can cross the host's low-memory threshold. It hit this project's own
multi-agent run — and twice a kill landed while a modal save-dialog held an X grab,
**wedging the whole Xvfb display** until it was restarted. Do NOT chase it as a
Scribobulate bug.
- **Confirm** it: `pgrep -a earlyoom` (is it running?), `dmesg | grep -iE 'oom|killed process'`, and the launch task's exit code (143/137 = signalled, not a panic).
- **Mitigate** it: run chunks **sequentially, not in parallel** (§6.1 — this is a second, resource-based reason on top of token velocity); if you must parallelize, bound the concurrent-instance count and give each Xvfb a smaller screen; close instances promptly. On a hit, just relaunch and **resume from the run-sheet's last item** (Recovery, below) — the app itself is fine.

**Wedge (alive but stuck / modal grab) — cheapest check first, most involved last:**
1. **Extra-toplevel heuristic (cheapest):** a modal dialog is a *separate*
   top-level window owned by the same PID. `xdotool search --pid <PID>` returning
   more than one large toplevel — or one with `_NET_WM_WINDOW_TYPE_DIALOG` — that
   *persists* across steps ⇒ a modal was left open.
2. **CPU-state hint:** `/proc/<PID>/stat` pegged at ~100% (spin) or dead-idle
   while it should be rendering, sustained over a window, corroborates a hang.
3. **Per-step progress assertion (most robust):** give every scripted action an
   expected *observable* — an `stat -c %Y` mtime bump after a save, a
   `xdotool getwindowname` title change after a tab switch, a match-count change —
   and if the expected change does not appear within a per-step timeout, and that
   repeats for K steps, declare the chunk wedged. This also catches *silent
   wrongness*, not just hangs.
4. **Global chunk watchdog:** a hard wall-clock cap per chunk; exceed it ⇒
   kill+relaunch regardless.

NOTE: `import -window` reads the X server's backing store and **succeeds even
when the app's main loop is hung** — a returned screenshot is NOT a liveness
proof. Use the checks above, not "a capture came back."

**Routine hygiene between steps:** send an *untargeted* `xdotool key Escape`
(never `--window`-targeted — a popover/menu grab silently swallows it) to clear
any stray popover or menu before it can wedge the next step. Caveat: Escape on a
save-confirm dialog picks the dialog's default (often Cancel), which may not be
what a test intended — so use it as hygiene between *independent* steps, not as a
substitute for handling a dialog a step deliberately opened.

**Recovery:** `kill <tracked-PID>` (never `pkill scribobulate` by name — it can
also kill the operator's own instance, which shares the app-id), relaunch with
`-n`, and **resume from the run-sheet's last completed item** rather than
re-running the whole chunk or silently skipping the item that caused the wedge.

### 1.10 Xvfb vs. the operator's real session (default choice)

Full guidance: 'Automated UI Testing' from the GTK4-Rs skill, "Running
non-disruptively: Xvfb." **Default to a private `Xvfb` display for every
routine run of this plan** — nothing above (build/launch/find-window/
screenshot/drive/cleanup) changes except that `DISPLAY` now targets the
private server instead of the operator's real one:

**Pick the display number, don't assume it.** `:99` is the conventional throwaway display,
which is exactly why it is contended — another agent's `xvfb-run` may already hold it, with
its own `-auth` cookie. The symptoms point away from the real cause: `Authorization required,
but no authorization protocol specified` and `Can't open display: (null)`, which read as a
broken X/auth setup rather than "someone else is on this display". Probe for a free one, and
never kill an `Xvfb` you did not start:

```bash
for n in $(seq 120 200); do
    [ ! -e /tmp/.X${n}-lock ] && [ ! -e /tmp/.X11-unix/X${n} ] && { DISP=":$n"; break; }
done
```

```bash
Xvfb :99 -screen 0 1920x1080x24 &
XVFB_PID=$!
sleep 0.3
DISPLAY=:99 cargo run --release -- -n /path/to/file.md &
APP_PID=$!
# ...every subsequent xdotool/import command also gets DISPLAY=:99...
kill $APP_PID $XVFB_PID   # kill both when the chunk/run ends
```

**Before an unattended live `DISPLAY=:0` run, INHIBIT the screen locker, screensaver,
and DPMS blanking for the batch's duration — this is the single most common way an
overnight live run breaks.** On an idle KDE/X11 session the locker/DPMS will engage
while the operator sleeps; a locked screen grabs input (so `xdotool` clicks/keys land
nowhere) and `import -window` then captures a black surface — the run stalls or, worse,
silently *mis-verifies* against blank pixels. Inhibit at run start and release at
teardown, e.g. hold a `systemd-inhibit --what=idle:sleep:handle-lid-switch` for the run,
and/or `xset s off -dpms` (restore with `xset s on +dpms` when done); on KDE also
consider suppressing the locker via its D-Bus inhibit. Verify the inhibit took (the
screen is still awake after the idle timeout) before trusting any capture. Never
"solve" a black screenshot by retrying the capture — first prove the display is awake.

**Escalate to the operator's real X11 session only** when the item under test
could plausibly depend on compositing (real alpha transparency, a theme's
compositor-drawn shadow), a window manager (focus transfer, alt-tab, maximize/
fullscreen, multi-window stacking), or GPU rendering — a bare `Xvfb` has none
of those unless you start them yourself, and §6's footprint gate (1.8) in
particular needs the real GPU driver stack, not Xvfb. State which environment
a screenshot/result came from whenever you report a finding — "confirmed under
Xvfb (no compositor/WM)" vs "confirmed on the operator's real session" — since
a clean Xvfb result doesn't clear the compositor/WM class of bug.

### 1.11 Mobile demo (operator AFK) — optional

When the operator is away and asks for a visual demo of a feature (or you want to
show off a user-visible change), produce screenshots they can review on their phone.
The mobile app mirrors any image **displayed in the interactive TUI**, so delivery is
just reading the PNGs back with the `Read` tool — no upload or file-sharing step, and
the operator need not be at the computer.

Two hard requirements:

- **Run it in the main interactive session — never in a subagent.** Only images
  surfaced in the top-level TUI transcript get mirrored; a subagent's screenshots stay
  in its own context. If a subagent produced the change, the main agent re-runs the
  capture itself.
- **Use a private `Xvfb` display** (§1.10), not the operator's real session — they're
  AFK and a demo must not steal focus on a session they may return to. Leave their
  installed/running instance untouched.

Procedure: launch the freshly-built binary on Xvfb with `-n`, opening a document that
exercises the feature (author a small temp `.md` if it needs specific content);
`import -window $WID` a shot per state, driving transitions as in §1.5; **`Read` each
PNG** so it renders inline (that is what pushes it to mobile), one caption per shot;
then kill only the tracked PID — guard the kill so a nonzero exit doesn't abort the
rest of the script — and remove temp files. Leaving the `Xvfb` display running is fine.

**Run the real-session items UNATTENDED while the operator is AWAY — not "attended"
with the operator watching.** A live run drives the operator's *real* mouse,
keyboard, and focus on their actual display, so it both disrupts anything they are
doing AND cannot make progress while they are using the screen — the same single
constraint: **the display must be FREE.** The intended model is therefore to
**batch** the real-session-only items, get the operator's one-time greenlight, and
run them **unattended on the real session while the operator is away — e.g.
overnight while they sleep** — rather than expecting them to sit and watch. ("Prefer
Xvfb, and warn before a live run" is about not colliding with an *in-use* session,
not about needing a human in the loop: almost every real-session item needs the real
`DISPLAY` free, not a human *action*, and is fully scriptable via `xdotool`/`import`/
shell once the screen is idle — GPU footprint 6.1–6.3, D-Bus single-instance
1.6/8.1/8.2/8.5, real-compositor annotation 17.33/17.35, multi-window restore 15.10.)
The genuine exceptions that a headless overnight run still can't cover are the ones
needing a live desktop **theme toggle** the app follows (2.13/2.15/2.19/18.7 — KDE/X11
won't push `prefer-dark` to running GTK apps; needs a GNOME session) and the
**screen-reader** pass (16.5) — flag those separately for a hands-on session.

**Run to completion WITHOUT prompting — this is the point of an overnight batch, not
an optional nicety.** Once the operator has given the one-time greenlight and gone to
bed, the agent must drive the *entire* batch through to the end unattended and **never
stop to ask the operator a question mid-run.** If an item genuinely requires operator
input or a manual action the agent cannot script (a decision to adjudicate, a
credential, a physical toggle), **DEFER it** — record it in the end-of-run report's
"needs operator input" list and **continue with every other item** — rather than
halting the run to wait for an answer. Work around the blocker; do not block on it.
The only hard stops permitted are a wedged display that cannot be recovered (§1.9) or
the whole batch being finished.

Why this is load-bearing and not mere preference: the value of the overnight window is
the *volume* of live-session work done while the display is free and no human is
waiting. If the agent stops to prompt, the run freezes there until the operator wakes
and returns to the terminal — often many hours later — so (a) every *other* live item
that could have run in those hours is squandered (the free-display window is gone), and
(b) those items are then *still* undone at the moment the operator is awake and needs
the machine back for their own work, forcing the disruptive live run into their waking
day — the exact collision the overnight model exists to avoid. A single mid-run prompt
therefore doesn't cost one question's delay; it can cost the entire batch. Defer, keep
going, and consolidate everything that needed a human into one report the operator
reads in the morning.

**Launch the single-instance items (1.6 / 8.1 / 8.2 / 8.5) WITHOUT `-n`.** `-n`
(`--new-instance` ⇒ `NON_UNIQUE`) is the isolation default *everywhere else* in this
plan, but it is exactly what defeats single-instance behaviour: `-n` opts out of the
handoff by design, on every platform and for the same reason — a `NON_UNIQUE` process
never registers as primary with whatever transport the platform uses (a D-Bus session
bus on **both** Linux and Windows — GIO has no Win32 backend for this, ScrAP-249) and
`src/platform/mac/single_instance.rs` skips its election
outright (macOS) — so a second `-n` launch spawns an independent process and
these items falsely "fail." To exercise the real forwarding, the FIRST instance must
launch as the primary (no `-n`), and the second invocation (`scribobulate <file>` / an
`open` forward) is what you're testing. This is the one place the `-n` default is
inverted — and it is safe overnight precisely because the operator has no instance of
their own registered (they closed them for the run); by day it would collide with
theirs. **8.7 is the one item here that deliberately starts from no running instance**,
so it needs no such isolation.

**Definition of done + where verdicts go.** The batch is complete only when EVERY
batched item has a recorded verdict (✅/❌/⚠️/⛔/blocked-needs-operator), not when the
agent runs out of steps or budget. Write per-item verdicts to a gitignored fragment
under `tests/reports/` (e.g. `MANUAL-TEST-REPORT-live.md`) as you go — so a wedge +
resume (§1.9) never loses completed results — and end with two explicit lists: the
verdict table, and the deferred "needs operator input" items (per the never-stop rule
above) for the operator to clear in the morning.

**Prefer running a real window manager ON the Xvfb display over escalating to
the operator's live session** for anything WM-dependent — most importantly
**popover / focus / input** behavior. A bare `Xvfb` (no WM) gives every
override-redirect surface input unconditionally, so it **cannot see** a whole
class of input bugs (e.g. a non-modal `GtkPopover` whose button click falls
through to the widget beneath, or an entry that can't hold keyboard focus — see
GTK4Rs/AP-83). Start the operator's own WM on the private display to get
faithful focus/grab semantics without touching their screen:

```bash
Xvfb :99 -screen 0 6400x1440x24 & sleep 2
DISPLAY=:99 kwin_x11 & sleep 4          # the operator's real WM, headless
DISPLAY=:99 cargo run --release -- -n /path/to/file.md &
# NOTE: with a WM there is now a title bar — get the client-area origin from
# `xwininfo -id $WID | grep 'Absolute upper-left'`, not the windowmove target.
```

Any annotation-overlay item (§17.5-17.6, §17.13-17.21) MUST be run under a WM
(kwin-on-Xvfb or the real session); a WM-less pass will falsely green them.

**Multi-window items (§3 §7.6-§7.10, §4.3) need a second window positioned
apart from the first, and bare `Xvfb` starts with no window manager at all** —
`xdotool windowactivate` fails outright (no `_NET_ACTIVE_WINDOW`; use
`xdotool windowfocus` instead) and windows stack at the same position until
moved. Two things this project's own live testing (GTK4Rs/AP-60)
confirmed behave differently without a WM:
- **Positioning and the drag itself work fine bare-Xvfb.** `xdotool
  windowmove`/`windowsize` apply directly (no WM to redirect them), and a
  genuine incremental-motion cross-window drag (§1.5-style, real
  `mousedown`/`mousemove`/`mouseup`, not a warp) lands correctly — confirmed
  live for 7.6/7.7/7.9/15.6.
- **7.10 (drop on bare desktop spawns a new window) could NOT be confirmed
  bare-Xvfb** — inconclusive, not disproven; GTK4Rs/AP-49 already documents this
  specific gesture as environment/backend-sensitive. If it matters for the
  change under test, either start a minimal non-compositing WM alongside Xvfb
  (`openbox --sm-disable &`, right after the `Xvfb` line) or run that one item
  on the operator's real session instead.

**Kill both `$APP_PID` and `$XVFB_PID`** when done — a stray `Xvfb :99` left
running collides with the next display number a later run tries to claim.

---

## 2. Fixtures

**All of these exist and are checked in under `tests/fixtures/`** — the pass is
reproducible as-is; nothing here needs creating first. Add a row when you add a
fixture, and keep it checked in.

- `table-test.md` — tables, outline, blockquotes, headings (used in the smoke test)
- `image-test.md` — inline image rendering (small + wide)
- `logo.png` — small (220px) local image asset referenced by `image-test.md`
- `wide.png` — 1600px-wide image for the fit-to-viewport check (2.21)
- `themes.md` — every surface a reading theme must reach, in one document (§18)
- `checkbox-test.md` — task-list checkboxes (checked/unchecked) beside a plain list, for the drawn-gutter marker and the interactive toggle
- `doc-links.md` — one link per §19 case: in-folder siblings (incl. two spellings of one path), `#fragment` links that hit and miss, a `..` traversal, a symlink escaping the folder, a `~/` path, a non-Markdown target, an explicit `file://`, a missing target, and https/mailto controls. (The colon-in-filename case, §19.7a, has no fixture here — a colon-named file is invalid on Windows and would break checkout (ScrAP-164); it is unit-tested cross-platform in `links::scheme_of`.)
- `escapes-via-symlink.md` — symlink → `../other-fixtures/outside-doc.md`, so containment must be decided on the resolved path (19.2). **Only a symlink where the checkout could make one**: with `core.symlinks=false` git writes a 32-byte text file holding the target path instead, and the §19 steps that use it are then not exercisable — see the precondition in §19 before ticking them
- `../other-fixtures/outside-doc.md` — the out-of-containment document; `other-fixtures/` also holds `outside.png` / `splash.jpeg` for the §14 image cases

| Fixture | Purpose | Covers |
|---|---|---|
| `large-doc.md` (several MB, generated) | Responsiveness on big files | 1.4 |
| `remote-image.md` | `https://` image reference | 2.5-part, 14.1/14.2 |
| `unsafe-local-image.md` | `../outside.png` + absolute-path image | 2.5, 2.7, 14.3/14.4 |
| `unsafe-scheme-image.md` | `file://`/`smb://` image reference | 14.8 |
| `anchors.md` | Duplicate-slug headings + same-doc `#fragment` links | 2.17 |
| `nav-keys-table.md` | A table with both cell shapes (plain/mixed `GtkLabel` and a pure-link `GtkLinkButton`) between two long runs of filler, so a cell is far from either end of the document | 9.33a/9.33b |
| `superscript-subscript.md` | tight `E=mc^2^`, multi-tilde `H~2~O and CO~2~`, `~~struck~~`, nested `~~a **b** c~~` | 10.10 (+ nested-strike limitation) |
| `highlight.md` | `==mark==` in prose (with internal spaces), inside a list/blockquote/table cell, mixed with bold/italic/code, and prose `a == b`/`x -= 1`/`== spaced ==` that must stay literal | 10.10a (across all four reading themes) |
| `code-blocks.md` | Every code-block shape in one file: a languaged fence, an unlanguaged fence, an indented (4-space) block, one inside a blockquote and one inside a list item, each with prose before and after for crossing selections | 2.8h |
| `line-breaks.md` | single-newline-separated lines + a blank-line paragraph gap | 2.19 |
| `lists.md` | All three bullet types (unordered, ordered `1.`, task `- [ ]`): short/long-wrapping/multi-source-line/nested items, two-digit ordered markers, and a loose item (cell-marker pairing) | 2.4, 2.4a |
| `emoji-and-unicode.md` | Non-ASCII filename target, emoji in headings/body | §4.5 below |
| `crlf-doc.md` | Windows line endings | §4.2 below |
| `second-doc.md` | A second, distinct file for multi-tab/multi-window scenarios | 15, 7.6-7.10 |
| `html-injection.md` | Embedded `<script>`/`<iframe>`/`onerror=` HTML | 2.7 |
| `deeply-nested.md` | Deeply nested lists/blockquotes/tables (stress) | §4.4 below |
| `annotations.md` | `{==highlight==}{>>comment<<}`, a standalone `{>>point<<}`, two annotations on one line, `{++ins++}`/`{--del--}`/`{~~a~>b~~}`, an unclosed `{==`, and a `{==` whose close crosses a blank line | 17.1-17.4, 17.9-17.12 |
| `annotate-inline.md` | A first paragraph mixing soft-wrapped lines, inline code, and **bold**, followed by an `xdotool` code fence and a cross-construct paragraph — the shape where a single-word selection used to engulf the whole block | 17.19 |
| `annotations-viewer.md` | 20 comment-bearing annotations scattered across ~186 lines (screens of filler between them, so navigation really scrolls), a **bare** `{==highlight==}` plus all three inert kinds, an annotation inside a **table cell**, and multi-byte text (em dashes) ahead of later annotations | §20 (esp. 20.2, 20.6, 20.7, 20.12, 20.14, 20.16) |

A throwaway external-writer script is also useful for §3/§5 (live reload):
```bash
# append a line every 0.2s x 20, to test coalescing (TDD 3.3)
for i in $(seq 1 20); do echo "edit $i" >> /tmp/scrib_live.md; sleep 0.2; done
```

---

## 3. Checklist by TDD.md section

Each line: rubric number, the concrete action, and what to look for. "Screenshot"
means capture and Read the image, don't infer from the log alone — see the
gtk4-rs skill's dev-loop doc on why geometry/rendering bugs leave no warning.

> **Before trusting any driven run, prove the input channel.** Send one keystroke and
> one click whose effect you can check independently (a tab count, a log line, a file on
> disk) and verify both landed. Measured on this machine under Xvfb + openbox: pointer
> events reached a GTK4 app while **every keystroke was silently dropped**, with
> `getactivewindow`/`getwindowfocus` both reporting the app's window and no tool
> erroring — so a whole drive can deliver nothing and be indistinguishable from one that
> found no bugs (ScrAP-245). Note also that `xdotool mousemove` takes SCREEN coordinates
> while screenshots are window-relative: add `xwininfo -id <win>` "Absolute upper-left".
> A pointer-only drive is a legitimate fallback — toolbar buttons and in-document
> affordances (task checkboxes, links) reach most paths without a keyboard.

### §1 Opening & displaying documents
- [ ] **1.1** `cargo run --release -- -n tests/fixtures/table-test.md` → content renders on launch
- [ ] **1.2** With a window already open and focused, File ▸ Open another file → lands as a tab of *that* window
- [ ] **1.3** `cargo run --release -- -n /tmp/does-not-exist.md` → empty editable doc targeting that path, no crash
- [ ] **1.4** Open `large-doc.md` → renders, window stays responsive (drag-scroll during load; no beachball/freeze)
- [ ] **1.4c** **Document I/O never freezes the window (the slow-filesystem check).** Stage the mount with `scripts/slowfs.py` — unprivileged FUSE, no root, read-delay only (read its module doc for why writes are not delayed and what covers them instead): `python3 scripts/slowfs.py --backing /tmp/docs --mount /tmp/slow --read-ms 1500`, then `fusermount -u /tmp/slow` afterwards. For an indefinite hang rather than a delay, sshfs to localhost and `kill -STOP` the sshfs process. With that in place: (a) open the document → the *existing* windows keep redrawing (drag one, scroll it) while the read is outstanding; (b) File ▸ Reload the same document → same. The **write** half of this check (Ctrl+S on a slow mount) is not reachable with this rig — `docio::slow_io` pins it as an automated test instead, and 4.10/4.11/5.7 below say the same. Any frozen window, greyed-out ("not responding") title bar, or unresponsive scroll during a step is a **FAIL** — this is the whole point of the change (TDD 1.4c). Resume the stopped process afterwards. If no slow mount can be staged on the machine at hand, record this item as **NOT RUN** rather than green: a local-disk run proves nothing here, because a local read returns before the window could have blinked.
- [ ] **1.4e** **Progress is reported for a slow operation and never for a fast one.** With the 1.4c mount: File ▸ Open a document from it → the status bar reads **"Opening…"** while the read is out and clears when the tab appears; File ▸ Reload → **"Reloading…"**. Then, on a **local** document, save/reload repeatedly and watch the footer — it must stay completely still. A flicker on a fast operation is a FAIL, not a cosmetic quibble: an indicator that blinks on every ordinary save is one nobody reads when it matters (TDD 1.4e).
- [ ] **1.4d** **A slow read must not delay a crash-recovery snapshot.** With the slow mount from 1.4c (note the state directory must NOT be on it — that is the point): open ~12 documents from it, type into one so it is dirty (arming a snapshot), then touch every one of those files externally at the same instant (`touch /slow/*.md`) so the watcher re-reads them all at once. Within a couple of seconds the dirty document's swap file must appear/refresh under `$XDG_STATE_HOME/scribobulate/swap/` (`ls -l --time-style=full-iso`). A snapshot that only lands once the reads finish is a FAIL (TDD 1.4d; the bound is `docio/pool.rs`'s, ScrAP-243).
- [ ] **1.5** From a blank/untouched window, File ▸ Open a file → loads into the existing blank tab, no new tab/window created (test with the blank tab NOT active, to cover "every tab, not just active")
- [ ] **1.9** **A UTF-8 BOM does not eat the first heading.** Write the *same* content twice, once with a BOM and once without, and compare them in one session — a single-file run cannot tell this defect from a bad fixture. On Windows: `Set-Content -Path bom.md -Encoding utf8 -Value "# BomHeading`n`nbody"` (5.1's `utf8` **is** BOM-ful; that is the point) and `[IO.File]::WriteAllText("$PWD\nobom.md", "# BomHeading`r`n`r`nbody`r`n")`. Elsewhere: `printf '\xEF\xBB\xBF# BomHeading\n\nbody\n' > bom.md`. Confirm the bytes with `Format-Hex bom.md | Select -First 1` / `xxd -l4 bom.md` → `EF BB BF`. Open both → **both** render "BomHeading" as a heading and **both** list it in the outline sidebar; the `#` must not appear as literal text and the outline must not say "No headings". Then save `bom.md` (Ctrl+S after any edit) → it is rewritten without the BOM, which is intended, and no "File changed on disk" prompt appears at any point (TDD 1.9).
- [ ] **1.6** `-n` forces `NON_UNIQUE` (POLICY / `main.rs`) so a `-n`-launched instance never owns the D-Bus name and can never RECEIVE a forward — this is the one item in this document where BOTH launches must omit `-n`, since 1.6 tests the batch-forward path itself, not File ▸ Open (see TDD 1.6). After `cargo build --release` (so the primary is a fresh binary — the usual staleness risk `-n` guards against doesn't apply here since the primary is what actually runs the test): launch the primary with `./target/release/scribobulate <file1.md> &`, track its PID, dirty its only tab, then run `./target/release/scribobulate <file2.md>` (foreground, no `-n`) — it D-Bus-forwards to the primary and exits immediately itself → `file2.md` opens in a brand-new window OF THE SAME (tracked) PID; the original window/tab is left untouched, not reused

### §2 Rendering fidelity
- [ ] **2.1** Open a doc mixing headings/bold/italic/lists/links/blockquotes → each renders with correct styling
- [ ] **2.2** Open `table-test.md`; narrow the window → wide cells wrap, no horizontal scrollbar, borders span full row height
- [ ] **2.9** **A link in a table cell is a link, in BOTH cell shapes** (GTK4Rs/AP-239; **Document Rendering CAM rows 2, 11 and 12** — display, interaction and appearance parity in a container context). Open `table-test.md` in Preview. §3's cells are *pure-link* cells (`GtkLinkButton`); the §2 cell reading "A [link to example] plus **bold** and *italic*…" is a *mixed* cell (a `GtkLabel` carrying `<a href>`). For **each shape in turn**: (a) the caption is in the theme's **link colour and underlined**, matching a body link on the same page — a caption in body ink is the GTK4Rs/AP-239 defect, and the two shapes differing from *each other* is the same defect in its theming half; (b) hovering shows the **pointer** cursor and, after a moment, a tooltip with the **URL**; (c) clicking opens the system browser and the preview does not navigate away. Then open `anchors.md` in Preview and use the table under the intro: click the **pure-link** cell and the **mixed** cell → both scroll to "Deep", landing in the same place as clicking the body "Jump to Deep" list item above (all three go through one activation path; if one of them instead does nothing, that shape is bypassing it). Hover the `&`-in-the-URL row's link → the tooltip shows the **whole URL including `&b=2`** — a blank or truncated tooltip there is the double-escape half of GTK4Rs/AP-239, and it is silent. Re-check the colour under a **reading theme** (View ▸ Theme ▸ Sepia or Neon), where a cell link falling back to the desktop's blue is unmissable.
- [ ] **2.13** Toggle desktop light/dark theme with a blockquote open → bar color follows accent, not hardcoded
- [ ] **2.14** Dark theme: code/link colors are dark-appropriate, not light-scheme values
- [ ] **2.15** Toggle desktop theme while a doc is open → re-renders live, no reload, no re-read from disk (touch mtime check)
- [ ] **2.11** Blockquote shows left bar, indented body, aligned with body-text column
- [ ] **2.10** Table immediately followed by a heading → heading starts on its own line
- [ ] **2.3** Fenced ` ```rust ` block → monospace + syntax highlighting
- [ ] **2.3a** Fenced ` ```typescript ` block (also try `tsx` and `toml`) → monospace **and multi-colour** syntax highlighting — keywords/strings/comments render in distinct inks, NOT a single flat ("all-gray") colour like an unlanguaged fence. (Regression guard for ScrAP-160 — syntect's default set omits TypeScript/TSX/TOML and silently falls back to plain text; `two-face` closes the gap. Pure-render, reproduces on bare Xvfb.)
- [ ] **2.4** `- [ ]` / `- [x]` list → checkboxes reflect state; a task item shows **only** the checkbox (no redundant `•`/number before it), at every nesting depth
- [ ] **2.4a** Open `lists.md` → for unordered, ordered, and task lists the marker sits alone in a left **gutter** (drawn, not buffer text) and every wrapped / multi-source-line / loose-blank-line-paragraph line aligns under the item **text** at the uniform content margin (never left of it, incl. loose items — the old cell-marker pairing outdent is gone); an ~8px gap separates items; nesting indents further. Numbers right-align (`9.`/`10.` line up).
- [ ] **2.4a-wrap** Open `lists.md` in **Preview** (or Split), then **narrow the window** until the long items soft-wrap onto two or more visual lines (zooming in a couple steps first, Ctrl++, makes the wrap happen at a wider width). For every kind — the long unordered item, the wrapping ordered item (`11.`), and the wrapping task (`- [x]`) — the marker (bullet dot, number, checkbox) must stay **top-aligned on the item's FIRST visual line**, level with the first line of its text, NOT drift down toward the vertical middle of the wrapped block. A single-line item is unaffected. (Regression guard for GTK4Rs/AP-145 — `line_yrange` returns the whole logical line, so centering on its full height floated the marker to the middle row; reproduces on bare Xvfb, no compositor needed.)
- [ ] **2.4a-container** In `lists.md` ▸ "In a container" → every quoted list sits **wholly inside** the blockquote: each marker (bullet, number, checkbox) draws to the **right of the quote's accent bar**, never on or left of it, and the items indent from the **quote's** text margin, not the body margin — the quote must not read lopsided (inset on the right by the quote but not on the left). Compare against the unquoted lists above: a quoted item sits **further right** than the same item at body level. Each nesting level steps in by **exactly one level's worth** from the level above — a nested quoted item's marker stays beside its own text, not stranded far left of it. (Traces TDD 2.4a container clause / Document Rendering CAM row 2; GTK4Rs/AP-96.)
- [ ] **2.4b** Open `checkbox-test.md` (or `lists.md`) in Preview or Split → click a task checkbox, **including near its LEFT edge**: it toggles `[ ]`↔`[x]`, the tab goes dirty ("Unsaved changes"), a second click reverts, and **Ctrl+Z** undoes exactly the toggle with the caret unmoved. Hovering a checkbox shows a pointer cursor + accent border; bullets/numbers are NOT interactive. (Regression guard for the GtkPaned wide-handle edge-click fix, GTK4Rs/AP-93 — reproduces on bare Xvfb.)
- [ ] **2.5** Open `image-test.md` → image loads relative to doc dir, not CWD; alt text hidden when image shown
- [ ] **2.21** In `image-test.md`, toggle Split (Alt+2) or narrow the pane below the image width → the 1600px `wide.png` scales down to fit the pane (no blank, no h-scrollbar) and re-fits on resize; the 220px `logo.png` stays 220px, left-aligned (not stretched)
- [ ] **2.12** Link inside a blockquote → opens in browser (not inert styled text)
- [ ] **2.6** Click an external link in body text → opens in system browser, preview pane doesn't navigate away. **With the browser ALREADY RUNNING and Scribobulate focused**, its window must **raise to the front** — a tab opening silently *behind* Scribobulate is a FAIL (tokenless launch, GTK4Rs/AP-99) and is indistinguishable from "the link did nothing"
- [ ] **2.24** Open `anchors.md` in Preview → put the pointer on the blank area to the RIGHT of "Jump to Deep", press and hold, drag LEFT across that link's text, release **on the link**. The text selects and the view must **stay where it is** — any scroll to the "Deep" heading is a FAIL (the drag's release activating a link it never clicked, GTK4Rs/AP-169). Repeat three ways, all of which must leave the view put: (a) press on "Jump to first Section", release on "Jump to Deep" (two different links); (b) press and release **inside one link's caption**, having dragged across it (selecting a caption to copy it must not navigate); (c) drag from body text upward, ending on a link. Then confirm the affordance still works: with nothing selected, a plain click (no drag) on "Jump to Deep" **does** scroll to the heading. (A click landing *inside* an existing selection is a known no-op that predates this rule — GtkTextView holds that press for a possible drag-and-drop and the app's gesture never sees the release; clear the selection first, or click twice.) Same-line drags avoid the autoscroll margins that make a synthetic drag scroll the view on its own (GTK4Rs/AP-142's aside / GTK4Rs/AP-144); reproduces on bare Xvfb. (TDD 2.24)
- [ ] **2.24-affordances** With `checkbox-test.md`: drag-select body text and release the drag over a **task checkbox** → it must NOT toggle (no dirty flag). With `annotations.md` in Preview: drag-select a line and release over a right-margin **comment marker** → the comment card must NOT open. Plain clicks on both still work (2.4b, 17.x). (TDD 2.24)
- [ ] **2.22** Hover a link whose caption differs from its URL → tooltip shows the **URL**; hover ordinary body text → no tooltip; hover a broken/blocked image placeholder → its own tooltip still appears (the view-level link tooltip must not suppress child tooltips)
- [ ] **2.17** Open `anchors.md`, click a same-doc fragment link → scrolls to heading (including one far below the current viewport, e.g. "Jump to Deep"); verify duplicate-slug disambiguation (`slug-1`, `slug-2`)
- [ ] **2.7** Open `unsafe-local-image.md` and `html-injection.md` → out-of-folder image blocked (`image-missing`), embedded HTML/script does not execute or read outside the doc folder
- [ ] **2.8** Select prose + heading + code + bold + link, Ctrl+C, paste into a plain-text editor → clipboard is Markdown source, not stripped plain text
- [ ] **2.8a** Drag-select *only* four letters inside a heading → Ctrl+C → paste = those letters, **no `#`**. Select letters inside a **bold** word → paste has **no `**`**. Select part of a link caption → paste = caption fragment only, **no `[](url)`**
- [ ] **2.8b** Select from inside `~~strike out~~` through a word after it (e.g. the `k` of "strike" through "outs" of "outside") → paste = `~~ke out~~ outs` (balanced, artificially completed). Select from before a link through part of its caption → paste = `[frag](url)` with the whole URL
- [ ] **2.8c** Select-All in the preview → Ctrl+C → paste equals the whole document source, identical to Copy Document
- [ ] **2.8d** Select a run of prose spanning two paragraphs → paste = exactly that prose with a blank line between them, no extra surrounding block
- [ ] **2.8e** Partial-select over an image / a table / a list item / a multi-line blockquote → paste is well-formed Markdown (the whole image/table source, the char-precise item/quote text, never a half-token); repeat each copy **after** a mutation (type a bold phrase then copy within it; insert a table then copy across it; cut then copy; reload then copy) → still correct
- [ ] **2.8f** In a table cell containing formatting (e.g. `**bold** text`, `` a `code` b ``, `see [site](url) here`), select the whole cell (triple-click) → Ctrl+C → paste is the cell's **Markdown** with formatting preserved (`**bold**`, backticks, `[site](url)`), not stripped plain text. Then select just the bold word inside the cell → paste is the bare word, no `**` (char-precise like body text)
- [ ] **2.8g** Multi-line blockquote (`> a` / `> b`) and a list (bulleted, ordered `1.`, and task `- [ ]`): select text *within* one quote line or one list item → paste is the bare text, **no** `>` / `-` / `1.` / `[ ]` marker (continuation `> ` also excluded). Then Select All → paste keeps every marker — each quote line its `> `, each item its exact source marker with ordered numbers and task-box state. A list item that contains a **nested** list is char-precise too: select within the nested item → bare text, no marker; select from the outer item's text into the nested one → the nested marker returns with its indent lead-in
- [ ] **2.8h** In a **code block** of several lines (`code-blocks.md`, or any fenced block): drag-select two words on ONE line → Ctrl+C → paste is **exactly those two words** — no ```` ``` ````, no other line of the block. This is the reported bug: the whole block used to come out. Then (a) select two whole lines → paste is those two lines only; (b) select from the paragraph ABOVE the block into the middle of a code line → paste ends with a closing ```` ``` **on its own line** ```` (paste it into a new document and confirm it renders as one complete code block, not as code swallowing the rest); (c) same from inside the block outward into the paragraph below → both fences present; (d) Select-All → still byte-identical to Copy Document. Repeat (a) for a code block **inside a blockquote** (no `> ` in the paste) and an **indented** 4-space block (the continuation indent survives, so the paste re-parses as a code block). Finally, with a partial code-block selection, **Annotate** it → the annotation still wraps the **whole** block (a `{==` inside a fence is a FAIL) (TDD 2.8h)
- [ ] **2.18** Drag-select over an inline image → semi-transparent selection tint over the image; selection drag still passes through
- [ ] **2.19** Switch to edit/split under dark theme → editor background/syntax match dark scheme; toggle theme live → editor updates alongside preview
- [ ] **2.15m** **macOS: the desktop light/dark toggle.** Two halves — check them separately, because the first can pass while the second fails and that is exactly how this was nearly shipped (ScrAP-184). (a) *Detection*: run with `RUST_LOG=scribobulate=debug`, then flip the system appearance (System Settings ▸ Appearance, or `osascript -e 'tell application "System Events" to tell appearance preferences to set dark mode to true'`) → the log prints `macOS appearance: switching to dark` **at the moment of the flip**, and `... to light` on the way back. A line that arrives many seconds late, or reports the *opposite* of what you just set, is ScrAP-185 (a deferred read) and not a detection failure. Also relaunch with dark already active → the line appears during startup. (b) *Painting*: **look at the window** — chrome, tab strip, outline sidebar and page must all follow, live, without a restart. This is the half that is not implied by (a): writing only the legacy `gtk-application-prefer-dark-theme` moved every intermediate value correctly and still painted light, which is why GTK 4.20's `gtk-interface-color-scheme` is set too (ScrAP-184). The pixel assertion in `cargo test --features gtk-integration-tests --test macos_dark_mode` covers a rendered window; **this item exists for the one thing that check cannot see — an already-open window repainting on a live flip.** Do not mark it green on (a) alone.
- [ ] **2.20** Open `line-breaks.md` → single-newline-separated lines each render on their own line (hard-wrap model); a blank line still starts a separated paragraph

### §3 Live reload
- [ ] **3.1** Open a file with no unsaved edits; `echo "new line" >> file.md` from a shell → preview/editor update automatically
- [ ] **3.2** Scroll partway through a long doc, then external-edit the file → reading position stays approximately put
- [ ] **3.2a** **Huge-doc far-scroll — LOAD-BEARING area-2 regression guard.** The snap-to-top settling is a genuinely GUI-only path (a fresh preview's line heights are still *draft* at restore time — a condition that can't be staged headlessly, see `preview/scroll.rs`'s gtk-test module doc), so per POLICY §"Manual integration testing" THIS live check is the regression guard, not an automated test. Open a very large document (~40k lines, e.g. a generated fixture), scroll FAR down (thousands of lines, well past the point one-shot validation reaches), then external-edit the file (append/prepend a line via a shell) so the whole preview widget is rebuilt fresh → the reading position is **restored down near where it was**, NOT snapped to the very top. Watch the console: no "snapshot … without a current allocation" spam, no input wedge afterward (wheel/PageDown still scroll). (ScrAP-65; progressive `notify::upper` restore)
- [ ] **3.3** Run the external-writer script (§2 fixtures) against the open file → updates coalesce, window stays responsive, no flicker storm
- [ ] **3.4** `rm` the open file externally → user is informed, editor retains current content (test then File ▸ Save recreates it)

### §4 Editing & saving
- [ ] **4.1** Type in the editor pane (edit or split) → preview updates promptly
- [ ] **4.1a** **Type into split mode while the first render is still validating** (the fatal-crash guard; use a document big enough that entering split visibly takes a moment — `tests/fixtures/large-doc.md`, 41,785 lines): open it, press **Alt+2**, and start typing about **3 s later, while the preview is still settling** (do not wait for it to finish — waiting is what hides this). The process must stay alive, the preview must catch up to the typed text, and the panes must stay line-aligned. Repeat with the typing started ~1 s in. Any death here — a silent disappearance, `SIGSEGV`, or `gtk_text_btree_line_number couldn't find line` in the console — is this defect returning: a re-render that hands the view a NEW `GtkTextBuffer` instead of rebuilding the live one strands the layout's cached line displays on the freed buffer (ScrAP-258). Watch the console; the crash also leaves a report in `$XDG_STATE_HOME/scribobulate/`
- [ ] **4.2** Type headings/emphasis/code/links in the editor → Markdown syntax highlighting applied
- [ ] **4.3** Edit, Ctrl+S → on-disk file matches editor content (`cat` it)
- [ ] **4.4** Edit → unsaved indicator appears; save → clears
- [ ] **4.5** Open a nonexistent path (1.3), type content, save → file created on disk with that content
- [ ] **4.6** Save with unsaved edits present → editor keeps exact content/cursor/focus, no flicker/reload (this is the file-watcher round-trip guard — watch closely for a visible reload flash); also confirm NO "File deleted on disk" toast/status appears (the atomic save's own rename must not be misread as an external deletion)
- [ ] **4.7** New/untitled doc, type content, Ctrl+S → Save As chooser appears; save → title/Copy Full Path/Reload activate, doc reads clean. Then Save As a titled doc to a different path → content written there, watcher now follows the new path (verify by external-editing the OLD path — no reaction — then the NEW path — reacts). Also: Save As a document whose chosen filename already ends in `.md` → saved file is named `name.md`, never `name.md.md`. Also: immediately after a Save As, externally `rm` the new file → "File deleted on disk" notice still appears, not silently swallowed (QA round-1 M1, a gap in KK's guard)
- [ ] **4.7-title** **Save As titles the window like every other path.** Read the title bar (or `Get-Process scribobulate | Select MainWindowTitle` on Windows — cheaper and unambiguous) at three moments. (a) One-tab window, Save As to `saved-as.md` → the title reads **`saved-as.md — Scribobulate`**; a bare `saved-as.md` with the suffix missing is the FAIL. (b) Open a second document in the same window (title now reads `<active>.md (+1 document) — Scribobulate`), then Save As the active one → it reads `saved-as.md (+1 document) — Scribobulate`; the filename updates but the `(+1 document)` count must NOT be dropped. **This sub-item's expectation was INVERTED by TDD 15.7's rewrite** — it used to require that the title *not* show the chosen filename (the title was a bare count then), and showing it is now mandatory; what catches a Save As that has grown its own derivation is the **count going missing**, no longer the filename appearing. (c) The tab's own strip label and its View ▸ Documents entry both show the new filename immediately, without a tab switch. All three come from one choke point, so a failure in any of them means Save As has grown its own derivation again (TDD 4.7 / 15.7; Derived-view CAM row 4, column B).

- [ ] **4.10** **A second Save while one is in flight is dropped, not raced.** *Automated* — `a_second_save_while_one_is_in_flight_is_dropped_not_raced` pins it deterministically via `docio::slow_io`, and a hand run needs a write-delaying mount this repository does not currently have (see `scripts/slowfs.py`). Run by hand only if one is available: Edit a document on the slow mount, press Ctrl+S, then press Ctrl+S again immediately → run with `RUST_LOG=scribobulate=warn` and look for `a save of … is already in flight; dropping this request`. The document must stay marked "Unsaved changes" until the first write lands, and one more Ctrl+S afterwards must write the newest text. Exactly one file version reaches disk — never an older buffer overwriting a newer one (TDD 4.10).
- [ ] **4.11** **A save writes the document it was invoked for, not the one you switch to.** Open two documents as tabs, at least one on the slow mount. Make the slow one dirty, press Ctrl+S, then immediately switch to the other tab. When the write lands: the slow document's file has the edited content, its tab's dirty marker clears, and its swap file under `$XDG_STATE_HOME/scribobulate/swap/` is **gone**; the other tab is untouched. A snapshot left behind is the specific regression here — the next launch would offer already-saved work back as "unsaved" (TDD 4.11; ScrAP-244).
- [ ] **4.12** **Save All.** Open three tabs: two titled and dirty, one clean. File ▸ Save All (or toolbar / Ctrl+Alt+S) → both dirty files match the editor; both dirty markers clear; the clean tab is untouched. With only clean tabs, Save All is **disabled**. With the **active** tab clean but a **background** tab dirty, Save All is still **enabled** and saves the background one. Add an untitled dirty tab and invoke Save All → a Save As chooser appears for it after the titled ones; cancelling that chooser leaves it dirty but does not undo the titled saves already done. After the batch, the tab that was active when you invoked Save All is active again (TDD 4.12).

### §5 Reconciliation
- [ ] **5.1** Make unsaved edits, then external-edit the file → conflict notification, offers reload-or-keep, local edits untouched either way
- [ ] **5.2** Unsaved edits + externally-changed file, then Ctrl+S → warned before overwrite
- [ ] **5.3** From a 5.1 conflict, choose reload → editor+preview show on-disk content, unsaved indicator clears
- [ ] **5.4** No unsaved edits, Auto-Reload on, external edit → silent reload + "File reloaded from disk." toast (~2.5s); re-edit while toast visible → timer resets, no second toast stacks
- [ ] **5.7** **A reload in flight when a save lands must not revert the document (Deferred-operation CAM 2/B).** Needs the slow mount from 1.4c. Open a document from it, type an edit, then File ▸ Reload and — without waiting — Ctrl+S. Whichever way the two resolve, the end state must be self-consistent: the buffer, the on-disk file and the dirty indicator agree. The specific failure to watch for is a tab showing **no** unsaved-changes marker while `cat` of the file differs from what is on screen — that is the reload having applied pre-save content *and* recorded it as clean, and the next save then puts the stale version back over the good one. Also check `RUST_LOG=scribobulate=info` for `discarding a superseded reload read` / `the save guard's read was superseded` — either line means the guard fired, which is the intended outcome, not a fault (TDD 5.6; ScrAP-244).
- [ ] **5.5** **A document that stops being admissible is refused on re-read.** Open an ordinary `.md` file, then externally replace it with something inadmissible while it stays open — simplest is `rm file.md && mkfifo file.md`. Then: File ▸ Reload → an error dialog naming the refusal, and the app does **not** hang (a hang here is the exact failure the check exists to prevent). Ctrl+S with an edit pending → the "Could not verify the file on disk … Overwrite anyway?" prompt rather than a silent write. Leave it alone → the watcher does nothing and no dialog appears unprompted. Restore with `rm file.md && echo hi > file.md` (TDD 5.5).
- [ ] **4.9** Edit, then Ctrl+S → **"File saved."** toast (~2.5s, save icon) + "File saved" in the status bar; toast auto-dismisses. Save As to a new path → same notice. Save immediately after an external reload → the notice **retargets** to "File saved." rather than stacking a second box over "File reloaded from disk." (one shared widget)

### §6 Resource footprint (go/no-go gate)
- [ ] **6.1** Per §1.8 above: `nvidia-smi` shows **no GPU client** for the process; VRAM < 50 MiB
- [ ] **6.2** Confirm no GL/GLES context held (no compositing pipeline) — check `GSK_RENDERER` effective value / absence of GL errors in log
- [ ] **6.3** Sum RSS across all app processes; run several live-reload cycles (§3.1 repeated ~20x) and re-measure — must not climb unbounded
- [ ] **6.4** *(Windows only)* Measure dedicated GPU memory and GPU engine utilisation for the PID (`Get-Counter "\GPU Process Memory(*)\Dedicated Usage"` and `"\GPU Engine(*)\Utilization Percentage"`, matched on `pid_<PID>_`). A non-zero reading is **expected and not a failure** — Windows composites every window through the GPU. What must hold: the figure stays far under 50 MiB, engine utilisation stays ~0%, and — the actual gate — the figure **does not grow** when the window is maximised (try ~9x the area) or when a much larger document is opened at a fixed window size, while RSS *does* grow with the document. Growth in either dimension means software compositing is not active (TDD 6.4)

### §7 Window & layout
- [ ] **7.0** *(Windows only, release build)* Launch the **installed** app from its Start-menu shortcut or by double-clicking a `.md` file — not from a terminal. **No console/CMD window may appear**, either in front or behind the app window, and none may show in the taskbar. Regression shape to watch for: a console-subsystem build allocates a console that owns the process, so closing that window kills the app with no save prompt. Verify with `Get-CimInstance Win32_Process -Filter "ParentProcessId = <PID>"` → no `conhost.exe`. A **debug** build is expected to keep its console (that is deliberate, so `RUST_LOG` still reaches the terminal during development) — test this against a release build only
- [ ] **7.0b** *(Windows only)* The icon at the far left of the title bar must be **Scribobulate's own** — the purple rounded square with the silver robot head and the orange Markdown badge — and the **same icon must appear in the taskbar button and in Alt+Tab**. A generic/unfamiliar glyph in any of the three means `com.extollit.scribobulate` stopped resolving: the name is served by the GResource bundled from `data/icons/scalable/apps/`, and Windows has no filesystem icon-theme install to fall back on, so a dropped `<file>` entry in `data/resources.gresource.xml` silently reverts it to GTK's default. This fails **silently** — no broken-image placeholder, unlike every other icon — which is why it needs its own check. (Traces TDD 7.0b; `icons::gtk_integration_tests` covers name resolution, this covers what Win32 actually draws.) This check covers only what **GTK** draws in a live window — the icon Explorer and the shell show is a separate channel with its own failure mode; see **7.0e**
- [ ] **7.0c** *(Windows only)* With the reading theme on **System**: put Windows in dark mode (Settings ▸ Personalization ▸ Colors ▸ "Choose your default app mode" → Dark) with the app **already running**. Within ~2s the whole app goes dark — toolbar, editor, sidebar — **and the title bar goes dark with it**, no restart. Switch back to Light → both revert. Then relaunch under each mode and confirm the window *opens* in the right one, with **no light flash** before it settles. Two failure shapes to tell apart: app dark but caption still light means the DWM call is not landing (`src/platform/win32/frame.rs` `sync_caption_theme`); nothing changing at all means detection is not landing (`track_system_dark_mode` — GTK itself never reads the Windows setting, so it is our poll or nothing). Note the caption follows the **desktop**, not the reading theme: selecting **Sepia** must leave the title bar dark on a dark desktop (TDD 18.7). (Traces TDD 7.0c.)
- [ ] **7.0a** *(Windows only)* The window must wear a **native Win11 frame**, not a GTK-drawn one: app icon at the far left, left-aligned title in the system font, native `‒ □ ✕` at the right. Then check the four things the native frame buys, each of which fails silently under CSD — **drag every edge and corner to resize**; **hover the maximize button → the Snap Layouts flyout appears**; **Alt+Space → the system menu opens**; **double-click the caption → maximize/restore**. If the title is centred and bold, CSD has come back — the usual cause is someone adding a `GtkHeaderBar` or `set_titlebar()` call, which silently defeats `GTK_CSD=0` (POLICY § Architecture rules)
- [ ] **7.0d** *(Windows only)* Maximize the window with the **title bar's own maximize button** — not by any command inside the app, and not by dragging to the top edge; the OS button is the whole point of the check. Then open, one at a time: a **menu-bar menu** (File), the **toolbar's reading-theme dropdown**, and a **right-click context menu** in the document. After each, the window must still fill the screen — while the menu is open *and* after Escape closes it. The failure is unmistakable once you know it: the window snaps back to whatever size it had before you maximized, the title bar still shows the *restore* glyph (Windows still thinks it is maximized), and the screen keeps a band of stale pixels where the window used to be. Finally click **restore** → the window must land on exactly the size and position it had before being maximized, not on the maximized one. Repeat the whole sequence in a **non-maximized** window as the control: nothing may move. Root cause if it returns is in `src/platform/win32/frame.rs` `track_maximized_size` — GDK-Win32 falls back to GTK's *remembered* size for any layout pass a popover triggers, and only the native frame puts us on that path (TDD 7.0d; **GTK4Rs/AP-158**, which also records why the obvious `gtk_window_maximize()` fix is wrong; `remembered_size_while_maximized` unit tests cover the rule, this covers the running window)
- [ ] **7.0e** *(Windows only, release build)* Check the four places the **shell** names the app, not the app itself. Right-click any file ▸ **Open with** → the entry reads **Scribobulate** with the app's own purple icon — **not** `scribobulate.exe`, and not a plain name with a generic icon. Then: **Settings ▸ Apps ▸ Default apps** → search "Scribobulate" and confirm the same name and icon; **Task Manager ▸ Details** with the app running → its row's description column reads `Scribobulate`; and right-click `bin\scribobulate.exe` ▸ **Properties ▸ Details** → File description `Scribobulate`, Product name `Scribobulate`, File version `0.1.0`. Repeat the Properties check on a freshly built `target\release\scribobulate.exe` — it must pass **before** any installer runs, because it is the binary and not the installer that carries this. **This is a different channel from 7.0b and passing one proves nothing about the other**: 7.0b is the GResource icon GTK draws inside a live window, whereas Explorer never asks the running process — it reads the file on disk unlaunched, so only the Win32 resource section `build.rs` embeds reaches it. Regression shape: a dropped `winresource` call leaves a perfectly working exe, and the shell silently falls back to icon-none plus the raw file name. **The caching trap, which will make a correct fix look broken.** The icon and the name are cached separately and only one of them self-heals. The icon cache keys on the file, so reinstalling refreshes it; the *name* is cached in `MuiCache` (`HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache`) keyed by **file path**, and nothing invalidates it — not a rebuild, not a reinstall, not restarting Explorer, not a reboot. The observed symptom is precisely "the icon came right and the name did not". Confirm the binary is innocent before chasing it: copy the exe to a path Explorer has never seen and query that copy — if the fresh path answers `Scribobulate`, the resource section is correct and only the cache is wrong. To clear it, delete the `<exe path>.FriendlyAppName` value (an `.ApplicationCompany` sibling may exist too) and restart `explorer.exe`; Windows repopulates both from the file. Note a stale entry masks the registry `FriendlyAppName` just as thoroughly as it masks `FileDescription`, so adding the former is not a workaround for it (TDD 7.0e)
- [ ] **7.1** Switch editor-only / preview-only / split → all three reachable
- [ ] **7.2** Resize window, set a layout, quit, relaunch (no CLI arg) → size/layout restored
- [ ] **7.3** In split mode: Swap Panes → editor/preview swap sides, scroll sync + zoom still work; Vertical Split → reorients top/bottom; drag the divider → panes resize (clamped so neither collapses); both controls greyed outside split; quit/relaunch → arrangement persists
- [ ] **7.4** Unsaved edits, close window → prompted save/discard/cancel; untitled doc + Save in that prompt → Save As chooser, window closes on success; pristine blank window closes with no prompt; multi-tab window with 2+ dirty tabs → prompted once per dirty tab sequentially, Cancel at any point aborts leaving all tabs intact
- [ ] **7.4-caption** **Every modal confirmation is captioned.** Raise each of the three in turn — the close prompt (dirty document, close the window), the overwrite warning (dirty document, external-edit its file, Ctrl+S), and the save-error report (make the file read-only, then Ctrl+S) — and read the **window caption**, not the text inside the dialog. Each must read **Scribobulate**. On Windows this is where it fails: an untitled GTK dialog is rendered by GDK-Win32 as a lone **`.`** beside the app icon and in the taskbar (`gdk_win32_surface_set_title` refuses an empty caption). Cheapest assertion: `Get-Process scribobulate | Select-Object MainWindowTitle` while the dialog is up, or `[void][Win]::GetWindowTextW(...)` on its HWND. On a GNOME session the same fix shows as a centred "Scribobulate" in the dialog's header bar where there was previously an empty strip — expected, not a regression (TDD 7.4).
- [ ] **7.5** Scroll partway, switch view mode → position holds. In split with uneven blocks (headings/code/tables): scroll either pane → the other stays **line-accurately** aligned (same heading/line at the top of both, not just the same fraction); type → alignment holds, no blank. In edit/split the outline highlights the **caret's** heading, live as the caret moves (now line-accurate map sync)
- [ ] **7.6** Two windows open; drag a tab from one onto the other's tab strip → tab (content+undo+view mode) moves, disappears from source. The former Shift-gate is retired (window/tabwidget.rs's fully-owned strip has no `GtkNotebook` reorder gesture to race, so GTK4Rs/AP-60 no longer applies): a plain drag now escalates to a cross-window move based purely on where it is dropped, while a drop back onto its OWN strip reorders in-window instead (see 15.6)
- [ ] **7.7** Single-tab window; drag that tab elsewhere → source window closes itself automatically; with only one tab, "Move Tab to New Window" is disabled in menu/toolbar/accel
- [ ] **7.8** Two windows, one with a single tab; drag a tab from the other toward it → strip is visible and accepts the drop; with only one window open and a single doc, its strip is ALSO visible (no longer hides at one tab)
- [ ] **7.9** Drag a tab over another window's strip → strip glows while hovered, stops when it leaves
- [ ] **7.9a** Drag a tab and watch what follows the pointer (TDD 7.9a) → it is **an image of the tab itself**: its label legible, its `×` visible, correctly framed within the little window, at full strength. **Do not accept "something is following the cursor"** — an unpainted drag surface does that too, and shows stale screen content instead of the tab, which reads as a misaligned slice of whatever was behind it. If driving this synthetically, capture the drag surface (`xdotool search --pid <PID>` → the small mapped toplevel with no `_NET_WM_WINDOW_TYPE`) and compare its contents before/after a change rather than judging one capture in isolation
- [ ] **7.10** (X11 only — the desktop-detach signal fires on no other backend; GTK4Rs/AP-49) Drag a tab off its strip onto empty desktop → new window opens containing it
- [ ] **7.11** Two+ tabs with one NOT active; hover that background tab → its `×` close button reveals; click the `×` → that specific tab closes (prompting to save first if it is dirty) WITHOUT ever making it active — whichever tab was already active stays active throughout (unless the tab you closed WAS the active one, in which case a neighbor becomes active exactly as Ctrl+W does). Single-tab window → clicking its `×` closes the whole window (TDD 7.11)
- [ ] **7.12** Right-click any tab → context menu with Close Tab / Close Other Tabs / Move to New Window, each acting on THAT clicked tab (not necessarily the active one); Close Other Tabs is disabled when it is the window's only tab, and Move to New Window is disabled under the same single-tab condition View ▸ Move Tab to New Window already uses (TDD 7.12)
- [ ] **7.12a** Tab context-menu access keys (TDD 7.13, ScrAP-70): each row shows one letter underlined (**C**lose Tab, Close **O**ther Tabs, **M**ove to New Window); with the menu open, pressing that bare letter (no Alt) invokes the row — `c` closes the clicked tab, `o` closes the others, `m` moves it. `C`/`M` match File ▸ Close Tab and View ▸ Move Tab to New Window. A disabled row (e.g. Close Other Tabs on a single-tab window) ignores its key.
- [ ] **7.12b** Open 3+ files (`scribobulate -n a.md b.md c.md`), edit two of them so ≥2 OTHER tabs are dirty, then right-click a third (clean) tab → Close Other Tabs. Exactly ONE "Save changes before closing this tab?" dialog appears at a time — the strip switches to the tab it's about — never several stacked at once (TDD 7.14). Discard advances to the next dirty tab; Cancel aborts the batch, leaving the remaining dirty tabs open and returning focus to the right-clicked tab. Clean others close with no prompt.
- [ ] **7.13** Repeatedly cycle view modes (Preview↔Edit↔Split, 10+ round-trips) AND toggle Swap Panes / Vertical Split several times each **while watching the console** → editor keeps full content in every mode (not just the top 1-2 lines), and ZERO `g_object_unref: G_IS_OBJECT` / `Gtk-CRITICAL` output (guards against a use-after-free regression — the reused editor must never be reparented; ScrAP-58/GTK4Rs/AP-105). Benign `GtkGizmo (slider) reported min -2` scrollbar warnings are expected and unrelated
- [ ] **7.14** Open two files at once (`scribobulate -n a.md b.md`) and WITHOUT switching tabs first, Move Tab to New Window (or drag) the initial (first, still-active) tab → the SOURCE window switches to the surviving neighbor: its content pane AND outline show the neighbor doc, not a blank pane with the moved tab's stale outline (TDD 15.17, ScrAP-62). Same for closing that first tab (Ctrl+W). Also: from that fresh first tab, Next/Previous Tab (Ctrl+PageDown/Up) navigates rather than no-op'ing (TDD 15.5)

- [ ] **7.15** **Reading position survives horizontal resize — GUI-only regression guard (Reading-Position Preservation CAM row 7, TDD 7.15, GTK4Rs/AP-153).** This is a live check by necessity: the drift is CUMULATIVE across an interactive drag and reproduces only with a real frame-edge drag — a single one-shot `windowsize` barely moves it, and a headless `#[gtk::test]` that pumps to full allocation settles the very validation race and yields a false PASS (GTK4Rs/AP-78). Open a long doc (e.g. a ~600-line generated fixture of numbered `## Section NNN` headings) in Preview or Split mode; start the window WIDE (~2400px), scroll to a mid-document heading and note its number. **Drag the right edge inward** to narrow the window a LOT — a repeated incremental drag, not a single jump → the noted heading stays at the top of the viewport; it does NOT creep upward toward Section 001. Diagnostic tell: **narrowing** is the drifting direction (widening holds on its own), so test narrowing. Then, with an external edit fired DURING/right after a resize (append a line via shell), the reload re-anchors to that same heading, not to a near-top position (GTK4Rs/AP-153 reload hardening). Console: no "snapshot … without a current allocation" spam, no input wedge afterward (wheel/PageDown still scroll).

- [ ] **7.17** **Installed app carries its own icon (TDD 7.17).** Install for the platform first (platform procedure: §A, *Install*) — the rubric does not apply to an uninstalled run. Then check the app's own icon wherever the platform shows one (title bar, taskbar/Dock, task switcher) (an **absolute** path — a bundle does not inherit the shell's working directory, and a relative one silently opens a blank document) and check the Dock and Cmd-Tab. Each surface shows the robot icon, never a generic placeholder ("exec" on macOS). Also open **Help ▸ About** → the dialog's logo is the app icon, not a broken-image placeholder; that surface comes from the GResource rather than the OS packaging, so it is the half that must hold even uninstalled. Machine-checkable half on macOS: `lsappinfo info -only bundleid,name -app "$(pgrep -f Scribobulate.app | head -1)"` reports `com.extollit.scribobulate` / `Scribobulate` rather than a null identifier and a lowercase name

### §8 Single-instance lifecycle
- [ ] **8.1** App running, launch again (no `-n`) with a different file → opens in a new window of the SAME process — one PID (platform procedure: §A, *Launch & instance identity*)
- [ ] **8.2** Open a file already open in a background tab; reopen via a **tokened** launch (platform procedure: §A, *Tokened launch*) → that window focuses, tab activates. Repeat via a **bare terminal** launch → focus-steal-prevention may substitute a taskbar flash instead (expected desktop behavior, not a bug — TDD 8.2 note)
- [ ] **8.2m** **macOS: run 8.1 and 8.2 on BOTH launch paths — they use different mechanisms and one passing says nothing about the other** (platform procedure: §A.2, *Launch & instance identity*, which describes both). (a) *Terminal path*: `./target/release/scribobulate <file>` twice → handled by `src/platform/mac/single_instance.rs` (`RUST_LOG=info` prints `single-instance: primary, listening on …` for the first and `handed N argument(s)` for the second). (b) *LaunchServices path*: `open -a target/macos/Scribobulate.app <file>` against a running bundled instance → handled by macOS itself, which reuses the app **without exec'ing a second process at all**, GTK's Quartz backend delivering the document to the running `open` handler. Cross them too: bundle first, then forward from a terminal. Confirm one PID throughout with `pgrep -x scribobulate` (**not** `pgrep -f`, which also matches your own shell command line and reports phantom processes)
- [ ] **8.2s** **Windows: re-run 8.1 and 8.2 against the STAGED tree, not `target\release\` — the dev tree cannot fail this test and the shipped installer can** (ScrAP-249). Single-instance on Windows rides a D-Bus session bus that GLib autolaunches by spawning `gdbus.exe` from beside the loaded GLib DLL; in `target\release\` that helper is reachable because gvsbuild's `bin` is on `PATH`, so the dev tree passes whether or not the redistributable ships it. Stage with `packaging\windows\stage.ps1`, then from a shell whose `PATH` has **no** gvsbuild entry (`$env:PATH -like '*gtk-build*'` must be `False`), launch `<stage>\bin\scribobulate.exe <file>` twice and count with `Get-Process scribobulate`. **One** process is a pass; two windows on one document is the defect. Cross-check the mechanism rather than only the outcome: `Get-Process gdbus` must show a daemon running from the staged `bin\` while the app is up (it exits on its own when the last client disconnects, so it never blocks an uninstall). The **default-app route is the one users actually take** — associate `.md` during install and double-click a second file in Explorer
- [ ] **8.3** Multiple windows open, close one → others + process stay alive; close the last → process exits (confirm by counting instances — platform procedure: §A, *Launch & instance identity*)
- [ ] **8.4** Open several docs as several windows, measure RAM/VRAM → grows modestly per window, not a full baseline each
- [ ] **8.5** App running, launch with `-n` → brand-new independent process (different PID), file opens there not in the original
- [ ] **8.6** Keeping one window open, repeatedly open a second window (Ctrl+N) and close it again (Ctrl+W) ~10–20 times, then sum RSS across the app's processes → it returns to (does not climb unboundedly above) the one-window baseline. This is the per-window tab-UI reclamation gate (ScrAP-60: a self-owned closure that strong-captured the content-`GtkPaned` used to strand the whole tab UI — TabBar/GtkStack/SplitView/editor — on every close). No `Gtk-WARNING`/`g_object_unref` output on close either.
- [ ] **8.7** **A force-killed instance must not wedge the next launch (TDD 8.7).** With the app running, force-kill it (platform procedure: §A, *Force-kill*) — not a clean quit, the point is that no shutdown code runs, then launch it again with a file → it starts normally, becomes the new primary, and opens the file. Repeat once more to confirm the *third* launch still forwards to the second rather than spawning its own process. On macOS the abandoned state is visible and worth eyeballing: `ls -l "$TMPDIR"scribobulate-*` shows a `.sock` left behind by the killed process, and the next launch must silently take it over

### §9 Menu bar, toolbar, actions
- [ ] **9.1** Ctrl+T → new tab in current window, pre-populated with the welcome/starter template content (a starting point, NOT literally empty — same as a no-arg launch / New Window; counted as an untouched "blank" tab per TDD 9.1), nothing else closes
- [ ] **9.2** Ctrl+O, pick a file → renders in current window
- [ ] **9.3** No selection → Copy greyed in Edit menu AND context menu simultaneously; select text → enabled in both
- [ ] **9.5** Mirror of 9.3 for the toolbar Copy button specifically
- [ ] **9.4** Fenced code block with a blank line inside → uniform 4-side padding, unbroken background, distinct from page bg in both themes
- [ ] **9.6** Save tracks unsaved-changes, NOT view mode (TDD 4.8): clean doc → Save greyed in every mode (toolbar + menu), including edit/split; make an edit → Save enables in every mode; switch that dirty doc to **preview-only** → Save stays ENABLED and File ▸ Save / the toolbar Save button writes to disk (`cat` the file to confirm), then clears dirty. Save As stays enabled in every mode regardless of dirty state
- [ ] **9.7** Cycle view mode from toolbar AND from View menu → both surfaces always agree on the active mode
- [ ] **9.8** File opened from disk, File ▸ Copy Full Path → clipboard has absolute path incl. filename
- [ ] **9.9** File open → Copy Full Path enabled in menu + toolbar
- [ ] **9.10** Blank welcome window → Copy Full Path disabled in both
- [ ] **9.11** Preview mode → Cut/Delete disabled everywhere; edit/split + selection → enabled everywhere, operates correctly; edit/split no selection → disabled again
- [ ] **9.12** File ▸ Reload with unsaved edits → confirm-discard prompt, then reverts to on-disk; blank window → Reload disabled; `chmod 000` the backing file then Reload → error dialog naming the failure, not a silent no-op (QA round-1 M2)
- [ ] **9.13** Auto-Reload on (default) → live reload occurs; toggle off, external-edit → nothing happens; toggle back on → catches up to latest on-disk state
- [ ] **9.14** Edit menu in edit/split → Insert Emoji + Change Case (UPPER/lower/Title/tOGGLE) present and functional; preview mode → both disabled
- [ ] **9.15** Select text inside a preview table cell → Copy enabled everywhere, copies cell text; clear selection → disabled again
- [ ] **9.16** Make an edit, Undo → reverts, Redo enables; Redo → reapplies; nothing left to undo/redo → disabled; preview mode → both disabled; fresh/reloaded doc → Undo disabled (reload isn't undoable)
- [ ] **9.18** Help ▸ About (F1 is the keyboard-shortcuts window, §16.1 — About has no accelerator) → modal with name/version/description/copyright/website link; License button shows Apache-2.0 text; System tab shows GTK + crate versions. The description names **no platform** (no "Linux", "macOS" or "Windows" — the same copy ships everywhere and must not go stale as platforms are added). The link reads **Scribobulate on GitHub**; click it → the system browser opens the project's GitHub page and the app keeps running (same launch path as §16.6)
- [ ] **9.18a** In About, open the **Credits** tab → a "Bundled open-source components" section lists the syntax-highlighting-grammar attribution (via two-face; MIT/Apache-2.0/BSD; points to THIRD-PARTY-LICENSES.md). Entries are plain text, NOT rendered as clickable `mailto:` links (GTK4Rs/AP-50 guard). The `THIRD-PARTY-LICENSES.md` file is present at the distribution root. (Third-party attribution obligation from bundling the `two-face` syntect grammars — ScrAP-160.)
- [ ] **9.17** Toggle View ▸ Toolbar off → hides, checkbox updates; still reachable via menu bar; toggle Status Bar too; quit/relaunch → hidden state persists
- [ ] **9.19** Edit ▸ Copy Document with a partial selection active → clipboard gets the WHOLE doc source, any view mode
- [ ] **9.20** Ctrl+G in edit/split with editor focus → dialog pre-filled with current line; confirm → caret jumps, scrolls into view; out-of-range clamps; disabled in preview or when editor unfocused
- [ ] **9.21** Move caret (type/arrows/click/Go To Line) → footer shows "Ln L, Col C" for the ACTIVE tab regardless of literal pane focus; preview-only → indicator hidden; switch tabs → immediately reflects new tab, never stale
- [ ] **9.22a** View ▸ Toolbar is a submenu: **Show** first, separator, then six section checkboxes (File/Edit/Format/View/Split/Zoom) each ticked to match current visibility
- [ ] **9.22b** Untick one section (e.g. Zoom) → exactly that group **and its leading separator** vanish; untick a section adjacent to another already-hidden one → still exactly ONE separator between each visible pair, never a doubled or orphaned rule; the bar begins with one left-edge separator (the accepted `file` quirk)
- [ ] **9.22c** Order preserved (I7): hide File, hide Zoom, show File, hide Edit, show Zoom, show Edit → visible sections stay in canonical File/Edit/Format/View/Split/Zoom order; a re-shown section returns to its original slot, never the end
- [ ] **9.22d** Untick **Show** (whole bar) → all six section checkboxes go disabled/greyed **but keep their ticks**; re-tick Show → the exact prior per-section configuration returns (round-trip: set e.g. Edit+Split hidden, hide whole bar, restore → Edit+Split still hidden, rest shown)
- [ ] **9.22e** Command sensitivity untouched (I6): with view mode + editor focus held fixed, note each toolbar button's greyed/live state (use genuinely-gated ones — Zoom at a ladder end, Split controls outside split mode, Save off editor focus); run the 9.22d whole-bar round-trip → every button's enabled/greyed state is identical before vs after
- [ ] **9.22f** Command still live when hidden: hide the Zoom section, press Ctrl++ → preview still zooms (hidden ≠ disabled command)
- [ ] **9.22g** Min-width: all sections shown → drag window as narrow as it goes, note width; hide half the sections → the window can now be dragged narrower (content-derived min drops); show all → min returns
- [ ] **9.22h** Persistence: hide some sections (and/or the whole bar), quit, relaunch → same sections hidden, same checkboxes ticked; a fresh profile (no session file) opens with the **default short toolbar — File/Edit/View shown, Format/Split/Zoom hidden** (their checkboxes unticked), not every section
- [ ] **9.23** Menu mnemonics + in-menu access keys (TDD 9.23, GTK4Rs/AP-68): with the window focused, `Alt+F` / `Alt+E` / `Alt+R` (Format) / `Alt+V` / `Alt+H` each open that top-level menu; holding `Alt` underlines the access letter in the bar. With a menu open, the bare underlined letter activates the item — e.g. `Alt+E` then `t` = Cut, `Alt+V` then `o` = Outline toggle and `e` = Edit mode, `Alt+F` then `x` = Exit. Access letters are unique within each open menu (a duplicate would only cycle focus, not activate). The `_` marker must NOT leak into non-menu surfaces sharing the same command label: hover the toolbar Save button → tooltip reads "Save" (no literal `_`), and the toolbar tooltips show plain labels. (Deliver keys with bare `xdotool key` after `windowactivate`, never `--window`; verify each by its effect on the main window, since the open popover is an uncapturable surface.)
- [ ] **9.25** Split-mode Copy/Select All follow the focused pane (TDD 9.25, ScrAP-72): in side-by-side split, select text in the **preview** pane → Copy (Edit menu, context menu, toolbar Copy button, Ctrl+C) enables and copies the preview selection; Ctrl+A / Edit ▸ Select All selects all of the **preview** (highlight is in the preview, not the editor). Repeat with the **editor** pane focused → the same commands act on the editor. Focus the editor with no selection while the preview still visibly shows one → Copy is **disabled** (tracks the focused pane, not a fixed one). In preview-only and edit-only modes, Copy/Select All behave exactly as before. (A read-only preview copies as Markdown *source*, so its clipboard text matches the editor's — judge by the Copy button's enabled state / the on-screen selection, not the copied text.)
- [ ] **9.24** Pane context-menu access keys + Change Case submenu (TDD 9.24, ScrAP-70/GTK4Rs/AP-69): in edit/split, select text and right-click the editor → each row shows one letter underlined, matching the Edit menu (Cut = `t`, Copy = `c`, Undo = `u`, Insert Emoji = `e`, …). With the menu open, the bare letter invokes the row (`t` = Cut). "Chan**g**e Case ▸" is a real submenu: `g` (or click) **slides** to a page with a Back row + the four variants carrying the SAME letters as the menu bar's submenu (**U**PPER, **l**ower, **T**itle, t**O**GGLE via `c`); pressing e.g. `u` there uppercases the selection, and the popover dismisses. Left arrow (or Back) returns to the main page. A fresh right-click always reopens on the main page (so `t` = Cut, never Title). Disabled rows ignore their key. ZERO `g_object_unref: G_IS_OBJECT` / "Broken accounting" in the console across repeated open/slide/activate cycles (GTK4Rs/AP-30 deferred popdown).
- [ ] **9.22i** Format focus-gate survives wrapping: hide then show the Format section while editing → the Format actions still gate on editor focus exactly as before
- [ ] **9.22j** Last-section short-circuit: hide sections until only one remains, then untick that last one → the **whole bar hides** (Show unticks) instead of leaving an empty strip, and the last section's checkbox **stays ticked**; re-tick **Show** → the bar returns showing exactly that one section (lossless round-trip)
- [ ] **9.31** Show **every** toolbar section (View ▸ Toolbar ▸ all six ticked) and open the File, Edit, Format, View and Help menus in turn → every icon is a **thin monochrome line glyph in the theme foreground**, uniform in weight with its neighbours. Two in particular, because on Windows they resolve from our GResource rather than the host theme — gvsbuild's Adwaita 50.0 ships no `emblems` symbolic category at all: the **View section's "show unsafe images" toggle** must show a *framed photo* (landscape with a sun), and the **File section's Auto-Reload toggle** must show *two arrows chasing round a circle*. **The failure to look for is NOT a ⚠ broken-image placeholder** — that is what everyone expects and it is the wrong tell. When a `-symbolic` name is missing, GTK silently falls back to the same name **without** the suffix, so the button renders the legacy **full-colour** icon: Auto-Reload appeared as a *dark filled rounded-square badge with white arrows*, glaringly heavier than every icon beside it. So scan for **an icon that looks full-colour, filled, or heavier than its row**, not for a missing one. Check under a **dark** desktop too — a symbolic icon recolours to the foreground and stays legible, whereas exactly these baked-colour fallbacks do not (ScrAP-169), which is what makes dark mode the fastest way to spot one. (Traces TDD 9.31; `icons::gtk_integration_tests` covers name *resolution*, this covers what is actually drawn. Note the two are not redundant and neither implies the other: `IconTheme::has_icon` returns **false** for a name that nonetheless *draws* via that legacy fallback.)
- [ ] **9.32a** Copy Link Location, **caret half** (TDD 9.32): in edit mode put the caret inside `[caption](https://example.com/x)` → the command enables **simultaneously** in the Edit menu, the Edit toolbar section (the link icon), and the right-click context menu; invoke it from each surface in turn → `xclip -o -selection clipboard` is exactly `https://example.com/x`, never the caption or the brackets. Repeat with an image `![alt](img.png)` (caret on the `!` too — it is part of the construct), with a title (`[a](u "T")` → `u` only), and with a balanced-parenthesis URL (`[Ruby](https://en.wikipedia.org/wiki/Ruby_(gem))` → the WHOLE URL, not truncated at the first `)`). Move the caret one character past the closing `)` → disabled everywhere; into ordinary prose, a bare `[bracketed]` span, or a reference link `[text][ref]` → disabled. With the caret parked inside a link, **undo an edit that removes the link** (or let a live external reload replace the line) without touching the caret → the state still updates (the `changed` boundary, not just caret moves). Switch that tab to **preview-only** → greyed in the menu bar and toolbar, then back to edit → live again. (Read the enabled bit over `org.gtk.Actions` rather than from a screenshot — sensitivity often has no pixel signal at all, §1.4 / GTK4Rs/AP-67.)
- [ ] **9.32b** Copy Link Location, **pointer half** — the browser gesture, and the half a caret-only gate leaves permanently greyed in the preview (TDD 9.32): in **preview-only** mode (no editor caret at all), right-click **directly on a rendered link** → the row is ENABLED; choose it → the clipboard holds **that** link's URL. Do it on a *different* link on the same page → its own URL, not the first one's (the target is the one pointed at, not "some link in the document"). Right-click a link that is a **whole table cell** (a `GtkLinkButton`, ScrAP-250) → same result, no special case; then right-click a link inside a **mixed** cell (`anchors.md`'s "mixed cell" row, or `table-test.md` §2's mixed cell — a `GtkLabel`'s `<a href>`, read back through `current_uri`) → same again. A greyed row on either shape while the link is plainly clickable is GTK4Rs/AP-239's interaction half. Right-click the **plain text beside** the link in that same mixed cell → greyed (the answer is the link pointed at, not "this cell has a link somewhere"). Right-click **ordinary prose** in the preview → the row is greyed again. In **edit** mode, right-click a link in the *source* while the caret sits in a DIFFERENT link → the clipboard gets the right-clicked one (the pointer beats the caret). **Disarm check** (the half no headless test can reach): with the caret nowhere near a link and the tab in preview-only, right-click a link, press Escape to dismiss the menu without choosing anything, then open the Edit menu → Copy Link Location is greyed again; a stale-armed target would leave it live. Repeat the whole item in **split** mode, right-clicking in the preview pane.

- [ ] **9.33a** Buffer-ends keys reach the ends of a big document (TDD 9.33, ScrAP-260): open `large-doc.md` and, **without waiting for it to settle**, press **Ctrl+End** once → the viewport arrives at the very last line (not part of the way, no second press); **Ctrl+Home** once → back to the first. Repeat in the **preview** pane (preview-only mode, click prose to focus it first) → identical. Press Ctrl+End and then immediately click somewhere mid-document → the pending jump is abandoned, the caret stays where you clicked.
- [ ] **9.33b** …and they still reach them with a **table cell** focused (TDD 9.33, ScrAP-264): open `nav-keys-table.md` in preview-only mode, use the outline to jump to **The table** (so you are far from both ends), then **click into a cell** — a text caret appears inside the cell's own text, which is how you know the cell, not the pane, holds the focus. Now press **Ctrl+Home** → the document goes to the top; return to the table, click a cell, **Ctrl+End** → the document goes to the last tail paragraph. The pre-fix failure is a *silent nothing*: the viewport does not move and only the cell's own caret jumps to the start of its text, so judge this by the **document position**, never by whether the key seemed to register. Repeat with **Home**, **End**, **←**, **→**, **↑**, **↓**, **PageUp** and **PageDown** from a focused cell → each moves the document exactly as it does with prose focused. Then reach a cell whose whole content is a **link** (the third column, a `GtkLinkButton`) — **Tab to it from the text cell beside it, do not click it**, since a click follows the link — and repeat Ctrl+Home → same result (this cell shape never had the defect; it is the control that proves the fix did not trade one shape for the other, and its focus ring is faint under some themes, so judge it by the Ctrl+Home, not by looking for the ring). Finally, click into a text cell and press **Shift+End** → the *cell's own* text highlights to the end of the cell and the document does **not** move (keyboard selection inside a cell is preserved; it is the one thing the fix must not take away).
### §10 Markdown formatting commands
- [ ] **10.1** Select text, apply each of Bold/Italic/Strikethrough/Highlight/Code Span/Superscript/Subscript → correct markers wrap, selection stays; the tight `^…^`/`~…~` insertion also renders raised/lowered in the preview
- [ ] **10.2** Re-apply the same command to already-wrapped text → markup removed, not doubled
- [ ] **10.3** No selection, invoke an inline command → empty marker pair inserted, caret between them
- [ ] **10.4** Caret/selection on line(s), pick H1-H6 → correct `#` prefix applied; same tier again → removes it
- [ ] **10.5** Quote/Code Block/Horizontal Bar → correct prefix/fence/rule; Quote and Code Block toggle off on re-apply
- [ ] **10.5a** Bulleted List / Numbered List (toolbar `•`/`1.`, Format menu items, `Ctrl+Alt+U`/`Ctrl+Alt+O`): select several lines → every line gets a `- ` (bulleted) / renumbered `1. `, `2. `, `3. `… (numbered) prefix; preview renders a real list; re-apply the same command → markers stripped (toggle off), including a mixed run (`- ` and `* `, or `1.` and `2)`)
- [ ] **10.5b** List/blockquote continuation on Enter: type `1. apple`, press Enter → next line auto-starts `2. ` (increments; caret after the marker), type more and Enter → `3. `. A bulleted item (`- x` or `* x`) continues with the **same** bullet char; a `> quote` line continues with `> ` (a nested `> 1. x` continues `> 2. `). Indentation is preserved (`   1. text` → Enter → `   2. `). Enter on an **empty** item/quote (`2. ` / `- ` / `> ` with only whitespace) removes the marker and adds **no** new line ("exits"). Each Enter is a **single** undo step (one Ctrl+Z removes the whole `\n<marker>` or restores the cleared marker); redo mirrors it. A multi-line **paste** is inserted verbatim, **complete, and with no per-line markers injected** — and this half needs the specific shape that broke it (ScrAP-199), not just any paste: cut a run of list lines **at least one of which ends in an inline span** (`` - **When** the `session_token` value ``) after the file has been open long enough to be syntax-highlighted, then paste it (a) on a blank line and (b) directly after a lone `- ` line. Every line must arrive, the last one included. Repeat the (a) case with a **middle-click** paste (select the block, middle-click at the destination), which reaches the buffer by a different route than Ctrl+V.
- [ ] **10.5d** Task List (toolbar `☑` glyph button — no icon; Format menu item **Tas_k List**, mnemonic `k`; accelerator **Ctrl+Alt+C**, also listed in the Help ▸ Keyboard Shortcuts window's Format group): select several lines → every line gets a `- [ ] ` prefix (via the button or Ctrl+Alt+C); switch to Preview/Split → each item renders as a **checkbox drawn in the left gutter** (interactive — clicking it toggles the source, see 2.4b); re-apply Task List → the `- [ ] ` markers are stripped (toggle off), including a mixed run (`- [ ] ` and `* [x] `). A bare bullet (`- foo`, no checkbox) is **not** a task item, so applying Task List over it prefixes `- [ ] ` rather than toggling off. Then Enter-continuation: at the end of a `- [ ] task` (or `- [x] done`) line press Enter → the next line begins a **fresh unchecked** `- [ ] ` (never `- [x] `); a nested `> - [ ] x` continues `> - [ ] `; Enter on an empty `- [ ] ` removes the marker and adds **no** new line ("exits"). (Traces TDD 10.5 / 10.13.)
- [ ] **10.5c** Code-fence auto-close on Enter: type a bare ```` ``` ```` on its own line and press Enter → a matching closing ```` ``` ```` appears on the line below and the caret sits on the empty middle line (indentation and a longer backtick run are mirrored); one Ctrl+Z removes the whole auto-closed fence. A fence with a language (```` ```rust ````) does **not** auto-close — a normal newline. Then, with a full block open (```` ``` ````/code/```` ``` ````), put the caret at the end of the **closing** fence and press Enter → a plain newline, **no** second fence stacked (only the opening fence auto-closes, not the closing one).
- [ ] **10.20** Block commands inside a blockquote (Traces TDD 10.20): type `> Heading`, caret on it → **Heading 3** gives `> ### Heading` (marker **inside** the quote, never `### > Heading`); Heading 3 again → `> Heading`; Heading 2 on `> ### Heading` → `> ## Heading` (re-tiers, no second prefix). On `> item`: Bulleted List → `> - item`, Numbered List → `> 1. item`, Task List → `> - [ ] item`, each toggling back off to `> item` on re-apply. Code Block on `> code` → `> ``` ` / `> code` / `> ``` ` (fences stay in the quote), re-apply → back to `> code`. Nesting is preserved verbatim: `> > note` → Heading 1 → `> > # note`. In Preview/Split each result renders as a real heading / list / checkbox / code card **within** the blockquote bar. A selection spanning a quoted and an unquoted line formats each in place (no quote marker added to the unquoted one). Quote itself is unchanged: `> ` per line, one level added when a spanned line is already quoted, exactly one removed on toggle-off. Heading on a plain `- item` still gives `## - item` (a list marker is not a container).
- [ ] **10.6** Preview mode or non-editor focus → Format commands disabled; clicking a Format toolbar button doesn't itself steal focus; find-bar-with-match-selected → Format commands + overlay stay enabled; switch to preview via View **menu** → Format still disables (mode-gated, not just focus-gated)
- [ ] **10.7** Ctrl+B / Ctrl+Shift+X / Shift+F2 etc. → matches the Format menu hints
- [ ] **10.8** Select text → overlay appears centered above selection with arrow; apply from overlay → selection preserved (chain another command); Escape/scroll/selection-clear/focus-loss → overlay dismisses
- [ ] **10.8b** Move a tab (View ▸ Move Tab to New Window, or a cross-window drag) whose mode is Edit/Split into another window, then select text in that tab's editor pane in the NEW window → the caret overlay appears there (one overlay per window, re-targeted to the active editor; its driver signals resolve the host window from the editor, not a captured ref — TDD 10.12, GTK4Rs/AP-52/GTK4Rs/AP-106)
- [ ] **10.9** Heading control at rest reads `(Hn)`; menu lists H1-H6; pick a level → applies, caption resets to `(Hn)`
- [ ] **10.10** Open `superscript-subscript.md` → tight `E=mc^2^` renders raised/smaller and `H~2~O` lowered/smaller (markers removed); a multi-tilde line `H~2~O and CO~2~` renders BOTH subscripts; `~~struck~~` renders strikethrough; the nested `~~a **bold** b~~` shows the `~~` literally (expected, not a regression); prose `2^10`/`1~2`/`a^b c^d` stay literal
- [ ] **10.10a** Highlight (`==text==`, mark). Command surfaces: Format menu **Hi_ghlight** (mnemonic `g`), the toolbar/overlay **H** glyph button, accelerator **Ctrl+Alt+H** (also listed in Help ▸ Keyboard Shortcuts, Format group). Behaviour (inherits 10.1-10.3): select → wraps `==…==`; re-apply → strips (toggle); no selection → `====` with caret between. Open `highlight.md`: in Preview/Split the marked spans show a **translucent highlighter wash** and the body text stays legible on top; a `==mark==` **inside a table cell, list item, and blockquote** highlights identically (no drift between body and cell). The wash colour is **per reading theme** — cycle View ▸ Reading Theme with the doc open and confirm each recolours live (no restart): System pale-yellow, Sepia warm tan-rose, **Synthwave radioactive toxic-green**, Terminal amber-phosphor, **Candy vivid lime**. Prose `a == b` / `x -= 1` / `y == 2` / `== spaced ==` stay **literal** (no wash). Copy a preview selection spanning a highlight → the source round-trips as `==text==`. Holds at every zoom level.
- [ ] **10.11** Insert Link/Image/Table with a selection → dialog pre-fills selection into caption/alt/first-cell; **Insert Link AND Insert Image with a selection → the URL field has initial focus (type immediately, no Tab); with NO selection → the first field (Text / Alt text) has focus; Insert Table always focuses Columns**; confirm → single-undo insertion, renders correctly; Cancel/Escape → no change; Browse… opens chooser rooted at doc folder, fills relative-or-absolute path; select exactly one existing link/image then Insert Link/Image → opens as Edit with fields pre-filled, replaces (not re-wraps) on confirm, surfaces relabel "Edit Link/Image" while held

### §11 Find & replace
- [ ] **11.1** Ctrl+F → find bar slides in, search field focused; Ctrl+H → replace row also shown
- [ ] **11.2** Type a search term → matches highlighted with "N of M" (or "No matches"); clear field → count hides
- [ ] **11.3** Enter/Next and Shift+Enter/Prev → advances/retreats, wraps past either end, genuinely advances (not re-selecting current)
- [ ] **11.6** Pure-preview mode, search → matches highlight in rendered preview, Next/Prev scroll it there (not the hidden editor buffer)
- [ ] **11.7** Pure-preview mode, doc with a table: search a term appearing in **some** cells → **only** matching cells show the amber/orange highlight (on the matched substring, not the whole cell); non-matching cells stay unhighlighted. Step Next/Prev across a cell match → current turns orange, the previous reverts to amber with **no stale colour left behind**; change the term so a highlighted cell no longer matches → its highlight clears cleanly. While the find bar is still open, **select text inside a table cell** (double-click a word / drag) → the blue selection is **visible** (an opaque full-coverage base bg once painted over cell selection; now match-only + `force_cell_repaint`, GTK4Rs/AP-45/GTK4Rs/AP-92). Close the bar → all cell highlights clear in place, no scroll jump (Document Rendering CAM row 11 — interaction parity in cells)
- [ ] **11.9** **Find sees link text wherever it is rendered** (TDD 11.9; ScrAP-250; **Document Rendering CAM row 8 x row 2** — find parity in a container context, the cell this defect left unverified). Open `tests/fixtures/table-test.md` in **pure-preview** mode, Ctrl+F, search **`example`** → the count must include §3's "Links Table" cells, whose whole content is one link (`[example.com](…)`, `[example.org](…)`) — those captions are `GtkLinkButton` text, not buffer text and not a cell label, and they used to be counted as **nothing at all** while the mixed cell in §2 ("A [link to example] plus **bold**…") matched normally. Step Next/Prev onto a link-cell match → the caption's matched substring turns orange, the previous match reverts to amber, and the preview **scrolls that cell's own row** into view. Change the term so a link cell stops matching → its highlight clears with no stale ink; close the bar → every caption is clean. **Watch for the fix's own trap:** a caption containing `&` or `<` must still render its literal characters, never blank — add `| [R&D <draft> notes](https://example.com) |` to a scratch table and confirm the caption reads normally with the find bar open and closed (ScrAP-163). Then open `tests/fixtures/doc-links.md` and search **`sibling`** → the link captions in the body list items highlight too (ordinary buffer text, the path that always worked — the point is that both agree).
- [ ] **11.4** Edit/split + replace row: Replace → changes current match + advances; Replace All → changes every match; preview mode → replace controls disabled with tooltip
- [ ] **11.5** Find bar open, switch view mode → bar stays open; Escape/close → hides, clears highlights, focus returns to editor
- [ ] **11.8** **Highlights survive every preview-rebuild boundary** (Document Rendering CAM row 8; GTK4Rs/AP-47). In pure-preview (or split) mode with matches highlighted, exercise each boundary and confirm the amber match markers **stay visible** (not erased until the next Next/Prev): (a) **switch the reading theme** (View ▸ Theme); (b) **switch view mode** preview→edit→preview (and preview↔split); (c) **trigger an external reload** (edit the open file on disk / touch it so the file-monitor reloads). Pre-fix, each boundary blanked the markers until a match was cycled.

### §13 Preview zoom
- [ ] **13.1/13.2/13.3** Ctrl++ / Ctrl+- / Ctrl+0 in preview or split → steps up/down the ladder / resets to 100%; body+headings+spacing scale together
- [ ] **13.4** At 50% → Zoom Out disabled; at 300% → Zoom In disabled; at exactly 100% → Reset Zoom disabled
- [ ] **13.5** Pure-edit mode → all three zoom controls disabled (menu + toolbar)
- [ ] **13.6** Set non-default zoom, quit, relaunch → zoom restored
- [ ] **13.7** Scroll partway, change zoom → viewport stays at ~same relative position, not top; **repeated fast** zoom steps (and a zoom right after scrolling) hold position too — no cumulative upward drift (ScrAP-65)
- [ ] **13.10** Change zoom in preview, then switch to edit/split → the editor scrolls normally (mouse wheel + PageUp/PageDown + caret nav), never input-frozen (ScrAP-65)

### §12 Document outline
- [ ] **12.1** Doc with nested H1▸H2▸H3 → outline lists all, in order, indented by level, plain text
- [ ] **12.2** Empty doc / no headings → "No headings" placeholder, no error
- [ ] **12.3** Heading containing `**bold**`/`` `code` ``/link → outline entry shows plain text, no markers
- [ ] **12.4** Preview/split, activate an entry → preview scrolls heading to top; pure-edit, activate → caret moves to that heading in source
- [ ] **12.5** Single-click AND arrow-key navigation both work with no double-click needed
- [ ] **12.6** Entry with sub-headings → visible chevron; click chevron → folds/unfolds without navigating
- [ ] **12.8** Long doc with blockquotes/rules/code/tables in preview or split → rapid-click far top↔bottom outline jumps + fast resize → preview never blanks, no "snapshot without allocation" spam, no spurious h-scrollbar
- [ ] **12.7** Toggle outline (View ▸ Outline / toolbar / F9) → hides, content reclaims width; toggle again → restores
- [ ] **12.9** Split mode, activate outline entry → preview scrolls, editor follows (preview drives, not editor)
- [ ] **12.10** Scroll into a doc, switch view mode → position AND outline selection both hold, neither resets to top
- [ ] **12.12** Preview/split, scroll so section N is at top → outline highlights row N; activate a different entry while stationary → navigates + persists as user-selected
- [ ] **12.13** Click entry A, then manually scroll to section B → outline highlights B (spy overrides); switch modes and back → re-selects A (last user-activated, not spy)
- [ ] **12.14** Edit-only mode, scroll so section N is at top → outline highlights N (same as preview/split)
- [ ] **12.15** Drag a tab (or Move Tab to New Window) into another window, then scroll that tab's view → outline still scroll-spies correctly post-move
- [ ] **12.15b** Scroll an **Edit-mode** tab so a lower section N is at the top, then Move Tab to New Window → in the new window the outline highlights section N **immediately** (matching the preserved scroll position), without needing to nudge the scroll first — the initial spy sync fires on arrival in edit mode too (TDD 12.16)
- [ ] **12.20** **Outline follows the document, live** (TDD 12.20; Derived-view CAM row 1/A-B). Split view, outline shown: type a new `## Heading` mid-document → after the debounce the outline gains that row, **in place**, without switching mode or tab. Retitle it, then change its level (`##`→`###`) → the row's text and indent follow; delete the line → the row goes. Ctrl+Z / Ctrl+Y each step → the outline tracks the undo/redo. Select a heading row, then edit a *different* part of the doc → the selection stays on the same heading. Repeat the type-a-heading step in **edit-only** mode, then switch to preview → the heading is already there (it was added live, not by the mode switch). Finally `echo '## Appended' >> file` externally → the reloaded outline gains it. **Watch:** any case where the row appears only *after* a mode/tab switch is a FAIL — self-healing is not passing
- [ ] **12.11** Outline shown → fixed "Outline" header doesn't scroll with the list; header's × button hides sidebar (same `win.outline` toggle)
- [ ] **12.16** Outline header shows **Expand all** and **Collapse all** buttons left of the × (icons render, not ⚠ placeholders, on breeze-dark AND Adwaita — standard expand-all/collapse-all-symbolic names, so a KDE/breeze host shows its own idiomatic chevron glyphs and our matching bundled chevron art fills in on Adwaita/headless; GTK4Rs/AP-48). Doc with nested headings (≥3 levels): on load the outline is **fully open** by default. Click **Collapse all** → tree folds to only the top-level (root) headings, each showing a collapsed chevron, and it HOLDS (no snap-back). Then expand **ONE** root (click its chevron) → its **direct children appear COLLAPSED**, not the whole subtree springing open — i.e. true recursive collapse, the user descends one level at a time (TDD 12.17; this is the autoexpand=false behavior — under the old autoexpand=true the subtree would have re-opened whole). Click **Expand all** → the whole tree re-opens to the deepest heading. Scroll a collapsed outline's document → no crash (the scroll-spy tolerates headings with no materialised row). On a headings-less doc ("No headings" placeholder) both buttons are a safe no-op (no crash). (TDD 12.17; GTK4Rs/AP-111; `win.outline-expand-all` / `win.outline-collapse-all`)
- [ ] **12.18** Outline scroll-spy follows expand/collapse (TDD 12.18; GTK4Rs/AP-112). Nested doc (≥3 levels), preview mode: click a **deep** entry (e.g. an H3) so the preview scrolls it to the top and the outline highlights **that deep heading**. Click **Collapse all** → the highlight **rises to the enclosing root**, and the preview does **NOT** scroll (it stays on the deep heading). Expand that root (chevron) → highlight **descends to the section** (H2); expand the section → highlight reaches the **exact H3** again — each step WITHOUT the preview scrolling. Click **Expand all** from the collapsed state → highlight returns to the exact deep heading, preview still unmoved. Repeat the whole sequence in **edit/split** with the **caret** in the deep section (highlight tracks the caret's heading, rises/descends the same way, and the caret never jumps). Regression guard: expanding/collapsing a node must never navigate — only clicking a *different* entry does. **Watch:** the preview/caret position is unchanged by every expand/collapse; only a genuine entry click moves it.
- [ ] **12.21** **Tab switch reveals the selected outline row** (TDD 12.21). Two long-outline docs open as tabs: in tab A scroll so a **far** section is highlighted in the outline; switch to tab B (shorter or scrolled elsewhere) then back to A → the outline still highlights the far section **and that row is visible in the list** (not correct-but-off-screen under the previous tab's scroller position). Document must **not** re-jump from a spurious outline navigation on the switch.

### §13 Preview zoom
- [ ] **13.1/13.2/13.3** Ctrl++ / Ctrl+- / Ctrl+0 in preview or split → steps up/down the ladder / resets to 100%; body+headings+spacing scale together
- [ ] **13.4** At 50% → Zoom Out disabled; at 300% → Zoom In disabled; at exactly 100% → Reset Zoom disabled
- [ ] **13.5** Pure-edit mode → all three zoom controls disabled (menu + toolbar)
- [ ] **13.6** Set non-default zoom, quit, relaunch → zoom restored
- [ ] **13.7** Scroll partway, change zoom → viewport stays at ~same relative position, not top; **repeated fast** zoom steps (and a zoom right after scrolling) hold position too — no cumulative upward drift (ScrAP-65)
- [ ] **13.10** Change zoom in preview, then switch to edit/split → the editor scrolls normally (mouse wheel + PageUp/PageDown + caret nav), never input-frozen (ScrAP-65)

### §14 Show Unsafe Images
- [ ] **14.1** Open `remote-image.md` with toggle OFF (default) → broken-image placeholder, no alt text shown
- [ ] **14.2** Toggle ON → remote image fetches over the network and displays
- [ ] **14.3** Open `unsafe-local-image.md`, toggle OFF → placeholder shown
- [ ] **14.4** Toggle ON → loads from absolute path, displays inline
- [ ] **14.5** With unsafe images showing, toggle OFF → immediate re-render, all unsafe images become placeholders
- [ ] **14.6** Set toggle on/off, quit, relaunch → state restored
- [ ] **14.7** Confirm the checkbox (View menu) and toolbar button both exist and mirror each other's state
- [ ] **14.8** Open `unsafe-scheme-image.md` (file://, smb://) with toggle ON → still refused (only http/https admitted)
- [ ] **14.9** Split mode, type a new unsafe image into the editor live, then flip the toggle → preview re-renders from CURRENT buffer, not a stale snapshot
- [ ] **14.9a** **The placeholder names the RIGHT reason — a safe image that is merely not there yet is "not found", never "blocked"** (TDD 14.9; ScrAP-34b). The tooltip is the whole check: all three reasons draw the identical `image-missing` icon, so a screenshot cannot tell them apart and only the hover text can. Stage a scratch folder — `mkdir -p /tmp/imgreason && cp tests/fixtures/logo.png /tmp/imgreason/outside.png && mkdir -p /tmp/imgreason/doc && printf '# Reasons\n' > /tmp/imgreason/doc/d.md` — and open `/tmp/imgreason/doc/d.md` with the toggle **OFF**. (a) *Contained but absent, arriving by live reload* — the reported defect, so drive it exactly this way rather than by opening a document that already references it: `printf '\n![late](late.png)\n' >> /tmp/imgreason/doc/d.md` → the auto-reload lands a placeholder; hover it → **"Image not found: late.png"**. A tooltip reading "Blocked image (enable Show Unsafe Images to load)" is the defect: `late.png` would sit *beside* the document, so the safety gate is not what stopped it, and that wording sends the reader to switch the gate off to "fix" a file that simply had not landed yet (which is why the toggle looked like a workaround). (b) *It lands* — `cp tests/fixtures/logo.png /tmp/imgreason/doc/late.png`, then force a re-render (touch the document again) → the image renders, toggle still OFF. (c) *A real refusal is unchanged* — `printf '\n![out](../outside.png)\n' >> /tmp/imgreason/doc/d.md` → that placeholder's tooltip **does** read "Blocked image (enable Show Unsafe Images to load): ../outside.png", because a real file is there and the gate is genuinely what stopped it; toggling ON then renders it, which is the proof the two reasons are not merely relabelled. Under Xvfb capture the hover with `import -window root` — a GTK tooltip is its own override-redirect surface and never appears in a `-window $WID` capture

### §15 Tabbed documents
- [ ] **15.2** 2+ tabs, Ctrl+W → only active tab closes, others unaffected; single-tab window, close it → whole window closes
- [ ] **15.3** Ctrl+N → new window with one blank tab, existing windows untouched
- [ ] **15.4** 2+ tabs, Ctrl+Shift+N → active tab (content+undo+mode) detaches to a new window; single-tab window → command disabled everywhere
- [ ] **15.5** 2+ tabs, Ctrl+Tab / Ctrl+Shift+Tab (and Ctrl+PageDown/Up) → cycles with wraparound, other windows unaffected
- [ ] **15.6** Drag a tab sideways within its own strip → reorders, content/active-state unaffected
- [ ] **15.7** One tab → title shows filename (or "Scribobulate") with NO parenthetical, strip STILL visible (single-tab strip stays shown); 2+ tabs → title shows the ACTIVE document plus a count of the others — "`alpha.md` (+1 document) — Scribobulate" at two tabs (singular!), "(+2 documents)" at three — strip visible, dirty tabs show "•". Then switch tabs with nothing else changed → the title re-aims at the newly active document, same count (an untitled active tab reads "Scribobulate (+2 documents)")
- [ ] **15.8** Two tabs, one Preview one Split (different swap/orientation) → switching between them restores each tab's own stored view settings
- [ ] **15.9** 2+ tabs, change zoom → every tab's preview in that window rescales, other windows' zoom unaffected
- [ ] **15.10** 2+ windows, 2+ tabs each, mixed modes/splits/zooms, quit, relaunch → every window/tab/mode/split/zoom/active-tab restored exactly
- [ ] **15.11** 3 tabs, 2 dirty, close window → sequential Save/Discard/Cancel per dirty tab; Cancel at any point aborts, all 3 tabs intact
- [ ] **15.12** Search a term in tab A, switch to tab B and back → tab A's query + match state preserved, not tab B's; repeat the A→B switch (and B has no query of its own) several times → no crash/abort (a `RefCell` double-borrow — GTK4Rs/AP-61)
- [ ] **15.13** 2 tabs (different backing files), tab 2 in background, external-edit tab 2's file → conflict/reload logic evaluated against tab 2's own state, resulting toast/reload applies to tab 2 not the visible tab
- [ ] **15.14** 2 tabs, different Show-Unsafe-Images settings, only one has a backing file → switching tabs immediately updates the toggle AND Copy Full Path/Reload enablement to match the newly active tab
- [ ] **15.15** Launch (or D-Bus-forward) with 2+ file paths at once → all land as tabs of ONE new window (or the current blank tab, per 1.5/1.6); if one path is already open elsewhere → skipped from the new window, its existing tab focused instead, the rest still land together
- [ ] **15.16** Reopen an already-open file (any tab, any window, active or not) via interface/CLI/D-Bus → that window focuses and tab activates, no duplicate
- [ ] **15.18** Open several files incl. one whose name has an underscore (`scribobulate -n a.md b_c.md …`) → View ▸ Documents lists all this window's tabs in strip order, "Untitled" for an unsaved tab, the underscore shown (not swallowed as a mnemonic), the active tab marked; clicking an item switches to that tab, and switching via the strip re-marks the correct item. New Document adds it; Ctrl+W removes it and re-marks the neighbor; drag-reorder reorders the list; Save As renames its entry. Move Tab to New Window → each window's Documents shows ONLY its own tabs (window A never lists window B's, and vice-versa). (TDD 15.18, GTK4Rs/AP-76)
- [ ] **15.19** Hover a tab whose document is saved → its tooltip shows the **full absolute file path** (verbatim, the same string Copy Full Path yields), not just the filename; hover an unsaved/new (untitled) tab → the tooltip reads **"Unsaved"**. After a Save As adopts a path (4.7), the same tab's tooltip updates from "Unsaved" to the new full path. The hovered tab's own `×`-close button keeps its "Close tab" tooltip (TDD 15.7)
- [ ] **15.20** The toolbar's **Documents combo** (view section, labelled "Documents" tooltip) is a **second surface of the same View ▸ Documents list** (Derived-view CAM row 4; Action CAM single-source, like the Reading Theme picker in 18.1). Open several files incl. one with an underscore → the combo opens the **same** tab list in strip order, the **same** active tab ticked as the menu, and its **button label shows the active document's filename** ("Untitled" for an unsaved tab). Picking an item switches to that tab; switching via the strip or the View ▸ Documents menu re-ticks the combo AND retargets its label with no interaction (one action, not a mirror). Open/New/Ctrl+W/drag-reorder/Save As-rename each update **both** the menu and the combo together; a very long filename is **ellipsized** in the label (full name still in the dropdown item). Move Tab to New Window → each window's combo lists ONLY its own tabs. (CAM row 4; GTK4Rs/AP-76)
- [ ] **15.22** Open a saved doc, make NO edits, then externally `rm` its backing file → its tab shows a leading **yellow ⚠** marker (alongside the "File deleted on disk — save to restore it" status notice), shown whether the tab is active or in the background, and combinable with the dirty "•". Ctrl+S (or File ▸ Save) → the file is re-created and the ⚠ **clears**; repeat the delete, then externally re-create the file → the ⚠ clears on the next monitor tick. Repeat the delete once more and try to **close** that tab (its ×, Ctrl+W, or the window) → a **Save / Discard / Cancel** prompt appears exactly as for an unsaved tab, even though the buffer is clean; Save re-creates the file and lets the close proceed. Also verify a name with a markup metacharacter (`echo hi > 'A&B.md'; scribobulate -n 'A&B.md'`, then delete it) → the tab label renders `⚠ A&B.md` intact, not blank (Pango-escaping, ScrAP-163). (TDD 15.22; ScrAP-163; CAM Derived-view row 4)

### §16 Keyboard-shortcuts help & status surfaces
- [ ] **16.1** Press **F1** → the keyboard-shortcuts window opens; close it, press **Ctrl+?** → it opens again; close it, choose **Help ▸ Keyboard Shortcuts** → it opens a third way. About does NOT open on F1 (that moved to §9.18's menu-only path) (TDD 16.1)
- [ ] **16.2** With the shortcuts window open → it is grouped **File / Edit / Format / View / Windows & Tabs**; spot-check that every listed key really works (e.g. Format shows **Task List → Ctrl+Alt+C** and pressing it in the editor prefixes `- [ ] `; View shows **Zoom In**, and Ctrl++ zooms). No group shows a blank/garbled accelerator (TDD 16.2)
- [ ] **16.3** Editor clean, Auto-Reload on, external-edit the file → besides the transient toast (§5.4) a **"File reloaded"** message appears in the footer status bar and clears itself after a few seconds, leaving the persistent "Ln L, Col C" / clean-state text intact underneath (TDD 16.3)
- [ ] **16.4** Hover each toolbar button for a command that has a shortcut → the tooltip reads `Label (Accel)` (e.g. **Open (Ctrl+O)**, **Zoom In (Ctrl++)**, **Outline (F9)**, **Go To Line (Ctrl+G)**, **Move Tab to New Window (Ctrl+Shift+N)**); a button whose command has no shortcut (e.g. Swap Panes) shows just the label. The displayed accel matches the shortcuts window and the real binding (TDD 16.4)
- [ ] **16.5** (Screen reader, e.g. Orca) Trigger a status change (make an edit → unsaved indicator; clean external reload → "File reloaded") → the change is announced politely, WITHOUT the keyboard focus jumping to the status bar (the status region carries the accessible "status" role) (TDD 16.5)
- [ ] **16.8** Open `doc-links.md` **and** a second document as two tabs in ONE window (so the window survives losing a tab). Click a broken link in the first tab → the footer shows **"Link target not found: …"**. Within ~2 s (well inside the notice's ~6 s life) press **Ctrl+Shift+N** to move that tab to a new window → within a few seconds the notice disappears from the **original** window's footer, leaving its persistent status intact, and it NEVER appears in the new window's footer. Repeat, but press **Ctrl+W** (close the tab) instead of moving it → the notice still clears from the window's footer. Both failures are silent stale text, so read the original window's footer after ~10 s, not just immediately (TDD 16.8)
- [ ] **16.6** Choose **Help ▸ Markdown Reference** → the system default browser opens the CommonMark reference (`commonmark.org/help/`) and the app keeps running normally (window stays responsive, no crash) (TDD 16.6)

### §17 Annotation & review (CriticMarkup)
> **Popovers need a window manager** — the preview selection popup and the margin-marker
> popover (§17.3-17.8, §17.13-17.21) map only under a real WM; run those on the operator's
> real X11 session (per §1.10), NOT a bare `Xvfb`. The `win.annotate` action, its toolbar
> button, and the in-surface comment CARD (§17.23-17.25, and the editor path) are
> keyboard/click-driven overlay children, so they DO exercise headlessly on `Xvfb` —
> only the popover-gated preview flow needs the real session.
- [ ] **17.1** Open `annotations.md` → the `{==highlighted claim==}{>>…<<}` renders as the claim words with an amber highlight background; NO `{==`/`==}`/`{>>`/`<<}` delimiters and NO comment text appear inline (TDD 17.1)
- [ ] **17.2** In `annotations.md` the `{++ins++}`, `{--del--}`, `{~~old~>new~~}` render as plain "ins", "del", "new" — no braces, no distinct styling (v1 inert) (TDD 17.2)
- [ ] **17.3** Click the amber marker in the right margin beside a commented claim → a popover opens showing the claim (quoted) and its comment text (TDD 17.3)
- [ ] **17.4** On the line carrying two annotations → there is ONE margin marker showing "2"; clicking it lists BOTH annotations, each with its own comment and Edit/Remove (TDD 17.4)
- [ ] **17.5** In preview, select a claim → a pop-up offers **💬 Annotate** → click it, type a comment, Save → the claim becomes highlighted with a margin marker, the title shows unsaved (•), and the file on disk (after Ctrl+S) contains `{==<claim>==}{>>comment<<}` around exactly the selected words (TDD 17.5)
- [ ] **17.6** Select text spanning a blank-line paragraph break → Annotate → Save → a point comment `{>>comment<<}` is inserted at the END of the selection (no highlight span), with its marker on that line (TDD 17.6)
- [ ] **17.7** Open a comment popover → **Edit** → change the text → Save → the popover/marker reflect the new comment and the file's comment body is replaced (nothing else changes) (TDD 17.7)
- [ ] **17.8** Open a comment popover → **Remove**: for a highlight+comment the claim TEXT stays but the highlight+marker vanish (CriticMarkup stripped); for a point comment the whole thing is deleted; re-render confirms the marker is gone (TDD 17.8)
- [ ] **17.9** Annotate + save, then File ▸ Reload (or close & reopen) → every annotation reappears exactly as stored (it lives in the file), no sidecar (TDD 17.9)
- [ ] **17.10** Select across an annotated claim in preview → Ctrl+C → paste elsewhere → the clipboard holds the CLEAN prose (the words, not `{==…==}{>>…<<}`) (TDD 17.10)
- [ ] **17.11** Open `annotations.md` in **split** view → scroll the editor, then the preview → each pane tracks the other to the corresponding line WITHOUT drift from the removed CriticMarkup; compare against an un-annotated doc which must sync identically (TDD 17.11)
- [ ] **17.12** In `annotations.md` the unclosed `{==` and the block-crossing `{==…\n\n…==}` render **literally** (delimiters visible), not swallowed and not treated as annotations (TDD 17.12)
- [ ] **17.13** Select text that OVERLAPS an existing highlight (part of it, or extending past its end into adjacent words) → Annotate → Save → the result is ONE highlight over the UNION (old span ∪ selection) with the new comment, NOT a nested/overlapping pair; check the source has a single well-formed `{==union==}{>>new<<}` (no `{==` inside another). Repeat selecting fully INSIDE an existing highlight → it re-annotates the same span with the new comment (an edit) (TDD 17.13)
- [ ] **17.14** After creating one annotation (17.5), select DIFFERENT text and annotate again → the "💬 Annotate" pop-up appears on the new selection and the second annotation is added; repeat a third time — creation stays repeatable with no reload/reopen (regression: the live-refresh buffer-swap used to kill the overlay after the first) (TDD 17.14)
- [ ] **17.15** Hover the pointer over a margin comment marker → the cursor becomes a **pointer/hand** (same as over a link); moving off it returns to the text/I-beam cursor (TDD 17.15)
- [ ] **17.16** With the annotate overlay showing (button OR an open comment entry), change the selection to different text → the overlay is dismissed and reappears fresh for the new selection (the stale entry does NOT linger); clear the selection (click elsewhere) → the overlay disappears entirely (regression: an open entry used to get stuck) (TDD 17.16)
- [ ] **17.17** Select text → Annotate → the comment entry appears **wide enough** that the "Add a comment…" placeholder is fully visible (not truncated to "Add a commen…") with the Save button beside it (regression: the popover kept the narrow Annotate-button width and clipped the entry) (TDD 17.17)
- [ ] **17.18** Select text that spans inline `code` AND **bold** (e.g. across the intro paragraph's `cargo test` and a **bold** word), across wrapped lines → Annotate → the highlight is ONE contiguous amber span and the code stays monospace / the bold stays bold (constructs intact); the source has a single well-formed `{==…==}` wrapping them WHOLE, no split `` ` `` or `**` (regression: it used to fragment) (TDD 17.18)
- [ ] **17.19** Open `annotate-inline.md` → in the first paragraph (which has soft-wrapped lines, inline `cargo test`, **bold**, and is followed by a code fence) select a single **plain** word (e.g. "verification") → Annotate → Save → ONLY that word is highlighted; the amber span does NOT run to the end of the paragraph or into the `xdotool` code block below (regression: a single word used to engulf the whole block) (TDD 17.19)
- [ ] **17.20** Select text → Annotate → in the comment entry type a comment, watching the **first** character → every character lands in the entry and the caret stays in it; the menubar (File/Edit/View…) never highlights or grabs focus mid-typing (regression: the first keystroke used to hand focus to a menu) (TDD 17.20)
- [ ] **17.21** Create an annotation (Annotate → Save), then edit a comment (marker → Edit → Save), then remove one (marker → Remove), watching the console → each re-renders the preview cleanly with NO "Broken accounting of active state for widget …" warnings and the app stays responsive for the next action (regression: committing inside the button press used to corrupt active-state up the widget tree) (TDD 17.21)
- [ ] **17.22** In `annotate-inline.md` select the inline code `cargo test` → Annotate → Save → the amber highlight is clearly visible OVER the code (the code's grey background does NOT hide it) and an amber marker chip appears in the right margin on that line (regression: a highlight over inline code was painted over by the code background and appeared to vanish — the annotation looked lost/"incorrect") (TDD 17.22)
- [ ] **17.23** With a selection in the active pane, check the four Annotate surfaces all work and gate together: the **Ctrl+Alt+M** accelerator, **Edit ▸ Annotate** (shows the accel hint), the 💬 **Annotate toolbar button** (Format section), and the **right-click context menu ▸ Annotate** (accel hint Ctrl+Alt+M; appears in the editor section with Insert Emoji / Change Case) each raise the comment card; with NO selection all four are disabled/greyed (Edit ▸ Annotate and the context-menu row are insensitive). For **Edit ▸ Annotate specifically**, the card must actually SHOW and STAY (focus in its entry, ready to type) — not flash and vanish (regression: the menu popover's pop-down restored focus to the pane and instantly dismissed the just-raised card, so "Edit ▸ Annotate did nothing"; the action now defers the card to idle so the pop-down settles first) (TDD 17.23)
- [ ] **17.24** In **edit** (or split) mode select a span of source text → press Ctrl+Alt+M (or the toolbar 💬, or Edit ▸ Annotate) → an in-surface card appears at the selection → type a comment, Save → the selected span is wrapped `{==…==}{>>comment<<}` (a blank-line-crossing selection becomes a point `{>>comment<<}` at the end) directly in the editor source, title shows unsaved (•); in split mode the preview live-updates with the highlight+marker (TDD 17.24)
- [ ] **17.25** In a scrolled view, select text near the TOP, the MIDDLE, and the BOTTOM of the viewport (try both panes) and raise the card each time → it always appears centered on the selection and just above it (or below when there's no room above), never pinned to a corner; repeat several times at different offsets — it must not drift further from the selection on repeated use (regression: GTK4Rs/AP-87, the card's margin was folded back into its re-measure and pinned it to the top) (TDD 17.25)
- [ ] **17.26** Click a margin comment marker to open its popover → the comment text is shown as a plain (read-only, non-selectable) label — it is **NOT** pre-highlighted/selected (no blue selection block over the comment) on open, and neither is the quoted claim above it (regression: the comment label was `selectable`, so it auto-selected all its text the instant the popover focused it) (TDD 17.3)
- [ ] **17.27** In **Preview** mode, Annotate a claim (17.5) → **Undo becomes enabled** (Edit ▸ Undo, the toolbar, and Ctrl+Z — no longer greyed in preview) → invoke Undo → the annotation is reverted AND the preview updates to drop its amber highlight + margin marker; **Redo** (Ctrl+Shift+Z) re-applies it and the highlight/marker return. Repeat the whole check in **split** mode (the preview there updates via its own live debounce) (TDD 9.16)
- [ ] **17.28** (Undo grouping regression) Annotate something → **Undo** it → **Redo** it → Annotate something ELSE → now press **Undo once** → it reverts ONLY the second annotation, leaving the first one intact (regression: `redo()` left GtkTextHistory with no undo barrier, so the next annotation merged into the redone step and a single Undo wrongly removed BOTH — now covered by the `redo_then_new_annotation_are_two_independent_undo_steps` integration test) (TDD 9.16)
- [ ] **17.29** Open a doc with a table (e.g. any fixture with `| … |`) in **Preview** → select a word **inside a cell** → **the 💬 Annotate pop-up appears over the cell selection on its own** (parity with a body selection, §17.5) AND Annotate is **enabled** on every surface (Edit ▸ Annotate, toolbar 💬, Ctrl+Alt+M, context menu) — with NO selection all are disabled again. Then Save a comment → the cell shows an amber highlight on the claim **immediately, WITHOUT switching view modes** (regression: it used to appear only after toggling to edit and back), the source has `{==…==}{>>…<<}`, title unsaved; repeat in **split** and confirm the editor shows the CriticMarkup and the preview cell highlights immediately (TDD 17.28 / Document Rendering CAM row 1; a cell selection is a selection island tracked via the primary clipboard, not a buffer signal — ScrAP-110)
- [ ] **17.30** After 17.29 in **Preview-only**: Undo → cell highlight and CriticMarkup gone; Redo → both return. Save → Reload → annotation reappears from disk. Copy the highlighted cell text (select in cell → Copy) → clipboard is clean prose without `{==` (TDD 17.29 / CAM rows 4–6)
- [ ] **17.31** Multi-row table: annotate a claim in a **lower** row (not the top) → Save → the right-margin marker chip sits **beside the EXACT annotated row** (vertically aligned with that cell's highlight), not at the table's top edge and not one row off, and it is correct on the FIRST paint **without** needing a view-mode toggle to fix it (regression, bug C: a `cell→view` cached Y read GTK's off-screen placeholder origin and drew the chip a row too high until a rebuild; now `cell→table` + the table-anchor `line_yrange` is placeholder-immune — GTK4Rs/AP-91); confirm in preview-only and split (TDD 17.30)
- [ ] **17.32** In a table with an annotated cell, click that cell's **right-margin marker chip** → it opens **that marker's comment popover** (claim + comment + Edit/Remove), NOT the create card; if a cell selection had the 💬 create pop-up showing, opening the marker **dismisses/does not stack** the create pop-up (only the marker popover remains). Then select cell text (💬 create pop-up shows) and click an **empty area of the preview** → the create pop-up **dismisses** and the cell selection clears (regression: for a cell selection the create pop-up used to linger "until you select text outside the table", because a cell selection fires no buffer signal — now driven off the primary-clipboard `changed`) (TDD 17.31 / preview-only and split)
- [ ] **17.33** **(Real compositor only — a WM-less Xvfb never arms the tooltip timer, so this passes vacuously headless; run it on the operator's real KDE/kwin session.)** Drive the whole cell-annotate flow (select cell → 💬 pop-up → Annotate → Save → click marker chip → Edit/Remove → dismiss; click several DIFFERENT markers in a row) in preview-only **and** split, with the mouse hovering the popovers, watching the console under `G_DEBUG=fatal-criticals` → NO `gdk_surface_get_device_position` / `_gtk_widget_find_at_coords` `GDK_IS_SURFACE` assertion appears and the app never aborts (the marker comment popover is now a single PERSISTENT instance — never unrealized per use — so a stale tooltip timer can't fire against a NULL popover surface; GTK4Rs/AP-117) (TDD 17.31)
- [ ] **17.34** In a **tall** table (rows exceeding the viewport) with an annotation in a **lower** row, in **Preview**: (a) on load the row's marker chip is drawn beside its row **without touching the mouse** (no mouse-move needed for it to appear); (b) **wheel- or keyboard-scroll** so the annotated row moves — the chip stays drawn, tracks the row, and remains visible after the scroll stops **without moving the mouse**; (c) click anywhere in the document → the chip does not flicker or vanish (regression, bug E: the chip Y was re-measured from a `cell→view` translate every snapshot, which dropped mid-scroll and flickered — now the placeholder- and scroll-immune `cell→table` transform + table-anchor `line_yrange` is recomputed per frame with no cache, so it neither flickers nor goes stale; GTK4Rs/AP-91) (TDD 17.32 / Document Rendering CAM row 1 "immediately and stably")
- [ ] **17.35** **(Real compositor only — the popup-driven revalidation scroll does not occur on WM-less Xvfb.)** In a **tall** table (rows exceeding the viewport — e.g. this repo's `sdd/ANTI-PATTERNS.md` TOC table) with an annotation in the **BOTTOM** row, in preview-only and split, verify the marker click neither scrolls the pane nor fails to open, in BOTH scenarios:
  - **(a) first-click:** the VERY FIRST marker click of the session — click the bottom row's chip → the pane does NOT jump toward the table top and the popover presents in place on the first try (regression: the first `set_parent`+`popup` forced the view's first full validation → `validate_onscreen` re-anchor scrolled AND shifted the chip's hitbox out from under the grab; now PRE-WARMED once at first map, scroll 0).
  - **(b) after adding annotations, scrolled mid-table:** add an annotation near the BOTTOM of the table → scroll UP a little, add another higher in the table → scroll back DOWN to the first → click its margin chip → the pane must NOT jump to the table top and the popover opens in place (regression: the in-place cell-label `set_markup` left the tall single-anchor table's LINE dirty, and the popup flushed that validation → `validate_onscreen` re-anchored to the table's paragraph = its top; now the saved scroll value is held across the popup's settle via a `value-changed` guard disconnected tick-settled — GTK4Rs/AP-118).
  (Document Rendering CAM row 11)
- [ ] **17.36** **Bug A — annotation persists across a mode switch.** In **preview-only** mode, annotate a claim (body or cell) → Save → the highlight+marker show. Now switch **Preview → Split** (once) → the annotation's highlight and marker are **still present** in the rebuilt preview (regression: the edit lived only in the editor buffer; `st.source` was never updated, so the fresh Split render read a stale source and the annotation vanished until a second toggle flushed it — now the annotation re-render flushes `editor_text()`→`st.source`; ScrAP-114). Repeat starting from **Preview → Edit → Split** and confirm it survives; the editor source shows the `{==…==}{>>…<<}` throughout (TDD 17.28 / Document Rendering CAM row 1)
- [ ] **17.37** **Bug 2 — removing the last cell annotation clears its highlight.** In a table cell, create an annotation (17.29) → the amber cell highlight shows. Open its marker → **Remove** → the amber highlight in the cell **disappears immediately** (no mode switch, no scroll jump), the source loses the `{==…==}{>>…<<}`, and the margin chip is gone. Do this when it is the **ONLY** annotation in the document (regression: the in-place cell-label reconciliation was gated on "a current annotation lands in a cell", so removing the LAST one skipped the copy and the cleared cell kept its stale amber markup until a full re-render — now the live cell labels are reconciled unconditionally; ScrAP-111). Also verify with two cell annotations: removing one leaves the OTHER highlighted correctly (Document Rendering CAM rows 1/4)
- [ ] **17.38** **Bug 3 — a repeated / formatted claim highlights the RIGHT occurrence.** In a table cell whose text both carries inline formatting (e.g. `` `code` `` or **bold**) AND repeats a word — e.g. `` | `fn` | the cat sat on the cat today | `` — select the **SECOND** "cat" and Annotate → Save → the amber highlight lands on the **second** "cat" (the one you selected), NOT the first (regression, ISSUES AAA / bug 3: the formatted-cell branch wrapped the first `find` match of the claim text; now it injects at the exact char offset, tag-aware). Also annotate a claim that spans **from normal text into a bold/code run** (e.g. select across the boundary) → the whole selected span is highlighted (not silently dropped), and the cell's bold/code styling is preserved (no broken/garbled markup) (Document Rendering CAM row 1)
- [ ] **17.39** **An editor-pane annotation never splits an inline construct.** In the **Edit** (or Split) pane, on a line containing `a **bold** b`, select only `bol` — strictly inside the `**` delimiters — and Annotate → type a comment → Save. The source becomes `a {==**bold**==}{>>comment<<} b`: the WHOLE construct is wrapped, never `**{==bol==}{>>…<<}d**`. Repeat for a selection starting inside a `` `code span` `` and for one inside a `[link](url)`'s text → each swallows the whole construct. Then select plain prose touching **no** construct → it is wrapped **character-precisely** (the balancer must not over-reach), and select **exactly** `**bold**` → it is not widened. **Then the four constructs this app tokenises itself** (invisible to pulldown-cmark, so they were the half that split — ScrAP-195): on lines containing `a ==mark== b`, `a ~~strike~~ b`, `a ^sup^ b` and `a ~sub~ b`, select a few characters strictly INSIDE the content (`ar`, `rik`, the `u`) and annotate → each wraps the WHOLE construct (`{===mark==…==}`-style, i.e. the `==`/`~~`/`^`/`~` pair is inside the annotation), never `a ==m{==ar==}{>>…<<}k== b`. Also select from inside `==mark==` through into a following `**bold**` → BOTH are swallowed whole. And confirm the tight-flanking rules still gate it: on `a == b` and `2^10`, selecting the `=`/`^` (literal, not a construct) stays character-precise. Repeat the same six selections in the **Preview** pane (that path balances through different code and must agree). **Claim-highlight EXTENT on those lines** (TDD 17.18): on `a ==mark== b`, annotate just the trailing `b` → the amber wash covers **only `b`**, NOT the whole `a mark b`; annotate the construct → only `mark` washes; check the same for `~~strike~~`/`^sup^`/`~sub~` lines and for a **table cell** containing `==mark==` plus an annotation (body and cell share one mapper, so they must agree). Switch to Preview and confirm each renders as a normal amber highlight with a margin marker (TDD 17.33 / 17.18)
- [ ] **17.54** **The annotation walk advances and reverses** (TDD 17.54). `annotations-viewer.md`, Preview: put the caret at the top, then press **Ctrl+Alt+N** four times → each press opens the **next** annotation's card, in document order. A press that re-opens the card already showing is the regression this item exists for (the walk is measured from the caret, and nothing used to move the caret, so every press answered "the first annotation" forever). Keep pressing past the last annotation → it wraps to the first. Now press **Ctrl+Alt+P** → it steps **back** one, and wraps from the first to the last. Click into the document somewhere else, press Ctrl+Alt+N → the walk resumes **from where you clicked**. Confirm both appear in **Edit ▸ Next/Previous Annotation** with their accelerators shown, and in the Keyboard Shortcuts window (Ctrl+?)
- [ ] **17.55** **The walk works in every view mode** (TDD 17.55). Repeat 17.54's first two presses in **Split** → the preview scrolls and the card opens **and** the editor caret lands on the annotation. Then in **Edit-only** → the editor caret steps from annotation to annotation (no card; there is no preview) — a mode where the keys do nothing is the regression. Use `annotations-viewer.md`, whose multi-byte text precedes the later annotations, and confirm the edit-mode caret lands exactly on the `{==`/`{>>`, not displaced. Finally open `table-test.md` (no annotations) and press both keys → nothing happens, no error, and the menu items stay **enabled**


---

### §18 Preview reading themes

**Environment:** bare Xvfb is fine for every item here EXCEPT 18.4-breeze (below),
which exists specifically to catch a desktop-theme-dependent bug. Run the sweep
under Xvfb, then do 18.4-breeze once — the whole point is that a theme other than
GTK's bundled Default paints the `text` node opaque (GTK4Rs/AP-100).

**Reset between runs:** the theme persists app-wide in `session.toml`, so point
`XDG_STATE_HOME` at a scratch dir per run or you will inherit the last run's theme
and misread a "wrong theme on launch" as a defect.

- [ ] **18.1** Open `themes.md` → View ▸ Reading Theme lists **System** and **Sepia**, System ticked. The toolbar's theme button (view section, palette icon) opens the **same** list with the **same** item ticked. Change the theme from one surface → the *other* surface's tick moves too, with no interaction (they are one action, not two mirrors)
- [ ] **18.2** **The regression bar — do this FIRST and keep the screenshot.** Under System, screenshot `themes.md`. This must be indistinguishable from the pre-theming build: page, body text, link/code colours, blockquote bar, table borders/header fill, list indents, annotation amber, find yellow. **Exception — heading sizes:** the Issue-H fix deliberately changed the default heading ramp to five tiers `[2.2, 1.8, 1.48, 1.2, 1.0]` (h1 larger, an explicit h5, h6 folding onto h5), so heading sizes will *not* match the pre-theming build and that is expected — verify them against §18.13 instead, not against the old baseline. Compare all other surfaces against `git stash`-ing the change if in any doubt. **Any visible difference under System, headings aside, is a FAIL**, however pretty
- [ ] **18.13** **Heading hierarchy.** In `themes.md` under System, the six heading lines read as a clean descending ramp h1 > h2 > h3 > h4 > h5, each visibly larger than the next (~15–18% steps), h5 at body size but bold. **`###### Heading 6` is pixel-identical to `##### Heading 5`** — h6 folds onto the deepest tier on purpose. Cross-check the outline sidebar: it shows the same fold (no distinct h6 row; h6 entries take the h5 style)
- [ ] **18.3** Scroll partway down, then select Sepia → the page repaints book-like (warm off-white page, serif body, soft brown text) **in the same window**, the reading position is **unchanged**, and the file is not re-read (touch-mtime check, as 2.15)
- [ ] **18.4** Under Sepia, sweep `themes.md` top to bottom → **no element is left on desktop-theme colours**: no white slab (code block), no blue bar (blockquote), no grey island (table header), no blue selection tint (drag over the image), no system-coloured horizontal rule, no white card (annotate a claim → the entry card; trigger a conflict → the toast). Headings/bold/italic/strike all take the page's ink. **A single white/blue/grey island is a FAIL** — that is exactly the surface-audit gap this rubric guards
- [ ] **18.4-breeze** **Not skippable, and Xvfb cannot see it.** On a desktop whose GTK theme paints `textview > text` opaque (Breeze/Breeze-Dark — i.e. the operator's real session, or `GTK_THEME=Breeze-Dark` on the Xvfb display), select Sepia → the page is **sepia**, not the theme's own base colour with sepia text on it. A widget-node-only background is silently overpainted by the `text` node (GTK4Rs/AP-100); GTK's bundled Default theme sets `text` transparent and so **cannot** reproduce this
- [ ] **18.5** Under **each** installed theme: the annotation highlight and the find highlights (Ctrl+F "flat") are **clearly visible** against that theme's page. A pale wash you have to hunt for is a FAIL — the system yellows are near-invisible on cream, which is why these are theme keys
- [ ] **18.6** Find "flat" under Sepia → the highlight on "flat" in the **table cell** is the **same colour** as on "flat" in body prose. Same for an annotation covering a cell claim vs a body claim. (Two application paths, one key — they were independent literals; ScrAP-36)
- [ ] **18.7** Under Sepia → the **toolbar, tab strip, outline sidebar, and editor** (Alt+2 for split) keep the **desktop** theme. Then toggle the desktop to dark with Sepia active → the editor/chrome go dark while the preview **stays sepia**. (The editor following the *page's* lightness instead of the desktop's is the exact regression ScrAP-131 records)
- [ ] **18.8** Under each theme, read a paragraph of body prose → comfortably legible; nothing washed out. (The automated contrast floor covers the arithmetic; this is the human check that the floor is set somewhere sane)
- [ ] **18.9** Under Sepia, Ctrl++ / Ctrl+- across the range → body and headings scale **together**, the heading hierarchy keeps its proportions, and the sepia colours and serif face are unchanged at every step. Nothing jumps to the system font mid-zoom. (Theme owns Pango scale; zoom owns CSS font-size — GTK4Rs/AP-101)
- [ ] **18.10** In `themes.md` under each theme → every list marker sits beside **its own** text at every depth, and quoted lists stay inside the quote's bar. Then add `list_step = 44` to a `[themes.<id>]` in `~/.config/scribobulate/themes.toml` → the indent widens **and the markers move with it**. A marker stranded away from its text is the GTK4Rs/AP-96 failure mode this rubric exists for
- [ ] **18.11** Put a hostile `~/.config/scribobulate/themes.toml` in place — invalid TOML; then `list_step = -5`; then `list_step = 10000`; then `background = "#fff; } * { background-color: red; }"`; then `font_family = 'Georgia; } * { color: red; }'` — relaunching after each → the app **starts every time**, renders **legibly**, and nothing bleeds outside its intended surface (no red anything). No crash, no broken layout
- [ ] **18.12** Select Sepia, quit, relaunch → Sepia is active **from the first paint** (no flash of the default theme first). Then delete Sepia from `themes.toml` and relaunch → falls back to System with the tick on System, no crash
- [ ] **18.13** Create `~/.config/scribobulate/themes.toml` with `[themes.forest]` (a new theme) and a one-key override of a shipped theme (e.g. `[themes.sepia]` `background = "#ffe9b0"`) → relaunch: Forest appears in **both** surfaces' lists and renders; Sepia's page takes the override while **everything else about Sepia is unchanged**. **This is the XDG trap guard**: the app redirects `XDG_CONFIG_HOME` at startup for the XCompose workaround, so a regression here makes the user's themes silently never load, with no error (ScrAP-128 family; `sdd/THEMING.md` → Search path)
- [ ] **18.14** The Forest theme from 18.13 required **no code change** — confirm it renders correctly (page, ink, derived link/code/bar/table colours all follow from its three base colours). Adding a theme is a TOML block

---

### §19 Local document-link navigation

**Fixture:** `doc-links.md` — every case below is one of its links. Its out-of-folder
targets are `tests/other-fixtures/outside-doc.md` and the `escapes-via-symlink.md`
symlink that points at it. **Run from a checkout**, since containment is decided
relative to the *document's own folder*.

**Watch throughout:** no click here may ever reach an external browser or file
handler except the last two (https/mailto). A silent no-op is a FAIL in every
refusal case — the notice is the feature.

**PRECONDITION for 19.2's and 19.3's symlink halves — check this before ticking
either.** `escapes-via-symlink.md` is committed as a git symlink (mode `120000`),
but a checkout only *materialises* one where the platform allows it. Where it does
not — Windows without Developer Mode, i.e. `core.symlinks=false` — git writes an
ordinary 32-byte text file whose entire content is the target path. There is then no
link to resolve, the file is contained in its own folder, and **the app correctly
navigates to it**. The step does not merely fail to test anything: it produces the
opposite of the specified outcome, and what opens looks like an unremarkable
one-line document rather than a security failure. Check first:

```
git ls-files -s tests/fixtures/escapes-via-symlink.md      # always 120000 (the index)
# unix:     test -L tests/fixtures/escapes-via-symlink.md && echo real symlink
# windows:  (Get-Item tests\fixtures\escapes-via-symlink.md).LinkType   # empty => plain file
```

If it is a plain file, **record the symlink halves of 19.2 and 19.3 as NOT
EXERCISED — do not tick them, and do not mark them failed either.** The traversal
halves of both steps are unaffected and still count. The behaviour itself is covered
cross-platform by `links::tests::doc_link_refuses_symlink_escape_when_toggle_off`
and its `..._resolves_to_its_real_target_when_toggle_on` sibling, which build their
own symlink in a temp directory and print an explicit `SKIPPED [...]` line naming
this same privilege when they cannot.

- [ ] **19.1** Click "A Markdown sibling" → `second-doc.md` opens as a **new tab in this window** (not an external handler, not a new window). Then click "The same sibling again" (a different spelling of the same path) → it **focuses the tab already open**, no duplicate — the File ▸ Open dedup, on canonicalized paths (TDD 19.1)
- [ ] **19.2** With **Load Unsafe Linked Documents OFF** (the default — confirm it is off for this tab), click "Traversal out of the folder" → **refused with a visible status-bar notice** naming the folder-boundary reason. Then, **only if the precondition above says the fixture is a real symlink**, click "A symlink pointing out of the folder" → refused the same way, decided on the **resolved** target rather than the link text. On a plain-file checkout that second click **will open a tab** and that is not a defect in the app — it is the fixture failing to exist; record it NOT EXERCISED (TDD 19.2)
- [ ] **19.3** Turn the toggle **ON** for this tab → the same two links now navigate. On the tab you land on (`outside-doc.md`), check its **own** toggle → it is **OFF**: permission re-roots at every hop and cannot ratchet along a link chain (TDD 19.3)
- [ ] **19.4** With the toggle ON in one tab: open a second tab → its toggle is **OFF**; open a second window → **OFF**; quit and relaunch (session restore) → **OFF everywhere**. The consent is per-document, per-session, and deliberately absent from the session file (TDD 19.4)
- [ ] **19.5** Turn the toggle ON, then **Move Tab to New Window** (or drag the tab across) → in the destination window the **File-menu checkbox and the toolbar button both report ON**, matching the moved tab, not the new window's OFF construction default (TDD 19.5)
- [ ] **19.6** Click "A non-Markdown local file" (`logo.png`) → refused with a visible **"Not a Markdown document"** notice. Repeat with the containment toggle **ON** → still refused; the toggle does not open this door (TDD 19.6)
- [ ] **19.7** Click "An explicit file:// URL" → refused with a visible notice, toggle ON or OFF. The scheme gate and the containment gate answer different questions and neither overrides the other (TDD 19.7)
- [ ] **19.7a** *(No manual step — unit-tested.)* A local path whose filename contains a colon (`report:draft.md`) must be read as a schemeless local reference, **not** refused as a disallowed scheme (a colon before the first slash is not a URL scheme; a genuine `file://` is still refused by 19.7). There is deliberately no fixture: a colon-named file is invalid on Windows and would break `git clone` (ScrAP-164), so this is verified cross-platform by `links::scheme_of` / `is_allowed_url` string-literal tests instead (TDD 19.7a; ScrAP-151)
- [ ] **19.8** Click "A missing sibling" → a visible **"Link target not found"** notice, whose wording is **distinguishable** from 19.2's containment refusal. Read both notices back to back: the reader must be able to tell "missing" from "out of bounds" (TDD 19.8)
- [ ] **19.9** With the toggle **OFF**, click "A home-directory path" (`~/notes.md`) → **Refused** (the containment reason), **not** "not found" — `~` expands to your home directory *before* the in/out-of-folder decision, so it is an absolute path outside the folder. Turn the toggle **ON** → it now resolves (and reports "not found" only if no such file exists). Then click "A directory literally named ~" → treated as an ordinary in-folder path component, so "not found", **not** a refusal. Finally confirm an image with a `~/` path reaches the **same** verdict as the link — one shared resolver, no drift (TDD 19.9)
- [ ] **19.10** Click "A sibling with a fragment" (`anchors.md#deep`) → the target opens as a new tab **and is scrolled so that heading sits at the top**, exactly as clicking it in that document's own outline would (TDD 19.10)
- [ ] **19.11** With `anchors.md` **already open** in some tab (this window or another) and scrolled somewhere else, click that same fragment link → the existing tab is **focused AND scrolled** to the heading. Focusing alone, leaving it where it was, is a FAIL (TDD 19.11)
- [ ] **19.12** Click "A fragment matching no heading" → the target still opens (or focuses) normally, scrolls nowhere in particular, and **no error notice appears** — the same silent outcome a same-document `#anchor` miss already has (TDD 19.12)
- [ ] **19.13** Click a plain relative link with no fragment (19.1's) → opens/focuses and **nothing scrolls**; the fragment feature is inert when there is no fragment (TDD 19.13)
- [ ] **19.ext** Click "An https link" and "A mailto link" → these DO go to the external handler, unchanged. (Not a §19 rubric; here so the refusals above are not mistaken for "all links are blocked")

---

### §20 Annotations viewer

**Environment:** the viewer's navigation opens annotation cards, so — like §17 —
every item here needs a **real WM on the X server** (per §1.10), not a bare `Xvfb`;
an override-redirect popover has nowhere to land without one. Use
`annotations-viewer.md` (§2 fixtures) unless an item names another file.

- [ ] **20.1** Open `annotations-viewer.md`, show the viewer (View ▸ Annotations) → one row per comment-bearing annotation, **in document order**, each showing the comment with the annotated claim as dimmed secondary text; a standalone point comment shows its comment only (TDD 20.1)
- [ ] **20.2** Same doc → the bare `{==highlight==}` (no comment) and the inert `{++ins++}`/`{--del--}`/`{~~a~>b~~}` are **absent** from the list, and the row count equals the number of margin chips the preview draws (TDD 20.2)
- [ ] **20.3** Open a doc with no annotations (`table-test.md`) → a muted "No annotations" placeholder, no error, no empty-list crash (TDD 20.3)
- [ ] **20.4** Preview mode, activate a row → the document scrolls to that annotation and its comment card opens, exactly as clicking its margin chip. In **edit-only**, activating moves the caret to the annotation's source position and opens **no** card. In **split**, it scrolls the *preview* (preview drives, as 12.9), opens the card, **and** places the editor caret on the annotation with the editor following into the same region (TDD 20.4)
- [ ] **20.5** Single-click a row → navigates immediately (no double-click). Give the list focus and use ↑/↓/PgUp/PgDown → each selection change navigates too. **Then keep arrowing, without touching the mouse:** the second and third presses must move on to the next rows — i.e. the focus stayed in the LIST across the navigation. "First arrow works, the next does nothing" is exactly the regression this item exists for: the card used to take the focus on open, so a keyboard browse ended after one row (TDD 20.5)
- [ ] **20.6** In `annotations-viewer.md` a bare highlight and an inert kind sit **before** a comment-bearing annotation (so its list index ≠ its chip index) → activate that annotation's row → the document lands on **that** annotation, never a neighbour (TDD 20.6)
- [ ] **20.7** Scroll so the annotated **table cell** is off-screen, then activate its row → the table scrolls into view and the card opens over the **correct cell**, and the position **holds** (no silent revert to the pre-navigation scroll) (TDD 20.7)
- [ ] **20.8** Toggle the viewer from View ▸ Annotations, the toolbar view button, and the pane's own × → all three hide/show the same pane and all three show one consistent checked state (one action, not three mirrors) (TDD 20.8)
- [ ] **20.9** Toggle outline and annotations independently → both shown = stacked sections with bold headers and **no draggable divider between them**; either alone fills the sidebar; **both off = the whole sidebar disappears** and content reclaims the full width (TDD 20.9)
- [ ] **20.10** With a row selected, scroll the document and move the caret around **without** activating anything → the viewer's selection does **not** move (annotations are notes, not sections — no scroll-spy, unlike the outline). **And** in a freshly launched window, show the viewer as your very first action → **no row is highlighted** until you activate one (TDD 20.10)
- [ ] **20.11** With the viewer shown, exercise each rebuild boundary in turn — live edit (after debounce), view-mode switch, external reload, runtime theme switch — → after each the list matches the current annotations (reading the live editor buffer in edit/split, the stored source in preview) and a still-present selection is preserved (TDD 20.11; Derived-view CAM row 3/C)
- [ ] **20.18** **Tab switch reveals the selected annotations row** (TDD 20.18). Two long annotated docs as tabs (`annotations-viewer.md` or similar): in tab A select a **far** list row; switch to tab B then back to A → the same annotation is still selected **and that row is visible in the list** (not correct-but-off-screen). The document must **not** re-navigate from the restore.
- [ ] **20.12** Long annotated doc (tables/code/blockquotes/rules) in preview and split → rapidly activate far top↔bottom rows while resizing → the preview never blanks, no "snapshot … without a current allocation" spam, no spurious horizontal scrollbar (TDD 20.12)
- [ ] **20.13** Toggle the viewer to a given state, quit, relaunch (session restore) → it returns in that state, per window, independently of the outline's state (TDD 20.13)
- [ ] **20.14** In `annotations-viewer.md` (multi-byte em dashes precede the later annotations, so byte offset ≠ char offset), edit-only mode, activate one of those rows → the caret lands **exactly** at the annotation, not displaced by the width of the preceding multi-byte runs (TDD 20.14)
- [ ] **20.15** Edit or split mode: type a new `{>>comment<<}` in the editor → after the debounce a row appears; edit a comment's text → the row's text updates; delete an annotation → its row goes. With a row selected, insert text *above* it (shifting spans) → the selection stays on the same annotation (TDD 20.15)
- [ ] **20.16** Long doc, annotations scattered far apart, preview or split → activate several rows in quick succession (click and key), before each scroll settles, including far up-and-down jumps → the document ends at the **last** selected annotation with selection and scroll agreeing; no earlier target keeps pulling the view back (TDD 20.16; GTK4Rs/AP-172)
- [ ] **20.17** **CRUD refreshes the list in the mode it happened in** (TDD 20.17; Derived-view CAM row 3/A). In **preview-only** mode with the viewer shown: select a claim → Annotate → Save → the new annotation's row appears **immediately**, no mode switch, tab switch, or reload. Then open its margin card → Edit the comment → the row's text updates at once; → Remove → the row disappears at once. Repeat the whole sequence in **split** and in **edit-only**. **Watch:** the failure mode is a list that looks right only after you switch modes — do NOT switch modes to check, verify in place

- [ ] **20.19** Hide the annotations pane, then show it again (View ▸ Annotations / F8) → **without touching the mouse**, press ↓ → the list moves its selection, i.e. showing the pane handed it the keyboard. Then, with the pane shown, switch tabs back and forth → the focus must **not** jump into the list on a tab switch (only the toggle focuses, never the reconcile). Repeat both halves for the outline pane / F9 (TDD 20.19)
- [ ] **20.20** With the list focused and an annotation's card open from it, press **Escape** → the card dismisses **and** the focus lands back in the document pane (press ↓ afterwards: the *document* scrolls, not the list) (TDD 20.20)

### §21 Crash forensics

**Environment:** entirely shell- and file-driven — no WM, no clicks. Run it under a
private `Xvfb` (§1.10) with **`XDG_STATE_HOME` pointed at a scratch directory**, so
you inspect this run's artefacts and never disturb the operator's own state. Every
check below reads a file; none needs a screenshot.

> Deliberate crashes here: use `kill -SEGV <pid>` on the app's **own** pid (the one
> you launched — §1.10's PID discipline applies exactly as elsewhere, and matters
> more, since the point of the exercise is to kill something).

- [ ] **21.1** Launch with a scratch `XDG_STATE_HOME` → `$XDG_STATE_HOME/scribobulate/scribobulate.log` exists and opens with `=== run start ===` followed by the version + git commit + profile, the executable path/size/mtime, the GTK **runtime** version, `renderer: cairo`, the pid and the start time — with **no `RUST_LOG` set**. Confirm stderr looks exactly as it did before (nothing new printed) (TDD 21.1)
- [ ] **21.2** `RUST_LOG=trace` launch, exercise the app until the log passes 1 MiB → a `scribobulate.log.1` appears beside it, the live file restarts from the header, the newest records are in the **live** file, and **no** `.log.2` is ever created (TDD 21.2)
- [ ] **21.3** Default launch (no `RUST_LOG`): open a file, save it, touch the file externally to trigger a reload, open File ▸ Open and cancel, close a tab, close the window → `scribobulate.log` has an `INFO` line for each, naming the path or tab id; stderr stayed quiet throughout (TDD 21.3)
- [ ] **21.4** With the app running, `kill -SEGV <its pid>` → a `crash-<stamp>-<pid>.log` appears in the state dir naming the signal, the fault address and the instruction pointer, above the same identity block as 21.1. Check the shell reports the death as a signal (`$?` = 139, and `dmesg`/`journalctl -k` still shows a `segfault at …` line) — the handler must not have converted the crash into a clean exit (TDD 21.4)
- [ ] **21.5** In that report, the `--- breadcrumbs ---` block lists the last things you did before the kill, in order, with timestamps — **including** GTK's benign gizmo warnings, which do *not* appear on stderr (TDD 21.5)
- [ ] **21.6** Read the report top to bottom → identity and fault first, breadcrumbs next, backtrace and executable mappings last (TDD 21.6)
- [ ] **21.7** Force a panic (easiest: a debug build with a temporary `panic!`, or any known panicking path) → a report with `kind: panic` naming the message and its `file:line`, the panic message still reaches stderr, and the app unwinds rather than aborting (TDD 21.7)
- [ ] **21.8** In the 21.4 report's `--- executable mappings ---`, find the line for `libgtk-4.so` → it carries the mapping's start address and file offset, so `instruction pointer − start + offset` resolves without a core dump. Sanity-check one frame from the backtrace against `addr2line` on the unstripped `target/release/scribobulate` (TDD 21.8)
- [ ] **21.9** Relaunch after 21.4 → exactly one `WARN … a crash report from a previous run is unread: <path>` on stderr and in the log, and a `crash-last-seen` marker appears. Relaunch a **second** time → no such warning (TDD 21.9)
- [ ] **21.10** Read the whole of `scribobulate.log` and the crash report → they contain paths, sizes, counts and event names, and **no document text, no selection, no clipboard content**. Do this with a document holding distinctive text and grep the artefacts for it (TDD 21.10)
- [ ] **21.11** Compare a default launch against the pre-forensics behaviour: no new stderr output, no perceptible startup delay, and `RUST_LOG` unset means **no** `debug`/`trace` records anywhere — including in the log file (TDD 21.11)
- [ ] **21.12** `ls -l "$XDG_STATE_HOME/scribobulate"/crash-*` after 21.4 → every report **and** the `crash-last-seen` marker are `-rw-------` (0600), with no group or world bits, under a permissive umask (`umask 0002` before launching, so a leaked mode is visible rather than masked by a strict default). Check a report the **panic hook wrote first and the signal handler then appended to** — the second writer is the case a fresh-file check cannot express (TDD 21.12)
- [ ] **21.12b** Trigger a panic inside a GTK callback (so it ends in `abort`) on a run that already wrote a report → the report holds **both** faults, oldest first: the panic's message, location and backtrace are still there *and* the `SIGABRT` entry follows them. The panic's own evidence must not have been truncated away by the fault it caused (TDD 21.12 / QA round 5 H-2)


### §22 Crash recovery (swap files)

> **Before driving ANY of these, confirm both the tree and the binary.** Two separate
> traps, and the second bit a seat that had already avoided the first:
>
> ```
> git log --oneline -1          # is this the commit you think you are testing?
> cargo test --lib close_semantics   # 0 tests / no filter match = the feature is not in this tree
> ```
>
> then **rebuild** before driving a GUI check. `cargo test` proves the *tree*; it says
> nothing about the `target/release` binary you are about to launch, which may predate the
> change by days. A stale binary against a correct tree fails every check below for a
> reason that looks exactly like a broken feature. (Reported by the Windows seat, who
> caught a binary one day older than the commit under test — on the very change whose
> subject was the write path.)


**Environment:** like §21, mostly shell- and file-driven, and it must run under a
private `Xvfb` (§1.10) with **`XDG_STATE_HOME` pointed at a scratch directory** —
here that is not merely politeness: the checks deliberately kill the app with unsaved
work in it, and a shared state directory would leave real recovery data behind for
the operator's own next launch. The recovery data lives in
`$XDG_STATE_HOME/scribobulate/swap/`; several checks are just `ls` on it.

> **PID discipline, and it matters more here than anywhere.** Every check below kills
> a running instance on purpose, so kill only the pid you launched (§1.10). Use
> `kill -9` (not `-TERM`, and never the window close button) where the check says
> "dies uncleanly" — a graceful close would resolve the dirty tabs through the save
> prompt and there would be nothing to recover, which reads as a failure of the
> feature rather than of the test.

- [ ] **22.1** Launch, type into a document without saving, wait ~4 s (the snapshot debounce is 3 s), `kill -9 <pid>`, relaunch → the tab comes back with the text you typed **and** still marked unsaved; the file on disk is byte-for-byte what it was before (`md5sum` it before and after) (TDD 22.1)
- [ ] **22.2** Type into a document, wait for the snapshot (`ls` the swap dir → one `.swap` file), close the tab and choose **Discard** → the `.swap` file is gone **immediately**, before you do anything else. Relaunch → nothing is recovered. Repeat with the whole *window* closing (the sweep that prompts per dirty tab) rather than one tab (TDD 22.2)
- [ ] **22.3** Type, wait for the snapshot, then **Save** → the `.swap` file goes. Repeat with **Save As** to a new name → likewise, and no stale file is left under the old name (TDD 22.3)
- [ ] **22.4** Type, wait for the snapshot, then **undo** back to the on-disk content (Ctrl+Z until the unsaved marker clears) → the `.swap` file goes, without any save (TDD 22.4)
- [ ] **22.5** File ▸ New Document, type into it, never save, wait, `kill -9`, relaunch → the text comes back in a tab that has **no** backing path (Save must prompt for a location, not silently write somewhere) (TDD 22.5)
- [ ] **22.6** Type into a document, wait for the snapshot, then delete `session.toml` from the state dir (simulating a crash that landed between the snapshot and the session write), `kill -9`, relaunch → the document is **still recovered**, into a tab of its own. This is the check that proves the recovery data is authoritative and the session file only advisory (TDD 22.6)
- [ ] **22.7** Type into a document, wait for the snapshot, `kill -9`, then edit the file **outside** the app (append a line), relaunch → the content is recovered **and** the external-change conflict prompt appears — the same one an ordinary outside edit raises, not a second bespoke one (TDD 22.7)
- [ ] **22.8** With two documents dirty in one window, `kill -9`, relaunch → each recovered tab shows its own notice naming the capture time, and the status bar reports "Recovered unsaved changes in 2 documents". Then relaunch after a **clean** quit → neither notice nor status message appears (not "Recovered 0 documents") (TDD 22.8)
- [ ] **22.9** After a recovery, use the notice's **Discard recovery** → the tab reloads from disk, the unsaved marker clears, and the swap dir is empty. Relaunch → nothing comes back (TDD 22.9)
- [ ] **22.10** Drop an unrelated file into the swap dir (`echo hello > "$XDG_STATE_HOME/scribobulate/swap/notmine.swap"`), relaunch → it is **still there, unmodified** (`md5sum` before/after) and nothing was recovered from it. Then truncate a real `.swap` file mid-header and relaunch → it is likewise left in place, with a warning in the log, and no half-recovered document (TDD 22.10)
- [ ] **22.11** Open a `.swap` file in any text editor (including Scribobulate itself) → the first line reads `+++scribobulate-swap 1`, the header names the document's path and capture time, and the document text follows the closing `+++` verbatim. Then create a file whose **name contains a newline** (`touch $'/tmp/we\nird.md'`), open it, type, wait, `kill -9`, relaunch → the content comes back **whole**, not truncated at the point the path would have injected a fence (TDD 22.11; ScrAP-233)
- [ ] **22.12** Type continuously for over a minute without pausing → the document is still snapshotted along the way (watch the `.swap` file's mtime advance), and typing never stutters. Then type a few characters and **immediately** click a menu / switch to another window / switch application → the `.swap` file's mtime updates **at once**, without waiting out the 3 s debounce (TDD 22.12)
- [ ] **22.13** `ls -l "$XDG_STATE_HOME/scribobulate/swap"` under a permissive umask (`umask 0002` before launching) → the directory is `drwx------` and every `.swap` file is `-rw-------`. Check it again after the file has been **overwritten** several times, not only when freshly created — re-applying the original file's mode on overwrite is the failure this catches (TDD 22.13; GTK4Rs/AP-167)
- [ ] **22.14** Launch a second instance with `--new-instance`, make a document dirty in it, wait for its snapshot, then relaunch a *third* instance while the second is still running → the third leaves the second's recovery data alone (its `.swap` file is untouched and its content is not opened in the third). Kill the second uncleanly → the next launch recovers it (TDD 22.14). **On Windows** the liveness probe cannot confirm the second instance at all — a known, still-open limitation there — so the third *does* open a duplicate tab for its content: expected on that platform, not a finding, and not a failure of this check on Linux/macOS
- [ ] **22.15** **A failed snapshot is reported on both surfaces, and the previous snapshot survives.** Point `XDG_STATE_HOME` at a small full filesystem (a `tmpfs` you can fill, or the researcher's `enospc.sh` rig), open a document, type, and let a snapshot succeed first (`ls` the swap dir → a `.swap` file with content). *Then* fill the filesystem and type again → (a) the tab shows a notice and the status bar reads **"Unsaved changes are not being backed up"**, worded about the *safety net* and **not** claiming the document failed to save; (b) 🔴 **the previous `.swap` file is still intact and non-empty** — this is the assertion that matters, because the naive implementation replaces it with a **0-byte file** exactly when the user most needs it (GTK4Rs/AP-167). Keep typing → the notice does **not** re-fire every few seconds. Free space and type again → the notice clears and a fresh snapshot lands (TDD 22.15)
- [ ] **22.16** Open the same file in two windows, make **different** edits in each, wait for both snapshots (`ls` → two `.swap` files), `kill -9`, relaunch → both sets of edits come back; neither overwrote the other (TDD 22.16)
- [ ] **22.17** **Reopening the crashed document by NAME must not produce two tabs.** This is the ordinary post-crash reopen and the route the rest of §22 never takes — every check above relaunches *bare*, which restores the tab from `session.toml` and so correlates the snapshot by identity. Do it the way a user does instead: type into `notes.md` without saving, wait ~4 s for the snapshot, `kill -9 <pid>`, then relaunch **with the file as an argument** (`./target/release/scribobulate notes.md`; on Windows double-click it in Explorer, or `start notes.md`) → the window holds **one** tab for `notes.md`, carrying the recovered text and marked unsaved. Two tabs of the same filename — one clean, one recovered — is the FAIL. Then repeat spelling the argument differently from the stored path: a **relative** path (`cd` to its directory and pass `./notes.md`), through a **symlink**, and on Windows with a different **letter case** (`NOTES.MD`) — one tab each time. Finally the negative: an **untitled** recovery (22.5) plus a file argument must still come back in its own tab, never merged into the argument's document (TDD 22.17)
- [ ] **22.16b** **Two recoveries for one path stay two documents.** Set up 22.16 (same file dirty in two `--new-instance` processes, different edits, two `.swap` files), `kill -9` both, then relaunch **with that file as an argument** → three tabs' worth of content is wrong and so is one: expect the file's tab to hold one recovery and a **second** tab to hold the other, with **both** texts present. The 22.17 fix correlates by path, and this is the boundary it must stop at — silently applying the second snapshot over the first would destroy recovered work, which is worse than the duplicate tab 22.17 removes (TDD 22.16)
- [ ] **22.15b** **A failure notice must not outlive the document it is about.** With snapshotting failing (as 22.15), (a) **close the affected tab** → the status message goes with it; the window must not keep reporting "not being backed up" for a document that no longer exists. Then (b) with another tab failing, **drag it to a second window** → the notice does not stay behind in the origin window. Both are permanent leaks if wrong — nothing retracts the message afterwards, and neither is on the happy path that 22.15 exercises
- [ ] **22.C-A** *(Derived-view CAM row 8, column A)* After a recovery, keep **typing** in the recovered tab → the notice stays up and keeps naming the same capture time. It reports "recovered content you have not saved", and typing more does not make that untrue
- [ ] **22.C-B** *(Derived-view CAM row 8, column B — the cell that found a real defect)* After a recovery, **save** the tab → the notice disappears at once. Confirm it is gone *before* doing anything else; a notice that only clears on the next tab switch is a CAM fail. Repeat with an **external reload** and with **Discard recovery**. **Why this matters:** the notice's button reverts the tab to disk, so a stale one offers to throw away work the user has just saved
- [ ] **22.C-C** *(Derived-view CAM row 8, column C)* With a recovery notice up, switch view mode (preview↔split↔edit), zoom in and out, and toggle the reading theme → the notice survives each, still naming the same time, still actionable. None of these makes the recovery any less unsaved
- [ ] **22.C-D** *(Derived-view CAM row 8, column D)* With two tabs open where only one was recovered, switch between them → the notice appears **only** on the recovered tab and never leaks onto the other. Then **drag the recovered tab to another window** → the notice travels with it and does not stay behind. Close the recovered tab → the notice goes with it
- [ ] **22.C-P** *(Reading-Position CAM row 10)* Recovery applies at startup, when nothing has scrolled yet, so there is no reading position to preserve and no check to run — **but** if a future change recovers into a live session, or restores a caret/scroll position from the swap header, re-read Reading-Position CAM row 10: the `n/a` is about *when* recovery runs, not what it does, and those changes end it
- [ ] **22.D** **A recovery reaches the PREVIEW, not just the editor.** Recover a document (as 22.1) and read it in **Preview** mode *without* switching to Edit first → the preview shows the recovered text. Then switch to Edit and back → still consistent. **Why this check exists:** the first implementation set the editor buffer but not the text the derived views render from, so the editor was right and the preview silently showed the pre-crash file — invisible to the whole automated suite, which asserted the editor. Preview is this application's default mode, so that bug was the feature silently doing nothing for most users
- [ ] **22.E** **A snapshot survives being interrupted.** With a document dirty, `kill -9` the app *while it is actively snapshotting* (type continuously and kill mid-burst; repeat a few times) → on every relaunch the swap file either decodes cleanly or is reported as damaged and **left in place** — never silently half-applied. A snapshot is written to a co-located `.swap.tmp` and renamed into place only after a complete write, so **no torn `.swap` should ever be observable**: an interrupted write leaves a `.tmp` (swept on the next launch — 22.T), never a half-written snapshot
- [ ] **22.T** **Stray snapshot temps are swept, and nothing else is.** Put four files in the swap dir before launching: `x-aaaa.swap.tmp` (ours, incomplete), `unrelated-tool.tmp` (**not** ours), `notmine.swap` (foreign), and a truncated `+++scribobulate-swap 1` file with no closing fence (ours, damaged). Launch → **only the first is gone**. The foreign `.tmp` and `.swap` survive because the state directory is shared and this must never become a general file shredder; the *damaged* one survives because it may be the only remaining copy of the user's work. Then kill the app mid-snapshot a few times and confirm no `.swap.tmp` accumulates across launches
- [ ] **22.L** *(POLICY § Logging — forensic threshold)* Run a recovery with **no `RUST_LOG` set**, then read `scribobulate.log` → there is an `INFO` line per recovered document naming the path and the **byte count**, and **no document text anywhere** (grep the log for distinctive text from the recovered buffer — §21.10's rule applies here too, and this is the one path that handles buffer content). Then leave a document dirty and typing for several minutes, `kill -SEGV` it, and read the crash report's breadcrumb ring → it is **not** filled with snapshot-write records: a periodic event must stay below the forensic threshold, or the ring's 64 slots describe nothing but the safety net


### §23 Back / Forward navigation history

> **Two harness facts that decide how these are run, both measured 2026-08-05 on
> GTK 4.6.9 / Xvfb.**
>
> **(a) Probe the action state by the app's UNIQUE bus name, never its well-known
> one.** The plan's `-n` requirement makes the process `NON_UNIQUE`, so it owns no
> *well-known* name — and `--dest com.extollit.scribobulate` then silently answers
> about whichever instance does hold it (the operator's), replying "The named action
> ('nav-back') does not exist" about an action that is registered and working
> (GTK4Rs/AP-253). It still owns a unique name, and everything works through it:
>
> ```bash
> BUS=$(busctl --user list --no-pager | awk -v p=$APP_PID '$2==p {print $1}')
> OP=/com/extollit/scribobulate/window/1
> gdbus call --session --dest "$BUS" --object-path $OP \
>   --method org.gtk.Actions.Describe nav-back        # → ((false|true, …),)
> gdbus call --session --dest "$BUS" --object-path $OP \
>   --method org.gtk.Actions.Activate nav-back "[]" "{}"
> ```
>
> That makes 23.5's sensitivity checks mechanical — read the bit, drive the action,
> re-read it — with no pointer and no pixels. Functional verification (invoke it and
> see whether the document changed) is the fallback.
>
> **(b) A toolbar chevron is pixel-identical enabled and disabled** (`compare -metric
> AE` = 0 across a real transition, GTK4Rs/AP-67), so never judge greying from a
> toolbar screenshot: use the probe above, or the **View menu**, whose items do grey
> visibly (a menu is a separate surface — `import -window root`, GTK4Rs/AP-134).

- [ ] **23.1** Open three documents in one window (`-n a.md b.md c.md`). Click tab **b**, then tab **c**. Invoke **Back** → **b** is active; again → **a**. Nothing reloads, no other document's scroll moves (TDD 23.1)
- [ ] **23.2** From **a** (after 23.1), invoke **Forward** twice → **b**, then **c**: the exact inverse of the two Back presses (TDD 23.2)
- [ ] **23.3** From **c**, press Back and Forward alternately a dozen times → you keep reaching **a** and **c** at the ends and never get stuck oscillating between two middle documents. **The failure this catches:** if traversal recorded itself, each Back would add an entry and Back/Forward would degenerate into a two-document toggle (TDD 23.3)
- [ ] **23.4** From **c**, Back to **b**, then navigate somewhere NEW instead of going forward — click tab **a**, or follow a link. Now invoke **Forward** → nothing happens; the trail to **c** is gone. The View menu shows Forward greyed (TDD 23.4)
- [ ] **23.5** With no navigation yet (a freshly opened window), **View ▸ Back and View ▸ Forward are both greyed**, and the toolbar buttons do nothing when clicked. Navigate once → Back greys in. At the oldest entry press Back again → **nothing changes**; it must not wrap around to the newest (contrast Previous Tab, which cycles). Check each command from **all three** of: the View menu, the toolbar chevrons, and the keyboard (TDD 23.5)
- [ ] **23.6** Every input drives the same command: **Alt+Left / Alt+Right**; the dedicated **Back/Forward keys** if the keyboard has them (`XF86Back`/`XF86Forward`); and the **two mouse thumb buttons** (`xdotool mousedown 8` / `mousedown 9`, or a real thumb switch — press it over the document area, over the toolbar, and over the tab strip; all three must navigate). Then open **Help ▸ Keyboard Shortcuts** → the View group lists Back = Alt+Left and Forward = Alt+Right, and the mouse buttons are deliberately absent (TDD 23.6)
- [ ] **23.7** Two windows, each with two documents visited. Invoke Back in window A → only A's document changes; B's active document and its own Back/Forward availability are untouched. No traversal ever activates a document living in the other window (TDD 23.7)
- [ ] **23.8** **A document that leaves the window leaves its history.** (a) Visit **a → b → c**, then **close b** (its × button, so the active tab is not the one closing) → Back from **c** goes to **a**, never to a closed document and never producing a Back press that visibly does nothing. (b) Visit **a → b**, then close **a** → Back is greyed on **b**. (c) Visit **a → b → c**, then **drag b out to its own window** → in the origin window Back from **c** reaches **a**; in the new window Back is greyed. (d) Close the ACTIVE tab and confirm that landing on its neighbour did *not* add a history entry — Back must not return you to the tab you were just on (TDD 23.8)
- [ ] **23.9** **Internal page switches are not navigations.** With **three** documents where two are untitled and dirty, invoke **Save All** and cancel each chooser → the sweep switches tabs to show you each one, and when it finishes the history is unchanged: Back goes where it went before the sweep, not on a tour of the prompted tabs. Repeat with **Close Other Tabs** on a window with two dirty others (Discard each). Then restart into a **restored session** (23.10) and, separately, trigger a **startup crash recovery** (§22.1) → neither leaves a history entry behind (TDD 23.9)
- [ ] **23.10** **Session-local and bounded.** Visit several documents, close the window, relaunch → the restored window's Back and Forward are **both greyed**, and the first navigation you make goes back to the *restored* document (not to whichever tab the window happened to be built with). There is no persisted history in `session.toml` — `grep -i nav ~/.local/state/scribobulate/session.toml` finds nothing (TDD 23.10)


## 4. Additional test scenarios (not yet in TDD.md)

These are candidate rubrics surfaced while planning this pass — not yet
contractual. Run them; if they hold and the operator agrees they're worth
locking in, promote the passing ones into `sdd/TDD.md` (new subsection per
area) rather than leaving them only here, since TDD.md is the living contract
and this file is just the run-sheet.

### 4.1 Live reload — beyond simple append (extends §3)
- [ ] External process **replaces the file via rename** (`mv new.md file.md` —
  changes the inode, unlike in-place `>>`/`sed -i` on some filesystems) → still
  detected and reloaded (rename-based atomic saves are how many editors,
  including this app's own `atomic_io::write_atomic`, actually save — confirm
  the *watcher* handles being pointed at a replaced inode, not just content
  changes to the same one)
- [ ] External process truncates the file to 0 bytes → treated as a content
  change (empty doc), not confused with 3.4's deletion path
- [ ] File's parent directory is removed and recreated with the same filename
  while open → watcher recovers or fails gracefully (not a silent stuck state)
- [ ] File permissions changed to unreadable while open (`chmod 000`), then an
  external reload is attempted → error surfaced, not a crash or silent no-op
- [ ] External edit arrives **while the save-conflict dialog (5.1) is already
  open** → second conflict doesn't stack a duplicate dialog or corrupt state
- [ ] Live reload against a file on a slow/networked mount (or simulate with
  `inotifywait` latency) → no premature partial-content read (torn read)

### 4.2 Saving — failure paths (extends §4)
- [ ] Save to a path whose directory is read-only (`chmod 555` the dir) →
  error surfaced to the user, editor content NOT lost
- [ ] Disk full during save (e.g. write into a small tmpfs) → error surfaced,
  original on-disk file not left half-written (covers `atomic_io`'s
  write-temp-then-rename claim under real ENOSPC, not just a crash-simulation)
- [ ] Save a document containing a `\0` byte or invalid UTF-8 pasted from
  another app → doesn't panic; either sanitizes or errors cleanly
- [ ] Round-trip CRLF (`crlf-doc.md`): open, edit, save → line endings not
  silently converted to LF (or if they are, that's a documented decision, not
  an accident — confirm which and note it)
- [ ] Round-trip a file with no trailing newline → save doesn't add one
  unasked (or confirm the deliberate normalization if it does)

### 4.3 Tabs/windows — scale and races (extends §7/§15)
- [ ] Open enough tabs to overflow the strip (15-20+) → tab strip
  scrolls/overflows usably (prev/next chevrons appear only while overflowing,
  only on the side scrolling is possible), no layout breakage, switching
  stays responsive; **check the terminal log for
  `Gtk-WARNING: Trying to snapshot … without a current allocation`** —
  none should appear at window-open OR while switching/scrolling tabs (a
  real bug reproduced this reliably on a live X11 desktop with enough
  overflowing tabs, invisible in a 2-3-tab Xvfb session — GTK4Rs/AP-104)
- [ ] Rapid-fire Ctrl+W across many tabs (script it) → no crash, no
  double-free-style GTK-CRITICAL, final tab count matches
- [ ] Open the same file via two different path spellings that resolve to the
  same inode (a relative path vs. its absolute form, or through a symlink) →
  treated as "already open" (15.16), not two independent tabs on one file
- [ ] Drag a tab across a virtual-desktop/workspace switch mid-drag → doesn't
  strand the drag or crash (WM-dependent; note the WM used)
- [ ] Close the app via `SIGTERM` (session logout, not the window's own quit
  path) with unsaved edits present → either prompts (if the session manager
  allows) or fails safe (does not silently discard — check what actually
  happens, since 7.4's prompt path assumes a normal quit)

### 4.4 Security / robustness (extends §2.7)
- [ ] Image path with a symlink INSIDE the doc folder pointing OUTSIDE it →
  blocked (the canonicalization must resolve the symlink, not just string-match
  the nominal path — TDD 2.7 says this explicitly, worth a dedicated fixture)
- [ ] Remote image toggle ON, image URL targets a private/link-local address
  (`http://169.254.169.254/...`, `http://127.0.0.1:<port>/...`) → expected to
  **succeed by design**: the opt-in remote-image path applies no private-IP/SSRF
  filtering (README "Show Unsafe Images" security note). This is the accepted
  design — the control is user trust/due-diligence — not a bug to report
- [ ] A Markdown file crafted with thousands of nested list items or a huge
  single table (stress/DoS-style) → app stays responsive or fails predictably,
  doesn't hard-hang the main loop

### 4.5 Accessibility & input diversity
- [ ] Full keyboard-only pass: open, edit, format, save, switch tabs, navigate
  outline, use find — without touching the mouse at all
- [ ] Tab order through toolbar/menu/panes is sane (no focus trap, no
  skipped/duplicate stops)
- [ ] Run under a screen reader (Orca) briefly — toolbar buttons and menu items
  have sensible accessible names (not blank or "button 1")
- [ ] High-contrast desktop theme → text stays legible (companion to 2.13/2.14
  which cover light/dark but not high-contrast specifically)
- [ ] Filename/path containing Unicode (e.g. `tests/fixtures/emoji-and-unicode.md`
  with actual emoji or accented characters in the STEM, not just content) →
  opens, title bar renders it correctly, saves back correctly

### 4.6 Process & environment
- [ ] Launch with a directory path instead of a `.md` file → clean error, not
  a panic or silent no-op
- [ ] Launch with a `.md` path containing shell-special characters (spaces,
  `$`, quotes) passed correctly quoted → opens correctly (regression guard for
  any place a path gets shelled-out or globbed internally)
- [ ] `RUST_LOG=trace` for a short session → no panic from verbose logging
  paths, log stays legible (spot-check, not exhaustive)
- [ ] Missing `libgtksourceview-5` types (simulate by checking startup on a
  minimal container, if available) → fails with a clear message, not an
  opaque dynamic-link crash (dev-environment sanity, low priority)

### 4.7 Session file corruption / migration
- [ ] Session file: `sed -i 's/windows/xxindows/' ~/.local/state/scribobulate/session.toml`
  (breaks the v1/v2 migration marker per `session.rs`'s doc comment) → app
  starts with sane defaults rather than crashing on a malformed key
- [ ] Truncate `session.toml` to a syntactically invalid TOML fragment → falls
  back to defaults per `parse()`'s `unwrap_or_default()`, doesn't crash
- [ ] Delete `session.toml` entirely, relaunch → first-run default window
  appears, no crash, and normal quit recreates the file

---

## 5. Time budget & recommended split

Rough estimate for a full pass through §§1-4, based on item counts (~139
checklist items in §3 across the 15 TDD sections, ~31 edge-case scenarios in
§4, 11 fixtures to create first):

| Phase | Items | Avg time/item | Subtotal |
|---|---|---|---|
| Create missing fixtures (§2) | 11 files + 1 script | ~5-10 min | 1-1.5 hr |
| §3 core checklist (screenshot-verified) | ~139 | ~2-4 min (many share one running instance) | 5-8 hr |
| §4 edge cases (disk-full, corruption, SIGTERM, a11y, drag races) | ~31 | ~10-15 min (each needs its own setup) | 5-7 hr |
| §3's TDD §6 footprint gate | 3 | ~15 min | 45 min |
| Checkbox/write-up pass (§7 below) | — | — | 30-60 min |

**Total: ~13-18 hours of focused work — realistically 2-3 working days**, not
a single sitting. What inflates it beyond the raw item count:

- Multi-window/drag scenarios (7.6-7.10, and §4.3's tab-scale/race items) are
  slow to set up and re-verify each time — each needs two windows positioned
  and a real drag gesture, not just a click.
- Screen-reader and high-contrast checks (§4.5) aren't scriptable at all —
  genuinely manual, no shortcut.
- Session-corruption tests (§4.7) each require editing a file, relaunching,
  and confirming recovery — can't batch across items.
- If an agent is driving the `xdotool`/`import` loop, reading ~139+ screenshots
  to verify each item also burns significant context — a single session
  realistically cannot complete this whole document; budget accordingly.

**Recommended split — three sessions, not one:**

1. **Fixtures + §3 §1-§5** — data-loss-critical paths (open/save/live-reload/
   reconciliation). Do this first; it's the highest-consequence area.
2. **§3 §6-§13** — footprint gate, window/layout, menu/toolbar/actions,
   formatting, find, outline, zoom. The bulk of the UI surface.
3. **§3 §14-§15 + all of §4** — unsafe images, tabs, and the edge-case/
   robustness scenarios. Do this last; it's the slowest per-item and the
   least likely to regress silently between releases.

### 5.1 Execution order by automation-fitness (run the machine-friendly work first)

The split above is ordered by *consequence*. When running **unattended or
machine-driven**, overlay a second ordering by *automation-fitness* — do the
sections that drive reliably (keyboard / file / shell, deterministic verdicts)
**first**, and defer the cumbersome ones (mouse-drag, popover/tab activation,
desktop-theme toggles, window resize) to **last**, ideally handing them to a
human driver or a GNOME + floating-WM host. Rationale and the specific host
limits are recorded in `tests/reports/MANUAL-TEST-REPORT.md` (§3 environment limits and
§3b workflow problems).

**Tier 1 — unattended-friendly, do first** (shell / keyboard / file-driven;
verdicts are deterministic):
- §6 footprint gate (pure shell — nvidia-smi / `/proc` / source grep; zero UI).
- §3 live-reload and §5 reconciliation (external file edits + toasts/dialogs).
- §4 editing & saving (keyboard typing + Ctrl+S + `cat`/`stat`).
- §11 find/replace and §10 formatting and §13 zoom (accelerators: Ctrl+F/H,
  Ctrl+B, Ctrl+±/0; keyboard-nav popovers).
- §1 open/display and §9 menu/actions (CLI launch + accelerators + action
  sensitivity).
- §8 single-instance lifecycle (CLI + PID checks) — **requires being the sole
  primary**: close any other `scribobulate` instance first (same app-id).
- The file/shell-shaped §4 edge cases: save-failure paths, live-reload edge
  cases, session-file corruption/migration, process/env launches.
- §21 crash forensics (log files, `kill -SEGV`, report contents) — pure shell and
  file inspection, and it needs no WM at all.

**Tier 2 — cumbersome, do last (or hand to a human / GNOME+floating-WM host):**
- §7 window & layout — needs real window **resize** (a tiling WM refuses it) and
  cross-window **tab drag**.
- §12 outline — needs reliable **row activation** clicks (synthetic clicks are
  unreliable; see report L-3).
- §14 show-unsafe-images and the §2 **theme** items (2.13/2.15/2.19-live) — need a
  live **desktop light/dark toggle** the app actually follows.
- §15 tabbed documents — the **drag/reorder/detach** items specifically.
- §4.5 accessibility (screen-reader / high-contrast) — genuinely manual.

**Mixed sections:** §2 rendering is mostly Tier 1 (render-and-look), but its
link-click (2.6/2.9/2.12/2.17) and theme (2.13/2.15) items are Tier 2 — run the
visual/keyboard items early and defer those. Note also that a **crash or a
Tier-2 blocker can gate a Tier-1 item** (e.g. a find-then-tab-switch crash blocks
§13 zoom) — fix known crashers before an unattended run.

---

## 6. Orchestrated multi-agent execution (splitting the load across sessions)

§5 established that a full pass is 13-18 hours and cannot fit in one agent
session's context window (reading ~139+ screenshots alone exhausts it). This
section is the concrete protocol for running it as multiple **sequential**
agent sessions coordinated by one orchestrator, instead of one agent pushing
through the whole document and losing context partway (or, worse, silently
skimming later sections once its budget gets tight).

### 6.1 Why sequential, never parallel

This is a **different shape** from this project's prior sub-agent experiment
(parallel implementation phases, each in its own git worktree, graded and
merged independently). That pattern's isolation came from the worktree: each
sub-agent had its own filesystem checkout, so concurrent writes never
collided. **GUI testing has no equivalent isolation.** Every chunk drives the
same X display via `xdotool`/`import` — one mouse, one keyboard, one focus,
typically one running app instance. §1's dev-loop reference is explicit that
`xdotool` "drives the operator's real mouse/keyboard — there is no isolated
virtual display." Two sub-agents driving input at the same time would
interleave clicks/keystrokes into whichever window happens to hold focus at
that instant — indistinguishable from a real app bug until you realize two
agents were fighting over the pointer. **Chunks run one at a time, each to
full completion — including its own cleanup, no stray processes left running
— before the next one starts.** The orchestrator enforces this by waiting for
each chunk to fully finish before launching the next; never fire off
multiple chunk sessions "in the background" against the same display.

A second, independent reason to run chunks sequentially even if the display
contention above were somehow solved: **token consumption velocity.** Each
chunk is screenshot-heavy (every visual check reads an image back, and
multimodal image reads are token-expensive) and runs its own build/test
cycles — that's real, sustained token spend, not a burst. Several chunks
running concurrently multiply that spend rate rather than just its total,
and a Claude usage window has a rate ceiling over time, not just a total-size
ceiling — so parallel chunks can burn through the window's budget and get
rate-limited or cut off mid-chunk, well before the same total work done
sequentially would. Running one chunk at a time keeps token velocity within
whatever the operator's usage window allows, even though the wall-clock time
to finish all chunks is the same either way.

A **third** reason, learned the hard way: **memory pressure.** Each chunk's app is
its own `Xvfb` + WM + release binary (~180–210 MB RSS); several at once can cross a
host's low-memory threshold and get an instance killed by `earlyoom`/the OOM killer
mid-test (see §1.9's warning — presents as a signalled exit with empty stderr, not a
crash, and a kill during a modal grab can wedge the whole display). Sequential runs
keep the resident footprint to one instance. **If you disregard this and parallelize
anyway**, note that `pgrep -x scribobulate | head/tail` is then **unreliable for
finding "your" PID** — it returns every concurrent instance, so you can track or kill
a *peer's* process; disambiguate by the launch display (`grep -z DISPLAY /proc/<pid>/environ`,
matching your `:NN`) or capture the PID from the launch itself (a before/after `pgrep`
diff), never by a bare name match.

### 6.2 Chunk boundaries

Default to the three-session split from §5 (fixtures + §3 §1-§5, §3 §6-§13,
§3 §14-§15 + all of §4) — it's already ordered by consequence (data-loss
paths first) and the three chunks are roughly comparable size (~5-8 hours
each). For finer-grained chunks — a chunk still proves too large for one
sub-agent's context, or you want more/faster checkpoints — split further
along existing TDD section boundaries. Never split a single TDD section's
own sub-rubrics (e.g. 15.1-15.16) across two chunks: they usually share
setup state (the same two-window/two-tab scenario), so splitting them forces
the second chunk to rebuild context the first chunk already had live.

### 6.3 Phase 0 — fixtures + run-sheet (orchestrator does this itself, once)

Run the fixture-creation step (§2) **before** spawning any chunk sub-agent,
and do it directly rather than delegating it to chunk 1 — every chunk
depends on the fixtures, so creating them once upfront avoids (a) chunk 1
spending part of its budget on fixture creation instead of testing, and (b)
any risk of two chunks racing to create the same file. **Commit the fixtures**
so every chunk starts from a known set.

Also in Phase 0, **create the shared mutable run-sheet** the chunks record into
(this file, MANUAL-TEST.md, is the immutable template — see the header — so it
is NOT the run-sheet). Either derive a scratch checklist from this document's
item list, or designate the per-item results section of
`tests/reports/MANUAL-TEST-REPORT.md` as the run-sheet. Chunks append their verdicts
there; on a wedge/crash a relaunched chunk **resumes from the last recorded
item** (§1.9) rather than repeating or skipping work. Append the reproducible
run trail (binary/build, PIDs, WIDs, screenshots, cleanup) to
`tests/reports/MANUAL-TEST-AUDIT.md` as you go — both files live in
`tests/reports/`, which is git-ignored (see the header note).

### 6.4 Sub-agent brief template

Every chunk sub-agent needs a brief covering all of the following — adapt
the bracketed parts, keep the rest:

```
You are executing ONE CHUNK of tests/MANUAL-TEST.md for the Scribobulate
project at <repo path>. Fixtures already exist (§2) — do not recreate them.

Your chunk: <exact item list, e.g. "§3 §6-§13 — TDD sections 6 through 13,
the footprint gate through preview zoom">.

Before starting:
- Read tests/MANUAL-TEST.md §1 (dev loop) in full — it has hard safety
  rules (track PID, never pkill by name, one window close at a time in a
  shared session, re-focus before every xdotool call, never --window-target
  a key send near an open popover/menu, default to a private Xvfb display
  per §1.10). Follow them exactly.
- Read 'Automated UI Testing' from the GTK4-Rs skill for the
  full version of those rules with the rationale/citations behind each.

For each checklist item in your chunk:
- Perform the action; capture a screenshot when the item is visual.
- Record the verdict in the SHARED RUN-SHEET / results file — NOT in
  tests/MANUAL-TEST.md (that file is an immutable template; see its header):
  `N.N — PASS <date>`, `— FAIL <date>: <what happened>`, or
  `— INCONCLUSIVE / BLOCKED / NOT-RUN <date>: <why>`.
- Note anything surprising even when it's not a failure (friction, an
  ambiguous result, a pre-existing gap) — same bar as the verify skill:
  if it made you pause, it's worth a line.

Boundaries:
- ONLY record results for items within your assigned chunk, in the shared
  run-sheet. NEVER edit tests/MANUAL-TEST.md (the immutable template). Don't
  touch other chunks' rows, other files, or fix anything you find broken —
  report it, don't fix it (that's a separate task after the full pass, reviewed
  centrally).
- Before you finish, confirm no scribobulate process you started is still
  running (`pgrep -a scribobulate` should show none of your test PIDs — an
  operator's own pre-existing instance, if any, is not yours to touch or
  count against this check).

When done, report: items run, pass/fail/blocked counts, and any finding that
looks like a genuine new bug (flag it clearly — don't file it in
ANTI-PATTERNS.md yourself; that's the orchestrator's job after reviewing all
chunks together, §6.6).
```

### 6.5 Spawning mechanism

This harness has no in-process "spawn a sub-agent" tool available, so realize
"sub-agent" as a **separate Claude Code session**, launched by the
orchestrator via the Bash tool in non-interactive/print mode, one chunk at a
time:

```bash
claude -p "$(cat chunk-2-brief.txt)" > chunk-2-report.txt 2>&1
```

Confirm the exact non-interactive flag against the installed `claude --help`
— it may differ across CLI versions. A single `Bash` tool call blocks until
the invoked process exits, which is what enforces "one chunk at a time" — do
not background it (`&`) or launch the next chunk's call before this one's
tool result returns.

A non-interactive session cannot answer an interactive permission prompt, so
either the project's `.claude/settings.json` must already allow the tools a
chunk needs (`Bash` for `cargo`/`xdotool`/`import`, `Read` for screenshots,
`Edit`/`Write` for recording its results in the shared run-sheet — see the
`fewer-permission-prompts` skill), or a human runs each chunk's brief in an
interactive session instead
of scripting it. Either way, the brief template and boundaries in §6.4 are
identical; only the launch mechanism changes.

### 6.6 After each chunk

Before launching the next chunk, the orchestrator:
- Confirms the prior chunk's results actually landed in the shared run-sheet /
  `tests/reports/MANUAL-TEST-REPORT.md` (diff it) — never in `tests/MANUAL-TEST.md`,
  which stays an untouched template — and that no stray process is still running.
- Reviews any flagged findings; files genuinely new pitfalls in
  `sdd/ANTI-PATTERNS.md` **itself**, centrally — even though chunks run
  sequentially rather than concurrently, this keeps one writer for that
  file instead of every chunk touching it, matching ANTI-PATTERNS.md's own
  note that it's edited only by the maintainer agent.
- Only then launches the next chunk.

### 6.7 Aggregation

Once every chunk has run, do the §7 "After a full pass" close-out **once**,
covering the whole document — not per chunk.

---

## 7. After a full pass

- Record every item's verdict — date + PASS / FAIL / INCONCLUSIVE / BLOCKED /
  NOT-RUN + a one-line note — in `tests/reports/MANUAL-TEST-REPORT.md` (its per-item
  results section), **not** by ticking boxes in this file (this document is the
  immutable template — see the header note). Don't leave stale results in the
  report from a prior binary/commit. Log the reproducible run trail (binary,
  PIDs, WIDs, screenshots, cleanup) in `tests/reports/MANUAL-TEST-AUDIT.md`.
- Any genuinely new pitfall discovered while running this (not already in
  `sdd/ISSUES.md` or `sdd/ANTI-PATTERNS.md`) → file it there, not just here;
  this document is the run-sheet, ISSUES.md/ANTI-PATTERNS.md are the living
  record.
- A §4 scenario that passed and the operator wants as a permanent contract →
  move it into `sdd/TDD.md` under a new or existing section number, then
  delete it from §4 here (it's now covered by the checklist in §3).

---

## A. Platform procedures

Every instruction in this document that differs per operating system lives here.
The preamble explains *why* the split exists and what it protects against; this
section is the reference itself.

**Scope.** Per-platform *mechanics* only: how to drive the app, how to
launch/kill/install it, how to read its logs, and what the session must provide.
Not which checks apply — an item impossible on a platform says so at the item, as
a declarative gate (`7.10 (X11 only)`), because that is a statement about the
*behaviour under test*, not about the harness.

**Expressing a per-platform difference in an ITEM — one rule.** Default to an
**inline branch** (`*Linux*: … *macOS*: …`) when the check is identical and only the
driving command differs; that is the overwhelmingly common case. Reach for a
**`m`-suffixed sibling item** (`2.15m`, `8.2m`) only when the *mechanism under test*
— not merely the tool driving it — exists on one platform alone. Testing the same
behaviour through a different tool is not a different test.

**Completeness check:** every subsection answers the same seven headings, in the
same order. A platform that cannot do one answers it with **not available** and why
— never by omitting the heading, so a reader can always tell "no procedure exists"
from "nobody wrote it down".

  1. Session prerequisites · 2. Drive loop · 3. Launch & instance identity ·
  4. Force-kill · 5. Install · 6. Tokened (desktop-integrated) launch ·
  7. Reading the GTK log

---

### A.1 Linux (X11) — the reference platform

**1. Session prerequisites.** X11 (`echo $XDG_SESSION_TYPE`); on native Wayland
prefix `GDK_BACKEND=x11` to force XWayland. Choose Xvfb vs the operator's real
session per **§1.10** — that decision is per-item and several items require a real
WM, which a bare `Xvfb` does not provide.

**2. Drive loop.** **§1 of this document is the Linux drive loop** — launch/PID
discipline, window lookup, screenshot, input delivery, cleanup, wedge detection. It
is not duplicated here; §1 is the copy, and its own canonical source is the
`gtk4-rs` skill's *Automated UI Testing* module.

**3. Launch & instance identity.** `./target/release/scribobulate <file>`; always
`-n` except where an item explicitly tests the forwarding path. Confirm how many
instances are live with `pgrep -a scribobulate`; scope every window lookup to the
PID you launched (`xdotool search --pid`), never to a name or class.

**4. Force-kill.** `kill -9 <pid>` — the PID you launched, never a pattern match.

**5. Install.** `./install.sh`, then check the title bar, the taskbar, and Alt-Tab.

**6. Tokened (desktop-integrated) launch.** `gio open <file>`, or open it from the
file manager. The distinction matters: a tokened launch carries a startup token and
may raise the window, where a bare terminal launch trips focus-steal prevention and
substitutes a taskbar flash.

**7. Reading the GTK log.** Warnings arrive on stderr as `Gtk-WARNING **:`; grep
that token literally.

---

### A.2 macOS (Quartz)

> **Certification status: certified by the macOS operator**, with one exception
> called out in place — *Reading the GTK log* is expected-but-unverified, because no
> GTK warning was ever reproduced there to grep. Every other step below was either
> run directly during the macOS pass or confirmed by its operator on review.
> GTK4Rs/AP-163 is the full write-up,
> including what was tried and why each alternative failed; this is the operational
> distillation. Live-verified on GTK 4.22.4 via this procedure: TDD 17.7, 17.8,
> 17.47–17.50.

**1. Session prerequisites.** A real, unlocked desktop session — there is no
headless equivalent of Xvfb here, so every item runs against the live machine.

**Install `cliclick` first (Homebrew) — it is a requirement, not a preference.**
Raw `osascript`/System Events does post real clicks, but it has **no
move-without-click primitive**, so calibration would degrade into
click-screenshot-adjust-click-again — and every one of those trial clicks can
actually trigger something in the app. `cliclick`'s `m:x,y` is what makes
calibration *non-destructive*, which is the entire point of the discipline.

Then two countermeasures, which are **continuous, not one-time setup**:

- **Defeat the idle lock.** `caffeinate -disu &` once at session start, disowned so
  it survives, then left alone for the whole session. It is a global system-level
  assertion, not scoped to a process tree, so there is no need to wrap it around any
  particular command. An agent driving purely through CLI/AppleScript generates no
  real HID input, so the OS reads the session as an idle human and locks it out from
  under the run — as short as 2–3 minutes. When a click or capture starts failing
  mid-session, check `ioreg -n Root -d1 | grep CGSSessionScreenIsLocked` **before**
  concluding the app is at fault.
- **Reassert frontmost before every click**, in the same shell invocation as the
  click: `osascript -e 'tell application "System Events" to set frontmost of process "<name>" to true'`.
  The controlling terminal is a real, focusable window that can regain frontmost
  status between tool-call turns, and a click issued without re-asserting can land
  on the terminal's own transcript — which then appears in the screenshot and reads
  convincingly as bogus application state.

**2. Drive loop.**

- **Window geometry** — `System Events`' `position of window 1` / `size of window 1`,
  in **points**.
- **Screenshot** — `screencapture -R<x>,<y>,<w>,<h>` for a window crop, `-C` to draw
  the cursor in. The image is in **pixels**; positions and clicks are in **points**.
  Every coordinate derived from a screenshot must be divided by the display's scale
  factor or it lands off-target.
- **Measure that scale factor; never hardcode 2×.** Divide the screenshot's pixel
  dimensions by the window's point dimensions, per axis, once per session — e.g. a
  window reported 1235×752 by `System Events` producing a 2470×1504 PNG is 2.0, but
  a non-Retina display or an unusual external-monitor scale will not be, and a
  hardcoded constant fails there silently. Read the PNG's dimensions with
  `sips -g pixelWidth -g pixelHeight <file>`.
- **Click — calibrate per TARGET, before the first click on it.** Compute the
  candidate point, move the cursor *only* (`cliclick m:x,y`), capture with
  `screencapture -C`, and confirm the cursor sits on the intended target **before**
  issuing the click. The lifetime is per *target*, not per click and not per
  session: a fixed element (a toolbar icon) stays calibrated for the rest of the
  session, but anything whose position depends on scroll, window size, or what was
  just built — a popover, a list row, a card — needs a fresh check before the first
  click on it.

  **Never record a calibrated coordinate in this document.** It is valid only for
  the window size, DPI and content that produced it; items point at this discipline,
  never at an x,y.

  This is not a fallback technique, it is the only reliable path — and it is also
  the **first thing to suspect** when something looks wrong. A mis-converted
  coordinate does not fail visibly: it lands somewhere else, often on a window
  behind the app, and the result impersonates a different bug entirely. Two separate
  "focus theft" investigations during the macOS pass turned out to be exactly this —
  raw thumbnail pixels used without the Retina conversion, landing on the terminal
  behind the app. **Check the arithmetic before believing any other diagnosis.**
- **Click / type / keys** — prefer `cliclick` (Homebrew) over raw `osascript`;
  same event-posting mechanism, but its move-only mode is what makes calibration
  practical.
- **Scroll** — **not available** in either `System Events` or `cliclick`; neither
  exposes a wheel primitive. Synthesize via JXA, with the cursor positioned over
  the target widget first (GTK routes wheel events to what is under the pointer,
  not to the focus widget):
  `osascript -l JavaScript -e 'ObjC.import("CoreGraphics"); var e = $.CGEventCreateScrollWheelEvent($(), $.kCGScrollEventUnitLine, 1, delta); $.CGEventPost($.kCGHIDEventTap, e);'`
- **Accessibility-based driving — not available.** GTK's Quartz backend exposes
  only the native window-chrome buttons to `NSAccessibility`; none of the app's own
  widgets appear in the tree. Clicking by stable AX reference — the platform-idiomatic
  approach that would sidestep coordinate math entirely — is not possible for this
  toolkit. Raw calibrated screen coordinates are the only path. The same limit rules
  out anything driven through the *native* menu bar (`System Events` →
  `click menu bar item` returns AppleScript error `-1728`): this app draws its own
  in-window menu row, which is not a native menu at all.
- **Driving or reading a GAction over `org.gtk.Actions` — not available.** There is
  no session bus on this platform, so §1's advice to read an action's enabled bit
  over D-Bus has no macOS equivalent. Use the functional route §1 offers alongside
  it: invoke the action and observe its effect (a disabled action-button silently
  ignores the click; an enabled one acts).

**3. Launch & instance identity.** `./target/release/scribobulate <file>`. Note
macOS has **two independent launch paths** that share no mechanism — the terminal
path (`src/platform/mac/single_instance.rs`, an `flock`-elected primary plus a
`$TMPDIR` Unix socket; `RUST_LOG=info` prints `single-instance: primary, listening on …`)
and the LaunchServices path (Finder / `open -a`), which reuses a running bundle
without any app code running. **One passing says nothing about the other** — this is
why item 8.2m exists. Count instances with `pgrep -a scribobulate`.

**4. Force-kill.** `kill -9 <pid>`.

**5. Install.** `packaging/macos/bundle.sh`, then
`open target/macos/Scribobulate.app --args "$PWD/<file>.md"` — an **absolute** path,
because a bundle does not inherit the shell's working directory.

**6. Tokened (desktop-integrated) launch.** Finder ▸ Open With, or
`open -a Scribobulate.app <file>`.

**7. Reading the GTK log.** **Expected, but unverified on this platform** — no GTK
warning has actually been reproduced and grepped here, so this is analogy from
Linux, not observation. Expect `Gtk-WARNING **:` on stderr; the token is what
several items grep for literally, and a build that changes the glib log format
breaks them. Confirm it the first time a warning is genuinely triggered on macOS,
and strike this caveat then.

---

### A.3 Windows (Win32)

> **Status: established, and exercised.** Built and used on the Windows seat while
> driving the §22 crash-recovery pass; every step below except *Install* was run
> directly. It needs **no installed tooling** — all of it is stock Windows
> PowerShell, which is the reason it exists at all: an `xdotool`/`import` equivalent
> was never going to arrive. **ScrAP-236** is the full write-up of the capture trap
> and why the screenshot half is part of the loop rather than an optional extra;
> this is the operational distillation. Live-verified through this procedure:
> §22 recovery-on-relaunch, the recovered-content and notice checks, the two-instance
> liveness degradation, and the snapshot-write-failure path.

**1. Session prerequisites.** An **interactive console session** — confirm with
`query session` (look for `console … Active`). A service/session-0 context has no
desktop and every step below silently addresses nothing. No idle-lock countermeasure
has been needed here, unlike the macOS hazard, but this is a *shared, real* desktop:
the app's windows appear on the operator's screen and keystrokes go to whatever holds
focus, so warn them before a live run and prefer `cargo test` whenever it would prove
the same thing.

**Before any driven run, check the BINARY, not just the tree.** `cargo test` proves
the working tree and says nothing about `target\release\scribobulate.exe`. Confirm
`(Get-Item target\release\scribobulate.exe).LastWriteTime` is newer than the commit
under test, and rebuild release if it is not — a stale binary yields a confident,
entirely wrong result, and it bites hardest on the change whose subject is the code
path being tested (see also §22's pre-flight).

**2. Drive loop.** Four parts, all stock PowerShell.

*Activate — and gate everything else on its boolean:*

```powershell
$ws = New-Object -ComObject WScript.Shell
if (-not $ws.AppActivate($p.Id)) { 'ACTIVATE FAILED'; exit 1 }
Start-Sleep -Milliseconds 700
```

`AppActivate` returns `$true`/`$false`. **Never type or capture without checking it** —
unfocused, keystrokes land in the operator's other windows.

*Type:* `Add-Type -AssemblyName System.Windows.Forms`, then
`[System.Windows.Forms.SendKeys]::SendWait("text{ENTER}more")`. Modifiers are
`%`=Alt, `+`=Shift, `^`=Ctrl; `^{END}` goes to end of buffer.

*Capture — activate, settle, THEN capture:*

```powershell
Add-Type -AssemblyName System.Drawing
if (-not ('P.W' -as [type])) { Add-Type -Namespace P -Name W -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
public struct RECT { public int L, T, R, B; }
'@ }
$r = New-Object 'P.W+RECT'; [void][P.W]::GetWindowRect($p.MainWindowHandle, [ref]$r)
$bmp = New-Object System.Drawing.Bitmap ($r.R-$r.L), ($r.B-$r.T)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
```

*Cleanup:* see step 4.

**Three traps, each of which has cost a cycle here:**

- **`CopyFromScreen` reads the SCREEN at those coordinates, not the window's pixels**
  (**ScrAP-236**). Capture an unfocused window and you get a valid, correctly-sized
  screenshot of whatever is in front of it. Always activate and settle first, and
  capture the **window rect only** — a full-desktop grab also photographs the
  operator's unrelated applications.
- **The app opens in PREVIEW, where keystrokes go nowhere.** Send `%+e`
  (Alt+Shift+E, `win.view-mode::edit`) first. You are in the editor when you see the
  line-number gutter and a `Ln n, Col n` status. Skipping this produces "keys sent,
  buffer not dirty", which reads as an application defect and is a harness gap —
  GTK4Rs/AP-163's shape, and a screenshot settles it in one step. **That is the argument
  for the capture half being part of the loop rather than an optional extra.**
- **`Add-Type -PassThru` returns an ARRAY.** Index it, or reference the type by name.

**3. Launch & instance identity.** `target\release\scribobulate.exe <file>`, or via
`Start-Process … -PassThru` to get the PID directly. Enumerate with
`Get-Process scribobulate | Select-Object Id, MainWindowTitle` — the window title is
often the cheapest assertion available and worth reading before reaching for a
screenshot. **Read it through a FRESH `Get-Process` every time, never off a held
`$p`:** `System.Diagnostics.Process` caches `MainWindowTitle` on first access and only
`$p.Refresh()` clears it, so a driver that keeps `$p` across an action reads the
PREVIOUS title and grades the previous state — **ScrAP-252**'s shape, a probe that
answers self-consistently for the wrong moment (MEASURED on Win10 19045 / PowerShell
5.1: a held object reported the old caption while `GetWindowTextW` already returned the
new one). This became live the moment a plain **tab switch** started retitling the
window (TDD 15.7): nothing in this harness was edited on the day it became wrong. The
piped form above is safe precisely because it builds a new object per call; do not
"optimise" it into a variable. Scope every lookup to the PID you launched, never to the image name: the
operator may have their own instance open. A second bare launch joins the first; use
`-n` (`--new-instance`) when an item needs two real processes. **It joins via a D-Bus
session bus, not via any Win32 backend — GIO has none** (ScrAP-249). GLib autolaunches
the bus by spawning `gdbus.exe` from beside the loaded GLib DLL, so in the dev tree it
works only because gvsbuild's `bin` is on `PATH`. That is exactly why single-instance
items verified here say nothing about the **installed** app, and why item 8.2s below
re-runs them against the staged tree.

**The launch ROUTE is load-bearing, not cosmetic.** A launch *with* a file argument
enters `open`; a launch *without* one enters `activate`. Startup work wired to only
one is invisible on the other, and the ordinary Windows route (step 6) passes an
argument. Test both — that asymmetry was a live bug (**ScrAP-235**).

**4. Force-kill.** `taskkill /F /PID <pid>`, or `Stop-Process -Force` on a PID you
hold. Only ever the PID you launched. For crash-recovery items the kill **must** be
unclean: a graceful close runs the save prompt and leaves nothing to recover, which
reads as a feature failure rather than a test error.

**Point `XDG_STATE_HOME` at a scratch directory for anything that writes state.**
These items deliberately kill the app with unsaved work in it, and a shared state
directory leaves real recovery data behind for the operator's next launch.

**5. Install.** Not established — see `packaging/` for the current state. This is the
one step below that has not been run.

**6. Tokened (desktop-integrated) launch.** Explorer double-click, or `start <file>`
once the file association is registered. Both pass a file argument, so this is the
route step 3's warning is about — it is how a Windows user ordinarily reopens a
document, including after a crash.

**7. Reading the GTK log.** As Linux — `Gtk-WARNING **:` on stderr; capture it by
redirecting from a console build. The app's own log under
`<XDG_STATE_HOME>\scribobulate\scribobulate.log` is usually the better source: it
carries the build SHA, the resolved GTK version and the renderer at every run start,
which is what tells you whether the binary you are looking at is the one you meant to
launch.
- [ ] **23.11** **A link to a section of the same document is a navigation.** Open a document with a table of contents linking its own headings (`sdd/TDD.md` and `sdd/CAM.md` both have one; `punkie-joe-farms.md` is the report this came from). Scroll to the TOC and click one entry → the preview jumps to that section, the **tab strip selection does not move**, and **View ▸ Back is no longer greyed**. Repeat with an **outline sidebar** row (F9) → same result. Then switch to pure-**edit** mode and activate an outline row → the caret moves and Back is *unchanged*, because nothing moved the preview (TDD 23.11)
- [ ] **23.12** **Back returns to where you clicked from.** Continuing from 23.11: invoke **Back** → the viewport returns to the TOC, at the position it was at when you clicked — **not** the top of the document. **Forward** → back to the section. Nothing re-renders and the active tab never changes. Now click the *same* TOC entry twice in a row → the second click adds no stop: one Back press leaves the section (TDD 23.12)
- [ ] **23.12a** **Back works when you clicked from the very top.** The 23.12 case above starts with a deliberate scroll, which hides this one. Open a document whose table of contents is the **first thing in the file** (`sdd/TDD.md`), do **not** scroll at all, and click a TOC entry from the top of the document → the preview jumps to that section. Now **Back** → the preview scrolls **back up to the top**; it must not sit still on the section. **Forward** → the section again. Before the fix the departure was recorded as line 0 and the restore treated that as "already at the top, nothing to do", so *both* directions did nothing and the feature looked entirely broken for TOC links while working for every other navigation (ScrAP-262, TDD 23.12)
- [ ] **23.12b** **Back after a cross-document fragment link goes to the top of the document it opened.** From document **a**, follow a link of the form `b.md#some-heading` where `b.md` is **not already open** → b opens in a new tab, scrolled to that heading. Press **Back once** → b stays the active tab and scrolls to **its top**. It must not jump to the *end* of b (the position was read before GTK had laid the new view out, and `line_at_y` answers an unallocated view with the last line — ScrAP-263), and it must not sit still on the heading (ScrAP-262). Press **Back** again → across to **a**. **Forward** twice retraces both. Repeat the whole check with `b.md` **already open** in another tab (TDD 23.11/23.12)
- [ ] **23.13** **Sections and documents are one history.** With two documents open: in **a**, follow a TOC link to a section; switch to **b**; in **b**, follow one of its TOC links. Now press Back repeatedly → b's TOC position, then a (at its section), then a's TOC position — the two kinds interleaved in the order you made them. Confirm the tab strip only moves on the step that crosses documents (TDD 23.13)
- [ ] **23.14** **A section the document no longer has is not a stop.** Follow a TOC link to a section, then **edit the file on disk to delete that heading** and let live reload take it (§3). (a) Press Back → you land back where you started, in one press, and Back is then greyed — the vanished section is not a stop on the way. (b) Repeat with the entry in a *different* document: follow a fragment link to `other.md#some-heading`, come back, delete that heading on disk, then Back/Forward onto that document → it activates and its viewport is **exactly where you left it**, never yanked to the top, and no notice appears (TDD 23.14)
