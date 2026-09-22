"""Regenerate THIRD_PARTY_LICENSES.md from `cargo metadata` and package.json.

    python scripts/licenses.py
Only crates compiled into the shipped app (normal dependencies, Windows x64)
are listed; build tools and dev dependencies are not distributed.
"""
import collections
import json
import os
import subprocess

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
ALLOWED = ["MIT", "Apache", "BSD", "Unlicense", "Zlib", "ISC", "Unicode", "CC0", "BSL-1.0", "MPL", "CDLA-Permissive"]

meta = json.loads(
    subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc"],
        cwd=ROOT, capture_output=True, check=True,
    ).stdout.decode("utf-8")
)
pk = {p["id"]: p for p in meta["packages"]}
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
root = next(p["id"] for p in meta["packages"] if p["name"] == "hushtype")
seen, stack = set(), [root]
while stack:
    i = stack.pop()
    if i in seen:
        continue
    seen.add(i)
    for d in nodes[i]["deps"]:
        if any(k["kind"] is None for k in d["dep_kinds"]):
            stack.append(d["pkg"])

crates = sorted((pk[i]["name"], pk[i]["version"], pk[i].get("license") or "see crate", pk[i].get("repository") or "") for i in seen
                if not pk[i]["name"].startswith("hushtype"))
bad = [c for c in crates if "GPL" in c[2] or not any(a in c[2] for a in ALLOWED)]
if bad:
    raise SystemExit(f"license review needed: {bad}")

npm = []
app = os.path.join(ROOT, "apps", "desktop")
for dep in list(json.load(open(os.path.join(app, "package.json")))["dependencies"]) + ["scheduler"]:
    j = json.load(open(os.path.join(app, "node_modules", dep, "package.json"), encoding="utf-8"))
    npm.append((dep, j["version"], j.get("license", "")))

counts = collections.Counter(c[2] for c in crates)
out = ["# Third-party licenses", "",
       "HushType is MIT licensed. It bundles the components below; all licenses permit",
       "redistribution in an MIT-licensed application. Regenerate with `python scripts/licenses.py`.", "",
       "## Major components", "",
       "| Component | License | Notes |", "|---|---|---|",
       "| whisper.cpp / ggml (vendored by whisper-rs-sys) | MIT | Speech recognition engine, statically linked |",
       "| OpenAI Whisper model weights (GGML conversions by ggerganov/whisper.cpp) | MIT | Downloaded on demand, not bundled in the installer |",
       "| Tauri | MIT OR Apache-2.0 | Desktop shell |",
       "| Microsoft WebView2 runtime | Microsoft software license | Part of Windows 10/11; not redistributed by us |",
       "| React / React DOM | MIT | Settings UI |", "",
       "MPL-2.0 crates are used unmodified (file-level copyleft; source is available from crates.io).", "",
       "## License summary (Rust crates)", "", "| License | Crates |", "|---|---|"]
out += [f"| {l} | {c} |" for l, c in counts.most_common()]
out += ["", "## Rust crates", "", "| Crate | Version | License |", "|---|---|---|"]
out += [f"| {n} | {v} | {l} |" for n, v, l, _ in crates]
out += ["", "## JavaScript packages (bundled into the UI)", "", "| Package | Version | License |", "|---|---|---|"]
out += [f"| {n} | {v} | {l} |" for n, v, l in npm]
open(os.path.join(ROOT, "THIRD_PARTY_LICENSES.md"), "w", encoding="utf-8").write("\n".join(out) + "\n")
print(f"{len(crates)} crates, {len(npm)} npm packages -> THIRD_PARTY_LICENSES.md")
