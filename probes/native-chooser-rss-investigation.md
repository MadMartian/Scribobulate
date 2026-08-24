# The macOS native-file-chooser RSS investigation

**What this is.** The full investigation behind `sdd/ISSUES.md`'s entry on native
file-chooser RSS growth on macOS: measurements, attributions, the retain-cycle analysis, the
upstream patch sketch and its two hazards, and the several ways the instruments misled the
people reading them.

**Why it is here and not in the register.** It was 56 KB inside `sdd/ISSUES.md` — 55% of that
file — and an issue entry exists in order to be DELETED when the defect is fixed. A dossier
that dies with the entry is a dossier nobody can consult afterwards, and a register that
cannot shrink has stopped being a register. So the debt entry keeps the symptom, the status
and the decision; the evidence lives here, beside `native-chooser-rss.m`, the probe that
produced most of it.

**Provenance, stated once and correctly.** The measurements were taken across BOTH locked and
unlocked sessions on the macOS seat, and the two are not interchangeable — the locked→unlocked
delta is itself evidence. Every figure below carries its own condition. An earlier rendering of
this material asserted three times that all figures came from a locked session while presenting
unlocked ones and declaring the unlocked gate passed; that claim is withdrawn, and the
per-figure labels are the authority.

**Reading it.** Content authored by the macOS seat and MEASURED there except where a line says
INFERRED. Source claims are version- and branch-scoped; where a tree is not named, treat the
claim as unscoped and re-derive before relying on it.

---

## S. Every native file chooser invocation grows RSS on macOS

**Severity**: Medium (unbounded within everything measured — no plateau over 20
cycles — and it affects every file dialog in the application, not one feature. Not a
crash, not data loss, and slow enough that an ordinary session will not notice it.)

**macOS only, MEASURED.** Opening a `GtkFileChooserNative` and **cancelling it** —
never completing an operation — grows the process's RSS by roughly **0.9–1.4 MB per
invocation**, steadily, with no plateau across 20 cycles. A large one-time jump
(~24–28 MB) accompanies the *first* construction in a process and is separate: that
is the native-panel machinery warming up and it does not recur.

Measured on macOS 4.22.4/Quartz, release build, window fixed, RSS via `ps -o rss=`:

| probe | cycles | growth | per cycle |
|---|---|---|---|
| Export chooser, opened and **cancelled** | 19 | 170528 → 192592 KB | ~1160 KB |
| File ▸ Open chooser, opened and **cancelled**, fresh instance | 19 | 169776 → 186304 KB | ~870 KB |
| Full export cycle (chooser + real export, PDF) | 20 | 175648 → 198784 KB | ~1150 KB |
| Full export cycle (chooser + real export, HTML) | 10 | 197440 → 209504 KB | ~1206 KB |

**It is not the export path**, and the numbers above are what establish that: a
cancelled chooser that runs no export at all grows at the same rate as a completed
export, and the plain **Open** dialog — which has nothing to do with exporting —
grows the same way. Export is simply a new place that opens a chooser repeatedly,
which is how this surfaced. Do not file it against the export feature.

**It is not a classic leak.** `leaks <pid>` against a live instance after 30 combined
cycles reported **472 leaks / ~103 KB**, against an RSS climb of ~34 MB over the same
run — two orders of magnitude apart. A second instance measured 353 leaks / ~96 KB
against a comparable climb. Whatever is accumulating is still **reachable**, so it
reads as fragmentation or a growing cache (font map, panel construction/teardown, some
platform-side pool) rather than unreachable memory.

**The application's own reference discipline is not the suspect.** Both call sites —
`window/export.rs`'s `choose_destination` and `app/appactions.rs`'s open action — go
through `saferizer::native_dialog::NativeDialogHolder`, which takes exactly one
external strong reference and drops it once when `response` fires, and both call
`destroy()` on the dialog in the response handler. That is the shape ScrAP-41
prescribes, and it is the same code on every platform.

**Linux does not reproduce it — with an important caveat about what was measured.**
An in-process probe constructing, showing, cancelling and dropping 40 `FileChooserNative`
dialogs under Xvfb (GTK 4.6, release, RSS via `/proc/self/statm`) ended **+2612 KB over
40 cycles, ~65 KB/cycle, and non-monotonic** — the reading fell twice (+2936 KB at cycle
10, +664 at 20, +700 at 30, +2612 at 40), which is noise around a flat baseline rather
than growth. **But the caveat is the whole value of the result:** with no portal present,
GTK on Linux backs `FileChooserNative` with its own in-process `GtkFileChooserDialog`
widget (the run emits "GtkDialog mapped without a transient parent"), whereas macOS backs
it with a real `NSOpenPanel`/`NSSavePanel`. So this is a negative about **a different
implementation**, not about the same code path behaving differently. It narrows the
suspect to the platform panel; it does not exonerate anything portable.

**Windows does not reproduce it either — and that negative is the informative one.** The
Linux non-reproduction came with a caveat: GTK backs `FileChooserNative` with its own
in-process widget there, so it measured a different implementation. **Windows closes that
gap.** It uses a genuine native panel — the Win32 common dialog, a separate shell-owned
window, gated per cycle by reading its title — and over **40 open-and-cancel invocations**
grew **+5.9 MB total, decelerating** (+2.9 MB in the first five cycles, +1.6 MB across
cycles 20→40): lazy one-time initialisation of the shell dialog machinery, not a
per-invocation cost. macOS's sustained ~1 MB/invocation would have been ≈ +40 MB over the
same run.

So **"a native panel implies the climb" is refuted**: two native-panel platforms, opposite
behaviour. Whatever this is, it is specific to macOS's `NSOpenPanel`/`NSSavePanel` path
rather than a property of `GtkFileChooserNative`'s native-dialog architecture. That is a
real narrowing of the search and it should be the starting point for whoever attributes it.

### ◐ ATTRIBUTED TO THE PLATFORM, NOT TO GTK — and not fixable by anyone here

**⚠️ PROVISIONAL: every measurement below was taken with the Mac's screen LOCKED, and this
finding is specifically about panel PRESENTATION — the powerbox / remote-view path a locked
screen is most likely to alter. An unlocked re-run is owed before any of it is quoted
outside this file.**

**The cost is MOSTLY AppKit's own price for presenting an `NSSavePanel` — about four fifths of
it — with a real GTK component on top.** Measured with **no GTK and no GLib in the process at
all**, a panel retained exactly as GTK retains it, 20 cycles per run, n=3 (GTK cell n=5).
**Both columns are kept deliberately**: the locked→unlocked delta is evidence in its own right
about what a locked session does to this measurement.

| configuration | locked | UNLOCKED | delta |
|---|---|---|---|
| AppKit, `orderOut:` only | 558.0 | **518.5** | −39.5 (−7.1%) |
| AppKit + accessory view | 973.7 | **918.7** | −55.0 (−5.6%) |
| AppKit, reuse panel + accessory | 73.3 | **49.4** | −23.9 (−32.6%) |
| AppKit, reuse + reconfigure | 78.3 | **53.9** | −24.4 (−31.2%) |
| AppKit, cached popup repopulated | 76.6 | **52.2** | −24.4 (−31.9%) |
| **GTK, the application's shape** | 927.7 | **1136.8** | **+209.1 (+22.5%)** |

⛔ **WITHDRAWN: "GTK is marginally cheaper than bare AppKit; there is no GTK-attributable
component in the bytes."** That was a **locked-session artefact and the direction flips.**
Like-for-like, accessory view in both: locked, GTK 927.7 vs AppKit 973.7 — GTK cheaper by 46.0
(−4.7%); unlocked, GTK 1136.8 vs AppKit 918.7 — **GTK DEARER by 218.1 (+23.7%)**. So there *is*
a GTK-attributable component, roughly **a fifth of the total**, and it was **invisible while
the screen was locked**. AppKit's 918.7 remains the dominant term at about 81%.

**Do not quote the 218 as a precise figure.** Two caveats, both from the seat that measured it:
the cells are not perfectly like-for-like — the GTK cell carries a transient parent window and
the internal `GtkFileChooserDialog` widget tree, the AppKit control carries neither — and GTK's
spread widened from 2.5% locked to ~10% unlocked (1088.0–1196.6). The defensible statement is
**"GTK adds roughly a fifth on top; direction confirmed, magnitude approximate."** INFERRED
mechanism, untested: with a real display the `GdkMacosWindow` parent and GTK's internal chooser
actually render and allocate, where locked they cost almost nothing.

✅ **Every non-byte finding is untouched, and the refcount evidence is IDENTICAL rather than
merely similar**: live panels 20 fresh / 1 reused; deallocs 0; `_retainedSelf` == the panel
with the same flag values; all six properties persisting on a cached panel; the extension
doubling reproducing exactly; state hygiene 20/20.

✅ **And the reuse mitigation got BETTER**: AppKit fresh 918.7 → reused 49.4 is a **94.6%**
saving unlocked, against 92.5% locked.

**The locked-screen caveat earned its keep, and that is worth stating plainly**: carrying it
cost a delay and it **changed one sign**. Every conclusion that rested on counts survived
untouched; the one that rested on a between-mode byte comparison inverted. That is the
sharpest available illustration of this entry's own rule 1 — the counts were evidence, the
byte differences were not.

That also explains the platform split this entry opens with, which nothing else did: Windows
and Linux never reproduced it because it is the macOS panel's own cost, not a property of
`GtkFileChooserNative`.

**And the retainer is now NAMED: the panel retains ITSELF.** macOS 26.6.1's `NSSavePanel`
carries an ivar `_retainedSelf`, read directly after dismissal in both modes:

```
_retainedSelf = 0x8af560600, panel = 0x8af560600, same = YES,
_panelCompleted = 1, _panelIsNowUseless = 0, _observingBridge = 1
```

The panel holds a strong reference to itself, still held after the completion handler has run
and after `orderOut:`/`close`. That is `DEALLOCATED = 0` explained in one ivar; it is
in-process AppKit state; it reproduces with no GTK anywhere in the process; and **a
self-retain we do not own cannot be balanced by GTK or by this application.** MEASURED: the
ivar values. INFERRED: that AppKit never balances it — from zero deallocations across runs up
to 40 cycles.

`_openWindows` is separately exonerated: relaunching with
`-NSApplicationStronglyReferencesOpenWindows NO` changes nothing (563.4 / 544.0 / 553.3,
n=3).

**"Never returned" is now tested rather than assumed.** Twenty cycles, then **45 seconds idle
at base run-loop level**, n=2: RSS 83168 → 83104 (−64 KB) and 83232 → 83184 (−48 KB), against
~10.7 MB accumulated. Live panels still 20, deallocs still 0. **0.5% comes back and the rest
stays** — there is no deferred drain and no delayed reclaim. That is also the last nail in the
retracted GDK autorelease-pool lead: undrained autoreleases would have come back on an idle
base-level loop, and they do not.

### ✅ ~93% IS RECOVERABLE UPSTREAM — by REUSING the panel, not by releasing it

**This is the first thing in the investigation that moves the number**, and it arrived by
testing the one configuration nobody had: not *releasing* a fresh panel better, but not
creating a fresh one at all. Pure AppKit, no GTK, 20 presentations per run, n=3, KB per
presentation — all cells locked-session and therefore internally comparable even before the
unlocked re-run:

| configuration | KB/presentation |
|---|---|
| fresh panel + fresh accessory view — **GTK's exact shape** | 980.2 |
| fresh panel, no accessory view | 574.3 |
| **reused** panel + fresh accessory view | 395.0 |
| **reused** panel + **reused** accessory view | **73.3** |
| (panel never shown at all, for scale) | ~21 |

**Caching both objects removes ~94.6% of the cost** (unlocked; 92.5% locked), and the decomposition is clean and
additive: a freshly created panel costs ~510 KB that is never reclaimed, a freshly created
accessory view costs ~330 KB of its own, and the two are independent — reusing the panel
while still building a new popup each time only reaches 395. A 40-cycle reuse run holds at
60.3 KB/cycle, so the residual is small and does not compound the way fresh allocation does.

GTK allocates **both** fresh on every invocation — the panel in `filechooser_quartz_launch`,
the `FilterComboBox` at `:314`. Caching them (at minimum one `NSSavePanel` and one
`NSOpenPanel`, which are different classes) is an upstream change with a measured number
attached, which is exactly what the four earlier remedy proposals lacked.

⛔ **The mitigation is NOT "hold the panel in a static" — it has a hard prerequisite.**
Every property GTK sets on the panel is set with a **one-sided guard and no else-branch
reset**, which is safe only because the panel is brand new each time:

```
if (data->select_multiple) [panel setAllowsMultipleSelection:YES];   /* :253-256, no else */
if (data->create_folders)  [panel setCanCreateDirectories:YES];      /* :260-263, no else */
if (data->accept_label)    [data->panel setPrompt:...];              /* :277-278, no else */
if (data->title)           [data->panel setTitle:...];               /* :280-281, no else */
if (data->message)         [data->panel setMessage:...];             /* :283-284, no else */
if (data->filters) { ... [data->panel setAccessoryView:...]; }       /* :313-350, no else */
```

On a cached panel each becomes stale state: a multi-select Open followed by a single-select
one still allows multiple selection; a titled chooser followed by an untitled one keeps the
old title; and a filtered chooser followed by an unfiltered one **keeps the previous accessory
view** — which is also the ~330 KB object measured independently above. So the mitigation is
"**convert configuration from partial to TOTAL**": every property assigned unconditionally on
every launch, including explicit resets when the GTK-side value is NULL or false, and
`accessoryView = nil` when there are no filters. **This is a stated prerequisite, not a
footnote** — a maintainer who caches without it ships six stale-state bugs, and they land on
users rather than on whoever files this. Two conveniences in the same finding: GTK sets no
delegate on the panel at all (no `setDelegate:` in the file), so there is no delegate-rewiring
hazard; and the cache must be keyed by **class** — `NSOpenPanel` and `NSSavePanel` are chosen
by action at `:229`/`:237`/`:251`, so it is two cached instances minimum.

✅ **Re-presentation of a completed panel WORKS — measured, so the thesis lives.** One panel
instance re-presented and reconfigured every cycle (distinct title, prompt,
`nameFieldStringValue`, `directoryURL`), reading state back off the live panel after each
presentation: **12/12 presentations returned that cycle's configuration, 0 mismatches**, twice,
n=2. No stale name, folder, title or prompt; `-URL` derives from the current cycle's directory
and name; `_panelIsNowUseless` stayed 0 through the last cycle.

⛔ **BUT CACHING THE FILTER CONTROL AS WRITTEN IS A USE-AFTER-FREE.** Verified on **GTK 4.6.9**
(`/opt/dev/oss/gtk`, branch `gtk-4-6` @ `492b44f20c`) — and the same shape holds on `main`
under different names, see the version table below:

```objc
@interface FilterComboBox : NSObject<NSComboBoxDelegate>   /* :75 */
- (id) initWithData:(FileChooserQuartzData *) quartz_data
{ [super init]; data = quartz_data; return self; }          /* :85-90 — raw pointer, never updated */
```

`comboBoxSelectionDidChange:` (`:91-106`) dereferences it hard — `data->filter_combo_box`,
`data->filters`, `data->panel`, `data->self` — ending in
`g_object_notify (G_OBJECT (data->self), "filter")`, and `filechooser_quartz_data_free` ends
with `g_free (data)`. So a **cached** control holds a dangling pointer from the first
invocation's teardown onward, and the first filter change on the second invocation is four
use-after-frees, one of them on freed GObject memory. **A crash, not a leak.** The prerequisite
is to re-point `data` on every launch.

Note the delegate object at `:306` — `[data->filter_combo_box setDelegate:[[FilterComboBox
alloc] initWithData:data]]` — is `alloc`'d inline and never released, and AppKit holds a
delegate weakly, so that unbalanced `+1` is the only thing keeping it alive. Both facts matter
to any caching patch.

⚠️ **A VERSION WARNING that cost this entry real accuracy — TWICE, in opposite directions.**

The Quartz filter control **differs between GTK 4.6 and main**, and every seat here got burned
by not saying which tree it meant:

| | GTK 4.6.9 (`gtk-4-6`) | `origin/main` |
|---|---|---|
| ivar | `filter_combo_box` (`:69`) | `filter_popup_button` (`:73`) |
| class | `FilterComboBox : NSObject<NSComboBoxDelegate>` (`:75`) | `FilterComboBox : NSPopUpButton` (`:79`) |
| control | `NSComboBox`, delegate installed via `setDelegate:` (`:306`) | `NSPopUpButton` subclass, allocated directly (`:314`) |
| handler | `comboBoxSelectionDidChange:` (`:91-106`) | `popUpButtonSelectionChanged:` (`:99`) |
| populate | `addItemsWithObjectValues:` (`:304`) | `addItemsWithTitles:` (`:315`) |
| launch priming | none | `[… popUpButtonSelectionChanged:NULL]` (`:348`) |

The reimplementation landed in `2a96dde115` ("macos: use NSPopUpButton for filter selection in
native filechooser"). **Direction matters and was initially got backwards here:
`NSComboBox` is the OLDER shape.** 4.6 is the pre-2023 code; `main` and the 4.22.4 the
measurements run on are both post-migration. Settled by the strongest artefact available — the
**shipped binary** on the measuring machine
(`/opt/homebrew/Cellar/gtk4/4.22.4/lib/libgtk-4.1.dylib`): `nm -u` imports
`_OBJC_CLASS_$_NSPopUpButton` and contains **zero** `NSComboBox` references; `strings` has
`popUpButtonSelectionChanged:` and `addItemsWithTitles:` and lacks
`comboBoxSelectionDidChange:` and `addItemsWithObjectValues:`.

**⚠️ Consequence for anyone patching this: the fix must be authored twice, the 4.6 variant is
the WORSE bug, and its naive fix is a USE-AFTER-FREE.**

On 4.6 there are **two** unbalanced allocations where `main` has one, and
`filechooser_quartz_data_free` (`:182-205`) releases only `filters` and `filter_names` —
`filter_combo_box` appears nowhere in it:

- the `NSComboBox` at `:303` — an unbalanced `+1` **with** a handle, the direct analogue of
  `main`'s `filter_popup_button`;
- the delegate at `:306`, `[data->filter_combo_box setDelegate:[[FilterComboBox alloc]
  initWithData:data]]` — an unbalanced `+1` with **no handle**, never stored anywhere.

⛔ **And that second leak is LOAD-BEARING.** AppKit holds delegates **weakly**, so the combo
box does not own it — the unbalanced retain is *precisely what stops the combo box holding a
dangling delegate pointer*. A 4.6 backport that simply releases the delegate **converts a leak
into a use-after-free**. Any 4.6 fix must `[filter_combo_box setDelegate:nil]` *before*
releasing, and must capture the pointer first because none is kept today.

**That is the SECOND naive patch on this file that ships a use-after-free** — the other being
a cached filter control whose raw `data` pointer is never re-pointed. Two independent
"obvious" fixes, both crashes.

⚠️ **And a THIRD way the obvious patch fails, which is quieter: it balances the books without
freeing anything.** `setAccessoryView:` **retains its argument**, so releasing
`filter_popup_button` while the panel still references it balances GTK's `alloc` and
**cannot deallocate the object**. The correct order is Qt's, and Qt does exactly this:
`orderOut:` → `setAccessoryView:nil` → release the popup button → release the panel. A patch
that omits the `nil` looks right, passes review, and reclaims nothing.

✅ **AND THE ORDER THAT INCLUDES IT DOES RECOVER — MEASURED, which upgrades part of the
ownership fix from correctness to footprint.** A dealloc spy on the popup, emulating GTK's
ownership exactly: releasing with `accessoryView` still set gives **0/20 deallocations at
932.2 KB/cycle**; `setAccessoryView:nil` *then* release gives **19/20 at 542.3** — about
**390 KB per invocation reclaimed**. Corroborated from inside GTK, interleaved and paired,
n=5: with-filter 1156.7 vs without-filter 735.3, a per-rep delta of **421.4 KB**
(range 344–492) bracketing the 390 measured in a process containing no GTK at all.

**Read the two figures with their scopes, which differ**: 390 KB is the AppKit-only
accessory component; GTK's 421 KB delta is filter-vs-no-filter and therefore also carries
`GtkFileFilter` and the internal chooser's filter handling, so it **bounds the accessory
component from above** rather than measuring it.

**So "no ownership fix recovers a byte" was too pessimistic and is corrected.** It holds for
the *panel* retain, which `_retainedSelf` pins regardless. It does **not** hold for the
accessory view: that half is a real ~390 KB per invocation, and an upstream report may say
so.

**On this file, assume the obvious fix is wrong until the ownership graph is drawn.** Three
independent "obvious" patches now: two crash, one silently no-ops.

**Not this project's exposure either way**: the macOS seat runs 4.22.4, so the 4.6 variant is
noted for the upstream fix and is not ours to chase.

**This seat's own error, recorded because it is the sharper of the two.** Told that the
researcher's identifiers "did not exist", this seat read `/opt/dev/oss/gtk`, asserted it was
current `main`, and ran `git log -S "NSPopUpButton"` — which returned nothing and was reported
as "never existed in GTK, across the entire repository history". **The checkout was on branch
`gtk-4-6` at 4.6.9**, so `-S` searched only that branch's ancestry and could not see a change
made on `main` after the 4.6 branch point. The researcher's identifiers were correct for `main`
and for the 4.22.4 the measurements run on; this seat's were correct for 4.6.9; and the
confident falsification was the least accurate claim of the three. `git log --all -S` finds it
in two seconds.

**The standing rule that comes out of it**: state the tree with every source claim — branch and
commit, not "the source" — and remember that `git log -S` is scoped to the ancestry of whatever
`HEAD` happens to be. A repository-wide negative requires `--all`.

✅ **The popup half survives repopulation — so the headline is firm at ~92%.** GTK
repopulates the filter control per invocation because the filter list changes
per call, and a cached popup refilled with a different list each cycle costs **76.6 KB per
presentation** (75.8 / 76.6 / 77.5) against 73.3 for a static one. The item list is not where
the money is. So caching both objects, with both reconfigured per invocation, removes **~94.6%**
of the per-invocation cost, and both halves are now measured rather than one measured and one
expected.

⛔ **The stale-state prerequisite is now MEASURED, not inferred from source, and it is worse
than the source read suggested.** An `NSOpenPanel` configured on cycle 1 (multi-select,
`canChooseDirectories`, title, message, prompt, accessory view) and then presented on cycle 2
with **nothing set at all** — exactly what GTK does for an untitled, unfiltered, single-select
chooser — reads back:

```
multiSelect=1  title='First invocation title'  message='First invocation message'
prompt='Choose'  accessoryView=NSPopUpButton  canChooseDirs=1
_panelCompleted=1  _panelIsNowUseless=0
```

**All six persist. AppKit resets nothing between presentations.** Every one of GTK's one-sided
`if` configurations therefore becomes a stale-state bug on a cached panel — including
filtered → unfiltered keeping the previous accessory view, which is simultaneously the ~330 KB
object being reclaimed. The same run also settles two mechanics: `_panelIsNowUseless` stays 0
across the second presentation, and `_panelCompleted` stays 1 after the first completion
*without* preventing re-presentation — **"completed" is not "spent"**.

### The caching MR's FOUR non-optional preconditions

It must not be filed as "hold it in a static". All four are measured or source-verified:

1. **Re-point the popup's `data` pointer on every launch** (`setData:`) — otherwise it is a
   use-after-free, above. The difference between reclaiming 330 KB and shipping a crash.
2. **Repopulate the popup's items per invocation** — free, per the 76.6 measurement.
3. **Make panel configuration TOTAL**, with explicit resets when the GTK-side value is
   NULL/false and `accessoryView = nil` when there are no filters — otherwise six stale-state
   bugs, measured above.
4. **Reset the name field explicitly on every launch, and VERIFY THE RETURNED URL rather than
   the properties.** Not an ordering fix — see below. This is the nastiest of the four,
   because every property-level check passes while the answer is wrong.

**The extension artifacts, MEASURED — and there are TWO of them, only one caused by caching.**
`allowedContentTypes` itself replaces cleanly across reconfiguration (8/8, alternating real
UTType lists, no residue). But the panel rewrites the name field's extension to match the
current type list, and a fresh-panel control separates what that produces:

| cycle | name set | types set | FRESH panel (GTK today) | CACHED panel |
|---|---|---|---|---|
| 1 | `doc-1.pdf` | PDF | `doc-1.pdf` ✅ | `doc-1.pdf` ✅ |
| 2 | `doc-2.pdf` | HTML | `doc-2.html` | `doc-2.html` |
| 3 | `doc-3.pdf` | PDF | `doc-3.pdf` ✅ | **`doc-3.pdf.pdf`** ⛔ |

- **Cycle 2 is PRE-EXISTING and out of scope.** The extension rewrite happens on a fresh panel
  too, so it is in GTK today, unrelated to caching, and probably correct behaviour. A patch
  that suppressed it would change what current users get. Do not let an MR "fix" it.
- **Cycle 3 is caching-induced** and appears only on the cached panel. That is the real
  precondition.

**Reversing the order relocates the artifact, it does not remove it.** GTK's order (name then
types) gives `doc-1.pdf | doc-2.html | doc-3.pdf.pdf`; reversed (types then name) gives
`doc-1.pdf | doc-2.pdf.html | doc-3.pdf` — cycle 3 comes out clean and cycle 2 breaks instead.
So there is no one-line ordering fix, and **both orders leave a cycle where every property
reads back correctly and the URL is wrong.** Hence: explicit name reset per launch, and verify
the URL.

### ⛔ A CRASH CLAIM THAT WAS FALSIFIED — kept because the falsification is the lesson

An "empty filter array crashes the chooser" defect was worked up here as source-verified,
gate-free and filable ahead of everything else. **It does not crash.** Measured on macOS
26.6.1: an end-to-end repro (a named `GtkFileFilter` with no patterns and no MIME types, added
to a chooser and shown) exits 0 having reached the apply path; and the premise tested directly
against a bare `NSSavePanel` with no GTK — `setAllowedFileTypes:` with `@[]`, with `nil`, and
with `@[@""]` — raises **no exception in any case**.

**Steps 1–3 of the chain are still correct** and independently verified twice: a rule-less
filter yields a non-nil zero-element array (`_gtk_file_filter_get_as_pattern_nsstrings`
returns `NULL` only on a MIME→UTI failure), `file_filter_to_quartz` guards `== NULL` only, and
the empty array reaches `setAllowedFileTypes:`. **Step 4 was wrong**: Apple's SDK header says
*"If the array is not nil and the array contains no items, an exception will be raised"*, and
that **documented contract was treated as observed behaviour**. The shipping implementation on
26 does not enforce it — plausibly because the deprecated property is now bridged onto
`allowedContentTypes`, whose "no restriction" value is precisely `@[]`. Unverifiable here: it
may still hold on macOS 11–13, and nobody has one.

**The transferable rule, which is the whole reason this is written up rather than deleted:**

> **An SDK header or documented contract is primary evidence for API SURFACE only** — "this
> class is not a singleton", `@property (copy)`, `API_DEPRECATED`. **Any claim about RUNTIME
> BEHAVIOUR** — an exception raised, an object freed, a handler invoked — **needs an
> observation.** A deprecated symbol's documented contract should be assumed stale until
> observed, since a deprecated API is exactly the one whose implementation has been rewritten
> underneath its documentation.

The disproof was in the same message as the claim: the deprecation note and the `nil`-vs-`@[]`
sentinel difference were both written down, and a deprecated property bridged onto a successor
whose "no restriction" value is `@[]` is precisely the circumstance in which the old contract
would be relaxed. Both halves of the counter-argument were on the page and no conclusion was
drawn from them.

**What survives**: not a crash and not a user-visible bug — an empty `allowedFileTypes` is
accepted and behaves as "no restriction", which is what the `nil` branch would have done. The
guard is still defensible as **robustness**, and in a form that cannot be falsified by anyone's
OS version: **a zero-rule filter should not produce a popup entry at all**, because it cannot
usefully appear in the list. And the deprecation note stands on its own, being an API-surface
claim: `setAllowedFileTypes:` is deprecated since macOS 12 and still called on 26, and
`allowedFileTypes` uses `nil` for "no restriction" where `allowedContentTypes` uses `@[]`, so
carrying the sentinel across inverts the branch.

**Do not file this as a crash report.** A maintainer who runs it on current macOS gets the
negative, and a first filing that does not reproduce costs more than the gated items put
together. Fold the guard and the deprecation note into the ownership issue as labelled
robustness items.

**Limits the caching recommendation must carry****Limits the caching recommendation must carry**, none of them reachable from a locked
headless probe: an actual **accept** has never been exercised (every measurement cancels, and
`-ok:` has been non-callable programmatically since 10.15), so "returns the correct URL when
the user picks a file" is inferred from the panel's derived `-URL`; **Save ↔ Open on one instance is impossible** (different classes, so a cache
is two instances minimum); and **sandboxed powerbox behaviour** is untested.

📏 **A numerical caution for anyone quoting these**: per-presentation figures are **not
comparable across different cycle counts**. The 12-cycle runs read 106.2 and 91.6 KB/cycle
against 74.9–80.8 for the same configuration at 20 cycles, because a fixed warm-up is
amortised over fewer presentations. Compare like with like or the mitigation looks worse than
it is.

**Severity note.** Before the reuse finding, the honest status was closer to
**intractable-but-real** than to open-pending-fix. It is no longer: there is a reachable
upstream change with a measured recovery. It stays **Medium and open**. Two things gate any
severity change in either direction — the unlocked re-run (every figure here is from a locked
session, against the presentation path a lock is most likely to alter) and the reconfiguration
probe above.

**Consequences, and they are unusually tidy:**

- **Every failed remedy now has ONE explanation instead of four.** Reaping non-visible
  panels, releasing the panel, releasing the accessory view — all recovered nothing, because
  none of the cost was ever GTK's to reclaim.
- **GTK's unbalanced retains are still real defects** (below), and still worth reporting
  upstream — but they are **not** the memory story, and a fix for them will not move this
  number. Anyone filing them must not claim a footprint impact it cannot deliver.
- ⚠️ **"No fix at any layer changes the footprint" was TRUE of every remedy tried, and is
  now FALSE.** See the reuse finding below — the one configuration nobody had tested.
  Recovering it is an upstream GTK change, not an app-side one, so the prohibition on
  app-side mitigation is untouched.

⚠️ **Do not restate this cost against the 50 MiB ceiling.** That ceiling (TDD §6) is a
**VRAM** contract and this is resident memory; they are different budgets. The arithmetic is
tempting and has already been attempted twice from two directions. If a severity statement is
needed, state it in RSS terms and say so.

---

**The GTK ownership defects — real, source-verified, and now known NOT to be this issue's
cause.** Kept because they are worth an upstream report on correctness grounds, and because
the reasoning behind each retraction is the useful part.

**What the source says, and what that does NOT establish.**
`gtk/gtkfilechoosernativequartz.c` runs two inconsistent ownership models on its two exit
paths, and this part is source-verified against `origin/main`:

- `filechooser_quartz_launch` takes an explicit `+1` on the panel and sets
  `setReleasedWhenClosed:YES`. **The ordinary path** (the user picks a file, or cancels)
  sends only `[data->panel orderOut:nil]`; `orderOut:` does not trigger
  `releasedWhenClosed`, and there is no `[data->panel release]` anywhere in the file. The
  missing release is real.
- **The programmatic-hide path** (`gtk_file_chooser_native_quartz_hide`) sends `[panel
  close]`, which *does* balance the panel retain — but it never calls
  `filechooser_quartz_data_free` and never clears `self->mode_data`, and `show()` overwrites
  `mode_data` unconditionally, so it orphans the **entire `FileChooserQuartzData`** instead,
  including the `g_object_ref(self)` taken at launch. That is why
  `gtk_file_chooser_native_finalize` — and `gtk_window_destroy(self->dialog)` — never runs.
  MEASURED and reproduced by GObject instance count: after 12 cycles, 12 live
  `GtkFileChooserNative`, 12 `GtkFileChooserDialog`, 12 `GtkFileChooserWidget`, 1632
  `GtkColumnViewCell`. **This half stands.**

⛔ **What does NOT follow, and was wrongly asserted here: that the missing release explains
this issue's RSS growth.** A dealloc spy reports panels **deallocated = 0 in every mode** —
including a mode that adds an explicit `[panel release]` balancing GTK's retain.

**The documented mechanism was checked and RULED OUT — which is a better position than
naming it.** Apple's AppKit Release Notes for macOS 10.13 ("NSWindow Lifecycle Changes") say
an ordered-in `NSWindow` is strongly referenced by AppKit *until it is explicitly ordered-out
**or** closed*. That looked like the answer for about an hour. It is not: a class-dump shows
`NSApplication` keeps **two** window lists (`_windowList` and `_openWindows`, with distinct
`_addWindow:`/`_addOpenWindow:` pairs), only `_openWindows` retains, and the reference it
holds is released on order-out — and GTK **does** send `[data->panel orderOut:nil]` on the
transient-parent branch, which is the branch every measured run took. So the documented
AppKit reference is already gone in the exact configuration that measured
`DEALLOCATED = 0`. Two related corrections come with it: `[NSApp windows]` is documented as
"all currently existing windows", so **membership proves the object is alive, not that the
list retains it** — the +1-per-invocation count is consistent with either — and
`-[NSSavePanel cancel:]` fired the completion handler **20/20** where `close` fired it
**0/12**, so ending the panel *session* and closing the *window* are not interchangeable.

⛔ **SUPERSEDED hypothesis, kept because its shape was right and its mechanism was wrong.**
The panel *is* an out-of-process object — `NSSavePanel` conforms to the remote-view exported
protocol — and the leading guess was that `NSXPCConnection`'s strong `exportedObject` slot
pinned it. Wrong mechanism: the connection is not what holds it, the panel holds *itself*
(above). Right neighbourhood, wrong object, and it would have sent the next investigator to
ViewBridge.

**Not a macOS 26 regression, which keeps severity where it is.** Class-dump comparison across
AppKit versions: on 1671.20.108 (≈10.14) `NSSavePanel : NSPanel <FIFinderViewDelegate,
NSTouchBarDelegate>` has none of `retained`, `panelCompleted`, `panelIsNowUseless` or
`observingBridge` — the mechanism is **absent**. On 1865.10.102 (≈11) it conforms to
`NSOpenAndSavePanelRemoteViewExportedToServiceProtocol` and carries all four as synthesised
`BOOL` properties — **present**. On 26.6.1 the flag `BOOL retained` has become the pointer
`_retainedSelf` and the protocol is renamed. Same mechanism, refactored, and it arrives *with*
the panel going out of process in the 10.15/11 window. VERIFIED: absent on 10.14, present on
11. INFERRED: that it was never balanced back then — a flag existing does not prove it was
never released, and the never-balanced half rests on the 26-only zero-dealloc evidence. So
this is roughly **five-year-old platform behaviour, not a fresh regression**.

**Open sub-question, stated rather than left silent, because the two answers read very
differently to Apple**: the *mechanism* is verified absent on 10.14 and present on 11, but
whether it was ever *balanced* on 11/12/13 is **unverified** — it needs a dealloc census on
one of those machines, and no seat here has one. "Apple broke this" and "this has been true
for five years" are different reports, and only the second is currently supported.

**Precedent — and its SCOPE, because it is easy to overclaim. Qt ships the balancing
release; Qt does NOT cache.**
`qtbase/src/plugins/platforms/cocoa/qcocoafiledialoghelper.mm` takes the *same*
`[[NSSavePanel savePanel] retain]` GTK takes, and in `dealloc` does
`[m_panel orderOut:m_panel]` … `[m_panel release]` — order-out **first**, release **second**.
That is the ordering the whole investigation converged on independently, and "Qt, in the same
position, does this" costs nothing to say — **about ownership**. It is not a caching precedent:
`QCocoaFileDialogHelper::show()` builds a fresh delegate on every show and the panel is
allocated in that delegate's `init`, so Qt allocates a fresh panel per invocation exactly as
GTK does. Citing Qt for the caching recommendation would be cited straight back.

**And that null answer is worth more than a precedent would have been.** On these numbers Qt
should exhibit the same ~510 KB per presentation — and it does: QTBUG-39205 ("Memory leak bug
of QFileDialog") plus forum reports of `QFileDialog` reaching 200–300 MB over repeated opens,
which this entry previously carried as "unconfirmed, may not be the same defect". They now
have a mechanism that predicts them, which upgrades them from padding to **corroboration** —
a second major toolkit, same lifecycle, same symptom, never diagnosed, entirely independent of
GTK.

**Adjacent platform behaviour on this exact macOS version**: Mozilla bug 2053177 (macOS 26,
resolved fixed) records that the `NSSavePanel`/`NSOpenPanel` sheet completion handler can run
*before* the panel's modal session has fully unwound; Firefox's remedy is to defer its
completion work by one main-thread turn. The measurements here are on macOS 26.6.1, so
AppKit is plausibly still tearing down while GTK's handler runs its `orderOut:`, its
`data_free` and its response emission.

**Cross-toolkit corroboration, none of it confirmed to be this defect**: an unanswered Apple
Developer Forums report of per-invocation growth under `NSOpenPanel` measured with both
`leaks` and `ps`; wxWidgets #22173 (a lifetime bug on repeated panel presentation, with
`NSRemoteView`/ViewBridge warnings); Qt QTBUG-39205. No Apple radar and no acknowledged
OS-side leak. "This shape has been seen in three other toolkits and never diagnosed" is
context, not evidence.

⚠️ **The residual puzzle is therefore still open, and is now the whole question**: with the
documented AppKit reference ruled out, something else retains the panel. The mechanism behind
the per-invocation cost remains **unattributed**.

**What is MEASURED and survives both retractions** (macOS seat,
`probes/native-chooser-rss.m`, real user-cancel path). Object counts first, because counts
are refcount evidence and single-run byte deltas are not:

- Objects accumulate per invocation, counted directly: **+1 `NSSavePanel`/`NSOpenPanel`**,
  and **+1 `NSAccessoryViewWindow`** when a `GtkFileFilter` is present.
- **Every GTK-side instance count returns to zero** on the real cancel path
  (`GtkFileChooserNative`, `GtkFileChooserDialog`, `GtkFileChooserWidget`,
  `GtkFileSystemModel`, `GtkColumnViewCell`, `GFileInfo`). The Rust reference handling is
  exonerated by measurement, not by assertion — this result has never wavered.
- **The cost tracks PRESENTING the panel**, not anything reachable from our layer (n=3 per
  cell, interleaved): no filter **596–626 KB/cycle**; with a filter **948.5** (spread 6);
  never shown **21**; the internal `GtkFileChooserDialog` built and destroyed with no panel
  at all, **5**.
- It is **not** the folder listing (empty folder 551 vs 123-entry 606), **not** the transient
  parent (579), **not** the GTK widget tree.
- **The baseline magnitude does meet the standing rule**: **927.7 KB per invocation, n=5,
  spread 2.5%** (five 20-cycle runs, the application's exact shape — Save + `GtkFileFilter` +
  transient parent, real `-cancel:` dismissal), with the window census naming what it covers:
  `NSSavePanel` 20, `NSAccessoryViewWindow` 20, `GdkMacosWindow` 1 — exactly 1:1 per
  invocation for both panel classes. Note what that implies about the retractions below:
  **the figures that failed to reproduce were always DIFFERENCES between modes, never the
  baseline.**
- ⛔ **RETRACTED — "a pure-AppKit control is ~9.6 KB/cycle".** It was n=1 and nine subsequent
  runs disagree. It was also **confounded**: it dismissed with `close` while GTK dismisses
  with `orderOut:`, so it varied two things at once. That single number was the entire basis
  for "AppKit doesn't do this, GTK's use of it does" — the claim this entry was built on for
  most of its life. Correcting the confound did not refine the control, it **inverted** it.
  Nobody has explained the plateau that one run showed, and nobody has invented a reason for
  it.

⛔ **RETRACTED — the GDK autorelease-pool lead.** `gdk/macos/gdkmacoseventsource.c` drains one
static process-wide pool only at loop level zero, where a plain `NSApplication` drains every
iteration at every level, and that was proposed as the explanation for a GTK-vs-AppKit
asymmetry. **There is no asymmetry.** A plain `NSApplication` shows the same ~560 KB per
presentation, so there is nothing for the pool to explain. It may still stand as a separate
GDK concern on its own merits; it is not this one, and it should not be carried here as an
open lead.

### ⛔ Two retractions, and the second one is the lesson

**Retraction 1 — the byte figures.** A 2×2 of RSS-per-cycle across filter/no-filter and
reap/no-reap was measured **once per cell** and briefly landed here, concluding that the
retains were ~85% of the growth. Re-run with **n=3 interleaved**, reaping shows no benefit
at all (948.5 vs 920–937 KB/cycle). The 418 KB/cycle figure that carried the conclusion was
**noise reported as signal**. A single run per cell is exactly the shape that reads as a
clean result and is not one.

**Retraction 2 — the verdict itself, on a false negative the measurement CONSTRUCTED.** The
supporting observation was "0 live `NSSavePanel`s after reaping" — a count of `[NSApp
windows]`. But `-close` **removes a panel from that list without freeing it**, so the
instrument reported success for an operation that had freed nothing. A dealloc spy shows
panels deallocated = 0 in *every* mode, including one with an explicit `[panel release]`
balancing GTK's retain. **The panel is held by more than GTK's one reference**, and the
patch that was about to be proposed would not have freed it.

That is the transferable lesson, and it is worth more than the verdict would have been: **a
count of a registry is not a count of live objects.** Membership of `[NSApp windows]` is a
property the operation under test directly manipulates, so measuring it measured the
operation's own side effect and called it the outcome. Ask what would make the number move
*other than* the thing you are testing, and prefer an instrument the operation cannot
touch — here, a dealloc spy.

**The screen being locked was the wrong caveat**, twice over. It was attached in good faith
and it was real, but `CGSSessionScreenLockedTime` was unchanged between the reproducing and
non-reproducing runs. Repeatability, and then instrument validity, were the variables.

**Standing rules for this issue**, each bought with a retraction, in the order that earned
its place:

1. **No measurement-derived claim leaves this file without a CONTROL that varies only the one
   thing under test.** Two conclusions were within a message of publication here, and both
   were overturned by a control rather than by more source reading — the first because the
   control itself varied two things at once (`close` vs `orderOut:`), the second when that
   control was corrected and inverted the result.
   **The source reading was correct throughout, and it was still wrong four times about what
   the defect MEANT.** Reading the code told us truly what the code does; it never once told
   us whether it mattered. **And the unlocked re-run is this rule's cleanest demonstration**:
   every conclusion resting on a COUNT came back identical, while the one resting on a
   between-mode BYTE comparison inverted its sign. Every one of the four reversals came from an
   experiment — a dealloc
   spy replacing a blind counter, a two-variable control being caught, that control's re-run
   inverting the premise, and a caching probe breaking a frame four failed remedies had agreed
   on. **And the hazard is not that source reading is unreliable; it is that it felt
   authoritative each time it was insufficient.** This rule is first because it is the one
   that did the work.
2. No byte figure is recorded without **n and spread**.
3. No window-list count is treated as a liveness count.
4. **A selector, symbol or line number taken from a dumped or vendored artifact is re-checked
   against the version actually under test before it is handed to anyone as a probe** — and
   **every source claim names its tree**, branch and commit, not "the source". This one cost
   three separate errors: a selector five AppKit majors stale, identifiers carried between a
   version that was measured and a version that was read, and a repository-wide "this never
   existed" that had searched only one branch's ancestry (`git log -S` is scoped to `HEAD`'s
   ancestry; a repository-wide negative needs `--all`).
6. **An SDK header or documented contract is primary evidence for API SURFACE only.** Any claim
   about RUNTIME BEHAVIOUR — an exception raised, an object freed, a handler invoked — needs an
   observation, and a deprecated symbol's documented contract should be assumed stale until
   observed. A whole crash filing was built on a header sentence the shipping implementation no
   longer enforces.
5. **When several remedies fail identically, check whether they SHARE AN ASSUMPTION before
   concluding the problem is intractable.** Four remedies here recovered nothing — reaping
   panels, releasing the panel, releasing the accessory view, balancing the retain — and all
   four are the same move: trying to give an object back. Not one tested *not taking it in the
   first place*, which is where 93% of the cost turned out to be. Four experiments that agree
   are not independent evidence when they share a premise; the agreement read as strength and
   was a blind spot with four windows.

**And the GTK ownership defect is best described as LATENT, not as a leak.** GTK takes a `+1`
it never returns on the completion path — verified from source, contrary to Qt's practice, and
a plain `alloc` with no matching `release` in the filter-control case. But it is
currently **masked**: AppKit retains the panel regardless, so the missing release frees nothing
today. It is incorrect code that would begin to matter the moment AppKit's behaviour changed.
That is a real but modest bug, and any report of it must say so in those terms.

**The growth is genuinely UNBOUNDED, not a bounded one-time cost.** Apple's shipping SDK
headers settle it verbatim — `/* Creates a new instance of the NSSavePanel. This class is
not a singleton. */`, character-identical across the 10.13 through 14.5 SDKs and reflowed
but unchanged in 15.5/26.5. A fresh `NSPanel` + view hierarchy + (10.15+) its
`NSRemoteView`/XPC plumbing leaks per invocation.

⚠️ **Two pieces of folklore to refuse, both of which read as authoritative.** (1) "`openPanel`
reuses an existing panel / is a singleton" — that comes from an Apple document Apple itself
banners **Retired**, and the shipping headers contradict it; it still circulates through
search engines and model summaries. (2) "`isReleasedWhenClosed` is ignored for save/open
panels" — there is **no** Apple documentation saying that. The only "ignored" clause Apple
writes concerns windows owned by window controllers. Either line would get a bug report
publicly corrected.

### ⛔ There is NO app-side mitigation. Do not let anyone "fix" this here.

This is the decisive constraint and it is a prohibition, not a preference.
`_gtk_native_dialog_emit_response` (`gtk/gtknativedialog.c`) clears `priv->visible`
**before** it emits `response`, and `gtk_native_dialog_hide()` opens with `if
(!priv->visible) return;`. So calling `.hide()` from inside a `response` handler returns
early and never reaches `klass->hide` — the file's one `[data->panel close]` is
**unreachable from application code on the dismissal path**.

A future agent will read an attributed leak and reach for a local fix. There isn't one. Two
rejected app-side routes are recorded so they are not re-proposed:

- *A macOS seam that reaps non-visible `NSSavePanel`s after a response.* Rejected on
  POLICY's platform-seam bar — a seam supplies a source or a transport the toolkit does not,
  and this is our code cleaning up after a toolkit bug. Its supposed benefit came from the
  retracted figures above, so it is not known to recover anything.
- *Dropping the `GtkFileFilter` from both call sites.* Rejected — it trades a user-facing
  capability, and its supposed benefit came from the same retracted set.

**The standing constraint on this codebase**, which outlives the leak: never call `hide()`
or `destroy()` on a **visible** native dialog. Both call sites
(`window/export.rs`'s `choose_destination`, `app/appactions.rs`'s open action) go through
`saferizer::native_dialog::NativeDialogHolder` and destroy from the `response` handler,
which is the safe shape. The unsafe one is reachable from ordinary code elsewhere in the
ecosystem: `gtkfiledialog.c`'s `cancelled_cb` → `response_cb` → `gtk_native_dialog_destroy`
→ dispose, which calls `hide()` while still visible — so tripping a `GCancellable` while a
panel is up walks into the second leak above.

**"It's deprecated" is not an answer.** The deprecation is a **header-location fact only** —
the public header moved to `gtk/deprecated/gtkfilechoosernative.h`, while the implementation
and every backend, `gtkfilechoosernativequartz.c` included, remain live internals under
`gtk/`. `GtkFileDialog`, the recommended 4.10+ replacement, constructs a
`GtkFileChooserNative` in ten places and brackets its single construction site with
`G_GNUC_BEGIN_IGNORE_DEPRECATIONS`. So **every `GtkFileDialog.open()` / `save()` /
`select_folder()` on macOS runs the leaking path.**

**Vintage: present since day one, nine years.** `ff2c5e38` (Tom Schoonjans, 2017-06-30,
"GtkFilechooserNative: add macOS support") introduced the retains *and*
`setReleasedWhenClosed:YES`, with `[data->panel close]` already confined to the hide path.
The shape has never changed; the accessory-view leak joined in 2023-08-11.

**Never reported upstream — a verified negative with working controls.** Searches of GNOME
GitLab (issues and MRs, `GNOME/gtk`) for `filechoosernativequartz`, `NSOpenPanel`,
`NSSavePanel` and `savepanel` return nothing while control queries return plenty, and all
34 commits touching the file are unrelated to object ownership. Two honest limits on that
negative: GitLab search covers title and description only, not comment bodies, and Stack
Overflow could not be reached at all — the tool returned zero even for its own bare control
query, which is a silently blocked search rather than a true negative.

**And the retain itself is legitimate — nobody should "fix" this by deleting it.** Apple's
own guidance is that an autoreleased panel must be retained before display; GTK is manual
retain/release Objective-C, not ARC. Keep the retain, add the release. Apple also documents
that on the completion-handler path the panel *may still be onscreen* when the handler runs
and that `orderOut:` is the sanctioned way to dismiss it — and `orderOut:` does not consult
`releasedWhenClosed`. GTK is following Apple's advice and then never releasing.

### 📋 Upstream filing: REVIEWED AND READY — blocked on the owner's decision, not on quality

The GNOME GitLab issue body is **written, reviewed and corrected**. The gate this entry set —
no measurement-derived claim leaves here without an unlocked re-run — **has been passed**; what
remains is an authority question. **Nobody here files it**: publishing on a third-party
tracker under the operator's identity is an external, public and effectively irreversible act
in his name, and no seat holds GNOME GitLab credentials in any case (checked, not assumed —
no `glab`, no config, no `gitlab.gnome.org` entry in `.netrc` or `.git-credentials`).

Body lives in the researcher's findings doc, §11 of
`researcher-findings-quartz-filechooser-native-retain-leaks.md`. Two defects were found in
review and fixed: a **stale commit pin** — the header claimed `main` re-verified unchanged
while `main` had moved ten commits — now re-pinned as a **pair** (read at one SHA, confirmed
current at another, with the ten commits' touched files listed so the "none of these files"
claim is checkable without re-running the compare); and a function line-range that pointed at
a comment rather than the declaration.

**When it is filed, put the URL here** and this paragraph becomes a link.

**⚠️ IF IT IS FILED UPSTREAM, LEAD WITH THE `hide()` LEAK — NOT THE PANEL RETAIN.**
The panel retain is now the **weakest** of the three findings: it is arithmetically real but
recovers **zero bytes**, because `_retainedSelf` pins the object regardless. A report that
leads with it invites a maintainer to apply the fix, measure nothing, and discredit everything
behind it. The `hide()`-path leak is the opposite in every respect — it is **GTK-owned heap**
(the `g_object_ref(self)`, the `GFile`s, the filter arrays, the C strings, the struct, plus
`GtkFileChooserNative`/`GtkFileChooserDialog`/`GtkColumnViewCell` trees that never finalize),
none of it AppKit's, none self-retained, **all of it genuinely recoverable**, and measured at
~4.6 MB/cycle. Lead with what a fix actually reclaims.

**There is no upstream patch shape to record, and that is now a stronger statement than it
was.** Four successive proposals were withdrawn on further reading or measurement — the last
because a dealloc spy showed that balancing GTK's retain frees nothing while AppKit still
holds its own reference. Do not copy a patch out of this entry; there isn't one, and the
missing `release` — real though it is in source — is not known to be this issue's cause. One
sub-part is unambiguous on its own terms and worth naming: the filter control and its delegate are each an
`alloc` with no matching `release` anywhere in the file, which is a leak regardless of who
else retains the panel. It will still not produce an observable dealloc while the panel
outlives the button.

⚠️ **Do not restate this issue's cost against the 50 MiB ceiling.** That ceiling (TDD §6) is
a **VRAM** contract, and this is resident memory. The two are different budgets and
conflating them would put a false claim into an upstream bug report.

**One honest gap**, stated because it is load-bearing for anyone re-deriving this: nobody
has documented whether `-[NSWindow close]` fires a `-beginWithCompletionHandler:` block.
The docs are silent both ways; Qt and Chromium both route around `close`; and this
project's own measurement is 0/12 completions via `close` against 20/20 via `-cancel:`. The
recommendation above is built so that it does not depend on that being settled.

**Family note, so the two are not conflated.** The *portal* backend has the mirror-image
hazard — it frees its data *before* emitting the response, risking a use-after-free, where
Quartz emits before freeing. Different bug, same family: who owns the backend data across
the response emission.

So the entry stays open at Medium: the accumulation is real, reproduced by object count,
present in every shipped macOS build, and below this project's layer — but its mechanism is
**not** established, and the next person to pick it up should start from the negative
results above rather than from a cause.

**Consequence for the export rubrics**: TDD 25.15 ("an export does not move the
footprint") **passes on Windows** — 15 verified exports cost +4.3 MB against 40 chooser-only
cycles costing +5.9 MB, so the export contributes nothing distinguishable from opening the
chooser, and the per-process GPU counters read a flat 0 B throughout. It remains
**unverified on macOS**, not failing — export's own contribution to RSS
cannot be separated from the chooser's while every measurement cycle opens one. The GPU
half of that rubric *is* verified: the process never appears as a GPU client, before or
after 30 export cycles, at either document size.
