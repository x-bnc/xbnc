import { useEffect, useState, type FormEvent } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { api, errorText } from "../lib/api";
import { useToast } from "../lib/toast";
import { t } from "../lib/i18n";
import { PERMISSION_TEXT, localized, type NetworkPlugin, type PluginManifest, type PluginSetting } from "../lib/plugins";
import { Callout, Switch } from "./ui";

type Config = NetworkPlugin["config"];

export function PermissionList({ permissions }: { permissions: string[] }) {
  if (permissions.length === 0) {
    return <p className="small muted">{t("Asks for no permissions: it can only talk to you.")}</p>;
  }
  return (
    <ul className="perm-list">
      {permissions.map((p) => (
        <li key={p}>
          <strong>{t(PERMISSION_TEXT[p]?.name ?? p)}</strong>
          <span className="small muted">{t(PERMISSION_TEXT[p]?.help ?? "")}</span>
        </li>
      ))}
    </ul>
  );
}

/** Plugins section of a network's Modules tab. */
export function NetworkPlugins({ networkId, editable }: { networkId: number; editable: boolean }) {
  const [list, setList] = useState<NetworkPlugin[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const toast = useToast();

  useEffect(() => {
    api
      .get<{ plugins: NetworkPlugin[] }>(`/networks/${networkId}/plugins`)
      .then((r) => setList(r.plugins))
      .catch((e) => setError(errorText(e)));
  }, [networkId]);

  const save = async (id: string, enabled: boolean, config: Config) => {
    const r = await api.put<{ enabled: boolean; config: Config }>(`/networks/${networkId}/plugins/${id}`, { enabled, config });
    setList((l) => l?.map((p) => (p.id === id ? { ...p, enabled: r.enabled, config: r.config } : p)) ?? null);
  };

  const toggle = async (p: NetworkPlugin, on: boolean) => {
    try {
      await save(p.id, on, p.config);
      const name = localized(p.manifest).name;
      toast.ok(on ? t("{module} turned on.", { module: name }) : t("{module} turned off.", { module: name }));
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  if (error) return <Callout kind="bad">{error}</Callout>;
  if (!list || list.length === 0) return null;
  return (
    <section className="stack-sm">
      <h3>{t("Plugins")}</h3>
      <p className="small muted">{t("Installed by this server's administrator. They run isolated and can only do what their permissions allow.")}</p>
      <ul className="module-list">
        {list.map((p) => {
          const expanded = open === p.id;
          const txt = localized(p.manifest);
          return (
            <li key={p.id} className={`module${p.enabled ? " on" : ""}`}>
              <div className="module-head">
                <button type="button" className="module-title" aria-expanded={expanded} onClick={() => setOpen(expanded ? null : p.id)}>
                  {expanded ? <ChevronDown size={16} aria-hidden /> : <ChevronRight size={16} aria-hidden />}
                  <span>
                    <strong>
                      {txt.name} <span className="small faint">{p.manifest.version}</span>
                    </strong>
                    <span className="small muted">{txt.description}</span>
                  </span>
                </button>
                <Switch checked={p.enabled} onChange={(v) => toggle(p, v)} label={txt.name} disabled={!editable} hideLabel />
              </div>
              {expanded && (
                <div className="module-body">
                  {txt.usage && <PluginUsage text={txt.usage} />}
                  <PermissionList permissions={p.manifest.permissions} />
                  <p className="small muted">{t("From your IRC client: /msg *{id} help (write something after the name; otherwise your client shows its own /msg help).", { id: p.id })}</p>
                  {p.manifest.settings.length > 0 && (
                    <PluginForm manifest={p.manifest} config={p.config} editable={editable} onSave={(c) => save(p.id, p.enabled, c)} />
                  )}
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/** How to use a plugin, one line per paragraph. */
export function PluginUsage({ text }: { text: string }) {
  return (
    <div className="plugin-usage">
      <strong className="small">{t("How to use it")}</strong>
      {text.split("\n").filter(Boolean).map((l, i) => (
        <p key={i} className="small" style={{ margin: 0 }}>
          {l}
        </p>
      ))}
    </div>
  );
}

function PluginForm({ manifest, config, editable, onSave }: { manifest: PluginManifest; config: Config; editable: boolean; onSave: (c: Config) => Promise<unknown> }) {
  const settings: PluginSetting[] = manifest.settings;
  const txt = localized(manifest);
  const [draft, setDraft] = useState<Config>(config);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const toast = useToast();
  const dirty = JSON.stringify(draft) !== JSON.stringify(config);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await onSave(draft);
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
      {settings.map((s) => {
        const id = `p-${s.key}`;
        const v = draft[s.key];
        const label = txt.label(s);
        const help = txt.help(s);
        if (s.type === "bool") {
          return <Switch key={s.key} checked={v === true} onChange={(on) => setDraft({ ...draft, [s.key]: on })} label={label} description={help || undefined} disabled={!editable} />;
        }
        return (
          <div className="field" key={s.key}>
            <label htmlFor={id}>{label}</label>
            {s.type === "lines" ? (
              <textarea
                id={id}
                className="input input-mono"
                rows={Math.min(8, Math.max(3, String(v ?? "").split("\n").length + 1))}
                value={String(v ?? "")}
                disabled={!editable}
                spellCheck={false}
                onChange={(e) => setDraft({ ...draft, [s.key]: e.target.value })}
              />
            ) : (
              <input
                id={id}
                className="input"
                type={s.type === "int" ? "number" : "text"}
                min={s.min ?? undefined}
                max={s.max ?? undefined}
                maxLength={s.type === "text" ? 400 : undefined}
                value={String(v ?? "")}
                disabled={!editable}
                onChange={(e) => setDraft({ ...draft, [s.key]: s.type === "int" ? Number(e.target.value) : e.target.value })}
              />
            )}
            {(help || (s.type === "int" && s.min != null && s.max != null)) && (
              <span className="hint">
                {help}
                {s.type === "int" && s.min != null && s.max != null && <> {t("Between {min} and {max}.", { min: s.min, max: s.max })}</>}
              </span>
            )}
          </div>
        );
      })}
      {editable && (
        <div className="row row-end">
          {dirty && (
            <button type="button" className="btn btn-ghost" onClick={() => setDraft(config)}>
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
