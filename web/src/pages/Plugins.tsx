import { useCallback, useEffect, useState, type ChangeEvent } from "react";
import { FileUp, Trash2 } from "lucide-react";
import { api, errorText, uploadRaw } from "../lib/api";
import { useToast } from "../lib/toast";
import { bytes, formatDate } from "../lib/format";
import { t, tx } from "../lib/i18n";
import { localized, type InstalledPlugin, type PluginManifest } from "../lib/plugins";
import { PermissionList, PluginUsage } from "../components/Plugins";
import { PluginDev } from "../components/PluginDev";
import { PluginCatalog } from "../components/PluginCatalog";
import { Callout, Confirm, Empty, Loading, Modal, Switch } from "../components/ui";

interface ListResp {
  plugins: InstalledPlugin[];
  can_manage: boolean;
  abi: number;
}

export function PluginsPage() {
  const toast = useToast();
  const [data, setData] = useState<ListResp | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const [removing, setRemoving] = useState<InstalledPlugin | null>(null);

  const load = useCallback(() => api.get<ListResp>("/plugins").then(setData).catch((e) => setError(errorText(e))), []);
  useEffect(() => {
    load();
  }, [load]);

  const toggle = async (p: InstalledPlugin, on: boolean) => {
    try {
      await api.patch(`/plugins/${p.id}`, { enabled: on });
      toast.ok(on ? t("{module} turned on.", { module: localized(p.manifest).name }) : t("{module} turned off.", { module: localized(p.manifest).name }));
      load();
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  const remove = async () => {
    if (!removing) return;
    try {
      await api.del(`/plugins/${removing.id}`);
      toast.ok(t("{name} uninstalled.", { name: localized(removing.manifest).name }));
      setRemoving(null);
      load();
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  if (error && !data) return <Callout kind="bad">{error}</Callout>;
  if (!data) return <Loading />;

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>{t("Plugins")}</h1>
          <p>{t("Extra features made by other people, in WebAssembly. Each plugin runs isolated: it cannot read files or see other networks, only reaches the internet if you approve its domains, and only does what its permissions allow.")}</p>
        </div>
        {data.can_manage && (
          <button className="btn btn-primary" onClick={() => setInstalling(true)}>
            <FileUp size={18} /> {t("Install plugin")}
          </button>
        )}
      </header>

      {data.plugins.length === 0 ? (
        <Empty title={t("No plugins installed")}>
          {data.can_manage
            ? tx("Install a .wasm file made with the xBNC SDK. There are examples in {path} and the guide is in docs/PLUGINS.md.", { path: <code>sdk/c/examples</code> })
            : t("When an administrator installs one, you can turn it on in each network's Modules tab.")}
        </Empty>
      ) : (
        <ul className="plugin-cards">
          {data.plugins.map((p) => (
            <li key={p.id} className={`panel stack-sm${p.enabled ? "" : " is-off"}`}>
              <div className="spread">
                <div>
                  <h2 style={{ margin: 0 }}>
                    {localized(p.manifest).name} <span className="small faint">{p.manifest.version}</span>
                  </h2>
                  <p className="small muted" style={{ margin: "2px 0 0" }}>
                    <code>*{p.id}</code>
                    {p.manifest.author && <> · {t("by {author}", { author: p.manifest.author })}</>}
                  </p>
                </div>
                {data.can_manage && (
                  <div className="row">
                    <Switch checked={p.enabled} onChange={(v) => toggle(p, v)} label={t("Available on the server")} hideLabel />
                    <button type="button" className="icon-btn" aria-label={t("Uninstall {name}", { name: localized(p.manifest).name })} onClick={() => setRemoving(p)}>
                      <Trash2 size={17} />
                    </button>
                  </div>
                )}
              </div>
              {localized(p.manifest).description && <p style={{ margin: 0 }}>{localized(p.manifest).description}</p>}
              {!p.loaded && <Callout kind="bad">{t("It could not be loaded: the file is missing or changed since it was installed. Install it again.")}</Callout>}
              {!p.enabled && <p className="small muted">{t("Turned off on the server: it does not run on any network.")}</p>}
              {localized(p.manifest).usage && <PluginUsage text={localized(p.manifest).usage} />}
              <PermissionList permissions={p.manifest.permissions} />
              {(p.manifest.http_domains?.length ?? 0) > 0 && (
                <HttpDomains
                  domains={p.manifest.http_domains ?? []}
                  approved={p.http_approved}
                  editable={data.can_manage}
                  onSave={async (list) => {
                    try {
                      await api.patch(`/plugins/${p.id}`, { http_approved: list });
                      toast.ok(t("Domains saved."));
                      load();
                    } catch (e) {
                      toast.error(errorText(e));
                    }
                  }}
                />
              )}
              <p className="small faint" style={{ margin: 0 }}>
                {t("Installed on {date} by {user}. {size}.", { date: formatDate(p.installed_at), user: p.installed_by, size: bytes(p.size) })}
                {p.manifest.homepage && (
                  <>
                    {" "}
                    <a href={p.manifest.homepage} target="_blank" rel="noreferrer noopener">
                      {t("Plugin page")}
                    </a>
                  </>
                )}
              </p>
            </li>
          ))}
        </ul>
      )}

      <p className="small muted">{t("Each user turns on the plugins they want in their networks' Modules tab, or with /msg *status enable <plugin>.")}</p>

      {data.can_manage && <PluginCatalog onInstalled={load} />}
      {data.can_manage && <PluginDev onInstalled={load} />}

      {installing && <InstallPlugin onClose={() => setInstalling(false)} onDone={() => { setInstalling(false); load(); }} />}
      {removing && (
        <Confirm title={t("Uninstall {name}?", { name: localized(removing.manifest).name })} confirmLabel={t("Uninstall")} onConfirm={remove} onClose={() => setRemoving(null)}>
          {t("It is removed from every network, with its settings and saved data.")}
        </Confirm>
      )}
    </div>
  );
}

/** Raw upload: the API takes the .wasm bytes as the request body. */
const upload = <T,>(path: string, file: File) => uploadRaw<T>(path, file, "application/wasm");

/** The sites a plugin asks to reach, and which ones are approved. */
function HttpDomains({ domains, approved, editable, onSave }: { domains: string[]; approved: string[]; editable: boolean; onSave: (list: string[]) => void }) {
  const [sel, setSel] = useState<string[]>(approved);
  useEffect(() => setSel(approved), [approved.join(",")]);
  const changed = sel.slice().sort().join(",") !== approved.slice().sort().join(",");
  return (
    <div className="stack-sm">
      <strong className="small">{t("Sites it may connect to")}</strong>
      <ul className="stack-sm" style={{ listStyle: "none", padding: 0, margin: 0 }}>
        {domains.map((d) => (
          <li key={d}>
            <label className="row" style={{ gap: 8 }}>
              <input
                type="checkbox"
                checked={sel.includes(d)}
                disabled={!editable}
                onChange={(e) => setSel(e.target.checked ? [...sel, d] : sel.filter((x) => x !== d))}
              />
              <code>{d}</code>
              <span className="small faint">{approved.includes(d) ? t("approved") : t("not approved")}</span>
            </label>
          </li>
        ))}
      </ul>
      {editable && changed && (
        <div className="row">
          <button className="btn btn-sm" onClick={() => onSave(sel)}>
            {t("Save domains")}
          </button>
        </div>
      )}
    </div>
  );
}

function InstallPlugin({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const toast = useToast();
  const [file, setFile] = useState<File | null>(null);
  const [info, setInfo] = useState<{ manifest: PluginManifest; sha256: string; installed_version: string | null } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [approve, setApprove] = useState<string[]>([]);

  const pick = async (e: ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    if (!f) return;
    setFile(f);
    setInfo(null);
    setError(null);
    if (f.size > 4 * 1024 * 1024) {
      setError(t("The plugin is larger than 4 MB."));
      return;
    }
    setBusy(true);
    try {
      const i = await upload<{ manifest: PluginManifest; sha256: string; installed_version: string | null }>("/plugins/inspect", f);
      setInfo(i);
      setApprove(i.manifest.http_domains ?? []);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const install = async () => {
    if (!file) return;
    setBusy(true);
    try {
      const r = await upload<{ id: string }>("/plugins", file);
      if ((info?.manifest.http_domains?.length ?? 0) > 0) {
        await api.patch(`/plugins/${r.id}`, { http_approved: approve });
      }
      toast.ok(t("{name} installed.", { name: info ? localized(info.manifest).name : file.name }));
      onDone();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={t("Install plugin")} onClose={onClose} wide>
      <p className="muted">{t("Choose the .wasm file. Before installing it you will see what it is and which permissions it asks for.")}</p>
      <label className="btn" style={{ alignSelf: "flex-start" }}>
        <FileUp size={18} aria-hidden /> {file?.name ?? t("Choose .wasm file")}
        <input type="file" accept=".wasm,application/wasm" className="sr-only" onChange={pick} />
      </label>
      {busy && !info && <p className="small muted">{t("Checking the plugin…")}</p>}
      {error && <Callout kind="bad">{error}</Callout>}
      {info && (
        <div className="stack-sm">
          <h3 style={{ margin: 0 }}>
            {localized(info.manifest).name} <span className="small faint">{info.manifest.version}</span>
          </h3>
          {localized(info.manifest).description && <p style={{ margin: 0 }}>{localized(info.manifest).description}</p>}
          <p className="small muted" style={{ margin: 0 }}>
            <code>*{info.manifest.id}</code>
            {info.manifest.author && <> · {t("by {author}", { author: info.manifest.author })}</>}
          </p>
          {info.installed_version && (
            <Callout>{t("Version {version} is already installed. It will be upgraded and every network keeps its settings.", { version: info.installed_version })}</Callout>
          )}
          <strong>{t("Permissions it asks for")}</strong>
          <PermissionList permissions={info.manifest.permissions} />
          {(info.manifest.http_domains?.length ?? 0) > 0 && (
            <div className="stack-sm">
              <strong>{t("Sites it wants to connect to")}</strong>
              <p className="small muted" style={{ margin: 0 }}>{t("Tick the ones you approve. The rest stay blocked; you can change this later.")}</p>
              {info.manifest.http_domains!.map((d) => (
                <label key={d} className="row" style={{ gap: 8 }}>
                  <input type="checkbox" checked={approve.includes(d)} onChange={(e) => setApprove(e.target.checked ? [...approve, d] : approve.filter((x) => x !== d))} />
                  <code>{d}</code>
                </label>
              ))}
            </div>
          )}
          <p className="small faint" style={{ margin: 0, overflowWrap: "anywhere" }}>
            SHA-256: <code>{info.sha256}</code>
          </p>
          <Callout kind="warn">{t("Only install plugins from people you trust: with permission to send messages, they can write on behalf of whoever turns them on.")}</Callout>
        </div>
      )}
      <div className="row row-end">
        <button className="btn btn-ghost" onClick={onClose}>
          {t("Cancel")}
        </button>
        <button className="btn btn-primary" disabled={!info || busy} onClick={install}>
          {busy && info ? t("Installing…") : info?.installed_version ? t("Upgrade") : t("Install")}
        </button>
      </div>
    </Modal>
  );
}
