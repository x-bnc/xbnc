import { useEffect, useMemo, useState, type FormEvent } from "react";
import { RotateCcw } from "lucide-react";
import { api, errorText, type Settings } from "../lib/api";
import { useToast } from "../lib/toast";
import { useSession } from "../lib/session";
import { UpdatePanel } from "../components/Updates";
import { Backups } from "../components/Backups";
import { bytes, duration, formatDate } from "../lib/format";
import { Callout, Field, Loading, Switch } from "../components/ui";
import { getLangs, locale, setLang, t, tk, tx } from "../lib/i18n";

interface SettingsResp {
  current: Settings;
  defaults: Settings;
  editable: boolean;
}

interface SystemInfo {
  version: string;
  hostname: string;
  started_at: number;
  uptime_seconds: number;
  os: string | null;
  arch: string;
  database: { path: string; size_bytes: number; schema_version: number; messages: number; networks: number };
  irc_listeners: { address: string; tls: boolean }[];
  web: { address: string; tls: boolean; public_url: string | null; session_hours: number };
  rss_bytes: number | null;
}

type Section = keyof Settings;
interface Def {
  section: Section;
  key: string;
  label: string;
  help: string;
  unit?: "minutes" | "seconds";
  min: number;
  max: number;
}

const FIELDS: { title: string; intro: string; items: Def[] }[] = [
  {
    title: tk("History"),
    intro: tk("How much is kept and how much a client gets when it reconnects."),
    items: [
      { section: "history", key: "retention_days", label: tk("Keep messages for (days)"), help: tk("Older messages are deleted every hour. 0 keeps them forever."), min: 0, max: 36500 },
      { section: "history", key: "playback_max", label: tk("Pending messages per reconnection"), help: tk("Most that is resent to a device when it comes back. If there are more, it is told so."), min: 0, max: 100000 },
      { section: "history", key: "new_client_playback", label: tk("Messages for a new device"), help: tk("What a device sees the first time it connects."), min: 0, max: 100000 },
      { section: "history", key: "chathistory_max", label: tk("CHATHISTORY limit"), help: tk("Messages per request for modern clients (Goguma, gamja, Kiwi IRC)."), min: 1, max: 10000 },
    ],
  },
  {
    title: tk("Usage limits"),
    intro: tk("They protect the server from abuse and from clients that do not read."),
    items: [
      { section: "limits", key: "max_connections", label: tk("Simultaneous IRC connections in total"), help: tk("Across all users."), min: 1, max: 1000000 },
      { section: "limits", key: "max_clients_per_user", label: tk("Devices at the same time per user"), help: tk("Can be adjusted per user in Users."), min: 1, max: 10000 },
      { section: "limits", key: "max_networks_per_user", label: tk("Networks per user"), help: tk("Can be adjusted per user in Users."), min: 1, max: 10000 },
      { section: "limits", key: "client_queue", label: tk("Queue per client (lines)"), help: tk("If a client does not read and the queue fills up, it is disconnected and gets the pending messages from the history."), min: 64, max: 1000000 },
    ],
  },
  {
    title: tk("Login protection"),
    intro: tk("Applies to IRC clients and the web panel at the same time."),
    items: [
      { section: "limits", key: "login_max_failures", label: tk("Failed attempts before blocking"), help: tk("Per IP address."), min: 1, max: 1000 },
      { section: "limits", key: "login_window_seconds", label: tk("Counting window (minutes)"), help: tk("Time over which failures are counted."), unit: "minutes", min: 1, max: 1000000 },
      { section: "limits", key: "login_block_seconds", label: tk("Block duration (minutes)"), help: tk("After that the IP can try again."), unit: "minutes", min: 1, max: 1000000 },
      { section: "limits", key: "registration_timeout_seconds", label: tk("Time to complete the login (seconds)"), help: tk("A client that takes longer is disconnected."), unit: "seconds", min: 5, max: 600 },
    ],
  },
  {
    title: tk("Alerts"),
    intro: tk("Warnings when something goes wrong. 0 turns an alert off. Active alerts show on the panel home page."),
    items: [
      { section: "alerts", key: "network_down_minutes", label: tk("Network without connection for (minutes)"), help: tk("A network that keeps trying to connect and fails. Networks you disconnect yourself do not count."), min: 0, max: 10080 },
      { section: "alerts", key: "failed_logins_10min", label: tk("Failed logins in 10 minutes"), help: tk("IRC, panel and API tokens together, from any address."), min: 0, max: 100000 },
      { section: "alerts", key: "memory_mb", label: tk("xBNC memory (MB)"), help: tk("Resident memory of the process."), min: 0, max: 1000000 },
      { section: "alerts", key: "database_mb", label: tk("Database size (MB)"), help: tk("If it gets close to the limit, keep history for fewer days."), min: 0, max: 10000000 },
    ],
  },
];

function getVal(s: Settings, d: Def): number {
  return (s[d.section] as Record<string, number>)[d.key];
}
function toShown(d: Def, v: number) {
  return d.unit === "minutes" ? v / 60 : v;
}
function fromShown(d: Def, v: number) {
  return d.unit === "minutes" ? Math.round(v * 60) : Math.round(v);
}

export function SettingsPage() {
  const toast = useToast();
  const { refresh } = useSession();
  const [data, setData] = useState<SettingsResp | null>(null);
  const [draft, setDraft] = useState<Settings | null>(null);
  const [sys, setSys] = useState<SystemInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .get<SettingsResp>("/settings")
      .then((d) => {
        setData(d);
        setDraft(structuredClone(d.current));
      })
      .catch((e) => setError(errorText(e)));
    api.get<SystemInfo>("/system").then(setSys).catch(() => {});
  }, []);

  const dirty = useMemo(() => data && draft && JSON.stringify(data.current) !== JSON.stringify(draft), [data, draft]);

  if (error && !data) return <Callout kind="bad">{error}</Callout>;
  if (!data || !draft) return <Loading />;

  const setVal = (d: Def, shown: number) => {
    setDraft((o) => {
      if (!o) return o;
      const n = structuredClone(o);
      (n[d.section] as Record<string, number>)[d.key] = fromShown(d, shown);
      return n;
    });
  };

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const r = await api.put<{ current: Settings }>("/settings", draft);
      setData({ ...data, current: r.current });
      setDraft(structuredClone(r.current));
      toast.ok(t("Settings saved and applied."));
      if (r.current.general.language !== data.current.general.language) {
        // The panel follows the new default unless the account has its own language.
        await refresh();
      }
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  // The default language is saved and applied at once (no need to press
  // Save). The person changing it follows it too: otherwise a language
  // chosen at sign-in would keep the panel in the old one.
  const changeLanguage = async (code: string) => {
    if (!data.editable || busy) return;
    setBusy(true);
    setError(null);
    try {
      const next = structuredClone(data.current);
      next.general.language = code;
      const r = await api.put<{ current: Settings }>("/settings", next);
      setData({ ...data, current: r.current });
      setDraft((d) => (d ? { ...d, general: { ...d.general, language: r.current.general.language } } : d));
      const mine = await api.put<{ effective: string }>("/me/language", { language: "" });
      await refresh();
      setLang(mine.effective);
      toast.ok(t("Language saved."));
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>{t("Settings")}</h1>
          <p>{tx("Changes apply immediately, without a restart. Initial values come from {file}.", { file: <code>xbnc.toml</code> })}</p>
        </div>
      </header>
      {!data.editable && <Callout>{t("Only a superadministrator can change these settings. You can view them.")}</Callout>}
      {error && <Callout kind="bad">{error}</Callout>}

      <form className="stack" onSubmit={save}>
        <section className="panel">
          <div className="panel-head">
            <div>
              <h2>{t("Language")}</h2>
              <p>{t("Language of xBNC's messages and of the panel for people who have not chosen another one in Your account. It applies as soon as you click it, to you as well.")}</p>
            </div>
          </div>
          <div className="seg" role="radiogroup" aria-label={t("Default language")}>
            {getLangs().map((l) => (
              <button
                key={l.code}
                type="button"
                role="radio"
                lang={l.code}
                aria-checked={draft.general.language === l.code}
                className={draft.general.language === l.code ? "on" : ""}
                disabled={!data.editable || busy}
                onClick={() => changeLanguage(l.code)}
              >
                {l.name}
              </button>
            ))}
          </div>
          <p className="muted small" style={{ marginTop: 10 }}>{tx("More languages are added as files in {dir} (see TRANSLATING.md) and appear here after a reload.", { dir: <code>/etc/xbnc/locales</code> })}</p>
        </section>
        {FIELDS.map((group) => (
          <section className="panel" key={group.title}>
            <div className="panel-head">
              <div>
                <h2>{t(group.title)}</h2>
                <p>{t(group.intro)}</p>
              </div>
            </div>
            <div className="grid-2">
              {group.items.map((d) => {
                const v = getVal(draft, d);
                const def = getVal(data.defaults, d);
                const id = `s-${d.key}`;
                const invalid = !Number.isFinite(v) || toShown(d, v) < d.min || toShown(d, v) > d.max;
                return (
                  <div className="field" key={d.key}>
                    <label htmlFor={id}>{t(d.label)}</label>
                    <div className="row" style={{ flexWrap: "nowrap" }}>
                      <input
                        id={id}
                        className="input"
                        type="number"
                        min={d.min}
                        max={d.max}
                        value={toShown(d, v)}
                        disabled={!data.editable}
                        aria-invalid={invalid || undefined}
                        onChange={(e) => setVal(d, Number(e.target.value))}
                      />
                      {data.editable && v !== def && (
                        <button type="button" className="icon-btn" title={t("Back to {value}", { value: toShown(d, def) })} aria-label={t("Back to the file's value: {value}", { value: toShown(d, def) })} onClick={() => setVal(d, toShown(d, def))}>
                          <RotateCcw size={16} />
                        </button>
                      )}
                    </div>
                    <span className={invalid ? "error" : "hint"}>{invalid ? t("Must be between {min} and {max}.", { min: d.min, max: d.max }) : t(d.help)}</span>
                  </div>
                );
              })}
            </div>
          </section>
        ))}
        <section className="panel stack-sm">
          <div className="panel-head">
            <div>
              <h2>{t("Alert notifications")}</h2>
              <p>{t("Every alert is written to the audit log. It can also be sent over IRC and to a webhook.")}</p>
            </div>
          </div>
          <Switch
            checked={draft.alerts.notify_irc}
            disabled={!data.editable}
            onChange={(v) => setDraft({ ...draft, alerts: { ...draft.alerts, notify_irc: v } })}
            label={t("Tell administrators over IRC")}
            description={t("A *status notice in the connected clients of every administrator and superadministrator.")}
          />
          <Field
            label={t("Webhook")}
            type="url"
            placeholder="https://…"
            value={draft.alerts.webhook_url}
            disabled={!data.editable}
            maxLength={500}
            onChange={(e) => setDraft({ ...draft, alerts: { ...draft.alerts, webhook_url: e.target.value.trim() } })}
            hint={tx("xBNC sends a JSON POST when each alert starts and when it is resolved. It includes the {text} and {content} fields, so it works directly with Slack, Discord or Mattermost.", { text: <code>text</code>, content: <code>content</code> })}
          />
        </section>
        {data.editable && dirty && (
          <div className="savebar" role="region" aria-label={t("Unsaved changes")}>
            <span>{t("You have unsaved changes.")}</span>
            <div className="row">
              <button type="button" className="btn btn-ghost" onClick={() => setDraft(structuredClone(data.current))}>{t("Discard")}{" "}</button>
              <button className="btn btn-primary" disabled={busy}>
                {busy ? t("Saving…") : t("Save settings")}
              </button>
            </div>
          </div>
        )}
      </form>

      {data.editable && (
        <Backups
          onSettings={(c) => {
            setData({ ...data, current: { ...data.current, backups: c.backups } });
            setDraft((d) => (d ? { ...d, backups: c.backups } : d));
          }}
        />
      )}

      {sys && (
        <section className="section">
          <h2>{t("Server")}</h2>
          <div className="facts">
            <div className="fact">
              <span>{t("Version")}</span>
              <b>xBNC {sys.version}</b>
            </div>
            <div className="fact">
              <span>{t("Server name")}</span>
              <b>{sys.hostname}</b>
            </div>
            <div className="fact">
              <span>{t("Active since")}</span>
              <b title={formatDate(sys.started_at)}>{duration(sys.uptime_seconds)}</b>
            </div>
            <div className="fact">
              <span>{t("System")}</span>
              <b>
                {sys.os ?? t("Linux")} ({sys.arch})
              </b>
            </div>
            <div className="fact">
              <span>{t("Memory")}</span>
              <b>{bytes(sys.rss_bytes)}</b>
            </div>
            <div className="fact">
              <span>{t("Database")}</span>
              <b>{bytes(sys.database.size_bytes)}</b>
            </div>
            <div className="fact">
              <span>{t("Stored messages")}</span>
              <b>{sys.database.messages.toLocaleString(locale())}</b>
            </div>
            <div className="fact">
              <span>{t("Schema version")}</span>
              <b>{sys.database.schema_version}</b>
            </div>
          </div>
          <UpdatePanel />
          <div className="panel stack-sm">
            <strong>{t("Configured in xbnc.toml")}</strong>
            <p className="small muted">{tx("These options are read at startup and need {cmd}: server name, listening ports, certificates, database path, secret key, web panel and log level.", { cmd: <code>{t("systemctl restart xbnc")}</code> })}</p>
            <ul className="small" style={{ margin: 0, paddingLeft: 18 }}>
              {sys.irc_listeners.map((l) => (
                <li key={l.address}>{tx(l.tls ? "IRC on {address} with TLS" : "IRC on {address} without TLS", { address: <code>{l.address}</code> })}</li>
              ))}
              <li>{tx(sys.web.tls ? "Panel on {address} with TLS, {hours} h sessions" : "Panel on {address} without TLS (behind a proxy), {hours} h sessions", { address: <code>{sys.web.address}</code>, hours: sys.web.session_hours })}</li>
              <li>{tx("Database at {path}", { path: <code>{sys.database.path}</code> })}</li>
            </ul>
          </div>
        </section>
      )}
    </div>
  );
}
