// Texts of the native modules. The server sends the catalog (ids, settings
// and limits); names, descriptions and labels live here so they can be
// translated with the rest of the panel.
import { t, tk } from "./i18n";

export interface SettingDef {
  key: string;
  type: "bool" | "int" | "text" | "lines" | "choice" | "secret" | "secret_lines";
  min?: number;
  max?: number;
  max_lines?: number;
  max_len?: number;
  options?: string[];
  default: string;
}

export interface ModuleDef {
  id: string;
  default_enabled: boolean;
  settings: SettingDef[];
}

export interface ModuleState {
  id: string;
  enabled: boolean;
  /** Secrets come back empty; `null` clears one when saving. */
  config: Record<string, string | number | boolean | null>;
  /** Secret settings that have a stored value. */
  secrets_set?: string[];
}

interface Text {
  name: string;
  summary: string;
  help?: string;
  settings?: Record<string, { label: string; help?: string; placeholder?: string; options?: Record<string, string> }>;
}

const TEXT: Record<string, Text> = {
  keepnick: {
    name: tk("Keep nick"),
    summary: tk("If your nick is taken when connecting, xBNC takes it back as soon as it is free."),
    help: tk("Checks every 30 seconds whether your nick is free and takes it back as soon as whoever holds it disconnects. If you change nick by hand from your client, it pauses until the next connection."),
  },
  away: {
    name: tk("Automatic away"),
    summary: tk("Marks you away when you close all your clients and, when you come back, sums up what you missed."),
    settings: {
      message: { label: tk("Away message"), help: tk("Empty: “Away” in your language."), placeholder: tk("Back soon") },
      delay: { label: tk("Wait before marking you away (seconds)") },
      summary: { label: tk("When I come back, tell me about private messages and mentions received") },
      nick: { label: tk("Nick while you are away"), help: tk("Empty: unchanged. {nick} is your nick, for example {nick}|away. It is cut to what the network allows (12 characters on Undernet)."), placeholder: tk("{nick}|away") },
    },
  },
  autoop: {
    name: tk("Automatic op and voice"),
    summary: tk("Gives op, halfop or voice to the people you choose when they join your channels (if you have op)."),
    settings: {
      delay: { label: tk("Wait before giving the mode (seconds)") },
      entries: {
        label: tk("People"),
        help: tk("One per line: mode (o, h or v), mask and, optionally, comma-separated channels (or *) and a key. With a key, xBNC sends a challenge and only gives the mode if the other person answers with the same key from their own xBNC."),
        placeholder: "o *!*@home.example.org #xbnc\nv friend!*@* * secret-key",
      },
    },
  },
  kickrejoin: {
    name: tk("Rejoin after a kick"),
    summary: tk("Rejoins the channel if you are kicked. Stops trying if you are kicked more than 3 times in a minute."),
    settings: { delay: { label: tk("Wait before rejoining (seconds)") } },
  },
  stickychan: {
    name: tk("Sticky channels"),
    summary: tk("Channels you cannot leave by mistake: xBNC blocks /part and rejoins if you leave."),
    settings: { channels: { label: tk("Channels"), help: tk("One per line."), placeholder: "#xbnc" } },
  },
  events: {
    name: tk("Events in the history"),
    summary: tk("Keeps joins, parts, kicks, nick changes, topics and modes next to the messages."),
    help: tk("Modern clients get them as real events; the rest as messages from *events when the history is played back."),
    settings: {
      joins: { label: tk("Keep joins, parts and quits"), help: tk("In big channels they make a lot of noise.") },
      other: { label: tk("Keep kicks, nick changes, topics and modes") },
    },
  },
  log: {
    name: tk("Log files"),
    summary: tk("Writes every conversation to daily text files, in the classic IRC log format."),
    help: tk("The files stay on the server, in the logs folder next to the database (for example /var/lib/xbnc/logs/user/network/#channel/2026-11-08.log). Times are in your time zone (Your account)."),
    settings: {
      retention_days: { label: tk("Delete files older than (days)"), help: tk("0 keeps them forever.") },
      events: { label: tk("Include events (joins, parts, kicks…)") },
    },
  },
  route_replies: {
    name: tk("Replies to whoever asked"),
    summary: tk("Replies to WHOIS, WHO, LIST, NAMES or ban lists go only to the device that asked, not to all of them."),
  },
  cert: {
    name: tk("Client certificate (CertFP)"),
    summary: tk("Identifies you on the network with a certificate instead of a password. Enables SASL EXTERNAL."),
    help: tk("Generate a certificate, add its fingerprint to your network account and reconnect. On Libera.Chat: connect identified and type /msg NickServ CERT ADD. Only works with TLS."),
    settings: { sasl_external: { label: tk("Use SASL EXTERNAL if the network offers it") } },
  },
  alias: {
    name: tk("Aliases"),
    summary: tk("Your own commands: you type /ghost bob and xBNC sends /msg NickServ GHOST bob."),
    settings: {
      aliases: {
        label: tk("Aliases"),
        help: tk("One per line: name and command. $1, $2… are the arguments, $1- all of them from the first, and $nick your nick."),
        placeholder: "ghost /msg NickServ GHOST $1\nop /mode $1 +o $nick",
      },
    },
  },
  watch: {
    name: tk("Watch words"),
    summary: tk("Copies messages containing what you care about to the *watch conversation, even if you were not connected."),
    settings: {
      mentions: { label: tk("Also tell me when someone mentions my nick") },
      patterns: {
        label: tk("Patterns"),
        help: tk("One per line. * matches any text: *deploy* finds “the deploy failed”. You can add a channel or nick after it to narrow it down."),
        placeholder: "*deploy*\n*xbnc* #xbnc",
      },
    },
  },
  block_motd: {
    name: tk("Hide the MOTD"),
    summary: tk("Does not show the server's message of the day to your clients."),
  },
  ctcp: {
    name: tk("CTCP replies"),
    summary: tk("Always answers VERSION (by default with the xBNC version and its web site), answers PING and TIME when no client is connected and stops CTCP floods."),
    settings: {
      flood_limit: { label: tk("CTCP requests allowed every 10 seconds") },
      version: { label: tk("VERSION reply"), help: tk("{version} is replaced by the xBNC version. Empty: the default text.") },
      block: { label: tk("CTCPs to ignore"), help: tk("One per line, for example TIME or FINGER."), placeholder: "FINGER" },
    },
  },
  dcc: {
    name: tk("DCC through xBNC"),
    summary: tk("DCC file transfers and chats go through xBNC: the other person does not see your IP and they work even behind a router."),
    help: tk("xBNC opens a port for each transfer and joins the two connections; nothing is stored on disk. On a server with a public IP it works with no setup; behind NAT, set the public IP in [dcc] public_ip in xbnc.toml and open the [dcc] ports in the firewall."),
    settings: {
      max_size_mb: { label: tk("Largest transfer (MB)"), help: tk("0 = no limit.") },
      wait: { label: tk("Seconds to accept a transfer") },
      private_addresses: { label: tk("Allow transfers from private addresses"), help: tk("When off, xBNC does not connect to internal networks (10.x, 192.168.x…) even if someone on IRC asks.") },
    },
  },
  services: {
    name: tk("Services login"),
    summary: tk("Logs you in to X (Undernet), NickServ, Q or AuthServ when connecting and hides your IP before joining channels."),
    help: tk("On Undernet it uses Login on Connect: the account goes in the server password and your real IP is never shown. If the network already has a server password, it logs in with /msg X login after connecting. Channels wait until the network confirms the login. With “Hide my IP”, no channel is ever joined before the network hides your IP, even if that takes longer than the wait (/msg *status joinnow joins them anyway). The password is stored encrypted and never shown again."),
    settings: {
      service: {
        label: tk("Service"),
        options: { auto: tk("Automatic, by network"), x: tk("X (Undernet)"), nickserv: tk("NickServ (Libera, Rizon, EvilNet…)"), q: tk("Q (QuakeNet)"), authserv: tk("AuthServ (GameSurge, AfterNet)") },
      },
      account: { label: tk("Account"), help: tk("Your account name on the service. With NickServ it can be empty if it is your nick.") },
      password: { label: tk("Password") },
      hide_host: { label: tk("Hide my IP (mode +x)"), help: tk("On X, Q and AuthServ. Other networks do it themselves.") },
      login_on_connect: { label: tk("Log in while connecting (Login on Connect)"), help: tk("Undernet only: the real IP is never shown.") },
      wait: { label: tk("Wait before joining channels (seconds)"), help: tk("Until the network confirms the login. 0: do not wait. With “Hide my IP” the channels always wait for the hidden IP.") },
      recover_nick: { label: tk("Get my nick back if it is taken"), help: tk("With NickServ: asks for the nick (REGAIN or GHOST) after logging in.") },
    },
  },
  invite: {
    name: tk("Invite and unban"),
    summary: tk("If you cannot join a channel (+i, +k, +l or a ban), asks X or ChanServ to invite or unban you and joins again."),
    help: tk("It tries at most three times every ten minutes per channel. You need access in the channel for the service to listen."),
    settings: {
      service: { label: tk("Channel service"), options: { auto: tk("Automatic, by network"), x: tk("X (Undernet)"), chanserv: tk("ChanServ"), q: tk("Q (QuakeNet)") } },
      unban: { label: tk("Ask to remove my ban") },
      join_invites: { label: tk("Join when invited to one of my channels") },
    },
  },
  charset: {
    name: tk("Character encoding"),
    summary: tk("Accents show correctly even when a channel uses Latin-1, and you can write in Latin-1 where needed."),
    help: tk("xBNC always understands Latin-1 messages that are not UTF-8. This module picks the exact table and the channels or private chats where your messages are sent in that encoding (for people with very old clients)."),
    settings: {
      legacy: { label: tk("Legacy encoding"), options: { "windows-1252": tk("Windows-1252 (the most common)"), "iso-8859-15": tk("ISO-8859-15 (with €)"), "iso-8859-1": tk("ISO-8859-1 (Latin-1)") } },
      send_legacy: { label: tk("Send in that encoding to"), help: tk("Channels or nicks, one per line. * for all. Empty: always UTF-8."), placeholder: "#xbnc" },
    },
  },
  detach: {
    name: tk("Detached channels"),
    summary: tk("Channels that keep recording everything but do not appear in your client. They come back if someone mentions you."),
    help: tk("From your client: /quote DETACH #channel to detach and /join #channel to see it again. The latest messages are shown when it comes back."),
    settings: {
      channels: { label: tk("Channels"), help: tk("One per line. * works (for example #*-bots)."), placeholder: "#xbnc" },
      on_mention: { label: tk("Show the channel if someone mentions my nick") },
      nicks: { label: tk("Show the channel if one of these nicks writes"), help: tk("One per line.") },
    },
  },
  flooddetach: {
    name: tk("Hide flooding channels"),
    summary: tk("If a channel suddenly fills with messages, it is hidden for a while so your client does not freeze. It keeps recording."),
    settings: {
      messages: { label: tk("Messages"), help: tk("How many messages…") },
      seconds: { label: tk("…in how many seconds") },
      calm: { label: tk("Quiet seconds before showing it again") },
    },
  },
  stripcontrols: {
    name: tk("Remove colours"),
    summary: tk("Removes colours, bold, underline and blinking from messages, also in the history."),
    settings: { channels: { label: tk("Only in these channels"), help: tk("One per line. Empty: in all of them.") } },
  },
  autocycle: {
    name: tk("Get op back when left alone"),
    summary: tk("If you are left alone in a channel without op, leaves and joins again to get it. Never in +i or keyed channels."),
    settings: { channels: { label: tk("Only in these channels"), help: tk("One per line. Empty: in all of them.") } },
  },
  crypt: {
    name: tk("Message encryption (FiSH)"),
    summary: tk("Encrypts your messages in channels and private chats with shared keys, compatible with FiSH for mIRC, HexChat and irssi."),
    help: tk("On IRC: /msg *crypt setkey #channel key, /msg *crypt keyx nick to agree a key in private and +p before a message to send it unencrypted. Keys are stored encrypted. FiSH protects from the network's servers; it is not a substitute for end-to-end encryption."),
    settings: {
      keys: { label: tk("Keys"), help: tk("One per line: channel or nick and key (cbc: first for CBC mode). They are stored encrypted: write all of them again to change them."), placeholder: "#xbnc cbc:a-long-key" },
      prefix: { label: tk("Mark for decrypted messages"), help: tk("For example [encrypted] . Empty: no mark."), placeholder: "[encrypted] " },
      key_exchange: { label: tk("Accept key exchanges (DH1080)"), help: tk("When someone proposes a key in private.") },
    },
  },
  ignore: {
    name: tk("Ignore"),
    summary: tk("Messages from people you ignore never reach any client or the history. On Undernet it uses SILENCE so they are not even sent."),
    help: tk("On IRC: /msg *ignore add nick, /msg *ignore del nick and /msg *ignore list."),
    settings: {
      masks: { label: tk("Ignore these"), help: tk("One per line: a nick or a nick!user@host mask (* works)."), placeholder: "pest\n*!*@spam.example" },
      silence: { label: tk("Use SILENCE when the network allows it") },
    },
  },
  schedule: {
    name: tk("Scheduled tasks"),
    summary: tk("Commands at a fixed time or every so often, in your time zone."),
    help: tk("Pick the time zone in Your account. Tasks only run while the network is connected."),
    settings: {
      tasks: {
        label: tk("Tasks"),
        help: tk("One per line: time and command (08:00 /away Working), days and time (mon-fri 18:00 /msg #channel bye) or interval (every 30m /msg #channel still here). Days: mon tue wed thu fri sat sun, weekdays, weekend."),
        placeholder: "mon-fri 08:00 /away Working\nmon-fri 15:00 /away\nevery 2h /msg #xbnc hello!",
      },
    },
  },
};

const BUILTIN: Record<string, { name: string; where: string }> = {
  perform: { name: tk("Commands on connect"), where: tk("In each network's configuration.") },
  chansaver: { name: tk("Remember channels"), where: tk("Always on: your /join and /part are remembered.") },
  sasl: { name: tk("SASL and NickServ"), where: tk("In each network's configuration.") },
  fail2ban: { name: tk("Brute-force blocking"), where: tk("In Settings and Security.") },
  notes: { name: tk("Notes"), where: tk("In Your account or with /msg *notes.") },
  lastseen: { name: tk("Last seen"), where: tk("In Users and with /msg *status seen.") },
  playback: { name: tk("Per-device history"), where: tk("Always on, with search on every network.") },
  webadmin: { name: tk("Web and IRC administration"), where: tk("This panel and /msg *status.") },
};

export const moduleName = (id: string) => (TEXT[id] ? t(TEXT[id].name) : id);
export const moduleSummary = (id: string) => (TEXT[id] ? t(TEXT[id].summary) : "");
export const moduleHelp = (id: string) => (TEXT[id]?.help ? t(TEXT[id].help!) : "");
export function settingText(id: string, key: string) {
  const s = TEXT[id]?.settings?.[key];
  return {
    label: s ? t(s.label) : key,
    help: s?.help ? t(s.help) : "",
    placeholder: s?.placeholder ? t(s.placeholder) : "",
  };
}
/** Label of one value of a choice setting. */
export function optionText(id: string, key: string, value: string) {
  const o = TEXT[id]?.settings?.[key]?.options?.[value];
  return o ? t(o) : value;
}
export const builtinName = (id: string) => (BUILTIN[id] ? t(BUILTIN[id].name) : id);
export const builtinWhere = (id: string) => (BUILTIN[id] ? t(BUILTIN[id].where) : "");
