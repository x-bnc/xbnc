import { useState, type ChangeEvent } from "react";
import { FileUp } from "lucide-react";
import { api, errorText } from "../lib/api";
import { Callout, CopyText, Modal } from "../components/ui";
import { t, tk } from "../lib/i18n";

interface Note {
  code: string;
  args: string[];
  text: string;
}

interface PlanNetwork {
  name: string;
  host: string;
  port: number;
  tls: boolean;
  nick: string;
  enabled: boolean;
  channels: string[];
  modules: string[];
  on_connect_lines: number;
  sasl: boolean;
  client_cert: boolean;
  skip: Note | null;
}

interface PlanUser {
  name: string;
  role: "admin" | "user";
  password: "keep" | "reset";
  networks: PlanNetwork[];
  skip: Note | null;
}

interface Plan {
  users: PlanUser[];
  warnings: Note[];
}

interface Report {
  users_created: string[];
  networks_created: number;
  channels_created: number;
  new_passwords: [string, string][];
  skipped: string[];
}

// One text per note code; {0}, {1}… are the note's arguments.
const NOTES: Record<string, string> = {
  user_name_invalid: tk("The name {0} is not valid in xBNC (1 to 32 letters, digits, dot, hyphen or underscore)."),
  user_exists: tk("A user called {0} already exists in xBNC; it is left untouched."),
  module_unsupported: tk("{0}: module {1} has no equivalent in xBNC."),
  password_reset: tk("{0}: the password is stored in a format xBNC cannot check. A new one will be generated."),
  config_only: tk("Only the configuration file was read. SASL, commands on connect, NickServ passwords and certificates live in the previous bouncer's data folder: to bring them, import on the server with “xbncd import”."),
  network_name_invalid: tk("The network name {0} is not valid in xBNC."),
  no_server: tk("It has no usable server."),
  many_servers: tk("{0}: it has {1} servers; {2} is used (xBNC keeps one address per network)."),
  no_tls: tk("{0}: connects without TLS ({1})."),
  nick_invalid: tk("{0}: nick {1} is not valid; {2} is used."),
  channel_invalid: tk("{0}: channel {1} is skipped because its name is not valid."),
  channel_disabled: tk("{0}: channel {1} was disabled and is not imported."),
  too_many_channels: tk("{0}: only the first 500 channels are imported."),
  cert_missing: tk("{0}: the client certificate was not found. Generate or upload one in the panel and enable the module."),
  module_entries: tk("{0}: module {1} is enabled, but its entries use a different syntax. Add them again in the Modules tab."),
  module_invalid: tk("{0}: module {1} cannot be configured: {2}"),
  too_many_commands: tk("{0}: only the first 50 commands on connect are kept."),
  network_invalid: tk("Some network data is not valid: {0}"),
};

function noteText(n: Note) {
  const tpl = NOTES[n.code];
  if (!tpl) return n.text;
  return t(tpl, Object.fromEntries(n.args.map((a, i) => [String(i), a])));
}

/** Two steps: paste or choose the file and preview, then import. */
export function ImportUsers({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const [config, setConfig] = useState("");
  const [fileName, setFileName] = useState<string | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [report, setReport] = useState<Report | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const pick = async (e: ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    if (!f) return;
    if (f.size > 250 * 1024) {
      setError(t("The file is too large for the panel. Import it on the server with “xbncd import”."));
      return;
    }
    setConfig(await f.text());
    setFileName(f.name);
    setPlan(null);
    setError(null);
  };

  const preview = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await api.post<{ plan: Plan }>("/import/preview", { config });
      setPlan(r.plan);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const apply = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await api.post<{ report: Report }>("/import/apply", { config });
      setReport(r.report);
      onDone();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const importable = plan?.users.filter((u) => !u.skip) ?? [];

  if (report) {
    return (
      <Modal title={t("Import finished")} onClose={onClose} wide>
        <p>
          {t("Created {users} users, {networks} networks and {channels} channels. Enabled networks are already connecting.", {
            users: report.users_created.length,
            networks: report.networks_created,
            channels: report.channels_created,
          })}
        </p>
        {report.skipped.length > 0 && <p className="small muted">{t("Not imported: {list}.", { list: report.skipped.join(", ") })}</p>}
        {report.new_passwords.length > 0 && (
          <>
            <Callout kind="warn">{t("These new passwords are shown only now. Give them to each user; they can change them in Your account.")}</Callout>
            {report.new_passwords.map(([u, p]) => (
              <div key={u} className="stack-sm">
                <strong>{u}</strong>
                <CopyText text={p} label={t("Copy {user}'s password", { user: u })} />
              </div>
            ))}
          </>
        )}
        <div className="row row-end">
          <button className="btn btn-primary" onClick={onClose}>
            {t("Close")}
          </button>
        </div>
      </Modal>
    );
  }

  return (
    <Modal title={t("Import from another bouncer")} onClose={onClose} wide>
      <p className="muted">
        {t("Brings users, networks, channels with their keys and equivalent modules from your previous bouncer's configuration file. Existing users are left untouched. You will see what will be created first.")}
      </p>
      {!plan && (
        <>
          <label className="btn" style={{ alignSelf: "flex-start" }}>
            <FileUp size={18} aria-hidden /> {fileName ?? t("Choose .conf file")}
            <input type="file" accept=".conf,text/plain" className="sr-only" onChange={pick} />
          </label>
          <div className="field">
            <label htmlFor="import-config">{t("Or paste its contents here")}</label>
            <textarea
              id="import-config"
              className="input input-mono"
              rows={8}
              spellCheck={false}
              value={config}
              onChange={(e) => {
                setConfig(e.target.value);
                setFileName(null);
              }}
            />
            <span className="hint">{t("Passwords stored with SHA-256 keep working: xBNC switches them to Argon2id the first time each user logs in.")}</span>
          </div>
        </>
      )}
      {error && <Callout kind="bad">{error}</Callout>}
      {plan && (
        <div className="stack-sm import-plan">
          {plan.users.map((u) => (
            <div key={u.name} className={`import-user${u.skip ? " is-skipped" : ""}`}>
              <div className="spread">
                <strong>{u.name}</strong>
                <span className="small muted">
                  {u.skip
                    ? t("Not imported")
                    : `${u.role === "admin" ? t("Administrator") : t("User")}, ${u.password === "keep" ? t("keeps their password") : t("new password")}`}
                </span>
              </div>
              {u.skip && <p className="small">{noteText(u.skip)}</p>}
              {!u.skip &&
                u.networks.map((n) => (
                  <div key={n.name} className="import-net small">
                    <b>{n.name}</b>{" "}
                    {n.skip ? (
                      <span className="muted">{noteText(n.skip)}</span>
                    ) : (
                      <span className="muted">
                        {n.host}:{n.port}
                        {n.tls ? " TLS" : ""}, {n.nick}
                        {!n.enabled && `, ${t("disabled")}`}
                        {n.channels.length > 0 && ` · ${n.channels.join(" ")}`}
                        {n.modules.length > 0 && ` · ${t("modules: {list}", { list: n.modules.join(", ") })}`}
                      </span>
                    )}
                  </div>
                ))}
            </div>
          ))}
          {plan.warnings.length > 0 && (
            <details open>
              <summary>{t("{n} notes", { n: plan.warnings.length })}</summary>
              <ul className="small">
                {plan.warnings.map((w, i) => (
                  <li key={i}>{noteText(w)}</li>
                ))}
              </ul>
            </details>
          )}
        </div>
      )}
      <div className="row row-end">
        {plan ? (
          <>
            <button className="btn btn-ghost" onClick={() => setPlan(null)}>
              {t("Back")}
            </button>
            <button className="btn btn-primary" disabled={busy || importable.length === 0} onClick={apply}>
              {busy ? t("Importing…") : t("Import {n} users", { n: importable.length })}
            </button>
          </>
        ) : (
          <>
            <button className="btn btn-ghost" onClick={onClose}>
              {t("Cancel")}
            </button>
            <button className="btn btn-primary" disabled={busy || !config.trim()} onClick={preview}>
              {busy ? t("Reading…") : t("See what will be imported")}
            </button>
          </>
        )}
      </div>
    </Modal>
  );
}
