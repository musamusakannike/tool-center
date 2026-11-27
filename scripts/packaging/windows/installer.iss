#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef SourceBundleDir
  #define SourceBundleDir "..\..\..\app\build\windows\x64\runner\Release"
#endif
#ifndef OutputDirPath
  #define OutputDirPath "..\..\..\dist"
#endif

[Setup]
AppId={{8D8E42B9-9A41-4A1E-BE8C-6E33F2D0A401}
AppName=Tool Center
AppVersion={#AppVersion}
AppPublisher=Tool Center Contributors
AppPublisherURL=https://github.com/musamusakannike/tool-center
DefaultDirName={autopf}\Tool Center
DefaultGroupName=Tool Center
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir={#OutputDirPath}
OutputBaseFilename=Tool-Center-{#AppVersion}-windows-x64-setup
Compression=lzma2/fast
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceBundleDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Tool Center"; Filename: "{app}\tool_center.exe"
Name: "{autodesktop}\Tool Center"; Filename: "{app}\tool_center.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\tool_center.exe"; Description: "{cm:LaunchProgram,Tool Center}"; Flags: nowait postinstall skipifsilent
