<#
.SYNOPSIS
    Installs z++ for the current user, or removes it. No admin rights needed.

.DESCRIPTION
    Copies zpp.exe to %LOCALAPPDATA%\Programs\z++ and registers it:
    a Start menu shortcut, "z++" in Win+R (App Paths), z++ in the "Open with" list for common
    text files, "Edit with z++" on the right-click menu, and an entry in Settings > Apps.
    Everything lives under HKCU and your profile, and -Uninstall removes all of it.
    Windows 11 does not let programs set themselves as the default app, so to open .txt files
    with z++ by default, use "Open with" > z++ > "Always" once.

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
$TextTypes = @('.txt', '.md', '.log', '.ini', '.cfg', '.conf', '.csv', '.json', '.xml', '.yml', '.yaml',
    '.toml', '.ps1', '.bat', '.cmd', '.py', '.rs', '.js', '.ts', '.css', '.html', '.sql', '.sh', '.gitignore')

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

function Register-App([string]$Version) {
    $open = "`"$InstalledExe`" `"%1`""
    Set-Value $AppKey 'FriendlyAppName' $AppName
    Set-Value "$AppKey\DefaultIcon" '' "`"$InstalledExe`",0"
    Set-Value "$AppKey\shell\open\command" '' $open
    foreach ($ext in $TextTypes) { Set-Value "$AppKey\SupportedTypes" $ext '' }

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
    New-StartMenuShortcut
    Update-Shell
    "Installed z++ $version to $InstallDir"
}

function Uninstall-Zpp {
    Assert-NotRunning
    foreach ($key in $AppKey, $AppPathsKey, $EditVerbKey, $UninstallKey) { Remove-Key $key }
    if (Test-Path -LiteralPath $Shortcut) { Remove-Item -LiteralPath $Shortcut -Force }
    Update-Shell
    # This script may be running from the install folder, so remove the folder after it exits.
    $cleanup = "Start-Sleep -Seconds 2; Remove-Item -LiteralPath '$InstallDir' -Recurse -Force"
    Start-Process pwsh.exe -ArgumentList '-NoProfile', '-Command', $cleanup -WindowStyle Hidden
    "Removed z++. Settings in HKCU\Software\z++ were kept."
}

if ($Uninstall) { Uninstall-Zpp } else { Install-Zpp }
