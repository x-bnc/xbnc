import { useEffect, useState, type FormEvent } from "react";
import { ChevronDown, ChevronRight, KeyRound, RefreshCw, Trash2, Upload } from "lucide-react";
import { api, errorText } from "../lib/api";
import { useToast } from "../lib/toast";
import { t, tx } from "../lib/i18n";
import { moduleHelp, moduleName, moduleSummary, optionText, settingText, type ModuleDef, type ModuleState } from "../lib/modules";
import { Callout, Confirm, CopyText, Loading, Modal, Switch } from "./ui";
import { NetworkPlugins } from "./Plugins";

type Config = ModuleState["config"];

/** Catalog of modules, fetched once per page view. */
export function useCatalog() {
  const [cat, setCat] = useState<{ modules: ModuleDef[]; builtin: string[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api.get<typeof cat>("/modules").then(setCat).catch((e) => setError(errorText(e)));
  }, []);
  return { cat, error };
}

/** Modules of one network: switch, settings and help for each. */
export function ModulesTab({ networkId, editable, onReconnect }: { networkId: number; editable: boolean; onReconnect: () => void }) {
  const { cat, error: catError } = useCatalog();
  const [states, setStates] = useState<ModuleState[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const toast = useToast();

  useEffect(() => {
    api
      .get<{ modules: ModuleState[] }>(`/networks/${networkId}/modules`)
      .then((r) => setStates(r.modules))
      .catch((e) => setError(errorText(e)));
  }, [networkId]);

  const save = async (id: string, enabled: boolean, config: Config) => {
    const r = await api.put<ModuleState>(`/networks/${networkId}/modules/${id}`, { enabled, config });
    setStates((s) => s?.map((m) => (m.id === id ? r : m)) ?? null);
    return r;
  };

  const toggle = async (id: string, on: boolean) => {
    const cur = states?.find((m) => m.id === id);
    if (!cur) return;
    try {
      await save(id, on, cur.config);
      toast.ok(on ? t("{module} turned on.", { module: moduleName(id) }) : t("{module} turned off.", { module: moduleName(id) }));
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  if (catError || error) return <Callout kind="bad">{catError ?? error}</Callout>;
  if (!cat || !states) return <Loading />;

  return (
    <div className="stack">
      <p className="muted" style={{ maxWidth: "70ch" }}>
        {t("Things xBNC does for you on this network. Changes apply immediately.")}
      </p>
      <ul className="module-list">
        {cat.modules.map((def) => {
          const st = states.find((s) => s.id === def.id);
          if (!st) return null;
          const expanded = open === def.id;
          const hasDetails = def.settings.length > 0 || def.id === "cert" || !!moduleHelp(def.id);
          return (
            <li key={def.id} className={`module${st.enabled ? " on" : ""}`}>
              <div className="module-head">
                <button
                  type="button"
                  className="module-title"
                  aria-expanded={hasDetails ? expanded : undefined}
                  onClick={() => hasDetails && setOpen(expanded ? null : def.id)}
                  disabled={!hasDetails}
                >
                  {hasDetails ? expanded ? <ChevronDown size={16} aria-hidden /> : <ChevronRight size={16} aria-hidden /> : <span style={{ width: 16 }} />}
                  <span>
                    <strong>{moduleName(def.id)}</strong>
                    <span className="small muted">{moduleSummary(def.id)}</span>
                  </span>
                </button>
                <Switch checked={st.enabled} onChange={(v) => toggle(def.id, v)} label={moduleName(def.id)} disabled={!editable} hideLabel />
              </div>
              {expanded && (
                <div className="module-body">
                  {moduleHelp(def.id) && <p className="small muted">{moduleHelp(def.id)}</p>}
                  {def.settings.length > 0 && (
                    <ModuleForm def={def} state={st} editable={editable} onSave={(config) => save(def.id, st.enabled, config)} />
                  )}
                  {def.id === "cert" && <CertPanel networkId={networkId} editable={editable} onReconnect={onReconnect} />}
                </div>
              )}
            </li>
          );
        })}
      </ul>
      <NetworkPlugins networkId={networkId} editable={editable} />
    </div>
  );
}

function ModuleForm({ def, state, editable, onSave }: { def: ModuleDef; state: ModuleState; editable: boolean; onSave: (c: Config) => Promise<ModuleState> }) {
  const [draft, setDraft] = useState<Config>(state.config);
  const isSet = (key: string) => (state.secrets_set ?? []).includes(key);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const toast = useToast();
  const dirty = JSON.stringify(draft) !== JSON.stringify(state.config);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const r = await onSave(draft);
      setDraft(r.config);
      toast.ok(t("Saved."));
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="stack-sm" onSubmit={submit}>
      {error && <Callout kind="bad">{error}</Callout>}
      {def.settings.map((s) => {
        const txt = settingText(def.id, s.key);
        const id = `m-${def.id}-${s.key}`;
        const v = draft[s.key];
        if (s.type === "bool") {
          return (
            <Switch
              key={s.key}
              checked={v === true}
              onChange={(on) => setDraft({ ...draft, [s.key]: on })}
              label={txt.label}
              description={txt.help || undefined}
              disabled={!editable}
            />
          );
        }
        if (s.type === "choice") {
          return (
            <div className="field" key={s.key}>
              <label htmlFor={id}>{txt.label}</label>
              <select id={id} className="input" value={String(v ?? s.default)} disabled={!editable} onChange={(e) => setDraft({ ...draft, [s.key]: e.target.value })}>
                {(s.options ?? []).map((o) => (
                  <option key={o} value={o}>
                    {optionText(def.id, s.key, o)}
                  </option>
                ))}
              </select>
              {txt.help && <span className="hint">{txt.help}</span>}
            </div>
          );
        }
        if (s.type === "secret" || s.type === "secret_lines") {
          const cleared = v === null;
          const placeholder = cleared ? t("Will be removed when saving") : isSet(s.key) ? t("Saved (type to change it)") : txt.placeholder || t("Not set");
          return (
            <div className="field" key={s.key}>
              <label htmlFor={id}>{txt.label}</label>
              {s.type === "secret" ? (
                <input
                  id={id}
                  className="input"
                  type="password"
                  autoComplete="new-password"
                  value={cleared ? "" : String(v ?? "")}
                  placeholder={placeholder}
                  disabled={!editable}
                  onChange={(e) => setDraft({ ...draft, [s.key]: e.target.value })}
                />
              ) : (
                <textarea
                  id={id}
                  className="input input-mono"
                  rows={3}
                  value={cleared ? "" : String(v ?? "")}
                  placeholder={placeholder}
                  disabled={!editable}
                  spellCheck={false}
                  autoComplete="off"
                  onChange={(e) => setDraft({ ...draft, [s.key]: e.target.value })}
                />
              )}
              <span className="hint">
                {txt.help}
                {isSet(s.key) && editable && !cleared && (
                  <>
                    {" "}
                    <button type="button" className="btn-link" onClick={() => setDraft({ ...draft, [s.key]: null })}>
                      {t("Remove")}
                    </button>
                  </>
                )}
              </span>
            </div>
          );
        }
        return (
          <div className="field" key={s.key}>
            <label htmlFor={id}>{txt.label}</label>
            {s.type === "lines" ? (
              <textarea
                id={id}
                className="input input-mono"
                rows={Math.min(8, Math.max(3, String(v ?? "").split("\n").length + 1))}
                value={String(v ?? "")}
                placeholder={txt.placeholder}
                disabled={!editable}
                spellCheck={false}
                onChange={(e) => setDraft({ ...draft, [s.key]: e.target.value })}
              />
            ) : (
              <input
                id={id}
                className="input"
                type={s.type === "int" ? "number" : "text"}
                min={s.min}
                max={s.max}
                maxLength={s.type === "text" ? s.max : undefined}
                value={String(v ?? "")}
                placeholder={txt.placeholder}
                disabled={!editable}
                onChange={(e) => setDraft({ ...draft, [s.key]: s.type === "int" ? Number(e.target.value) : e.target.value })}
              />
            )}
            {(txt.help || s.type === "int") && (
              <span className="hint">
                {txt.help}
                {s.type === "int" && <> {t("Between {min} and {max}.", { min: s.min ?? 0, max: s.max ?? 0 })}</>}
              </span>
            )}
          </div>
        );
      })}
      {editable && (
        <div className="row row-end">
          {dirty && (
            <button type="button" className="btn btn-ghost" onClick={() => setDraft(state.config)}>
              {t("Discard")}
            </button>
          )}
          <button className="btn btn-primary" disabled={busy || !dirty}>
            {busy ? t("Saving…") : t("Save")}
          </button>
        </div>
      )}
    </form>
  );
}

interface CertInfo {
  present: boolean;
  certificate?: string;
  fingerprints?: { sha1: string; sha256: string; sha512: string } | null;
}

function CertPanel({ networkId, editable, onReconnect }: { networkId: number; editable: boolean; onReconnect: () => void }) {
  const [info, setInfo] = useState<CertInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [changed, setChanged] = useState(false);
  const toast = useToast();
  const url = `/networks/${networkId}/cert`;

  useEffect(() => {
    api.get<CertInfo>(url).then(setInfo).catch((e) => setError(errorText(e)));
  }, [url]);

  const generate = async () => {
    setBusy(true);
    setError(null);
    try {
      setInfo(await api.post<CertInfo>(`${url}/generate`));
      setChanged(true);
      toast.ok(t("Certificate created."));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    try {
      setInfo(await api.del<CertInfo>(url));
      setChanged(true);
      setRemoving(false);
      toast.ok(t("Certificate removed."));
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  if (error && !info) return <Callout kind="bad">{error}</Callout>;
  if (!info) return <Loading />;

  return (
    <div className="panel stack-sm" style={{ background: "var(--sunken)" }}>
      <strong className="row" style={{ gap: 8 }}>
        <KeyRound size={16} aria-hidden /> {t("Certificate")}
      </strong>
      {error && <Callout kind="bad">{error}</Callout>}
      {info.present && info.fingerprints ? (
        <>
          <p className="small muted">
            {t("Add one of these fingerprints to your network account. Most networks (Libera.Chat, Atheme or Anope) use SHA-512 or SHA-256; OFTC uses SHA-1.")}
          </p>
          <div className="field">
            <label>SHA-512</label>
            <CopyText text={info.fingerprints.sha512} />
          </div>
          <div className="field">
            <label>SHA-256</label>
            <CopyText text={info.fingerprints.sha256} />
          </div>
          <div className="field">
            <label>SHA-1</label>
            <CopyText text={info.fingerprints.sha1} />
          </div>
        </>
      ) : (
        <p className="small muted">{t("This network has no certificate yet.")}</p>
      )}
      {changed && (
        <Callout kind="warn">
          {tx("The certificate is used when connecting. {reconnect} to apply it.", {
            reconnect: (
              <button type="button" className="btn btn-sm" onClick={onReconnect}>
                <RefreshCw size={14} /> {t("Reconnect")}
              </button>
            ),
          })}
        </Callout>
      )}
      {editable && (
        <div className="row">
          <button type="button" className="btn" onClick={generate} disabled={busy}>
            <KeyRound size={16} /> {info.present ? t("Generate another") : t("Generate certificate")}
          </button>
          <button type="button" className="btn btn-ghost" onClick={() => setUploading(true)}>
            <Upload size={16} /> {t("Upload mine")}
          </button>
          {info.present && (
            <button type="button" className="btn btn-ghost btn-danger" onClick={() => setRemoving(true)}>
              <Trash2 size={16} /> {t("Delete")}
            </button>
          )}
        </div>
      )}
      {uploading && (
        <UploadCert
          url={url}
          onClose={() => setUploading(false)}
          onDone={(i) => {
            setInfo(i);
            setChanged(true);
            setUploading(false);
          }}
        />
      )}
      {removing && (
        <Confirm title={t("Delete the certificate")} confirmLabel={t("Delete")} onClose={() => setRemoving(false)} onConfirm={remove}>
          <p>{t("If your network account only accepts this certificate, you will not be able to identify until you add another one.")}</p>
        </Confirm>
      )}
    </div>
  );
}

function UploadCert({ url, onClose, onDone }: { url: string; onClose: () => void; onDone: (i: CertInfo) => void }) {
  const [cert, setCert] = useState("");
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      onDone(await api.put<CertInfo>(url, { certificate: cert, private_key: key }));
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal title={t("Upload certificate")} onClose={onClose}>
      <form className="stack" onSubmit={submit}>
        {error && <Callout kind="bad">{error}</Callout>}
        <div className="field">
          <label htmlFor="up-cert">{t("Certificate (PEM)")}</label>
          <textarea id="up-cert" className="input input-mono" rows={5} value={cert} onChange={(e) => setCert(e.target.value)} placeholder="-----BEGIN CERTIFICATE-----" spellCheck={false} />
        </div>
        <div className="field">
          <label htmlFor="up-key">{t("Private key (PEM)")}</label>
          <textarea id="up-key" className="input input-mono" rows={5} value={key} onChange={(e) => setKey(e.target.value)} placeholder="-----BEGIN PRIVATE KEY-----" spellCheck={false} />
          <span className="hint">{t("It is stored encrypted and never shown again.")}</span>
        </div>
        <div className="row row-end">
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            {t("Cancel")}
          </button>
          <button className="btn btn-primary" disabled={busy || !cert.trim() || !key.trim()}>
            {busy ? t("Saving…") : t("Save certificate")}
          </button>
        </div>
      </form>
    </Modal>
  );
}
