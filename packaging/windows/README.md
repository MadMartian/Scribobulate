# Windows packaging

Builds a Scribobulate install for Windows: the release binary plus the GTK4
runtime it needs, wrapped in a per-user installer.

**Not self-contained, and the exception is deliberate.** Everything GTK needs
ships inside the install; Microsoft's C runtime does not, and the installer runs
Microsoft's own redistributable instead. The reasoning is in
[The MSVC runtime](#the-msvc-runtime-and-why-this-machine-cannot-verify-it) below —
read it before "fixing" the omission, because copying those DLLs app-local is the
arrangement that was deliberately reversed.

## Prerequisites

| Tool | Install |
|---|---|
| Rust (MSVC toolchain) | `winget install --id Rustlang.Rustup` |
| MSVC + Windows SDK | Visual Studio 2022 with the C++ workload |
| **The C++ redistributable component** | Part of the same C++ workload — `package.ps1` embeds `vc_redist.x64.exe` in the installer and **throws** if it is absent. See the note below |
| MSYS2 | `winget install --id MSYS2.MSYS2` — **genuinely required, not a precaution**: gvsbuild's first run installs `m4`, `bison`, `flex`, `make`, `patch` and `diffutils` into it via `pacman` and drives the autotools-based upstream steps through them |
| gvsbuild | `pip install --user gvsbuild` |
| Inno Setup 6 | `winget install --id JRSoftware.InnoSetup` |

**The redistributable is a PACKAGING prerequisite, not a build one**, and it fails
at the worst moment: `cargo build` and `stage.ps1` never touch it, so a machine
without it builds and stages perfectly and then throws at the last step of a long
run. `package.ps1`'s `Find-VCRedist` reads the version out of
`VC\Auxiliary\Build\Microsoft.VCRedistVersion.default.txt` under the Visual Studio
root and expects `VC\Redist\MSVC\<version>\vc_redist.x64.exe` beside it. It is
discovered rather than hardcoded because the version differs per Visual Studio
install, so a constant would be wrong on somebody's box. If the throw names a path
that does not exist, the C++ workload was installed without its redistributable
component; add it in the Visual Studio Installer.

## Pipeline

```powershell
# 1. GTK runtime (~14 min on 12 cores; once per machine)
gvsbuild build --configuration release --vs-ver vs2022 gtk4 gtksourceview5
gvsbuild build --configuration release --vs-ver vs2022 --fast-build adwaita-icon-theme

# 2. The app itself — ALL FOUR variables, not just the first two
$env:PKG_CONFIG_PATH = "C:\gtk-build\gtk\x64\release\lib\pkgconfig"
$env:PATH = "C:\gtk-build\gtk\x64\release\bin;$env:PATH"
$env:LIB = "C:\gtk-build\gtk\x64\release\lib;$env:LIB"
$env:INCLUDE = "C:\gtk-build\gtk\x64\release\include;$env:INCLUDE"
cargo build --release

# 3. OPTIONAL — stage the tree on its own, to inspect what will ship.
#    Step 4 does this for you; running both stages it twice.
.\packaging\windows\stage.ps1

# 4. Stage and compile the installer. Use this rather than calling ISCC by hand:
#    it invokes stage.ps1 itself, discovers vc_redist.x64.exe and passes
#    /DRedistFile, and locates ISCC.exe in either of its two normal homes.
.\packaging\windows\package.ps1
```

**Step 3 is not a prerequisite of step 4** — `package.ps1` calls `stage.ps1`
directly, forwarding `-GtkPrefix` so both halves build against one GTK. Run step 3
alone when you want to look at the staged tree without spending the Inno Setup
compile; running it before step 4 is harmless (the output directory is cleared at
the start of every stage) but does the work twice.

**Two of those four are easy to omit and neither failure names them.** Setting only
`PKG_CONFIG_PATH` and `PATH` gets you past `pkg-config` and then fails at link time with
`LINK : fatal error LNK1181: cannot open input file 'gtk-4.lib'`, which reads as a broken
GTK install rather than a missing environment. `LIB` and `INCLUDE` are what fix it, and
`build.bat` has always set all four (it also guards the empty-entry case, since an empty
element in `LIB`/`INCLUDE` means "the current directory" to the toolchain). This block
listed only two until it was measured against a bare shell.

**A failed link still leaves a VALID `THIRD-PARTY-LICENSES.md` behind**, because `build.rs`
runs and generates it before anything is linked. So the file's presence does NOT imply the
build succeeded — what protects staging is `stage.ps1`'s precondition on
`target\release\scribobulate.exe`, not the notices file existing.

## The MSVC runtime, and why this machine cannot verify it

The staged tree contains **no C runtime**. `vcruntime140.dll` and `vcruntime140_1.dll` used
to be copied app-local out of the Visual Studio redistributable directory; that made us a
redistributor of Microsoft's Distributable Code, whose terms require the *distributor* to
make end users **agree** to protective terms — a click-through, not a file on disk. The
installer now runs Microsoft's own `vc_redist.x64.exe` when the machine lacks the runtime,
so Microsoft's terms travel with Microsoft's code.

**A DEVELOPMENT OR CI MACHINE CANNOT TELL YOU WHETHER THIS WORKS, AND IT WILL LOOK LIKE IT
CAN.** Every machine capable of building this software already has the MSVC runtime
installed. Remove the DLLs, rebuild, install, launch — **it starts**, because it loads
`C:\Windows\System32\vcruntime140.dll` and would have done so whether or not the
bootstrapper functions. The condition under test is satisfied by the environment rather
than by the change, so the green result carries no information.

The observation that means something is the **pair**:

- runtime absent, bootstrapper disabled → the app **fails to start**
- runtime absent, bootstrapper runs → the app starts

Without the failing half you have measured the test machine, not the installer. **That
requires a clean Windows image — a VM, or Windows Sandbox where it is available.** Neither
is present on the seat that wrote this, so the end-to-end claim is recorded here as
**unverified** rather than inferred from a box that was always going to pass.

**What HAS been verified on a machine that has the runtime**, and what each check is worth:

| Verified | How | What it does not cover |
|---|---|---|
| Nothing else needs a CRT DLL | `dumpbin /dependents` over all **36** staged binaries (33 DLLs in `bin\`, `scribobulate.exe`, `gdbus.exe`, and the one gdk-pixbuf loader): **all 36** import `vcruntime140.dll`, `vcruntime140_1.dll` is imported by `cairo-2.dll` alone, and no `msvcp140`/`concrt140`/`mfc`/`vcomp`/`vccorlib` appears anywhere | — |
| The detector's *negative* branch | Registry read returns `Installed=1`, version 14.44.35211; the prerequisite is skipped and Setup raises no prompt | that it correctly says **yes** on a machine without the runtime |
| The redistributable is extracted before it is run | detector forced True in a throwaway build; the probe found the file present in `{tmp}` | that `vc_redist.x64.exe` then installs successfully |
| **Refusal leaves the machine untouched** | detector forced True with the exec target absent → Setup exit code **7**, install directory never created, zero files written | that the same happens specifically on a dismissed UAC prompt |

The third row is there because the first attempt got it wrong: the redistributable was
declared as a `{tmp}` entry in `[Files]`, and `[Files]` is processed **after**
`PrepareToInstall` runs — so the prerequisite would have targeted a file that did not exist
yet, and reported it as a refused elevation prompt. `dontcopy` plus an explicit
`ExtractTemporaryFile` is the fix. **That bug was invisible on this machine until the
detector was forced**, which is the same lesson as the table above.

Output: `build\installer\Scribobulate-<version>-x64-setup.exe`. The version is read
out of `Cargo.toml` by the `.iss` itself, so nothing here restates it.

**The prefix above is the default, not a fixture.** `build.bat`, `pipeline.ps1` and
`stage.ps1` all resolve it as `%SCRIB_GTK_PREFIX%` when set, else that path — set the
variable once and every entry point follows, which is what stops a tree being staged
from a different GTK than the binary was built against.

**`ISCC.exe` lives in one of two places and both are normal.**
`winget install --id JRSoftware.InnoSetup` defaults to **user** scope
(`%LOCALAPPDATA%\Programs\Inno Setup 6\`, as above); a machine-scope install lands in
`%ProgramFiles(x86)%\Inno Setup 6\`. `package.ps1`'s `Find-Iscc` probes `iscc` on
`PATH` and then both, so it does not need to be told. (`pipeline.ps1 -Package`
reaches it by calling `package.ps1`; the probe is not in the pipeline runner.)

**`/DStageDir` is not optional.** The `.iss` opens with `#ifndef StageDir / #error`,
so omitting it does not produce a wrongly-rooted installer — it produces none.

### Everyday builds: `build.bat`

`pipeline.ps1` is the *gate*. For ordinary work — build it, run it, run the suite —
`build.bat` sets the same GTK environment and gets out of the way. It locates the
repo relative to itself, so it works from any directory:

```bat
build                      :: cargo build
build release [args...]    :: cargo build --release [args...]
build run notes.md         :: cargo run -- notes.md
build test [args...]       :: the GTK integration suite [+ libtest args]
build clippy               :: anything else is passed to cargo verbatim
```

`release`, `run` and `test` forward everything after the keyword, so
`build test --skip <case>` reaches the runner the way `pipeline.ps1`'s step 5
passes it, and `build release --locked` works. A path given to `run` is resolved
against **your** current directory, not the repo root — the arguments are
collected before the script relocates itself, so `build run notes.md` opens the
`notes.md` beside you.

The GTK prefix is `%SCRIB_GTK_PREFIX%` when set, otherwise gvsbuild's default —
the same one `pipeline.ps1` assumes. A missing or half-built prefix is reported
here by name, rather than surfacing later as a `gtk4-sys` build script that
"cannot probe" and names neither gvsbuild nor the variable.

**It deliberately does not `setlocal`.** Run from `cmd.exe`, the environment stays
behind in that console, so a plain `cargo run` works in that window afterwards —
which is the actual complaint the script answers. Note the scope carefully:
**that only holds for `cmd.exe`.** Invoked from PowerShell it runs in a child
process, so `.\packaging\windows\build.bat run notes.md` works perfectly but leaves
the PowerShell session's own environment untouched, and a bare `cargo run` there
still fails. From PowerShell, either keep using `build.bat` for every invocation or
set the four variables in the session by hand (they are listed under **Pipeline**
above).

**A successful build can look like a PowerShell error, and isn't one.** Cargo
writes its status lines — `Compiling`, `Finished` — to *stderr*, and Windows
PowerShell 5.1 wraps a native command's stderr into `ErrorRecord`s when that
stream is redirected into the pipeline — `2>&1` or `2>$null`, including inside a
pipe. That much is measured. Whether a given host does the same on your behalf is
NOT: the ISE and VS Code's integrated terminal are commonly cited and have not
been tested here, while a plain console and a piped child process were tested and
do **not** do it. The result is a red `NativeCommandError` / `RemoteException`
block whose message text is cargo's own success line:

```
.\packaging\windows\build.bat :     Finished `dev` profile [...] target(s) in 0.11s
    + FullyQualifiedErrorId : NativeCommandError
```

Nothing failed; check `$LASTEXITCODE` (`0`) rather than the colour. This is not
`build.bat` — a bare `cargo build` in the same host does it identically, and a
plain conhost PowerShell window shows neither. `build.bat` deliberately does not
merge cargo's stderr into stdout to hide it: that would cost every caller the
stream separation, and `build run` would fold the app's own diagnostics into its
output.

⚠ **Cosmetic here, fatal in `pipeline.ps1`.** The paragraph above holds for
`build.bat`, which runs cargo inside a child `cmd.exe` so PowerShell never sees the
individual lines. `pipeline.ps1` calls cargo directly *and* sets
`$ErrorActionPreference = 'Stop'`, which promotes each wrapped line to a
**terminating** error — so in a capturing host the gate dies during step 2 having
passed step 1, quoting cargo's own `Compiling` line as the failure. Run it from a
plain console window, and note that piping or redirecting it (`... 2>&1 | ...`) makes
any host a capturing one.

It never `pause`s, which is a decision rather than an omission. The usual "pause
only when double-clicked" idiom tests whether `%cmdcmdline%` contains the script's
own name — but that is true of **any** `cmd /c "script"`, which is exactly how
PowerShell and most launchers invoke a `.bat`, so the script sits waiting for a
keypress nobody knew to press and reads as a hang. The same idiom also shells out
to `find`, and MSYS2 is a gvsbuild prerequisite: with its bin directory ahead on
`PATH`, `find` is Unix `find` and the test means something else entirely. A tool
run from a shell should just exit.

A convenience wrapper elsewhere on the machine — `%USERPROFILE%\build-scribobulate.bat`
— should therefore be a shim that `call`s this file rather than a second copy of the
environment (the checkout path is the only host-specific fact, and the only thing
worth keeping outside version control), and it is the right place for a pause,
because a wrapper knows how it is meant to be launched:

```bat
@echo off
call "C:\path\to\Scribobulate\packaging\windows\build.bat" %*
set "RC=%ERRORLEVEL%"
if not "%RC%"=="0" pause
exit /b %RC%
```

### One command for all of it

`pipeline.ps1` sets the GTK environment for you and runs the build pipeline, stopping
at the first failure:

```powershell
.\packaging\windows\pipeline.ps1                   # run the pipeline
.\packaging\windows\pipeline.ps1 -SkipIntegration  # skip the GTK suite (unattended-safe)
.\packaging\windows\pipeline.ps1 -Package          # also build the installer
.\packaging\windows\pipeline.ps1 -ListSteps        # print the derived step list
.\packaging\windows\pipeline.ps1 -SelfTest         # validate the contract and the derivation
```

**Which steps run, in what order, and how each is judged is not documented here.**
That is `scripts/pipeline.steps`, and this runner *derives* its step list from that file
rather than restating it. A list in this README would be a second copy of the contract,
and the only thing a second copy reliably does is stop matching — which is what the
previous version of this section did, advertising "POLICY steps 1-5" after the step list
had grown past it. Run `-ListSteps` for the current answer; it cannot be stale.

`-ListSteps` exists to be **diffed against the other platforms' runners**. All three
derive from the one contract, so a clean diff is evidence they conform to it rather than
merely resembling each other. Output is LF-terminated deliberately, so the comparison
needs no normalisation to be read.

`-Package` runs `packaging\windows\package.ps1`, which owns the staging and the Inno
Setup invocation — including forwarding `-Prefix` to `stage.ps1`, choosing the staged
tree's path and passing it as `/DStageDir`, and failing outright if Inno Setup is
missing. That last is deliberate: packaging was asked for explicitly, so a green run
that quietly produced no installer is worse than a red one.

Test carve-outs are **contract data**, in `scripts/pipeline.steps` as
`carveout.windows`, not a variable in this script — so all three platforms exclude the
same tests or knowingly differ. Each is applied by name via `--skip` and printed in the
run, and the mechanism works while the list is empty, which is what lets the step say
`no carve-outs` and have it mean something. The list is empty today.

## Windows and CI

**There is a workflow now — `.github/workflows/pipeline.yml` — and it does not replace
this script.** Its Windows job runs `pipeline.ps1 -SelfTest` and `-ListSteps` so the
port's derived step list can finally be diffed against the other two, which no single
machine could do. Executing the *whole* pipeline on a hosted Windows runner is still
unsolved: it needs a gvsbuild GTK, which is the provisioning job the two notes at the
end of this section exist to shorten. So this file stays the way a developer builds,
gates and packages the app on their own machine, and CI is a caller of it rather than a
second port.

That decision was reached the long way, and the reasoning below is why the workflow that
exists looks the way it does.

A `.github/workflows/windows.yml` was added on the port branch and was **removed**.
Worth recording why, because it was not merely a scope preference: its triggers were

```yaml
on:
  push:
    branches: [master, "feat/**"]
  pull_request:
    branches: [master]
```

so it was not branch-local infrastructure — it would have run against the shared
repository the moment anything merged, committing the whole project to a CI system
nobody had agreed to. **A workflow file is not scoped by the branch it sits on**,
which is the whole reason its presence was a cross-project decision rather than a
port detail, and why it was removed rather than left for the merge to sort out. The
decision has since been made deliberately, at the project level, and the workflow that
carries it records this constraint in its own header.

The knowledge it carried is preserved rather than discarded — it is all in this file
and in `pipeline.ps1`: the gvsbuild version pin (an unpinned install silently changes
which GTK versions you are testing against), building **release** rather than
gvsbuild's `debug-optimized` default (POLICY makes release the reference for
behaviour *and* the footprint gate), `--fast-build` for `adwaita-icon-theme` (without
it gvsbuild rebuilds the entire dependency chain including the fragile `gettext`
step), and which POLICY gates are meaningful on Windows.

Two things that a future CI attempt would need to re-derive, recorded here so it does
not have to:

- **Cache only the install prefix**, never `C:\gtk-build` itself. The prefix is
  ~144 MB; the full tree is ~6.5 GB of sources and intermediates and would exhaust a
  repository cache quota for no benefit, since nothing downstream reads the build tree.
- **Enable long paths for git only** (`git config --system core.longpaths true`), never
  the system-wide `LongPathsEnabled` registry key, so the runner stays in the
  constrained `MAX_PATH` regime the `#[cfg(windows)]` test in `src/atomic_io.rs` exists
  to exercise. Enabling it would mask that failure mode.

  **What this used to say, and why it no longer does.** It justified the choice by
  claiming a runner thereby exercises a case "this dev box cannot (it has long paths
  enabled)". That contrast was never checked, and it is false for the Windows seat's
  box: `HKLM\SYSTEM\CurrentControlSet\Control\FileSystem\LongPathsEnabled` is **0**
  there, and `git config core.longpaths` is empty at system, global and local scope.
  Both the runner and that dev box are in the constrained regime, so the practice is
  right on its own terms and simply is not a contrast. (Measured on one Windows host —
  another machine may well have the key set, which is the point: it is a per-machine
  fact, not something to assert about "the dev box".) The same claim was corrected in
  `.github/workflows/pipeline.yml` first and left standing here, which is worse than
  correcting neither — the tree then contradicts itself and a reader cannot tell which
  copy was measured. If either is edited again, edit both.

  Note the setting is **necessary but not sufficient** — `unique_tmp_path_for` uses
  `with_file_name`, so the temp file is a sibling of the *target*, and the overflow
  needs a deep **document** directory, which a shallow runner workspace does not
  provide. What actually drives the case is
  `write_atomic_survives_a_document_directory_that_overflows_max_path`, which builds
  the deep directory itself and asserts both preconditions. Its comments once recorded
  the constrained branch as unmeasured; that gap is **now closed** — the test passes on
  a host with `LongPathsEnabled=0`, which is where the constrained branch runs rather
  than the success one. The test deliberately does not pin the outcome: what must hold
  either way is the atomicity contract.

### The dev prefix and CI's GTK are not the same binary

`gvsbuild build --configuration release` (above) is not merely a label. It sets Meson
`--buildtype=release`, and GTK's own `meson.build` reacts to that:

```
if debug
  gtk_debug_cflags += '-DG_ENABLE_DEBUG'
elif optimization in ['2','3','s']
  gtk_debug_cflags += ['-DG_DISABLE_CAST_CHECKS', '-DG_DISABLE_ASSERT']
endif
```

So a `--configuration release` prefix has **`g_assert` compiled out of GTK and GSK**.
The prebuilt zip CI consumes (`GTK4_Gvsbuild_<ver>_x64.zip`) is built *without*
`--configuration`, i.e. gvsbuild's `debug-optimized` default, and **keeps its
assertions**.

**Nothing on disk distinguishes them.** gvsbuild rewrites the configuration string to
`release` for install pathing, so both land in `C:\gtk-build\gtk\x64\release`. Same
version number, same directory name, same `pkg-config --modversion`. Only the
compiled-in literals differ:

```
:: present in CI's build, ABSENT from a --configuration release build
findstr /C:"!priv->is_realized" <prefix>\bin\gtk-4-1.dll
```

The consequence is not academic and is not fixed by testing more carefully: **every
`g_assert`-backed GTK/GSK contract is unenforced on a `--configuration release` prefix
and enforced on CI.** A local run cannot fail that whole class of defect. This was
measured, not theorised — a realized `GskRenderer` dropped without `unrealize()` exits
0 against a release prefix and aborts against CI's with
`Gsk:ERROR:gskrenderer.c:130:gsk_renderer_dispose: assertion failed: (!priv->is_realized)`,
which is how that defect reached a hosted runner from a green desk.

**Practice: when the question touches GTK behaviour, test against the zip CI uses.**
Unpack `GTK4_Gvsbuild_<pinned version>_x64.zip` anywhere and point the toolchain at it
for that run only:

```powershell
$env:SCRIB_GTK_PREFIX = 'C:\path\to\unpacked-zip'
$env:PATH             = "$env:SCRIB_GTK_PREFIX\bin;$env:PATH"
$env:PKG_CONFIG_PATH  = "$env:SCRIB_GTK_PREFIX\lib\pkgconfig"
```

Per-invocation, never a persisted user or machine variable — the everyday prefix stays
where it is, and `--configuration release` remains correct for the footprint gate and
for release as the behavioural reference (above). This is not a replacement for the dev
prefix; it is the second one you reach for when the answer must match CI.

## How the app icon reaches Windows

`scribobulate.ico` beside this file is rendered from
`data/icons/scalable/apps/com.extollit.scribobulate.svg`, the single source of truth
for the app's art. It has **three** consumers, and each needs its own channel:

| Where the icon shows | Channel | Set by |
|---|---|---|
| Title bar, taskbar, Alt+Tab | GResource, looked up by icon name at runtime | `data/resources.gresource.xml` |
| "Open with", Default apps, Task Manager, shortcuts, uninstall entry | Win32 `RT_GROUP_ICON` inside `scribobulate.exe` | `build.rs` (`winresource`) |
| A `.md` file's own icon once associated | ProgID `DefaultIcon` → the loose `.ico` | `scribobulate.iss` |

**The shell never asks the running process.** It reads the executable on disk without
launching it, so nothing GTK sets at runtime — `icon_name`, the window title — is
visible to Explorer. Rust emits no resource section by default, and a binary missing
one still runs perfectly: the only symptom is a generic icon and the bare file name
`scribobulate.exe` in every shell surface. `build.rs` therefore treats a failed
`res.compile()` as a hard build error rather than a warning.

The friendly name comes from the same section's `VERSIONINFO`. The shell resolves it
as ProgID `FriendlyAppName` → the exe's `FileDescription` → the file name; the
installer sets the first and `build.rs` the second, so the name is right both for an
installed copy and for a binary run straight out of `target\release`.

**The name is cached per path, and the cache outlives everything you would try.**
`MuiCache` (`HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache`)
stores `<full exe path>.FriendlyAppName` and is keyed by **path only** — no rebuild,
reinstall, Explorer restart or reboot invalidates it. The icon cache *does* key on
the file, so the two diverge, and the giveaway is a fix where **the icon comes right
and the name does not**. Do not go looking in the binary: copy the exe to a path the
shell has never seen and query *that* copy —

```powershell
# ASSOCF_INIT_BYEXENAME = 2, ASSOCSTR_FRIENDLYAPPNAME = 4 via shlwapi!AssocQueryStringW
Copy-Item .\target\release\scribobulate.exe "$env:TEMP\probe.exe"
(Get-Item "$env:TEMP\probe.exe").VersionInfo.FileDescription   # what the file says
```

If the untouched path answers `Scribobulate`, the resource section is fine and only
the cache is stale. Clear it by deleting that path's `.FriendlyAppName` value (an
`.ApplicationCompany` sibling is written alongside it) and restarting `explorer.exe`.
A stale entry masks the registry `FriendlyAppName` exactly as it masks
`FileDescription`, so setting the former is not a way around it.

## Two things that will bite you

**Clear `NoDefaultCurrentDirectoryInExePath` before running gvsbuild — and clear it
the right way.** When that variable is defined, `cmd.exe` stops searching the working
directory, and gvsbuild's `gettext` step invokes `create-lists.bat` as a bare name.
It fails with `'create-lists.bat' is not recognized` followed by
`U1052: file 'gettext-runtime-objs.mak' not found`, which reads exactly like a
corrupted source tree even though the file is present. `vswhere.exe` fails the same
way. Hardened and CI environments often set it.

```powershell
Remove-Item Env:NoDefaultCurrentDirectoryInExePath -ErrorAction SilentlyContinue
```

**`[Environment]::SetEnvironmentVariable('NoDefaultCurrentDirectoryInExePath', $null)`
does not work** — it leaves the variable *defined but empty*, and `cmd.exe` tests
whether it is defined, not what it holds. Both forms make `$env:VAR` print empty, so
checking `$env:VAR` gives false confirmation. Verify with a real probe instead:
drop a `.bat` in a scratch directory and see whether `cmd /c` finds it by bare name.

**Use `--fast-build` on follow-up gvsbuild runs.** Without it, building one extra
package rebuilds the whole dependency chain — including the fragile `gettext` step.

## Layout, and why it is that shape

```
Scribobulate\
  LICENSE                      the app's own licence
  THIRD-PARTY-LICENSES.md      generated at build time from notices\
  bin\    scribobulate.exe + gdbus.exe + 33 DLLs
  lib\gdk-pixbuf-2.0\2.10.0\   loaders.cache + loaders\
  share\glib-2.0\schemas\      gschemas.compiled
  share\icons\                 Adwaita, hicolor
  share\gtksourceview-5\       RNG/DTD schemas only
  share\licenses\<Id>\         one directory per licences.psd1 row
  share\scribobulate\          themes.toml, sprites\
```

Measured on a staged tree: 905 files, 45.2 MB, of which 36 are binaries, and
`share\licenses\` holds 39 texts across 35 component directories. `stage.ps1`
prints both counts as it runs (`Staged 39 licence texts for 35 components`), so a
divergence from this block is visible without a diff.

GTK derives its prefix on Windows by taking the path of the loaded GLib DLL and
stripping a trailing `bin`. **DLLs must therefore live in `<root>\bin` for
`<root>\share` to resolve.** Flattening the tree breaks icon and schema lookup at
runtime, not at build time.

Notes on the contents:

- **`rsvg-2-2.dll` ships even though a running instance never shows it loaded at
  startup.** It is pulled in lazily by `pixbufloader_svg.dll` the first time an
  Adwaita *symbolic* icon is drawn. A dependency list captured from a live process
  will miss it, and the result is a build that starts perfectly and then renders
  broken toolbar icons.
- **GtkSourceView's language specs and style schemes are not shipped** — they are
  compiled into `gtksourceview-5-0.dll` as GResource.
- **`loaders.cache` is copied verbatim, never regenerated.** gvsbuild writes it with
  a relative loader path, so it relocates cleanly; regenerating it on the build
  machine would bake in absolute paths.
- **`gdbus.exe` is not optional tooling.** GIO has no Win32-native uniqueness
  backend, so `GApplication` negotiates over a D-Bus session bus here exactly as on
  Linux, and GLib autolaunches that bus by spawning `gdbus.exe` from beside the
  loaded GLib DLL. Omit it and every launch elects itself primary: one process per
  document, silently (ScrAP-249). It is invisible from a dev-tree run, where
  gvsbuild's `bin` is on `PATH` — test single-instance against the STAGED tree.
- **The licence texts are staged from `licenses.psd1`, one directory per row.**
  `LICENSE` and `THIRD-PARTY-LICENSES.md` go to the install root because they cover
  the whole distribution; a notice that covers one DLL in `bin\` goes under
  `share\licenses\<Id>\` instead. `verify-licenses.ps1` is the gate over that
  arrangement — see [Licence staging](#licence-staging-and-its-gate).
- **`THIRD-PARTY-LICENSES.md` is generated, not authored.** `build.rs` builds it
  from the files in `notices\`, and it is not committed. Edit `notices\*.md`; a
  change made to the generated file is overwritten by the next build.
- **`themes.toml` ships as a reference copy, not as a requirement.** The same file is
  compiled into the binary (`include_str!`), so every shipped theme resolves whether
  or not this copy exists — deleting it from an install changes nothing on screen. It
  is here for discoverability, matching what the Linux packages install to
  `/usr/share/scribobulate/`: without it a Windows user has no installed file to read
  before writing their own `%APPDATA%\scribobulate\themes.toml`. It resolves as search
  path row 3 (`$XDG_DATA_DIRS`), so a user override still wins over it. Verified on a
  staged tree by perturbing this copy and watching the perturbation reach the screen —
  the only check that distinguishes "shipped" from "actually read".

## Licence staging, and its gate

`licenses.psd1` says which upstream project every staged file belongs to and where
its licence text comes from. `stage.ps1` copies one text per row into
`share\licenses\<Id>\`, and puts `LICENSE` and `THIRD-PARTY-LICENSES.md` at the
install root — the split is by *scope*: a notice covering the whole distribution
goes to the root, a notice covering one DLL goes beside the other component
notices.

```powershell
.\packaging\windows\verify-licenses.ps1              # check a staged tree against the table
.\packaging\windows\verify-licenses.ps1 -Report      # list every row and what it resolved to
.\packaging\windows\verify-licenses.ps1 -SelfTest    # prove the checker can fail
```

**It is not wired into the pipeline, the contract or the workflow** — nothing runs
it for you, so run it after a staging change. It fails on four disagreements, and
the last is the one that earns its keep:

1. a staged file with **no row** — somebody else's code shipped unattributed;
2. a row with **no staged file** — the table describes an artefact that no longer
   exists, which is how a manifest becomes fiction while every row still reads
   correctly;
3. a row whose licence text is **missing** — gvsbuild ships empty `share\doc`
   directories for freetype, graphene and libxml2, so a table written from a
   directory listing names files that are not there;
4. a row whose licence text is **not the licence**. Each row declares a string
   that must occur in its Source, because file-existence is a predicate standing
   in for a semantic question and it answers "fine" for `pcre2\COPYING` (four
   lines pointing at a file that is not shipped), `cairo\COPYING` (a summary
   pointing at two that are not shipped), and `gettext\COPYING` (the GPL-3.0 for
   the gettext *tools*, where the DLL we ship is libintl under LGPL-2.1).

`THIRD-PARTY-LICENSES.md` is **generated** by `build.rs` from `notices\*.md` and is
not committed. Change a notice by editing its file in `notices\`; edits to the
generated file are overwritten by the next build.

## Installer behaviour

Per-user by design — `PrivilegesRequired=lowest`, so it installs to
`%LOCALAPPDATA%\Programs\Scribobulate` with no elevation prompt and writes nothing
outside `HKCU`.

File associations follow an opt-in model:

- The `Scribobulate.Document` ProgID and an `OpenWithProgids` entry are **always**
  registered, so the app appears under "Open with" and in Settings ▸ Default apps.
- It becomes the **default** handler for `.md`/`.markdown` only when the user ticks
  that box, which is unchecked by default. Silently seizing a file type is the kind
  of thing users have to go and undo.

**The installer is unsigned**, so SmartScreen warns on first run when it has been
downloaded rather than built locally, and the publisher shows as unknown in the UAC
and Programs-and-Features surfaces. Fixing this needs a code-signing certificate, not
a change here. Until there is one, expect the warning and do not read it as a defect
in the package.

**When checking whether an installer change landed, look at a value only that change
writes.** Inno's own uninstall registration writes some of the same keys the
`[Registry]` section does, so `DisplayIcon` or `InstallDate` showing a previous
package's value proves nothing about whether your edit applied — every ambiguous
witness costs a re-run to interpret.

## Uninstalling

**Windows uninstalls through Apps & Features, not through a script in this
repository.** The install is registered with Windows by Inno Setup, so Windows owns
the removal:

- **Settings ▸ Apps ▸ Installed apps** (Windows 11) or **Settings ▸ Apps ▸ Apps &
  features** (Windows 10) — find **Scribobulate**, then **Uninstall**.
- Or use the **Uninstall Scribobulate** shortcut in the Start menu group, which the
  installer creates pointing at Inno Setup's own uninstaller in the install
  directory.

Either route removes the install directory, the ProgID, the `OpenWithProgids`
entries and the `RegisteredApplications` entry — every registry write carries an
`uninsdeletekey` or `uninsdeletevalue` flag, so the registration comes back out with
the install.

**Anything you created after installing is left alone.** The `.iss` has no
`[UninstallDelete]` section and names no user directory, so your themes at
`%APPDATA%\scribobulate\themes.toml`, your configuration and your session state
survive an uninstall and are still there for a reinstall.

**`./uninstall.sh` in the repository root does not work here, by design.** It is a
`uname -s` router for the platforms that install from source; on a Windows shell it
refuses and points at this section rather than half-removing an install it did not
create. There is nothing to run in its place — the two routes above are the whole
answer.
