; Shotoshot - per-user Windows installer (Inno Setup 6).
; Build: powershell -NoProfile -ExecutionPolicy Bypass -File packaging\build-installer.ps1
; Test:  powershell -NoProfile -ExecutionPolicy Bypass -File packaging\test-installer.ps1
; RULE: keep AppVersion equal to [workspace.package] version in Cargo.toml (checked by the build script).

#define AppVersion "0.3.24"
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
; Same wording as the version resource of the application itself.
AppCopyright=Copyright (C) 2026 {#AppPublisher}
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
; Nothing left to say at the end: the app is already starting ([Run]).
DisableFinishedPage=yes
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
; Lists what was really packed: build-installer.ps1 reads it to refuse an incomplete setup.
OutputManifestFile=Shotoshot-Setup-{#AppVersion}.manifest.txt
; Restart Manager stays as a fallback; the [Code] section closes the app first.
CloseApplications=yes
RestartApplications=no
; The [Registry] section associates .shoto files: Explorer must be told.
ChangesAssociations=yes
; The wizard speaks the language of Windows (English when Inno has no translation);
; the language of Shotoshot is asked on a page of its own ([Code]).
ShowLanguageDialog=no
LanguageDetectionMethod=uilanguage
; No AppMutex: development builds share the app's mutex names, so it would
; report a dev instance as "running" and block install/uninstall.

; The translations of Inno Setup 6 for the languages of Shotoshot.
[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
; Inno ships no official Chinese: contributed translations listed on jrsoftware.org/files/istrans,
; taken from github.com/jrsoftware/issrc (Files/Languages, commit 6ef32198, 2026-10-22).
Name: "chinesesimplified"; MessagesFile: "languages\ChineseSimplified.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "polish"; MessagesFile: "compiler:Languages\Polish.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "chinesetraditional"; MessagesFile: "languages\ChineseTraditional.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "ukrainian"; MessagesFile: "compiler:Languages\Ukrainian.isl"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "czech"; MessagesFile: "compiler:Languages\Czech.isl"

; Our own texts in each of these languages, and the choices of the language page. Generated
; from crates/shotoshot-text/locales by `cargo run -p shotoshot-text --example inno`.
#include "languages.isi"

[Files]
Source: "..\target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
; GPL: the license and the pointer to the source code travel with the binary.
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "ABOUT.txt"; DestDir: "{app}"; Flags: ignoreversion

; The pre-0.2.0 install becomes this one: same AppId, so its uninstall entry is replaced.
[InstallDelete]
Type: filesandordirs; Name: "{autopf}\{#OldAppName}"
Type: files; Name: "{autoprograms}\{#OldAppName}.lnk"
Type: files; Name: "{autodesktop}\{#OldAppName}.lnk"

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
; No desktop shortcut is offered (Mickael, 2026-10-15: no task page, the Start menu is enough).
; Only a pre-0.2.0 "Shot Shot" desktop shortcut is carried over under the new name.
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Check: KeepsOldDesktopLink

; A double-click on an editable ".shoto" image reopens it in Shotoshot. Per-user keys, removed
; with the application. The file is a PNG, so it is declared as an image.
; Thumbnails: the perceived type alone gives none (0x8004B200, probed 2026-10-10). Binding the
; extension to Windows' own Photo Thumbnail Provider (shell extension category
; {E357FCCD-...}, provider {C7657C4A-...}) makes Explorer and the desktop show the capture
; itself, decoded as the PNG it is. The TypeOverlay value makes Explorer stamp the Shotoshot
; icon on that thumbnail, as it does for documents. Nothing of ours is loaded in Explorer;
; packaging/test-thumbnail.ps1 proves the thumbnail after an installation.
[Registry]
Root: HKCU; Subkey: "Software\Classes\.shoto"; ValueType: string; ValueName: ""; ValueData: "Shotoshot.Document"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\.shoto"; ValueType: string; ValueName: "Content Type"; ValueData: "image/png"
Root: HKCU; Subkey: "Software\Classes\.shoto"; ValueType: string; ValueName: "PerceivedType"; ValueData: "image"
Root: HKCU; Subkey: "Software\Classes\.shoto\ShellEx\{{E357FCCD-A995-4576-B01F-234630154E96}"; ValueType: string; ValueName: ""; ValueData: "{{C7657C4A-9F68-40fa-A4DF-96BC08EB3551}"
Root: HKCU; Subkey: "Software\Classes\Shotoshot.Document"; ValueType: string; ValueName: ""; ValueData: "{cm:ShotoFile}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Shotoshot.Document"; ValueType: string; ValueName: "TypeOverlay"; ValueData: "{app}\{#AppExeName},0"
Root: HKCU; Subkey: "Software\Classes\Shotoshot.Document\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKCU; Subkey: "Software\Classes\Shotoshot.Document\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""

[Run]
; Shotoshot starts by itself once installed, with no "Open Shotoshot" box (Mickael, 2026-10-17).
; Not postinstall, so it runs as soon as the files are in place; runasoriginaluser keeps it
; unelevated even when setup was run as administrator. Silent installs launch nothing
; (test-installer.ps1, delivery step 6).
; The language chosen on our page is handed over just before (WriteLanguageChoice).
Filename: "{app}\{#AppExeName}"; Flags: nowait runasoriginaluser skipifsilent; BeforeInstall: WriteLanguageChoice

; Gives back to Windows what Shotoshot took, before the files are removed; the app owns the logic.
; - Print Screen goes back to the Snipping Tool if Shotoshot took it
;   (crates/shotoshot-app/src/print_screen.rs).
; - A desktop that still shows the Board gets its own wallpaper back; the notes stay on disk
;   (crates/shotoshot-app/src/board/bake.rs).
; Each is a no-op otherwise. The app's tests keep this list equal to the restores it offers.
[UninstallRun]
Filename: "{app}\{#AppExeName}"; Parameters: "--restore-print-screen"; Flags: runhidden waituntilterminated; RunOnceId: "RestorePrintScreen"
Filename: "{app}\{#AppExeName}"; Parameters: "--restore-wallpaper"; Flags: runhidden waituntilterminated; RunOnceId: "RestoreWallpaper"

[Code]
const
  WM_CLOSE = $0010;
  AppWindowClass = 'Shotoshot.Window';
  OldWindowClass = 'Shotshot.Window';
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  RunValue = 'Shotoshot';
  OldRunValue = 'Shotshot';
  { The settings formats whose language is line SettingsLanguageLine (0-based, header
    included) of settings.txt, comma-separated. The app's tests keep both equal to what
    Settings::encode writes (crates/shotoshot-core/src/settings.rs). }
  SettingsHeader = 'shotoshot-settings-';
  KnownSettingsVersions = '16,17';
  SettingsLanguageLine = 28;
  { Read, then deleted, by the app when it starts (crates/shotoshot-app/src/language.rs). }
  LanguageChoiceFile = 'installer-language.txt';

var
  HadOldDesktopLink: Boolean;
  LanguagePage: TInputOptionWizardPage;
  LanguagePageShown: Boolean;

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

function DataFolder: String;
begin
  Result := ExpandConstant('{localappdata}\{#AppName}');
end;

{ The language stored by the installed app: 'auto' or a language tag. 'auto' when there are
  no settings yet or they are in a format this setup does not know. }
function CurrentLanguage(): String;
var
  Lines: TArrayOfString;
  Version: String;
begin
  Result := 'auto';
  if not LoadStringsFromFile(DataFolder + '\settings.txt', Lines) then
    Exit;
  if GetArrayLength(Lines) <= SettingsLanguageLine then
    Exit;
  if Copy(Lines[0], 1, Length(SettingsHeader)) <> SettingsHeader then
    Exit;
  Version := Copy(Lines[0], Length(SettingsHeader) + 1, Length(Lines[0]));
  if Pos(',' + Version + ',', ',' + KnownSettingsVersions + ',') = 0 then
    Exit;
  Result := Trim(Lines[SettingsLanguageLine]);
end;

function ChoiceCount: Integer;
begin
  Result := StrToIntDef(CustomMessage('ChoiceCount'), 1);
end;

{ "Windows language", then each language under its own name, on the choice the app has
  now: an update validated as it is changes nothing. }
procedure InitializeWizard;
var
  I: Integer;
  Current: String;
begin
  LanguagePage := CreateInputOptionPage(wpWelcome, CustomMessage('LanguagePageTitle'),
    CustomMessage('LanguagePageDescription'), '', True, True);
  LanguagePage.Add(CustomMessage('LanguageAuto'));
  for I := 1 to ChoiceCount - 1 do
    LanguagePage.Add(CustomMessage('ChoiceName' + IntToStr(I)));
  LanguagePage.SelectedValueIndex := 0;
  Current := CurrentLanguage();
  for I := 1 to ChoiceCount - 1 do
    if CompareText(CustomMessage('ChoiceTag' + IntToStr(I)), Current) = 0 then
      LanguagePage.SelectedValueIndex := I;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = LanguagePage.ID then
    LanguagePageShown := True;
end;

{ Runs just before the app is started ([Run]), so the app finds it at once. A silent install
  shows no page and hands over nothing: the app keeps its own choice. }
procedure WriteLanguageChoice;
var
  Choice: String;
begin
  if not LanguagePageShown then
    Exit;
  Choice := CustomMessage('ChoiceTag' + IntToStr(LanguagePage.SelectedValueIndex));
  if ForceDirectories(DataFolder)
    and SaveStringToFile(DataFolder + '\' + LanguageChoiceFile, Choice, False) then
    Log('Language handed to the app: ' + Choice)
  else
    Log('Could not hand the language to the app: ' + Choice);
end;

{ Read before [InstallDelete] removes the old shortcut. }
function KeepsOldDesktopLink: Boolean;
begin
  Result := HadOldDesktopLink;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  HadOldDesktopLink := FileExists(ExpandConstant('{autodesktop}\{#OldAppName}.lnk'));
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
