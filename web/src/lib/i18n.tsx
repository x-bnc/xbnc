// Panel translations.
//
// Texts are written in English in the source and wrapped in t("..."). Each
// language is a file on the server (locales/<code>.json, see
// TRANSLATING.md) whose "panel" section maps the English text to its
// translation; the panel downloads the list of languages and the texts of
// the one in use from /api/v1/i18n. A missing text is shown in English, so
// an incomplete translation never breaks anything. Placeholders use {name}:
// t("Hello {name}", { name }). A key may carry a context after "|"
// ("Disabled|network state") for languages that need it; English shows the
// part before the "|".
import { Fragment, useSyncExternalStore, type ReactNode } from "react";

export type Lang = string;

export interface LangInfo {
  code: string;
  name: string;
  english_name?: string;
  locale?: string;
  translators?: string[];
  complete?: number;
}

/** Shown until the server answers (and if it cannot be reached). */
let langs: LangInfo[] = [
  { code: "en", name: "English", locale: "en-GB" },
  { code: "es", name: "Español", locale: "es-ES" },
];
const catalogs = new Map<string, Record<string, string>>();
const KEY = "xbnc-lang";

function parse(code: string | null | undefined): Lang | null {
  const c = (code ?? "").toLowerCase().replace(/_/g, "-");
  if (!c) return null;
  if (langs.some((l) => l.code === c)) return c;
  const base = c.split("-")[0];
  return langs.some((l) => l.code === base) ? base : null;
}

function saved(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

let browserMatched = false;

function initial(): Lang {
  const s = parse(saved());
  if (s) return s;
  for (const l of navigator.languages ?? [navigator.language]) {
    const p = parse(l);
    if (p) {
      browserMatched = true;
      return p;
    }
  }
  return "en";
}

let current: Lang = initial();
document.documentElement.lang = current;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((f) => f());

async function loadCatalog(code: Lang): Promise<boolean> {
  if (code === "en" || catalogs.has(code)) return true;
  try {
    const r = await fetch(`/api/v1/i18n/${encodeURIComponent(code)}`, { credentials: "same-origin" });
    if (!r.ok) return false;
    catalogs.set(code, (await r.json()) as Record<string, string>);
    return true;
  } catch {
    return false;
  }
}

/**
 * Loads the list of languages and the texts of the current one. Called once
 * before the first render.
 */
export async function initI18n(): Promise<void> {
  try {
    const r = await fetch("/api/v1/i18n", { credentials: "same-origin" });
    if (r.ok) {
      const d = (await r.json()) as { languages: LangInfo[] };
      if (Array.isArray(d.languages) && d.languages.length) langs = d.languages;
    }
  } catch {
    /* keep the built-in list */
  }
  browserMatched = false;
  const want = initial();
  if (await loadCatalog(want)) current = want;
  else current = "en";
  document.documentElement.lang = current;
}

export function getLang(): Lang {
  return current;
}

export function getLangs(): LangInfo[] {
  return langs;
}

/** True once the person chose a language in this browser. */
export function hasChosenLang(): boolean {
  return parse(saved()) !== null;
}

export function setLang(code: string, remember = true) {
  const l = parse(code);
  if (!l) return;
  if (remember) {
    try {
      localStorage.setItem(KEY, l);
    } catch {
      /* ignore */
    }
  }
  if (l === current) return;
  void loadCatalog(l).then((ok) => {
    if (!ok) return;
    current = l;
    document.documentElement.lang = l;
    notify();
  });
}

/**
 * Language of a signed-in user: the one chosen in their account, otherwise
 * the server's default (the same rule xBNC uses for IRC messages).
 */
export function applyAccountLanguage(userLang: string | null | undefined, serverDefault: string | undefined) {
  const l = parse(userLang) ?? parse(serverDefault);
  if (l) setLang(l);
}

/**
 * Before signing in: what this browser chose, else its languages, else the
 * server's default.
 */
export function applyLoginLanguage(serverDefault: string | undefined) {
  if (!hasChosenLang() && !browserMatched && serverDefault) {
    setLang(serverDefault, false);
  }
}

// A language picked on the sign-in page is saved to the account at sign-in,
// so the last choice wins.
let loginPick: Lang | null = null;

export function pickLoginLang(code: string) {
  loginPick = parse(code);
  setLang(code);
}

export function takeLoginPick(): Lang | null {
  const l = loginPick;
  loginPick = null;
  return l;
}

export function useLang(): Lang {
  return useSyncExternalStore(
    (f) => {
      listeners.add(f);
      return () => listeners.delete(f);
    },
    () => current,
  );
}

export function t(text: string, params?: Record<string, string | number>): string {
  const tr = catalogs.get(current)?.[text] || text.split("|")[0];
  if (!params) return tr;
  return tr.replace(/\{(\w+)\}/g, (m, k: string) => (k in params ? String(params[k]) : m));
}

/** BCP-47 locale for dates and numbers. */
export function locale(): string {
  return langs.find((l) => l.code === current)?.locale || (current === "en" ? "en-GB" : current);
}

/** Marks a module-level text for translation; translate it with t() where shown. */
export const tk = (text: string) => text;

/**
 * Like t(), for sentences with embedded elements: placeholders are replaced
 * by React nodes. tx("Use port {port}.", { port: <code>6697</code> })
 */
export function tx(text: string, parts: Record<string, ReactNode>): ReactNode {
  const tr = t(text);
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of tr.matchAll(/\{(\w+)\}/g)) {
    out.push(tr.slice(last, m.index));
    out.push(<Fragment key={`${m[1]}-${m.index}`}>{m[1] in parts ? parts[m[1]] : m[0]}</Fragment>);
    last = (m.index ?? 0) + m[0].length;
  }
  out.push(tr.slice(last));
  return <>{out}</>;
}
