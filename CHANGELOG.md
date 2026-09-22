# Changelog

All notable changes are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/) and the project uses [Semantic Versioning](https://semver.org/).

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
