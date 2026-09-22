import { useCallback, useEffect, useState } from "react";
import { api, errorText, on, type DictationResult, type Settings, type Snapshot, type Status } from "./api";
import { AboutPage, DictionaryPage, HistoryPage, ModelsPage, SettingsPage } from "./pages";

const TABS = [
  ["dashboard", "Home"],
  ["settings", "Settings"],
  ["models", "Models"],
  ["dictionary", "Dictionary"],
  ["history", "History"],
  ["about", "About"],
] as const;

// Backend routes that live inside another tab.
const ALIASES: Record<string, [string, string?]> = {
  microphone: ["settings", "microphone"],
  shortcut: ["settings", "shortcut"],
};

function readRoute(): [string, string?] {
  const r = window.location.hash.replace(/^#\/?/, "") || "dashboard";
  return ALIASES[r] ?? [r];
}

export interface Ctx {
  snap: Snapshot;
  update: (patch: Partial<Settings>) => Promise<void>;
  toast: (msg: string, error?: boolean) => void;
}

export default function App() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [[route, anchor], setRoute] = useState(readRoute());
  const [toastMsg, setToast] = useState<{ msg: string; error: boolean } | null>(null);

  const toast = useCallback((msg: string, error = false) => {
    setToast({ msg, error });
    window.setTimeout(() => setToast(null), error ? 6000 : 3000);
  }, []);

  useEffect(() => {
    api.getState().then(setSnap);
    const onHash = () => setRoute(readRoute());
    window.addEventListener("hashchange", onHash);
    const offs = [
      on<string>("navigate", (r) => (window.location.hash = `#/${r}`)),
      on<Status>("status", (status) => setSnap((s) => (s ? { ...s, status } : s))),
      on<Settings>("settings", (settings) => setSnap((s) => (s ? { ...s, settings } : s))),
    ];
    return () => {
      window.removeEventListener("hashchange", onHash);
      offs.forEach((f) => f());
    };
  }, []);

  useEffect(() => {
    const t = snap?.settings.theme;
    if (t && t !== "system") document.documentElement.dataset.theme = t;
    else delete document.documentElement.dataset.theme;
  }, [snap?.settings.theme]);

  useEffect(() => {
    if (anchor) document.getElementById(anchor)?.scrollIntoView();
  }, [anchor, snap !== null]);

  const update = useCallback(
    async (patch: Partial<Settings>) => {
      if (!snap) return;
      try {
        const res = await api.saveSettings({ ...snap.settings, ...patch });
        res.warnings.forEach((w) => toast(w));
      } catch (e) {
        toast(errorText(e), true);
        api.getState().then(setSnap);
      }
    },
    [snap, toast],
  );

  if (!snap) return null;
  const ctx: Ctx = { snap, update, toast };
  const go = (r: string) => (window.location.hash = `#/${r}`);

  if (route === "welcome" || !snap.settings.onboarded) return <Welcome ctx={ctx} />;

  return (
    <div className="app">
      <nav>
        <div className="brand">HushType</div>
        {TABS.map(([id, label]) => (
          <button key={id} className={route === id ? "active" : ""} onClick={() => go(id)}>
            {label}
          </button>
        ))}
      </nav>
      <main>
        <div>
          {route === "settings" && <SettingsPage ctx={ctx} />}
          {route === "models" && <ModelsPage ctx={ctx} />}
          {route === "dictionary" && <DictionaryPage ctx={ctx} />}
          {route === "history" && <HistoryPage ctx={ctx} />}
          {route === "about" && <AboutPage ctx={ctx} />}
          {!["settings", "models", "dictionary", "history", "about"].includes(route) && <Home ctx={ctx} />}
        </div>
      </main>
      {toastMsg && <div className={`toast ${toastMsg.error ? "error" : ""}`}>{toastMsg.msg}</div>}
    </div>
  );
}

function modelLabel(s: Status): string {
  const m = s.model;
  if (!s.modelInstalled) return "Not installed";
  switch (m.state) {
    case "loaded":
      return `Loaded (${m.backend})`;
    case "loading":
      return "Loading…";
    case "error":
      return m.message;
    default:
      return "Not loaded (loads when you start dictating)";
  }
}

function Home({ ctx }: { ctx: Ctx }) {
  const { snap } = ctx;
  const st = snap.status;
  const [partial, setPartial] = useState("");
  const [last, setLast] = useState<DictationResult | null>(null);

  useEffect(() => {
    const offs = [
      on<string>("partial", setPartial),
      on<DictationResult>("dictation-result", (r) => {
        setLast(r);
        setPartial("");
      }),
      on<string>("dictation-error", (e) => ctx.toast(e, true)),
    ];
    return () => offs.forEach((f) => f());
  }, [ctx]);

  const phaseText = { idle: "Ready", recording: "Listening…", processing: "Processing…" }[st.phase];
  const mode = { hold: "Hold", toggle: "Press to start, press again to stop:", hybrid: "Hold to talk, or tap once for hands-free:" }[
    snap.settings.hotkeyMode
  ];

  return (
    <>
      <h1>
        <span className="pill">
          <span className={`dot ${st.phase}`} /> {phaseText}
        </span>
      </h1>
      {st.hotkeyError && <p className="error-text">{st.hotkeyError}</p>}
      {!st.cpuSupported && <p className="error-text">This processor lacks AVX2, which the speech engine needs.</p>}
      <div className="card">
        <div className="row">
          <div className="label">
            {mode} <span className="kbd">{st.hotkey}</span>
            <div className="hint">Speak, and the text is typed into whatever app you're using. Esc cancels.</div>
          </div>
        </div>
        <div className="row">
          <div className="label">
            Model: <b>{st.modelId}</b>
            <div className="hint">{modelLabel(st)}</div>
          </div>
          {!st.modelInstalled ? (
            <button className="btn primary" onClick={() => (window.location.hash = "#/models")}>
              Install model
            </button>
          ) : st.model.state === "loaded" ? (
            <button className="btn" onClick={() => api.unloadModel()}>
              Unload
            </button>
          ) : (
            <button className="btn" onClick={() => api.loadModel()} disabled={st.model.state === "loading"}>
              Load now
            </button>
          )}
        </div>
      </div>
      <h2>Try it</h2>
      <div className="card" style={{ padding: 12 }}>
        <textarea id="try" placeholder={`Click here, then hold ${st.hotkey} and speak…`} />
        {partial && <p className="muted">“{partial}”</p>}
        {last && (
          <p className="hint">
            Last: {last.outcome} into {last.app} — {(last.audioMs / 1000).toFixed(1)} s of audio, text ready {last.finalLatencyMs} ms
            after you stopped.
          </p>
        )}
      </div>
    </>
  );
}

function Welcome({ ctx }: { ctx: Ctx }) {
  const { snap, update } = ctx;
  const [startup, setStartup] = useState(true);
  const installed = snap.status.modelInstalled;
  const finish = async () => {
    await update({ launchAtStartup: startup });
    await api.finishOnboarding();
    window.location.hash = "#/dashboard";
  };
  return (
    <main>
      <div>
        <h1>Welcome to HushType</h1>
        <p>
          Voice typing for every app. Hold <span className="kbd">{snap.status.hotkey}</span>, speak, release — clean, punctuated text
          appears where your cursor is.
        </p>
        <p className="hint">Speech is recognised on this PC with Whisper. No audio or text ever leaves your computer.</p>
        <h2>1. Speech model</h2>
        <div className="card" style={{ padding: 12 }}>
          {installed ? <p>✓ Model <b>{snap.status.modelId}</b> is installed.</p> : <ModelsPage ctx={ctx} compact />}
        </div>
        <h2>2. Microphone</h2>
        <div className="card" style={{ padding: 12 }}>
          <SettingsPage ctx={ctx} only="microphone" />
        </div>
        <h2>3. Start with Windows</h2>
        <div className="card">
          <div className="row">
            <div className="label">Start HushType when I sign in (it waits quietly in the tray)</div>
            <input type="checkbox" checked={startup} onChange={(e) => setStartup(e.target.checked)} />
          </div>
        </div>
        <button className="btn primary" disabled={!installed} onClick={finish}>
          {installed ? "Finish" : "Install a model to continue"}
        </button>
      </div>
    </main>
  );
}
