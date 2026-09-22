# Shared helpers for the end-to-end and profiling scripts.
# Drives the real app: presses the global shortcut with SendInput and feeds
# a WAV file instead of the microphone (HUSHTYPE_TEST_AUDIO).

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class E2E {
    [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr extra; }
    [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public KEYBDINPUT ki; public long pad; }
    [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] inputs, int size);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    static INPUT K(ushort vk, bool up) { var i = new INPUT(); i.type = 1; i.ki.wVk = vk; i.ki.dwFlags = up ? 2u : 0u; return i; }
    public static void Keys(ushort[] down) {
        var list = new System.Collections.Generic.List<INPUT>();
        foreach (var k in down) list.Add(K(k, false));
        for (int j = down.Length - 1; j >= 0; j--) list.Add(K(down[j], true));
        SendInput((uint)list.Count, list.ToArray(), Marshal.SizeOf(typeof(INPUT)));
    }
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint a, uint b, bool attach);
    [DllImport("user32.dll")] static extern bool BringWindowToTop(IntPtr h);
    [DllImport("user32.dll")] static extern bool IsIconic(IntPtr h);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    public static string Title(IntPtr h) { var sb = new StringBuilder(1024); GetWindowText(h, sb, 1024); return sb.ToString(); }
    // Bring a window to the foreground despite the foreground lock by briefly
    // attaching to the current foreground thread's input queue.
    public static bool Focus(IntPtr h) {
        for (int i = 0; i < 10; i++) {
            if (GetForegroundWindow() == h) return true;
            uint pid;
            uint fgThread = GetWindowThreadProcessId(GetForegroundWindow(), out pid);
            uint me = GetCurrentThreadId();
            AttachThreadInput(me, fgThread, true);
            if (IsIconic(h)) ShowWindow(h, 9);
            BringWindowToTop(h);
            SetForegroundWindow(h);
            AttachThreadInput(me, fgThread, false);
            System.Threading.Thread.Sleep(150);
        }
        return GetForegroundWindow() == h;
    }
}
"@

$Root = Resolve-Path "$PSScriptRoot\.."
$Fixtures = Join-Path $Root 'tests\fixtures'
$TestHome = Join-Path $env:TEMP 'hushtype-e2e'
$TestAudio = Join-Path $TestHome 'test.wav'

function Tap-Hotkey { [E2E]::Keys([uint16[]](0x11, 0x10, 0x20)) }   # Ctrl+Shift+Space
function Press-Enter { [E2E]::Keys([uint16[]](0x0D)) }
function Press-CtrlS { [E2E]::Keys([uint16[]](0x11, 0x53)) }

function New-TestHome([hashtable]$Overrides = @{}) {
    if (Test-Path $TestHome) { Remove-Item -Recurse -Force $TestHome }
    New-Item -ItemType Directory -Force "$TestHome\config" | Out-Null
    $s = @{
        onboarded = $true; startMinimized = $true; launchAtStartup = $false; showIndicator = $true;
        hotkey = 'Ctrl+Shift+Space'; hotkeyMode = 'hybrid'; silenceTimeoutMs = 1000; saveHistory = $true;
        playSounds = $false; insertMethod = 'auto'; restoreClipboard = $true; livePreview = $true; unloadAfterMin = 15
    }
    foreach ($k in $Overrides.Keys) { $s[$k] = $Overrides[$k] }
    $s | ConvertTo-Json | Set-Content -Encoding ascii "$TestHome\config\settings.json"
}

function Start-App([string]$Exe, [hashtable]$Env = @{}) {
    Get-Process hushtype -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 500
    $psi = New-Object System.Diagnostics.ProcessStartInfo $Exe
    $psi.Arguments = '--autostart'
    $psi.UseShellExecute = $false
    $psi.EnvironmentVariables['HUSHTYPE_HOME'] = $TestHome
    $psi.EnvironmentVariables['HUSHTYPE_TEST_AUDIO'] = $TestAudio
    foreach ($k in $Env.Keys) { $psi.EnvironmentVariables[$k] = $Env[$k] }
    $p = [System.Diagnostics.Process]::Start($psi)
    Start-Sleep -Seconds 2
    return $p
}

function History-Count {
    $f = "$TestHome\data\history.jsonl"
    if (Test-Path $f) { return @(Get-Content $f).Count } else { return 0 }
}

# Trigger one hands-free dictation of $Fixture and wait for it to finish.
function Invoke-Dictation([string]$Fixture, [int]$TimeoutSec = 40) {
    Copy-Item (Join-Path $Fixtures $Fixture) $TestAudio -Force
    $before = History-Count
    Tap-Hotkey
    $t0 = Get-Date
    while (((Get-Date) - $t0).TotalSeconds -lt $TimeoutSec) {
        Start-Sleep -Milliseconds 250
        if ((History-Count) -gt $before) { Start-Sleep -Milliseconds 600; return $true }
    }
    return $false
}

function Last-History {
    $f = "$TestHome\data\history.jsonl"
    if (-not (Test-Path $f)) { return $null }
    return (Get-Content $f | Select-Object -Last 1 | ConvertFrom-Json)
}

# Find a top-level window whose title matches, then focus it. Returns the
# handle only if it is verifiably in the foreground (so the test never types
# into some other window).
function Wait-Window([scriptblock]$Match, [int]$TimeoutSec = 20) {
    $t0 = Get-Date
    while (((Get-Date) - $t0).TotalSeconds -lt $TimeoutSec) {
        $p = Get-Process | Where-Object { $_.MainWindowHandle -ne 0 -and (& $Match $_.MainWindowTitle) } | Select-Object -First 1
        if ($p) {
            Start-Sleep -Milliseconds 800
            if ([E2E]::Focus($p.MainWindowHandle)) { return $p.MainWindowHandle }
        }
        Start-Sleep -Milliseconds 300
    }
    return [IntPtr]::Zero
}

function Is-Focused([IntPtr]$h) { return [E2E]::GetForegroundWindow() -eq $h }

# Memory of the app: main process + any WebView2 children.
function App-Memory {
    $main = Get-Process hushtype -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $main) { return $null }
    $kids = Get-CimInstance Win32_Process -Filter "ParentProcessId=$($main.Id)" | ForEach-Object { $_.ProcessId }
    $desc = @()
    foreach ($k in $kids) { $desc += $k; $desc += (Get-CimInstance Win32_Process -Filter "ParentProcessId=$k" | ForEach-Object { $_.ProcessId }) }
    $all = @($main) + @($desc | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
    [pscustomobject]@{
        MainWsMB      = [math]::Round($main.WorkingSet64 / 1MB, 1)
        MainPrivateMB = [math]::Round($main.PrivateMemorySize64 / 1MB, 1)
        TotalWsMB     = [math]::Round((($all | Measure-Object WorkingSet64 -Sum).Sum) / 1MB, 1)
        Processes     = $all.Count
        CpuSec        = $main.TotalProcessorTime.TotalSeconds
        Threads       = $main.Threads.Count
    }
}
