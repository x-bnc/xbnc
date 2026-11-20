import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { Hash, Lock, Plus, X, ShieldCheck } from "lucide-react";
import { api, errorText, type Network, type Preset, type User } from "../lib/api";
import { useMe } from "../lib/session";
import { useToast } from "../lib/toast";
import { colorFor } from "../lib/format";
import { Callout, Empty, Field, Loading, Nick, SelectField, StatePill, Switch } from "../components/ui";
import { t, tk, tx } from "../lib/i18n";

export function Networks() {
  const me = useMe();
  const [params, setParams] = useSearchParams();
  const all = params.get("all") === "1" && me.permissions.manage_all_networks;
  const [nets, setNets] = useState<Network[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setNets(null);
    api
      .get<Network[]>(`/networks${all ? "?all=true" : ""}`)
      .then(setNets)
      .catch((e) => setError(errorText(e)));
  }, [all]);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>{t("Networks")}</h1>
          <p>{t("Each network is a permanent connection to an IRC server. xBNC joins your channels and keeps the messages even when your client is closed.")}</p>
        </div>
        <div className="row">
          {me.permissions.manage_all_networks && (
            <Switch checked={all} onChange={(v) => setParams(v ? { all: "1" } : {})} label={t("All networks")} />
          )}
          {(me.permissions.manage_own_networks || me.permissions.manage_all_networks) && (
            <Link to="/networks/new" className="btn btn-primary">
              <Plus size={18} />{" "}{t("Add network")}</Link>
          )}
        </div>
      </header>
      {error && <Callout kind="bad">{error}</Callout>}
      {!nets ? (
        <Loading />
      ) : nets.length === 0 ? (
        <Empty
          title={t("No networks yet")}
          action={
            me.permissions.manage_own_networks ? (
              <Link to="/networks/new" className="btn btn-primary">
                <Plus size={18} />{" "}{t("Add network")}</Link>
            ) : undefined
          }
        >{t("Add a network so xBNC connects and keeps your session open.")}</Empty>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>{t("Network")}</th>
                {all && <th>{t("User")}</th>}
                <th>{t("State")}</th>
                <th>{t("Server")}</th>
                <th>{t("Nick")}</th>
                <th>{t("Channels")}</th>
                <th>{t("Devices")}</th>
              </tr>
            </thead>
            <tbody>
              {nets.map((n) => (
                <tr key={n.id}>
                  <td>
                    <Link to={`/networks/${n.id}`} className="row" style={{ gap: 10, fontWeight: 700, color: "var(--ink)", textDecoration: "none" }}>
                      <span style={{ width: 5, height: 22, borderRadius: 3, background: colorFor(n.name) }} />
                      {n.name}
                    </Link>
                  </td>
                  {all && <td>{n.owner}</td>}
                  <td>
                    <StatePill state={n.enabled ? (n.status?.state ?? "disconnected") : "disabled"} />
                  </td>
                  <td className="mono">
                    {n.host}:{n.port} {n.tls && <Lock size={13} aria-label="TLS" />}
                  </td>
                  <td>
                    <Nick name={n.status?.nick ?? n.nick} />
                  </td>
                  <td>{n.status?.channels.length ?? 0}</td>
                  <td>{n.status?.clients.length ?? 0}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

// ------------------------------------------------------------- wizard

type Step = 1 | 2 | 3 | 4;

// Descriptions of the known networks (sent by the server in English).
export const PRESET_TEXTS = [
  tk("Free software communities and open source projects"),
  tk("Debian, Tor, the Linux kernel and other projects"),
  tk("One of the oldest networks"),
  tk("Historic network, very popular in Europe"),
  tk("Anime, games and all kinds of communities"),
  tk("Classic network with all kinds of channels"),
  tk("Games and European communities"),
  tk("Social and support communities"),
  tk("Hackerspaces and technical communities"),
  tk("Historic network of projects and communities"),
  tk("Network with NickServ services"),
];
const STEP_LABELS = [tk("Network"), tk("Identity"), tk("Account"), tk("Channels")];

export function NewNetwork() {
  const me = useMe();
  const nav = useNavigate();
  const toast = useToast();
  const [step, setStep] = useState<Step>(1);
  const [presets, setPresets] = useState<Preset[]>([]);
  const [users, setUsers] = useState<User[]>([]);
  const [preset, setPreset] = useState<string | null>(null);
  const [f, setF] = useState({
    user_id: me.user.id,
    name: "",
    host: "",
    port: 6697,
    tls: true,
    nick: me.user.username,
    username: "",
    realname: "",
    sasl: false,
    sasl_username: "",
    sasl_password: "",
    server_password: "",
    svc: false,
    svc_user: "",
    svc_pass: "",
  });
  const [channels, setChannels] = useState<string[]>([]);
  const [chanInput, setChanInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const set = <K extends keyof typeof f>(k: K, v: (typeof f)[K]) => setF((o) => ({ ...o, [k]: v }));

  useEffect(() => {
    api.get<Preset[]>("/presets").then(setPresets).catch(() => {});
    if (me.permissions.manage_all_networks) api.get<User[]>("/users").then(setUsers).catch(() => {});
  }, [me.permissions.manage_all_networks]);

  const choosePreset = (p: Preset | null) => {
    if (!p) {
      setPreset("custom");
      setF((o) => ({ ...o, name: "", host: "", port: 6697, tls: true }));
      return;
    }
    setPreset(p.name);
    setF((o) => ({ ...o, name: p.name, host: p.host, port: p.port, tls: p.tls, sasl: p.sasl && o.sasl }));
  };
  const chosen = presets.find((p) => p.name === preset);

  const nameOk = /^[A-Za-z0-9._-]{1,32}$/.test(f.name);
  const hostOk = /^[^\s/:@]+$/.test(f.host);
  const nickOk = /^[^\s,*?!@#&:$+][^\s,*?!@]{0,63}$/.test(f.nick);
  const canNext: Record<Step, boolean> = {
    1: nameOk && hostOk && f.port > 0 && f.port < 65536,
    2: nickOk,
    3:
      (!f.sasl || (f.sasl_username.trim() !== "" && f.sasl_password !== "")) &&
      (!f.svc || (f.svc_user.trim() !== "" && f.svc_pass !== "" && !/\s/.test(f.svc_user + f.svc_pass))),
    4: true,
  };

  const addChannel = () => {
    const parts = chanInput
      .split(/[\s,]+/)
      .map((c) => c.trim())
      .filter(Boolean)
      .map((c) => (/^[#&!+]/.test(c) ? c : `#${c}`));
    setChannels((v) => Array.from(new Set([...v, ...parts])));
    setChanInput("");
  };

  const create = async () => {
    setBusy(true);
    setError(null);
    const pending = chanInput.trim()
      ? chanInput.split(/[\s,]+/).filter(Boolean).map((c) => (/^[#&!+]/.test(c) ? c : `#${c}`))
      : [];
    try {
      const n = await api.post<Network>("/networks", {
        user_id: f.user_id,
        name: f.name,
        host: f.host,
        port: Number(f.port),
        tls: f.tls,
        nick: f.nick,
        username: f.username || undefined,
        realname: f.realname || undefined,
        sasl_username: f.sasl ? f.sasl_username : undefined,
        sasl_password: f.sasl ? f.sasl_password : undefined,
        server_password:
          f.svc && chosen?.service?.server_password
            ? chosen.service.server_password.replace("{user}", f.svc_user.trim()).replace("{pass}", f.svc_pass)
            : f.server_password || undefined,
        on_connect:
          f.svc && chosen?.service?.template
            ? chosen.service.template.replace("{user}", f.svc_user.trim()).replace("{pass}", f.svc_pass)
            : undefined,
        channels: Array.from(new Set([...channels, ...pending])),
      });
      toast.ok(t("Network {name} added. xBNC is connecting.", { name: n.name }));
      nav(`/networks/${n.id}`);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const next = (e: FormEvent) => {
    e.preventDefault();
    if (!canNext[step]) return;
    if (step < 4) setStep((s) => (s + 1) as Step);
    else create();
  };

  const userOptions = useMemo(
    () => users.map((u) => ({ value: String(u.id), label: u.id === me.user.id ? t("{username} (you)", { username: u.username }) : u.username })),
    [users, me.user.id],
  );

  return (
    <div className="page" style={{ maxWidth: 820 }}>
      <header className="page-head">
        <div>
          <h1>{t("Add a network")}</h1>
          <p>{t("Four short steps. You can change anything later.")}</p>
        </div>
        <Link to="/networks" className="btn btn-ghost">{t("Cancel")}{" "}</Link>
      </header>

      <ol className="steps" aria-label={t("Steps")} style={{ listStyle: "none", padding: 0, margin: 0 }}>
        {STEP_LABELS.map((l, i) => (
          <li key={l} className={`step${step === i + 1 ? " current" : ""}${step > i + 1 ? " done" : ""}`} aria-current={step === i + 1 ? "step" : undefined}>
            <b>{i + 1}</b> {t(l)}
          </li>
        ))}
      </ol>

      {error && <Callout kind="bad">{error}</Callout>}

      <form className="stack" onSubmit={next}>
        {step === 1 && (
          <>
            <h2>{t("Which network do you want to connect to?")}</h2>
            <div className="presets">
              {presets.map((p) => (
                <button
                  type="button"
                  key={p.name}
                  className="preset"
                  aria-pressed={preset === p.name}
                  style={{ ["--net" as string]: colorFor(p.name) }}
                  onClick={() => choosePreset(p)}
                >
                  <strong>{p.label}</strong>
                  <span>{t(p.description)}</span>
                </button>
              ))}
              <button
                type="button"
                className="preset"
                aria-pressed={preset === "custom"}
                style={{ ["--net" as string]: "var(--ink-3)" }}
                onClick={() => choosePreset(null)}
              >
                <strong>{t("Other network")}</strong>
                <span>{t("Type the address of any IRC server")}</span>
              </button>
            </div>
            {preset && (
              <div className="panel stack">
                <div className="grid-2">
                  <Field
                    label={t("Short name")}
                    value={f.name}
                    onChange={(e) => set("name", e.target.value)}
                    hint={t("You will use it when connecting: user/name. No spaces.")}
                    error={f.name && !nameOk ? t("Use only letters, digits, dot, hyphen or underscore.") : null}
                    required
                  />
                  <Field
                    label={t("Server")}
                    value={f.host}
                    onChange={(e) => set("host", e.target.value.trim())}
                    placeholder="irc.example.org"
                    error={f.host && !hostOk ? t("Type only the server name, without port or slashes.") : null}
                    required
                    mono
                  />
                </div>
                <div className="grid-2">
                  <Field label={t("Port")} type="number" min={1} max={65535} value={f.port} onChange={(e) => set("port", Number(e.target.value))} required />
                  <div className="field" style={{ justifyContent: "flex-end" }}>
                    <Switch
                      checked={f.tls}
                      onChange={(v) => {
                        set("tls", v);
                        if (v && f.port === 6667) set("port", 6697);
                        if (!v && f.port === 6697) set("port", 6667);
                      }}
                      label={t("Encrypted connection (TLS)")}
                      description={t("Recommended. Almost every network supports it on port 6697.")}
                    />
                  </div>
                </div>
                {!f.tls && <Callout kind="warn">{t("Without TLS, your messages and passwords travel unencrypted to the IRC server.")}</Callout>}
              </div>
            )}
          </>
        )}

        {step === 2 && (
          <>
            <h2>{t("How will people see you on {network}?", { network: chosen?.label ?? f.name })}</h2>
            {me.permissions.manage_all_networks && userOptions.length > 1 && (
              <SelectField
                label={t("This network is for")}
                value={String(f.user_id)}
                onChange={(v) => set("user_id", Number(v))}
                options={userOptions}
                hint={t("As an administrator you can set up networks for other users.")}
              />
            )}
            <Field
              label={t("Nick")}
              value={f.nick}
              onChange={(e) => set("nick", e.target.value.trim())}
              error={!nickOk ? t("No spaces or the characters , * ? ! @, and not starting with #.") : null}
              hint={t("If it is taken when connecting, xBNC will add an underscore.")}
              required
            />
            <div className="grid-2">
              <Field label={t("Ident (optional)")} value={f.username} onChange={(e) => set("username", e.target.value.trim())} placeholder={f.nick} hint={t("Shown as nick!ident@host.")} />
              <Field label={t("Real name (optional)")} value={f.realname} onChange={(e) => set("realname", e.target.value)} placeholder={f.nick} hint={t("Shown in /whois.")} />
            </div>
          </>
        )}

        {step === 3 && (
          <>
            <h2>{t("Do you have a registered account on the network?")}</h2>
            <p className="muted">
              {chosen?.service
                ? t("If you registered an account on {label}, xBNC can identify you automatically every time it connects. Passwords are stored encrypted.", { label: chosen.label })
                : t("If you registered your nick with NickServ, xBNC can identify you automatically with SASL every time it connects. Passwords are stored encrypted.")}
            </p>
            {chosen?.service && (
              <>
                <Switch
                  checked={f.svc}
                  onChange={(v) => set("svc", v)}
                  label={t("Identify me with {bot}", { bot: chosen.service.bot })}
                  description={
                    chosen.service.server_password
                      ? t("{label} signs in with {bot} while connecting (Login on Connect): your IP is never shown (mode +x) and, if {bot} is unavailable, xBNC waits and tries again.", { label: chosen.label, bot: chosen.service.bot })
                      : chosen.service.template?.includes("+x")
                      ? t("{label} does not use SASL: xBNC will send the login to {bot} every time it connects and will hide your IP (mode +x).", { label: chosen.label, bot: chosen.service.bot })
                      : t("{label} does not use SASL: xBNC will send the login to {bot} every time it connects.", { label: chosen.label, bot: chosen.service.bot })
                  }
                />
                {f.svc && (
                  <div className="grid-2">
                    <Field label={t("{bot} username", { bot: chosen.service.bot })} value={f.svc_user} onChange={(e) => set("svc_user", e.target.value)} required autoComplete="off" />
                    <Field label={t("{bot} password", { bot: chosen.service.bot })} type="password" value={f.svc_pass} onChange={(e) => set("svc_pass", e.target.value)} required autoComplete="new-password" />
                  </div>
                )}
              </>
            )}
            <Switch
              checked={f.sasl}
              onChange={(v) => set("sasl", v)}
              label={t("Identify me with SASL")}
              description={chosen && !chosen.sasl ? t("This network does not usually offer SASL; turn it on only if you know it supports it.") : t("Recommended if you have an account.")}
            />
            {f.sasl && (
              <div className="grid-2">
                <Field label={t("Account")} value={f.sasl_username} onChange={(e) => set("sasl_username", e.target.value)} placeholder={f.nick} required autoComplete="off" />
                <Field label={t("Account password")} type="password" value={f.sasl_password} onChange={(e) => set("sasl_password", e.target.value)} required autoComplete="new-password" />
              </div>
            )}
            <details>
              <summary className="small" style={{ cursor: "pointer" }}>{t("The server asks for a connection password")}</summary>
              <div style={{ marginTop: 12 }}>
                <Field
                  label={t("Server password (PASS)")}
                  type="password"
                  value={f.server_password}
                  onChange={(e) => set("server_password", e.target.value)}
                  hint={t("Only for private servers that require it.")}
                  autoComplete="new-password"
                />
              </div>
            </details>
          </>
        )}

        {step === 4 && (
          <>
            <h2>{t("Which channels do you want to be in?")}</h2>
            <p className="muted">{t("xBNC will join them every time it connects. They are also saved automatically when you /join from your client.")}</p>
            <div className="row" style={{ alignItems: "flex-end" }}>
              <div style={{ flex: 1, minWidth: 200 }}>
                <Field
                  label={t("Channel")}
                  value={chanInput}
                  onChange={(e) => setChanInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      addChannel();
                    }
                  }}
                  placeholder="#xbnc"
                  mono
                />
              </div>
              <button type="button" className="btn" onClick={addChannel} disabled={!chanInput.trim()}>
                <Plus size={16} />{" "}{t("Add")}</button>
            </div>
            {channels.length > 0 && (
              <div className="row">
                {channels.map((c) => (
                  <span key={c} className="pill pill-accent">
                    <Hash size={14} /> {c.replace(/^#/, "")}
                    <button type="button" className="icon-btn" style={{ width: 22, height: 22 }} aria-label={t("Remove {c}", { c })} onClick={() => setChannels((v) => v.filter((x) => x !== c))}>
                      <X size={14} />
                    </button>
                  </span>
                ))}
              </div>
            )}
            <div className="panel stack-sm">
              <strong>{t("Overview")}</strong>
              <span className="small muted">
                {tx(f.tls ? "{name} at {server} with TLS, nick {nick}" : "{name} at {server} unencrypted, nick {nick}", {
                  name: f.name,
                  server: <span className="mono">{f.host}:{f.port}</span>,
                  nick: <Nick name={f.nick} />,
                })}
                {f.sasl && (
                  <>
                    {t(", identified with SASL as {account}", { account: f.sasl_username })} <ShieldCheck size={14} aria-hidden />
                  </>
                )}
                {f.svc && chosen?.service && (
                  <>
                    {t(", identified with {bot} as {account}", { bot: chosen.service.bot, account: f.svc_user })} <ShieldCheck size={14} aria-hidden />
                  </>
                )}
              </span>
            </div>
          </>
        )}

        <div className="row row-end">
          {step > 1 && (
            <button type="button" className="btn btn-ghost" onClick={() => setStep((s) => (s - 1) as Step)}>{t("Back")}</button>
          )}
          <button className="btn btn-primary" disabled={!canNext[step] || busy || (step === 1 && !preset)}>
            {step < 4 ? t("Continue") : busy ? t("Adding…") : t("Add and connect")}
          </button>
        </div>
      </form>
    </div>
  );
}
