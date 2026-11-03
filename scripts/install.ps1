<#
.SYNOPSIS
    Installs z++ for the current user, or removes it. No admin rights needed.

.DESCRIPTION
    Copies zpp.exe to %LOCALAPPDATA%\Programs\z++ and registers it:
    a Start menu shortcut, "z++" in Win+R (App Paths), "Edit with z++" on the right-click menu,
    an entry in Settings > Apps, and a handler for every file type Windows Notepad and Notepad++
    open. z++ becomes the default for those types that have no default app yet. Windows 11 does
    not let an app take a type that already has a default (such as .txt from Notepad), so those
    are switched in Settings > Apps > Default apps > z++.
    Everything lives under HKCU and your profile, and -Uninstall removes all of it.

.EXAMPLE
    pwsh -File scripts\install.ps1                 # installs the release build for this PC's architecture
    pwsh -File scripts\install.ps1 -Exe path\zpp.exe
    pwsh -File "$env:LOCALAPPDATA\Programs\z++\uninstall.ps1" -Uninstall
#>
[CmdletBinding()]
param(
    [string]$Exe,
    [switch]$Uninstall
)
$ErrorActionPreference = 'Stop'

$AppName = 'z++'
$InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\z++'
$InstalledExe = Join-Path $InstallDir 'zpp.exe'
$Shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'z++.lnk'
# Paths under HKEY_CURRENT_USER. The .NET registry API is used because PowerShell's registry
# provider treats the "*" in the Edit with z++ key as a wildcard.
$AppKey = 'Software\Classes\Applications\zpp.exe'
$AppPathsKey = 'Software\Microsoft\Windows\CurrentVersion\App Paths\zpp.exe'
$UninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\z++'
$EditVerbKey = 'Software\Classes\*\shell\zpp'
$ProgId = 'zpp.file'
$ProgIdKey = "Software\Classes\$ProgId"
$CapabilitiesKey = 'Software\z++\Capabilities'
$InstallStateKey = 'Software\z++\Install'
$RegisteredAppsKey = 'Software\RegisteredApplications'

# Types Windows Notepad registers (its package manifest, Microsoft.WindowsNotepad 11.2607).
$NotepadTypes = @('.txt', '.log', '.ini', '.inf', '.ps1', '.psd1', '.psm1', '.scp', '.wtx')
# Types Notepad++ offers in its file association settings (PowerEditor/src/MISC/RegExt/regExtDlg.cpp).
$NotepadPlusPlusTypes = @(
    '.txt', '.log', '.ini', '.inf',
    '.h', '.hh', '.hpp', '.hxx', '.c', '.cpp', '.cxx', '.cc', '.m', '.mm',
    '.vcxproj', '.vcproj', '.props', '.vsprops', '.manifest',
    '.java', '.cs', '.pas', '.pp', '.inc',
    '.html', '.htm', '.shtml', '.shtm', '.hta', '.asp', '.aspx', '.css', '.js', '.json', '.mjs', '.jsm', '.jsp',
    '.php', '.php3', '.php4', '.php5', '.phps', '.phpt', '.phtml',
    '.xml', '.xhtml', '.xht', '.xul', '.kml', '.xaml', '.xsml',
    '.sh', '.bsh', '.bash', '.bat', '.cmd', '.nsi', '.nsh', '.lua', '.pl', '.pm', '.py',
    '.rc', '.as', '.mx', '.vb', '.vbs',
    '.f', '.for', '.f90', '.f95', '.f2k', '.tex', '.sql',
    '.nfo', '.mak')
$ExtraTypes = @('.md', '.cfg', '.conf', '.csv', '.yml', '.yaml', '.toml', '.rs', '.ts', '.gitignore')
$TextTypes = @($NotepadTypes + $NotepadPlusPlusTypes + $ExtraTypes | ForEach-Object { $_.ToLowerInvariant() } | Sort-Object -Unique)

# A release zip has zpp.exe next to this script; a repo checkout has it under target\.
function Get-DefaultExe {
    $beside = Join-Path $PSScriptRoot 'zpp.exe'
    if (Test-Path -LiteralPath $beside) { return $beside }
    $repo = Split-Path $PSScriptRoot -Parent
    $target = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
    Join-Path $repo "target\$target\release\zpp.exe"
}

function Assert-NotRunning {
    $running = Get-Process zpp -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $InstalledExe }
    if ($running) { throw "z++ is running from $InstallDir. Close it and run this again." }
}

# Name '' is the key's default value.
function Set-Value([string]$Key, [string]$Name, $Value, [string]$Type = 'String') {
    $k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($Key)
    try { $k.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]$Type) } finally { $k.Dispose() }
}

function Remove-Key([string]$Key) {
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($Key, $false)
}

function Get-Value([Microsoft.Win32.RegistryKey]$Root, [string]$Key, [string]$Name) {
    $k = $Root.OpenSubKey($Key)
    if (-not $k) { return $null }
    try { $k.GetValue($Name) } finally { $k.Dispose() }
}

function Remove-Value([string]$Key, [string]$Name) {
    $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($Key, $true)
    if (-not $k) { return }
    try { $k.DeleteValue($Name, $false) } finally { $k.Dispose() }
}

# True when nothing opens this type by default yet (or z++ already does), so z++ can take it
# without overriding anything. Types with a default belong to the user to change in Settings.
function Test-Claimable([string]$Ext) {
    $exts = "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$Ext"
    foreach ($choice in 'UserChoice', 'UserChoiceLatest') {
        if (Get-Value ([Microsoft.Win32.Registry]::CurrentUser) "$exts\$choice" 'ProgId') { return $false }
    }
    $current = Get-Value ([Microsoft.Win32.Registry]::ClassesRoot) $Ext ''
    -not $current -or $current -eq $ProgId
}

# Lets Windows offer z++ for every type: a ProgID, "Open with" entries, and the Capabilities
# that make z++ appear in Settings > Apps > Default apps. Returns the types z++ became the default for.
function Register-FileTypes {
    $open = "`"$InstalledExe`" `"%1`""
    Set-Value $ProgIdKey '' 'Text file (z++)'
    Set-Value $ProgIdKey 'FriendlyTypeName' 'Text file (z++)'
    Set-Value "$ProgIdKey\DefaultIcon" '' "`"$InstalledExe`",0"
    Set-Value "$ProgIdKey\shell\open\command" '' $open

    Set-Value $CapabilitiesKey 'ApplicationName' $AppName
    Set-Value $CapabilitiesKey 'ApplicationDescription' 'A small, fast plain text editor.'
    Set-Value $CapabilitiesKey 'ApplicationIcon' "`"$InstalledExe`",0"
    Set-Value $RegisteredAppsKey $AppName $CapabilitiesKey

    $claimed = @()
    foreach ($ext in $TextTypes) {
        Set-Value "$AppKey\SupportedTypes" $ext ''
        Set-Value "Software\Classes\$ext\OpenWithProgids" $ProgId ([byte[]]@()) 'None'
        Set-Value "$CapabilitiesKey\FileAssociations" $ext $ProgId
        if (Test-Claimable $ext) {
            Set-Value "Software\Classes\$ext" '' $ProgId
            $claimed += $ext
        }
    }
    Set-Value $InstallStateKey 'ClaimedTypes' ([string[]]$claimed) 'MultiString'
    $claimed
}

function Unregister-FileTypes {
    $claimed = @(Get-Value ([Microsoft.Win32.Registry]::CurrentUser) $InstallStateKey 'ClaimedTypes')
    foreach ($ext in $claimed) {
        if ($ext -and (Get-Value ([Microsoft.Win32.Registry]::CurrentUser) "Software\Classes\$ext" '') -eq $ProgId) {
            Remove-Value "Software\Classes\$ext" ''
        }
    }
    foreach ($ext in $TextTypes) { Remove-Value "Software\Classes\$ext\OpenWithProgids" $ProgId }
    Remove-Value $RegisteredAppsKey $AppName
    foreach ($key in $ProgIdKey, $CapabilitiesKey, $InstallStateKey) { Remove-Key $key }
}

function Register-App([string]$Version) {
    $open = "`"$InstalledExe`" `"%1`""
    Set-Value $AppKey 'FriendlyAppName' $AppName
    Set-Value "$AppKey\DefaultIcon" '' "`"$InstalledExe`",0"
    Set-Value "$AppKey\shell\open\command" '' $open

    Set-Value $AppPathsKey '' $InstalledExe

    Set-Value $EditVerbKey '' 'Edit with z++'
    Set-Value $EditVerbKey 'Icon' "`"$InstalledExe`",0"
    Set-Value "$EditVerbKey\command" '' $open

    $uninstall = "pwsh.exe -NoProfile -ExecutionPolicy Bypass -File `"$InstallDir\uninstall.ps1`" -Uninstall"
    Set-Value $UninstallKey 'DisplayName' $AppName
    Set-Value $UninstallKey 'DisplayVersion' $Version
    Set-Value $UninstallKey 'Publisher' 'Zain Naboulsi'
    Set-Value $UninstallKey 'DisplayIcon' "`"$InstalledExe`",0"
    Set-Value $UninstallKey 'InstallLocation' $InstallDir
    Set-Value $UninstallKey 'URLInfoAbout' 'https://github.com/suspicious-cow/z-plus-plus'
    Set-Value $UninstallKey 'UninstallString' $uninstall
    Set-Value $UninstallKey 'NoModify' 1 'DWord'
    Set-Value $UninstallKey 'NoRepair' 1 'DWord'
    $kb = [int]((Get-ChildItem $InstallDir -File | Measure-Object Length -Sum).Sum / 1KB)
    Set-Value $UninstallKey 'EstimatedSize' $kb 'DWord'
}

function New-StartMenuShortcut {
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($Shortcut)
    $link.TargetPath = $InstalledExe
    $link.WorkingDirectory = $InstallDir
    $link.IconLocation = "$InstalledExe,0"
    $link.Description = 'z++ text editor'
    $link.Save()
}

# Tells Explorer that file associations changed, so icons and menus refresh without a sign-out.
function Update-Shell {
    Add-Type -Namespace Zpp -Name Shell -MemberDefinition '[DllImport("shell32.dll")] public static extern void SHChangeNotify(int e, uint f, System.IntPtr a, System.IntPtr b);'
    [Zpp.Shell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
}

function Install-Zpp {
    if (-not $Exe) { $Exe = Get-DefaultExe }
    if (-not (Test-Path -LiteralPath $Exe)) { throw "No exe at $Exe. Build it first: cargo build --release --target <target>" }
    Assert-NotRunning
    New-Item -ItemType Directory -Force $InstallDir | Out-Null
    Copy-Item -LiteralPath $Exe $InstalledExe -Force
    Copy-Item -LiteralPath $PSCommandPath (Join-Path $InstallDir 'uninstall.ps1') -Force
    foreach ($doc in 'LICENSE', 'NOTICE') {
        $src = @((Join-Path $PSScriptRoot $doc), (Join-Path (Split-Path $PSScriptRoot -Parent) $doc)) |
            Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        if ($src) { Copy-Item -LiteralPath $src $InstallDir -Force }
    }
    $version = (Get-Item -LiteralPath $InstalledExe).VersionInfo.ProductVersion
    Register-App $version
    $claimed = @(Register-FileTypes)
    New-StartMenuShortcut
    Update-Shell
    "Installed z++ $version to $InstallDir"
    "z++ can open $($TextTypes.Count) file types. It is now the default for the $($claimed.Count) that had no default app."
    "To switch the rest (such as .txt from Notepad): Settings > Apps > Default apps > z++, or run:"
    '    start ms-settings:defaultapps?registeredAppUser=z++'
}

function Uninstall-Zpp {
    Assert-NotRunning
    Unregister-FileTypes
    foreach ($key in $AppKey, $AppPathsKey, $EditVerbKey, $UninstallKey) { Remove-Key $key }
    if (Test-Path -LiteralPath $Shortcut) { Remove-Item -LiteralPath $Shortcut -Force }
    Update-Shell
    # This script may be running from the install folder, so remove the folder after it exits.
    $cleanup = "Start-Sleep -Seconds 2; Remove-Item -LiteralPath '$InstallDir' -Recurse -Force"
    Start-Process pwsh.exe -ArgumentList '-NoProfile', '-Command', $cleanup -WindowStyle Hidden
    "Removed z++. Settings in HKCU\Software\z++ were kept."
}

if ($Uninstall) { Uninstall-Zpp } else { Install-Zpp }
