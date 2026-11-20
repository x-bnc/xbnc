// Thin client for the xBNC REST API (/api/v1).
import { t, tk } from "./i18n";
// Session: HttpOnly cookie set by the server. Mutations send the CSRF token.

export type Role = "superadmin" | "admin" | "moderator" | "user" | "limited";

export interface User {
  id: number;
  username: string;
  role: Role;
  enabled: boolean;
  max_networks: number | null;
  max_clients: number | null;
  max_history_mb: number | null;
  totp_enabled: boolean;
  last_login_at: number | null;
  created_at: number;
  networks: number;
  connected_clients: number;
  /** Chosen language; empty = server default. */
  language: string;
  last_seen_at: number | null;
  /** Address of the profile photo, or null. */
  avatar?: string | null;
}

export interface Permissions {
  edit_settings: boolean;
  view_system: boolean;
  view_users: boolean;
  manage_users: boolean;
  suspend_users: boolean;
  view_audit: boolean;
  manage_security: boolean;
  manage_own_networks: boolean;
  manage_all_networks: boolean;
}

export interface IrcListener {
  address: string;
  port: string;
  tls: boolean;
  lan_only: boolean;
  /** Names the certificate is valid for without "accept invalid certificates". */
  trusted_names?: string[];
}

export interface Me {
  user: User;
  csrf: string;
  permissions: Permissions;
  irc: IrcListener[];
  /** How IRC clients reach this bouncer (see ConnectHelp). */
  irc_connect?: {
    /** The domain or IP address given at installation (hostname in xbnc.toml). */
    public_host: string | null;
    /** This machine's address on the local network, when different. */
    local_host: string | null;
    listeners: IrcListener[];
  };
  hostname: string;
  version: string;
  /** Effective language of this user. */
  language: string;
  /** IANA time zone of this user ("" = UTC). */
  timezone?: string;
  languages: { code: string; name: string }[];
}

export type NetState = "connected" | "connecting" | "reconnecting" | "disconnected" | "disabled";

export interface ChannelStatus {
  name: string;
  topic: string | null;
  members: number;
  ops: number;
}

export interface ClientStatus {
  id: number;
  name: string;
  peer: string;
  connected_at: number;
  caps: string[];
}

export interface NetStatus {
  state: NetState;
  nick: string;
  server: string;
  tls: boolean;
  connected_since: number | null;
  last_error: string | null;
  caps: string[];
  network_name: string | null;
  channels: ChannelStatus[];
  clients: ClientStatus[];
}

export interface Network {
  id: number;
  user_id: number;
  owner: string;
  name: string;
  host: string;
  port: number;
  tls: boolean;
  nick: string;
  username: string;
  realname: string;
  has_server_password: boolean;
  sasl_username: string | null;
  has_sasl_password: boolean;
  enabled: boolean;
  status: NetStatus | null;
}

export interface Channel {
  name: string;
  saved: boolean;
  has_key: boolean;
  joined: boolean;
  topic: string | null;
  members: number | null;
  ops: number | null;
  /** null: server setting; -1: not stored; 0: forever; N: days. */
  retention_days?: number | null;
  /** Saved but not joined (`*status disablechan`). */
  disabled?: boolean;
  /** Join order. */
  position?: number;
}

export interface Preset {
  name: string;
  label: string;
  host: string;
  port: number;
  tls: boolean;
  sasl: boolean;
  description: string;
  /** Services bot to log in to after connecting, for networks without SASL. */
  service?: { bot: string; template?: string; server_password?: string };
}

export interface HistoryMessage {
  id: number;
  target: string;
  ts: number;
  nick: string;
  sender: string;
  command: string;
  text: string;
  outgoing: boolean;
}

export interface Settings {
  history: {
    retention_days: number;
    playback_max: number;
    new_client_playback: number;
    chathistory_max: number;
  };
  limits: {
    max_connections: number;
    max_clients_per_user: number;
    max_networks_per_user: number;
    login_max_failures: number;
    login_window_seconds: number;
    login_block_seconds: number;
    registration_timeout_seconds: number;
    client_queue: number;
  };
  general: { language: string };
  backups: { enabled: boolean; interval_hours: number; keep: number };
  alerts: {
    network_down_minutes: number;
    failed_logins_10min: number;
    memory_mb: number;
    database_mb: number;
    notify_irc: boolean;
    webhook_url: string;
  };
}

export interface Alert {
  kind: "network_down" | "failed_logins" | "memory" | "database";
  subject: string;
  message: string;
  since: number;
}

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

let csrf = "";
export function setCsrf(token: string) {
  csrf = token;
}

// Server messages are written in English for logs and the CLI; the panel
// shows them in Spanish when it knows them.
// Server messages are in English; these turn the common ones into a
// clearer sentence, which is then translated like any other panel text.
const TRANSLATIONS: [RegExp, string][] = [
  [/^Wrong username or password$/, tk("Wrong username or password.")],
  [/^Too many failed login attempts from your address \((.+)\); blocked for (\d+) more minutes/, tk("Too many failed attempts from your address ({1}): it is blocked for {2} more minutes. An IRC client that keeps trying with a wrong password also counts: correct the server password in it or close it. An administrator can lift the block in Security → Blocked IPs.")],
  [/^Too many failed/, tk("Too many failed attempts from your address. Wait a few minutes and try again.")],
  [/^This account is disabled$/, tk("This account is suspended. Talk to an administrator.")],
  [/^Enter the code from your authenticator app$/, tk("Enter the code from your authenticator app.")],
  [/^The verification code is not correct$/, tk("The verification code is not correct.")],
  [/^Your session has expired/, tk("Your session has expired. Sign in again.")],
  [/^Please log in$/, tk("Sign in to continue.")],
  [/^You do not have permission/, tk("You do not have permission to do this.")],
  [/^Missing or invalid CSRF token$/, tk("The page was out of date. Reload it and try again.")],
  [/^password must have at least (\d+) characters$/, tk("The password must have at least {1} characters.")],
  [/^user (.+) already exists$/, tk("There is already a user called {1}.")],
  [/^network (.+) already exists for this user$/, tk("You already have a network called {1}.")],
  [/^a network named (.+) already exists for this user$/, tk("There is already a network called {1} for this user.")],
  [/^user already has the maximum of (\d+) networks$/, tk("This user already has the maximum of {1} networks.")],
  [/^username must be 1-32 characters/, tk("The username must have 1 to 32 characters: letters, numbers, dot, hyphen or underscore.")],
  [/^network name must be 1-32 characters/, tk("The short name of the network must have 1 to 32 characters: letters, numbers, dot, hyphen or underscore.")],
  [/^invalid host$/, tk("The server address is not valid.")],
  [/^invalid port$/, tk("The port is not valid.")],
  [/^invalid nick$/, tk("The nick is not valid: no spaces and none of the characters , * ? ! @.")],
  [/^invalid username$/, tk("The ident (username) is not valid.")],
  [/^SASL needs both an account name and a password$/, tk("SASL needs both the account and the password.")],
  [/^There must be at least one active superadmin$/, tk("There must be at least one active superadministrator.")],
  [/^You cannot change your own role or suspend yourself$/, tk("You cannot change your own role or suspend yourself.")],
  [/^You cannot delete your own account$/, tk("You cannot delete your own account.")],
  [/^Your current password is not correct$/, tk("Your current password is not correct.")],
  [/^Your password is not correct$/, tk("The password is not correct.")],
  [/^The code is not correct/, tk("The code is not correct. Check the time on your phone and try again.")],
  [/^The setup code is not correct$/, tk("The setup code is not correct.")],
  [/^This bouncer is already set up$/, tk("This bouncer is already set up.")],
  [/^Channel names start with #/, tk("Channel names start with # and have no spaces or commas.")],
  [/^This network is disabled/, tk("The network is disabled. Enable it first.")],
  [/^Too many requests/, tk("Too many requests in a row. Wait a moment.")],
  [/^Cross-origin request refused$/, tk("Request refused because it came from another site.")],
  [/^Token names are 1 to 60 characters$/, tk("Token names must have 1 to 60 characters.")],
  [/^at most (\d+) tokens per user$/, tk("You can have at most {1} API tokens.")],
  [/^alerts\.webhook_url must be an http/, tk("The webhook must be an http:// or https:// address.")],
  [/^This action needs a panel session/, tk("This can only be done from the panel, not with a token.")],
  [/^this is not a WebAssembly file$/, tk("The file is not a WebAssembly plugin.")],
  [/^no xbnc\.manifest section/, tk("The file has no xBNC manifest: package it with “xbncd plugin pack”.")],
  [/^the plugin imports (.+), which xBNC does not provide/, tk("The plugin uses {1}, which xBNC does not provide (only xbnc.log and xbnc.now_ms).")],
  [/^the plugin does not export (.+)$/, tk("The plugin does not export: {1}.")],
  [/^the plugin failed to start/, tk("The plugin fails to start.")],
  [/^This plugin is turned off on this server$/, tk("This plugin is turned off on this server.")],
  [/^plugin id (.+) is reserved$/, tk("The identifier {1} is reserved.")],
  [/^plugin uses ABI version (\d+)/, tk("The plugin uses version {1} of the interface, which this xBNC does not support.")],
  [/^Update checks are not configured/, tk("Update checks are not configured: add [updates] repository to xbnc.toml.")],
  [/^An update is already being installed$/, tk("An update is already being installed.")],
  [/^Cannot update: the release has no signed package for this system$/, tk("Cannot update: the published release has no signed package for this system.")],
  [/^Cannot update: signature does not match|^Cannot update: .*signature/, tk("Cannot update: the package signature is not valid. Nothing was installed.")],
  [/^Cannot update: there is no newer version$/, tk("There is no newer version.")],
  [/^Cannot update: check for updates first$/, tk("Check for new versions first.")],
  [/^Cannot update: (.*)$/, tk("Cannot update: {1}")],
  // v0.8
  [/^You can only chat on your own running networks$/, tk("You can only chat on your own running networks.")],
  [/^E-mail is not configured on this server/i, tk("E-mail is not configured on this server ([smtp] section of xbnc.toml).")],
  [/^Invalid push subscription$/, tk("This browser gave invalid notification data. Try again.")],
  [/^too many devices with notifications \((\d+)\)$/, tk("There are too many devices with notifications (at most {1}). Remove one.")],
  [/^Telegram needs both the bot token and the chat id$/, tk("Telegram needs both the bot token and the chat id.")],
  [/^invalid Telegram bot token$/, tk("The Telegram bot token is not valid.")],
  [/^invalid Telegram chat id$/, tk("The Telegram chat id is not valid.")],
  [/^the Discord webhook must start with/, tk("The Discord webhook must start with https://discord.com/api/webhooks/.")],
  [/^web addresses must be valid https:\/\/ addresses$/, tk("Web addresses must be valid https:// addresses.")],
  [/^invalid e-mail address$/, tk("The e-mail address is not valid.")],
  [/^retention must be between -1 and 36500 days$/, tk("Retention must be between -1 and 36500 days.")],
  [/^the history quota must be between 1 and 1000000 MB$/, tk("The history space must be between 1 and 1000000 MB.")],
  [/^Cannot create the backup: (.*)$/, tk("Cannot create the backup: {1}")],
  [/^Cannot restore this backup: (.*)$/, tk("Cannot restore this backup: {1}")],
  [/^The file is too large$/, tk("The file is too large.")],
  [/^Upload interrupted/, tk("The upload was interrupted. Try again.")],
  [/the backup comes from a newer xBNC/, tk("The backup comes from a newer version of xBNC: update first.")],
  [/the archive has no database/, tk("The file does not contain the database (xbnc.db).")],
  [/the database in the archive is damaged/, tk("The database in the backup is damaged.")],
  [/the secret key in the archive is not valid/, tk("The encryption key in the backup is not valid.")],
  [/(truncated archive|unknown backup format|unexpected file in the archive|the archive contains something)/, tk("The file is not a valid xBNC backup.")],
  [/^The passkey request expired; try again$/, tk("The passkey request expired; try again.")],
  [/^The passkey could not be verified/, tk("The passkey could not be verified.")],
  [/^This passkey is not registered here$/, tk("This passkey is not registered here.")],
  [/^The passkey did not verify who you are/, tk("The passkey did not verify who you are (PIN, fingerprint or face).")],
  [/^this passkey is already registered$/, tk("This passkey is already registered.")],
  [/^too many passkeys \((\d+)\)$/, tk("You have too many passkeys (at most {1}).")],
  [/^Passkeys need the panel to be opened by name/, tk("Passkeys need the panel to be opened by its name (for example https://bnc.example.org or http://localhost), not by IP address.")],
  [/^Passkeys need a browser request/, tk("Passkeys only work from the browser.")],
  [/^This account is disabled$/, tk("This account is suspended.")],
  [/^Single sign-on needs web.public_url$/, tk("Single sign-on needs web.public_url in xbnc.toml.")],
  [/^Cannot reach the sign-in provider: (.*)$/, tk("Cannot reach the sign-in provider: {1}")],
  [/^The sign-in took too long or was already used; try again$/, tk("The sign-in took too long or was already used; try again.")],
  [/^The provider cancelled the sign-in: (.*)$/, tk("The provider cancelled the sign-in: {1}")],
  [/^The provider did not return a code$/, tk("The provider did not return a code.")],
  [/^No xBNC account is linked to this identity/, tk("No xBNC account is linked to this identity. Sign in with your password and link it from your profile, or ask an administrator.")],
  [/^A local user named (.+) already exists\./, tk("There is already a local user called {1}. Sign in with your password and link the account from your profile.")],
  [/^that account is already linked to another xBNC user$/, tk("That account is already linked to another xBNC user.")],
  [/^your account is not in a group allowed to use this bouncer$/, tk("Your account is not in any group allowed to use this bouncer.")],
  [/^the ID token/, tk("The provider returned an invalid identity.")],
  [/^the provider refused the sign-in: (.*)$/, tk("The provider refused the sign-in: {1}")],
  [/^The plugin did not ask for (.+); only the domains it asks for can be approved$/, tk("The plugin did not ask for {1}; only the domains it asks for can be approved.")],
  [/^Cannot read the plugin catalog: (.*)$/, tk("Cannot read the plugin catalog: {1}")],
  [/^Cannot download the plugin: (.*)$/, tk("Cannot download the plugin: {1}")],
  [/^The downloaded plugin is not the one in the catalog$/, tk("The downloaded plugin is not the one in the catalog.")],
  [/the catalog signature is not valid/, tk("The catalog signature is not valid.")],
  [/^the http permission needs 1 to 10 http_domains$/, tk("The http permission needs 1 to 10 domains (http_domains).")],
  [/^http_domains/, tk("The plugin domains (http_domains) are not valid.")],
  [/must be between|must be at most|must be greater than|must be at least/, tk("A value is out of the allowed range: ")],
];

export function translate(message: string): string {
  for (const [re, text] of TRANSLATIONS) {
    const m = message.match(re);
    if (m) {
      const params: Record<string, string> = {};
      m.slice(1).forEach((v, i) => (params[String(i + 1)] = v ?? ""));
      const tr = t(text, params);
      return text.endsWith(": ") ? tr + message : tr;
    }
  }
  return message;
}

type Listener = () => void;
const unauthorizedListeners = new Set<Listener>();
export function onUnauthorized(fn: Listener) {
  unauthorizedListeners.add(fn);
  return () => {
    unauthorizedListeners.delete(fn);
  };
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = { Accept: "application/json" };
  const raw = body instanceof Blob;
  if (raw) headers["Content-Type"] = (body as Blob).type;
  else if (body !== undefined) headers["Content-Type"] = "application/json";
  if (method !== "GET") headers["X-CSRF-Token"] = csrf;
  let resp: Response;
  try {
    resp = await fetch(`/api/v1${path}`, {
      method,
      headers,
      credentials: "same-origin",
      body: body === undefined ? undefined : raw ? (body as Blob) : JSON.stringify(body),
    });
  } catch {
    throw new ApiError(0, "network", t("Cannot reach xBNC. Check that the service is running."));
  }
  const text = await resp.text();
  let data: unknown = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = null;
  }
  if (!resp.ok) {
    const d = (data ?? {}) as { error?: string; message?: string };
    const code = d.error ?? "error";
    if (resp.status === 401 && (code === "session_expired" || code === "not_logged_in")) {
      unauthorizedListeners.forEach((l) => l());
    }
    throw new ApiError(resp.status, code, translate(d.message ?? `Error ${resp.status}`));
  }
  return data as T;
}

export const api = {
  get: <T>(path: string) => request<T>("GET", path),
  post: <T>(path: string, body: unknown = {}) => request<T>("POST", path, body),
  put: <T>(path: string, body: unknown) => request<T>("PUT", path, body),
  patch: <T>(path: string, body: unknown) => request<T>("PATCH", path, body),
  del: <T>(path: string, body?: unknown) => request<T>("DELETE", path, body),
};

export function errorText(e: unknown): string {
  if (e instanceof ApiError) return e.message;
  if (e instanceof Error) return e.message;
  return t("Something unexpected went wrong.");
}

/** Uploads a file as the raw request body (plugins, backups). */
export async function uploadRaw<T>(path: string, file: File, contentType: string): Promise<T> {
  const csrf = (await api.get<{ csrf: string }>("/auth/me")).csrf;
  const resp = await fetch(`/api/v1${path}`, {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": contentType, "X-CSRF-Token": csrf, Accept: "application/json" },
    body: file,
  });
  const data = await resp.json().catch(() => ({}));
  if (!resp.ok) throw new ApiError(resp.status, data.error ?? "error", translate(data.message ?? `Error ${resp.status}`));
  return data as T;
}
