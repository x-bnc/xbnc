import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from "react";
import { useNavigate, useParams, Link } from "react-router-dom";
import { Hash, KeyRound, Lock, LockOpen, Plug, PlugZap, RefreshCw, Trash2, Users as UsersIcon, MonitorSmartphone, Crown } from "lucide-react";
import { api, errorText, type Channel, type IrcListener, type Network } from "../lib/api";
import { useMe } from "../lib/session";
import { useToast } from "../lib/toast";
import { ago, colorFor, duration, formatDate } from "../lib/format";
import { Callout, Confirm, CopyText, Empty, Field, Loading, Nick, StatePill, Switch, Tabs } from "../components/ui";
import { HistoryView } from "../components/HistoryView";
import { t, tk, tx } from "../lib/i18n";
import { ModulesTab } from "../components/Modules";

type Tab = "overview" | "channels" | "devices" | "history" | "modules" | "settings";

/** Plain-language explanation for common upstream errors. */
function errorHint(err: string): string | null {
  const e = err.toLowerCase();
  if (e.includes("throttl") || e.includes("too fast"))
    return t("The network is throttling repeated reconnections from your IP. xBNC will wait a few minutes before trying again; do not press Connect several times, each attempt makes the wait longer.");
  if (e.includes("certificate"))
    return t("The server certificate is not valid or the port does not use TLS. Check the server and that the port is the TLS one (usually 6697), or turn TLS off if the network does not offer it.");
  if (e.includes("registration timed out"))
    return t("The server did not answer the registration: usually a wrong port (TLS against a plain-text port or the other way round).");
  if (e.includes("sasl"))
    return t("Check the SASL account and password in Configuration, or turn SASL off if the network does not support it.");
  if (e.includes("k-lined") || e.includes("g-lined") || e.includes("banned"))
    return t("The network has banned this connection. Check the network's rules or contact its administrators.");
  return null;
}

type ClientId = "hexchat" | "mirc" | "weechat" | "irssi";
const CLIENTS: { value: ClientId; label: string }[] = [
  { value: "hexchat", label: "HexChat" },
  { value: "mirc", label: "mIRC" },
  { value: "weechat", label: "WeeChat" },
  { value: "irssi", label: "irssi" },
];

/** Loopback, private, CGNAT and link-local addresses: only reachable inside a network. */
function isLocalHost(h: string): boolean {
  const v = h.replace(/^\[|\]$/g, "").toLowerCase();
  return (
    v === "localhost" ||
    /^127\./.test(v) ||
    /^10\./.test(v) ||
    /^192\.168\./.test(v) ||
    /^172\.(1[6-9]|2\d|3[01])\./.test(v) ||
    /^100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\./.test(v) ||
    /^169\.254\./.test(v) ||
    v === "::1" ||
    /^f[cd][0-9a-f]{2}:/.test(v) ||
    /^fe80:/.test(v)
  );
}

/** A name others can use to reach this machine (not "xbnc" or a bare machine name). */
function isAddress(h: string): boolean {
  return h.includes(".") || h.includes(":");
}

type Route = { host: string; port: string; tls: boolean; invalid: boolean };
type Where = "internet" | "local";

/** How to point an IRC client at this bouncer, with the real address, port and login. */
export function ConnectHelp({ network, compact }: { network: string; compact?: boolean }) {
  const me = useMe();
  const [client, setClient] = useState<ClientId>("hexchat");
  const [device, setDevice] = useState<string>("hexchat");
  const [deviceEdited, setDeviceEdited] = useState(false);
  const [password, setPassword] = useState("");

  const info = me.irc_connect;
  const listeners = info?.listeners ?? me.irc;
  const pageHost = location.hostname;
  const publicHost = info?.public_host ?? (isAddress(pageHost) && !isLocalHost(pageHost) ? pageHost : null);
  // The machine itself (127.0.0.1, localhost) is no address for other devices.
  const loopback = /^(127\.|localhost$|::1$|\[::1\]$)/.test(pageHost);
  const localHost = info?.local_host ?? (isLocalHost(pageHost) && !loopback ? pageHost : null);
  const trusted = (l: IrcListener | undefined, host: string) => !!l?.trusted_names?.includes(host);
  const wrap = (h: string) => (h.includes(":") ? `[${h}]` : h);

  // From anywhere: the listener open to the internet, TLS if there is one.
  const open = listeners.find((l) => l.tls && !l.lan_only) ?? listeners.find((l) => !l.lan_only);
  // At home: a plain-text port needs no certificate at all.
  const plain = listeners.find((l) => !l.tls);
  const anyTls = listeners.find((l) => l.tls);

  const routes: Partial<Record<Where, Route>> = {};
  if (publicHost && open && !isLocalHost(publicHost)) {
    routes.internet = { host: publicHost, port: open.port, tls: open.tls, invalid: open.tls && !trusted(open, publicHost) };
  }
  const homeHost = localHost ?? (publicHost && isLocalHost(publicHost) ? publicHost : null);
  if (homeHost) {
    const l = plain ?? anyTls;
    if (l) routes.local = { host: homeHost, port: l.port, tls: l.tls, invalid: l.tls && !trusted(l, homeHost) };
  }
  const choices = (Object.keys(routes) as Where[]).filter((w) => routes[w]);
  const preferred: Where = isLocalHost(pageHost) && routes.local ? "local" : routes.internet ? "internet" : "local";
  const [where, setWhere] = useState<Where>(preferred);
  const route: Route | undefined = routes[where] ?? routes[choices[0]];

  const user = me.user.username;
  const dev = device.trim().replace(/[^A-Za-z0-9_-]/g, "") || "default";
  const login = `${user}/${network}@${dev}`;
  const full = `${login}:${password}`;
  const shown = password ? `${login}:${"•".repeat(Math.min(password.length, 12))}` : `${login}:`;
  const b = (text: string) => <strong>{text}</strong>;
  const pickClient = (c: ClientId) => {
    setClient(c);
    if (!deviceEdited) setDevice(c);
  };

  if (!route) {
    return (
      <section className="panel stack">
        <h2>{t("Connect your IRC client")}</h2>
        <Callout kind="warn">
          {t("This bouncer has no port for IRC clients. An administrator must add one to [[listen]] in xbnc.toml (sudo xbncd info shows the current ones).")}
        </Callout>
      </section>
    );
  }
  const { host: rawHost, port, tls, invalid } = route;
  const host = wrap(rawHost);
  // The server password with the real password only once it is typed.
  const passFor = password ? full : `${login}:${t("YOUR_PASSWORD")}`;
  // What is shown on screen: never the typed password itself.
  const passShown = password ? shown : passFor;

  const steps: Record<ClientId, ReactNode> = {
    hexchat: (
      <ol className="howto">
        <li>{tx("{menu} and call it {name}. Press {edit}.", { menu: b(t("HexChat → Network List → Add")), name: <code>xBNC</code>, edit: b(t("Edit")) })}</li>
        <li>{tx("In {servers} change the line to {address}.", { servers: <em>{t("Servers")}</em>, address: <code>{host}/{port}</code> })}</li>
        <li>
          {!tls
            ? tx("Leave {ssl} unchecked.", { ssl: b(t("Use SSL")) })
            : invalid
              ? tx("Check {ssl} and {invalid}.", { ssl: b(t("Use SSL for all the servers on this network")), invalid: b(t("Accept invalid SSL certificates")) })
              : tx("Check {ssl}.", { ssl: b(t("Use SSL for all the servers on this network")) })}
        </li>
        <li>
          {tx("{method}: {password}. In {field} paste the server password.", {
            method: <em>{t("Login method")}</em>,
            password: b(t("Server password (/PASS password)")),
            field: <em>{t("Password")}</em>,
          })}
        </li>
        <li>{tx("Close it and press {connect}.", { connect: b(t("Connect")) })}</li>
      </ol>
    ),
    mirc: (
      <div className="stack-sm">
        <p className="small muted">
          {tls && invalid
            ? t("Type this in any window and accept the certificate when mIRC asks:")
            : t("Type this in any window:")}
        </p>
        <CopyText text={`/server ${host} ${tls ? "+" : ""}${port} ${passFor}`} display={`/server ${host} ${tls ? "+" : ""}${port} ${passShown}`} />
      </div>
    ),
    weechat: (
      <div className="stack-sm">
        <CopyText text={`/server add xbnc ${host}/${port}${tls ? " -tls" : ""}`} />
        {tls && invalid && <CopyText text="/set irc.server.xbnc.tls_verify off" />}
        <CopyText text={`/set irc.server.xbnc.password "${passFor}"`} display={`/set irc.server.xbnc.password "${passShown}"`} />
        <CopyText text="/connect xbnc" />
      </div>
    ),
    irssi: (
      <div className="stack-sm">
        <CopyText text="/network add xbnc" />
        <CopyText
          text={`/server add${tls ? " -tls" : ""}${tls && invalid ? " -notls_verify" : ""} -network xbnc ${host} ${port} ${passFor}`}
          display={`/server add${tls ? " -tls" : ""}${tls && invalid ? " -notls_verify" : ""} -network xbnc ${host} ${port} ${passShown}`}
        />
        <CopyText text="/connect xbnc" />
      </div>
    ),
  };

  return (
    <section className="panel stack">
      <div>
        <h2>{t("Connect your IRC client")}</h2>
        <p className="muted" style={{ marginTop: 4 }}>
          {t("Create a server in your IRC client with these details. They are the real ones of this bouncer and of your account.")}
        </p>
      </div>
      {choices.length > 1 && (
        <div className="field">
          <label>{t("Where do you connect from?")}</label>
          <Tabs
            value={where}
            onChange={setWhere}
            items={[
              { value: "internet" as Where, label: t("From the internet") },
              { value: "local" as Where, label: t("From your local network") },
            ]}
            label={t("Where do you connect from?")}
          />
        </div>
      )}
      <div className="grid-3">
        <div className="field">
          <label>{t("Server")}</label>
          <CopyText text={host} label={t("Copy server")} />
        </div>
        <div className="field">
          <label>{t("Port")}</label>
          <CopyText text={port} label={t("Copy port")} />
          <span className="hint">{tls ? t("With SSL/TLS on") : t("Without SSL")}</span>
        </div>
        <div className="field">
          <label>{t("User")}</label>
          <CopyText text={user} label={t("Copy username")} />
        </div>
      </div>
      <div className="grid-2">
        <Field
          label={t("Name of this device")}
          hint={t("One name per device (pc, laptop, phone…): each one gets only the messages it has not seen.")}
          value={device}
          maxLength={32}
          onChange={(e) => {
            setDevice(e.target.value);
            setDeviceEdited(true);
          }}
        />
        <Field
          label={t("Your password")}
          hint={t("The one you use in this panel. It is only used to build the line below: it is not sent or saved.")}
          type="password"
          autoComplete="off"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
      </div>
      <div className="field">
        <label>{t("Server password")}</label>
        {password ? (
          <CopyText text={full} display={shown} label={t("Copy server password")} />
        ) : (
          <>
            <div className="cmd"><code>{shown}<em className="muted">{t("your password")}</em></code></div>
            <span className="hint">{t("Write your password above to copy the complete line, or type it yourself after the colon in your client.")}</span>
          </>
        )}
      </div>
      {tls && invalid && (
        <Callout kind="warn">
          {tx("The certificate of this port is not valid for {host} (it is self-signed or made for another name). Turn on {accept} in your client, or accept the warning the first time.", { host: <code>{rawHost}</code>, accept: b(t("accept invalid certificates")) })}
        </Callout>
      )}
      {where === "internet" && routes.local && (
        <p className="small muted">
          {tx("From outside your network, your router must forward port {port} to this machine ({local}). Inside your network the domain may not work if your router does not support it: then use {other}.", { port: <code>{port}</code>, local: <code>{routes.local.host}</code>, other: b(t("From your local network")) })}
        </p>
      )}
      {where === "local" && routes.local && !routes.local.tls && (
        <p className="small muted">
          {tx("Port {port} without SSL only accepts devices on your local network; from anywhere else use the port with SSL.", { port: <code>{routes.local.port}</code> })}
        </p>
      )}
      {!compact && (
        <div className="stack-sm">
          <Tabs value={client} onChange={pickClient} items={CLIENTS} label={t("IRC client")} />
          {steps[client]}
        </div>
      )}
    </section>
  );
}

export function NetworkDetail() {
  const { id } = useParams();
  const nid = Number(id);
  const me = useMe();
  const toast = useToast();
  const nav = useNavigate();
  const [net, setNet] = useState<Network | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("overview");

  const load = useCallback(async () => {
    try {
      setNet(await api.get<Network>(`/networks/${nid}`));
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, [nid]);

  useEffect(() => {
    setNet(null);
    setTab("overview");
    load();
    const timer = setInterval(load, 5000);
    return () => clearInterval(timer);
  }, [load]);

  if (error && !net) return <Callout kind="bad">{error}</Callout>;
  if (!net) return <Loading />;

  const st = net.status;
  const state = net.enabled ? (st?.state ?? "disconnected") : "disabled";
  const canEdit = (net.user_id === me.user.id && me.permissions.manage_own_networks) || me.permissions.manage_all_networks;
  const color = colorFor(net.name);

  const control = async (action: "connect" | "disconnect" | "jump") => {
    try {
      await api.post(`/networks/${nid}/control`, { action });
      toast.ok(action === "disconnect" ? t("Disconnected.") : action === "jump" ? t("Reconnecting…") : t("Connecting…"));
      setTimeout(load, 800);
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  return (
    <div className="page" style={{ ["--net" as string]: color }}>
      <header className="page-head" style={{ borderLeft: `6px solid ${color}`, paddingLeft: 18 }}>
        <div>
          <div className="row">
            <h1>{net.name}</h1>
            <StatePill state={state} />
          </div>
          <p>
            {(() => {
              const server = <span className="mono">{net.host}:{net.port}</span>;
              const nick = st?.nick ? <Nick name={st.nick} /> : null;
              if (net.tls) {
                return nick ? tx("{server} with TLS, connected as {nick}", { server, nick }) : tx("{server} with TLS", { server });
              }
              return nick ? tx("{server} unencrypted, connected as {nick}", { server, nick }) : tx("{server} unencrypted", { server });
            })()}
            {net.owner !== me.user.username && <>. {t("Network of {owner}", { owner: net.owner })}</>}
          </p>
        </div>
        {net.enabled && (
          <div className="row">
            {state === "connected" || state === "connecting" ? (
              <>
                <button className="btn" onClick={() => control("jump")}>
                  <RefreshCw size={17} />{" "}{t("Reconnect")}{" "}</button>
                <button className="btn btn-danger" onClick={() => control("disconnect")}>
                  <Plug size={17} />{" "}{t("Disconnect")}{" "}</button>
              </>
            ) : (
              <button className="btn btn-primary" onClick={() => control("connect")}>
                <PlugZap size={17} />{" "}{t("Connect now")}</button>
            )}
          </div>
        )}
      </header>

      {st?.last_error && state !== "connected" && (
        <Callout kind="warn">
          <strong>{t("Last error:")}</strong> {st.last_error}
          {errorHint(st.last_error) && <div className="small" style={{ marginTop: 6 }}>{errorHint(st.last_error)}</div>}
        </Callout>
      )}
      {!net.enabled && (
        <Callout kind="info">{t("This network is disabled: xBNC will not connect until you enable it in Configuration.")}</Callout>
      )}

      <Tabs<Tab>
        value={tab}
        onChange={setTab}
        label={t("Network sections")}
        items={[
          { value: "overview", label: t("Overview") },
          { value: "channels", label: t("Channels"), count: st?.channels.length },
          { value: "devices", label: t("Devices"), count: st?.clients.length },
          { value: "history", label: t("History") },
          { value: "modules", label: t("Modules") },
          { value: "settings", label: t("Configuration") },
        ]}
      />

      {tab === "overview" && <Overview net={net} />}
      {tab === "channels" && <Channels nid={nid} reload={load} />}
      {tab === "devices" && <Devices net={net} reload={load} />}
      {tab === "history" && <HistoryView nid={nid} />}
      {tab === "modules" && <ModulesTab networkId={nid} editable={canEdit} onReconnect={() => control("jump")} />}
      {tab === "settings" && (canEdit ? <NetworkSettings net={net} onSaved={setNet} onDeleted={() => nav("/networks")} /> : (
        <Callout>{t("An administrator manages this network's configuration. You can connect, disconnect, manage channels and browse the history.")}</Callout>
      ))}
    </div>
  );
}

function Overview({ net }: { net: Network }) {
  const st = net.status;
  return (
    <div className="stack">
      <div className="facts">
        <div className="fact">
          <span>{t("Connected since")}</span>
          <b>{st?.connected_since ? duration((Date.now() - st.connected_since) / 1000) : "—"}</b>
        </div>
        <div className="fact">
          <span>{t("Official name")}</span>
          <b>{st?.network_name ?? "—"}</b>
        </div>
        <div className="fact">
          <span>{t("SASL identification")}</span>
          <b>{net.sasl_username ?? t("No")}</b>
        </div>
        <div className="fact">
          <span>{t("Server capabilities")}</span>
          <b>{st?.caps.length ? st.caps.join(", ") : "—"}</b>
        </div>
      </div>
      <ConnectHelp network={net.name} />
    </div>
  );
}

function Channels({ nid, reload }: { nid: number; reload: () => void }) {
  const toast = useToast();
  const [rows, setRows] = useState<Channel[] | null>(null);
  const [name, setName] = useState("");
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);

  const load = useCallback(() => api.get<Channel[]>(`/networks/${nid}/channels`).then(setRows).catch((e) => toast.error(errorText(e))), [nid]);
  useEffect(() => {
    load();
  }, [load]);

  const add = async (e: FormEvent) => {
    e.preventDefault();
    const n = /^[#&!+]/.test(name.trim()) ? name.trim() : `#${name.trim()}`;
    setBusy(true);
    try {
      await api.post(`/networks/${nid}/channels`, { name: n, key: key || undefined });
      toast.ok(t("Joining {n}.", { n }));
      setName("");
      setKey("");
      setTimeout(() => {
        load();
        reload();
      }, 900);
    } catch (err) {
      toast.error(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const setRetention = async (ch: string, days: number | null) => {
    try {
      await api.put(`/networks/${nid}/channels/${encodeURIComponent(ch)}/history`, { retention_days: days });
      toast.ok(days === -1 ? t("History of {ch} is no longer stored.", { ch }) : t("History of {ch} saved.", { ch }));
      load();
    } catch (err) {
      toast.error(errorText(err));
    }
  };

  const remove = async (ch: string) => {
    try {
      await api.del(`/networks/${nid}/channels/${encodeURIComponent(ch)}`);
      toast.ok(t("You left {ch}.", { ch }));
      setRemoving(null);
      setTimeout(() => {
        load();
        reload();
      }, 700);
    } catch (err) {
      toast.error(errorText(err));
    }
  };

  return (
    <div className="stack">
      <form className="panel row" style={{ alignItems: "flex-end" }} onSubmit={add}>
        <div style={{ flex: 2, minWidth: 180 }}>
          <Field label={t("Join a channel")} value={name} onChange={(e) => setName(e.target.value)} placeholder="#xbnc" mono required />
        </div>
        <div style={{ flex: 1, minWidth: 140 }}>
          <Field label={t("Key (if any)")} type="password" value={key} onChange={(e) => setKey(e.target.value)} autoComplete="off" />
        </div>
        <button className="btn btn-primary" disabled={busy || !name.trim()}>
          <Hash size={17} />{" "}{t("Join and save")}</button>
      </form>

      {!rows ? (
        <Loading />
      ) : rows.length === 0 ? (
        <Empty title={t("No channels")}>{t("Join a channel here or with /join from your client; xBNC will remember it.")}</Empty>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>{t("Channel")}</th>
                <th>{t("State")}</th>
                <th>{t("Users")}</th>
                <th>{t("Topic")}</th>
                <th>{t("History")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((c) => (
                <tr key={c.name}>
                  <td style={{ fontWeight: 700 }}>
                    <span className="row" style={{ gap: 6 }}>
                      {c.name} {c.has_key && <KeyRound size={14} aria-label={t("with a key")} />}
                    </span>
                  </td>
                  <td>
                    {c.joined ? <span className="badge badge-ok">{t("Joined")}</span> : c.disabled ? <span className="badge" title={t("Not joined until /msg *status enablechan")}>{t("Disabled|channel state")}</span> : <span className="badge">{t("Pending")}</span>}{" "}
                    {!c.saved && <span className="badge">{t("Not saved")}</span>}
                  </td>
                  <td>
                    {c.members != null ? (
                      <span className="row" style={{ gap: 12 }}>
                        <span className="row" style={{ gap: 4 }}>
                          <UsersIcon size={14} /> {c.members}
                        </span>
                        {!!c.ops && (
                          <span className="row faint" style={{ gap: 4 }} title={t("Operators")}>
                            <Crown size={14} /> {c.ops}
                          </span>
                        )}
                      </span>
                    ) : (
                      "—"
                    )}
                  </td>
                  <td className="muted" style={{ maxWidth: 380 }}>
                    {c.topic ?? ""}
                  </td>
                  <td>
                    {c.saved ? (
                      <select
                        className="input input-sm"
                        aria-label={t("History of {ch}", { ch: c.name })}
                        value={c.retention_days == null ? "" : String(c.retention_days)}
                        onChange={(e) => setRetention(c.name, e.target.value === "" ? null : Number(e.target.value))}
                      >
                        <option value="">{t("Server setting")}</option>
                        <option value="-1">{t("Do not store")}</option>
                        <option value="1">{t("1 day")}</option>
                        <option value="7">{t("7 days")}</option>
                        <option value="30">{t("30 days")}</option>
                        <option value="90">{t("90 days")}</option>
                        <option value="365">{t("1 year")}</option>
                        <option value="0">{t("Forever")}</option>
                        {c.retention_days != null && ![-1, 0, 1, 7, 30, 90, 365].includes(c.retention_days) && (
                          <option value={String(c.retention_days)}>{t("{n} days", { n: c.retention_days })}</option>
                        )}
                      </select>
                    ) : (
                      "—"
                    )}
                  </td>
                  <td className="actions">
                    <button className="btn btn-sm btn-ghost btn-danger" onClick={() => setRemoving(c.name)}>{t("Leave")}{" "}</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {removing && (
        <Confirm title={t("Leave {removing}", { removing })} confirmLabel={t("Leave the channel")} onClose={() => setRemoving(null)} onConfirm={() => remove(removing)}>
          <p>{t("xBNC will leave the channel and stop joining it when reconnecting. The stored history is kept.")}</p>
        </Confirm>
      )}
    </div>
  );
}

function Devices({ net, reload }: { net: Network; reload: () => void }) {
  const toast = useToast();
  const clients = net.status?.clients ?? [];
  const kick = async (id: number) => {
    try {
      await api.del(`/networks/${net.id}/clients/${id}`);
      toast.ok(t("Device disconnected."));
      setTimeout(reload, 500);
    } catch (e) {
      toast.error(errorText(e));
    }
  };
  if (clients.length === 0) {
    return (
      <Empty title={t("No devices connected")}>{tx("When you open your IRC client with the password {login} it will show up here.", { login: <code>{net.owner}/{net.name}@{t("device")}:…</code> })}</Empty>
    );
  }
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>{t("Device")}</th>
            <th>{t("Address")}</th>
            <th>{t("Connected")}</th>
            <th>{t("Modern features")}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {clients.map((c) => (
            <tr key={c.id}>
              <td style={{ fontWeight: 700 }}>
                <span className="row" style={{ gap: 8 }}>
                  <MonitorSmartphone size={16} /> {c.name}
                </span>
              </td>
              <td className="mono">{c.peer}</td>
              <td title={formatDate(c.connected_at)}>{ago(c.connected_at)}</td>
              <td className="small muted">{c.caps.length ? c.caps.join(", ") : t("None (classic client)")}</td>
              <td className="actions">
                <button className="btn btn-sm btn-ghost btn-danger" onClick={() => kick(c.id)}>{t("Disconnect")}{" "}</button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function NetworkSettings({ net, onSaved, onDeleted }: { net: Network; onSaved: (n: Network) => void; onDeleted: () => void }) {
  const toast = useToast();
  const [f, setF] = useState({
    name: net.name,
    host: net.host,
    port: net.port,
    tls: net.tls,
    nick: net.nick,
    username: net.username,
    realname: net.realname,
    enabled: net.enabled,
    sasl: !!net.sasl_username,
    sasl_username: net.sasl_username ?? "",
    sasl_password: "",
    server_password: "",
    clear_server_password: false,
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const set = <K extends keyof typeof f>(k: K, v: (typeof f)[K]) => setF((o) => ({ ...o, [k]: v }));

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const body: Record<string, unknown> = {
      name: f.name,
      host: f.host,
      port: Number(f.port),
      tls: f.tls,
      nick: f.nick,
      username: f.username,
      realname: f.realname,
      enabled: f.enabled,
    };
    if (!f.sasl) {
      body.sasl_username = "";
      body.sasl_password = "";
    } else {
      body.sasl_username = f.sasl_username;
      if (f.sasl_password) body.sasl_password = f.sasl_password;
      else if (!net.has_sasl_password) {
        setError(t("Enter the SASL account password."));
        setBusy(false);
        return;
      }
    }
    if (f.clear_server_password) body.server_password = "";
    else if (f.server_password) body.server_password = f.server_password;
    try {
      const n = await api.patch<Network>(`/networks/${net.id}`, body);
      onSaved(n);
      setF((o) => ({ ...o, sasl_password: "", server_password: "", clear_server_password: false }));
      toast.ok(t("Changes saved. If you changed the connection, xBNC is reconnecting."));
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    try {
      await api.del(`/networks/${net.id}`);
      toast.ok(t("Network {name} deleted.", { name: net.name }));
      onDeleted();
    } catch (err) {
      toast.error(errorText(err));
    }
  };

  return (
    <div className="stack" style={{ maxWidth: 760 }}>
    <form className="stack" onSubmit={save}>
      {error && <Callout kind="bad">{error}</Callout>}
      <section className="panel stack">
        <h3>{t("Connection")}</h3>
        <Switch checked={f.enabled} onChange={(v) => set("enabled", v)} label={t("Network enabled")} description={t("If you disable it, xBNC disconnects and does not reconnect.")} />
        <div className="grid-2">
          <Field label={t("Short name")} value={f.name} onChange={(e) => set("name", e.target.value)} hint={t("Changing it changes the login: user/name.")} required />
          <Field label={t("Server")} value={f.host} onChange={(e) => set("host", e.target.value.trim())} mono required />
        </div>
        <div className="grid-2">
          <Field label={t("Port")} type="number" min={1} max={65535} value={f.port} onChange={(e) => set("port", Number(e.target.value))} required />
          <div className="field" style={{ justifyContent: "flex-end" }}>
            <Switch checked={f.tls} onChange={(v) => set("tls", v)} label="TLS" description={f.tls ? t("Encrypted connection with a verified certificate.") : t("Unencrypted.")} />
          </div>
        </div>
      </section>
      <section className="panel stack">
        <h3>{t("Identity")}</h3>
        <div className="grid-3">
          <Field label={t("Nick")} value={f.nick} onChange={(e) => set("nick", e.target.value.trim())} required />
          <Field label={t("Ident")} value={f.username} onChange={(e) => set("username", e.target.value.trim())} required />
          <Field label={t("Real name")} value={f.realname} onChange={(e) => set("realname", e.target.value)} />
        </div>
      </section>
      <section className="panel stack">
        <h3>{t("Authentication")}</h3>
        <Switch checked={f.sasl} onChange={(v) => set("sasl", v)} label={t("Identify me with SASL")} />
        {f.sasl && (
          <div className="grid-2">
            <Field label={t("Account")} value={f.sasl_username} onChange={(e) => set("sasl_username", e.target.value)} required autoComplete="off" />
            <Field
              label={t("Account password")}
              type="password"
              value={f.sasl_password}
              onChange={(e) => set("sasl_password", e.target.value)}
              placeholder={net.has_sasl_password ? t("Saved (type to change it)") : ""}
              autoComplete="new-password"
            />
          </div>
        )}
        <div className="grid-2">
          <Field
            label={t("Server password")}
            type="password"
            value={f.server_password}
            onChange={(e) => set("server_password", e.target.value)}
            placeholder={net.has_server_password ? t("Saved (type to change it)") : t("None")}
            disabled={f.clear_server_password}
            autoComplete="new-password"
          />
          {net.has_server_password && (
            <div className="field" style={{ justifyContent: "flex-end" }}>
              <Switch checked={f.clear_server_password} onChange={(v) => set("clear_server_password", v)} label={t("Remove the server password")} />
            </div>
          )}
        </div>
        <p className="small faint row" style={{ gap: 6 }}>
          {net.tls ? <Lock size={14} /> : <LockOpen size={14} />}{" "}{t("Passwords are stored encrypted and never shown.")}</p>
      </section>
      <div className="row row-end">
        <button className="btn btn-primary" disabled={busy}>
          {busy ? t("Saving…") : t("Save changes")}
        </button>
      </div>
    </form>
    <OnConnectEditor nid={net.id} />

      <section className="panel stack" style={{ borderColor: "var(--bad)" }}>
        <div className="spread">
          <div>
            <h3>{t("Delete this network")}</h3>
            <p className="muted small">{t("The network, its channels and all its history are deleted. This cannot be undone.")}</p>
          </div>
          <button type="button" className="btn btn-danger" onClick={() => setDeleting(true)}>
            <Trash2 size={17} />{" "}{t("Delete network")}</button>
        </div>
      </section>
      {deleting && (
        <Confirm title={t("Delete {name}", { name: net.name })} confirmLabel={t("Delete permanently")} typeToConfirm={net.name} onClose={() => setDeleting(false)} onConfirm={remove}>
          <p>{tx("xBNC will disconnect from {host} and delete this network's history. If you only want to pause it, disable it.", { host: <strong>{net.host}</strong> })}</p>
          <p className="small muted">{t("Need the history?")}{" "}<Link to="#" onClick={(e) => { e.preventDefault(); window.open(`/api/v1/networks/${net.id}/history/export`, "_blank"); }}>{t("Download it first")}</Link>.
          </p>
        </Confirm>
      )}
    </div>
  );
}

const ON_CONNECT_EXAMPLES = [
  { label: "Undernet (X)", text: tk("/mode $nick +x\n/msg x@channels.undernet.org login USER PASSWORD") },
  { label: "QuakeNet (Q)", text: tk("/msg Q@CServe.quakenet.org AUTH USER PASSWORD\n/mode $nick +x") },
  { label: "NickServ", text: tk("/msg NickServ IDENTIFY PASSWORD") },
];

/** Commands sent after every connection (services login, user modes). */
function OnConnectEditor({ nid }: { nid: number }) {
  const toast = useToast();
  const [text, setText] = useState<string | null>(null);
  const [saved, setSaved] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .get<{ commands: string }>(`/networks/${nid}/on-connect`)
      .then((r) => {
        setText(r.commands);
        setSaved(r.commands);
      })
      .catch((e) => setError(errorText(e)));
  }, [nid]);

  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (text === null) return;
    setBusy(true);
    setError(null);
    try {
      const r = await api.put<{ commands: string }>(`/networks/${nid}/on-connect`, { commands: text });
      setText(r.commands);
      setSaved(r.commands);
      toast.ok(t("Commands saved. They will be sent on the next connection."));
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="panel stack" onSubmit={save}>
      <div>
        <h3>{t("Commands on connect")}</h3>
        <p className="muted small" style={{ marginTop: 4 }}>{tx("xBNC sends them every time it connects to the network, before joining channels. Use them to identify on networks without SASL (like the X login on Undernet) or to set modes. With {x}, no channel is joined until the network has hidden your IP, so it is never shown. Write them as in your client, one per line; {nick} is replaced with your nick. They are stored encrypted.", { nick: <code>$nick</code>, x: <code>/mode $nick +x</code> })}</p>
      </div>
      {error && <Callout kind="bad">{error}</Callout>}
      {text === null ? (
        !error && <Loading />
      ) : (
        <>
          <label className="sr-only" htmlFor={`oc-${nid}`}>{t("Commands on connect")}</label>
          <textarea
            id={`oc-${nid}`}
            className="input input-mono"
            rows={4}
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={t("/msg x@channels.undernet.org login USER PASSWORD")}
            spellCheck={false}
            autoComplete="off"
          />
          <div className="spread">
            <div className="row" style={{ gap: 6 }}>
              <span className="small faint">{t("Examples:")}</span>
              {ON_CONNECT_EXAMPLES.map((ex) => (
                <button key={ex.label} type="button" className="btn btn-ghost btn-sm" onClick={() => setText((cur) => (cur ? `${cur.trimEnd()}\n${t(ex.text)}` : t(ex.text)))}>
                  {ex.label}
                </button>
              ))}
            </div>
            <button className="btn btn-primary" disabled={busy || text === saved}>
              {busy ? t("Saving…") : t("Save commands")}
            </button>
          </div>
        </>
      )}
    </form>
  );
}
