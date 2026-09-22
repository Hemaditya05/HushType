# Resource profile of the real app: idle, settings window, recording,
# transcription, model loaded, after unload — plus a dictation leak test.
# Dictations are triggered with the real global shortcut and fed from a WAV
# file; HUSHTYPE_DRY_RUN skips the final insertion so nothing is typed into
# your windows while this runs.
#
#   powershell -ExecutionPolicy Bypass -File scripts\profile.ps1 [-Cycles 100] [-Out docs\profile-results.md]
param([string]$Exe = "$PSScriptRoot\..\target\release\hushtype.exe", [int]$Cycles = 100, [string]$Out = "$PSScriptRoot\..\docs\profile-results.md")
. "$PSScriptRoot\e2e-lib.ps1"
$Exe = (Resolve-Path $Exe).Path
$cores = [Environment]::ProcessorCount
$lines = @()
function Log($s) { Write-Host $s; $script:lines += $s }

function Sample([string]$label, [int]$cpuWindowSec = 0) {
    $a = App-Memory
    $cpu = ''
    if ($cpuWindowSec -gt 0) {
        Start-Sleep -Seconds $cpuWindowSec
        $b = App-Memory
        $cpu = '{0:N2}%' -f (100 * ($b.CpuSec - $a.CpuSec) / $cpuWindowSec / $cores)
        $a = $b
    }
    Log ("| {0} | {1} MB | {2} MB | {3} MB | {4} | {5} |" -f $label, $a.MainWsMB, $a.MainPrivateMB, $a.TotalWsMB, $a.Processes, $cpu)
}

New-TestHome @{ unloadAfterMin = 15 }
Get-Process hushtype -ErrorAction SilentlyContinue | Stop-Process -Force
$app = Start-App $Exe @{ HUSHTYPE_DRY_RUN = '1'; HUSHTYPE_UNLOAD_SECS = '30' }
Start-Sleep -Seconds 8

Log "## App resource profile"
Log ""
Log "Measured $(Get-Date -Format 'yyyy-MM-dd') with ``scripts/profile.ps1`` on $((Get-CimInstance Win32_Processor).Name), $cores logical cores, $([math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory/1GB)) GB RAM. Model: base.en (default). CPU % is of the whole machine."
Log ""
Log "| State | Main process working set | Main process private | All HushType processes (working set) | Processes | CPU |"
Log "|---|---|---|---|---|---|"
Sample 'Idle in tray (model not loaded), 60 s' 60

# Settings window open (second launch -> single-instance -> opens window).
$psi = New-Object System.Diagnostics.ProcessStartInfo $Exe
$psi.UseShellExecute = $false
$psi.EnvironmentVariables['HUSHTYPE_HOME'] = $TestHome
$null = [System.Diagnostics.Process]::Start($psi)
Start-Sleep -Seconds 6
Sample 'Settings window open' 5
$w = Get-Process | Where-Object { $_.MainWindowTitle -eq 'HushType' } | Select-Object -First 1
if ($w) { $null = $w.CloseMainWindow() }
Start-Sleep -Seconds 8
Sample 'Window closed again (WebView2 exited)' 10

# One dictation: sample during recording and after.
Copy-Item (Join-Path $Fixtures 'medium.wav') $TestAudio -Force
$before = History-Count
Tap-Hotkey
Start-Sleep -Seconds 4
Sample 'Recording (live preview on), 4 s window' 4
while ((History-Count) -eq $before) { Start-Sleep -Milliseconds 200 }
Start-Sleep -Seconds 2
Sample 'Idle, model loaded, 30 s' 20

# Leak test.
Log ""
Log "### Dictation leak test ($Cycles cycles)"
Log ""
$private = @()
$ok = 0
for ($i = 1; $i -le $Cycles; $i++) {
    if (Invoke-Dictation 'terminal.wav' 30) { $ok++ }
    $m = App-Memory
    $private += $m.MainPrivateMB
    if ($i % 10 -eq 0) { Write-Host "cycle $i : private $($m.MainPrivateMB) MB, working set $($m.MainWsMB) MB" }
}
$early = ($private[5..14] | Measure-Object -Average).Average
$late = ($private[($Cycles - 10)..($Cycles - 1)] | Measure-Object -Average).Average
Log ("{0}/{1} dictations completed. Private memory: cycles 6-15 average {2:N1} MB, last 10 average {3:N1} MB, change {4:+0.0;-0.0} MB. Max {5:N1} MB." -f $ok, $Cycles, $early, $late, ($late - $early), (($private | Measure-Object -Maximum).Maximum))
Log ""
Log "| State | Main process working set | Main process private | All HushType processes (working set) | Processes | CPU |"
Log "|---|---|---|---|---|---|"
Sample 'Right after the leak test' 0
Start-Sleep -Seconds 40
Sample 'After idle model unload (30 s timeout for this test), 60 s' 60

Get-Process hushtype -ErrorAction SilentlyContinue | Stop-Process -Force
$lines -join "`n" | Set-Content -Encoding utf8 $Out
Write-Host "wrote $Out"
