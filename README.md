# HushType

**Private, local voice typing for Windows.** Hold a shortcut, speak naturally, release — clean, punctuated text is typed into whatever app you're using: browsers, VS Code, terminals, Word, Discord, WhatsApp, Notepad…

- **Runs entirely on your PC.** Speech recognition uses [whisper.cpp](https://github.com/ggerganov/whisper.cpp) in-process. No audio or text is ever uploaded. No account, no telemetry. Works offline once a model is downloaded.
- **Small background footprint.** A native tray app: ~14 MB while idle (measured), and near-zero CPU. The microphone is off except while you're dictating.
- **Clean output.** Removes filler words, adds punctuation and capitalization, fixes stutters, applies your custom vocabulary, understands spoken punctuation ("comma", "question mark", "new line").
- **Context-aware.** In terminals it never adds a trailing period and never presses Enter; in chat apps line breaks become Shift+Enter so nothing is sent by accident.

Inspired by the experience of tools like Wispr Flow; an independent open-source implementation (MIT).

## Using it

| Action | How |
|---|---|
| Dictate (push-to-talk) | Hold **Ctrl+Shift+Space**, speak, release |
| Dictate hands-free | Tap **Ctrl+Shift+Space** once, speak; it stops after a pause (or tap again) |
| Cancel | **Esc** while recording |
| Settings, models, history | Click the tray icon |

A small pill near the bottom of the screen shows **● Listening…** with a live preview, then **Processing…**, then **✓ Inserted**. The shortcut, mode (hold / toggle / hybrid), indicator, sounds and cleanup rules are configurable.

Spoken punctuation: *comma, period / full stop, question mark, exclamation mark, colon, semicolon, open/close paren, open/close quote, new line, new paragraph*. Words used literally ("add a comma", "the trial period") are left alone.

## Supported systems

- Windows 10 (1809+) and Windows 11, x64.
- CPU with AVX2 (Intel Haswell 2013+ / AMD Zen or Excavator+). The app shows a clear message on older CPUs.
- WebView2 runtime (preinstalled on Windows 11 and current Windows 10; the installer fetches it otherwise) — only used by the settings window.
- 4 GB RAM minimum, 8 GB recommended.

## Install

Run `HushType_<version>_x64-setup.exe`. It installs per-user (no admin rights), creates Start-menu shortcuts and offers to launch HushType. The first-run screen downloads a speech model (60 MB for the default) and asks whether to start with Windows. Uninstall from *Settings › Apps*; tick "Delete application data" to also remove models, history and settings.

**Offline install:** copy a model file (see the table below) into `%LOCALAPPDATA%\HushType\models\` before first use.

## Models

Models are downloaded from the official whisper.cpp repository on Hugging Face and verified with SHA-256. Quantized versions are used: 2–3× less memory than the originals with negligible accuracy loss.

| Model | File | Download | RAM loaded (approx.) | Notes |
|---|---|---|---|---|
| Tiny (English) | `ggml-tiny.en-q5_1.bin` | 31 MB | ~90 MB | Fastest; for old PCs |
| **Base (English)** — default | `ggml-base.en-q5_1.bin` | 57 MB | ~150 MB | Best speed/accuracy balance on laptops |
| Small (English) | `ggml-small.en-q5_1.bin` | 181 MB | ~330 MB | More accurate, ~3× slower on CPU |
| Medium (English) | `ggml-medium.en-q5_0.bin` | 514 MB | ~800 MB | Needs a fast CPU or a GPU build |
| Tiny / Base / Small / Medium (multilingual) | `ggml-{size}-q5_1.bin` (`q5_0` for medium) | same | same | 99 languages, automatic language detection |

The default (`base.en`) was chosen from measurements on a 4-core i7-1165G7 laptop: about 0.75 s to transcribe a 5 s sentence on CPU, with 0 % word error rate on the test sentences. See [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

**Memory lifecycle:** the model loads when you first dictate (loading overlaps with you speaking), stays loaded while you use it, and is unloaded after 15 minutes of inactivity by default (*Never / 5 / 15 / 30 / 60 min*). "Load model at startup" is off by default.

**GPU:** the default build runs on the CPU, which is the fastest option on most laptops without a discrete GPU. whisper.cpp's CUDA and Vulkan backends can be enabled at build time (`--features cuda` or `--features vulkan`, with the CUDA toolkit / Vulkan SDK installed); the app then uses the GPU and falls back to the CPU if none is found.

## Privacy

See [PRIVACY.md](PRIVACY.md). In short: **no audio leaves the computer**, audio is processed in memory and discarded, there is no telemetry or analytics, and the only network request is the model download you start yourself. History (text only) is stored locally and can be disabled or cleared; storing audio is off by default.

## Architecture

```
apps/desktop/            Tauri 2 app: tray, commands, dictation state machine (Rust)
  src/                   Settings UI (React + TypeScript), loaded only while the window is open
crates/engine/           Audio capture (cpal/WASAPI), resampling, VAD, whisper.cpp worker,
                         streaming dictation sessions, model catalog + verified downloads
crates/text/             Transcript cleanup: fillers, spoken punctuation, stutters, numbers,
                         dictionary, questions, capitalization, per-app formatting (pure Rust)
crates/platform/         Windows integration: global hotkey, text insertion (SendInput),
                         clipboard save/restore, foreground app, native overlay, sounds, autostart
tools/bench/             Benchmark / diagnostics CLI
scripts/                 Fixtures, end-to-end test, profiling, icons, license report
tests/fixtures/          Generated speech clips (scripts/make-fixtures.ps1)
```

How a dictation flows:

1. **Hotkey** — `RegisterHotKey` on a dedicated thread (the combination never reaches the focused app; conflicts with other apps are reported). Release detection for push-to-talk polls the key state *only while the keys are held*.
2. **Capture** — the microphone opens only now (WASAPI shared mode via cpal). The device callback just down-mixes to mono into a bounded queue; the dictation thread resamples to 16 kHz and runs an energy-based **VAD** with an adaptive noise floor.
3. **Streaming transcription** — a single long-lived whisper.cpp worker thread owns the model. While you speak, previews of the current chunk are transcribed every ~0.5 s (with an encoder window sized to the audio, ~3× faster than the default 30 s window). Long dictations are split at pauses (≥12 s) and committed chunks are transcribed while you keep talking, so releasing the key only waits for the last few seconds. Final jobs pre-empt previews.
4. **Cleanup** — `crates/text` turns the raw transcript into final text using the settings and the app context.
5. **Insertion** — text is pasted and the previous clipboard contents (all formats) are restored, with the dictated text hidden from Win+V history; in terminals it is typed as Unicode key events instead (some editors, e.g. Win11 Notepad, scramble fast synthetic typing, so pasting is the default elsewhere). Typing into apps running as administrator is blocked by Windows — HushType detects this and puts the text on the clipboard instead.
6. The **indicator** is a native layered window drawn with GDI (no WebView), the **tray** is Tauri's, and the settings window is created on demand and destroyed on close so its WebView2 processes exit.

The engine and text crates are platform-independent; macOS/Linux support needs a new `crates/platform` backend (hotkey, insertion, overlay).

## Development

Prerequisites: Rust (stable, MSVC), Visual Studio 2022 Build Tools (C++ workload, includes CMake/Ninja), Node.js 20+, and **libclang** for generating the whisper.cpp bindings — either install LLVM (`winget install LLVM.LLVM`) or `pip install libclang`, then set `LIBCLANG_PATH` to the folder containing `libclang.dll`.

```powershell
cd apps\desktop
npm install
npx tauri dev          # debug build with hot-reloading UI
```

Tests:

```powershell
cargo test --workspace --exclude hushtype      # unit + integration tests (text rules, VAD, resampler, hotkeys)
powershell -File scripts\make-fixtures.ps1      # generate speech clips (Windows TTS, offline)
cargo run --release -p hushtype-bench -- transcribe tests\fixtures\short.wav
powershell -File scripts\e2e.ps1               # real hotkey -> Notepad / browser / VS Code / Terminal
powershell -File scripts\profile.ps1           # memory/CPU profile + dictation leak test
```

## Building the installer

```powershell
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"   # or wherever libclang.dll is
cd apps\desktop
npm install
npx tauri build
```

Output:
- Executable: `target\release\hushtype.exe`
- Installer: `target\release\bundle\nsis\HushType_0.1.0_x64-setup.exe`

The build compiles whisper.cpp with a portable AVX2 baseline and the static CRT (no VC++ redistributable needed).

## Troubleshooting

| Problem | Fix |
|---|---|
| "Windows is blocking microphone access" | *Settings › Privacy & security › Microphone* → enable *Let desktop apps access your microphone*. |
| Shortcut "already used by another application" | Pick another shortcut in *Settings › Keyboard shortcut*. |
| Nothing typed into an admin window (e.g. elevated terminal) | Windows forbids it; the text is on the clipboard — press Ctrl+V. Or run HushType as administrator. |
| Text appears in the wrong place | Text goes to whatever has keyboard focus when transcription finishes. |
| Recognition is inaccurate | Try the Small model, check the microphone level in Settings, add names to the Dictionary. |
| First dictation is slow | The model loads on first use (~0.3 s for Base). Enable *Load model at startup* to avoid it. |
| Logs | `%LOCALAPPDATA%\HushType\logs\hushtype.log` (never contains your speech or text). |

## Updates

There is no auto-updater (by design, for now): a self-updater needs code signing infrastructure. Install new versions over the old one; settings are kept. A signed Tauri updater is planned.

## License

MIT — see [LICENSE](LICENSE). Third-party components: [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
