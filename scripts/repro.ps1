# Quick stress check: back-to-back dictations via the hotkey (dry run).
param([string]$Exe = "$PSScriptRoot\..\target\release\hushtype.exe", [int]$N = 8, [switch]$Window)
. "$PSScriptRoot\e2e-lib.ps1"
New-TestHome
$null = Start-App (Resolve-Path $Exe).Path @{ HUSHTYPE_DRY_RUN = '1' }
if ($Window) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo (Resolve-Path $Exe).Path
    $psi.UseShellExecute = $false
    $psi.EnvironmentVariables['HUSHTYPE_HOME'] = $TestHome
    $null = [System.Diagnostics.Process]::Start($psi)
    Start-Sleep -Seconds 5
    $w = Get-Process | Where-Object { $_.MainWindowTitle -eq 'HushType' } | Select-Object -First 1
    if ($w) { $null = $w.CloseMainWindow(); Write-Host 'window opened and closed' }
    Start-Sleep -Seconds 3
}
$ok = 0
for ($i = 1; $i -le $N; $i++) { if (Invoke-Dictation 'terminal.wav' 20) { $ok++ } else { Write-Host "cycle $i timed out" } }
Write-Host "$ok/$N completed"
Get-Content "$TestHome\data\logs\hushtype.log" | Select-String 'hotkey|dictation:|WARN|ERROR' | Select-Object -Last 14 | ForEach-Object Line
Get-Process hushtype -ErrorAction SilentlyContinue | Stop-Process -Force
