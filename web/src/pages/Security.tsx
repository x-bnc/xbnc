import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { api, errorText } from "../lib/api";
import { useMe } from "../lib/session";
import { useToast } from "../lib/toast";
import { ago, duration, formatDate } from "../lib/format";
import { Empty, Loading, Switch, Tabs } from "../components/ui";
import { t, tk } from "../lib/i18n";

interface LoginEvent {
  ts: number;
  ip: string;
  username: string;
  source: string;
  success: boolean;
  reason: string;
}
interface Overview {
  blocked: { ip: string; remaining_seconds: number }[];
  failed_logins_24h: number;
  policy: { max_failures: number; window_seconds: number; block_seconds: number; min_password_length: number };
  events: LoginEvent[];
}
export interface WebSession {
  id: number;
  username: string;
  ip: string;
  user_agent: string;
  created_at: number;
  last_seen: number;
  expires_at: number;
  current: boolean;
}
interface AuditRow {
  ts: number;
  actor: string;
  action: string;
  detail: string;
}

type Tab = "logins" | "blocked" | "sessions" | "audit";

const REASONS: Record<string, string> = {
  ok: tk("Success"),
  "bad password": tk("Wrong password"),
  "bad password (SASL)": tk("Wrong password (SASL)"),
  "bad 2FA code": tk("Wrong 2FA code"),
  "account disabled": tk("Account suspended"),
};

/** Short, readable description of a browser user agent. */
export function describeAgent(ua: string): string {
  if (!ua) return t("Unknown");
  const browser = /Firefox\//.test(ua) ? t("Firefox") : /Edg\//.test(ua) ? t("Edge") : /Chrome\//.test(ua) ? t("Chrome") : /Safari\//.test(ua) ? t("Safari") : t("Browser");
  const os = /Windows/.test(ua) ? t("Windows") : /Android/.test(ua) ? t("Android") : /iPhone|iPad/.test(ua) ? "iOS" : /Mac OS/.test(ua) ? "macOS" : /Linux/.test(ua) ? t("Linux") : "";
  return os ? t("{browser} on {os}", { browser, os }) : browser;
}

export function SessionsTable({ all }: { all: boolean }) {
  const toast = useToast();
  const [rows, setRows] = useState<WebSession[] | null>(null);
  const load = useCallback(() => api.get<WebSession[]>(`/sessions${all ? "?all=true" : ""}`).then(setRows).catch((e) => toast.error(errorText(e))), [all]);
  useEffect(() => {
    load();
  }, [load]);
  const revoke = async (s: WebSession) => {
    try {
      await api.del(`/sessions/${s.id}`);
      toast.ok(t("Session closed."));
      if (s.current) location.reload();
      else load();
    } catch (e) {
      toast.error(errorText(e));
    }
  };
  if (!rows) return <Loading />;
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            {all && <th>{t("User")}</th>}
            <th>{t("Device")}</th>
            <th>IP</th>
            <th>{t("Last activity")}</th>
            <th>{t("Started")}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {rows.map((s) => (
            <tr key={s.id}>
              {all && <td style={{ fontWeight: 700 }}>{s.username}</td>}
              <td>
                {describeAgent(s.user_agent)} {s.current && <span className="badge badge-accent">{t("This session")}</span>}
              </td>
              <td className="mono">{s.ip}</td>
              <td>{ago(s.last_seen)}</td>
              <td>{formatDate(s.created_at)}</td>
              <td className="actions">
                <button className="btn btn-sm btn-ghost btn-danger" onClick={() => revoke(s)}>{t("Sign out")}</button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function SecurityPage() {
  const me = useMe();
  const toast = useToast();
  const [tab, setTab] = useState<Tab>("logins");
  const [onlyFailures, setOnlyFailures] = useState(false);
  const [data, setData] = useState<Overview | null>(null);
  const [audit, setAudit] = useState<AuditRow[] | null>(null);

  const load = useCallback(() => {
    api.get<Overview>(`/security?limit=200${onlyFailures ? "&failures=true" : ""}`).then(setData).catch((e) => toast.error(errorText(e)));
  }, [onlyFailures]);
  useEffect(() => {
    load();
  }, [load]);
  useEffect(() => {
    if (tab === "audit" && !audit) api.get<AuditRow[]>("/audit?limit=500").then(setAudit).catch((e) => toast.error(errorText(e)));
  }, [tab, audit]);

  const unblock = async (ip: string) => {
    try {
      await api.del(`/security/blocked/${encodeURIComponent(ip)}`);
      toast.ok(`${ip} desbloqueada.`);
      load();
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  const p = data?.policy;
  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>{t("Security")}</h1>
          <p>
            {p
              ? t("An IP that fails {max_failures} times within {duration} is blocked for {duration2}, both on IRC and in the panel.", { max_failures: p.max_failures, duration: duration(p.window_seconds), duration2: duration(p.block_seconds) })
              : t("Logins, blocks and administrative activity.")}{" "}
            {me.permissions.edit_settings && <Link to="/settings">{t("Change the policy")}</Link>}
          </p>
        </div>
        {data && (
          <div className="row">
            <span className={`pill ${data.failed_logins_24h ? "pill-reconnecting" : "pill-connected"}`}>{t("{n} failures in 24 h", { n: data.failed_logins_24h })}</span>
            <span className={`pill ${data.blocked.length ? "pill-disconnected" : ""}`}>{t("{n} blocked IPs", { n: data.blocked.length })}</span>
          </div>
        )}
      </header>

      <Tabs<Tab>
        value={tab}
        onChange={setTab}
        label={t("Security sections")}
        items={[
          { value: "logins", label: t("Login attempts") },
          { value: "blocked", label: t("Blocked IPs"), count: data?.blocked.length },
          { value: "sessions", label: t("Panel sessions") },
          { value: "audit", label: t("Audit") },
        ]}
      />

      {tab === "logins" &&
        (!data ? (
          <Loading />
        ) : (
          <div className="stack">
            <Switch checked={onlyFailures} onChange={setOnlyFailures} label={t("Failed only")} />
            {data.events.length === 0 ? (
              <Empty title={t("No attempts recorded")} />
            ) : (
              <div className="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>{t("When")}</th>
                      <th>{t("Result")}</th>
                      <th>{t("User")}</th>
                      <th>{t("From")}</th>
                      <th>IP</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.events.map((e, i) => (
                      <tr key={i}>
                        <td title={formatDate(e.ts)}>{ago(e.ts)}</td>
                        <td>
                          <span className={`badge ${e.success ? "badge-ok" : "badge-bad"}`}>{REASONS[e.reason] ? t(REASONS[e.reason]) : e.reason}</span>
                        </td>
                        <td>{e.username || <span className="faint">{t("empty")}</span>}</td>
                        <td>{e.source === "web" ? t("Web panel") : t("IRC client")}</td>
                        <td className="mono">{e.ip}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        ))}

      {tab === "blocked" &&
        (!data ? (
          <Loading />
        ) : data.blocked.length === 0 ? (
          <Empty title={t("No blocked IPs")}>{t("When someone gets the password wrong too many times, their address will show up here.")}</Empty>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>IP</th>
                  <th>{t("Unblocked in")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {data.blocked.map((b) => (
                  <tr key={b.ip}>
                    <td className="mono">{b.ip}</td>
                    <td>{duration(b.remaining_seconds)}</td>
                    <td className="actions">
                      {me.permissions.manage_security && (
                        <button className="btn btn-sm" onClick={() => unblock(b.ip)}>{t("Unblock")}{" "}</button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ))}

      {tab === "sessions" && <SessionsTable all={me.permissions.manage_security} />}

      {tab === "audit" &&
        (!audit ? (
          <Loading />
        ) : audit.length === 0 ? (
          <Empty title={t("No activity yet")} />
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("When")}</th>
                  <th>{t("Who")}</th>
                  <th>{t("Action")}</th>
                  <th>{t("Detail")}</th>
                </tr>
              </thead>
              <tbody>
                {audit.map((a, i) => (
                  <tr key={i}>
                    <td title={formatDate(a.ts)} style={{ whiteSpace: "nowrap" }}>
                      {ago(a.ts)}
                    </td>
                    <td>{a.actor.replace(/^web:/, "").replace(/^cli:/, "terminal: ")}</td>
                    <td className="mono">{a.action}</td>
                    <td className="muted">{a.detail}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ))}
    </div>
  );
}
