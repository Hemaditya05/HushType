# Changelog

All notable changes are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/) and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- Recognition accuracy: timestamp tokens are no longer suppressed (suppressing them makes whisper.cpp drop audio and omit words, [whisper.cpp#2186](https://github.com/ggml-org/whisper.cpp/issues/2186)), final transcription uses beam search instead of greedy sampling, the encoder window never drops below 512 frames, and quiet segments are no longer discarded as "no speech".
- Audio is DC-corrected and gain-normalized before transcription, so quiet laptop microphones no longer lose words.
- Live previews are rate-limited by their own cost and run on half the cores over at most the last 10 s of speech, instead of re-transcribing the whole recording every 0.45 s. They no longer saturate the CPU during long dictations.
- Long speech is split into chunks at pauses from 6 s (was 12 s), so more of the work happens while you are still talking and less after you release the key.
- Short recordings are transcribed exactly as captured; voice-activity trimming now only applies above 12 s, where it saves real time.
- The microphone reader and the shortcut listener run above the transcription threads, so key presses and audio are not delayed by a decode in progress.
- Default model unload timer lowered to 5 minutes, and default microphone sensitivity raised slightly.

## [0.1.0] - 2026-09-22

### Added
- Local speech recognition with whisper.cpp (in-process, no Python), quantized Tiny/Base/Small/Medium models (English-only and multilingual), SHA-256-verified downloads with resume.
- Global shortcut (default Ctrl+Shift+Space) with push-to-talk, toggle and hybrid (hold or tap) modes; Esc cancels; conflict detection.
- Streaming dictation: live preview while speaking, chunked transcription of long dictations, adaptive encoder window for low latency.
- Energy-based voice activity detection with adaptive noise floor, auto-stop after silence in hands-free mode.
- Text cleanup: filler removal, spoken punctuation, stutter removal, number formatting, question detection, capitalization, custom dictionary with fuzzy matching, per-app formatting (terminal, code, chat).
- Text insertion via Unicode key events or clipboard paste with full clipboard restore; elevated-window detection.
- Native floating indicator, tray icon with recording state, start/stop sounds.
- Settings, microphone test, model manager, dictionary editor, history (search, copy, delete, clear), first-run setup.
- Launch at startup, idle model unloading (default 15 min), per-user NSIS installer.
- Benchmark CLI, end-to-end test harness, profiling script.
