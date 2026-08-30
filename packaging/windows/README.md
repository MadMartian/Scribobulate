# Windows packaging

Builds a Scribobulate install for Windows: the release binary plus the GTK4 runtime
it needs, wrapped in a per-user installer.

**Not self-contained, and the exception is deliberate.** The GTK stack ships in the
package; Microsoft's Visual C++ runtime does NOT. `scribobulate.exe` and the staged
GTK DLLs import `VCRUNTIME140.dll`, Windows does not ship it, and this installer
neither installs nor checks for it — so on a machine that has never had a Visual C++
redistributable the app installs and then fails to start. Copying the DLLs in is the
obvious fix and it is the wrong one: it makes this project a redistributor of
Microsoft's Distributable Code, whose terms require an end-user click-through no
vendored file can present.

## Prerequisites

| Tool | Install |
|---|---|
| Rust (MSVC toolchain) | `winget install --id Rustlang.Rustup` |
| MSVC + Windows SDK | Visual Studio 2022 with the C++ workload |
| MSYS2 | `winget install --id MSYS2.MSYS2` — **genuinely required, not a precaution**: gvsbuild's first run installs `m4`, `bison`, `flex`, `make`, `patch` and `diffutils` into it via `pacman` and drives the autotools-based upstream steps through them |
| gvsbuild | `pip install --user gvsbuild` |
| Inno Setup 6 | `winget install --id JRSoftware.InnoSetup` |

## Pipeline

```powershell
# 1. GTK runtime (~14 min on 12 cores; once per machine)
gvsbuild build --configuration release --vs-ver vs2022 gtk4 gtksourceview5
gvsbuild build --configuration release --vs-ver vs2022 --fast-build adwaita-icon-theme

# 2. The app itself
$env:PKG_CONFIG_PATH = "C:\gtk-build\gtk\x64\release\lib\pkgconfig"
$env:PATH = "C:\gtk-build\gtk\x64\release\bin;$env:PATH"
cargo build --release

# 3. Stage the redistributable tree
.\packaging\windows\stage.ps1

# 4. Compile the installer
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" `
    /DStageDir="$PWD\build\stage\Scribobulate" `
    packaging\windows\scribobulate.iss
```

Output: `build\installer\Scribobulate-<version>-x64-setup.exe`. The version is read
out of `Cargo.toml` by the `.iss` itself, so nothing here restates it.

**The prefix above is the default, not a fixture.** `build.bat`, `pipeline.ps1` and
`stage.ps1` all resolve it as `%SCRIB_GTK_PREFIX%` when set, else that path — set the
variable once and every entry point follows, which is what stops a tree being staged
from a different GTK than the binary was built against.

**`ISCC.exe` lives in one of two places and both are normal.**
`winget install --id JRSoftware.InnoSetup` defaults to **user** scope
(`%LOCALAPPDATA%\Programs\Inno Setup 6\`, as above); a machine-scope install lands in
`%ProgramFiles(x86)%\Inno Setup 6\`. `package.ps1` probes `iscc` on `PATH` and then
both, so it does not need to be told — and `pipeline.ps1 -Package` inherits that by
calling `package.ps1` rather than carrying a second copy of the probe, which is what
its own comment records as the way the two would silently stop matching.

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

## Why this is a script, not CI

**Scribobulate has no GitHub Actions workflow, and adding one is out of scope for
this port.** The delivery mechanism here is deliberately a local script: something a
developer runs to build, gate and package the app on their own machine.

A `.github/workflows/windows.yml` was added on the port branch and has since been
**removed**. Worth recording why, because it is not merely a scope preference: its
triggers were

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
port detail, and why it was removed rather than left for the merge to sort out.

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
  the system-wide `LongPathsEnabled` registry key. Leaving that at its default is what
  makes a runner exercise the constrained `MAX_PATH` case that this dev box cannot
  (it has long paths enabled). Enabling it would mask the failure mode you want caught.
  Note this is **necessary but not sufficient** — `unique_tmp_path_for` uses
  `with_file_name`, so the temp file is a sibling of the *target*, and the overflow
  needs a deep **document** directory, which a shallow runner workspace does not
  provide. What actually drives the case is the `#[cfg(windows)]` test
  `write_atomic_survives_a_document_directory_that_overflows_max_path` in
  `src/atomic_io.rs`, which builds the deep directory itself and asserts both
  preconditions; its own comments record what remains unmeasured (a host with
  `LongPathsEnabled=0`, where the constrained branch runs instead of the success one).

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
  bin\    scribobulate.exe + 33 DLLs
  lib\gdk-pixbuf-2.0\2.10.0\   loaders.cache + loaders\
  share\glib-2.0\schemas\      gschemas.compiled
  share\icons\                 Adwaita, hicolor
  share\gtksourceview-5\       RNG/DTD schemas only
  share\scribobulate\          themes.toml
```

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
- **`themes.toml` ships as a reference copy, not as a requirement.** The same file is
  compiled into the binary (`include_str!`), so every shipped theme resolves whether
  or not this copy exists — deleting it from an install changes nothing on screen. It
  is here for discoverability, matching what the Linux packages install to
  `/usr/share/scribobulate/`: without it a Windows user has no installed file to read
  before writing their own `%APPDATA%\scribobulate\themes.toml`. It resolves as search
  path row 3 (`$XDG_DATA_DIRS`), so a user override still wins over it. Verified on a
  staged tree by perturbing this copy and watching the perturbation reach the screen —
  the only check that distinguishes "shipped" from "actually read".

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
- Or the **Uninstall Scribobulate** shortcut in the Start menu group, which the
  installer creates pointing at Inno Setup's own uninstaller.

Either route removes the install directory, the ProgID, the `OpenWithProgids`
entries and the `RegisteredApplications` entry.

**How the registry writes come back out**, stated as the rule someone ADDING a write
needs rather than as a summary that happens to hold today. MEASURED on the `.iss`: 13
`Root:` writes, 7 flags, and both halves are load-bearing.

- **Two subtree roots carry `uninsdeletekey` on their *first* write, which takes their
  children with them.** `Software\Classes\Scribobulate.Document` (children:
  `FriendlyAppName`, `DefaultIcon`, `shell\open\command`) and
  `Software\Scribobulate\Capabilities` (children: `ApplicationDescription` and the two
  `FileAssociations` entries). Those six writes carry no flag and need none.
- **Five values outside any flagged subtree carry `uninsdeletevalue` individually** —
  the two `OpenWithProgids` entries, the two default-handler values, and the
  `RegisteredApplications` entry.

So the rule is: a value under an already-flagged subtree root needs nothing, a value
anywhere else needs its own flag. "Every write carries a flag" is a false description
of a true outcome — harmless as a belief, but its mirror image ("writes are cleaned up
automatically") ships a new subkey root with no flag and nothing to notice it.

**Anything you created after installing is left alone.** The `.iss` has no
`[UninstallDelete]` section and names no user directory — the only two `appdata`
mentions in it are comments — so your themes at
`%APPDATA%\scribobulate\themes.toml`, your configuration and your session state
survive an uninstall and are still there for a reinstall.

**`.\uninstall.sh` in the repository root does not work here, by design.** It is a
`uname -s` router for the platforms that install from source; on a Windows shell it
refuses and points at this section rather than half-removing an install it did not
create. There is nothing to run in its place — the two routes above are the whole
answer.
