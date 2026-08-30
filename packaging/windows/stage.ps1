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

# ---------------------------------------------------------------------------
# THE MSVC RUNTIME IS NOT STAGED, AND THAT IS THE POINT.
#
# This script used to copy vcruntime140.dll and vcruntime140_1.dll app-local out
# of the Visual Studio redistributable directory. Doing that made us a
# redistributor of Microsoft's Distributable Code, which carries a term an
# Apache-2.0 LICENSE file does not satisfy: the distributor must require end
# users to AGREE to protective terms, which is a click-through, not a file on
# disk. Not shipping the files removes the obligation instead of documenting it.
#
# The dependency has NOT gone away -- it moved to the machine's own copy.
# MEASURED with dumpbin /dependents over all 38 staged binaries: 37 import
# vcruntime140.dll (everything except the CRT DLL that imports the other one),
# vcruntime140_1.dll is imported by cairo-2.dll ALONE, and no msvcp140,
# concrt140, mfc, vcomp or vccorlib is imported anywhere in the tree. So removing
# exactly these two files is sufficient; nothing else drags a CRT DLL in.
#
# scribobulate.iss now installs Microsoft's own redistributable when the machine
# does not already carry it, so Microsoft's terms travel with Microsoft's code.
#
# A DEVELOPER BOX CANNOT NOTICE IF THIS IS WRONG. Every machine that can build
# this software already has the runtime, so the app starts either way and the
# green tells you nothing -- see packaging/windows/README.md for what an honest
# verification of this requires.
# ---------------------------------------------------------------------------

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

# The RNG/DTD validation schemas. GtkSourceView's .lang specs and style schemes
# are compiled into gtksourceview-5-0.dll as GResource, so there is nothing else
# to ship for syntax highlighting.
#
# THE COPY IS RECURSIVE AND THE PREFIX HOLDS MORE THAN THAT -- which is how a font
# nobody uses reached every installer built to date. The comment above used to say
# "only the RNG/DTD validation schemas" and describe the INTENT, while the code
# took the whole directory. So the exclusion is spelled out below rather than left
# to a reader to notice the gap between the two.
if (Test-Path "$GtkPrefix\share\gtksourceview-5") {
    Copy-Item "$GtkPrefix\share\gtksourceview-5" "$OutDir\share\" -Recurse
}

# BuilderBlocks.ttf is dropped: a 4-glyph synthetic font (.notdef/block/empty/
# smallblock) that exists for GtkSourceMap's minimap mosaic, and Scribobulate has
# no minimap. MEASURED, not reasoned -- the claim was confirmed by installing the
# built installer and running it, twice on one install, with the font present and
# then removed, at pinned window geometry: the editor rendered PIXEL-IDENTICAL
# both times. The only 236 differing pixels sat in a 42x16 box on the "View"
# menubar label, a focus underline left by the test's own keystrokes.
#
# Looking alone would NOT have settled this. An unused font is invisible, so a
# correct-looking window cannot tell "unused" from "used and fine"; only the A/B
# discriminates. The consumer is established from the shipped DLL's own strings,
# where the minimap CSS `font-family: BuilderBlocks; font-size: 4px` sits beside
# GTK_SOURCE_IS_MAP and gtk_source_map_set_view -- so this no longer rests on a
# grep for GtkSourceMap in our source.
#
# DENYLIST, NOT ALLOWLIST, deliberately. Copying only the three known-good
# subdirectories would silently omit anything upstream adds that we DO need, and
# a missing runtime file is a worse failure than a stray 500-byte one. The cost
# is the reverse blind spot: a future addition here ships unnoticed, and the
# licence gate will not catch it because the gtksourceview row claims the whole
# subtree by pattern.
#
# Not conditional on the file existing: Remove-Item is given a path that may be
# absent if upstream ever drops it, and this must not become the line that breaks
# staging on a gvsbuild bump.
$sourceViewFonts = Join-Path $OutDir 'share\gtksourceview-5\fonts'
if (Test-Path $sourceViewFonts) {
    Remove-Item $sourceViewFonts -Recurse -Force
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

# The sprite copy every platform ships. WHY it is shipped is stated once, in
# packaging/linux/payload.sh beside the Linux copy of this step -- read it there rather
# than trusting a second copy here. The packaging scripts once carried that rationale
# verbatim, which hid the fact that the commands underneath the copies did different
# things with an empty directory, a subdirectory, and a filename with a space.
# `-Recurse -Force` survives all three; the Linux side now does too. `-Force` is
# load-bearing rather than defensive: without it a second copy into an existing
# destination throws ResourceExists per directory, which terminates under this script's
# $ErrorActionPreference = 'Stop'.
Copy-Item "$RepoRoot\data\sprites" "$OutDir\share\scribobulate\" -Recurse -Force

# ---------------------------------------------------------------------------
# The licence texts the product is obliged to carry. No scribobulate.iss change
# is needed for any of them: line 76 already takes {#StageDir}\* with
# recursesubdirs, so whatever lands here is installed.
#
# THE INSTALLER WAS SHOWING THE LICENCE AND SHIPPING IT NOWHERE. scribobulate.iss
# sets LicenseFile to the repo's LICENSE, which displays it in the setup wizard
# and installs nothing. The installed tree contained no licence text at all --
# not ours, not the syntax grammars', not librsvg's Rust graph's -- while the
# wizard made the obligation look discharged. An absence that reads as handled is
# worse than a plain omission, and it is what the three "row matches no staged
# file" entries in licenses.psd1 were reporting.
#
# TWO GO TO THE ROOT AND ONE DOES NOT. LICENSE and THIRD-PARTY-LICENSES.md are
# about the product as a whole and sit where a person looks first. The librsvg
# notice is about one DLL in bin\, so it goes under share\licenses\librsvg\ --
# and it has to be staged from the repo rather than copied out of the GTK prefix,
# because the crates it attributes are statically linked into rsvg-2-2.dll and
# leave no file of their own in any installed tree.
#
# MISSING SOURCES THROW. A missing DLL above throws, and a missing licence is the
# more serious absence -- it must not be the one failure this script shrugs off.
# verify-licenses.ps1 would catch it later, but a packager that knowingly emits a
# short tree makes a downstream gate the only thing standing between us and
# shipping it, which is one process change away from nothing at all.
# ---------------------------------------------------------------------------
# The librsvg Rust notice USED TO BE COPIED HERE BY HAND. It no longer is: it is
# an ordinary row in licenses.psd1, so the manifest-driven block below stages it
# like every other licence text, to share\licenses\librsvg-rust\. Copying it here
# as well would put 219 KB of identical bytes in two directories and give the
# reader two places to believe are authoritative.
$notices = @(
    @{ From = "$RepoRoot\LICENSE"
       To   = "$OutDir\LICENSE" }
    @{ From = "$RepoRoot\THIRD-PARTY-LICENSES.md"
       To   = "$OutDir\THIRD-PARTY-LICENSES.md" }
)
foreach ($n in $notices) {
    if (-not (Test-Path $n.From)) { throw "Licence text not found at $($n.From)" }
    Copy-Item $n.From $n.To
}

# ---------------------------------------------------------------------------
# EVERY COMPONENT'S LICENCE TEXT, staged from the manifest.
#
# THIS IS THE HALF THAT WAS MISSING, and licenses.psd1 said so in its own opening
# line: "the installer currently ships not one line of their licence text". The
# table names, for all 34 components, where each licence text COMES FROM -- and
# nothing ever copied them anywhere. verify-licenses.ps1 reads those Sources off
# the BUILD MACHINE, from the repo and the GTK prefix, so all four of its
# conditions could pass while the installed product carried no LGPL text at all.
# Exactly the defect recorded above for LICENSE and THIRD-PARTY-LICENSES.md --
# an obligation that reads as discharged because a gate is green -- one layer out.
#
# DRIVEN BY THE MANIFEST, NOT BY A SECOND LIST. A hand-kept list of files to copy
# would be a fourth restatement of the table and would drift from it on the first
# dependency change, silently, because both would still read plausibly. The rows
# are the single source of truth: add a component there and its text ships here.
#
# One text per component directory, named by row Id rather than by upstream
# filename, so `share\licenses\glib\` answers "what covers GLib" without the
# reader knowing that GLib's text happens to be called LGPL-2.1-or-later.txt.
#
# ROWS WITH NO TEXT ARE REPORTED, NOT SKIPPED SILENTLY, and deliberately do NOT
# throw. No row is in that state today -- msvc-runtime was the last one, and it
# was deleted rather than closed when the runtime stopped shipping -- so this path
# is dormant rather than dead: the next component whose terms exist only online
# lands here. Throwing would make this script fail on a condition the gate already
# reports precisely, and would block staging on a decision that has nothing to do
# with staging. The gate's condition 3 stays the hard failure.
# ---------------------------------------------------------------------------
$manifest = Import-PowerShellDataFile -LiteralPath "$PSScriptRoot\licenses.psd1"
$noText   = @()
$staged   = 0

foreach ($row in $manifest.Rows) {
    if (-not $row.Source) { continue }
    foreach ($src in @($row.Source)) {
        # Same resolution as verify-licenses.ps1's New-SourceReader. A scheme it
        # would reject is a typo in the table, so it throws here too rather than
        # quietly staging nothing.
        if ($src -like 'prefix:*') {
            $from = Join-Path $GtkPrefix $src.Substring(7)
        } elseif ($src -like 'repo:*') {
            $from = Join-Path $RepoRoot  $src.Substring(5)
        } else {
            throw "Row '$($row.Id)' Source must start with 'prefix:' or 'repo:', got: $src"
        }

        if (-not (Test-Path -LiteralPath $from -PathType Leaf)) {
            $noText += "$($row.Id) -> $src"
            continue
        }

        $destDir = Join-Path "$OutDir\share\licenses" $row.Id
        New-Item -ItemType Directory -Force -Path $destDir | Out-Null
        Copy-Item -LiteralPath $from -Destination (Join-Path $destDir (Split-Path $from -Leaf))
        $staged++
    }
}

Write-Host "Staged $staged licence texts for $($manifest.Rows.Count) components"
foreach ($m in $noText) { Write-Warning "no licence text on disk: $m" }

$files = Get-ChildItem $OutDir -Recurse -File
$size  = ($files | Measure-Object Length -Sum).Sum
Write-Host ("Staged {0} files, {1:N1} MB -> {2}" -f $files.Count, ($size / 1MB), (Resolve-Path $OutDir))
