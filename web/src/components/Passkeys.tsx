import { useEffect, useState } from "react";
import { Fingerprint, Link2, Trash2 } from "lucide-react";
import { api, errorText, translate } from "../lib/api";
import { useToast } from "../lib/toast";
import { formatDate } from "../lib/format";
import { passkeysSupported, registerPasskey, wasCancelled } from "../lib/webauthn";
import { Callout, Confirm, Field } from "./ui";
import { t } from "../lib/i18n";

interface Passkey {
  id: number;
  label: string;
  created_at: number;
  last_used_at: number | null;
}

interface Identity {
  provider: string;
  subject: string;
  created_at: number;
}

export function Passkeys() {
  const toast = useToast();
  const [keys, setKeys] = useState<Passkey[]>([]);
  const [ids, setIds] = useState<Identity[]>([]);
  const [oidc, setOidc] = useState<string | null>(null);
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [removing, setRemoving] = useState<Passkey | null>(null);

  const load = () => {
    api.get<Passkey[]>("/me/passkeys").then(setKeys).catch(() => {});
    api.get<Identity[]>("/me/identities").then(setIds).catch(() => {});
  };
  useEffect(() => {
    load();
    api.get<{ oidc: { name: string } | null }>("/auth/methods").then((m) => setOidc(m.oidc?.name ?? null)).catch(() => {});
    const q = new URLSearchParams(window.location.search);
    if (q.get("sso") === "linked") toast.ok(t("Account linked."));
    const err = q.get("sso_error");
    if (err) setError(translate(err));
    if (q.has("sso") || err) window.history.replaceState(null, "", window.location.pathname);
  }, []);

  const add = async () => {
    setBusy(true);
    setError(null);
    try {
      await registerPasskey(label.trim() || t("Passkey"));
      setLabel("");
      toast.ok(t("Passkey saved. You can now sign in with it."));
      load();
    } catch (e) {
      if (!wasCancelled(e)) setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!removing) return;
    setBusy(true);
    try {
      await api.del(`/me/passkeys/${removing.id}`);
      setRemoving(null);
      load();
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const link = async () => {
    try {
      const r = await api.post<{ url: string }>("/me/identities/oidc");
      window.location.href = r.url;
    } catch (e) {
      setError(errorText(e));
    }
  };

  const unlink = async (i: Identity) => {
    try {
      await api.del("/me/identities", { provider: i.provider, subject: i.subject });
      load();
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  return (
    <section className="panel stack-sm">
      <div className="panel-head">
        <div>
          <h2>
            <Fingerprint size={20} aria-hidden /> {t("Passkeys")}
          </h2>
          <p>{t("Sign in to the panel with your device's fingerprint, face or PIN, without typing your password. Your IRC clients keep using the password.")}</p>
        </div>
      </div>
      {error && <Callout kind="bad">{error}</Callout>}
      {keys.length > 0 && (
        <ul className="stack-sm" style={{ listStyle: "none", padding: 0 }}>
          {keys.map((k) => (
            <li key={k.id} className="row" style={{ justifyContent: "space-between" }}>
              <span>
                {k.label}{" "}
                <span className="small faint">
                  · {t("created {date}", { date: formatDate(k.created_at) })}
                  {k.last_used_at ? ` · ${t("used {date}", { date: formatDate(k.last_used_at) })}` : ""}
                </span>
              </span>
              <button className="icon-btn" onClick={() => setRemoving(k)} aria-label={t("Remove")}>
                <Trash2 size={16} />
              </button>
            </li>
          ))}
        </ul>
      )}
      {passkeysSupported ? (
        <div className="row" style={{ alignItems: "flex-end" }}>
          <div style={{ flex: 1, maxWidth: 320 }}>
            <Field label={t("Passkey name")} value={label} onChange={(e) => setLabel(e.target.value)} placeholder={t("For example: Phone")} maxLength={60} />
          </div>
          <button className="btn" onClick={add} disabled={busy}>
            {t("Add passkey")}
          </button>
        </div>
      ) : (
        <p className="small muted">{t("This browser does not support passkeys, or the panel is not opened over https or localhost.")}</p>
      )}

      {(oidc || ids.length > 0) && (
        <>
          <h3>
            <Link2 size={18} aria-hidden /> {t("Linked accounts")}
          </h3>
          {ids.length === 0 && <p className="small muted">{t("You have no outside account linked.")}</p>}
          <ul className="stack-sm" style={{ listStyle: "none", padding: 0 }}>
            {ids.map((i) => (
              <li key={i.provider + i.subject} className="row" style={{ justifyContent: "space-between" }}>
                <span>
                  {i.provider === "oidc" ? oidc ?? "SSO" : t("LDAP directory")}{" "}
                  <span className="small faint mono">{i.subject}</span>
                </span>
                <button className="icon-btn" onClick={() => unlink(i)} aria-label={t("Unlink")}>
                  <Trash2 size={16} />
                </button>
              </li>
            ))}
          </ul>
          {oidc && !ids.some((i) => i.provider === "oidc") && (
            <div className="row">
              <button className="btn" onClick={link}>
                {t("Link with {name}", { name: oidc })}
              </button>
            </div>
          )}
        </>
      )}
      {removing && (
        <Confirm title={t("Remove this passkey?")} confirmLabel={t("Remove")} onConfirm={remove} onClose={() => setRemoving(null)} busy={busy}>
          {t("You will no longer be able to sign in with “{name}”. Delete it from the device too if you will not use it.", { name: removing.label })}
        </Confirm>
      )}
    </section>
  );
}
