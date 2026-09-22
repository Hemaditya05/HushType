# End-to-end test: real hotkey -> recording (file-fed audio) -> Whisper ->
# cleanup -> insertion into Notepad, Edge/Chrome, VS Code and Windows Terminal.
# Also checks that the clipboard survives (typing and paste modes).
#
#   powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1 [-Exe path\to\hushtype.exe]
#
# It takes over keyboard focus for about two minutes; don't type meanwhile.
# Every step verifies the target window is in the foreground before sending
# keys and fails the test instead of typing elsewhere.
param([string]$Exe = "$PSScriptRoot\..\target\release\hushtype.exe", [string]$Only = '')
. "$PSScriptRoot\e2e-lib.ps1"
$Exe = (Resolve-Path $Exe).Path
$work = Join-Path $env:TEMP 'hushtype-e2e-files'
New-Item -ItemType Directory -Force $work | Out-Null
$results = @()
$sentinel = "clipboard-sentinel-$(Get-Random)"

function Record($name, $ok, $detail) {
    $script:results += [pscustomobject]@{ Test = $name; Result = $(if ($ok) { 'PASS' } else { 'FAIL' }); Detail = $detail }
    Write-Host ("{0,-16} {1}  {2}" -f $name, $(if ($ok) { 'PASS' } else { 'FAIL' }), $detail)
}
function Norm($s) { ("$s" -replace '\s+', ' ').Trim() }
function Want($name) { -not $Only -or ($Only -split ',') -contains $name }

# Dictate into a file-backed editor, save with Ctrl+S, compare the file.
function Test-FileEditor($name, [scriptblock]$launch, $fixture, $expect, [scriptblock]$titleMatch) {
    $file = Join-Path $work "$name.txt"
    Set-Content -Path $file -Value '' -NoNewline
    Set-Clipboard -Value $sentinel
    & $launch $file
    $h = Wait-Window $titleMatch 30
    if ($h -eq [IntPtr]::Zero) { Record $name $false 'window did not appear / could not be focused'; return [IntPtr]::Zero }
    Start-Sleep -Seconds 1
    if (-not [E2E]::Focus($h)) { Record $name $false 'lost focus before dictation'; return $h }
    $done = Invoke-Dictation $fixture
    if (-not (Is-Focused $h)) { Record $name $false 'focus changed during the test (text may have gone elsewhere)'; return $h }
    Press-CtrlS; Start-Sleep -Seconds 1
    $text = Norm (Get-Content $file -Raw -Encoding UTF8)
    $clip = Get-Clipboard
    $ok = $done -and ($text -eq $expect) -and ($clip -eq $sentinel)
    Record $name $ok "got '$text' | clipboard intact: $($clip -eq $sentinel)"
    return $h
}

Write-Host "HushType end-to-end test using $Exe"
New-TestHome
$null = Start-App $Exe

if (Want 'notepad') {
    $null = Test-FileEditor 'notepad' { param($f) Start-Process notepad.exe $f } 'short.wav' `
        'Create a function that fetches the user profile and returns the email address.' { param($t) $t -like 'notepad.txt*' }
    Get-Process notepad -ErrorAction SilentlyContinue | Stop-Process -Force
}

if (Want 'browser') {
    $browser = @("$env:ProgramFiles\Google\Chrome\Application\chrome.exe", "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe",
        "$env:LOCALAPPDATA\Google\Chrome\Application\chrome.exe", "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe") |
        Where-Object { Test-Path $_ } | Select-Object -First 1
    $bname = if ($browser -like '*chrome*') { 'chrome' } else { 'edge' }
    Set-Clipboard -Value $sentinel
    # No spaces or quotes in the URL so it survives command-line parsing; the
    # page mirrors the textarea into the window title.
    $page = 'data:text/html,<title>e2e-ready</title><textarea%20autofocus%20oninput=document.title=this.value></textarea>'
    $profileDir = Join-Path $work 'browser-profile'
    # A separate profile = a separate browser instance; never the user's windows.
    $bp = Start-Process $browser -ArgumentList "--new-window --user-data-dir=`"$profileDir`" --no-first-run --no-default-browser-check $page" -PassThru
    $h = Wait-Window { param($t) $t -like 'e2e-ready*' } 30
    $isolated = $false
    if ($h -ne [IntPtr]::Zero) {
        $wpid = 0; [void][E2E]::GetWindowThreadProcessId($h, [ref]$wpid)
        $isolated = (Get-CimInstance Win32_Process -Filter "ProcessId=$wpid").CommandLine -like "*$profileDir*"
    }
    if ($h -ne [IntPtr]::Zero -and -not $isolated) {
        Record $bname $false 'browser reused an existing (user) instance; skipped to avoid typing into it'
    } elseif ($h -ne [IntPtr]::Zero) {
        Start-Sleep -Seconds 1
        $done = Invoke-Dictation 'question.wav'
        Start-Sleep -Seconds 1
        $title = [E2E]::Title($h)
        $got = if ($title -match '^(.*?) - (Google Chrome|.*Edge.*)$') { $matches[1] } else { $title }
        $clip = Get-Clipboard
        Record $bname ($done -and (Norm $got) -eq 'Can you check whether the build passed on the main branch?' -and $clip -eq $sentinel) "got '$(Norm $got)' | clipboard intact: $($clip -eq $sentinel)"
    } else { Record $bname $false 'browser window did not appear / could not be focused' }
    Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like "*$profileDir*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

if (Want 'vscode') {
    $code = "$env:LOCALAPPDATA\Programs\Microsoft VS Code\Code.exe"
    if (Test-Path $code) {
        $h = Test-FileEditor 'vscode' { param($f) Start-Process $code -ArgumentList @('--new-window', '--disable-extensions', $f) } 'tech.wav' `
            'We deploy the TypeScript service to Kubernetes and store the data in PostgreSQL.' { param($t) $t -like 'vscode.txt*' }
        if ($h -ne [IntPtr]::Zero -and [E2E]::Focus($h)) { [E2E]::Keys([uint16[]](0x11, 0x57)) } # Ctrl+W closes the test file
    } else { Record 'vscode' $false 'VS Code not installed' }
}

if (Want 'terminal') {
    $out = Join-Path $work 'terminal.txt'
    Remove-Item $out -ErrorAction SilentlyContinue
    Set-Content -Path "$work\read.ps1" -Value "`$host.UI.RawUI.WindowTitle = 'e2e-term'; `$x = Read-Host 'dictate'; Set-Content -Path '$out' -Value `$x; exit"
    Set-Clipboard -Value $sentinel
    Start-Process wt.exe -ArgumentList @('-w', 'new', '--title', 'e2e-term', '--suppressApplicationTitle', 'powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "$work\read.ps1")
    $h = Wait-Window { param($t) $t -like '*e2e-term*' } 30
    if ($h -ne [IntPtr]::Zero) {
        Start-Sleep -Seconds 2
        $done = Invoke-Dictation 'terminal.wav'
        if (Is-Focused $h) {
            Press-Enter; Start-Sleep -Seconds 2
            $got = if (Test-Path $out) { Norm (Get-Content $out -Raw) } else { '' }
            $clip = Get-Clipboard
            Record 'terminal' ($done -and $got -eq 'git status' -and $clip -eq $sentinel) "got '$got' (HushType pressed no Enter) | clipboard intact: $($clip -eq $sentinel)"
        } else { Record 'terminal' $false 'focus changed during the test' }
    } else { Record 'terminal' $false 'terminal window did not appear / could not be focused' }
}

if (Want 'paste') {
    Get-Process hushtype -ErrorAction SilentlyContinue | Stop-Process -Force
    New-TestHome @{ insertMethod = 'paste' }
    $null = Start-App $Exe
    $null = Test-FileEditor 'notepad-paste' { param($f) Start-Process notepad.exe $f } 'medium.wav' `
        'Tomorrow morning I will review the pull request, update the documentation, and then schedule a short meeting with the design team to go over the new onboarding flow.' { param($t) $t -like 'notepad-paste.txt*' }
    Get-Process notepad -ErrorAction SilentlyContinue | Stop-Process -Force
    $last = Last-History
    Record 'history-entry' ($last -and $last.app -eq 'Notepad' -and $last.raw -and $last.text) "app=$($last.app) raw+processed stored: $([bool]($last.raw -and $last.text))"
}

Get-Process hushtype -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Host ''
$failed = @($results | Where-Object Result -eq 'FAIL').Count
Write-Host "$($results.Count - $failed)/$($results.Count) passed"
exit $failed
