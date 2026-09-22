# Benchmarks

All numbers below were measured on the development machine (Intel i7-1165G7 laptop, 4 cores / 8 threads, 16 GB RAM, Intel Iris Xe, no discrete GPU; Windows 11) with the release build. Nothing is estimated. GPU usage: none (CPU build; the iGPU is not used). Reproduce with `hushtype-bench run` and `scripts/profile.ps1`.

## Summary

- Idle in tray: **13.6 MB** working set, **0.00% CPU**, 1 process. Microphone off.
- Settings window: WebView2 adds ~465 MB across 6 helper processes only while open; they exit when it closes.
- Default model (base.en) loaded: ~200 MB resident (working set). Committed (private) memory is ~720 MB because whisper.cpp reserves compute buffers it only partly touches.
- After idle unload: back to **6 MB** private.
- Leak test: 100 hotkey-triggered dictations; committed memory plateaued (max 726.8 MB, last-10 average 720.2 MB). 99/100 completed; the one miss was a synthetic hotkey tap that Windows did not deliver (no "hotkey Pressed" in the log), not an app failure.
- Typical latency with base.en: text ready ~0.8-1.4 s after you stop speaking; microphone opens in ~50 ms (first open ~110 ms).
- CPU while recording with live preview: ~46% of the machine (4 inference threads). Turn off *Live preview* to reduce it.

## App resource profile

Measured 2026-09-22 with `scripts/profile.ps1` on 11th Gen Intel(R) Core(TM) i7-1165G7 @ 2.80GHz, 8 logical cores, 16 GB RAM. Model: base.en (default). CPU % is of the whole machine.

| State | Main process working set | Main process private | All HushType processes (working set) | Processes | CPU |
|---|---|---|---|---|---|
| Idle in tray (model not loaded), 60 s | 13.6 MB | 2.9 MB | 13.6 MB | 1 | 0.00% |
| Settings window open | 26.2 MB | 4.9 MB | 478.5 MB | 7 | 0.00% |
| Window closed again (WebView2 exited) | 2.1 MB | 4.9 MB | 2.1 MB | 1 | 0.00% |
| Recording (live preview on), 4 s window | 187.9 MB | 724.8 MB | 187.9 MB | 1 | 45.90% |
| Idle, model loaded, 30 s | 203.1 MB | 726.8 MB | 203.1 MB | 1 | 0.00% |

### Dictation leak test (100 cycles)

99/100 dictations completed. Private memory: cycles 6-15 average 651.4 MB, last 10 average 720.2 MB, change +68.8 MB. Max 726.8 MB.

| State | Main process working set | Main process private | All HushType processes (working set) | Processes | CPU |
|---|---|---|---|---|---|
| Right after the leak test | 174.7 MB | 720.2 MB | 174.7 MB | 1 |  |
| After idle model unload (30 s timeout for this test), 60 s | 2.8 MB | 6.3 MB | 2.8 MB | 1 | 0.00% |


## Engine benchmark

Machine: 8 logical cores, AVX2 true, AVX-512 true, 15.8 GB RAM. Backend: CPU (no GPU backend compiled). Inference threads: 4.

Measured with `hushtype-bench run` (release build). Audio: synthesized speech from `scripts/make-fixtures.ps1` (Windows SAPI voice). WER is computed on the raw recogniser output against the script text, ignoring case and punctuation.

Microphone capture latency (open device -> first audio buffer, 5 runs): [136, 50, 45, 54, 53] ms

### Model load and memory (benchmark process working set)

Private = committed memory of the benchmark process (the models are processed one after another in the same process).

| Model | Load (incl. warm-up) | Private before load | Private loaded | Peak working set during transcription | Private after unload |
|---|---|---|---|---|---|
| tiny.en | 232 ms | 3 MB | 651 MB | 247 MB | 3 MB |
| base.en | 372 ms | 3 MB | 715 MB | 297 MB | 4 MB |
| small.en | 1466 ms | 4 MB | 938 MB | 523 MB | 3 MB |

### Latency per utterance

`partial` = preview of the first 1.5 s; `final` = full utterance transcribed at once (worst case, no streaming); CPU = average utilisation of the whole machine during the final job.

| Model | Clip | Audio | Partial | Final | CPU | WER |
|---|---|---|---|---|---|---|
| tiny.en | short | 5.1 s | 699 ms | 1500 ms | 47% | 0.0% |
| tiny.en | question | 3.5 s | 706 ms | 3518 ms | 40% | 0.0% |
| tiny.en | punct | 3.0 s | 224 ms | 357 ms | 47% | 0.0% |
| tiny.en | tech | 5.7 s | 225 ms | 427 ms | 46% | 7.7% |
| tiny.en | terminal | 1.7 s | 707 ms | 276 ms | 48% | 0.0% |
| tiny.en | medium | 9.9 s | 235 ms | 728 ms | 44% | 0.0% |
| tiny.en | long | 22.8 s | 254 ms | 1297 ms | 47% | 5.1% |
| base.en | short | 5.1 s | 407 ms | 815 ms | 47% | 0.0% |
| base.en | question | 3.5 s | 427 ms | 740 ms | 49% | 0.0% |
| base.en | punct | 3.0 s | 454 ms | 652 ms | 49% | 28.6% |
| base.en | tech | 5.7 s | 470 ms | 946 ms | 49% | 7.7% |
| base.en | terminal | 1.7 s | 483 ms | 588 ms | 48% | 0.0% |
| base.en | medium | 9.9 s | 449 ms | 1226 ms | 48% | 0.0% |
| base.en | long | 22.8 s | 483 ms | 2454 ms | 48% | 1.7% |
| small.en | short | 5.1 s | 1491 ms | 2480 ms | 49% | 0.0% |
| small.en | question | 3.5 s | 1632 ms | 2484 ms | 49% | 0.0% |
| small.en | punct | 3.0 s | 1567 ms | 2507 ms | 49% | 42.9% |
| small.en | tech | 5.7 s | 1747 ms | 3545 ms | 49% | 7.7% |
| small.en | terminal | 1.7 s | 1614 ms | 2217 ms | 49% | 0.0% |
| small.en | medium | 9.9 s | 1683 ms | 4547 ms | 49% | 0.0% |
| small.en | long | 22.8 s | 1507 ms | 9045 ms | 49% | 1.7% |

### Streaming dictation (real-time audio through the full pipeline)

Audio is fed at real-time speed; recording auto-stops after 0.8 s of silence. `after stop` is the delay between the end of recording and the final text.

| Model | Clip | First partial (from start) | Final text after stop |
|---|---|---|---|
| tiny.en | short | 1160 ms | 1668 ms |
| tiny.en | medium | 670 ms | 655 ms |
| tiny.en | long | 680 ms | 515 ms |
| base.en | short | 880 ms | 935 ms |
| base.en | medium | 940 ms | 1296 ms |
| base.en | long | 850 ms | 1380 ms |
| small.en | short | 2010 ms | 2617 ms |
| small.en | medium | 1970 ms | 4769 ms |
| small.en | long | 2444 ms | 7034 ms |
