import { useEffect, useState, type FormEvent } from "react";
import { api, errorText, translate, type Me } from "../lib/api";
import { useSession } from "../lib/session";
import { colorFor } from "../lib/format";
import { Callout, Field } from "../components/ui";
import { useTheme } from "../components/Layout";
import { getLangs, pickLoginLang, t, tk, useLang } from "../lib/i18n";
import { KeyRound, LogIn } from "lucide-react";
import { VERSION } from "../version";
import { passkeysSupported, signInWithPasskey, wasCancelled } from "../lib/webauthn";

interface Methods {
  passkeys: boolean;
  oidc: { name: string } | null;
  ldap: boolean;
}

// A short, believable IRC scene for the welcome side. It plays once.
const SCENE: [string, string, string][] = [
  ["09:12", "lucia", tk("Who's working on this project?")],
  ["09:13", "tomas", tk("y2k is the creator, he's been working hard on it.")],
  ["09:14", "y2k", tk("Still a lot to do, but I'm getting there!")],
  ["09:15", "tomas", tk("And ARI3L has been helping with the translations too.")],
];

function Scene() {
  const [shown, setShown] = useState(0);
  useEffect(() => {
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
      setShown(SCENE.length);
      return;
    }
    if (shown >= SCENE.length) return;
    const timer = setTimeout(() => setShown((n) => n + 1), shown === 0 ? 400 : 900);
    return () => clearTimeout(timer);
  }, [shown]);
  return (
    <div className="auth-term" aria-hidden>
      <div className="auth-term-bar">
        <span />
        <span />
        <span />
        <em>#xbnc</em>
      </div>
      <div className="auth-log">
        {SCENE.slice(0, shown).map(([time, nick, text]) => (
          <div key={time + nick}>
            <span className="t">{time}</span>
            <b style={{ color: colorFor(nick) }}>{nick}</b> {t(text)}
          </div>
        ))}
      </div>
    </div>
  );
}

function AuthFrame({ children }: { children: React.ReactNode }) {
  useTheme();
  const lang = useLang();
  return (
    <div className="auth">
      <section className="auth-side">
        <div className="auth-brand">
          <img src="/logo-xbnc-dark.png" alt="xBNC" className="auth-logo" width="172" height="57" />
        </div>
        <h1>{t("Your IRC networks, connected even when you close your client.")}</h1>
        <Scene />
      </section>
      <section className="auth-main">
        <div className="auth-lang" role="group" aria-label={t("Language")}>
          {getLangs().map((l) => (
            <button key={l.code} type="button" lang={l.code} aria-pressed={lang === l.code} className={lang === l.code ? "on" : ""} onClick={() => pickLoginLang(l.code)}>
              {l.name}
            </button>
          ))}
        </div>
        <div className="auth-card">{children}</div>
        <p className="credit">
          xBNC v{VERSION} -{" "}
          <a href="https://xbnc.org" target="_blank" rel="noopener noreferrer">
            https://xbnc.org
          </a>
        </p>
      </section>
    </div>
  );
}

export function Login() {
  const { signedIn } = useSession();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [needCode, setNeedCode] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [methods, setMethods] = useState<Methods | null>(null);

  useEffect(() => {
    api.get<Methods>("/auth/methods").then(setMethods).catch(() => {});
    const q = new URLSearchParams(window.location.search);
    const sso = q.get("sso_error");
    if (sso) {
      setError(translate(sso));
      window.history.replaceState(null, "", window.location.pathname);
    }
  }, []);

  const passkey = async () => {
    setBusy(true);
    setError(null);
    try {
      signedIn(await signInWithPasskey());
    } catch (err) {
      if (!wasCancelled(err)) setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const me = await api.post<Me>("/auth/login", { username, password, totp_code: needCode ? code : undefined });
      signedIn(me);
    } catch (err) {
      const msg = errorText(err);
      if ((err as { code?: string }).code === "totp_required") {
        setNeedCode(true);
      } else {
        setError(msg);
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthFrame>
      <div>
        <h2>{needCode ? t("Two-step verification") : t("Sign in")}</h2>
        <p className="muted" style={{ marginTop: 6 }}>
          {needCode
            ? t("Open your authenticator app and enter the 6-digit code for xBNC.")
            : t("Use the same username and password as in your IRC client.")}
        </p>
      </div>
      {error && <Callout kind="bad">{error}</Callout>}
      <form className="stack" onSubmit={submit}>
        {!needCode ? (
          <>
            <Field label={t("User")} value={username} onChange={(e) => setUsername(e.target.value)} autoComplete="username" required autoFocus />
            <Field
              label={t("Password")}
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="current-password"
              required
            />
          </>
        ) : (
          <Field
            label={t("Verification code")}
            value={code}
            onChange={(e) => setCode(e.target.value.replace(/\D/g, "").slice(0, 6))}
            inputMode="numeric"
            autoComplete="one-time-code"
            mono
            required
            autoFocus
          />
        )}
        <button className="btn btn-primary" disabled={busy}>
          {busy ? t("Signing in…") : needCode ? t("Verify and sign in") : t("Sign in")}
        </button>
        {needCode && (
          <button type="button" className="btn btn-ghost" onClick={() => { setNeedCode(false); setCode(""); }}>{t("Back")}{" "}</button>
        )}
      </form>
      {!needCode && ((passkeysSupported && methods?.passkeys) || methods?.oidc) && (
        <div className="stack-sm">
          <div className="or-divider"><span>{t("or")}</span></div>
          {passkeysSupported && methods?.passkeys && (
            <button type="button" className="btn" onClick={passkey} disabled={busy}>
              <KeyRound size={16} aria-hidden /> {t("Sign in with a passkey")}
            </button>
          )}
          {methods?.oidc && (
            <a className="btn" href="/api/v1/auth/oidc/start">
              <LogIn size={16} aria-hidden /> {t("Sign in with {name}", { name: methods.oidc.name })}
            </a>
          )}
        </div>
      )}
      <p className="small faint">{t("Forgot your password? An administrator can reset it from Users.")}</p>
    </AuthFrame>
  );
}

export function Setup() {
  const { signedIn } = useSession();
  const [token, setToken] = useState("");
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mismatch = repeat.length > 0 && repeat !== password;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (password !== repeat) return;
    setBusy(true);
    setError(null);
    try {
      signedIn(await api.post<Me>("/setup", { token, username, password }));
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthFrame>
      <div>
        <h2>{t("Welcome to xBNC")}</h2>
        <p className="muted" style={{ marginTop: 6 }}>{t("Create the superadministrator account. The installer shows the setup code; to see it again, run this on the server:")}</p>
      </div>
      <div className="cmd">
        <code>sudo xbncd setup-code</code>
      </div>
      {error && <Callout kind="bad">{error}</Callout>}
      <form className="stack" onSubmit={submit}>
        <Field
          label={t("Setup code")}
          value={token}
          onChange={(e) => setToken(e.target.value.toUpperCase())}
          placeholder="XXXX-XXXX-XXXX"
          mono
          required
          autoFocus
          autoComplete="off"
        />
        <Field label={t("User")} value={username} onChange={(e) => setUsername(e.target.value)} autoComplete="username" required hint={t("Letters, digits, dot, hyphen and underscore.")} />
        <Field
          label={t("Password")}
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="new-password"
          minLength={10}
          required
          hint={t("At least 10 characters. You will also use it in your IRC client.")}
        />
        <Field
          label={t("Repeat the password")}
          type="password"
          value={repeat}
          onChange={(e) => setRepeat(e.target.value)}
          autoComplete="new-password"
          error={mismatch ? t("The passwords do not match.") : null}
          required
        />
        <button className="btn btn-primary" disabled={busy || mismatch}>
          {busy ? t("Creating…") : t("Create account and sign in")}
        </button>
      </form>
    </AuthFrame>
  );
}
