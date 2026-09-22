# Contributing

Thanks for helping! Bug reports, fixes, new cleanup rules and platform ports are all welcome.

## Setup

See *Development* in the [README](README.md). In short: Rust (MSVC), VS 2022 Build Tools, Node 20+, and libclang (`LIBCLANG_PATH`).

## Before sending a pull request

1. `cargo fmt --all` and `cargo clippy --workspace`.
2. `cargo test --workspace --exclude hushtype` — all tests must pass.
3. If you touch text cleanup (`crates/text`), add cases to `crates/text/tests/pipeline.rs`. Rules must be conservative: preserve meaning, never rewrite what the user said.
4. If you touch audio/transcription, run `cargo run --release -p hushtype-bench -- run` and include before/after numbers.
5. If you touch insertion or hotkeys, run `scripts\e2e.ps1` on Windows 10 or 11.
6. Keep the idle footprint low: no polling loops, no background timers, nothing allocated per frame in the UI.

## Adding dependencies

Every dependency costs startup time, binary size and maintenance. Prefer the standard library or the Windows API. New dependencies must be under a permissive license (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, MPL-2.0 unmodified); run `python scripts/licenses.py`, which fails on anything else.

## Porting to macOS / Linux

`crates/engine` and `crates/text` are portable. A port needs a `crates/platform` backend implementing the same API: global hotkey with press/release, text insertion, clipboard save/restore, focused-app detection, overlay, sounds and autostart.

## Code of conduct

Be respectful and constructive. Harassment of any kind is not tolerated.
