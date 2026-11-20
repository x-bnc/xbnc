// Types and texts shared by the plugin pages.
import { getLang, tk } from "./i18n";

export interface PluginSetting {
  key: string;
  type: "bool" | "int" | "text" | "lines";
  label: string;
  help: string;
  default: unknown;
  min?: number | null;
  max?: number | null;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  homepage: string;
  permissions: string[];
  settings: PluginSetting[];
  abi: number;
  http_domains?: string[];
  usage?: string;
  translations?: Record<string, { name?: string; description?: string; usage?: string; settings?: Record<string, { label?: string; help?: string }> }>;
}

/** The manifest's texts in the panel's language, when the author wrote them. */
export function localized(m: PluginManifest): { name: string; description: string; usage: string; label: (s: PluginSetting) => string; help: (s: PluginSetting) => string } {
  const tr = m.translations?.[getLang()];
  return {
    name: tr?.name || m.name,
    description: tr?.description || m.description,
    usage: tr?.usage || m.usage || "",
    label: (s) => tr?.settings?.[s.key]?.label || s.label || s.key,
    help: (s) => tr?.settings?.[s.key]?.help || s.help,
  };
}

export interface InstalledPlugin {
  id: string;
  manifest: PluginManifest;
  enabled: boolean;
  loaded: boolean;
  size: number;
  sha256: string;
  installed_at: number;
  installed_by: string;
  http_approved: string[];
}

export interface NetworkPlugin {
  id: string;
  manifest: PluginManifest;
  enabled: boolean;
  config: Record<string, string | number | boolean>;
}

/** What each permission lets a plugin do, in plain words. */
export const PERMISSION_TEXT: Record<string, { name: string; help: string }> = {
  send: {
    name: tk("Send messages"),
    help: tk("Can write in channels and to other people on your behalf."),
  },
  raw: {
    name: tk("IRC commands"),
    help: tk("Can send IRC commands (MODE, KICK, TOPIC…) on your behalf. Never QUIT, NICK or passwords."),
  },
  timer: {
    name: tk("Timer"),
    help: tk("Runs every minute even when nothing happens."),
  },
  presence: {
    name: tk("Who joins and leaves"),
    help: tk("Sees who joins, leaves, quits or changes nick in your channels, and who is in each one."),
  },
  http: {
    name: tk("Internet requests"),
    help: tk("Can fetch https pages, only from the domains you approve when installing it."),
  },
};
