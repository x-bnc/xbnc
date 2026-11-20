import { useEffect, useState } from "react";
import { api, errorText } from "../lib/api";
import { formatDate } from "../lib/format";
import { t, tx } from "../lib/i18n";
import { Callout, Confirm } from "./ui";

export interface UpdateInfo {
  current: string;
  configured: boolean;
  latest: string | null;
  newer: boolean;
  url: string | null;
  notes: string | null;
  checked_at: number | null;
  error: string | null;
  one_click: boolean;
  one_click_blocker: "no_key" | "not_package" | null;
  installing: boolean;
  last_result: { ok: boolean; message: string; at: number } | null;
}

/** Notice on the home page when a newer version is published. */
export function UpdateBanner() {
  const [u, setU] = useState<UpdateInfo | null>(null);
  useEffect(() => {
    api.get<UpdateInfo>("/updates").then(setU).catch(() => {});
  }, []);
  if (!u?.newer) return null;
  return (
    <Callout>
      {tx("A new xBNC version is out: {latest} (you have {current}). {link}", {
        latest: <b>{u.latest}</b>,
        current: u.current,
        link: u.url ? (
          <a href={u.url} target="_blank" rel="noreferrer noopener">
            {t("See what's new")}
          </a>
        ) : (
          ""
        ),
      })}
    </Callout>
  );
}

/** Version and update check, in Settings. */
export function UpdatePanel() {
  const [u, setU] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [installing, setInstalling] = useState(false);
  useEffect(() => {
    api.get<UpdateInfo>("/updates").then(setU).catch(() => {});
  }, []);
  // While an update is installing, xBNC restarts: poll until it answers
  // with the new version or the helper reports a result.
  useEffect(() => {
    if (!u?.installing && !installing) return;
    const id = window.setInterval(() => {
      api
        .get<UpdateInfo>("/updates")
        .then((n) => {
          setU(n);
          if (!n.installing && (n.last_result || !n.newer)) setInstalling(false);
        })
        .catch(() => {});
    }, 3000);
    return () => window.clearInterval(id);
  }, [u?.installing, installing]);
  if (!u) return null;
  const install = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.post("/updates/install", {});
      setConfirming(false);
      setInstalling(true);
    } catch (e) {
      setError(errorText(e));
      setConfirming(false);
    } finally {
      setBusy(false);
    }
  };
  const check = async () => {
    setBusy(true);
    setError(null);
    try {
      setU(await api.post<UpdateInfo>("/updates/check", {}));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="panel stack-sm">
      <strong>{t("Updates")}</strong>
      {!u.configured ? (
        <p className="small muted">
          {tx("Checking for new versions is off. To turn it on, say where releases are published in {cfg} and restart.", {
            cfg: <code>{t("[updates] repository = \"owner/xbnc\"")}</code>,
          })}
        </p>
      ) : (
        <>
          {installing || u.installing ? (
            <Callout>{t("Installing version {latest}. xBNC will restart and this page will refresh by itself.", { latest: u.latest ?? "" })}</Callout>
          ) : u.newer && u.one_click ? (
            <Callout>
              <div className="stack-sm">
                <span>{tx("New version available: {latest}.", { latest: <b>{u.latest}</b> })}</span>
                <div>
                  <button className="btn btn-primary" onClick={() => setConfirming(true)} disabled={busy}>
                    {t("Update now")}
                  </button>
                </div>
              </div>
            </Callout>
          ) : u.newer ? (
            <Callout>
              {tx("New version available: {latest}. To update, run {cmd} on the server.", {
                latest: <b>{u.latest}</b>,
                cmd: <code>{u.one_click_blocker === "not_package" ? "sudo ./scripts/update.sh" : "sudo apt install ./xbnc_…deb"}</code>,
              })}
              {u.one_click_blocker === "no_key" && (
                <p className="small muted" style={{ marginTop: 8 }}>
                  {tx("To update from the panel, add the release public key in {cfg}.", { cfg: <code>[updates] public_key</code> })}
                </p>
              )}
            </Callout>
          ) : (
            u.latest && <p className="small">{t("You have the latest published version ({version}).", { version: u.current })}</p>
          )}
          {u.notes && u.newer && <pre className="update-notes">{u.notes}</pre>}
          {u.last_result && !installing && !u.installing && (
            <Callout kind={u.last_result.ok ? undefined : "bad"}>
              {u.last_result.ok
                ? t("Last update from the panel: successful ({date}).", { date: formatDate(u.last_result.at) })
                : t("The last update from the panel failed: {error}", { error: u.last_result.message })}
            </Callout>
          )}
          {u.error && <p className="small muted">{t("The last check failed: {error}", { error: u.error })}</p>}
          <div className="row" style={{ alignItems: "center" }}>
            <button className="btn" onClick={check} disabled={busy}>
              {busy ? t("Checking…") : t("Check now")}
            </button>
            {u.checked_at && <span className="small faint">{t("Last checked: {date}", { date: formatDate(u.checked_at) })}</span>}
          </div>
        </>
      )}
      {error && <Callout kind="bad">{error}</Callout>}
      {confirming && (
        <Confirm
          title={t("Update to {latest}", { latest: u.latest ?? "" })}
          confirmLabel={busy ? t("Downloading…") : t("Update now")}
          busy={busy}
          primary
          onClose={() => setConfirming(false)}
          onConfirm={install}
        >
          <p>{t("xBNC will download the package, check its signature and install it. The service will restart: IRC clients reconnect by themselves within a few seconds.")}</p>
          <p className="small muted">{t("Make a backup first if this release has major changes.")}</p>
        </Confirm>
      )}
    </div>
  );
}
