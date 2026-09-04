# Probes

Standalone programs that measure a GTK/GtkSourceView runtime behaviour this
project depends on. They are **artefacts, not gates** — nothing in
`scripts/pipeline.steps` runs them, and nothing should. They exist so a claim
about the toolkit can be re-run by whoever doubts it, rather than believed
because a message once said so.

**Most are C on purpose**: each has to be runnable against an arbitrary installed
GTK, by a seat that may not have this crate building, in order to compare two
platforms' *toolkits* rather than two platforms' builds of us. This section
deliberately does not count them. It used to — "seven are C", "the eighth is
Rust", "all eight were written" — and the count was wrong within two commits of
being written, which is the same failure POLICY's build section calls out for test
counts: the one property of a set guaranteed to be stale by the next addition.
The *reasons* below are stable; the tally was not.

**Some are Objective-C**, because their subject is what GTK's Quartz backend does
with an `NSSavePanel` — a question no C probe can ask, since answering it means
counting live `NSWindow`s, reading private AppKit ivars, and sending the panel the
same `-cancel:` the user's Cancel button sends. They are macOS-only by
construction and say so.

**One is Rust**, because its subject is the **gtk4-rs marshalling layer** and a
C probe cannot prove anything about a Rust trampoline. It is a standalone crate
with its own `[workspace]`, deliberately outside the application's, so that it is
never built, linted or gated with the app — it asserts a property of the binding,
so it is *expected* to start failing when the binding is fixed upstream, and that
must not read as a Scribobulate regression. Its `Cargo.lock` is committed: the
measurement is about a specific binding version's marshalling, so a floating lock
would change the subject.

## Why these exist

Most were written on the macOS seat during the lone-carriage-return work, to
supply the one leg no other seat could: **GTK 4.22.4 / GtkSourceView 5.20.0 /
Quartz**. The Linux seat measured 4.6.9 / 5.4.1 / X11 and the researcher read the
C source; the open question was whether GtkSourceView's *tag geometry* had moved
between 5.4.1 and 5.20.0, which is the one claim that cannot be settled by
diffing GTK.

It had not. Every one of them reproduces the Linux numbers.

`native-chooser-rss.m` and its companions came later and from the opposite
situation:
a footprint climb that **only** this platform showed, where the job was not to
compare seats but to find out what was accumulating.

`iter-diagnostics.c` is here for a different reason, and it is the one worth
reading first: it exists because this seat drew a **wrong conclusion** from a
real observation, and the probe is what corrected it. See its section.

## Building

No display is needed for `undo-replay`; `crlf-run-boundary` needs a live display
because it drives a real clipboard.

```sh
cc probes/crlf-run-boundary.c -o /tmp/crlf-run-boundary \
   $(pkg-config --cflags --libs gtk4 gtksourceview-5)
cc probes/undo-replay.c -o /tmp/undo-replay \
   $(pkg-config --cflags --libs gtk4 gtksourceview-5)
cc probes/iter-diagnostics.c -o /tmp/iter-diagnostics \
   $(pkg-config --cflags --libs gtk4)
cc probes/binding-shape.c -o /tmp/binding-shape \
   $(pkg-config --cflags --libs gtk4)
cc probes/listview-scroll-snap.c -o /tmp/listview-scroll-snap \
   $(pkg-config --cflags --libs gtk4)

clang -ObjC -O1 -g -Wno-deprecated-declarations -o /tmp/native-chooser-rss \
   probes/native-chooser-rss.m $(pkg-config --cflags --libs gtk4) -framework AppKit

# The two pure-AppKit rigs need no GTK at all — that is the point of them.
clang -ObjC -O1 -o /tmp/accessory-view-dealloc \
   probes/accessory-view-dealloc.m -framework AppKit
clang -ObjC -O1 -o /tmp/appkit-panel-control \
   probes/appkit-panel-control.m -framework AppKit

clang -ObjC -O1 -o /tmp/middleclick-primary-paste \
   probes/middleclick-primary-paste.m $(pkg-config --cflags --libs gtk4) -framework AppKit

cc probes/textview-anchored-toggle.c    -o /tmp/textview-anchored-toggle    $(pkg-config --cflags --libs gtk4)
cc probes/textbuffer-selection-leak.c   -o /tmp/textbuffer-selection-leak   $(pkg-config --cflags --libs gtk4)
cc probes/textview-primary-overwrite.c  -o /tmp/textview-primary-overwrite  $(pkg-config --cflags --libs gtk4)
cc probes/textview-selection-clipboard.c -o /tmp/textview-selection-clipboard $(pkg-config --cflags --libs gtk4)

# the Rust ones build and run themselves
cargo run --manifest-path probes/binding-shape-rs/Cargo.toml
cargo run --manifest-path probes/svg-rasterise-rs/Cargo.toml
```

A `GLib-WARNING **: poll(2) failed due to: Resource temporarily unavailable` on
macOS is an unrelated main-loop artefact and is not a result.

---

## `crlf-run-boundary.c`

**Question:** can `insert_range_not_inside_self`'s chunking split an intact
`\r\n`, so that a handler repairing lone carriage returns per-chunk corrupts a
CRLF that never was one?

**Answer: yes.** `insert_range_not_inside_self` chunks on
`gtk_text_iter_forward_to_tag_toggle` and has no line-terminator awareness, and
GTK explicitly permits an iterator between the `\r` and the `\n`. A
`gtksourceview:context-classes:no-spell-check` context still **open** when the
document ends with CRLF puts a toggle inside that final pair.

Run it twice. `repair` enables a handler shaped like the one that was proposed
for `src/lineendings.rs`:

```sh
/tmp/crlf-run-boundary            # mechanism only: shows the split, no rewrite
/tmp/crlf-run-boundary repair     # enables the handler: shows the corruption
```

### The four fixtures

| # | Fixture | Clipboard | Toggle placement | Expected |
|---|---------|-----------|------------------|----------|
| 1 | Unclosed fence, CRLF: `` ```rust\r\nfn main() {}\r\n `` | CLIPBOARD | `off=22 prev=0d here=0a` — **splits the CRLF** | corrupts under `repair` |
| 2 | Closed fence, CRLF (control) | CLIPBOARD | `off=26 prev=60 here=0d` — does not split | byte-identical, both arms |
| 3 | Unclosed fence | **PRIMARY** | as #1 | corrupts under `repair` |
| 4 | Closed fence (control) | **PRIMARY** | as #2 | byte-identical, both arms |
| 5 | Unclosed fence, published as **plain text** | CLIPBOARD | toggle still present at `off=22` | **one** emission, byte-identical |

Fixture 2 is what makes a null result meaningful: it proves the rig can paste
correctly, so the tag geometry is the only variable. Fixture 5 measures the
remedy — note it does not remove the toggle, it removes the
*chunking-by-toggle*, which is why it is robust against future changes to
GtkSourceView's grammar.

Fixtures 3 and 4 exist because `GtkTextView` calls
`add_selection_clipboard(buffer, PRIMARY)` at realize, so PRIMARY carries
GtkTextBuffer content and refreshes on **selection**. The trigger is therefore
selecting inside an unclosed fence and middle-clicking, with no explicit copy.

**The split is a precondition, and the rig checks it, not just reports it.**
`report_toggles` computes whether a fixture's source document actually has a
tag toggle sitting inside its final CRLF (the `case-bad` count) before the
paste happens; each `Case` carries `expect_split` recording whether its
verdict *depends* on that toggle being there. Fixtures 1 and 3 require it —
"corrupts under `repair`" is only a meaningful negative if the toggle that
would cause the chunking was actually present. Fixtures 2 and 4 are controls
and must NOT have it. Fixture 5 does not require it either, even though the
toggle happens to be present today: the plain-text remedy removes the
chunking-by-toggle mechanism itself, so its byte-identical verdict holds
whether or not GtkSourceView still places a toggle there. If a future
GtkSourceView release moves the toggle off the CRLF, fixtures 1 or 3 would
otherwise paste clean and get silently misread as "corruption fixed" — `main`
checks `expect_split` against the measured `case-bad` count and refuses that
reading: it prints a loud `RIG BROKEN` line naming the fixture and exits `2`.

### Expected output, measured 2026-08-20 on GTK 4.22.4 / GtkSourceView 5.20.0 / GdkMacosDisplay

Arm 1, no repair — the split is real but nothing rewrites, so nothing corrupts:

```
run 1  off=0    len=22   60 60 60 72 75 73 74 0d 0a 66 6e 20 6d 61 69 6e 28 29 20 7b 7d 0d
run 2  off=22   len=1    0a
VERDICT: byte-identical
```

Arm 2, `repair` — fixtures 1 and 3 corrupt, 2, 4 and 5 stay clean:

```
run 1  off=0    len=22   ... 7b 7d 0d
  ^ handler REWROTE this run
run 2  off=0    len=22   ... 7b 7d 0a
run 3  off=22   len=1    0a
expected ... 7b 7d 0d 0a
actual   ... 7b 7d 0a 0a
VERDICT: *** CORRUPTED ***  (SAME LENGTH — a char-count check would have passed)
==== OVERALL: CORRUPTION REPRODUCED ====
```

**The diff is byte-wise on purpose.** `0d 0a` becoming `0a 0a` preserves both
byte length and character count, so a length assertion or a `chars().count()`
comparison passes on a corrupted buffer. The researcher's first probe reported
"round-trip preserved" for exactly that reason.

**The corruption is diagnostic-silent — on every platform, not just this one.**
This probe emits no `Gtk-CRITICAL` and no `Gtk-WARNING` at all, and so do the
researcher's four equivalent rigs at GTK 4.6.9 / X11, all run under
`G_DEBUG=fatal-criticals`, all producing the same corrupted bytes.

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | No corruption, and every `expect_split` fixture's precondition held — a trustworthy null result. |
| `1` | At least one fixture's pasted bytes differ from its source — corruption reproduced. |
| `2` | **Rig blind, not "worse corruption".** A fixture marked `expect_split` measured `case-bad == 0` — no toggle sat inside its CRLF, so the mechanism this probe exists to exercise never fired. GtkSourceView's tag geometry likely moved since 5.4.1/5.20.0; re-derive the offsets in `report_toggles`'s output before trusting exit `0` or `1` from this binary again. This mirrors `binding-shape-rs`'s multi-run precondition assert (above) — the difference is this probe reports the failure as a distinct exit code rather than a panic, so a caller can tell "rig blind" apart from "the process crashed". |

Mutation-tested 2026-08-22 on GTK 4.6.9 / GtkSourceView 5.20.0 / X11 (Xvfb): forcing
fixture 1's and 3's stashed `case-bad` to `0` (simulating the toggle no longer
splitting the CRLF) produces, unmodified otherwise:

```
=== UNCLOSED fence (researcher's repro)  (repair=off, clipboard=CLIPBOARD) ===
  ...
  VERDICT: byte-identical
  *** RIG BROKEN: this fixture requires a tag toggle inside the CRLF to test
  anything, but none was found (case-bad=0) — GtkSourceView's tag geometry may
  have moved. The VERDICT above is MEANINGLESS. ***
...
==== OVERALL: RIG BROKEN (precondition unmet — verdict meaningless) ====
```
exiting `2`. Fixtures 2, 4 (controls, `expect_split = FALSE`) and 5 (plain-text
remedy, robust by design — see above) do not fire even with the same forced
`case-bad = 0`. Restoring the real `case-bad` value returns the binary to its
normal exit `0` / no-corruption output.

Read the first version of that sentence in `iter-diagnostics.c`'s section before
trusting any platform claim here: this seat originally recorded it as *"silent on
Quartz, loud on Linux"*, which was a misattribution. The Linux seat's diagnostics
came from a different, co-located defect in his test, not from this one.

---

## `undo-replay.c`

**Question:** does undo replay re-emit `insert-text`, so a repair handler fires
during replay and undo restores different bytes than were deleted?

**Answer: yes, silently.** `gtk_text_buffer_history_insert` calls the *public*
`gtk_text_buffer_insert`, so the handler runs. `GtkTextHistory` cannot notice:
`gtk_text_history_text_inserted` opens with `return_if_applying`, and
`gtk_text_buffer_history_delete`'s `expected_text` argument is ignored by GTK's
implementation.

The precondition is a buffer already holding a lone `\r` — a legacy document
populated **before** the hook is armed. That is one line of `build_tab_editor`
ordering away, which is why the ordering there is load-bearing rather than
incidental.

The second arm measures the remedy: `undo`/`redo` are `G_SIGNAL_RUN_LAST` with
the replay in the default handler, so a plain `connect` setting a flag and a
`connect_after` clearing it straddles the replay exactly.

```sh
/tmp/undo-replay
```

### Expected output, measured 2026-08-20 on GTK 4.22.4 / GtkSourceView 5.20.0

```
== undo replay through the hook (guard=off) ==
  before delete          78 0d 79
  after undo             78 0a 79
  hook fired during replay: 1
  VERDICT: *** UNDO DIVERGED ***

== undo replay with the bracket (guard=ON) ==
  before delete          78 0d 79
  after undo             78 0d 79
  hook fired during replay: 0
  VERDICT: undo restored the original
```

Both arms match the researcher's 4.6.9 measurements byte-for-byte, which is what
establishes that this one is version-independent rather than a 4.22 quirk.

There is a design question underneath it that no probe can settle: if the
invariant is that no buffer ever holds a lone `\r`, then what *should* undoing a
deletion inside a legacy document restore? The bracket preserves the original
bytes, which contradicts the invariant. Pick one deliberately and write it down.

---

## `iter-diagnostics.c`

**Question:** the Linux seat's in-tree test emitted `Gtk-WARNING Invalid text
buffer iterator` plus `Gtk-CRITICAL gtk_text_buffer_insert: assertion
'gtk_text_iter_get_buffer (iter) == buffer' failed`, and neither
`crlf-run-boundary.c` here nor the researcher's four Linux rigs emit anything at
all. Is that a platform difference?

**Answer: no. It is a handler-SHAPE difference, and it follows the code.** This
probe isolates three shapes against the same buffer and payload `"x\ry"`:

```sh
/tmp/iter-diagnostics 0   # A: insert at the handler's own iter
/tmp/iter-diagnostics 1   # B: insert at an iter belonging to a DIFFERENT buffer
/tmp/iter-diagnostics 2   # C: capture an iter, mutate, reuse the captured iter
```

### Expected output, measured 2026-08-20 on GTK 4.22.4 / GdkMacosDisplay

| Arm | Result bytes | Diagnostics |
|-----|--------------|-------------|
| A own-iter | `78 0a 79` | **none** |
| B foreign-iter | (empty) | 1 × `Gtk-CRITICAL … 'gtk_text_iter_get_buffer (iter) == buffer' failed` |
| C stale-iter | `5a` | 2 × — the `Gtk-WARNING Invalid text buffer iterator` then the `Gtk-CRITICAL`, in that order |

Arm C reproduces the Linux seat's exact pair in his order, **on Quartz**, which
is what establishes that the diagnostics are a property of the shape and not of
the backend. The researcher confirmed the same three arms at GTK 4.6.9 / X11 and
source-read the emission site (`gtk_text_iter_make_surreal`, gtktextiter.c
:173-184, a plain `chars_changed_stamp` comparison); `grep -c` for
`GDK_WINDOWING|__APPLE__|G_OS_WIN32|QUARTZ` across `gtktextiter.c` and
`gtktextbuffer.c` returns 0 and 0, so there is no backend-conditional code in
either file.

Arm C's result bytes differ slightly between the two seats' runs (`5a` here
versus `78 0a 79` on the researcher's) because the arms mutate in a slightly
different order. That is incidental; the diagnostic pair is the signature.

### The conclusion that matters, and it is uncomfortable

An earlier reading of this evidence was that `G_DEBUG=fatal-criticals` is blind
on macOS while catching the class on Linux. **That was wrong**, and the
corrected version is worse: the CRLF chunk-boundary corruption is
**diagnostic-silent on every platform**. Nothing was being suppressed here;
there was nothing to catch, because a correctly-shaped handler emits nothing
while corrupting the text.

`fatal-criticals` guards against *misuse of the API*, not against *wrong answers
from correct API use*. For a silent-corruption class the tripwire has to be a
content assertion on the bytes. The Linux seat's test went red because it
counted `\r` and `\n` occurrences, not because a runtime check fired — and the
two diagnostics it also emitted were reporting a *different*, co-located defect
(a `GtkTextIter` held across a mutation, fixable with a `GtkTextMark`).
Attributing them to the CRLF bug would have been a false attribution.

---

## `binding-shape.c`

**Question:** `iter-diagnostics.c` showed the diagnostics follow the handler's
shape. But *which* shape does gtk4-rs actually give us? The Linux seat's Rust
code emits ~19 diagnostics on a multi-run paste and the researcher's C probes
emit zero, doing what looks like the same thing.

**Answer: the gtk4-rs binding is the variable, and it is a defect, not a
difference.** `connect_insert_text`'s trampoline (gtk4-0.10.3
`src/text_buffer.rs:76`, identical at 0.9.7:85) hands the closure a **copy** of
the caller's `GtkTextIter` via `from_glib_none` and **never writes it back**. So
a nested `buffer.insert(iter, …)` revalidates the copy while the iterator
`insert_range_untagged` is still holding stays stale. In C the handler receives
the caller's iter by pointer and the nested insert repairs it in place.

This probe runs both shapes in one binary against one buffer, so the binding is
the only variable.

```sh
/tmp/binding-shape
```

The source is `"ab\r\ncd\r\nef\r\ngh"` with an anonymous tag ending at each
`\n`, so `insert_range` chunks the CRLF pairs apart. Both buffers share one
`GtkTextTagTable`, which `gtk_text_buffer_insert_range` requires and which is
what `create_clipboard_contents_buffer` does for a same-app copy. **The
multi-run precondition is asserted**, and a single-run result is reported as a
broken rig rather than silently passing — the first version of this probe used
two tag tables, produced 0 emissions and an empty destination, and would have
read as a clean null result without that assertion.

### Expected output, measured 2026-08-20 on GTK 4.22.4 / GdkMacosDisplay

| Shape | Emissions | Diagnostics | Result |
|-------|-----------|-------------|--------|
| C (insert at the signal's own iter) | 6 | **none** | `61 62 0a 0a 63 64 0a 0a 65 66 0a 0a 67 68` — CRLF corrupted, order intact |
| gtk4-rs (insert at a copy, no write-back) | 6 | **15 × `Gtk-WARNING` + 3 × `Gtk-CRITICAL`** | `0a 0a 0a 67 68 65 66 0a 63 64 0a 61 62 0a` — **text SCRAMBLED** |

Both rows reproduce the researcher's GTK 4.6.9 / X11 numbers byte-for-byte,
diagnostic counts included. That is what establishes the cause as the binding
rather than the backend or the GTK version.

**The second row is the finding that matters.** Because the destination iterator
never advances, later chunks land at a stale position and the document comes out
reordered. CRLF-to-LF+LF is the mild end of what this shape does; losing the
document's line order is the other end, and it is the same code path.

### Consequence

`TextBufferImpl::insert_text` — the **vfunc** override — does not have this
flaw. gtk4-0.10.3 `src/subclass/text_buffer.rs:360-373` writes the iterator back
(`*iter_ptr = *iter.to_glib_none().0;`). So the vfunc route is not merely tidier
than `connect_insert_text`; on this binding version it is *correct where
`connect_insert_text` is not*, independently of anything to do with clipboards
or line endings. Any handler in this codebase that mutates a buffer from
`connect_insert_text` inherits the defect above.

---

## `binding-shape-rs/`

**Question:** `binding-shape.c` shows the copy-without-write-back scrambles a
multi-run paste, but it does so by *simulating* the Rust shape in C. Does the
real gtk4-rs binding behave that way, and is the proposed remedy — overriding the
`TextBufferImpl::insert_text` vfunc instead of connecting to the signal —
actually correct?

**Answer: yes to both, with one caveat that must travel with the remedy.**

Three arms, all in the real binding. Measured 2026-08-20 on GTK 4.22.4 /
gtk4-rs 0.10 / GdkMacosDisplay:

| Arm | Shape | Emissions | Diagnostics | Result |
|-----|-------|-----------|-------------|--------|
| R1 | `connect_insert_text` + `stop_emission` + nested insert | 9 | present | `0a 0a 0a 67 68 65 66 0a 63 64 0a 61 62 0a` — **scrambled** |
| R2 | `TextBufferImpl::insert_text` + `parent_insert_text` | 6 | **0** | `61 62 0a 0a 63 64 0a 0a 65 66 0a 0a 67 68` — CRLF corrupted, **order intact** |
| R3 | `insert_markup("AB<b>CD</b>EF")` into the vfunc | — | — | `ABCDEFCDEFEF` — **over-read** |

**R1** reproduces `binding-shape.c`'s simulated output byte-for-byte, which is
what establishes that the C simulation was faithful.

**R2 is the finding.** The vfunc route fixes the iterator defect and does
*nothing whatever* about the chunking — which is the correct division of labour
between the two problems, and a result that "fixed" both would have meant the rig
was lying. It also needs no `stop_emission`, no reentrancy flag, and cannot reach
the stack-overflow failure mode, because `parent_insert_text` calls the default
handler directly rather than starting a new emission. Three hazards deleted by
one trait impl.

**R3 is the caveat.** The subclass trampoline ignores its `_length` argument and
reads the pointer as a NUL-terminated string, so a caller passing a *bounded*
length gets the wrong text. GTK does this on a public path:
`gtk_text_buffer_insert_markup` → `pango_parse_markup` →
`gtk_text_buffer_insert_with_attributes` → `gtk_text_buffer_insert (buffer, iter,
text + start, end - start)` (gtktextbuffer.c @4.22.4:4928), one bounded emission
per attribute run, each pointing into the middle of one longer string. The damage
shape is **duplicated trailing text at every attribute boundary**, not a crash and
not an over-read into garbage — which is exactly why it would survive review.

Not reachable in this codebase today, and that was checked rather than assumed:

```sh
grep -rniE "snippet|completion|insert_markup|insert_with_attributes|insert_with_tags" src/
```

returns only unrelated prose (annotation claim snippets, async I/O completions).
No `GtkSourceCompletion`, no snippet bundle, no markup insert.

So the claim the project may assert is narrower than "the vfunc is correct":

> Adopt `TextBufferImpl::insert_text`; it fixes the iterator defect. Do **not**
> let a bounded-length caller reach it — `insert_markup`,
> `insert_with_attributes`, or a GtkSourceView snippet/completion — until the
> binding is fixed upstream. If completion or snippets are ever enabled, re-run
> arm R3 first.

**This obligation is now a build failure rather than a sentence here.** `clippy.toml`
bans `TextBufferExt::insert_markup` and the `sourceview5::CompletionWords` type, both
citing arm R3, and `-D warnings` makes either a hard stop — so "re-run R3 first" is
enforced at the moment someone tries, instead of depending on them having read this
paragraph. Two notes from installing it, because both are the reusable part:
`insert_with_attributes`, the probe's other named caller, turns out **not to be bound in
gtk4-rs 0.10 at all**, so there is nothing to ban and the hazard is unreachable from Rust
— checked against the crate source rather than assumed, since a ban on a path that does
not resolve fails OPEN while looking installed. And GtkSourceView's two internal sites
are inside the library, where no ban on our own call sites could ever see them, which is
why the guard is on the TYPE that turns the feature on rather than on any call.

**Whether GtkSourceView reaches a bounded-length insert internally is no longer
open — it does, at two sites, and the researcher audited every
`gtk_text_buffer_insert` call in the 5.20.0 tarball to find them:**

- `completion-providers/words/gtksourcecompletionwords.c:395-430` —
  `len = strlen (word) - strlen (text);` then `gtk_text_buffer_insert (…, word,
  len)`, with **no NUL inserted first**. Accepting a `GtkSourceCompletionWords`
  proposal mid-word emits `len < strlen`. This is the route.
- `vim/gtksourcevimcommand.c:417` — `gtk_text_buffer_insert (…, text, strlen
  (text) - 1)`, deliberately dropping the trailing byte. Vim emulation only.

So the guard is **specific, not general**: re-run arm R3 before enabling
`GtkSourceCompletionWords` or vim mode. Not "before enabling completion" — it is
that *provider* which does it, and a different provider needs its own check.

### The file-loading path is safe, and why is the interesting part

`GtkSourceBufferOutputStream` — every `GtkSourceFileLoader` read, which is the
path a document would arrive by — does pass a bounded `nvalid`. It is safe
anyway, because GtkSourceView defends it explicitly
(`gtksourcebufferoutputstream.c:711-737`): it temporarily writes a NUL at
`nvalid`, inserts, then restores the original byte, with a comment citing *"issues
with pygobject marshalling of the insert-text signal"* and GNOME bug **726689**,
filed in 2014.

That is worth stating plainly: **the loader is safe because the C library is
protecting language bindings that ignore the length argument, not because the
binding is correct.** The same defect was found through PyGObject a decade ago,
GtkSourceView carries a workaround for it in one path and not in the other two,
and gtk4-rs has now reproduced it. Prior art of that shape is what makes the
upstream report hard to close as won't-fix.

### The closing assertion is a falsifier

The probe ends by asserting that the **vfunc** arm preserves document order, and
fails loudly if it does not. That is deliberate: the point is to be able to
withdraw the recommendation, not to confirm it. A recommendation that cannot fail
is folklore.

---

# The PRIMARY selection on macOS

Three probes answering one design question: **is the PRIMARY selection clipboard
inert on Quartz, as "PRIMARY is an X11 concept" would suggest?**

**It is not, on all four counts that matter.** It is published automatically, it
carries `GtkTextBuffer` content, it is readable, and it is reachable by an
ordinary user gesture. Any design that treats PRIMARY as Linux-only with a
no-op macOS path is wrong.

```sh
cc probes/primary-liveness.c    -o /tmp/primary-liveness    $(pkg-config --cflags --libs gtk4)
cc probes/primary-middleclick.c -o /tmp/primary-middleclick $(pkg-config --cflags --libs gtk4)
cc probes/primary-crlf.c        -o /tmp/primary-crlf        $(pkg-config --cflags --libs gtk4 gtksourceview-5)
```

All three need a real, unlocked desktop session — there is no headless
equivalent here, and `primary-middleclick` additionally needs a synthetic HID
event posted to the window server.

## `primary-liveness.c` — published automatically, and readable

**It never calls `gtk_text_buffer_add_selection_clipboard` itself.** That is the
whole design of the probe: `GtkTextView` is supposed to do that at realize, and a
rig that wires it by hand cannot tell you whether the toolkit did. An earlier
measurement in this thread *did* wire it by hand and therefore proved nothing.

Measured 2026-08-20, GTK 4.22.4 / GdkMacosDisplay:

```
BEFORE selection:  PRIMARY  provider=NULL     formats=(empty)
AFTER select-all:  PRIMARY  provider=present
                   formats: GtkTextBuffer gchararray text/plain;charset=utf-8 text/plain
                   read_text -> "hello selected world"
```

It also tracks selection *changes* — narrowing to five characters re-publishes
`"hello"`. The control arm does an explicit `copy_clipboard` to CLIPBOARD and
sees the identical format list, so the rig can tell populated from empty.

## `primary-middleclick.c` — reachable by a real gesture

Posts a genuine `CGEventOtherMouseDown`/`Up` with `kCGMouseButtonCenter` to the
window server, not a synthesised GTK signal.

```
BUTTON PRESS DELIVERED: button=2 at (249,170)
FINAL buffer: AAAA BBBBAAAA
VERDICT: *** CHANGED -- a middle-click PRIMARY paste DID occur ***
```

GDK-Quartz delivers the middle button as button 2, `GtkTextView` handles it, and
it pastes from PRIMARY.

### Read this before trusting any run of it

**The `BUTTON PRESS DELIVERED` line is a positive control and it is not
decoration.** This probe's first two runs both reported "unchanged — no
middle-click PRIMARY paste occurred", and both verdicts were worthless:

1. The first ran its report timer out *before anything ever clicked*. A rig that
   answers a question it was never asked will answer it confidently.
2. The second's verdict turned out to be a fragment bleeding from a surviving
   earlier process writing to the same output path — visible only as a missing
   `FINAL buffer:` line and a truncated `VERDICT:` prefix in a hex dump.

So: **a negative result from this probe means nothing unless the delivery line is
present and exactly one process is alive.** Run it with a unique output path and
check `pgrep`. The failure mode is not that the probe breaks; it is that it
produces a plausible, wrong, confidently-worded answer.

Caveat on reachability, stated rather than scoped around: a Mac trackpad has no
middle button, so performing this gesture needs a three-button mouse. That is a
hardware-availability question, not an API one. Whether GDK-Quartz routes a
*trackpad* gesture to button 2 is **not established** — only a synthetic
button-2 event was tested.

## `primary-crlf.c` — clean today, and the mechanism is armed

The mirror of `lineendings`' same-application CRLF guard, on the route that needs
no explicit copy. Uses the **automatic** publish path, and pastes into a buffer
carrying only an observer — the tree's state with no repair hook.

```
toggle at off=22 sits INSIDE the final CRLF
PRIMARY holds GtkTextBuffer: YES
run 1 len=22  60 60 60 72 75 73 74 0d 0a 66 6e 20 6d 61 69 6e 28 29 20 7b 7d 0d
run 2 len=1   0a
expected/actual identical  ->  byte-identical, PRIMARY is CLEAN
```

**Do not read "PRIMARY is clean on macOS" as "the mechanism is absent there."**
The emission dump above is why this probe prints one. The multi-run split *is*
present — the toggle sits inside the final CRLF, the paste arrives as two
emissions, and run 1 ends `0d` with run 2 supplying the `0a`. It is clean solely
because nothing mutates the payload. Attach any payload-mutating handler to those
buffers and it corrupts here exactly as it does on X11.

---

## `native-chooser-rss.m` — where a native chooser's memory goes

**macOS only.** Every `GtkFileChooserNative` invocation grew the process's RSS by
roughly a megabyte, with no plateau across twenty cycles, and `leaks(1)` reported
about a hundred kilobytes against a thirty-megabyte climb — so whatever was
accumulating was still *reachable*, and the useful question was not "what leaked"
but "what is still holding it".

RSS read from outside the process cannot answer that, so this probe reads the
process from inside it:

- **A `vmmap`-equivalent, self-administered.** It walks its own VM map with
  `mach_vm_region_recurse` and sums dirty pages per user tag, which is the same
  accounting `vmmap -summary` prints — and needs no `task_for_pid`, so it works
  with no developer-tools authorisation and no `sudo`.
- **Every malloc zone's in-use bytes**, which separates *live heap that is still
  referenced* from *pages the allocator has not returned*. Those two have the same
  RSS signature and completely different causes, and this is the fork the whole
  investigation turns on.
- **Live GObject counts** (`GOBJECT_DEBUG=instance-count`) for the chooser's
  object graph, so "is the widget tree freed?" is answered by counting it rather
  than by inferring it from bytes.
- **Live `NSSavePanel`/`NSOpenPanel` counts** from `[NSApp windows]`, which is the
  AppKit half of the same question.

Two things about the *method* are worth carrying to the next probe of this kind.

**Dismiss it the way the user dismisses it.** `gtk_native_dialog_hide()` and the
Cancel button take different paths through the Quartz backend, and only the second
is the one the application takes. A probe that used `hide()` because it is the API
in reach would have measured — and did, for one wrong afternoon — a leak the
application never triggers. `--cancel` sends `-[NSSavePanel cancel:]` to the live
panel instead, which is exactly what Cancel does, needs no synthetic key events,
and therefore also works with the screen locked, where a keystroke-driven probe
silently sends its Escape to whatever app is frontmost.

**A window-list count is not a liveness count, and mine read as success while
being blind.** The probe originally reported "0 live `NSSavePanel`s" after closing
them and I took that as the panel being freed. `-close` removes a panel from
`[NSApp windows]` *without* deallocating it, so the counter had simply stopped
being able to see its subject — the shape of a check that passes because it went
blind rather than because the condition holds. `--track-dealloc` attaches a
`DeallocSpy` (an associated object whose `-dealloc` increments a counter) and
answers the question the window list cannot. It reports zero deallocations in
every mode tried, including closing the panel *and* explicitly balancing GTK's
retain — which is how the probe established that something beyond GTK holds the
panel.

**Measure every cell more than once.** The first run of the mode matrix produced
a clean, plausible story in which closing spent panels recovered half the growth.
Repeating each cell three times, interleaved, that difference vanished into noise
(948.5 KB/cycle against 920-937, spreads of 6-15). One run per cell is exactly
the shape that reads as a result and isn't one, and the interleaving matters as
much as the repetition: run all of mode A then all of mode B and any drift over
wall-clock lands entirely on B.

**Do not count instances inside a signal handler.** `g_signal_emit` holds a
reference on the emitting instance for the duration of the emission, so an
instance count taken from a `response` handler reports one dialog still live that
is in fact about to finalize. The checkpoint runs from the main loop for this
reason, and the comment at that site says so.

### Modes

| flag | what it isolates |
|---|---|
| *(default)* | dismissal via `gtk_native_dialog_hide()` |
| `--cancel` | dismissal via `-[NSSavePanel cancel:]` — the user's path |
| `--reap` | close spent panels after each response — tests a candidate remedy |
| `--widget-only` | the internal `GtkFileChooserDialog` alone, no native panel |
| `--no-filter` | no `GtkFileFilter`, so no accessory view on the panel |
| `--open` | `NSOpenPanel` rather than `NSSavePanel` |
| `--no-show` / `--folder` / `--instances` / `--linger` | construction only; folder to enumerate; instance counts; stay alive for `heap(1)` |

The design rule behind the flag list: each one removes exactly one thing, so the
difference between two runs names a cause instead of suggesting one.

Two properties that rule depends on, and which the probe now enforces rather than
assumes. **An unknown flag is rejected with exit 2**, because a silently ignored
`--instaces` runs the default configuration and prints a clean, wrong answer under the
heading you thought you had selected — the difference between two runs then names a
cause that was never varied. And **every mode that changes what is measured is echoed
in the run header**, not just some of them: a transcript that omits a mode cannot be
told apart from one that did not use it, which is the same failure one step later.

---

## `empty-filter-crash.c` and `allowed-file-types-empty.m` — a claim that did not survive

These two exist to record a **falsification**, which is the rarer and more useful
kind of artefact: a crash report was one message away from being filed against
GNOME, and these are the runs that stopped it.

The prediction was that a `GtkFileFilter` carrying a name and no rules would abort
the Quartz chooser at open, with no user interaction. Its chain was verified from
source at every step: GTK builds a non-nil, zero-element `NSArray` for a ruleless
filter, the `NULL` guard in `file_filter_to_quartz` lets an empty array through,
the handler's `containsObject:@""` test is `NO` for it, and GTK primes the initial
filter selection itself at launch — so the empty array reaches
`-[NSSavePanel setAllowedFileTypes:]` unattended. Apple's SDK header for that
property states that a non-nil empty array raises an exception.

It does not. `empty-filter-crash.c` runs the whole path end to end and exits 0.

**The second probe is the one that matters, and the reason there are two.** A
clean pass from the first says only "my repro didn't crash", which is a statement
about the probe. `allowed-file-types-empty.m` tests the *premise* in isolation,
against a bare `NSSavePanel` with no GTK in the process, and shows that
`setAllowedFileTypes:` raises nothing on macOS 26.6.1 for `@[]`, for `nil`, or for
`@[@""]`. That is a statement about the platform, and it is what turned a negative
result into a retraction rather than a shrug. Interrogate a negative as hard as a
positive: a failed reproduction is evidence about your setup until you have
falsified the mechanism itself.

The transferable rule, which cost a nearly-filed bug report to learn: **an SDK
header is primary evidence for API *surface*** — that a class is not a singleton,
that a property is `copy`, that a symbol is deprecated — **and not for runtime
*behaviour***. Exceptions raised, objects freed, handlers invoked: those need an
observation. And a deprecated symbol's documented contract should be assumed stale
until someone watches it, since deprecation is precisely when an implementation
gets rewritten underneath its documentation.

What survives the falsification is the version-independent half: a zero-rule
filter should not produce a filter entry at all. That argument cannot be refuted
by a maintainer's OS version, which is exactly why it is the one worth making.

---

## `textview-selection-clipboard.c` — which caller raises the assertion

**Question.** When an application takes PRIMARY over by removing GTK's selection-clipboard
registration at realize, who raises `gtk_text_buffer_remove_selection_clipboard: assertion
'selection_clipboard != NULL'`?

**Why a probe and not a reading.** The assertion message names neither the caller nor the
buffer — it asserts on the result of a local — and there are three live callers. Reading it
as "my handler ran and found nothing" is an inference, and the source trace says that
inference is false.

**Measured** 2026-08-21, GTK 4.22.4 / Quartz. The backtrace names the caller outright:
`gtk_text_view_set_buffer`. `--take-over` raises exactly one critical from inside
`set_buffer`; `--control` raises none on an identical swap. The registration is ref-counted,
`set_buffer` removes from the outgoing buffer and adds to the incoming one on every swap, so
the take-over is undone by the swap — which the preview does on every re-render.

**This is one of the two measurements that killed the publisher-side design.**

---

## `textview-primary-overwrite.c` — the other publisher-side design, also refuted

**Question.** Can an application own PRIMARY by OVERWRITING the clipboard content instead of
removing GTK's registration — leaving GTK's ref counting alone?

**The trap it was built to find, and did.** GTK clears PRIMARY only while the content is
still its own (guarded on `get_content == priv->selection_content`). Once the application
owns it, GTK will never relinquish on the application's behalf, so handling only the
has-selection case leaves the application's stale text on PRIMARY after the user deselects —
silent, and worse than the defect being fixed.

Together with the probe above, this is why the take-over was deleted rather than repaired:
both designs attacked the PUBLISHER in order to change what the CONSUMER does.

---

## `middleclick-primary-paste.m` — attacking the consumer instead

**Question.** Can an application replace GTK's middle-click PRIMARY paste with its own, and
does its gesture actually receive the event?

**The design it validates** is the one that shipped: leave GTK's publishing entirely alone,
and fix the consumer — the editor's middle-click reads PRIMARY as *text*, which is safe
against any publisher's content, including the preview's still-rich one.

**A 2×2, agreed before running, so no claim is uncontrolled**: `--baseline` (setting on, no
gesture) proves the rig delivers a middle click at all and must be run first — without it a
silent gesture is indistinguishable from an undelivered event; `--gtk-off` proves the setting
is what gates GTK's branch; `--ours` is the shipped design; `--both-on` decides whether
turning the setting off is required or merely tidy.

---

## `textbuffer-selection-leak.c` — a leak that no application change reaches

**Question.** Does a `GtkTextBuffer` leak a reference per select-then-deselect, and does
taking PRIMARY over change that? (ScrAP-313.)

**Why measure it again.** The register's refcount table records a SOURCE read, not a run —
and ScrAP-157 is this project's standing example of a Linux-era defect simply absent on a
later GTK. More importantly, the entry had been cited *against* a proposed change, with the
claim that the take-over avoids the leak.

**Measured** 2026-08-21, GTK 4.22.4 / Quartz, 7 cycles: control `+0`; select/deselect `+7`;
select/deselect **with** the take-over `+7` — **identical**. The leak reproduces, and the
take-over neither causes nor cures it. This is the probe that refuted an objection raised
against removing the take-over, which is the whole reason it exists.

---

## `appkit-panel-control.m` — the control that inverted its own conclusion

**Question.** How much of the native file chooser's per-invocation footprint growth is GTK's?

**Read this before quoting it.** The FIRST version of this control was wrong in a way that
flattered a conclusion: it dismissed the panel with `-close` while GTK's completion path sends
only `-orderOut:`, so control and treatment differed in two ways at once and the entire
difference was charged to GTK.

**Corrected, it inverts the conclusion.** A process with no GTK in it at all grows ~0.56 MB per
panel presentation (~0.97 MB with an accessory view), linearly to at least 40 cycles, and the
panel is never deallocated whichever dismissal is used. **The cost is AppKit's.** On macOS
26.6.1 `NSSavePanel` holds a strong reference to itself in a `_retainedSelf` ivar, which is why
nothing anyone releases ever deallocates it.

Every switch is a variable someone asked to hold still: `--dismiss` isolates close from
orderOut, `--accessory` isolates the filter popup GTK installs, `--no-spy` proves the dealloc
instrument is not itself causing what it measures, and `--verbose` prints the per-cycle curve
so its SHAPE (linear vs plateauing) is visible rather than just its total.

---

## `accessory-view-dealloc.m` — an ordering bug that looks like a correct fix

**Question.** Does `-[NSSavePanel setAccessoryView:nil]` actually let the accessory view
deallocate, or does releasing it while the panel still references it merely balance the books
without freeing anything?

**Measured** (macOS 26.6.1, unlocked, n=3, 20 presentations per run):

| arm | deallocations | per cycle |
|---|---|---|
| release popup with `accessoryView` still set | 0/20 | 932.2 KB |
| `setAccessoryView:nil` first, THEN release | 19/20 | 542.3 KB |

**The failure mode is the ORDERING, not the release** — and the wrong version is exactly what
someone writes reasoning only from "we own a +1, so give it back". It looks right, it passes
review, and it reclaims nothing.

Two honesty notes carried from the source, because they are the reusable part: the residual
(542.3) is *close* to `appkit-panel-control.m`'s bare-panel figure, but the two rigs differ in
`setReleasedWhenClosed:`, dismissal path and inter-cycle gap, and the exact invocation behind
the comparison figure was never recorded — so that agreement is **INFERRED, not MEASURED**. And
it is 19/20, not 20/20; the entry should not say "all". Nineteen against zero carries the
verdict without needing either.

**Do not de-duplicate the two ObjC rigs' boilerplate.** `accessory-view-dealloc.m` rests its
conclusion on the two probes sharing no code path — the duplication is load-bearing.

## `listview-scroll-snap.c`

**Question:** what does it take to make a `GtkListView` jump to the END of its list
during a fast upward wheel scroll — the tree model, the non-uniform row heights, or
the widget itself?

**Answer: the widget itself, on any GTK older than 4.10.1** (GNOME/gtk#2971). Measured
on 4.6.9 / X11 / cairo, 540 rows, `page_size` 800:

- A plain `GtkStringList` with uniform rows snaps. The `GtkTreeListModel` and the
  per-level font sizes the application happens to use are both innocent (`flat` mode
  reproduces; `varied` and `tree` add nothing).
- `PROBE_AUTO=86` removes the input stack entirely — a `g_timeout` writing the
  adjustment every 8 ms — and it still snaps, to `upper - page_size`, about every 23
  writes. So it is the WRITE RATE, not the wheel: `kinetic_scrolling=FALSE` changes
  nothing, and the same 60 steps 100 ms apart never snap.
- Upward only. `PROBE_START=8000` still lands at the end, so the destination is the
  list's end rather than anything derived from where the gesture began.
- `PROBE_SELECT=100` changes nothing: not selection- or focus-driven.
- `PROBE_COUNT=1` reports 205 realized rows — `GTK_LIST_VIEW_MAX_LIST_ITEMS` 200 plus
  extras — which is the recycling that the defect needs.

**`PROBE_WRAP_BOX=1` is a trap, kept deliberately.** Wrapping the list in a `GtkBox` so
the `GtkScrolledWindow` interposes a `GtkViewport` makes the snap vanish — and makes the
list stop working: the viewport never drives the list's own scrollable adjustment
(`own_value` stays 0), so past ~row 205 the pane renders BLANK and the smooth scroll is a
scroll over nothing. It reads as a clean fix in an adjustment trace, which is exactly why
it is in here rather than in the application.

```sh
/tmp/listview-scroll-snap flat 540        # then wheel up fast over the list
PROBE_AUTO=86 /tmp/listview-scroll-snap flat 540    # no input needed
```

Every line of output is `V value upper page` (value-changed) or `C …` (changed); a
value equal to `upper - page` mid-gesture is the defect.

---

## `svg-rasterise-rs/`

**Question:** the preview's zoom must enlarge a document image, and an SVG blown up
from its natural-size raster goes soft exactly where it matters — the text drawn inside
a diagram, which is what the reader zoomed in to read. `GdkTexture::from_file` offers no
size control, so the candidate route is gdk-pixbuf's scaled loaders. But **"the loader
accepted a size" and "the loader re-rendered at that size" are indistinguishable from
the outside**, and only the second is worth building on. Which is it?

**Answer: it re-renders — through both the file and the bytes route.** MEASURED
2026-09-03 on the Linux reference host, gdk-pixbuf 2.42.8 / librsvg 2.52.5 / GTK 4.6.9.

The discriminator is the **anti-aliased fringe**. A vector re-render at 3× draws a ~1px
soft edge along each shape's perimeter; a bilinear upscale of a 1× raster stretches that
same fringe to ~3px, so it carries roughly three times as many intermediate-luminance
pixels for the same drawing. Counting them decides it without anyone looking at anything.

| Route | Result at 3× | Fringe |
|---|---|---|
| `Pixbuf::from_file_at_scale` | 600×300 | **4.7‰** |
| `Pixbuf::from_file` + `scale_simple(Bilinear)` (the control) | 600×300 | 19.2‰ |
| `PixbufLoader::set_size` + `write` (the in-memory/remote shape) | 600×300 | **4.7‰** |

Three further facts the same run establishes, each of which the design depends on:

- **`PixbufFormat::is_scalable()` discriminates.** `svg` answers `true`, `png` answers
  `false`, so `Pixbuf::file_info` — already called on this path for the pixel cap —
  answers "may I re-render this?" with no extra read.
- **A viewBox-only SVG has a natural size anyway.** With `width`/`height` deleted and
  only `viewBox="0 0 200 100"` left, both `file_info` and `Texture::from_file` still
  report 200×100, so "the size at zoom 1.0" is well defined for the shape most
  hand-authored diagrams have.
- **`Texture::from_file` gives the natural size and nothing else** — 200×100 in both
  cases — which is precisely why the scaled loader is needed rather than a texture the
  widget is simply asked to draw larger.

**What it costs, which is the other reason to keep this rig.** Timing the same call on
`sdd/system-overview.svg` (1000×1112) measures `file_info` under a millisecond,
`from_file` at 230-239 ms, and `from_file_at_scale` at 3× at **227-294 ms** — the two
decodes cost about the same, and both are expensive because librsvg parses and renders the
whole document. The macOS seat measures roughly half those figures on Homebrew GTK 4.22.4. The application decodes per render with no cache, which is
recorded in the debt register.

**The letterbox hazard is `preserve_aspect_ratio = FALSE`, and only that.** Never pass it
hoping to stretch: librsvg still honours the document's own `preserveAspectRatio`, so it
paints the art inside the canvas you asked for and hands back transparent padding.

⚠ Passing BOTH axes with `preserve_aspect_ratio` TRUE is **not** a hazard, contrary to what
this file first said. The loader fits inside the box and returns the aspect-correct size —
a square request against a 4:1 document comes back 512×128, not padded. Measured
identically on three hosts (Linux/librsvg 2.52.5, macOS/Homebrew GTK 4.22.4,
Windows/gvsbuild GTK 4.22.4) by three seats. The application still passes one axis and
`-1`, for the smaller and true reason: no rounding of ours then decides which axis binds.

**What it does not answer.** The probe measures *this host's* image stack. The SVG
pixbuf loader is a separate package on Linux (`librsvg2-common`) and comes from a
different build on Homebrew and gvsbuild, so its presence is a per-platform question for
the seats, not something this rig can settle. The application treats a missing or failing
loader as a fall-back to the natural-size decode, never as a broken image, so the worst
case there is a soft enlargement rather than an absent one.
