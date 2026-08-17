<#
.SYNOPSIS
    Stage a self-contained Scribobulate tree for Windows distribution.

.DESCRIPTION
    Copies the release binary together with the GTK4 runtime it needs into a
    single relocatable directory, which packaging\windows\scribobulate.iss then
    wraps in an installer.

    The layout is not arbitrary. GTK derives its installation prefix on Windows
    by taking the path of the loaded GLib DLL and stripping a trailing "bin", so
    DLLs MUST live in <root>\bin for <root>\share to be found. Flattening the
    tree breaks icon and schema lookup in ways that only show up at runtime.

.PARAMETER GtkPrefix
    The gvsbuild output prefix. Defaults to %SCRIB_GTK_PREFIX% when set, else
    gvsbuild's own default -- the same resolution order build.bat and pipeline.ps1
    use, so no entry point can quietly stage from a different GTK than the one the
    binary was built against.

.PARAMETER OutDir
    Where to write the staged tree. Cleared if it already exists.

.EXAMPLE
    .\packaging\windows\stage.ps1 -OutDir build\stage\Scribobulate
#>
[CmdletBinding()]
param(
    [string]$GtkPrefix = $(if ($env:SCRIB_GTK_PREFIX) { $env:SCRIB_GTK_PREFIX } else { "C:\gtk-build\gtk\x64\release" }),
    [string]$OutDir    = "$PSScriptRoot\..\..\build\stage\Scribobulate",
    [string]$RepoRoot  = "$PSScriptRoot\..\.."
)

$ErrorActionPreference = 'Stop'

$exeSrc = Join-Path $RepoRoot "target\release\scribobulate.exe"
if (-not (Test-Path $exeSrc)) {
    throw "Release binary not found at $exeSrc. Run 'cargo build --release' first."
}
if (-not (Test-Path $GtkPrefix)) {
    throw "GTK prefix not found at $GtkPrefix. Build it with: gvsbuild build --configuration release gtk4 gtksourceview5 adwaita-icon-theme"
}

if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path "$OutDir\bin" | Out-Null

# The runtime DLL set. The first 32 are those observed loaded by a running
# instance; rsvg-2-2 is NOT among them and must still ship — it is pulled in
# lazily by pixbufloader_svg.dll the first time an Adwaita *symbolic* icon is
# drawn. Omit it and you get a build that starts perfectly and then shows
# broken toolbar icons.
$dlls = @(
    'cairo-2', 'cairo-gobject-2', 'cairo-script-interpreter-2', 'epoxy-0',
    'ffi-8', 'fontconfig-1', 'freetype-6', 'fribidi-0', 'gdk_pixbuf-2.0-0',
    'gio-2.0-0', 'glib-2.0-0', 'gmodule-2.0-0', 'gobject-2.0-0',
    'graphene-1.0-0', 'gtk-4-1', 'gtksourceview-5-0', 'harfbuzz-subset',
    'harfbuzz', 'iconv', 'intl', 'jpeg62', 'libexpat', 'libpng16',
    'pango-1.0-0', 'pangocairo-1.0-0', 'pangoft2-1.0-0', 'pangowin32-1.0-0',
    'pcre2-8-0', 'pixman-1-0', 'tiff', 'xml2-16', 'zlib1',
    'rsvg-2-2'
)

$missing = @()
foreach ($d in $dlls) {
    $p = Join-Path "$GtkPrefix\bin" "$d.dll"
    if (Test-Path $p) { Copy-Item $p "$OutDir\bin\" } else { $missing += $d }
}
if ($missing.Count -gt 0) {
    throw "Missing DLLs in ${GtkPrefix}\bin: $($missing -join ', ')"
}

Copy-Item $exeSrc "$OutDir\bin\"

# GLib's helper executables. These are NOT optional tooling -- gdbus.exe is a
# load-bearing part of single-instance behaviour on Windows, and omitting it
# produces a build that starts perfectly and then opens a second PROCESS for
# every document (TDD 8.1/8.2 broken in the shipped installer while passing in
# the dev tree, where gvsbuild's bin is on PATH -- ScrAP-249).
#
# Why: GIO has no Win32-native uniqueness backend. `GApplication` negotiates
# uniqueness over a D-Bus session bus on Windows exactly as on Linux, and with
# no DBUS_SESSION_BUS_ADDRESS set, GLib autolaunches one by spawning gdbus.exe
# from beside the loaded GLib DLL. No gdbus.exe means no bus; no bus means
# g_application_register() still succeeds, still reports is_remote() == false,
# and every launch elects itself primary -- the same silent degradation macOS
# showed in ScrAP-174, reached by a different route.
#
# Measured on this staged layout: without gdbus.exe, two launches on one
# document give two processes and two windows; with it, one. Both copies of the
# app unify even when installed at different paths, so the install location is
# not a factor -- only the helper's presence is.
$helpers = @('gdbus')

$missingHelpers = @()
foreach ($h in $helpers) {
    $p = Join-Path "$GtkPrefix\bin" "$h.exe"
    if (Test-Path $p) { Copy-Item $p "$OutDir\bin\" } else { $missingHelpers += $h }
}
if ($missingHelpers.Count -gt 0) {
    throw "Missing helper executables in ${GtkPrefix}\bin: $($missingHelpers -join ', ')"
}

# gdk-pixbuf SVG loader plus its cache. gvsbuild writes the cache with a
# RELATIVE loader path, so it survives relocation without rewriting — do not
# regenerate it here, which would bake in absolute build-machine paths.
New-Item -ItemType Directory -Force -Path "$OutDir\lib\gdk-pixbuf-2.0\2.10.0\loaders" | Out-Null
Copy-Item "$GtkPrefix\lib\gdk-pixbuf-2.0\2.10.0\loaders.cache" "$OutDir\lib\gdk-pixbuf-2.0\2.10.0\"
Copy-Item "$GtkPrefix\lib\gdk-pixbuf-2.0\2.10.0\loaders\*.dll" "$OutDir\lib\gdk-pixbuf-2.0\2.10.0\loaders\"

# GTK aborts at startup without its compiled GSettings schemas.
New-Item -ItemType Directory -Force -Path "$OutDir\share\glib-2.0\schemas" | Out-Null
Copy-Item "$GtkPrefix\share\glib-2.0\schemas\gschemas.compiled" "$OutDir\share\glib-2.0\schemas\"

# Adwaita supplies every named icon in the toolbar; hicolor is the fallback
# theme GTK expects to exist.
New-Item -ItemType Directory -Force -Path "$OutDir\share\icons" | Out-Null
Copy-Item "$GtkPrefix\share\icons\Adwaita" "$OutDir\share\icons\" -Recurse
Copy-Item "$GtkPrefix\share\icons\hicolor" "$OutDir\share\icons\" -Recurse

# Only the RNG/DTD validation schemas. GtkSourceView's .lang specs and style
# schemes are compiled into gtksourceview-5-0.dll as GResource, so there is
# nothing else to ship for syntax highlighting.
if (Test-Path "$GtkPrefix\share\gtksourceview-5") {
    Copy-Item "$GtkPrefix\share\gtksourceview-5" "$OutDir\share\" -Recurse
}

# The reading themes, as the Linux packages already install them
# (packaging/linux/payload.sh -> /usr/share/scribobulate/themes.toml). The same
# file is compiled into the binary as the last-resort fallback, so this copy is
# never REQUIRED -- omitting it costs nothing at startup and every shipped theme
# still resolves. What it costs is discoverability: a Windows user who wants to
# add or tweak a theme otherwise has no installed copy to read, and the search
# path's first row (%APPDATA%\scribobulate\themes.toml) is a file they must
# invent from scratch. Linux users have had the reference copy since the feature
# shipped; this closes that gap rather than adding a mechanism.
#
# It lands under <root>\share because that is search-path row 3
# ($XDG_DATA_DIRS -> .../scribobulate/themes.toml), and GLib derives <root>\share
# on Windows from the loaded module's prefix -- the SAME prefix rule stated at the
# top of this script for icons and schemas, so it costs no new assumption. Being
# row 3, it sits BELOW a user override (row 1), which is what makes an override
# still win against it.
New-Item -ItemType Directory -Force -Path "$OutDir\share\scribobulate" | Out-Null
Copy-Item "$RepoRoot\data\themes.toml" "$OutDir\share\scribobulate\"

$files = Get-ChildItem $OutDir -Recurse -File
$size  = ($files | Measure-Object Length -Sum).Sum
Write-Host ("Staged {0} files, {1:N1} MB -> {2}" -f $files.Count, ($size / 1MB), (Resolve-Path $OutDir))
