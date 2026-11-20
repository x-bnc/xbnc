import { useEffect, useState, type FormEvent } from "react";
import { Camera, Clock, FileDown, KeyRound, Languages, ShieldCheck, ShieldOff, StickyNote, Trash2 } from "lucide-react";
import { api, errorText } from "../lib/api";
import { useMe, useSession } from "../lib/session";
import { useToast } from "../lib/toast";
import { formatDate, roleHelp, roleLabel } from "../lib/format";
import { Avatar, Callout, Confirm, CopyText, Field, Modal, SelectField } from "../components/ui";
import { SessionsTable } from "./Security";
import { Notifications } from "../components/Notifications";
import { Passkeys } from "../components/Passkeys";
import { getLangs, setLang, t, tk, tx } from "../lib/i18n";

export function ProfilePage() {
  const me = useMe();
  return (
    <div className="page" style={{ maxWidth: 900 }}>
      <header className="page-head">
        <div className="row" style={{ gap: 16 }}>
          <Avatar name={me.user.username} size={56} src={me.user.avatar} />
          <div>
            <h1>{me.user.username}</h1>
            <p style={{ marginTop: 2 }}>
              {roleLabel(me.user.role)}. {roleHelp(me.user.role)}
            </p>
          </div>
        </div>
      </header>
      <ProfilePhoto />
      <Language />
      <TimeZone />
      <Notifications />
      <ChangePassword />
      <TwoFactor />
      <Passkeys />
      <Notes />
      <ApiTokens />
      <TestReport />
      <section className="section">
        <div>
          <h2>{t("Where you are signed in")}</h2>
          <p className="muted" style={{ marginTop: 4 }}>{t("Open panel sessions. Close the ones you do not recognise.")}</p>
        </div>
        <SessionsTable all={false} />
      </section>
      <p className="small faint">{t("Account created on {date}. xBNC {version}.", { date: formatDate(me.user.created_at), version: me.version })}</p>
    </div>
  );
}

/** Shrinks a picture to a 256×256 square (cropped to the centre) in the
 * browser; the upload carries no location or camera data. */
async function squarePhoto(file: File): Promise<Blob> {
  const url = URL.createObjectURL(file);
  try {
    const img = await new Promise<HTMLImageElement>((ok, fail) => {
      const i = new Image();
      i.onload = () => ok(i);
      i.onerror = () => fail(new Error(t("This file is not an image the browser can open.")));
      i.src = url;
    });
    const side = Math.min(img.naturalWidth, img.naturalHeight);
    if (!side) throw new Error(t("This file is not an image the browser can open."));
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 256;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error(t("This file is not an image the browser can open."));
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(img, (img.naturalWidth - side) / 2, (img.naturalHeight - side) / 2, side, side, 0, 0, 256, 256);
    const make = (type: string, q?: number) => new Promise<Blob | null>((ok) => canvas.toBlob(ok, type, q));
    // WebP where the browser can write it; otherwise PNG (Safari).
    let blob = await make("image/webp", 0.9);
    if (!blob || blob.type !== "image/webp") blob = await make("image/png");
    if (!blob) throw new Error(t("This file is not an image the browser can open."));
    return blob;
  } finally {
    URL.revokeObjectURL(url);
  }
}

function ProfilePhoto() {
  const me = useMe();
  const { refresh } = useSession();
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const pick = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    try {
      const blob = await squarePhoto(file);
      await api.put("/me/avatar", blob);
      await refresh();
      toast.ok(t("Photo saved."));
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    setBusy(true);
    try {
      await api.del("/me/avatar");
      await refresh();
      toast.ok(t("Photo removed."));
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="panel stack-sm">
      <h2 className="row" style={{ gap: 8 }}>
        <Camera size={20} aria-hidden /> {t("Profile photo")}
      </h2>
      <p className="muted small">
        {t("Shown in the panel to you and to the administrators. It is cropped to a square of 256×256 pixels.")}
      </p>
      <div className="row" style={{ gap: 12, flexWrap: "wrap", alignItems: "center" }}>
        <Avatar name={me.user.username} size={64} src={me.user.avatar} />
        <label className={`btn${busy ? " disabled" : ""}`}>
          <Camera size={16} aria-hidden /> {me.user.avatar ? t("Change photo") : t("Choose a photo")}
          <input
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif"
            className="sr-only"
            disabled={busy}
            onChange={(e) => {
              void pick(e.target.files?.[0]);
              e.target.value = "";
            }}
          />
        </label>
        {me.user.avatar && (
          <button className="btn ghost" onClick={() => void remove()} disabled={busy}>
            <Trash2 size={16} aria-hidden /> {t("Remove photo")}
          </button>
        )}
      </div>
    </section>
  );
}

function Language() {
  const me = useMe();
  const { refresh } = useSession();
  const toast = useToast();
  const value = me.user.language ?? "";
  const change = async (code: string) => {
    try {
      const r = await api.put<{ effective: string }>("/me/language", { language: code });
      // Refresh first: changing the language re-renders the whole panel.
      await refresh();
      setLang(r.effective);
      toast.ok(t("Language saved."));
    } catch (e) {
      toast.error(errorText(e));
    }
  };
  return (
    <section className="panel stack-sm">
      <h2 className="row" style={{ gap: 8 }}>
        <Languages size={20} aria-hidden /> {t("Language")}
      </h2>
      <p className="muted small">{t("For this panel and for xBNC's messages in your IRC client (*status and notices).")}</p>
      <div className="seg" role="radiogroup" aria-label={t("Language")}>
        <button type="button" role="radio" aria-checked={value === ""} className={value === "" ? "on" : ""} onClick={() => change("")}>
          {t("Server default")}
        </button>
        {getLangs().map((l) => (
          <button key={l.code} type="button" role="radio" aria-checked={value === l.code} className={value === l.code ? "on" : ""} lang={l.code} onClick={() => change(l.code)}>
            {l.name}
          </button>
        ))}
      </div>
    </section>
  );
}

/** Downloads the test report (what each network supports). */
function TestReport() {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const download = async () => {
    setBusy(true);
    try {
      const r = await api.get<Record<string, unknown>>("/diagnostics");
      const blob = new Blob([JSON.stringify(r, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `xbnc-informe-${new Date().toISOString().slice(0, 10)}.json`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="panel stack-sm">
      <h2 className="row" style={{ gap: 8 }}>
        <FileDown size={20} aria-hidden /> {t("Test report")}
      </h2>
      <p className="muted small">
        {t("A file with what each of your networks supports (capabilities, ISUPPORT, login, active modules) and how xBNC handled it. It contains no passwords, messages, other people's nicks or your devices' IP addresses.")}
      </p>
      <div>
        <button type="button" className="btn" disabled={busy} onClick={download}>
          {busy ? t("Preparing…") : t("Download report")}
        </button>
      </div>
    </section>
  );
}

/** Time zones the browser knows, or a short list on old browsers. */
function zoneList(): string[] {
  try {
    const f = (Intl as unknown as { supportedValuesOf?: (k: string) => string[] }).supportedValuesOf;
    if (f) return f("timeZone");
  } catch {
    /* fall through */
  }
  return ["Europe/Madrid", "Europe/London", "Europe/Lisbon", "Atlantic/Canary", "America/Mexico_City", "America/Bogota", "America/Argentina/Buenos_Aires", "America/New_York", "UTC"];
}

function TimeZone() {
  const me = useMe();
  const { refresh } = useSession();
  const toast = useToast();
  const [value, setValue] = useState(me.timezone ?? "");
  const [busy, setBusy] = useState(false);
  const browser = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const save = async (tz: string) => {
    setBusy(true);
    try {
      const r = await api.put<{ timezone: string }>("/me/timezone", { timezone: tz });
      setValue(r.timezone);
      await refresh();
      toast.ok(t("Time zone saved."));
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="panel stack-sm">
      <h2 className="row" style={{ gap: 8 }}>
        <Clock size={20} aria-hidden /> {t("Time zone")}
      </h2>
      <p className="muted small">{t("For file logs and scheduled tasks. Empty: UTC.")}</p>
      <form
        className="row"
        style={{ gap: 8, flexWrap: "wrap" }}
        onSubmit={(e) => {
          e.preventDefault();
          save(value.trim());
        }}
      >
        <input
          className="input"
          style={{ maxWidth: 320 }}
          list="tz-list"
          value={value}
          placeholder="Europe/Madrid"
          aria-label={t("Time zone")}
          onChange={(e) => setValue(e.target.value)}
        />
        <datalist id="tz-list">
          {zoneList().map((z) => (
            <option key={z} value={z} />
          ))}
        </datalist>
        <button className="btn btn-primary" disabled={busy || value.trim() === (me.timezone ?? "")}>
          {t("Save")}
        </button>
        {browser && browser !== value && (
          <button type="button" className="btn btn-ghost" disabled={busy} onClick={() => save(browser)}>
            {t("Use this device's ({zone})", { zone: browser })}
          </button>
        )}
      </form>
    </section>
  );
}

interface Note {
  key: string;
  text: string;
  updated_at: number;
}

function Notes() {
  const toast = useToast();
  const [notes, setNotes] = useState<Note[] | null>(null);
  const [key, setKey] = useState("");
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const load = () =>
    api
      .get<{ notes: Note[] }>("/notes")
      .then((r) => setNotes(r.notes))
      .catch((e) => setError(errorText(e)));
  useEffect(() => {
    load();
  }, []);
  const add = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      await api.put(`/notes/${encodeURIComponent(key.trim())}`, { text });
      setKey("");
      setText("");
      load();
      toast.ok(t("Note saved."));
    } catch (err) {
      setError(errorText(err));
    }
  };
  const remove = async (k: string) => {
    try {
      await api.del(`/notes/${encodeURIComponent(k)}`);
      load();
    } catch (err) {
      toast.error(errorText(err));
    }
  };
  return (
    <section className="panel stack-sm">
      <h2 className="row" style={{ gap: 8 }}>
        <StickyNote size={20} aria-hidden /> {t("Notes")}
      </h2>
      <p className="muted small">{t("Quick notes you can also read and write from IRC with /msg *notes.")}</p>
      {error && <Callout kind="bad">{error}</Callout>}
      {notes && notes.length > 0 && (
        <ul className="notes">
          {notes.map((n) => (
            <li key={n.key}>
              <code>{n.key}</code>
              <span>{n.text}</span>
              <button type="button" className="icon-btn" aria-label={t("Delete the note {key}", { key: n.key })} onClick={() => remove(n.key)}>
                <Trash2 size={15} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <form className="row" onSubmit={add} style={{ alignItems: "flex-end" }}>
        <div style={{ flex: "0 1 160px" }}>
          <Field label={t("Key")} value={key} onChange={(e) => setKey(e.target.value)} maxLength={32} required />
        </div>
        <div style={{ flex: "1 1 260px" }}>
          <Field label={t("Text")} value={text} onChange={(e) => setText(e.target.value)} maxLength={400} required />
        </div>
        <button className="btn" disabled={!key.trim() || !text.trim()}>
          {t("Save note")}
        </button>
      </form>
    </section>
  );
}

function ChangePassword() {
  const toast = useToast();
  const [cur, setCur] = useState("");
  const [pw, setPw] = useState("");
  const [rep, setRep] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mismatch = rep.length > 0 && rep !== pw;
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (pw !== rep) return;
    setBusy(true);
    setError(null);
    try {
      await api.put("/me/password", { current_password: cur, new_password: pw });
      toast.ok(t("Password changed. Remember to update it in your IRC clients."));
      setCur("");
      setPw("");
      setRep("");
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="panel">
      <div className="panel-head">
        <div>
          <h2>{t("Password")}</h2>
          <p>{t("It is the same for the panel and your IRC clients. Changing it closes your other panel sessions.")}</p>
        </div>
      </div>
      <form className="stack" onSubmit={submit}>
        {error && <Callout kind="bad">{error}</Callout>}
        <div className="grid-3">
          <Field label={t("Current password")} type="password" value={cur} onChange={(e) => setCur(e.target.value)} autoComplete="current-password" required />
          <Field label={t("New password")} type="password" value={pw} onChange={(e) => setPw(e.target.value)} autoComplete="new-password" minLength={10} required hint={t("At least 10 characters.")} />
          <Field label={t("Repeat it")} type="password" value={rep} onChange={(e) => setRep(e.target.value)} autoComplete="new-password" required error={mismatch ? t("They do not match.") : null} />
        </div>
        <div className="row row-end">
          <button className="btn btn-primary" disabled={busy || mismatch}>{t("Change password")}</button>
        </div>
      </form>
    </section>
  );
}

function TwoFactor() {
  const me = useMe();
  const { refresh } = useSession();
  const toast = useToast();
  const [setup, setSetup] = useState<{ secret: string; uri: string; qr_svg: string } | null>(null);
  const [code, setCode] = useState("");
  const [disabling, setDisabling] = useState(false);
  const [pw, setPw] = useState("");
  const [error, setError] = useState<string | null>(null);
  const on = me.user.totp_enabled;

  const start = async () => {
    setError(null);
    try {
      setSetup(await api.post("/me/totp/setup"));
    } catch (e) {
      toast.error(errorText(e));
    }
  };
  const enable = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api.post("/me/totp/enable", { code });
      toast.ok(t("Two-step verification turned on."));
      setSetup(null);
      setCode("");
      refresh();
    } catch (err) {
      setError(errorText(err));
    }
  };
  const disable = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api.post("/me/totp/disable", { password: pw });
      toast.ok(t("Two-step verification turned off."));
      setDisabling(false);
      setPw("");
      refresh();
    } catch (err) {
      setError(errorText(err));
    }
  };

  return (
    <section className="panel">
      <div className="panel-head">
        <div>
          <h2 className="row" style={{ gap: 10 }}>
            {on ? <ShieldCheck color="var(--ok)" /> : <ShieldOff color="var(--ink-3)" />}{" "}{t("Two-step verification")}</h2>
          <p>
            {on
              ? t("On. When you sign in to the panel you will be asked for a code from your authenticator app.")
              : t("Protect the panel with a code from Google Authenticator, Aegis, 1Password or another compatible app.")}
          </p>
          <p className="small faint" style={{ marginTop: 4 }}>{t("IRC clients still sign in with the password only, because the protocol has no second step.")}</p>
        </div>
        {on ? (
          <button className="btn btn-danger" onClick={() => setDisabling(true)}>{t("Turn off")}{" "}</button>
        ) : (
          <button className="btn btn-primary" onClick={start}>{t("Turn on")}{" "}</button>
        )}
      </div>

      {setup && (
        <Modal title={t("Turn on two-step verification")} onClose={() => setSetup(null)}>
          <ol className="stack" style={{ paddingLeft: 20, margin: 0 }}>
            <li>{t("Scan this code with your authenticator app.")}</li>
          </ol>
          <img className="qr" alt={t("QR code for the authenticator app")} src={`data:image/svg+xml;utf8,${encodeURIComponent(setup.qr_svg)}`} />
          <details>
            <summary className="small" style={{ cursor: "pointer" }}>{t("I cannot scan it")}</summary>
            <div style={{ marginTop: 8 }}>
              <p className="small muted">{t("Add an account manually with this key:")}</p>
              <CopyText text={setup.secret.replace(/(.{4})/g, "$1 ").trim()} />
            </div>
          </details>
          <form className="stack" onSubmit={enable}>
            <ol start={2} style={{ paddingLeft: 20, margin: 0 }}>
              <li>{t("Enter the 6-digit code the app shows.")}</li>
            </ol>
            {error && <Callout kind="bad">{error}</Callout>}
            <Field label={t("Code")} value={code} onChange={(e) => setCode(e.target.value.replace(/\D/g, "").slice(0, 6))} inputMode="numeric" autoComplete="one-time-code" mono required />
            <div className="row row-end">
              <button type="button" className="btn btn-ghost" onClick={() => setSetup(null)}>{t("Cancel")}{" "}</button>
              <button className="btn btn-primary" disabled={code.length !== 6}>{t("Turn on")}{" "}</button>
            </div>
          </form>
        </Modal>
      )}
      {disabling && (
        <Modal title={t("Turn off two-step verification")} onClose={() => setDisabling(false)}>
          <form className="stack" onSubmit={disable}>
            <p>{t("To confirm, enter your password.")}</p>
            {error && <Callout kind="bad">{error}</Callout>}
            <Field label={t("Password")} type="password" value={pw} onChange={(e) => setPw(e.target.value)} autoComplete="current-password" required />
            <div className="row row-end">
              <button type="button" className="btn btn-ghost" onClick={() => setDisabling(false)}>{t("Cancel")}{" "}</button>
              <button className="btn btn-danger-solid">{t("Turn off")}</button>
            </div>
          </form>
        </Modal>
      )}
    </section>
  );
}

interface ApiToken {
  id: number;
  name: string;
  prefix: string;
  scope: "read" | "write" | "metrics";
  created_at: number;
  expires_at: number | null;
  last_used_at: number | null;
  last_ip: string | null;
}

const SCOPES: { value: ApiToken["scope"]; label: string; help: string }[] = [
  { value: "read", label: tk("Read only"), help: tk("Can see everything you see, but cannot change anything.") },
  { value: "write", label: tk("Read and write"), help: tk("Can do what you can do in the panel, except change your password, two-step verification or tokens.") },
  { value: "metrics", label: tk("Metrics only"), help: tk("Can only read /api/v1/metrics, for Prometheus or another monitoring system.") },
];

const EXPIRY = [
  { value: "30", label: tk("30 days") },
  { value: "90", label: tk("90 days") },
  { value: "365", label: tk("1 year") },
  { value: "0", label: tk("Never expires") },
];

function ApiTokens() {
  const toast = useToast();
  const [tokens, setTokens] = useState<ApiToken[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [scope, setScope] = useState<ApiToken["scope"]>("read");
  const [days, setDays] = useState("90");
  const [busy, setBusy] = useState(false);
  const [created, setCreated] = useState<{ name: string; token: string } | null>(null);
  const [revoke, setRevoke] = useState<ApiToken | null>(null);
  const load = () =>
    api
      .get<{ tokens: ApiToken[] }>("/tokens")
      .then((r) => setTokens(r.tokens))
      .catch((e) => setError(errorText(e)));
  useEffect(() => {
    load();
  }, []);
  const create = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const r = await api.post<{ name: string; token: string }>("/tokens", { name: name.trim(), scope, expires_days: Number(days) });
      setCreated({ name: r.name, token: r.token });
      setName("");
      load();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };
  const doRevoke = async () => {
    if (!revoke) return;
    try {
      await api.del(`/tokens/${revoke.id}`);
      toast.ok(t("Token {name} revoked.", { name: revoke.name }));
      setRevoke(null);
      load();
    } catch (err) {
      toast.error(errorText(err));
    }
  };
  const scopeLabel = (s: string) => t(SCOPES.find((x) => x.value === s)?.label ?? s);
  return (
    <section className="panel stack-sm">
      <h2 className="row" style={{ gap: 8 }}>
        <KeyRound size={20} aria-hidden /> {t("API tokens")}
      </h2>
      <p className="muted small">
        {tx("For scripts, bots and monitoring. Send them in the {header} header. Each token has your account's permissions, limited by its scope.", {
          header: <code>Authorization: Bearer xbnc_…</code>,
        })}
      </p>
      {error && <Callout kind="bad">{error}</Callout>}
      {tokens && tokens.length > 0 && (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>{t("Name")}</th>
                <th>{t("Scope")}</th>
                <th>{t("Last used")}</th>
                <th>{t("Expires")}</th>
                <th aria-label={t("Actions")} />
              </tr>
            </thead>
            <tbody>
              {tokens.map((k) => (
                <tr key={k.id}>
                  <td>
                    <strong>{k.name}</strong>
                    <div className="small faint">
                      <code>{k.prefix}…</code>
                    </div>
                  </td>
                  <td>{scopeLabel(k.scope)}</td>
                  <td className="small">{k.last_used_at ? `${formatDate(k.last_used_at)}${k.last_ip ? ` (${k.last_ip})` : ""}` : t("Never")}</td>
                  <td className="small">{k.expires_at ? formatDate(k.expires_at) : t("Never expires")}</td>
                  <td style={{ textAlign: "right" }}>
                    <button type="button" className="btn btn-ghost btn-sm" onClick={() => setRevoke(k)}>
                      {t("Revoke")}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {tokens && tokens.length === 0 && <p className="small faint">{t("You have no tokens yet.")}</p>}
      <form className="grid-2" onSubmit={create}>
        <Field label={t("Token name")} hint={t("To recognise it, for example “prometheus” or “alert bot”.")} value={name} onChange={(e) => setName(e.target.value)} maxLength={60} required />
        <SelectField label={t("Expires after")} value={days} onChange={setDays} options={EXPIRY.map((o) => ({ value: o.value, label: t(o.label) }))} />
        <SelectField
          label={t("Scope")}
          hint={t(SCOPES.find((x) => x.value === scope)!.help)}
          value={scope}
          onChange={(v) => setScope(v as ApiToken["scope"])}
          options={SCOPES.map((o) => ({ value: o.value, label: t(o.label) }))}
        />
        <div className="field" style={{ justifyContent: "flex-end" }}>
          <button className="btn" disabled={busy || !name.trim()}>
            {busy ? t("Creating…") : t("Create token")}
          </button>
        </div>
      </form>
      {created && (
        <Modal title={t("Token {name} created", { name: created.name })} onClose={() => setCreated(null)}>
          <Callout kind="warn">{t("Copy it now: this is the only time it is shown. xBNC only keeps a fingerprint and cannot recover it.")}</Callout>
          <CopyText text={created.token} label={t("Token")} />
          <div className="row" style={{ justifyContent: "flex-end" }}>
            <button className="btn btn-primary" onClick={() => setCreated(null)}>
              {t("I have copied it")}
            </button>
          </div>
        </Modal>
      )}
      {revoke && (
        <Confirm title={t("Revoke token {name}?", { name: revoke.name })} confirmLabel={t("Revoke")} onConfirm={doRevoke} onClose={() => setRevoke(null)}>
          {t("Anything using it will stop working immediately.")}
        </Confirm>
      )}
    </section>
  );
}
