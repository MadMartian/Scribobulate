; Scribobulate — Windows installer (Inno Setup 6)
;
; Per-user install by design: no elevation prompt, nothing written outside
; HKCU and %LOCALAPPDATA%. A Markdown viewer does not need machine-wide
; installation, and requiring admin rights to install one is a friction cost
; with no matching benefit.
;
; Build with:
;   ISCC.exe /DStageDir="<path to staged tree>" packaging\windows\scribobulate.iss
;
; The staged tree is produced by packaging\windows\stage.ps1 — see the README
; beside this file for the full pipeline.

#ifndef StageDir
  #error StageDir is not defined. Pass /DStageDir="<path to staged tree>".
#endif
; Same rule as StageDir and for the same reason: package.ps1 discovers the
; Visual Studio redistributable and passes it explicitly, so an ISCC invocation
; without it builds NO installer rather than one that silently ships an app
; whose C runtime nothing provides.
#ifndef RedistFile
  #error RedistFile is not defined. Pass /DRedistFile="<path to vc_redist.x64.exe>".
#endif
#if !FileExists(RedistFile)
  #error RedistFile does not exist. package.ps1 discovers it; do not hand-write the path.
#endif

#define AppName        "Scribobulate"
; Read from Cargo.toml rather than restated here, so the installer cannot ship a
; stale version the moment the crate version moves — the same single source the
; macOS bundler derives from (packaging/macos/bundle.sh). ReadIni is the Windows
; INI reader, which trims whitespace around `=` and strips one surrounding pair of
; double quotes, so TOML's `version = "0.1.0"` under [package] arrives as 0.1.0.
#define AppVersion     ReadIni(SourcePath + "\..\..\Cargo.toml", "package", "version", "")
#if AppVersion == ""
  #error Could not read `version` from [package] in Cargo.toml. The installer must not \
         fall back to a hardcoded version — that is exactly the drift this reads it to avoid.
#endif
#define AppPublisher   "Extollit"
#define AppExeName     "scribobulate.exe"
; The version floor for the runtime check, READ OFF THE FILE WE EMBED rather
; than written here. A hardcoded floor is the same defect stage.ps1 records
; against hardcoding `Microsoft.VC143.CRT`: CI builds on a different Visual
; Studio than a developer box, so the correct minimum differs per build and any
; constant is wrong on one of them. GetFileVersion returns "14.44.35211.0".
#define RedistMajor
#define RedistMinor
#define RedistBld
#define RedistBuild
; GetVersionComponents is a PROCEDURE, not a function -- it returns nothing and
; writes through its out-parameters, so it has to be invoked with #expr and its
; success judged from what it wrote. Testing its "result" compiles to
; "Wrong unary operator" and stops the build, which is at least loud.
#expr GetVersionComponents(RedistFile, RedistMajor, RedistMinor, RedistBld, RedistBuild)
#if RedistMajor == 0
  #error Could not read a version from RedistFile. Refusing to build an installer \
         whose runtime check has no floor -- it would accept a redistributable too old \
         to satisfy the binaries we ship.
#endif

[Setup]
; Stable across versions — this is what lets an upgrade replace an existing
; install rather than sitting alongside it. Never regenerate it.
AppId={{7C4B9F2E-3A61-4D58-9E17-6B0D2F8A5C43}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; `lowest` keeps the whole install per-user: {autopf} resolves to
; {localappdata}\Programs and no UAC prompt is raised.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir={#StageDir}\..\..\installer
OutputBaseFilename=Scribobulate-{#AppVersion}-x64-setup
; setup.exe is a separate binary compiled by Inno, so it needs the .ico directly.
SetupIconFile={#SourcePath}\scribobulate.ico
; scribobulate.exe carries its own icon resource (build.rs embeds it), so this
; can name the app itself and stay correct if the art ever changes.
UninstallDisplayIcon={app}\bin\{#AppExeName}
; Without this, Programs and Features lists "Scribobulate version 0.1.0" — the
; version is already its own column there, so the name should just be the name.
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile={#SourcePath}\..\..\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; \
    GroupDescription: "Additional shortcuts:"; Flags: unchecked
; Unchecked by default — taking over a file type is the user's call, not ours.
; The ProgID below is registered either way, so Scribobulate always appears in
; "Open with" without claiming the default.
Name: "assocmd"; Description: "Make Scribobulate the default app for &Markdown (.md, .markdown) files"; \
    GroupDescription: "File associations:"; Flags: unchecked

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#SourcePath}\scribobulate.ico"; DestDir: "{app}"; Flags: ignoreversion
; EMBEDDED, not downloaded, so the installer stays turn-key offline -- the same
; property staging the runtime app-local used to provide. It costs ~24 MB in the
; setup.exe.
;
; dontcopy, NOT `DestDir: {tmp}` WITH A Check -- and the difference is a bug this
; file shipped with for one build. Entries in [Files] are processed AFTER
; PrepareToInstall runs, so a {tmp} entry has not been extracted yet at the moment
; the prerequisite needs to run: the ShellExec would target a file that does not
; exist. MEASURED, by forcing the detector True on a machine that does not need
; the runtime -- Setup aborted reporting that administrator approval was refused,
; which is a plausible message for an entirely different fault. dontcopy plus an
; explicit ExtractTemporaryFile puts the extraction where the code can order it.
Source: "{#RedistFile}"; Flags: dontcopy

; No IconFilename: the shortcuts take scribobulate.exe's own embedded icon, so
; the art has exactly one source. The loose .ico above stays for DefaultIcon
; below, which is the *document* icon and cannot come from the exe.
[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\bin\{#AppExeName}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\bin\{#AppExeName}"; Tasks: desktopicon

[Registry]
; --- ProgID: always registered, so the app is offered in "Open with" without
;     ever silently becoming the default handler. ---
Root: HKCU; Subkey: "Software\Classes\Scribobulate.Document"; \
    ValueType: string; ValueName: ""; ValueData: "Markdown Document"; Flags: uninsdeletekey
; The name shown for the APP in "Open with" and Settings ▸ Default apps. The
; shell resolves it as FriendlyAppName → the exe's VERSIONINFO FileDescription
; → the bare file name; scribobulate.exe now carries the middle one, so this is
; belt-and-braces — but it is also the only one of the three that a shipped
; installer can correct without a rebuild. Verified applied on an upgrade install
; via /LOG. It is no help against the MuiCache trap, though — see the README
; beside this file before trusting any observation about the displayed name.
Root: HKCU; Subkey: "Software\Classes\Scribobulate.Document"; \
    ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"
; The DOCUMENT icon (what a .md file looks like once associated), which is a
; different thing from the app icon the exe carries — hence the loose .ico.
Root: HKCU; Subkey: "Software\Classes\Scribobulate.Document\DefaultIcon"; \
    ValueType: string; ValueName: ""; ValueData: "{app}\scribobulate.ico"
Root: HKCU; Subkey: "Software\Classes\Scribobulate.Document\shell\open\command"; \
    ValueType: string; ValueName: ""; ValueData: """{app}\bin\{#AppExeName}"" ""%1"""

; Advertise as a candidate for .md/.markdown without displacing the incumbent.
Root: HKCU; Subkey: "Software\Classes\.md\OpenWithProgids"; \
    ValueType: string; ValueName: "Scribobulate.Document"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.markdown\OpenWithProgids"; \
    ValueType: string; ValueName: "Scribobulate.Document"; ValueData: ""; Flags: uninsdeletevalue

; --- Default handler: ONLY when the user opted in. ---
Root: HKCU; Subkey: "Software\Classes\.md"; ValueType: string; ValueName: ""; \
    ValueData: "Scribobulate.Document"; Tasks: assocmd; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.markdown"; ValueType: string; ValueName: ""; \
    ValueData: "Scribobulate.Document"; Tasks: assocmd; Flags: uninsdeletevalue

; Registered Applications — puts Scribobulate in Settings ▸ Default apps.
Root: HKCU; Subkey: "Software\Scribobulate\Capabilities"; \
    ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Scribobulate\Capabilities"; \
    ValueType: string; ValueName: "ApplicationDescription"; \
    ValueData: "Native Markdown viewer and editor that renders on the CPU"
Root: HKCU; Subkey: "Software\Scribobulate\Capabilities\FileAssociations"; \
    ValueType: string; ValueName: ".md"; ValueData: "Scribobulate.Document"
Root: HKCU; Subkey: "Software\Scribobulate\Capabilities\FileAssociations"; \
    ValueType: string; ValueName: ".markdown"; ValueData: "Scribobulate.Document"
Root: HKCU; Subkey: "Software\RegisteredApplications"; \
    ValueType: string; ValueName: "{#AppName}"; ValueData: "Software\Scribobulate\Capabilities"; \
    Flags: uninsdeletevalue

[Run]
Filename: "{app}\bin\{#AppExeName}"; Description: "Launch {#AppName}"; \
    Flags: nowait postinstall skipifsilent

; ---------------------------------------------------------------------------
;  The Microsoft C runtime, installed by Microsoft's own redistributable.
;
;  WHY THIS EXISTS. Every binary we ship imports vcruntime140.dll -- MEASURED,
;  37 of 38 staged binaries -- and we used to satisfy that by copying the DLL
;  app-local. That made us a redistributor of Microsoft's Distributable Code,
;  whose terms require the distributor to make end users AGREE to protective
;  terms. Running Microsoft's installer instead means Microsoft's terms travel
;  with Microsoft's code, and the obligation is gone rather than documented.
;
;  PrivilegesRequired=lowest IS DELIBERATELY UNCHANGED. Setup itself stays
;  per-user and raises no prompt. vc_redist.x64.exe carries its own
;  requireAdministrator manifest, so Windows raises UAC for that child process
;  alone -- and only on a machine that does not already have the runtime, which
;  is the minority case. The no-admin property is spent narrowly, not wholesale.
;  It has to be launched with ShellExec rather than Exec: CreateProcess from a
;  non-elevated parent fails an elevation-requiring image with error 740 instead
;  of prompting.
;
;  DECLINING THE PROMPT ABORTS THE INSTALL, and that is a choice rather than an
;  oversight. PrepareToInstall runs BEFORE any file is written, so a refusal
;  leaves the machine untouched. Installing an application that cannot start is
;  worse than declining to install it, and it is the outcome a happy-path test
;  never shows you.
; ---------------------------------------------------------------------------
[Code]
var
  RuntimeChecked: Boolean;
  RuntimeMissing: Boolean;

{ True when the machine has no x64 MSVC runtime at least as new as the one our
  binaries were built against.

  READS THE 64-BIT VIEW EXPLICITLY. Setup is a 32-bit process, so a plain HKLM
  query is redirected to Wow6432Node -- which on this box also holds an
  Installed=1, so the wrong view answers plausibly rather than failing. That is
  the shape that makes a redirection bug invisible.

  COMPARES A VERSION, NOT MERELY Installed=1. The CRT is forward-compatible but
  not backward: a machine carrying 14.0 from Visual Studio 2015 satisfies an
  existence check and then fails to start the app on a missing export, which is
  precisely the silent failure this whole change removes. }
function VCRuntimeNeeded: Boolean;
var
  Key: String;
  Installed, Major, Minor, Bld: Cardinal;
  NeedMajor, NeedMinor, NeedBld: Integer;
begin
  if RuntimeChecked then begin
    Result := RuntimeMissing;
    exit;
  end;

  Key := 'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64';
  Installed := 0; Major := 0; Minor := 0; Bld := 0;

  if not RegQueryDWordValue(HKEY_LOCAL_MACHINE_64, Key, 'Installed', Installed) then
    Installed := 0;
  RegQueryDWordValue(HKEY_LOCAL_MACHINE_64, Key, 'Major', Major);
  RegQueryDWordValue(HKEY_LOCAL_MACHINE_64, Key, 'Minor', Minor);
  RegQueryDWordValue(HKEY_LOCAL_MACHINE_64, Key, 'Bld',   Bld);

  { Split at COMPILE time by ISPP, so the runtime code carries no version
    parser. The registry's Major/Minor/Bld line up with the file version's
    first three components: 14.44.35211.0 against Major=14 Minor=44 Bld=35211. }
  NeedMajor := {#RedistMajor};
  NeedMinor := {#RedistMinor};
  NeedBld   := {#RedistBld};

  RuntimeMissing := (Installed <> 1) or
                    (Major < Cardinal(NeedMajor)) or
                    ((Major = Cardinal(NeedMajor)) and (Minor < Cardinal(NeedMinor))) or
                    ((Major = Cardinal(NeedMajor)) and (Minor = Cardinal(NeedMinor)) and
                     (Bld < Cardinal(NeedBld)));
  RuntimeChecked := True;
  Result := RuntimeMissing;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ExitCode: Integer;
begin
  Result := '';
  if not VCRuntimeNeeded then
    exit;

  { Ordered explicitly, because [Files] has not been processed yet -- see the
    comment on the dontcopy entry above. }
  ExtractTemporaryFile('vc_redist.x64.exe');

  if not ShellExec('runas', ExpandConstant('{tmp}\vc_redist.x64.exe'),
                   '/install /passive /norestart', '', SW_SHOW,
                   ewWaitUntilTerminated, ExitCode) then begin
    { ShellExec itself failed -- the overwhelmingly common cause is the user
      dismissing the UAC prompt (ERROR_CANCELLED, 1223). Nothing has been
      written yet; say what is missing and why, and stop. }
    Result := 'Scribobulate needs the Microsoft Visual C++ runtime, which this' + #13#10 +
              'computer does not have. Installing it requires administrator' + #13#10 +
              'approval, and that was not given.' + #13#10 + #13#10 +
              'Nothing has been installed. Run Setup again and approve the' + #13#10 +
              'prompt, or install the Microsoft Visual C++ Redistributable' + #13#10 +
              'separately and then run Setup again.';
    exit;
  end;

  { 3010 is "success, reboot required" and is not a failure. Anything else
    non-zero is: the runtime is absent and the app would not start. }
  if ExitCode = 3010 then begin
    NeedsRestart := True;
    exit;
  end;
  if ExitCode <> 0 then
    Result := 'The Microsoft Visual C++ runtime installer failed with code ' +
              IntToStr(ExitCode) + '.' + #13#10 + #13#10 +
              'Nothing has been installed, because Scribobulate cannot start' + #13#10 +
              'without that runtime.';
end;
