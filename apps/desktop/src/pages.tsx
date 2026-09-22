import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  api,
  errorText,
  formatBytes,
  on,
  type AppContext,
  type Diagnostics,
  type DownloadEvent,
  type HistoryEntry,
  type InputDevice,
  type ModelsPayload,
  type Settings,
  type Term,
} from "./api";
import type { Ctx } from "./App";

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="row">
      <div className="label">
        {label}
        {hint && <div className="hint">{hint}</div>}
      </div>
      {children}
    </div>
  );
}

function Check({ ctx, k, label, hint }: { ctx: Ctx; k: keyof Settings; label: string; hint?: string }) {
  return (
    <Row label={label} hint={hint}>
      <input type="checkbox" checked={ctx.snap.settings[k] as boolean} onChange={(e) => ctx.update({ [k]: e.target.checked })} />
    </Row>
  );
}

const LANGUAGES: [string, string][] = [
  ["auto", "Detect automatically"],
  ["en", "English"],
  ["es", "Spanish"],
  ["fr", "French"],
  ["de", "German"],
  ["it", "Italian"],
  ["pt", "Portuguese"],
  ["nl", "Dutch"],
  ["pl", "Polish"],
  ["ru", "Russian"],
  ["uk", "Ukrainian"],
  ["tr", "Turkish"],
  ["hi", "Hindi"],
  ["ja", "Japanese"],
  ["ko", "Korean"],
  ["zh", "Chinese"],
  ["ar", "Arabic"],
  ["sv", "Swedish"],
];

// ---------------------------------------------------------------------------
// Settings (incl. microphone and shortcut)
// ---------------------------------------------------------------------------

export function SettingsPage({ ctx, only }: { ctx: Ctx; only?: "microphone" }) {
  const s = ctx.snap.settings;
  if (only === "microphone") return <Microphone ctx={ctx} />;
  return (
    <>
      <h1>Settings</h1>
      <h2 id="shortcut">Keyboard shortcut</h2>
      <Shortcut ctx={ctx} />
      <h2 id="microphone">Microphone</h2>
      <div className="card" style={{ padding: 12 }}>
        <Microphone ctx={ctx} />
      </div>
      <h2>General</h2>
      <div className="card">
        <Check ctx={ctx} k="launchAtStartup" label="Launch when Windows starts" hint="Starts silently in the tray." />
        <Check ctx={ctx} k="startMinimized" label="Start minimized" hint="Don't open this window when HushType is started manually." />
        <Check ctx={ctx} k="showIndicator" label="Show recording indicator" hint="The small pill showing Listening / Processing. Errors are always shown." />
        <Row label="Indicator position">
          <select value={s.indicatorTop ? "top" : "bottom"} onChange={(e) => ctx.update({ indicatorTop: e.target.value === "top" })}>
            <option value="bottom">Bottom of screen</option>
            <option value="top">Top of screen</option>
          </select>
        </Row>
        <Check ctx={ctx} k="playSounds" label="Play sounds" />
        <Row label="Theme">
          <select value={s.theme} onChange={(e) => ctx.update({ theme: e.target.value as Settings["theme"] })}>
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </Row>
      </div>
      <h2>Speech</h2>
      <div className="card">
        <Row label="Language" hint="English-only models (.en) always use English.">
          <select value={s.language} onChange={(e) => ctx.update({ language: e.target.value })}>
            {LANGUAGES.map(([v, l]) => (
              <option key={v} value={v}>
                {l}
              </option>
            ))}
          </select>
        </Row>
        <Row label={`Stop after silence: ${(s.silenceTimeoutMs / 1000).toFixed(1)} s`} hint="Hands-free / toggle mode ends after this pause.">
          <input type="range" min={500} max={6000} step={250} value={s.silenceTimeoutMs} onChange={(e) => ctx.update({ silenceTimeoutMs: +e.target.value })} />
        </Row>
        <Row label={`Voice detection sensitivity: ${s.vadSensitivity}%`} hint="Raise it for quiet voices, lower it in noisy rooms.">
          <input type="range" min={0} max={100} step={5} value={s.vadSensitivity} onChange={(e) => ctx.update({ vadSensitivity: +e.target.value })} />
        </Row>
        <Check ctx={ctx} k="livePreview" label="Live preview while speaking" hint="Shows partial text in the indicator. Uses extra CPU while recording." />
        <Row label="Unload model when inactive" hint="Frees memory; the model reloads automatically when you dictate.">
          <select value={s.unloadAfterMin} onChange={(e) => ctx.update({ unloadAfterMin: +e.target.value })}>
            <option value={0}>Never</option>
            <option value={5}>After 5 minutes</option>
            <option value={15}>After 15 minutes</option>
            <option value={30}>After 30 minutes</option>
            <option value={60}>After 60 minutes</option>
          </select>
        </Row>
        <Check ctx={ctx} k="loadModelAtStartup" label="Load model at startup" hint="Faster first dictation, more memory while idle." />
      </div>
      <h2>Text</h2>
      <div className="card">
        <Check ctx={ctx} k="removeFillers" label="Remove filler words" hint="um, uh, filler “like”…" />
        <Check ctx={ctx} k="smartPunctuation" label="Smart punctuation" hint="Sentence endings and question marks." />
        <Check ctx={ctx} k="autoCapitalize" label="Auto capitalization" />
        <Check ctx={ctx} k="spokenPunctuation" label="Spoken punctuation" hint="“comma”, “question mark”, “new line”, “new paragraph”…" />
        <Check ctx={ctx} k="aggressiveCleanup" label="Aggressive cleanup" hint="Also removes “you know”, “basically”, repeated phrases." />
        <Check ctx={ctx} k="contextAware" label="Adapt formatting to the app" hint="E.g. no trailing period in terminals; Shift+Enter for new lines in chat apps." />
        <Row label="Insert text by">
          <select value={s.insertMethod} onChange={(e) => ctx.update({ insertMethod: e.target.value as Settings["insertMethod"] })}>
            <option value="auto">Automatic (paste and restore clipboard; type in terminals)</option>
            <option value="type">Always typing</option>
            <option value="paste">Always pasting</option>
          </select>
        </Row>
        <Check ctx={ctx} k="restoreClipboard" label="Restore clipboard after pasting" />
        <Preview />
      </div>
      <h2>Privacy</h2>
      <div className="card">
        <Check ctx={ctx} k="saveHistory" label="Save transcription history" hint="Stored only on this PC." />
        <Check ctx={ctx} k="storeAudio" label="Keep audio recordings with history" hint="Off by default. WAV files in your local app data folder." />
        <Row label="Telemetry" hint="HushType has no telemetry or analytics. Audio never leaves this computer.">
          <span className="pill">Permanently off</span>
        </Row>
      </div>
    </>
  );
}

function Preview() {
  const [raw, setRaw] = useState("um so can you like create a function that uh returns the user's email");
  const [ctxName, setCtx] = useState<AppContext>("general");
  const [out, setOut] = useState("");
  useEffect(() => {
    const t = window.setTimeout(() => api.previewText(raw, ctxName).then(setOut), 150);
    return () => window.clearTimeout(t);
  }, [raw, ctxName]);
  return (
    <div className="row" style={{ display: "block" }}>
      <div className="label" style={{ marginBottom: 6 }}>
        Try the cleanup{" "}
        <select value={ctxName} onChange={(e) => setCtx(e.target.value as AppContext)}>
          <option value="general">in a normal app</option>
          <option value="terminal">in a terminal</option>
          <option value="code">in a code editor</option>
          <option value="chat">in a chat app</option>
        </select>
      </div>
      <input type="text" style={{ width: "100%" }} value={raw} onChange={(e) => setRaw(e.target.value)} />
      <p style={{ whiteSpace: "pre-wrap" }}>→ {out}</p>
    </div>
  );
}

function keyName(e: KeyboardEvent): string | null {
  const c = e.code;
  if (/^Key[A-Z]$/.test(c)) return c.slice(3);
  if (/^Digit[0-9]$/.test(c)) return c.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(c)) return c;
  if (/^Numpad[0-9]$/.test(c)) return "Num" + c.slice(6);
  const map: Record<string, string> = {
    Space: "Space", Tab: "Tab", Enter: "Enter", Backspace: "Backspace", Pause: "Pause", ScrollLock: "ScrollLock",
    Insert: "Insert", Delete: "Delete", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown",
    ArrowLeft: "Left", ArrowRight: "Right", ArrowUp: "Up", ArrowDown: "Down", Backquote: "`", Minus: "-", Equal: "=",
    BracketLeft: "[", BracketRight: "]", Backslash: "\\", Semicolon: ";", Quote: "'", Comma: ",", Period: ".", Slash: "/",
  };
  return map[c] ?? null;
}

function Shortcut({ ctx }: { ctx: Ctx }) {
  const s = ctx.snap.settings;
  const [capturing, setCapturing] = useState(false);
  const [candidate, setCandidate] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);

  useEffect(() => {
    if (!capturing) return;
    api.suspendHotkey(true);
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      const key = keyName(e);
      if (!key) return;
      const parts = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Win", key].filter(Boolean);
      const label = parts.join("+");
      api
        .checkHotkey(label)
        .then((r) => {
          setCandidate(r.label);
          setWarning(r.warning);
        })
        .catch((err) => {
          setCandidate(null);
          setWarning(errorText(err));
        });
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      api.suspendHotkey(false);
    };
  }, [capturing]);

  const save = async () => {
    if (candidate) await ctx.update({ hotkey: candidate });
    setCapturing(false);
    setCandidate(null);
    setWarning(null);
  };

  return (
    <div className="card">
      <Row label="Shortcut" hint={ctx.snap.status.hotkeyError ?? undefined}>
        {capturing ? (
          <div className="actions">
            <span className="kbd">{candidate ?? "Press keys…"}</span>
            <button className="btn primary" disabled={!candidate} onClick={save}>
              Save
            </button>
            <button className="btn" onClick={() => setCapturing(false)}>
              Cancel
            </button>
          </div>
        ) : (
          <div className="actions">
            <span className="kbd">{s.hotkey}</span>
            <button className="btn" onClick={() => setCapturing(true)}>
              Change
            </button>
          </div>
        )}
      </Row>
      {warning && <p className="hint error-text">{warning}</p>}
      <Row label="Mode">
        <select value={s.hotkeyMode} onChange={(e) => ctx.update({ hotkeyMode: e.target.value as Settings["hotkeyMode"] })}>
          <option value="hybrid">Hold to talk, tap for hands-free (recommended)</option>
          <option value="hold">Push-to-talk (hold)</option>
          <option value="toggle">Toggle (press to start and stop)</option>
        </select>
      </Row>
    </div>
  );
}

function Microphone({ ctx }: { ctx: Ctx }) {
  const [devices, setDevices] = useState<InputDevice[]>([]);
  const [level, setLevel] = useState(-100);
  const [err, setErr] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const mic = ctx.snap.settings.microphone;

  useEffect(() => {
    api.listMicrophones().then(setDevices).catch((e) => setErr(errorText(e)));
    const off = on<{ db: number; error: string | null }>("mic-level", (l) => {
      if (l.error && l.error !== "stopped") setErr(l.error);
      if (l.error) setTesting(false);
      setLevel(l.db);
    });
    return () => {
      off();
      api.stopMicTest();
    };
  }, []);

  const test = () => {
    setErr(null);
    setTesting(true);
    api.startMicTest(mic).catch((e) => {
      setErr(errorText(e));
      setTesting(false);
    });
  };
  const pct = Math.max(0, Math.min(100, ((level + 60) / 60) * 100));
  return (
    <>
      <div className="actions" style={{ alignItems: "center" }}>
        <select value={mic} onChange={(e) => ctx.update({ microphone: e.target.value })} style={{ flex: 1 }}>
          <option value="">System default{devices.find((d) => d.is_default) ? ` (${devices.find((d) => d.is_default)!.name})` : ""}</option>
          {devices.map((d) => (
            <option key={d.name} value={d.name}>
              {d.name}
            </option>
          ))}
        </select>
        {testing ? (
          <button className="btn" onClick={() => api.stopMicTest()}>
            Stop test
          </button>
        ) : (
          <button className="btn" onClick={test}>
            Test
          </button>
        )}
      </div>
      {testing && (
        <div className="meter" style={{ marginTop: 10 }}>
          <div style={{ width: `${pct}%` }} />
        </div>
      )}
      {err && (
        <p className="error-text">
          {err}{" "}
          <button className="btn" onClick={() => api.openExternal("mic-privacy")}>
            Open Windows microphone settings
          </button>
        </p>
      )}
      {devices.length === 0 && !err && <p className="hint">No microphones found.</p>}
    </>
  );
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

export function ModelsPage({ ctx, compact }: { ctx: Ctx; compact?: boolean }) {
  const [data, setData] = useState<ModelsPayload | null>(null);
  const [dl, setDl] = useState<DownloadEvent | null>(null);
  const refresh = () => api.listModels().then(setData);

  useEffect(() => {
    refresh();
    const offs = [
      on<DownloadEvent>("model-download", (e) => {
        setDl(e.done ? null : e);
        if (e.done) {
          if (e.error) ctx.toast(e.error, true);
          refresh();
        }
      }),
      on("model-status", () => refresh()),
    ];
    return () => offs.forEach((f) => f());
  }, []);

  if (!data) return null;
  const hw = data.hardware;
  const shown = compact ? data.models.filter((m) => m.recommended || m.selected) : data.models;
  const download = (id: string) => api.downloadModel(id).catch((e) => ctx.toast(errorText(e), true));

  return (
    <>
      {!compact && <h1>Speech models</h1>}
      {!compact && (
        <div className="card" style={{ padding: 12 }}>
          <div>
            CPU: {hw.cpu.logical_cores} threads, AVX2 {hw.cpu.avx2 ? "✓" : "✗"}, inference uses {hw.threads} threads · RAM{" "}
            {Math.round(hw.ramTotalMb / 1024)} GB ({Math.round(hw.ramAvailableMb / 1024)} GB free)
          </div>
          <div className="hint">
            GPU: {hw.gpus.map((g) => g.name).join(", ") || "none detected"} —{" "}
            {hw.gpuBackends.length ? `accelerated with ${hw.gpuBackends.join(", ")}` : "this build runs on the CPU (see README for GPU builds)"}
          </div>
        </div>
      )}
      <div className="card">
        {shown.map((m) => {
          const busy = dl?.id === m.id;
          return (
            <div className="row" key={m.id}>
              <div className="label">
                <b>{m.name}</b> {m.recommended && <span className="pill">Recommended</span>} {m.selected && <span className="pill">In use</span>}
                <div className="hint">
                  {m.description} · {formatBytes(m.size_bytes)} download · ~{m.ram_mb} MB RAM
                </div>
                {busy && dl && (
                  <div className="meter" style={{ marginTop: 6 }}>
                    <div style={{ width: `${(100 * dl.downloaded) / dl.total}%` }} />
                  </div>
                )}
                {busy && dl && (
                  <div className="hint">
                    {formatBytes(dl.downloaded)} / {formatBytes(dl.total)} · {formatBytes(dl.bytesPerSec)}/s
                  </div>
                )}
              </div>
              <div className="actions">
                {busy ? (
                  <button className="btn" onClick={() => api.cancelDownload()}>
                    Cancel
                  </button>
                ) : !m.installed ? (
                  <button className="btn primary" disabled={!!dl} onClick={() => download(m.id)}>
                    Download
                  </button>
                ) : (
                  <>
                    {!m.selected && (
                      <button className="btn" onClick={() => ctx.update({ model: m.id }).then(refresh)}>
                        Use
                      </button>
                    )}
                    {!compact && (
                      <button className="btn danger" onClick={() => api.deleteModel(m.id).then(refresh)}>
                        Delete
                      </button>
                    )}
                  </>
                )}
              </div>
            </div>
          );
        })}
      </div>
      {!compact && (
        <div className="actions">
          <button className="btn" onClick={() => api.openExternal("models")}>
            Open models folder
          </button>
          <span className="hint">Offline install: copy a model file from the README's table into this folder.</span>
        </div>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------
// Dictionary
// ---------------------------------------------------------------------------

export function DictionaryPage({ ctx }: { ctx: Ctx }) {
  const [terms, setTerms] = useState<Term[]>([]);
  const [term, setTerm] = useState("");
  const [aliases, setAliases] = useState("");
  const [filter, setFilter] = useState("");
  useEffect(() => {
    api.getDictionary().then(setTerms);
  }, []);
  const save = async (next: Term[]) => {
    try {
      await api.saveDictionary(next);
      setTerms(await api.getDictionary());
    } catch (e) {
      ctx.toast(errorText(e), true);
    }
  };
  const add = () => {
    if (!term.trim()) return;
    const a = aliases.split(",").map((x) => x.trim()).filter(Boolean);
    save([{ term: term.trim(), aliases: a }, ...terms.filter((t) => t.term.toLowerCase() !== term.trim().toLowerCase())]);
    setTerm("");
    setAliases("");
  };
  const shown = terms.filter((t) => !filter || t.term.toLowerCase().includes(filter.toLowerCase()));
  return (
    <>
      <h1>Dictionary</h1>
      <p className="hint">
        Words and names spelled exactly as written here. They also help the recogniser hear them. Add “sounds like” spellings for words it gets
        wrong (e.g. <i>super base</i> → Supabase).
      </p>
      <div className="card" style={{ padding: 12 }}>
        <div className="actions">
          <input type="text" placeholder="Term, e.g. Kubernetes" value={term} onChange={(e) => setTerm(e.target.value)} />
          <input type="text" placeholder="Sounds like (comma separated, optional)" style={{ flex: 1 }} value={aliases} onChange={(e) => setAliases(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
          <button className="btn primary" onClick={add}>
            Add
          </button>
        </div>
      </div>
      <div className="actions" style={{ marginBottom: 8 }}>
        <input type="search" placeholder="Search" value={filter} onChange={(e) => setFilter(e.target.value)} />
        <button className="btn" onClick={() => api.resetDictionary().then(setTerms)}>
          Reset to defaults
        </button>
      </div>
      <div className="card">
        {shown.map((t) => (
          <div className="row" key={t.term}>
            <div className="label">
              <b>{t.term}</b>
              <div className="chips">{t.aliases.map((a) => <span key={a}>{a}</span>)}</div>
            </div>
            <button className="btn danger" onClick={() => save(terms.filter((x) => x.term !== t.term))}>
              Remove
            </button>
          </div>
        ))}
        {shown.length === 0 && <p className="hint">No terms.</p>}
      </div>
    </>
  );
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

export function HistoryPage({ ctx }: { ctx: Ctx }) {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<number | null>(null);
  const load = (offset: number, q = query) =>
    api.getHistory(offset, 50, q).then((p) => {
      setEntries((prev) => (offset === 0 ? p.entries : [...prev, ...p.entries]));
      setTotal(p.total);
    });
  useEffect(() => {
    load(0);
    return on("dictation-result", () => load(0));
  }, []);
  const q = useRef<number>(0);
  const search = (v: string) => {
    setQuery(v);
    window.clearTimeout(q.current);
    q.current = window.setTimeout(() => load(0, v), 200);
  };
  const clear = async () => {
    if (!window.confirm("Delete all history entries?")) return;
    await api.clearHistory();
    load(0);
  };
  return (
    <>
      <h1>History</h1>
      {!ctx.snap.settings.saveHistory && <p className="hint">History is turned off in Settings › Privacy.</p>}
      <div className="actions" style={{ marginBottom: 8 }}>
        <input type="search" placeholder="Search" value={query} onChange={(e) => search(e.target.value)} />
        <button className="btn danger" onClick={clear} disabled={total === 0}>
          Clear history
        </button>
      </div>
      <div className="card">
        {entries.map((e) => (
          <div className="entry" key={e.id}>
            <div className="hint">
              {new Date(e.timestamp).toLocaleString()} · {e.app} · {(e.durationMs / 1000).toFixed(1)} s
            </div>
            <div style={{ whiteSpace: "pre-wrap", margin: "4px 0" }}>{e.text}</div>
            {open === e.id && <div className="hint">Raw: {e.raw}</div>}
            <div className="actions">
              <button className="btn" onClick={() => api.copyText(e.text).then(() => ctx.toast("Copied"))}>
                Copy
              </button>
              <button className="btn" onClick={() => setOpen(open === e.id ? null : e.id)}>
                {open === e.id ? "Hide raw" : "Show raw"}
              </button>
              <button className="btn danger" onClick={() => api.deleteHistory(e.id).then(() => load(0))}>
                Delete
              </button>
            </div>
          </div>
        ))}
        {entries.length === 0 && <p className="hint">Nothing yet.</p>}
      </div>
      {entries.length < total && (
        <button className="btn" onClick={() => load(entries.length)}>
          Load more
        </button>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------
// About
// ---------------------------------------------------------------------------

export function AboutPage({ ctx }: { ctx: Ctx }) {
  const [d, setD] = useState<Diagnostics | null>(null);
  useEffect(() => {
    api.diagnostics().then(setD);
  }, []);
  return (
    <>
      <h1>About HushType {ctx.snap.version}</h1>
      <p>Private, local voice typing for Windows. Open source under the MIT License.</p>
      <div className="card" style={{ padding: 12 }}>
        <b>Privacy</b>
        <p className="hint">
          Speech recognition runs entirely on this computer (whisper.cpp). Audio is processed in memory and discarded. There is no telemetry, no
          analytics and no account. The only network access is downloading a speech model when you ask for it.
        </p>
      </div>
      <div className="card" style={{ padding: 12 }}>
        <b>Built with</b>
        <p className="hint">
          whisper.cpp &amp; ggml (MIT), OpenAI Whisper models (MIT), whisper-rs (Unlicense), Tauri (MIT/Apache-2.0), cpal (Apache-2.0),
          windows-rs (MIT/Apache-2.0), ureq (MIT/Apache-2.0), React (MIT). Full list in THIRD_PARTY_LICENSES.md.
        </p>
      </div>
      {d && (
        <div className="card" style={{ padding: 12 }}>
          <b>Diagnostics</b>
          <p className="hint">
            Memory: {d.workingSetMb.toFixed(0)} MB working set, {d.privateMb.toFixed(0)} MB private (main process; this window runs in separate
            WebView2 processes that exit when it closes).
            {d.elevated && " Running as administrator."}
          </p>
          <pre>{d.whisper}</pre>
          <pre>{d.paths.data}</pre>
          <div className="actions">
            <button className="btn" onClick={() => api.openExternal("logs")}>
              Open logs
            </button>
            <button className="btn" onClick={() => api.openExternal("data")}>
              Open data folder
            </button>
          </div>
        </div>
      )}
    </>
  );
}
