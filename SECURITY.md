# Security Policy

## Reporting a vulnerability

Please report security issues privately via GitHub's *Report a vulnerability* (Security Advisories) on this repository rather than opening a public issue. Include steps to reproduce and the affected version. We aim to acknowledge reports within 7 days.

## Design principles

- **Transcripts are untrusted input.** HushType only inserts text. It never executes dictated text, never runs shell commands, and never presses Enter in terminals (line breaks become spaces there; in chat apps they become Shift+Enter).
- **No listening sockets.** The UI talks to the core through Tauri's in-process IPC; no local HTTP server or port is opened.
- **Minimal network.** The only outbound connection is the user-initiated model download over HTTPS, checked against pinned SHA-256 hashes before the file is used. The download host can be overridden with `HUSHTYPE_MODEL_MIRROR` for air-gapped mirrors; hashes are still enforced.
- **No self-updater.** Unsigned auto-update mechanisms are a common attack vector; updates are installed manually until a signed update channel exists.
- **Least privilege.** Installs per user without administrator rights. Windows prevents normal-integrity apps from sending input to elevated windows; HushType respects that and falls back to the clipboard.
- **Sensitive data.** Logs never contain audio or transcribed text. Audio is not stored unless the user enables it.
- **Shell access is allow-listed.** The UI can only open a fixed set of folders, `ms-settings:` pages, or `https://` links.

## Supported versions

Only the latest release receives security fixes.
