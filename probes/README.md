# Probes

Standalone C programs that measure a GTK/GtkSourceView runtime behaviour this
project depends on. They are **artefacts, not gates** — nothing in
`scripts/pipeline.steps` runs them, and nothing should. They exist so a claim
about the toolkit can be re-run by whoever doubts it, rather than believed
because a message once said so.

Seven are C on purpose: each has to be runnable against an arbitrary installed
GTK, by a seat that may not have this crate building, in order to compare two
platforms' *toolkits* rather than two platforms' builds of us.

The eighth is Rust, because its subject is the **gtk4-rs marshalling layer** and a
C probe cannot prove anything about a Rust trampoline. It is a standalone crate
with its own `[workspace]`, deliberately outside the application's, so that it is
never built, linted or gated with the app — it asserts a property of the binding,
so it is *expected* to start failing when the binding is fixed upstream, and that
must not read as a Scribobulate regression. Its `Cargo.lock` is committed: the
measurement is about a specific binding version's marshalling, so a floating lock
would change the subject.

## Why these exist

All eight were written on the macOS seat during the lone-carriage-return work,
to supply the one leg no other seat could: **GTK 4.22.4 / GtkSourceView 5.20.0 /
Quartz**. The Linux seat measured 4.6.9 / 5.4.1 / X11 and the researcher read the
C source; the open question was whether GtkSourceView's *tag geometry* had moved
between 5.4.1 and 5.20.0, which is the one claim that cannot be settled by
diffing GTK.

It had not. Every one of them reproduces the Linux numbers.

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

# the Rust one builds and runs itself
cargo run --manifest-path probes/binding-shape-rs/Cargo.toml
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
