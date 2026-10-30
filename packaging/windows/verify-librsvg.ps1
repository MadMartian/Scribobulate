<#
.SYNOPSIS
    Is the librsvg this installer delivers at or above the security floor?
.DESCRIPTION
    CVE-2026-96889 / RUSTSEC-2026-0305 (librsvg #1241): a use-after-free when a nested
    XInclude redefines an XML entity, fixed in librsvg 2.63.2. The floor is not written here:
    it is read from packaging\librsvg-floor, the one place it is written, which the macOS
    gate (packaging/macos/verify-librsvg.sh) reads too.

    THE VERSION IS READ FROM THE STAGED DLL, by loading it and reading its exported
    rsvg_{major,minor,micro}_version. Not from pkg-config and not from the gvsbuild pin: both
    describe the prefix, and the staged tree is what is delivered. A prefix and a stage can
    disagree, and a developer prefix built by an older gvsbuild is exactly the case this
    gate exists for.

    The DLL is loaded with LOAD_WITH_ALTERED_SEARCH_PATH, so its dependencies resolve from
    the staged bin\ beside it, and in a CHILD process, so no staged file stays mapped into
    the caller (which goes on to compile the installer from that tree).

    FAILS CLOSED: no librsvg in the tree, a floor file that is missing or malformed, a DLL
    that will not load, or missing version symbols are all failures, never a skipped check.
    A tree without librsvg also has no working SVG loader, so there is nothing to pass.
.PARAMETER Path
    The staged tree (its bin\rsvg-2-2.dll is read) or a path to an rsvg-2-2.dll.
.PARAMETER SelfTest
    Prove the comparison and the reader can fail, then exit.
.EXAMPLE
    .\packaging\windows\verify-librsvg.ps1 build\stage\Scribobulate
.NOTES
    Negative control, not in -SelfTest because it needs an older prefix that comes and goes:
    pointing -Path at a gvsbuild 2026.8.0 prefix's bin\rsvg-2-2.dll (librsvg 2.62.3) must FAIL.
#>
[CmdletBinding()]
param(
    [string]$Path,
    [switch]$SelfTest,
    # Internal: the child process that loads the DLL and prints its version.
    [string]$ReadVersionOf
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($ReadVersionOf) {
    Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public static class RsvgVersionReader {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll", CharSet = CharSet.Ansi, SetLastError = true)]
    static extern IntPtr GetProcAddress(IntPtr module, string name);
    [DllImport("kernel32.dll")]
    static extern bool FreeLibrary(IntPtr module);
    const uint LOAD_WITH_ALTERED_SEARCH_PATH = 0x8;
    public static string Read(string dll) {
        IntPtr module = LoadLibraryExW(dll, IntPtr.Zero, LOAD_WITH_ALTERED_SEARCH_PATH);
        if (module == IntPtr.Zero)
            throw new Exception("cannot load " + dll + " (Win32 error " + Marshal.GetLastWin32Error() + ")");
        try {
            var parts = new string[3];
            var names = new[] { "major", "minor", "micro" };
            for (int i = 0; i < 3; i++) {
                IntPtr p = GetProcAddress(module, "rsvg_" + names[i] + "_version");
                if (p == IntPtr.Zero) throw new Exception(dll + " exports no rsvg_" + names[i] + "_version");
                parts[i] = ((uint)Marshal.ReadInt32(p)).ToString();
            }
            return string.Join(".", parts);
        } finally { FreeLibrary(module); }
    }
}
'@
    [RsvgVersionReader]::Read($ReadVersionOf)
    exit 0
}

# The repository root is resolved here, not in a param default: under `powershell -File`,
# $PSScriptRoot is empty while defaults are evaluated (see verify-licenses.ps1).
$FloorFile = Join-Path $PSScriptRoot '..\librsvg-floor'

# The floor, as written in packaging\librsvg-floor. Throws on anything but one x.y.z line.
function Get-Floor([string]$File) {
    if (-not (Test-Path -LiteralPath $File -PathType Leaf)) { throw "the librsvg floor file is missing: $File" }
    $text = [System.IO.File]::ReadAllText($File).Trim()
    if ($text -notmatch '^\d+\.\d+\.\d+$') { throw "the librsvg floor file does not hold one x.y.z version: '$text' ($File)" }
    $text
}

# $true when $Have >= $Want, numerically per component (so 2.100.0 > 2.63.2, which a string
# compare gets wrong). Throws on a version that is not x.y.z, so garbage never passes.
function Test-VersionAtLeast([string]$Have, [string]$Want) {
    foreach ($v in $Have, $Want) { if ($v -notmatch '^\d+\.\d+\.\d+$') { throw "not a version: '$v'" } }
    $h = [int[]]($Have -split '\.'); $w = [int[]]($Want -split '\.')
    for ($i = 0; $i -lt 3; $i++) {
        if ($h[$i] -gt $w[$i]) { return $true }
        if ($h[$i] -lt $w[$i]) { return $false }
    }
    $true
}

# The runtime version of the librsvg at $Dll, read in a child process. Throws when it
# cannot be read.
function Read-RsvgVersion([string]$Dll) {
    if (-not (Test-Path -LiteralPath $Dll -PathType Leaf)) { throw "no librsvg at $Dll" }
    $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath -ReadVersionOf $Dll 2>&1
    $code = $LASTEXITCODE
    $text = (@($out) | ForEach-Object { "$_" }) -join ' '
    if ($code -ne 0 -or $text -notmatch '^\d+\.\d+\.\d+$') { throw "cannot read the librsvg version of ${Dll}: $text" }
    $text
}

if ($SelfTest) {
    $failures = 0
    function Check([string]$Have, [string]$Expect) {
        $got = try { if (Test-VersionAtLeast $Have $script:Floor) { 'ok' } else { 'fail' } } catch { 'fail' }
        if ($got -eq $Expect) { Write-Host "   ok     $Have vs $script:Floor" }
        else { Write-Host "   FAILED $Have vs $script:Floor (expected $Expect, got $got)"; $script:failures++ }
    }
    function CheckThrows([string]$Label, [scriptblock]$Body) {
        $threw = try { & $Body | Out-Null; $false } catch { $true }
        if ($threw) { Write-Host "   ok     $Label fails closed" }
        else { Write-Host "   FAILED $Label did not fail"; $script:failures++ }
    }
    $script:Floor = Get-Floor $FloorFile
    # ANTI-VACUITY first: a comparator that refuses everything passes every "fail" case.
    Check $script:Floor ok
    Check '2.63.10' ok
    Check '2.100.0' ok
    Check '3.0.0' ok
    Check '2.63.1' fail
    Check '2.62.3' fail
    Check '1.99.99' fail
    Check 'garbage' fail
    Check '2.63' fail
    # The floor file and the reader fail closed.
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("verify-librsvg-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        Set-Content -LiteralPath "$tmp\floor" -Value '2.63' -Encoding ascii
        CheckThrows 'a malformed floor file' { Get-Floor "$tmp\floor" }
        CheckThrows 'a missing floor file' { Get-Floor "$tmp\absent" }
        CheckThrows 'a missing DLL' { Read-RsvgVersion "$tmp\rsvg-2-2.dll" }
        Set-Content -LiteralPath "$tmp\rsvg-2-2.dll" -Value 'not a DLL' -Encoding ascii
        CheckThrows 'a file that is not a DLL' { Read-RsvgVersion "$tmp\rsvg-2-2.dll" }
        CheckThrows 'a DLL without the version symbols' { Read-RsvgVersion "$env:SystemRoot\System32\kernel32.dll" }
    } finally { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }
    if ($failures) { Write-Host "verify-librsvg self-test: $failures FAILED"; exit 1 }
    Write-Host 'verify-librsvg self-test: all passed'
    exit 0
}

if (-not $Path) { throw 'usage: verify-librsvg.ps1 <staged tree | rsvg-2-2.dll> | -SelfTest' }
$dll = if (Test-Path -LiteralPath $Path -PathType Container) { Join-Path $Path 'bin\rsvg-2-2.dll' } else { $Path }
$floor = Get-Floor $FloorFile
$have = Read-RsvgVersion $dll
if (Test-VersionAtLeast $have $floor) {
    Write-Host "   librsvg $have >= $floor ($dll)"
    exit 0
}
Write-Host "error: $dll is librsvg $have, below the security floor $floor (CVE-2026-96889)."
Write-Host '       Move the gvsbuild pin to a release carrying a fixed librsvg; never lower the floor to pass.'
exit 1
