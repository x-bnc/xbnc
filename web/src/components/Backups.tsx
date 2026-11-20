import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { Archive, Download, RotateCcw, Trash2 } from "lucide-react";
import { api, errorText, uploadRaw, type Settings } from "../lib/api";
import { useToast } from "../lib/toast";
import { bytes, formatDate } from "../lib/format";
import { Callout, Confirm, Field, Switch } from "./ui";
import { t } from "../lib/i18n";

interface Backup {
  name: string;
  size: number;
  created_at: number;
}

interface BackupList {
  postgres: boolean;
  backups: Backup[];
  restore_pending: boolean;
  folder: string;
  settings: Settings["backups"];
}

/** Restarts the service and reloads the panel when it is back. */
export async function restartAndWait(): Promise<void> {
  await api.post("/system/restart");
  await new Promise((r) => setTimeout(r, 2500));
  for (let i = 0; i < 60; i++) {
    try {
      await fetch("/api/v1/health", { cache: "no-store" }).then((r) => {
        if (!r.ok) throw new Error();
      });
      window.location.reload();
      return;
    } catch {
      await new Promise((r) => setTimeout(r, 1000));
    }
  }
  throw new Error(t("xBNC is not back after a minute. Check the service on the server."));
}

export function Backups({ onSettings }: { onSettings?: (s: Settings) => void }) {
  const toast = useToast();
  const [data, setData] = useState<BackupList | null>(null);
  const [auto, setAuto] = useState<Settings["backups"] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<{ kind: "restore" | "delete"; b: Backup } | null>(null);
  const file = useRef<HTMLInputElement>(null);

  const load = () =>
    api
      .get<BackupList>("/backups")
      .then((d) => {
        setData(d);
        setAuto(d.settings);
      })
      .catch((e) => setError(errorText(e)));
  useEffect(() => {
    load();
  }, []);
  if (!data || !auto) return error ? <Callout kind="bad">{error}</Callout> : null;

  const run = async (key: string, f: () => Promise<unknown>, ok?: string) => {
    setBusy(key);
    setError(null);
    try {
      await f();
      if (ok) toast.ok(ok);
      await load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const create = () => run("create", () => api.post("/backups"), t("Backup created."));
  const remove = (b: Backup) => run("delete", () => api.del(`/backups/${encodeURIComponent(b.name)}`), t("Backup deleted."));
  const restore = (b: Backup) => run("restore", () => api.post(`/backups/${encodeURIComponent(b.name)}/restore`));
  const cancel = () => run("cancel", () => api.del("/backups/restore"), t("Restore cancelled."));
  const uploaded = (e: ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    e.target.value = "";
    if (f) run("upload", () => uploadRaw("/backups/restore", f, "application/gzip"));
  };
  const saveAuto = () =>
    run(
      "auto",
      async () => {
        const cur = await api.get<{ current: Settings }>("/settings");
        const r = await api.put<{ current: Settings }>("/settings", { ...cur.current, backups: auto });
        onSettings?.(r.current);
      },
      t("Automatic backups saved."),
    );
  const restart = async () => {
    setBusy("restart");
    try {
      await restartAndWait();
    } catch (e) {
      setError(errorText(e));
      setBusy(null);
    }
  };

  const autoDirty = JSON.stringify(auto) !== JSON.stringify(data.settings);

  if (data.postgres) {
    return (
      <section className="panel stack-sm">
        <h2>
          <Archive size={20} aria-hidden /> {t("Backups")}
        </h2>
        <Callout>{t("This server keeps its data in PostgreSQL: back up with pg_dump or your database tools. Keep the encryption key (secret_key_file) too.")}</Callout>
      </section>
    );
  }

  return (
    <section className="panel stack-sm">
      <div className="panel-head">
        <div>
          <h2>
            <Archive size={20} aria-hidden /> {t("Backups")}
          </h2>
          <p>{t("Each backup holds the database, the encryption key and the plugins. It contains every password: keep it somewhere safe.")}</p>
        </div>
        <button className="btn" onClick={create} disabled={busy !== null}>
          {busy === "create" ? t("Creating…") : t("Back up now")}
        </button>
      </div>
      {error && <Callout kind="bad">{error}</Callout>}
      {data.restore_pending && (
        <Callout kind="warn">
          <div className="stack-sm">
            <span>{t("A restore is ready. It is applied when xBNC restarts; the current data is kept aside just in case.")}</span>
            <div className="row">
              <button className="btn btn-sm btn-primary" onClick={restart} disabled={busy !== null}>
                {busy === "restart" ? t("Restarting…") : t("Restart now")}
              </button>
              <button className="btn btn-sm" onClick={cancel} disabled={busy !== null}>
                {t("Cancel the restore")}
              </button>
            </div>
          </div>
        </Callout>
      )}
      {data.backups.length === 0 ? (
        <p className="small muted">{t("No backups yet. They are saved in {folder}.", { folder: data.folder })}</p>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>{t("Date")}</th>
                <th>{t("Size")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {data.backups.map((b) => (
                <tr key={b.name}>
                  <td title={b.name}>{formatDate(b.created_at)}</td>
                  <td>{bytes(b.size)}</td>
                  <td className="row row-end" style={{ gap: 4 }}>
                    <a className="icon-btn" href={`/api/v1/backups/${encodeURIComponent(b.name)}`} download aria-label={t("Download")} title={t("Download")}>
                      <Download size={16} />
                    </a>
                    <button className="icon-btn" onClick={() => setConfirm({ kind: "restore", b })} aria-label={t("Restore")} title={t("Restore")} disabled={busy !== null}>
                      <RotateCcw size={16} />
                    </button>
                    <button className="icon-btn" onClick={() => setConfirm({ kind: "delete", b })} aria-label={t("Delete")} title={t("Delete")} disabled={busy !== null}>
                      <Trash2 size={16} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <div className="row">
        <button className="btn btn-sm" onClick={() => file.current?.click()} disabled={busy !== null}>
          {busy === "upload" ? t("Uploading…") : t("Restore from a file…")}
        </button>
        <input ref={file} type="file" accept=".tar.gz,.tgz,application/gzip" hidden onChange={uploaded} />
      </div>

      <h3>{t("Automatic backups")}</h3>
      <Switch checked={auto.enabled} onChange={(v) => setAuto({ ...auto, enabled: v })} label={t("Make automatic backups")} />
      <div className="grid-2" style={{ opacity: auto.enabled ? 1 : 0.55 }}>
        <Field label={t("Every how many hours")} type="number" min={1} max={720} value={auto.interval_hours} onChange={(e) => setAuto({ ...auto, interval_hours: Number(e.target.value) })} />
        <Field label={t("Backups kept")} type="number" min={1} max={365} value={auto.keep} onChange={(e) => setAuto({ ...auto, keep: Number(e.target.value) })} hint={t("The oldest ones are deleted automatically.")} />
      </div>
      {autoDirty && (
        <div className="row row-end">
          <button className="btn btn-primary btn-sm" onClick={saveAuto} disabled={busy !== null}>
            {t("Save automatic backups")}
          </button>
        </div>
      )}

      {confirm && (
        <Confirm
          title={confirm.kind === "restore" ? t("Restore this backup?") : t("Delete this backup?")}
          confirmLabel={confirm.kind === "restore" ? t("Prepare the restore") : t("Delete")}
          busy={busy !== null}
          onClose={() => setConfirm(null)}
          onConfirm={async () => {
            const c = confirm;
            setConfirm(null);
            if (c.kind === "restore") await restore(c.b);
            else await remove(c.b);
          }}
        >
          {confirm.kind === "restore"
            ? t("It is checked now and applied when xBNC restarts. Everything goes back to how it was on {date}; the current data is kept aside.", { date: formatDate(confirm.b.created_at) })
            : t("The backup from {date} is deleted from the server.", { date: formatDate(confirm.b.created_at) })}
        </Confirm>
      )}
    </section>
  );
}
