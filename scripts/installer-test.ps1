# Silent install -> launch -> silent uninstall check for the NSIS installer.
param([string]$Setup = "$PSScriptRoot\..\target\release\bundle\nsis\HushType_0.1.0_x64-setup.exe")
$ErrorActionPreference = 'Stop'
$silent = '/S'
Get-Process hushtype -ErrorAction SilentlyContinue | Stop-Process -Force
$p = Start-Process $Setup -ArgumentList $silent -Wait -PassThru
Write-Host "install exit code: $($p.ExitCode)"
$dir = "$env:LOCALAPPDATA\HushType"
$startMenu = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs"
$uninstKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\HushType'
Write-Host "installed exe: $(Test-Path "$dir\hushtype.exe"); uninstaller: $(Test-Path "$dir\uninstall.exe")"
Write-Host "start menu shortcuts: $(@(Get-ChildItem $startMenu -Recurse -Filter 'HushType*.lnk').Count)"
Write-Host "uninstall registry entry: $(Test-Path $uninstKey)"
Get-Process hushtype -ErrorAction SilentlyContinue | Stop-Process -Force

$psi = New-Object System.Diagnostics.ProcessStartInfo "$dir\hushtype.exe"
$psi.Arguments = '--autostart'; $psi.UseShellExecute = $false
$psi.EnvironmentVariables['HUSHTYPE_HOME'] = "$env:TEMP\hushtype-install-test"
$a = [System.Diagnostics.Process]::Start($psi)
Start-Sleep -Seconds 4
Write-Host "installed app running: $(-not $a.HasExited); working set $([math]::Round((Get-Process -Id $a.Id).WorkingSet64 / 1MB, 1)) MB"
Stop-Process -Id $a.Id -Force

$u = Start-Process "$dir\uninstall.exe" -ArgumentList $silent -Wait -PassThru
Start-Sleep -Seconds 3
Write-Host "uninstall exit code: $($u.ExitCode)"
Write-Host "exe removed: $(-not (Test-Path "$dir\hushtype.exe")); models kept: $(Test-Path "$dir\models\ggml-base.en-q5_1.bin")"
Write-Host "uninstall entry removed: $(-not (Test-Path $uninstKey)); shortcuts left: $(@(Get-ChildItem $startMenu -Recurse -Filter 'HushType*.lnk').Count)"
Write-Host "autostart entry left: $([bool](Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name HushType -ErrorAction SilentlyContinue))"
