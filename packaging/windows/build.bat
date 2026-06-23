@echo off
rem ===========================================================================
rem  Scribobulate - build / run / test on Windows, with the GTK environment set.
rem
rem  Why this file exists: the GTK toolchain lives entirely inside the gvsbuild
rem  prefix, so out of the box `cargo build` cannot find pkg-config and
rem  `cargo run` cannot find the GTK DLLs - the app builds and then fails to
rem  start, or fails to build at all. The four variables below are the whole
rem  difference. `pipeline.ps1` sets the same four for the gated build; this is
rem  the everyday one-liner for working on the app.
rem
rem  Usage - run it from anywhere; the repo is located relative to this script,
rem  never from the current directory:
rem
rem     build                     cargo build
rem     build release [args...]   cargo build --release [args...]
rem     build run [file.md ...]   cargo run -- [file.md ...]
rem     build test [args...]      the GTK integration suite [+ libtest args]
rem     build <args...>           anything else goes to cargo verbatim (build clippy)
rem
rem  The GTK prefix is %SCRIB_GTK_PREFIX% when set, else gvsbuild's default
rem  (the same default pipeline.ps1 assumes).
rem ===========================================================================

rem Resolve the repo root NOW, before anything can `shift`: a bare `shift`
rem renumbers from %0, so the moment arguments are consumed %~dp0 stops naming
rem this script and silently starts resolving argument 1 against the current
rem directory instead. Nothing warns; pushd simply lands somewhere else.
set "SCRIB_REPO=%~dp0..\.."

rem `if "%VAR%"==""` rather than `if not defined VAR`: an inherited variable can
rem arrive defined-but-empty, which `defined` reports as set - the exact shape
rem that cost two GTK builds in ScrAP-165. This form treats empty and unset alike.
if "%SCRIB_GTK_PREFIX%"=="" set "SCRIB_GTK_PREFIX=C:\gtk-build\gtk\x64\release"

rem Check the directory PKG_CONFIG_PATH will point at, not merely the prefix:
rem a half-built or half-deleted gvsbuild tree still has the top folder, and
rem the failure it produces downstream (a gtk4-sys build script that "cannot
rem probe") names neither this variable nor gvsbuild.
if not exist "%SCRIB_GTK_PREFIX%\lib\pkgconfig" (
    echo.
    echo   No GTK build found at: %SCRIB_GTK_PREFIX%
    echo.
    echo   Build it with gvsbuild first - see packaging\windows\README.md - or
    echo   point SCRIB_GTK_PREFIX at the prefix you already have:
    echo.
    echo       set SCRIB_GTK_PREFIX=D:\somewhere\gtk\x64\release
    echo.
    goto :failed
)

rem Deliberately no `setlocal`: run from an interactive console, this leaves the
rem environment behind, so a plain `cargo run` works in that window afterwards -
rem which is the actual complaint this script answers. The guard keeps repeat
rem invocations idempotent instead of stacking PATH entries, and it records WHICH
rem prefix it applied rather than merely that it ran: keyed on a bare "1", a
rem console that had already run this script would validate a newly-set
rem SCRIB_GTK_PREFIX in the check above and then quietly build against the old
rem one. Switching prefixes prepends a second set of entries and the new one is
rem searched first, so it wins.
if /i "%SCRIB_GTK_ENV%"=="%SCRIB_GTK_PREFIX%" goto :environment_ready
set "PATH=%SCRIB_GTK_PREFIX%\bin;%PATH%"
set "PKG_CONFIG_PATH=%SCRIB_GTK_PREFIX%\lib\pkgconfig"
rem Appended only when there is something to append: prepending onto an unset
rem LIB/INCLUDE would leave a trailing `;`, and an empty entry in those lists
rem means "the current directory" to the toolchain.
if defined LIB (set "LIB=%SCRIB_GTK_PREFIX%\lib;%LIB%") else (set "LIB=%SCRIB_GTK_PREFIX%\lib")
if defined INCLUDE (set "INCLUDE=%SCRIB_GTK_PREFIX%\include;%INCLUDE%") else (set "INCLUDE=%SCRIB_GTK_PREFIX%\include")
set "SCRIB_GTK_ENV=%SCRIB_GTK_PREFIX%"
:environment_ready

rem ---------------------------------------------------------------------------
rem  Arguments are collected HERE, ahead of the pushd, while the caller's
rem  directory is still current: `build run notes.md` has to open the caller's
rem  notes.md, and %~f1 resolves against the current directory - collected after
rem  the pushd it would name a file in the repo root instead, which for a
rem  document that exists in both places is silently the wrong file.
rem ---------------------------------------------------------------------------
set "SCRIB_ARGS="
set "SCRIB_MODE=debug"
if "%~1"=="" goto :ready
set "SCRIB_MODE=verbatim"
if /i "%~1"=="release" goto :collect
if /i "%~1"=="run"     goto :collect
if /i "%~1"=="test"    goto :collect
goto :ready

rem A shift loop, not `%1 %2 %3 %4`: there is no "%* minus the first token", and
rem the numbered form drops everything past the fourth argument without a word.
rem That is what reduced `build test --skip <case>` to a bare `build test` -
rem the same class of defect as ScrAP-201 one layer up, and just as green.
:collect
set "SCRIB_MODE=%~1"
shift
:collect_next
if "%~1"=="" goto :ready
set "SCRIB_TOKEN=%1"
rem Only `run` takes paths. Rewriting a token that merely happens to name a file
rem would corrupt a test filter (`build test src`, run from the repo root), so
rem the expansion is scoped to the one mode whose arguments are documents. A
rem flag never names an existing file, so flags pass through untouched.
if /i not "%SCRIB_MODE%"=="run" goto :collect_add
if exist "%~1" set "SCRIB_TOKEN="%~f1""
:collect_add
set "SCRIB_ARGS=%SCRIB_ARGS% %SCRIB_TOKEN%"
shift
goto :collect_next

:ready
rem pushd, not `cd /d`: with no `setlocal` (see above) a `cd` would follow the
rem caller home and leave their console sitting in the repo root, which looks like
rem the script "moved" them somewhere.
pushd "%SCRIB_REPO%"
if errorlevel 1 goto :failed

if /i "%SCRIB_MODE%"=="debug"   goto :debug
if /i "%SCRIB_MODE%"=="release" goto :release
if /i "%SCRIB_MODE%"=="run"     goto :run
if /i "%SCRIB_MODE%"=="test"    goto :test

rem Verbatim. %* is immune to `shift`, so it is still the whole original command
rem line even where the collector above consumed arguments.
cargo %*
goto :finished

:debug
cargo build
goto :finished

:release
cargo build --release%SCRIB_ARGS%
goto :finished

:run
rem Everything after the "run" token is the document to open; SCRIB_ARGS already
rem carries its leading space, and any relative path in it is now absolute.
cargo run --%SCRIB_ARGS%
goto :finished

:test
rem --test-threads=1 for the libtest targets; the GTK suite is a harness = false
rem target that is sequential by construction (it owns the process main thread).
rem Trailing arguments land after the `--`, so `build test --skip <case>` reaches
rem the runner exactly as pipeline.ps1's step 5 passes it.
cargo test --features gtk-integration-tests -- --test-threads=1%SCRIB_ARGS%
goto :finished

:failed
set "RC=1"
goto :done

:finished
set "RC=%ERRORLEVEL%"
popd

:done
rem The four GTK variables are left behind on purpose (see above). These are this
rem script's own bookkeeping and are not part of that bargain.
set "SCRIB_REPO="
set "SCRIB_MODE="
set "SCRIB_ARGS="
set "SCRIB_TOKEN="
rem No `pause` here, deliberately. The usual "pause only when double-clicked"
rem idiom tests whether cmd's own command line contains this script's name — but
rem that is true of ANY `cmd /c "script"`, which is exactly how PowerShell (and
rem most launchers) invoke a .bat, so the script appears to hang waiting for a
rem keypress nobody knew to press. It also shells out to `find`, and MSYS2 is a
rem gvsbuild prerequisite: with its bin directory on PATH, `find` is Unix find and
rem the test means something else entirely. A tool run from a shell should just
rem exit; a double-clickable wrapper is where a pause belongs (README).
exit /b %RC%
