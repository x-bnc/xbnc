import { useEffect, useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { Copy } from "lucide-react";
import { api, errorText, type Network } from "../lib/api";
import { useMe } from "../lib/session";
import { useToast } from "../lib/toast";
import { colorFor } from "../lib/format";
import { t } from "../lib/i18n";
import { builtinName, builtinWhere, type ModuleState } from "../lib/modules";
import type { NetworkPlugin } from "../lib/plugins";
import { Callout, Empty, Loading, Modal } from "../components/ui";
import { ModulesTab, useCatalog } from "../components/Modules";

const LAST = "xbnc.modules.network";

/** Modules and plugins of one network at a time, picked from a list that
 * stays usable with many networks, plus copying a setup to other networks. */
export function ModulesPage() {
  const me = useMe();
  const { cat, error: catError } = useCatalog();
  const [nets, setNets] = useState<Network[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [params, setParams] = useSearchParams();
  const [copying, setCopying] = useState(false);
  const [filter, setFilter] = useState("");
  const [reload, setReload] = useState(0);

  useEffect(() => {
    api
      .get<Network[]>("/networks")
      .then((list) => setNets(list.filter((n) => n.user_id === me.user.id).sort((a, b) => a.name.localeCompare(b.name))))
      .catch((e) => setError(errorText(e)));
  }, [me.user.id]);

  const selected = useMemo(() => {
    if (!nets || nets.length === 0) return null;
    const want = Number(params.get("net")) || Number((() => { try { return localStorage.getItem(LAST); } catch { return null; } })());
    return nets.find((n) => n.id === want) ?? nets[0];
  }, [nets, params]);

  const pick = (n: Network) => {
    setParams({ net: String(n.id) }, { replace: true });
    try {
      localStorage.setItem(LAST, String(n.id));
    } catch {
      /* private mode */
    }
  };

  if (catError || error) return <Callout kind="bad">{catError ?? error}</Callout>;
  if (!cat || !nets) return <Loading />;
  const editable = me.permissions.manage_own_networks;
  const shown = filter ? nets.filter((n) => n.name.toLowerCase().includes(filter.toLowerCase())) : nets;

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>{t("Modules")}</h1>
          <p>{t("Pick a network and turn on or set up its modules and plugins. To avoid repeating the work on every network, copy one network's setup to the others.")}</p>
        </div>
      </header>

      {nets.length === 0 ? (
        <Empty title={t("You have no networks yet")}>{t("Modules are turned on per network. Add a network to get started.")}</Empty>
      ) : (
        <>
          <section className="section stack-sm">
            <div className="spread">
              <h2 style={{ margin: 0 }}>{t("Network")}</h2>
              {nets.length > 6 && (
                <input className="input input-sm" placeholder={t("Find network")} value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("Find network")} />
              )}
            </div>
            <div className="net-picker" role="tablist" aria-label={t("Network")}>
              {shown.map((n) => (
                <button
                  key={n.id}
                  type="button"
                  role="tab"
                  aria-selected={selected?.id === n.id}
                  className={`net-chip${selected?.id === n.id ? " on" : ""}`}
                  onClick={() => pick(n)}
                >
                  <span className="dot" style={{ background: colorFor(n.name) }} />
                  {n.name}
                </button>
              ))}
            </div>
          </section>

          {selected && (
            <section className="section stack-sm">
              <div className="spread">
                <h2 style={{ margin: 0 }}>
                  <Link to={`/networks/${selected.id}`}>{selected.name}</Link>
                </h2>
                {editable && nets.length > 1 && (
                  <button className="btn btn-sm" onClick={() => setCopying(true)}>
                    <Copy size={15} aria-hidden /> {t("Copy to other networks…")}
                  </button>
                )}
              </div>
              <ModulesTab key={`${selected.id}-${reload}`} networkId={selected.id} editable={editable} onReconnect={() => api.post(`/networks/${selected.id}/control`, { action: "jump" }).catch(() => {})} />
            </section>
          )}
        </>
      )}

      <section className="section">
        <h2>{t("Always on")}</h2>
        <p className="muted">{t("Core features that need no switching on.")}</p>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>{t("Feature")}</th>
                <th>{t("Where to configure it")}</th>
              </tr>
            </thead>
            <tbody>
              {cat.builtin.map((id) => (
                <tr key={id}>
                  <td>
                    <strong>{builtinName(id)}</strong>
                  </td>
                  <td className="small">{builtinWhere(id)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      {copying && selected && (
        <CopySetup
          from={selected}
          others={nets.filter((n) => n.id !== selected.id)}
          onClose={() => setCopying(false)}
          onDone={() => {
            setCopying(false);
            setReload((r) => r + 1);
          }}
        />
      )}
    </div>
  );
}

/** Copies the modules and plugins (on/off and settings) of one network to others. */
function CopySetup({ from, others, onClose, onDone }: { from: Network; others: Network[]; onClose: () => void; onDone: () => void }) {
  const toast = useToast();
  const [targets, setTargets] = useState<number[]>(others.map((n) => n.id));
  const [plugins, setPlugins] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const mods = (await api.get<{ modules: ModuleState[] }>(`/networks/${from.id}/modules`)).modules;
      // The client certificate belongs to one network: it is not copied.
      const portable = mods.filter((m) => m.id !== "cert");
      const plugs = plugins ? (await api.get<{ plugins: NetworkPlugin[] }>(`/networks/${from.id}/plugins`)).plugins : [];
      for (const id of targets) {
        for (const m of portable) await api.put(`/networks/${id}/modules/${m.id}`, { enabled: m.enabled, config: m.config });
        for (const p of plugs) await api.put(`/networks/${id}/plugins/${p.id}`, { enabled: p.enabled, config: p.config });
      }
      toast.ok(t("Setup copied to {n} networks.", { n: targets.length }));
      onDone();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={t("Copy the setup of {network}", { network: from.name })} onClose={onClose}>
      <div className="stack-sm">
        <p className="small muted">{t("The ticked networks get the same modules on and the same settings as {network}. The client certificate is not copied.", { network: from.name })}</p>
        {error && <Callout kind="bad">{error}</Callout>}
        <div className="row" style={{ gap: 8 }}>
          <button type="button" className="btn btn-sm btn-ghost" onClick={() => setTargets(others.map((n) => n.id))}>
            {t("All")}
          </button>
          <button type="button" className="btn btn-sm btn-ghost" onClick={() => setTargets([])}>
            {t("None")}
          </button>
        </div>
        <div className="copy-targets">
          {others.map((n) => (
            <label key={n.id} className="row" style={{ gap: 8 }}>
              <input type="checkbox" checked={targets.includes(n.id)} onChange={(e) => setTargets(e.target.checked ? [...targets, n.id] : targets.filter((x) => x !== n.id))} />
              <span className="dot" style={{ background: colorFor(n.name) }} />
              {n.name}
            </label>
          ))}
        </div>
        <label className="row" style={{ gap: 8 }}>
          <input type="checkbox" checked={plugins} onChange={(e) => setPlugins(e.target.checked)} />
          {t("Copy the plugins too")}
        </label>
        <div className="row row-end">
          <button className="btn btn-ghost" onClick={onClose}>
            {t("Cancel")}
          </button>
          <button className="btn btn-primary" onClick={run} disabled={busy || targets.length === 0}>
            {busy ? t("Copying…") : t("Copy")}
          </button>
        </div>
      </div>
    </Modal>
  );
}
