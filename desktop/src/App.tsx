import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./App.css";

type Settings = {
  clientId: string;
  dryRun: boolean;
  scheduleEnabled: boolean;
  runAtLogin: boolean;
  runOnStartup: boolean;
  runHour: number;
  runMinute: number;
  accountEmail?: string | null;
  accountName?: string | null;
};

type CleanupResult = {
  scanned: number;
  matched: number;
  deleted: number;
  failed: number;
  dryRun: boolean;
  matches: MatchRecord[];
};

type MatchRecord = {
  id: string;
  sender: string;
  subject: string;
  receivedAt: string;
};

type RunRecord = {
  id: string;
  trigger: string;
  startedAt: string;
  finishedAt: string;
  status: string;
  result?: CleanupResult | null;
  error?: string | null;
};

type AppState = {
  settings: Settings;
  signedIn: boolean;
  fixedPrefix: string;
  schedule: {
    enabled: boolean;
    loaded: boolean;
    nextRunDescription?: string | null;
  };
  latestRun?: RunRecord | null;
};

type DevicePrompt = {
  userCode: string;
  verificationUri: string;
  message: string;
  expiresIn: number;
};

type CleanupEvent =
  | { type: "started"; dryRun: boolean; trigger: string }
  | { type: "scanProgress"; scanned: number }
  | { type: "matched"; record: MatchRecord }
  | { type: "scanComplete"; scanned: number; matched: number }
  | { type: "deleteProgress"; deleted: number; failed: number; total: number }
  | { type: "finished"; result: CleanupResult }
  | { type: "failed"; message: string };

const defaultSettings: Settings = {
  clientId: "",
  dryRun: true,
  scheduleEnabled: true,
  runAtLogin: true,
  runOnStartup: true,
  runHour: 8,
  runMinute: 0,
  accountEmail: null,
  accountName: null,
};

function messageFrom(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong.";
}

function formatDate(value?: string | null) {
  if (!value) return "Never";
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString();
}

function App() {
  const [appState, setAppState] = useState<AppState | null>(null);
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [history, setHistory] = useState<RunRecord[]>([]);
  const [prompt, setPrompt] = useState<DevicePrompt | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [running, setRunning] = useState(false);
  const [saving, setSaving] = useState(false);
  const [progress, setProgress] = useState("Ready");
  const [matches, setMatches] = useState<MatchRecord[]>([]);
  const [error, setError] = useState("");
  const [showSettings, setShowSettings] = useState(false);
  const startupTriggered = useRef(false);

  const refresh = useCallback(async () => {
    const [state, runs] = await Promise.all([
      invoke<AppState>("get_app_state"),
      invoke<RunRecord[]>("get_run_history"),
    ]);
    setAppState(state);
    setSettings(state.settings);
    setHistory(runs);
  }, []);

  useEffect(() => {
    refresh().catch((reason) => setError(messageFrom(reason)));
    const unlisten = listen<CleanupEvent>("cleanup-progress", ({ payload }) => {
      switch (payload.type) {
        case "started":
          setMatches([]);
          setProgress(payload.dryRun ? "Scanning safely…" : "Scanning before deletion…");
          break;
        case "scanProgress":
          setProgress(`Scanned ${payload.scanned} messages…`);
          break;
        case "matched":
          setMatches((current) => [...current, payload.record]);
          break;
        case "scanComplete":
          setProgress(
            `Scan complete: ${payload.matched} matches among ${payload.scanned} messages.`,
          );
          break;
        case "deleteProgress":
          setProgress(
            `Moved ${payload.deleted} of ${payload.total}; ${payload.failed} failed.`,
          );
          break;
        case "finished":
          setProgress(
            payload.result.dryRun
              ? `Dry run complete: ${payload.result.matched} matches.`
              : `Cleanup complete: ${payload.result.deleted} moved to Deleted Items.`,
          );
          break;
        case "failed":
          setError(payload.message);
          setProgress("Cleanup failed");
      }
    });
    return () => {
      void unlisten.then((dispose) => dispose());
    };
  }, [refresh]);

  useEffect(() => {
    if (
      appState?.signedIn &&
      appState.settings.runOnStartup &&
      !startupTriggered.current
    ) {
      startupTriggered.current = true;
      void runNow();
    }
  }, [appState]);

  const latest = history[0] ?? appState?.latestRun;
  const configured = Boolean(settings.clientId);

  const timeValue = useMemo(
    () => `${String(settings.runHour).padStart(2, "0")}:${String(settings.runMinute).padStart(2, "0")}`,
    [settings.runHour, settings.runMinute],
  );

  async function saveSettings(next = settings, applySchedule = false) {
    setSaving(true);
    setError("");
    try {
      const state = await invoke<AppState>("save_app_settings", { settings: next });
      setAppState(state);
      setSettings(state.settings);
      if (applySchedule) {
        if (next.scheduleEnabled) {
          await invoke("apply_schedule");
        } else {
          await invoke("disable_schedule");
        }
        await refresh();
      }
    } catch (reason) {
      setError(messageFrom(reason));
      throw reason;
    } finally {
      setSaving(false);
    }
  }

  async function connect(event: FormEvent) {
    event.preventDefault();
    setConnecting(true);
    setError("");
    try {
      await saveSettings(settings);
      const nextPrompt = await invoke<DevicePrompt>("begin_device_sign_in", {
        clientId: settings.clientId,
      });
      setPrompt(nextPrompt);
      await openUrl(nextPrompt.verificationUri);
      await invoke("complete_device_sign_in");
      setPrompt(null);
      if (settings.scheduleEnabled) {
        try {
          await invoke("apply_schedule");
        } catch (scheduleError) {
          setError(`Microsoft connected, but scheduling needs attention: ${messageFrom(scheduleError)}`);
        }
      }
      await refresh();
    } catch (reason) {
      setError(messageFrom(reason));
    } finally {
      setConnecting(false);
    }
  }

  async function runNow() {
    setRunning(true);
    setError("");
    setProgress("Starting…");
    try {
      const result = await invoke<CleanupResult>("run_cleanup");
      setMatches(result.matches);
      await refresh();
    } catch (reason) {
      setError(messageFrom(reason));
    } finally {
      setRunning(false);
    }
  }

  async function toggleDeleteMode(enabled: boolean) {
    if (enabled) {
      const confirmed = window.confirm(
        "Enable deletion mode? Matching messages will be moved from Junk Email to Deleted Items. They will not be permanently purged.",
      );
      if (!confirmed) return;
    }
    const next = { ...settings, dryRun: !enabled };
    setSettings(next);
    await saveSettings(next);
  }

  async function disconnect() {
    if (!window.confirm("Disconnect this Microsoft account? Scheduled cleanup will no longer work until you reconnect.")) {
      return;
    }
    try {
      await invoke("sign_out");
      await refresh();
    } catch (reason) {
      setError(messageFrom(reason));
    }
  }

  if (!appState) {
    return <main className="loading">Opening Daily Email Cleanout…</main>;
  }

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">✦</div>
          <div>
            <h1>Daily Email Cleanout</h1>
            <p>Quietly clears one persistent kind of Outlook junk.</p>
          </div>
        </div>
        <button className="ghost-button" onClick={() => setShowSettings((value) => !value)}>
          {showSettings ? "Dashboard" : "Settings"}
        </button>
      </header>

      {error && (
        <div className="alert" role="alert">
          <strong>Needs attention</strong>
          <span>{error}</span>
          <button onClick={() => setError("")}>Dismiss</button>
        </div>
      )}

      {!configured || !appState.signedIn ? (
        <section className="setup-card">
          <span className="eyebrow">First-time setup</span>
          <h2>Connect your personal Microsoft account</h2>
          <p>
            Enter the Application (client) ID from your Microsoft Entra app
            registration. Authorization opens in your browser; your password is
            never stored here.
          </p>
          <form onSubmit={connect}>
            <label htmlFor="client-id">Microsoft Application Client ID</label>
            <input
              id="client-id"
              value={settings.clientId}
              onChange={(event) =>
                setSettings({ ...settings, clientId: event.currentTarget.value.trim() })
              }
              placeholder="00000000-0000-0000-0000-000000000000"
              spellCheck={false}
              required
            />
            <button className="primary-button" type="submit" disabled={connecting}>
              {connecting ? "Waiting for Microsoft…" : "Connect Microsoft Account"}
            </button>
          </form>
          {prompt && (
            <div className="device-code">
              <span>Enter this code at Microsoft</span>
              <strong>{prompt.userCode}</strong>
              <button
                className="secondary-button"
                onClick={() => navigator.clipboard.writeText(prompt.userCode)}
              >
                Copy code
              </button>
              <p>{prompt.message}</p>
            </div>
          )}
        </section>
      ) : showSettings ? (
        <section className="settings-layout">
          <div>
            <span className="eyebrow">Preferences</span>
            <h2>Local automation</h2>
            <p>Settings stay on this Mac. The sender rule is intentionally fixed.</p>
          </div>

          <div className="settings-card">
            <div className="setting-row">
              <div>
                <strong>Sender prefix</strong>
                <p>Only full sender addresses beginning with this value match.</p>
              </div>
              <code>{appState.fixedPrefix}</code>
            </div>

            <div className="setting-row">
              <div>
                <strong>Deletion mode</strong>
                <p>Dry run reports matches without moving mail.</p>
              </div>
              <label className="switch-label">
                <input
                  type="checkbox"
                  checked={!settings.dryRun}
                  onChange={(event) => void toggleDeleteMode(event.currentTarget.checked)}
                />
                <span>{settings.dryRun ? "Dry run" : "Move matches"}</span>
              </label>
            </div>

            <div className="setting-row">
              <div>
                <strong>Daily schedule</strong>
                <p>Runs locally through macOS even when this window is closed.</p>
              </div>
              <input
                type="checkbox"
                checked={settings.scheduleEnabled}
                onChange={(event) =>
                  setSettings({ ...settings, scheduleEnabled: event.currentTarget.checked })
                }
              />
            </div>

            <div className="setting-row">
              <div>
                <strong>Cleanup time</strong>
                <p>Uses this Mac’s current local timezone.</p>
              </div>
              <input
                type="time"
                value={timeValue}
                onChange={(event) => {
                  const [hour, minute] = event.currentTarget.value.split(":").map(Number);
                  setSettings({ ...settings, runHour: hour, runMinute: minute });
                }}
              />
            </div>

            <div className="setting-row">
              <div>
                <strong>Run at login</strong>
                <p>Performs a catch-up scan when your macOS session starts.</p>
              </div>
              <input
                type="checkbox"
                checked={settings.runAtLogin}
                onChange={(event) =>
                  setSettings({ ...settings, runAtLogin: event.currentTarget.checked })
                }
              />
            </div>

            <div className="setting-row">
              <div>
                <strong>Run when the app opens</strong>
                <p>Performs a catch-up scan whenever you launch this window.</p>
              </div>
              <input
                type="checkbox"
                checked={settings.runOnStartup}
                onChange={(event) =>
                  setSettings({ ...settings, runOnStartup: event.currentTarget.checked })
                }
              />
            </div>

            <div className="settings-actions">
              <button
                className="primary-button"
                disabled={saving}
                onClick={() => void saveSettings(settings, true)}
              >
                {saving ? "Saving…" : "Save & Apply Schedule"}
              </button>
              <button className="danger-button" onClick={() => void disconnect()}>
                Disconnect Microsoft
              </button>
            </div>
          </div>
        </section>
      ) : (
        <>
          <section className="hero-grid">
            <div className="hero-card">
              <div className="status-line">
                <span className={`status-dot ${appState.signedIn ? "online" : ""}`} />
                {settings.accountEmail}
              </div>
              <span className="eyebrow">Protected pattern</span>
              <h2><code>{appState.fixedPrefix}</code></h2>
              <p>
                Every run scans the complete Junk Email backlog, snapshots all
                matches, then moves them safely to Deleted Items.
              </p>
              <div className="mode-pill">
                {settings.dryRun ? "Dry run — nothing will move" : "Delete mode — matches move to Deleted Items"}
              </div>
              <button className="run-button" disabled={running} onClick={() => void runNow()}>
                <span>{running ? "Working…" : "Run cleanup now"}</span>
                <small>{running ? progress : "Scan the complete Junk Email folder"}</small>
              </button>
            </div>

            <div className="summary-stack">
              <article className="summary-card">
                <span>Next automatic run</span>
                <strong>
                  {appState.schedule.loaded
                    ? appState.schedule.nextRunDescription
                    : "Schedule not installed"}
                </strong>
                <small>
                  {appState.schedule.loaded
                    ? "The local helper can run with this window closed."
                    : "Open Settings and apply the schedule after installing the app."}
                </small>
              </article>
              <article className="summary-card">
                <span>Last cleanup</span>
                <strong>{latest ? formatDate(latest.finishedAt) : "No runs yet"}</strong>
                <small>
                  {latest?.result
                    ? `${latest.result.matched} matched · ${latest.result.deleted} moved · ${latest.result.failed} failed`
                    : latest?.error ?? "Ready for the first scan."}
                </small>
              </article>
            </div>
          </section>

          <section className="activity-card">
            <div className="section-heading">
              <div>
                <span className="eyebrow">Current activity</span>
                <h2>{progress}</h2>
              </div>
              <span className="count-badge">{matches.length} matches shown</span>
            </div>
            {matches.length === 0 ? (
              <div className="empty-state">
                <span>◎</span>
                <p>Run a cleanup to review matching messages here.</p>
              </div>
            ) : (
              <div className="match-list">
                {matches.map((match) => (
                  <article key={match.id}>
                    <div>
                      <strong>{match.subject}</strong>
                      <span>{match.sender}</span>
                    </div>
                    <time>{formatDate(match.receivedAt)}</time>
                  </article>
                ))}
              </div>
            )}
          </section>
        </>
      )}
    </main>
  );
}

export default App;
