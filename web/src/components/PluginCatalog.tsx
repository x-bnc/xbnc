import { useEffect, useState } from "react";
import { BadgeCheck, Store } from "lucide-react";
import { api, errorText } from "../lib/api";
import { useToast } from "../lib/toast";
import { localized, type PluginManifest } from "../lib/plugins";
import { PermissionList } from "./Plugins";
import { Callout } from "./ui";
import { t } from "../lib/i18n";

interface Entry {
  id: string;
  source: "official" | "remote";
  manifest: PluginManifest;
  size: number;
  installed_version: string | null;
}

interface Catalog {
  plugins: Entry[];
  remote: string | null;
  remote_error: string | null;
}

/** Official plugins (and a signed remote catalog), installed in one click. */
export function PluginCatalog({ onInstalled }: { onInstalled: () => void }) {
  const toast = useToast();
  const [cat, setCat] = useState<Catalog | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [open, setOpen] = useState(false);

  const load = () => api.get<Catalog>("/plugins/catalog").then(setCat).catch(() => {});
  useEffect(() => {
    load();
  }, []);
  if (!cat) return null;

  const install = async (e: Entry) => {
    setBusy(e.id);
    try {
      await api.post(`/plugins/catalog/${encodeURIComponent(e.id)}`);
      toast.ok(
        (e.manifest.http_domains?.length ?? 0) > 0
          ? t("{name} installed. Check below the sites it may connect to.", { name: localized(e.manifest).name })
          : t("{name} installed. Turn it on in each network's Modules tab.", { name: localized(e.manifest).name }),
      );
      await load();
      onInstalled();
    } catch (err) {
      toast.error(errorText(err));
    } finally {
      setBusy(null);
    }
  };

  const pending = cat.plugins.filter((e) => e.installed_version !== e.manifest.version).length;

  return (
    <section className="panel stack-sm">
      <div className="spread">
        <div>
          <h2 style={{ margin: 0 }}>
            <Store size={20} aria-hidden /> {t("Catalog")}
          </h2>
          <p className="small muted" style={{ margin: "4px 0 0" }}>
            {t("Official xBNC plugins, ready to install in one click.")}
            {pending > 0 && ` ${t("{n} available or with a new version.", { n: pending })}`}
          </p>
        </div>
        <button className="btn btn-sm" onClick={() => setOpen(!open)}>
          {open ? t("Hide") : t("Show the catalog")}
        </button>
      </div>
      {open && cat.remote_error && <Callout kind="warn">{t("Could not read the external catalog: {error}", { error: cat.remote_error })}</Callout>}
      {open && (
        <div className="catalog-grid">
          {cat.plugins.map((e) => {
            const l = localized(e.manifest);
            const installed = e.installed_version;
            const upgrade = installed && installed !== e.manifest.version;
            return (
              <article key={e.id} className="catalog-card stack-sm">
                <div className="spread" style={{ alignItems: "flex-start" }}>
                  <strong>{l.name}</strong>
                  <span className="small faint">
                    {e.source === "official" && <BadgeCheck size={14} aria-label={t("Official")} />} {e.manifest.version}
                  </span>
                </div>
                <p className="small" style={{ margin: 0 }}>{l.description}</p>
                <PermissionList permissions={e.manifest.permissions} />
                <div className="row row-end">
                  {installed && !upgrade ? (
                    <span className="small muted">{t("Installed")}</span>
                  ) : (
                    <button className="btn btn-sm btn-primary" disabled={busy !== null} onClick={() => install(e)}>
                      {busy === e.id ? t("Installing…") : upgrade ? t("Update to {v}", { v: e.manifest.version }) : t("Install")}
                    </button>
                  )}
                </div>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
