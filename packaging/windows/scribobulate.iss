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
