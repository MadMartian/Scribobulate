# Building and installing Scribobulate on Windows

There is no prebuilt download yet — you build it from this clone. The whole
sequence is below. Budget about twenty minutes the first time, nearly all of it
GTK compiling once; after that a build is a couple of minutes.

## 1. One-time toolchain setup

| Tool | Install |
|---|---|
| Rust (MSVC toolchain) | `winget install --id Rustlang.Rustup` |
| Visual Studio 2022, **C++ workload** | Include the **C++ redistributable** component — step 4 needs it, and its absence is not detected until then |
| Python 3.9+ | `winget install --id Python.Python.3.13` — gvsbuild is a Python package |
| Git | `winget install --id Git.Git` — gvsbuild fetches upstream sources with it |
| MSYS2 | `winget install --id MSYS2.MSYS2` — **genuinely required**: gvsbuild's first run installs `m4`, `bison`, `flex`, `make`, `patch` and `diffutils` into it and drives the autotools-based upstream steps through them |
| gvsbuild | `pip install --user gvsbuild` |
| Inno Setup 6 | `winget install --id JRSoftware.InnoSetup` — only if you want the installer (step 4) |

Then build the GTK runtime. Once per machine, not once per checkout:

```powershell
Remove-Item Env:NoDefaultCurrentDirectoryInExePath -ErrorAction SilentlyContinue
gvsbuild build --configuration release --vs-ver vs2022 gtk4 gtksourceview5
gvsbuild build --configuration release --vs-ver vs2022 --fast-build adwaita-icon-theme
```

Roughly 14 minutes on 12 cores. The result lands in
`C:\gtk-build\gtk\x64\release`. If you put it somewhere else, set
`SCRIB_GTK_PREFIX` to that path once and every script here follows it.

**That first line is not boilerplate.** With
`NoDefaultCurrentDirectoryInExePath` defined, `cmd.exe` stops searching the
working directory and gvsbuild's `gettext` step fails with `'create-lists.bat'
is not recognized` followed by `U1052: file 'gettext-runtime-objs.mak' not
found` — which reads exactly like a corrupted source tree. Hardened and
corporate environments often set it.

**`--fast-build` on the second line matters too.** Without it, adding one
package rebuilds the whole dependency chain, including the fragile `gettext`
step you just got past.

## 2. Build the app

```powershell
.\packaging\windows\build.bat release
```

That is the whole command. `build.bat` finds the repo relative to itself, so it
works from any directory, and it sets the four environment variables the GTK
toolchain needs before calling cargo:

```
build                     cargo build
build release [args...]   cargo build --release [args...]
build run [file.md ...]   cargo run -- [file.md ...]
build test [args...]      the GTK integration suite [+ libtest args]
build <args...>           anything else goes to cargo verbatim (build clippy)
```

A path given to `run` resolves against **your** current directory, not the repo
root, so `build run notes.md` opens the `notes.md` beside you.

<details>
<summary>Setting the environment by hand instead</summary>

A bare `cargo build` fails with `The pkg-config command could not be found`,
because none of the GTK toolchain is on the default `PATH`. All four of these
are required — setting only the first two gets you past `pkg-config` and then
fails at link time with `LINK : fatal error LNK1181: cannot open input file
'gtk-4.lib'`, which reads as a broken GTK install rather than a missing
variable:

```powershell
$p = if ($env:SCRIB_GTK_PREFIX) { $env:SCRIB_GTK_PREFIX } else { 'C:\gtk-build\gtk\x64\release' }
$env:PKG_CONFIG_PATH = "$p\lib\pkgconfig"
$env:PATH            = "$p\bin;$env:PATH"
$env:LIB             = "$p\lib;$env:LIB"
$env:INCLUDE         = "$p\include;$env:INCLUDE"
cargo build --release
```

Run from `cmd.exe`, `build.bat` deliberately leaves this environment behind in
that console, so a plain `cargo` works there afterwards. **Run from PowerShell
it does not** — it executes in a child process, so `build.bat run notes.md`
works but a bare `cargo run` in that session still fails.

</details>

## 3. Run it

```powershell
.\packaging\windows\build.bat run README.md
```

Or the binary directly, once built:
`.\target\release\scribobulate.exe path\to\document.md`. A second launch joins
the instance already running; pass `--new-instance` (`-n`) for a separate one.

## 4. Install it

Optional — steps 2 and 3 already give you a working application. This builds the
per-user installer, which is what you want if you would rather have Scribobulate
on the Start menu than in `target\release`.

```powershell
.\packaging\windows\package.ps1
```

Output: `build\installer\Scribobulate-<version>-x64-setup.exe`. It stages the
runtime tree itself, so there is no separate step to run first, and it finds
both Inno Setup and the Visual C++ redistributable without being told where they
are. Run it and you get:

| What | Where |
|---|---|
| The application | `%LOCALAPPDATA%\Programs\Scribobulate` |
| Start menu | *Scribobulate*, plus *Uninstall Scribobulate* |
| Registry | `HKEY_CURRENT_USER` only |
| Your themes | `%APPDATA%\scribobulate\themes.toml` |
| Crash reports | `%LOCALAPPDATA%\scribobulate\` |

No administrator password, nothing outside your own user account, and the GTK
runtime travels inside it. Two tick-boxes, both **off** by default: a desktop
shortcut, and making Scribobulate the default app for `.md`/`.markdown`. It
appears under *Open with* either way.

Two things to expect:

- **SmartScreen warns on first run.** The installer is not code-signed, so
  Windows does not recognise the publisher. *More info* ▸ *Run anyway*.
- **One administrator prompt, only on a machine without Microsoft's Visual C++
  runtime.** The installer carries Microsoft's own redistributable and runs it,
  because everything we ship imports `VCRUNTIME140.dll` and Windows does not
  include it. Declining the prompt stops the install with **nothing written** —
  better than an application that cannot start.

### Uninstalling

Through Windows, because Windows was told about the install: **Settings ▸ Apps ▸
Installed apps** (or *Apps & features* on Windows 10), or the **Uninstall
Scribobulate** shortcut in the Start menu. Either removes the application and
every registry entry Setup made.

Your themes, configuration and session state are left alone and picked up again
on reinstall. Delete `%APPDATA%\scribobulate` and `%LOCALAPPDATA%\scribobulate`
by hand if you want them gone.

`./uninstall.sh` in the repository root does **not** work here, deliberately —
it serves the platforms that install from source, and refuses rather than
half-removing an install it did not create.

## Running the full gate

`build.bat` is for working on the app; `pipeline.ps1` is the gate:

```powershell
.\packaging\windows\pipeline.ps1                   # the whole pipeline
.\packaging\windows\pipeline.ps1 -SkipIntegration  # skip the GTK suite (unattended-safe)
.\packaging\windows\pipeline.ps1 -Package          # also build the installer
.\packaging\windows\pipeline.ps1 -ListSteps        # print the derived step list
```

Which steps run and in what order is not restated here — it comes from
`scripts/pipeline.steps`, and `-ListSteps` prints the current answer, which
cannot be stale.

## When it goes wrong

| Symptom | Cause |
|---|---|
| `The pkg-config command could not be found` | Not using `build.bat`, and the GTK environment is unset. Step 2. |
| `LNK1181: cannot open input file 'gtk-4.lib'` | `LIB` and `INCLUDE` are missing — `PKG_CONFIG_PATH` and `PATH` alone are not enough. |
| `No GTK build found at: ...` | Step 1 not run, or a half-built prefix. Point `SCRIB_GTK_PREFIX` at the one you have. |
| `'create-lists.bat' is not recognized`, `U1052` during gvsbuild | `NoDefaultCurrentDirectoryInExePath`. See step 1. |
| A red `NativeCommandError` quoting cargo's own `Finished` line | Not a failure. Windows PowerShell 5.1 wraps a native command's stderr into error records when the stream is redirected, and cargo writes status lines to stderr. Check `$LASTEXITCODE`, not the colour. |
| `cargo` works via `build.bat` but not bare, in PowerShell | `build.bat` sets the environment in a child process. Use it for every invocation, or set the four variables in the session. |
| `target\release\scribobulate.exe` run bare: a LIVE process, no window, ~3 MB working set, and no error dialog | The GTK runtime DLLs are not on `PATH` (gvsbuild's `bin`). It reads as a startup hang or an app that silently died, which is why it is here rather than under the row below: there is no loader error to search for, and the process stays alive. Launch through `build.bat`, or set the four variables in the session. MEASURED by the Windows seat while verifying a driven run. **A missing Visual C++ runtime gives the same signature** (live, ~4.6 MB, no window of its own); the one difference is a `VCRUNTIME140.dll was not found` System Error box, which is owned by `csrss` rather than the app, so any capture or window enumeration scoped to the app's PID misses it. The System event log records it under source `Application Popup`. MEASURED on a runtime-stripped VM. |
| Installed, but the app will not start: `VCRUNTIME140.dll was not found` | Microsoft's Visual C++ runtime is missing. Setup installs it when absent, via Microsoft's own `vc_redist.x64.exe` behind an admin prompt; declining that prompt aborts the install. So seeing this after a completed install means the runtime was removed afterwards: re-run Setup, or install the [redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe). |

**`Remove-Item Env:NoDefaultCurrentDirectoryInExePath` is the fix;
`[Environment]::SetEnvironmentVariable(..., $null)` is not.** The latter leaves
the variable *defined but empty*, and `cmd.exe` tests whether it is defined, not
what it holds. Both make `$env:VAR` print empty, so checking that confirms
nothing.

## One thing worth knowing about your GTK

`gvsbuild build --configuration release` compiles `g_assert` **out** of GTK and
GSK. The prebuilt archive CI uses is gvsbuild's `debug-optimized` default and
keeps its assertions, and nothing on disk distinguishes the two — same version,
same directory name, same `pkg-config --modversion`.

So every `g_assert`-backed GTK contract is unenforced on your machine and
enforced in CI, and a local run cannot fail that class of defect. When the
question touches GTK behaviour, point `SCRIB_GTK_PREFIX` at an unpacked
`GTK4_Gvsbuild_<version>_x64.zip` for that run only, and leave your everyday
prefix where it is.
