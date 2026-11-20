import { useEffect, useState } from "react";
import { BellRing, Smartphone, Trash2 } from "lucide-react";
import { api, errorText } from "../lib/api";
import { useToast } from "../lib/toast";
import { formatDate } from "../lib/format";
import { Callout, Field, Switch } from "./ui";
import { t } from "../lib/i18n";

interface NotifySettings {
  enabled: boolean;
  only_when_away: boolean;
  privates: boolean;
  mentions: boolean;
  telegram_token: string;
  telegram_chat: string;
  discord_webhook: string;
  webhook_url: string;
  email: string;
  push: boolean;
}

interface PushDevice {
  endpoint: string;
  label: string;
  created_at: number;
}

interface TestResult {
  channel: string;
  ok: boolean;
  error: string | null;
}

const CHANNEL_NAME: Record<string, string> = {
  telegram: "Telegram",
  discord: "Discord",
  webhook: "Webhook",
  email: "Email",
  push: "Push",
};

function b64ToBytes(b64: string): Uint8Array {
  const s = atob(b64.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat((4 - (b64.length % 4)) % 4));
  return Uint8Array.from(s, (c) => c.charCodeAt(0));
}

function deviceLabel(): string {
  const ua = navigator.userAgent;
  const os = /Android/.test(ua) ? "Android" : /iPhone|iPad/.test(ua) ? "iOS" : /Mac/.test(ua) ? "macOS" : /Windows/.test(ua) ? "Windows" : /Linux/.test(ua) ? "Linux" : "";
  const br = /Firefox/.test(ua) ? "Firefox" : /Edg\//.test(ua) ? "Edge" : /Chrome/.test(ua) ? "Chrome" : /Safari/.test(ua) ? "Safari" : t("Browser");
  return os ? t("{browser} on {os}", { browser: br, os }) : br;
}

export function Notifications() {
  const toast = useToast();
  const [s, setS] = useState<NotifySettings | null>(null);
  const [emailAvailable, setEmailAvailable] = useState(false);
  const [devices, setDevices] = useState<PushDevice[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<TestResult[] | null>(null);
  const [thisDevice, setThisDevice] = useState<string | null>(null);
  const pushSupported = "serviceWorker" in navigator && "PushManager" in window && window.isSecureContext;

  const loadDevices = () => api.get<PushDevice[]>("/me/push").then(setDevices).catch(() => {});
  useEffect(() => {
    api
      .get<{ settings: NotifySettings; email_available: boolean }>("/me/notifications")
      .then((r) => {
        setS(r.settings);
        setEmailAvailable(r.email_available);
      })
      .catch((e) => setError(errorText(e)));
    loadDevices();
    if (pushSupported) {
      navigator.serviceWorker
        .getRegistration("/")
        .then((reg) => reg?.pushManager.getSubscription())
        .then((sub) => setThisDevice(sub?.endpoint ?? null))
        .catch(() => {});
    }
  }, []);

  if (!s) return error ? <Callout kind="bad">{error}</Callout> : null;
  const set = <K extends keyof NotifySettings>(k: K, v: NotifySettings[K]) => setS({ ...s, [k]: v });

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await api.put<{ settings: NotifySettings }>("/me/notifications", s);
      setS(r.settings);
      toast.ok(t("Notifications saved."));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    setBusy(true);
    setResults(null);
    try {
      await api.put("/me/notifications", s);
      const r = await api.post<{ results: TestResult[] }>("/me/notifications/test");
      setResults(r.results);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const enablePush = async () => {
    setBusy(true);
    try {
      const perm = await Notification.requestPermission();
      if (perm !== "granted") {
        toast.error(t("The browser does not allow notifications for this page. Turn them on in the site settings."));
        return;
      }
      const reg = await navigator.serviceWorker.register("/sw.js", { scope: "/" });
      await navigator.serviceWorker.ready;
      const { public_key } = await api.get<{ public_key: string }>("/push/key");
      const sub = await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: b64ToBytes(public_key) as BufferSource });
      const json = sub.toJSON();
      await api.post("/me/push", { endpoint: json.endpoint, keys: json.keys, label: deviceLabel() });
      setThisDevice(sub.endpoint);
      if (!s.push) setS({ ...s, push: true });
      await loadDevices();
      toast.ok(t("This device will get notifications."));
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const removeDevice = async (d: PushDevice) => {
    try {
      await api.del("/me/push", { endpoint: d.endpoint });
      if (d.endpoint === thisDevice) {
        const reg = await navigator.serviceWorker.getRegistration("/");
        const sub = await reg?.pushManager.getSubscription();
        await sub?.unsubscribe();
        setThisDevice(null);
      }
      await loadDevices();
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  return (
    <section className="panel stack">
      <div className="panel-head">
        <div>
          <h2>
            <BellRing size={20} aria-hidden /> {t("Notifications")}
          </h2>
          <p>{t("Tells you about mentions and private messages while you are not connected: on your phone, by Telegram, Discord, e-mail or a web address of your own.")}</p>
        </div>
      </div>
      {error && <Callout kind="bad">{error}</Callout>}
      <Switch checked={s.enabled} onChange={(v) => set("enabled", v)} label={t("Turn on notifications")} />
      <div className="stack-sm" style={{ opacity: s.enabled ? 1 : 0.55 }}>
        <Switch checked={s.only_when_away} onChange={(v) => set("only_when_away", v)} label={t("Only when I have no client connected")} />
        <Switch checked={s.mentions} onChange={(v) => set("mentions", v)} label={t("When someone writes my nick in a channel")} />
        <Switch checked={s.privates} onChange={(v) => set("privates", v)} label={t("When I get a private message")} />
      </div>

      <h3>
        <Smartphone size={18} aria-hidden /> {t("On your phone or computer")}
      </h3>
      <Switch checked={s.push} onChange={(v) => set("push", v)} label={t("Browser notifications on the devices turned on")} />
      {pushSupported ? (
        <div className="row">
          <button className="btn" onClick={enablePush} disabled={busy || Boolean(thisDevice)}>
            {thisDevice ? t("This device already gets notifications") : t("Turn on for this device")}
          </button>
        </div>
      ) : (
        <p className="small muted">{t("This browser does not support notifications, or the panel is not opened over https. On a phone, add the panel to the home screen and open it from there.")}</p>
      )}
      {devices.length > 0 && (
        <ul className="stack-sm" style={{ listStyle: "none", padding: 0 }}>
          {devices.map((d) => (
            <li key={d.endpoint} className="row" style={{ justifyContent: "space-between" }}>
              <span>
                {d.label || t("Device")} <span className="small faint">· {formatDate(d.created_at)}{d.endpoint === thisDevice ? ` · ${t("this one")}` : ""}</span>
              </span>
              <button className="icon-btn" onClick={() => removeDevice(d)} aria-label={t("Remove")}>
                <Trash2 size={16} />
              </button>
            </li>
          ))}
        </ul>
      )}

      <h3>{t("Telegram")}</h3>
      <p className="small muted">{t("Create a bot with @BotFather, write to it once and enter its token and your chat id here (@userinfobot tells you).")}</p>
      <div className="grid-2">
        <Field label={t("Bot token")} value={s.telegram_token} onChange={(e) => set("telegram_token", e.target.value)} autoComplete="off" mono />
        <Field label={t("Chat id")} value={s.telegram_chat} onChange={(e) => set("telegram_chat", e.target.value)} autoComplete="off" mono />
      </div>
      <Field label={t("Discord webhook")} value={s.discord_webhook} onChange={(e) => set("discord_webhook", e.target.value)} placeholder="https://discord.com/api/webhooks/…" mono hint={t("In Discord: Channel settings > Integrations > Webhooks.")} />
      <Field label={t("Your own web address (https)")} value={s.webhook_url} onChange={(e) => set("webhook_url", e.target.value)} placeholder="https://…" mono hint={t("Gets a POST with the notification as JSON.")} />
      <Field
        label={t("E-mail")}
        type="email"
        value={s.email}
        onChange={(e) => set("email", e.target.value)}
        disabled={!emailAvailable && !s.email}
        hint={emailAvailable ? undefined : t("E-mail is not configured on this server ([smtp] section of xbnc.toml).")}
      />
      {results && (
        <ul className="stack-sm" style={{ listStyle: "none", padding: 0 }}>
          {results.length === 0 && <li className="small muted">{t("Nothing is configured yet.")}</li>}
          {results.map((r) => (
            <li key={r.channel}>
              <span className={`pill ${r.ok ? "pill-connected" : "pill-disconnected"}`}>
                <span className="dot" />
                {CHANNEL_NAME[r.channel] ?? r.channel}
              </span>{" "}
              {r.ok ? t("Sent") : r.error}
            </li>
          ))}
        </ul>
      )}
      <div className="row row-end">
        <button className="btn" onClick={test} disabled={busy}>
          {t("Send a test notification")}
        </button>
        <button className="btn btn-primary" onClick={save} disabled={busy}>
          {t("Save notifications")}
        </button>
      </div>
    </section>
  );
}
