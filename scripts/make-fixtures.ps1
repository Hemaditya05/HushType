# Generates spoken test fixtures with the offline Windows speech synthesizer
# (System.Speech). Used by the benchmark tool and the end-to-end tests.
# Output: tests/fixtures/*.wav (16 kHz, 16-bit, mono) + fixtures.json
param([string]$OutDir = "$PSScriptRoot\..\tests\fixtures")

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
New-Item -ItemType Directory -Force $OutDir | Out-Null

$fixtures = @(
    @{ id = 'short';    text = 'Create a function that fetches the user profile and returns the email address.' },
    @{ id = 'question'; text = 'Can you check whether the build passed on the main branch?' },
    @{ id = 'punct';    text = 'Hello comma how are you question mark' },
    @{ id = 'tech';     text = 'We deploy the TypeScript service to Kubernetes and store the data in PostgreSQL.' },
    @{ id = 'terminal'; text = 'git status' },
    @{ id = 'medium';   text = 'Tomorrow morning I will review the pull request, update the documentation, and then schedule a short meeting with the design team to go over the new onboarding flow.' },
    @{ id = 'long';     text = 'Voice typing works best when you speak in complete sentences. Take a short pause between ideas, and the application will add punctuation for you. Everything is processed locally on this computer, so no audio is ever uploaded to a server. If a word is recognised incorrectly, add it to the dictionary and it will be spelled correctly next time.' }
)

$format = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$voices = $synth.GetInstalledVoices() | Where-Object { $_.Enabled -and $_.VoiceInfo.Culture.Name -like 'en-*' }
if ($voices.Count -gt 0) { $synth.SelectVoice($voices[0].VoiceInfo.Name) }
$synth.Rate = 0

$manifest = @()
foreach ($f in $fixtures) {
    $path = Join-Path $OutDir "$($f.id).wav"
    $synth.SetOutputToWaveFile($path, $format)
    $synth.Speak($f.text)
    $synth.SetOutputToNull()
    $manifest += [pscustomobject]@{ id = $f.id; file = "$($f.id).wav"; text = $f.text }
    Write-Host "wrote $path"
}
$synth.Dispose()
$manifest | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $OutDir 'fixtures.json')
