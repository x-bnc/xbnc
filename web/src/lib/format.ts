// Formatting helpers and the IRC colour system.
import { locale, t, tk } from "./i18n";

// The mIRC palette, toned for legibility on light and dark backgrounds.
// IRC clients colour nicks by hashing them into this palette; the panel
// does the same for nicks and networks so colours stay recognisable.
export const IRC_COLORS = [
  "#2E4BF5", // 12 blue
  "#D63DCE", // 13 pink
  "#1A9E5B", // 3 green
  "#E5484D", // 4 red
  "#F07800", // 7 orange
  "#0E9AA7", // 10 teal
  "#8E2DB8", // 6 purple
  "#B58A00", // 8 yellow (darkened)
  "#2BB673", // 9 light green
  "#1FA2C9", // 11 cyan
  "#B5512A", // 5 brown
  "#5B6BD6", // 2 navy (lightened)
];

export function colorFor(name: string): string {
  let h = 0;
  for (const c of name.toLowerCase()) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return IRC_COLORS[h % IRC_COLORS.length];
}

export function bytes(n: number | null | undefined): string {
  if (n == null) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

export function duration(seconds: number): string {
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (d > 0) return `${d} d ${h} h`;
  if (h > 0) return `${h} h ${m} min`;
  if (m > 0) return `${m} min`;
  return `${Math.max(0, Math.floor(seconds))} s`;
}

const fmt = (o: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat(locale(), o);

export const formatDate = (ms: number | null | undefined) =>
  ms ? fmt({ dateStyle: "medium", timeStyle: "short" }).format(new Date(ms)) : t("Never");
export const formatTime = (ms: number) => fmt({ hour: "2-digit", minute: "2-digit" }).format(new Date(ms));
export const formatDay = (ms: number) =>
  fmt({ weekday: "long", day: "numeric", month: "long", year: "numeric" }).format(new Date(ms));
export const formatNumber = (n: number) => n.toLocaleString(locale());

export function ago(ms: number | null | undefined): string {
  if (!ms) return t("never");
  const s = Math.round((Date.now() - ms) / 1000);
  if (s < 45) return t("just now");
  if (s < 3600) return t("{n} min ago", { n: Math.round(s / 60) });
  if (s < 86400) return t("{n} h ago", { n: Math.round(s / 3600) });
  return t("{n} days ago", { n: Math.round(s / 86400) });
}

const STATE_LABEL: Record<string, string> = {
  connected: tk("Connected|network state"),
  connecting: tk("Connecting"),
  reconnecting: tk("Reconnecting"),
  disconnected: tk("Disconnected"),
  disabled: tk("Disabled|network state"),
};

const ROLE_LABEL: Record<string, string> = {
  superadmin: tk("Superadministrator"),
  admin: tk("Administrator"),
  moderator: tk("Moderator"),
  user: tk("User"),
  limited: tk("Limited user"),
};

const ROLE_HELP: Record<string, string> = {
  superadmin: tk("Everything, including system settings and administrators."),
  admin: tk("Manages users, all networks and security."),
  moderator: tk("Sees users and the audit log, and can suspend users."),
  user: tk("Manages their own networks, channels and history."),
  limited: tk("Uses the networks an administrator sets up for them."),
};

export const stateLabel = (s: string) => (STATE_LABEL[s] ? t(STATE_LABEL[s]) : s);
export const roleLabel = (r: string) => (ROLE_LABEL[r] ? t(ROLE_LABEL[r]) : r);
export const roleHelp = (r: string) => (ROLE_HELP[r] ? t(ROLE_HELP[r]) : "");

/** Strips mIRC formatting codes from IRC text. */
export function stripFormatting(text: string): string {
  // eslint-disable-next-line no-control-regex
  return text.replace(/\x03(\d{1,2}(,\d{1,2})?)?|[\x02\x0F\x16\x1D\x1F\x1E\x11]/g, "");
}
