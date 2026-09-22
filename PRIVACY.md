# Privacy

HushType is built so that your voice and words stay on your computer.

## What happens to your audio

- The microphone is opened only after you press the shortcut (or choose *Start Listening*) and is closed as soon as recording ends. While idle, HushType does not capture or buffer any audio.
- Audio is transcribed in memory by whisper.cpp running inside the HushType process. It is **never sent anywhere** — there is no cloud speech service, not even as an option.
- Audio buffers are released right after transcription. Audio is written to disk **only** if you turn on *Settings › Privacy › Keep audio recordings with history* (off by default); those WAV files live in `%LOCALAPPDATA%\HushType\recordings` and are deleted with their history entries.

## What is stored

| Data | Where | Control |
|---|---|---|
| Settings | `%APPDATA%\HushType\settings.json` | — |
| Dictionary | `%APPDATA%\HushType\dictionary.json` | Edit in the app |
| History (time, app name, raw and cleaned text) | `%LOCALAPPDATA%\HushType\history.jsonl` | Turn off, delete entries, or clear all |
| Speech models | `%LOCALAPPDATA%\HushType\models` | Delete in *Models* |
| Logs (timings, errors) | `%LOCALAPPDATA%\HushType\logs` | Never contain transcribed text or audio |

The window title of the focused app is read at dictation time only to choose formatting (e.g. a WhatsApp tab in a browser counts as a chat app). It is not stored; history records only the app's name.

Text inserted by pasting is marked so that Windows clipboard history and cloud clipboard ignore it, and your previous clipboard contents are restored.

## Network

HushType makes network requests only when **you** download a speech model (HTTPS to `huggingface.co`, verified by SHA-256). There is:

- no telemetry, analytics, crash reporting or usage statistics (and no setting to enable them),
- no account or sign-in,
- no update check.

Once a model is installed, HushType works fully offline. The settings window is a local WebView page with a strict Content Security Policy; it loads nothing from the internet.

## Uninstalling

The uninstaller removes the program and the *start with Windows* entry. Tick *Delete application data* to also remove settings, dictionary, history, recordings, logs and models.
