import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Hash, MonitorSmartphone, Plus, AlertTriangle } from "lucide-react";
import { api, type Alert } from "../lib/api";
import { UpdateBanner } from "../components/Updates";
import { useMe } from "../lib/session";
import { bytes, colorFor, duration, formatDay } from "../lib/format";
import { useLiveData } from "../components/Layout";
import { Callout, Empty, Nick, Sparkline, StatePill } from "../components/ui";
import { ConnectHelp } from "./NetworkDetail";
import { locale, t, tx } from "../lib/i18n";

interface Dash {
  scope: "global" | "personal";
  uptime_seconds: number;
  messages_stored: number;
  system?: {
    users: number;
    db_size_bytes: number;
    connections: number;
    irc_clients: number;
    failed_logins_24h: number;
    blocked_ips: number;
  };
}

function greeting() {
  const h = new Date().getHours();
  if (h < 6) return t("Good evening");
  if (h < 14) return t("Good morning");
  if (h < 21) return t("Good afternoon");
  return t("Good evening");
}

export function Dashboard() {
  const me = useMe();
  const { frame, rates, connected } = useLiveData();
  const [dash, setDash] = useState<Dash | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () => api.get<Dash>("/dashboard").then((d) => alive && setDash(d)).catch(() => {});
    load();
    const timer = setInterval(load, 30000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  const all = frame?.networks ?? [];
  const mine = all.filter((n) => n.owner === me.user.username);
  const others = all.filter((n) => n.owner !== me.user.username);
  const up = mine.filter((n) => n.state === "connected").length;
  const sys = frame?.system;
  const last = rates[rates.length - 1];

  const summary =
    mine.length === 0
      ? t("You have no networks yet. Add one and xBNC will stay connected for you.")
      : up === mine.length
        ? mine.length === 1
          ? t("Your network is connected.")
          : t("Your {length} networks are connected.", { length: mine.length })
        : t("{up} of {length} networks connected.", { up, length: mine.length });

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <p className="faint small" style={{ marginTop: 0 }}>
            {formatDay(Date.now())}
          </p>
          <h1>
            {greeting()}, {me.user.username}
          </h1>
          <p>{summary}</p>
        </div>
        {me.permissions.manage_own_networks && (
          <Link to="/networks/new" className="btn btn-primary">
            <Plus size={18} />{" "}{t("Add network")}</Link>
        )}
      </header>

      {me.permissions.view_system && <ActiveAlerts />}
      {me.permissions.view_system && <UpdateBanner />}

      {!connected && frame && <p className="small faint">{t("Reconnecting to the live panel…")}</p>}

      {mine.length === 0 && frame ? (
        <Empty
          title={t("Add your first IRC network")}
          action={
            me.permissions.manage_own_networks ? (
              <Link to="/networks/new" className="btn btn-primary">
                <Plus size={18} />{" "}{t("Choose a network")}</Link>
            ) : undefined
          }
        >
          {me.permissions.manage_own_networks
            ? t("Pick Libera.Chat, OFTC or another well-known network, or type the address of any server. It takes less than a minute.")
            : t("An administrator has to set up your networks. When they do, they will show up here.")}
        </Empty>
      ) : (
        <section className="section" aria-labelledby="h-networks">
          <h2 id="h-networks" className="sr-only">{t("Your networks")}</h2>
          <div className="netboard">
            {mine.map((n) => (
              <Link key={n.id} to={`/networks/${n.id}`} className="netcard" style={{ ["--net" as string]: colorFor(n.name) }}>
                <div className="spread">
                  <span className="netcard-name">{n.name}</span>
                  <StatePill state={n.state} />
                </div>
                <div className="small muted">
                  {n.nick ? (
                    tx("Connected as {nick}", { nick: <Nick name={n.nick} /> })
                  ) : (
                    t("Not connected right now")
                  )}
                </div>
                <div className="netcard-facts">
                  <span>
                    <Hash size={15} /> {n.channels === 1 ? t("1 channel") : t("{count} channels", { count: n.channels })}
                  </span>
                  <span>
                    <MonitorSmartphone size={15} /> {n.clients === 1 ? t("1 device") : t("{count} devices", { count: n.clients })}
                  </span>
                </div>
                {n.last_error && n.state !== "connected" && (
                  <div className="small" style={{ color: "var(--bad)", display: "flex", gap: 6 }}>
                    <AlertTriangle size={15} style={{ flex: "none", marginTop: 3 }} /> {n.last_error}
                  </div>
                )}
              </Link>
            ))}
          </div>
        </section>
      )}

      {mine.length > 0 && <ConnectHelp network={mine[0].name} />}

      {sys && (
        <section className="section" aria-labelledby="h-sistema">
          <div className="spread">
            <h2 id="h-sistema">{t("System")}</h2>
            <span className="small faint">{t("Up for {time}", { time: duration(frame?.uptime_seconds ?? 0) })}</span>
          </div>
          <div className="meters">
            <div className="meter">
              <span className="meter-label">{t("Messages stored per second")}</span>
              <span className="meter-value">{last ? last.messages.toFixed(1) : "0"}</span>
              <Sparkline values={rates.map((r) => r.messages)} label={t("Messages per second over the last two minutes")} />
            </div>
            <div className="meter">
              <span className="meter-label">{t("Incoming traffic")}</span>
              <span className="meter-value">{bytes(last?.traffic_in ?? 0)}/s</span>
              <Sparkline values={rates.map((r) => r.traffic_in)} color="#1A9E5B" label={t("Incoming traffic")} />
            </div>
            <div className="meter">
              <span className="meter-label">{t("Outgoing traffic")}</span>
              <span className="meter-value">{bytes(last?.traffic_out ?? 0)}/s</span>
              <Sparkline values={rates.map((r) => r.traffic_out)} color="#F07800" label={t("Outgoing traffic")} />
            </div>
            <div className="meter">
              <span className="meter-label">{t("Memory and CPU")}</span>
              <span className="meter-value">
                {bytes(sys.rss_bytes)} <span className="small muted">{(sys.cpu_percent ?? 0).toFixed(1)} %</span>
              </span>
              <Sparkline values={rates.map((r) => r.rss)} color="#8E2DB8" label={t("Memory used")} />
            </div>
          </div>
          <div className="facts">
            <div className="fact">
              <span>{t("Users")}</span>
              <b>{dash?.system?.users ?? "—"}</b>
            </div>
            <div className="fact">
              <span>{t("Connected IRC clients")}</span>
              <b>{sys.irc_clients}</b>
            </div>
            <div className="fact">
              <span>{t("Connected networks")}</span>
              <b>
                {all.filter((n) => n.state === "connected").length} / {all.length}
              </b>
            </div>
            <div className="fact">
              <span>{t("Messages in the history")}</span>
              <b>{dash ? dash.messages_stored.toLocaleString(locale()) : "—"}</b>
            </div>
            <div className="fact">
              <span>{t("Database")}</span>
              <b>{bytes(dash?.system?.db_size_bytes)}</b>
            </div>
            <div className="fact">
              <span>{t("Reconnections")}</span>
              <b>{sys.counters.reconnects}</b>
            </div>
            <div className="fact">
              <span>{t("Failed logins (24 h)")}</span>
              <b>{dash?.system?.failed_logins_24h ?? "—"}</b>
            </div>
            <div className="fact">
              <span>{t("Blocked IPs")}</span>
              <b>{sys.blocked_ips}</b>
            </div>
          </div>
          {others.length > 0 && (
            <p className="small muted">{others.length === 1 ? t("There is also 1 network of other users.") : t("There are also {n} networks of other users.", { n: others.length })}{" "}
              <Link to="/networks?all=1">{t("See them all")}</Link>
            </p>
          )}
        </section>
      )}
    </div>
  );
}

function alertText(a: Alert) {
  switch (a.kind) {
    case "network_down":
      return t("Network {name} has been trying to connect for longer than allowed.", { name: a.subject });
    case "failed_logins":
      return t("Many failed logins in the last 10 minutes.");
    case "memory":
      return t("xBNC is using more memory than the configured limit.");
    case "database":
      return t("The database is larger than the configured size.");
    default:
      return a.message;
  }
}

/** Active alerts, refreshed every 30 seconds (the server evaluates them at that pace). */
function ActiveAlerts() {
  const [alerts, setAlerts] = useState<Alert[]>([]);
  useEffect(() => {
    let alive = true;
    const load = () =>
      api
        .get<{ alerts: Alert[] }>("/alerts")
        .then((r) => alive && setAlerts(r.alerts))
        .catch(() => {});
    load();
    const timer = setInterval(load, 30000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);
  if (alerts.length === 0) return null;
  return (
    <div className="stack-sm" role="status" aria-live="polite">
      {alerts.map((a) => (
        <Callout kind="warn" key={`${a.kind}/${a.subject}`}>
          <span>
            <strong>{alertText(a)}</strong>{" "}
            <span className="small muted">
              {t("For {time}.", { time: duration(Math.max(0, Math.round((Date.now() - a.since) / 1000))) })}{" "}
              <Link to="/settings">{t("Adjust alerts")}</Link>
            </span>
          </span>
        </Callout>
      ))}
    </div>
  );
}
