# Plan: Move Windows to gvsbuild 2026.10.0 (GTK 4.24.1, fixed librsvg)

**Status**: in progress (deferral lifted 2026-10-30; the operator chose to ship with the Win32
input-method workaround below). Phase 1 done; phase 2 owed.

## Problem

The Windows installer carries librsvg **2.62.3**, which has the XInclude use-after-free
CVE-2026-96889 (`sdd/CVE.md` ScrSEC-1). Until it carries a fixed librsvg, every SVG a Windows user
opens is protected only by our interim XInclude screen, which review has already breached six
times (ScrSEC-1, "Leaks found and closed"), and which refuses honest SVGs that use XInclude or
declare entities.

The fix is released. gvsbuild **2026.10.0** (published 2026-10-26) ships librsvg **2.63.2**. But
that release is not a librsvg patch on our current runtime. It moves the whole Windows GTK
runtime from **GTK 4.22.4 to 4.24.1**, with Pango, gdk-pixbuf, HarfBuzz, OpenSSL and other
dependencies updated alongside. Windows has its own workarounds written against 4.22.4, so the
move needs the same ratification as any runtime change.

### Root cause

We do not build GTK for Windows. CI installs gvsbuild's published, pinned archive, and that archive
is the only route by which a library version reaches the installer. A fixed dependency therefore
arrives only as a whole gvsbuild release.

## Previously attempted

Nothing. The screen was written as the interim workaround while no fixed gvsbuild release existed.

## Possible approaches

### ✅ 1. Move the pin, ratify GTK 4.24.1 on Windows, then let the screen follow the loaded decoder

Phase 1 moves the CI pin to gvsbuild 2026.10.0 and ratifies the new runtime on the `windows` seat
against every Windows-specific workaround and rubric. Phase 2 replaces the screen's platform rule
("enforced except on macOS from a verified bundle") with one rule for every platform: the screen
stands down when the librsvg the process has actually loaded is a fixed release (≥ 2.63.2), and
applies whenever that cannot be established. That is the macOS rule (decide from what dyld
loaded, R9-AP2-05) generalised.

**Pros**: closes ScrSEC-1 on Windows with the upstream fix rather than our screen; honest XInclude
SVGs render again where the decoder is fixed; Linux gets the same benefit automatically the day its
distribution ships a fixed librsvg, with no code change; a Windows binary run against an older GTK
prefix (a developer tree, an old gvsbuild) is still screened.
**Cons**: a GTK minor-version jump on one platform, with its own ratification cost; phase 2 adds a
per-platform way to read the loaded librsvg's version, which must fail safe.

### ❌ 2. Move the pin and compile the screen out on Windows

**Rejected**: a compile-time platform test is exactly what R8-SEC-V2 rejected on macOS. The same
binary run against any other GTK prefix (a developer's local build, an older archive) would be
unscreened, and nothing would say so.

### ❌ 3. Move the pin and keep the screen on Windows indefinitely

**Rejected**: Windows would carry the fix and still refuse honest XInclude and entity SVGs for no
reason, and the screen would stay security-critical code that has to be hardened every review round.
It remains the fallback if phase 2's version reading cannot be made reliable on Windows.

### ❌ 4. Keep GTK 4.22.4 and build our own runtime with librsvg 2.63.2

**Rejected**: we would own a from-source GTK build in CI (multi-hour, multi-gigabyte, cached), drift
from the published archive every other gvsbuild user tests, and still face the 4.24 move later.

## Recommendation

Approach 1, in two phases, each its own commit and ratification.

**Phase 1: the upgrade** (`windows` seat drives; `linux` reviews and merges)
1. **Read the GDK-Win32 delta first** (`researcher`). Diff 4.22.4 → 4.24.1 for every behaviour our
   Windows code was written against: the theme light/dark reading, the native frame, the maximize
   latch and integer DPI scale in window placement, popovers as their own HWNDs, and the dispose
   loop. The list of sites is under *Technical details*. Each workaround is then kept, simplified
   or deleted on measurement, never on the changelog alone.
2. **Move the pin.** Set `GVSBUILD_VERSION` and `GVSBUILD_SHA256` together, in the one place CI
   names them. The digest comes from the release API, never from the download.
3. **Rebuild the local prefix** on the `windows` seat to the same gvsbuild version, then run the full
   `packaging/windows/pipeline.ps1` including packaging.
4. **Ratify on Windows**, narrow checks per POLICY's table: the Windows-specific rubrics (native
   frame, desktop light/dark, window-place restore including the 125%/150% run, popovers and menus,
   keys and focus), plus the 4.24 changes the gtk4-rs skill records (reduced-motion scroll jumps,
   capture-phase click gestures, popover anchor-resize). Add whatever the step 1 diff turns up.
   **Done**: every Windows check driven passed unchanged, including window-place at 100/125/150%
   (sizes exact, no creep). Not run, each for a stated reason: fullscreen restore (Windows has no
   fullscreen control), the taskbar and Alt+Tab icon (outside a window capture), and the two
   gestures above (covered by the GTK suite only). The step 1 diff turned up the input-method
   defect below and nothing else that changed a workaround.
5. **Footprint and attribution.** Measure VRAM on the release build (POLICY: a rendering-dependency
   change; 50 MiB ceiling). Run the licence verification for the changed dependencies, and record
   the installer size against the previous one.
   **Done**: VRAM unmeasurable on the `windows` seat — the GPU memory counter reads 0 for every
   process there, the compositor included — so the ceiling stands on the Cairo renderer the log
   confirms and on the other platforms' readings. Licence gate clean (921 staged files, 36 rows,
   the 36th attributing the app's own theme sprites, which had none); the librsvg Rust notice regenerated for 2.63.2; the attribution documents
   updated to 2026.10.0's measured versions and patch set (GTK 4 is no longer patched). Installer
   45,601,663 bytes, the first figure on record.
6. **Prove the fix is delivered.** Measure the librsvg version in the installed tree, and add a
   packaging check that refuses an installer whose librsvg is below the fixed release.
   **Done**: `packaging/windows/verify-librsvg.ps1` reads the staged DLL's own version and
   `package.ps1` refuses to build below the floor; the floor is written once, in
   `packaging/librsvg-floor`, which the macOS gate now reads too. Measured 2.63.2 in the staged
   and the installed tree; a 2.62.3 DLL is refused.

**Phase 2: one screen rule for every platform**
1. Read the loaded librsvg's version at run time on each platform, through the library the loader
   actually mapped (macOS: the dyld image list, as now; Windows: the loaded module; Linux: the
   mapped shared object). An unknown version counts as unfixed.
2. Stand the screen down exactly where that version is ≥ 2.63.2, and log the decision once.
3. Update TDD 2.23c and its manual check, CVE.md ScrSEC-1 (Windows: Closed for the installer;
   the screen stays for Linux), ISSUES Q, TECH and the Windows packaging README.

## Proposed TDD Rubrics

*Edit them here. Completing the plan ratifies them; they enter TDD.md as each phase is built.
Rubric 2 replaces 2.23c's platform clause.*

### A Windows install carries an SVG decoder without the published XInclude flaw
- **Given** a Windows installer produced by the build pipeline
- **When** it is installed and opens an SVG
- **Then** the SVG decoder doing the work is a release in which the XInclude flaw is fixed, and the
  pipeline refuses to produce an installer whose decoder is not

### XInclude screening follows the decoder actually in use
- **Given** the app running on any platform
- **When** it decodes an SVG
- **Then** the XInclude screen applies whenever the SVG decoder it has loaded is not known to be
  fixed — an older release, or one whose version cannot be established — and stands down when the
  loaded decoder is a fixed release

### An SVG that uses XInclude renders where the decoder is fixed
- **Given** an SVG that uses XInclude, and a decoder known to be fixed
- **When** a document referencing it is shown
- **Then** it renders as the decoder renders it, with no broken-image placeholder and no "blocked"
  notice; and the same SVG under a decoder not known to be fixed is refused as TDD 2.23c describes

### The Windows behaviours ratified on GTK 4.22 still hold on the new runtime
- **Given** the Windows build on the moved runtime
- **When** the Windows-specific checks of the manual test plan are run
- **Then** each passes as it did on the previous runtime, or its change is recorded and accepted,
  never silently different

## Technical details preserved

- **The pin**: `.github/workflows/pipeline.yml`, the Windows job's `GVSBUILD_VERSION` /
  `GVSBUILD_SHA256` pair. These are the only two lines naming the version; bump them together.
- **Current**: 2026.8.0, `sha256:1f95a92d037f5292da05e6ab1037032ff21ddb7b20d4ac8e83e3674c864c07b0`,
  librsvg 2.62.3, GTK 4.22.4.
- **Target**: 2026.10.0, asset `GTK4_Gvsbuild_2026.10.0_x64.zip`,
  `sha256:8005b860447e73b10c7f508b1f348e2b6390e01dd61fd5c617b8190a046022fe` (read from the release
  API on 2026-10-28; re-read before pinning, since a release tag is mutable), librsvg 2.63.2
  (gvsbuild "Update librsvg to version 2.63.2", 2026-10-05), GTK 4.24.1. gvsbuild `main` already pins
  librsvg 2.63.4 for the next release.
- **Digest procedure**: `gh api repos/wingtk/gvsbuild/releases/tags/<version>`, then the asset's
  `digest`. Hashing the download proves only that the download did not corrupt.
- **Windows code written against 4.22.4** (each states 4.22.4 in its own header; re-measure each):
  - `src/platform/win32/appearance.rs`: GTK does not read the Windows light/dark setting.
  - `src/platform/win32/frame.rs`: native-frame behaviour traced in the 4.22.4 sources.
  - `winplace/src/win32.rs`: the `is_set_maximized` latch (GTK4Rs/AP-158) and the integer
    `dpi/96` surface scale (R9-AP2-06).
  - `src/window/tabs/dnd.rs`: the popover-child dispose loop (GTK4Rs/AP-80).
  - Popover and dialog HWND capture rules in `tests/MANUAL-TEST.md` §A.3 (GTK4Rs/AP-328).
- **Build configurations**: the CI archive is gvsbuild's `debug-optimized` build (GTK assertions
  compiled in), while the README's local recipe builds `--configuration release` (assertions
  compiled out, GTK4Rs/AP-272). A critical that appears in only one of them is this, not a
  platform difference.
- **GTK 4.24.1 brought a Win32 input-method use-after-free** (GNOME/gtk MR !9958; fixed by MR
  !10411, in no gvsbuild release yet): a focused text widget unrealized before it loses focus
  leaves a display-wide message filter on its freed context. 16 of the GTK suite's cases failed
  on it, none on 4.22.4 with the same binary. `src/focusrelease.rs` clears the window's focus
  first, from one `unrealize` emission hook; retire it when the pin carries the fix. Whether to
  ship the move with that workaround or hold for a fixed release is the operator's call.
- **macOS is already on GTK 4.24.1** (Homebrew) and its pipeline is green there, so 4.24's
  cross-platform behaviour is partly ratified. What is new here is GDK-Win32.
