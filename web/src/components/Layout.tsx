import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { NavLink, Link, useLocation } from "react-router-dom";
import { Home, Network as NetIcon, Users, ShieldCheck, SlidersHorizontal, UserCircle, LogOut, Moon, Sun, Menu, Plus, Puzzle, Blocks, MessagesSquare, AtSign } from "lucide-react";
import { useLive } from "../lib/live";
import { useMe, useSession } from "../lib/session";
import { colorFor, roleLabel } from "../lib/format";
import { Avatar } from "./ui";
import { t } from "../lib/i18n";
import { BRAND } from "../version";

type Live = ReturnType<typeof useLive>;
const LiveCtx = createContext<Live | null>(null);

export function useLiveData(): Live {
  const v = useContext(LiveCtx);
  if (!v) throw new Error("useLiveData outside Layout");
  return v;
}

type Theme = "light" | "dark";

function initialTheme(): Theme {
  try {
    const saved = localStorage.getItem("xbnc-theme");
    if (saved === "light" || saved === "dark") return saved;
  } catch {
    /* storage unavailable */
  }
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function useTheme(): [Theme, () => void] {
  const [theme, setTheme] = useState<Theme>(initialTheme);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  const toggle = () => {
    const next = theme === "dark" ? "light" : "dark";
    setTheme(next);
    try {
      localStorage.setItem("xbnc-theme", next);
    } catch {
      /* ignore */
    }
  };
  return [theme, toggle];
}

export function Layout({ children }: { children: ReactNode }) {
  const live = useLive();
  const me = useMe();
  const { logout } = useSession();
  const [theme, toggleTheme] = useTheme();
  const [open, setOpen] = useState(false);
  const loc = useLocation();
  useEffect(() => setOpen(false), [loc.pathname]);

  const p = me.permissions;
  const networks = live.frame?.networks ?? [];
  const mine = networks.filter((n) => n.owner === me.user.username);

  const nav = (
    <nav className={`sidebar${open ? " open" : ""}`} aria-label={t("Main navigation")}>
      <Link to="/" className="brand">
        <img src="/logo-xbnc-dark.png" alt="xBNC" className="brand-logo logo-on-light" width="132" height="43" />
        <img src="/logo-xbnc.png" alt="xBNC" className="brand-logo logo-on-dark" width="132" height="43" />
      </Link>
      <NavLink to="/" end className="nav-link">
        <Home size={19} />{" "}{t("Home")}{" "}</NavLink>
      <NavLink to="/chat" className="nav-link">
        <MessagesSquare size={19} /> {t("Chat")}
      </NavLink>
      <NavLink to="/mentions" className="nav-link">
        <AtSign size={19} /> {t("Mentions")}
      </NavLink>
      <NavLink to="/networks" className="nav-link">
        <NetIcon size={19} />{" "}{t("Networks")}{" "}</NavLink>
      <NavLink to="/modules" className="nav-link">
        <Puzzle size={19} /> {t("Modules")}
      </NavLink>
      <NavLink to="/plugins" className="nav-link">
        <Blocks size={19} /> {t("Plugins")}
      </NavLink>

      <div className="nav-group">{t("Your networks")}</div>
      <div className="rail">
        {mine.map((n) => (
          <NavLink key={n.id} to={`/networks/${n.id}`} className="rail-item" title={`${n.name}: ${n.state}`}>
            <span className="rail-bar" style={{ background: colorFor(n.name), opacity: n.state === "connected" ? 1 : 0.35 }} />
            <span className="rail-name">{n.name}</span>
            <span className={`pill pill-${n.state}`} style={{ padding: 3 }} aria-label={n.state}>
              <span className="dot" />
            </span>
          </NavLink>
        ))}
        {p.manage_own_networks && (
          <Link to="/networks/new" className="rail-item faint">
            <span />
            <span className="rail-name row" style={{ gap: 6 }}>
              <Plus size={15} />{" "}{t("Add network")}</span>
            <span />
          </Link>
        )}
      </div>

      {(p.view_users || p.view_audit || p.view_system) && <div className="nav-group">{t("Administration")}</div>}
      {p.view_users && (
        <NavLink to="/users" className="nav-link">
          <Users size={19} />{" "}{t("Users")}{" "}</NavLink>
      )}
      {p.view_audit && (
        <NavLink to="/security" className="nav-link">
          <ShieldCheck size={19} />{" "}{t("Security")}{" "}</NavLink>
      )}
      {p.view_system && (
        <NavLink to="/settings" className="nav-link">
          <SlidersHorizontal size={19} />{" "}{t("Settings")}{" "}</NavLink>
      )}

      <div className="sidebar-foot">
        <NavLink to="/profile" className="nav-link">
          <UserCircle size={19} />{" "}{t("Your account")}</NavLink>
        <div className="who">
          <Avatar name={me.user.username} src={me.user.avatar} />
          <div className="who-text">
            <div style={{ fontWeight: 700 }}>{me.user.username}</div>
            <div className="small faint" title={roleLabel(me.user.role)}>
              {roleLabel(me.user.role)}
            </div>
          </div>
          <button className="icon-btn" onClick={toggleTheme} aria-label={theme === "dark" ? t("Use light theme") : t("Use dark theme")}>
            {theme === "dark" ? <Sun size={18} /> : <Moon size={18} />}
          </button>
          <button className="icon-btn" onClick={logout} aria-label={t("Sign out")} title={t("Sign out")}>
            <LogOut size={18} />
          </button>
        </div>
        <p className="credit">{BRAND}</p>
      </div>
    </nav>
  );

  return (
    <LiveCtx.Provider value={live}>
      <div className="shell">
        {nav}
        <div style={{ minWidth: 0 }}>
          <div className="topbar-mobile">
            <button className="icon-btn" onClick={() => setOpen(true)} aria-label={t("Open menu")}>
              <Menu size={22} />
            </button>
            <span>
              <img src="/logo-xbnc-dark.png" alt="xBNC" className="brand-logo brand-logo-sm logo-on-light" width="98" height="32" />
              <img src="/logo-xbnc.png" alt="xBNC" className="brand-logo brand-logo-sm logo-on-dark" width="98" height="32" />
            </span>
            <Avatar name={me.user.username} size={30} src={me.user.avatar} />
          </div>
          {open && <div style={{ position: "fixed", inset: 0, zIndex: 40 }} onClick={() => setOpen(false)} />}
          <main className="main" id="contenido">
            {children}
          </main>
        </div>
      </div>
    </LiveCtx.Provider>
  );
}
