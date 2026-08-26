#define MyAppName "IoT Toolbox"
#define MyAppVersion GetEnv("VERSION")
#define MyAppExeName "iot-toolbox.exe"

[Setup]
AppId={{A80C2785-5E15-4D77-BF68-2C3989B63541}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=IoT Toolbox contributors
DefaultDirName={autopf}\IoT Toolbox
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=iot-toolbox-{#MyAppVersion}-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=iot-toolbox.ico
UninstallDisplayIcon={app}\{#MyAppExeName}

[Files]
Source: "..\..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent
