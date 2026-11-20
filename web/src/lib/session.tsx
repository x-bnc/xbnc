import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { api, onUnauthorized, setCsrf, type Me } from "./api";
import { applyAccountLanguage, applyLoginLanguage, takeLoginPick } from "./i18n";

interface SessionCtx {
  me: Me | null;
  loading: boolean;
  setupNeeded: boolean;
  refresh: () => Promise<void>;
  signedIn: (me: Me) => void;
  logout: () => Promise<void>;
}

const Ctx = createContext<SessionCtx | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const [me, setMe] = useState<Me | null>(null);
  const [loading, setLoading] = useState(true);
  const [setupNeeded, setSetupNeeded] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const m = await api.get<Me>("/auth/me");
      setCsrf(m.csrf);
      applyAccountLanguage(m.user.language, m.language);
      setMe(m);
    } catch {
      setMe(null);
      api
        .get<{ language: string }>("/health")
        .then((h) => applyLoginLanguage(h.language))
        .catch(() => {});
      try {
        const s = await api.get<{ needed: boolean }>("/setup");
        setSetupNeeded(s.needed);
      } catch {
        setSetupNeeded(false);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
    return onUnauthorized(() => setMe(null));
  }, [refresh]);

  const signedIn = useCallback((signed: Me) => {
    let m = signed;
    setCsrf(m.csrf);
    const picked = takeLoginPick();
    if (picked && picked !== m.user.language) {
      m = { ...m, user: { ...m.user, language: picked } };
      api.put("/me/language", { language: picked }).catch(() => {});
    }
    applyAccountLanguage(m.user.language, m.language);
    setMe(m);
    setSetupNeeded(false);
  }, []);

  const logout = useCallback(async () => {
    try {
      await api.post("/auth/logout");
    } finally {
      setMe(null);
    }
  }, []);

  return (
    <Ctx.Provider value={{ me, loading, setupNeeded, refresh, signedIn, logout }}>{children}</Ctx.Provider>
  );
}

export function useSession(): SessionCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("useSession outside SessionProvider");
  return c;
}

/** Current user; only valid inside the authenticated shell. */
export function useMe(): Me {
  const { me } = useSession();
  if (!me) throw new Error("not signed in");
  return me;
}
