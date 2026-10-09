; Shotoshot - per-user Windows installer (Inno Setup 6).
; Build: powershell -NoProfile -ExecutionPolicy Bypass -File packaging\build-installer.ps1
; Test:  powershell -NoProfile -ExecutionPolicy Bypass -File packaging\test-installer.ps1
; RULE: keep AppVersion equal to [workspace.package] version in Cargo.toml (checked by the build script).

#define AppVersion "0.2.0"
#define AppName "Shotoshot"
#define AppExeName "shotoshot.exe"
#define AppPublisher "Mickaël Mancini"
; Former product name (before 0.2.0, renamed on 2026-09-28): the upgrade removes that install.
#define OldAppName "Shot Shot"
#define OldExeName "shotshot.exe"

[Setup]
; NEVER change AppId: it identifies the installed product for upgrades and uninstall.
AppId={{9C13F890-2667-4ED8-AE3B-6C5A0516295C}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
VersionInfoCompany={#AppPublisher}
; Per-user install, no UAC prompt: {autopf} resolves to %LOCALAPPDATA%\Programs.
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#AppName}
; Versions before 0.2.0 were named "Shot Shot": move them to the new folder, never reuse it.
UsePreviousAppDir=no
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=..\crates\shotoshot-app\assets\shotoshot.ico
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
OutputDir=out
OutputBaseFilename=Shotoshot-Setup-{#AppVersion}
; Restart Manager stays as a fallback; the [Code] section closes the app first.
CloseApplications=yes
RestartApplications=no
; No AppMutex: development builds share the app's mutex names, so it would
; report a dev instance as "running" and block install/uninstall.

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion

; The pre-0.2.0 install becomes this one: same AppId, so its uninstall entry is replaced.
[InstallDelete]
Type: filesandordirs; Name: "{autopf}\{#OldAppName}"
Type: files; Name: "{autoprograms}\{#OldAppName}.lnk"
Type: files; Name: "{autodesktop}\{#OldAppName}.lnk"

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

; Gives Print Screen back to the Windows Snipping Tool if Shotoshot took it (no-op otherwise).
; Runs before files are removed; the app owns this logic (crates/shotoshot-app/src/print_screen.rs).
[UninstallRun]
Filename: "{app}\{#AppExeName}"; Parameters: "--restore-print-screen"; Flags: runhidden waituntilterminated; RunOnceId: "RestorePrintScreen"

[Code]
const
  WM_CLOSE = $0010;
  AppWindowClass = 'Shotoshot.Window';
  OldWindowClass = 'Shotshot.Window';
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  RunValue = 'Shotoshot';
  OldRunValue = 'Shotshot';

function FindWindowEx(Parent, ChildAfter: HWND; ClassName: String; WindowName: Cardinal): HWND;
  external 'FindWindowExW@user32.dll stdcall';
function GetWindowThreadProcessId(Wnd: HWND; var ProcessId: Cardinal): Cardinal;
  external 'GetWindowThreadProcessId@user32.dll stdcall';
function GetTickCount: Cardinal;
  external 'GetTickCount@kernel32.dll stdcall';

function InstalledExePath: String;
begin
  Result := ExpandConstant('{app}\{#AppExeName}');
end;

function OldExePath: String;
begin
  Result := ExpandConstant('{autopf}\{#OldAppName}\{#OldExeName}');
end;

{ Returns ',pid1,pid2,' for processes whose image is exactly ExePath.
  Scoped by full path so a development build running elsewhere is never touched.
  When Terminate is true, those processes are killed. }
function InstalledProcesses(ExeName, ExePath: String; Terminate: Boolean): String;
var
  Locator, Service, Items, Item: Variant;
  I: Integer;
begin
  Result := ',';
  Locator := CreateOleObject('WbemScripting.SWbemLocator');
  Service := Locator.ConnectServer('.', 'root\CIMV2');
  Items := Service.ExecQuery('SELECT ProcessId, ExecutablePath FROM Win32_Process WHERE Name = ''' + ExeName + '''');
  for I := 0 to Items.Count - 1 do
  begin
    Item := Items.ItemIndex(I);
    if not VarIsNull(Item.ExecutablePath) then
      if CompareText(Item.ExecutablePath, ExePath) = 0 then
      begin
        Result := Result + IntToStr(Item.ProcessId) + ',';
        if Terminate then
          Item.Terminate(0);
      end;
  end;
end;

{ Wall-clock wait: each WMI query itself takes a few hundred milliseconds. }
function WaitForExit(ExeName, ExePath: String; TimeoutMs: Cardinal): Boolean;
var
  Start: Cardinal;
begin
  Start := GetTickCount;
  Result := InstalledProcesses(ExeName, ExePath, False) = ',';
  while not Result and (GetTickCount - Start < TimeoutMs) do
  begin
    Sleep(100);
    Result := InstalledProcesses(ExeName, ExePath, False) = ',';
  end;
end;

{ Asks the app installed at ExePath to quit (WM_CLOSE lets it remove its tray icon),
  then kills whatever is left. }
procedure CloseApp(ExeName, ExePath, WindowClass: String);
var
  Pids: String;
  Wnd: HWND;
  Pid: Cardinal;
begin
  try
    Pids := InstalledProcesses(ExeName, ExePath, False);
    if Pids = ',' then
      Exit;
    Log('Closing running Shotoshot processes (' + ExePath + '):' + Pids);
    Wnd := FindWindowEx(0, 0, WindowClass, 0);
    while Wnd <> 0 do
    begin
      Pid := 0;
      GetWindowThreadProcessId(Wnd, Pid);
      if Pos(',' + IntToStr(Pid) + ',', Pids) > 0 then
        PostMessage(Wnd, WM_CLOSE, 0, 0);
      Wnd := FindWindowEx(0, Wnd, WindowClass, 0);
    end;
    if WaitForExit(ExeName, ExePath, 3000) then
      Log('Closed gracefully: ' + ExePath)
    else
    begin
      Log('Graceful close timed out; terminating.');
      InstalledProcesses(ExeName, ExePath, True);
      if WaitForExit(ExeName, ExePath, 5000) then
        Log('Terminated: ' + ExePath)
      else
        Log('Still running: ' + ExePath);
    end;
  except
    Log('Could not close ' + ExePath + ': ' + GetExceptionMessage);
  end;
end;

{ Closes this installation and a pre-0.2.0 "Shot Shot" one. }
procedure CloseInstalledApp;
begin
  CloseApp('{#AppExeName}', InstalledExePath, AppWindowClass);
  CloseApp('{#OldExeName}', OldExePath, OldWindowClass);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  CloseInstalledApp;
  Result := '';
end;

{ A pre-0.2.0 "start with Windows" value points to the removed exe: keep the user's
  choice under the new name, pointing to this installation. }
procedure CurStepChanged(CurStep: TSetupStep);
var
  Command: String;
begin
  if CurStep <> ssPostInstall then
    Exit;
  if RegQueryStringValue(HKCU, RunKey, OldRunValue, Command) then
    if CompareText(RemoveQuotes(Command), OldExePath) = 0 then
    begin
      RegWriteStringValue(HKCU, RunKey, RunValue, '"' + InstalledExePath + '"');
      RegDeleteValue(HKCU, RunKey, OldRunValue);
      Log('Start with Windows kept under the new name.');
    end;
end;

function InitializeUninstall(): Boolean;
begin
  CloseInstalledApp;
  Result := True;
end;

{ The app's own "start with Windows" option writes HKCU Run\Shotoshot (Run\Shotshot
  before 0.2.0). Remove each only when it points to this or the former installation. }
procedure RemoveRunValue(Name: String);
var
  Command: String;
begin
  if RegQueryStringValue(HKCU, RunKey, Name, Command) then
    if (CompareText(RemoveQuotes(Command), InstalledExePath) = 0)
      or (CompareText(RemoveQuotes(Command), OldExePath) = 0) then
      RegDeleteValue(HKCU, RunKey, Name);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep <> usPostUninstall then
    Exit;
  RemoveRunValue(RunValue);
  RemoveRunValue(OldRunValue);
end;
