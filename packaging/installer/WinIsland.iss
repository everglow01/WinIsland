#ifndef Channel
  #define Channel "stable"
#endif

#ifndef SourceDirectory
  #error "SourceDirectory must be provided."
#endif

#ifndef OutputDirectory
  #error "OutputDirectory must be provided."
#endif

#ifndef Version
  #error "Version must be provided."
#endif

#define AppName "WinIsland"
#define AppId "{{AA737034-4733-47BF-A8D4-3D7DB5B986D5}"
#define InstallDirectory "WinIsland"
#define IdentityPackageName "Eatgrapes.WinIsland"
#define IdentityPackageFile "Eatgrapes.WinIsland.msix"
#define IdentityCertificateFile "Eatgrapes.WinIsland.cer"

#if Channel == "nightly"
  #define OutputFile "WinIsland-Nightly-Setup"
#else
  #define OutputFile "WinIsland-Setup"
#endif

[Setup]
AppId={#AppId}
AppName={#AppName}
AppVersion={#Version}
AppPublisher=Eatgrapes
DefaultDirName={localappdata}\{#InstallDirectory}
DisableProgramGroupPage=yes
OutputDir={#OutputDirectory}
OutputBaseFilename={#OutputFile}
SetupIconFile={#SourceDirectory}\resources\icon-dark.ico
UninstallDisplayIcon={app}\WinIsland.exe
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
RestartApplications=no
SetupLogging=yes

[Files]
Source: "{#SourceDirectory}\WinIsland.exe"; DestDir: "{app}"; Flags: ignoreversion restartreplace
Source: "{#SourceDirectory}\sherpa-onnx-c-api.dll"; DestDir: "{app}"; Flags: ignoreversion restartreplace skipifsourcedoesntexist
Source: "{#SourceDirectory}\onnxruntime.dll"; DestDir: "{app}"; Flags: ignoreversion restartreplace skipifsourcedoesntexist
Source: "{#SourceDirectory}\onnxruntime_providers_shared.dll"; DestDir: "{app}"; Flags: ignoreversion restartreplace skipifsourcedoesntexist
Source: "{#SourceDirectory}\resources\icon-dark.png"; DestDir: "{app}\resources"; Flags: ignoreversion
Source: "{#SourceDirectory}\resources\icon-dark.ico"; DestDir: "{app}\resources"; Flags: ignoreversion
Source: "{#SourceDirectory}\resources\licenses\*"; DestDir: "{app}\resources\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#SourceDirectory}\identity\{#IdentityPackageFile}"; DestDir: "{app}\identity"; Flags: ignoreversion
Source: "{#SourceDirectory}\identity\{#IdentityCertificateFile}"; DestDir: "{app}\identity"; Flags: ignoreversion
Source: "{#SourceDirectory}\identity\Install-Identity.ps1"; DestDir: "{app}\identity"; Flags: ignoreversion
Source: "{#SourceDirectory}\identity\Remove-Identity.ps1"; DestDir: "{app}\identity"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\WinIsland.exe"

[Run]
Filename: "{app}\WinIsland.exe"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent; Check: IdentityInstallationSucceeded

[UninstallRun]
Filename: "{sys}\WindowsPowerShell\v1.0\powershell.exe"; Parameters: "{code:RemoveIdentityParameters}"; WorkingDir: "{app}"; Flags: runhidden waituntilterminated logoutput; RunOnceId: "Remove{#IdentityPackageName}"

[Code]
var
  IdentityInstalled: Boolean;

function PowerShellLiteral(Value: String): String;
begin
  StringChangeEx(Value, '''', '''''', True);
  Result := '''' + Value + '''';
end;

function IdentityCommand(ScriptName: String): String;
begin
  Result := '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass ' +
    '-Command "$env:PSModulePath = $PSHOME + ''\Modules''; ' +
    '$ErrorActionPreference = ''Stop''; & ' +
    PowerShellLiteral(ExpandConstant('{app}\identity\') + ScriptName);
end;

function RemoveIdentityParameters(Param: String): String;
begin
  Result := IdentityCommand('Remove-Identity.ps1') +
    ' -PackageName ' + PowerShellLiteral('{#IdentityPackageName}') +
    '; exit $LASTEXITCODE"';
end;

function IdentityInstallationSucceeded: Boolean;
begin
  Result := IdentityInstalled;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Parameters: String;
  ErrorMessage: String;
  ResultCode: Integer;
begin
  if CurStep <> ssPostInstall then
    exit;

  IdentityInstalled := False;
  WizardForm.StatusLabel.Caption := 'Registering Windows app identity...';
  Parameters := IdentityCommand('Install-Identity.ps1') +
    ' -PackagePath ' + PowerShellLiteral(ExpandConstant('{app}\identity\{#IdentityPackageFile}')) +
    ' -CertificatePath ' + PowerShellLiteral(ExpandConstant('{app}\identity\{#IdentityCertificateFile}')) +
    ' -ExternalLocation ' + PowerShellLiteral(ExpandConstant('{app}')) +
    ' -PackageName ' + PowerShellLiteral('{#IdentityPackageName}') +
    '; exit $LASTEXITCODE"';

  try
    if not ExecAndLogOutput(
      ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
      Parameters, ExpandConstant('{app}'), SW_SHOWNORMAL,
      ewWaitUntilTerminated, ResultCode, nil) then
      ErrorMessage := SysErrorMessage(ResultCode)
    else if ResultCode <> 0 then
      ErrorMessage := 'Exit code: ' + IntToStr(ResultCode)
    else
      IdentityInstalled := True;
  except
    ErrorMessage := GetExceptionMessage;
  end;

  if not IdentityInstalled then
  begin
    ErrorMessage := 'Windows app registration failed. ' + ErrorMessage +
      '. Run Setup again to repair the installation. See the Setup log for details.';
    Log(ErrorMessage);
    SuppressibleMsgBox(ErrorMessage, mbCriticalError, MB_OK, IDOK);
  end;
end;

function GetCustomSetupExitCode: Integer;
begin
  if IdentityInstalled then
    Result := 0
  else
    Result := 1;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if (CurPageID = wpFinished) and not IdentityInstalled then
  begin
    WizardForm.FinishedHeadingLabel.Caption := 'Windows app registration failed';
    WizardForm.FinishedLabel.Caption :=
      'WinIsland could not finish installing. Run Setup again to repair the installation. ' +
      'See the Setup log for details.';
  end;
end;

#if Channel == "nightly"
const
  LegacyNightlyUninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{B835C9EA-88D3-4CF3-9837-324EEDC65BD0}_is1';

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  UninstallCommand: String;
  ResultCode: Integer;
begin
  Result := '';
  if not RegQueryStringValue(HKEY_CURRENT_USER_64, LegacyNightlyUninstallKey,
    'QuietUninstallString', UninstallCommand) then
  begin
    RegQueryStringValue(HKEY_CURRENT_USER_64, LegacyNightlyUninstallKey,
      'UninstallString', UninstallCommand);
  end;

  if UninstallCommand = '' then
    exit;

  if not Exec('>', UninstallCommand + ' /VERYSILENT /SUPPRESSMSGBOXES /NORESTART',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
  begin
    Result := 'The previous WinIsland Nightly installation could not be removed: ' +
      SysErrorMessage(ResultCode);
  end
  else if ResultCode <> 0 then
  begin
    Result := 'The previous WinIsland Nightly installation could not be removed (exit code ' +
      IntToStr(ResultCode) + ').';
  end;
end;
#endif
