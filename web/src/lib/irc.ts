/** A parsed IRC line (enough for the panel chat). */
export interface IrcMsg {
  tags: Record<string, string>;
  prefix: string;
  nick: string;
  command: string;
  params: string[];
}

function unescapeTag(v: string): string {
  return v.replace(/\\(.)/g, (_, c: string) => ({ ":": ";", s: " ", "\\": "\\", r: "\r", n: "\n" })[c] ?? c);
}

export function parseIrc(line: string): IrcMsg | null {
  let rest = line;
  const tags: Record<string, string> = {};
  if (rest.startsWith("@")) {
    const sp = rest.indexOf(" ");
    if (sp < 0) return null;
    for (const kv of rest.slice(1, sp).split(";")) {
      const eq = kv.indexOf("=");
      if (eq < 0) tags[kv] = "";
      else tags[kv.slice(0, eq)] = unescapeTag(kv.slice(eq + 1));
    }
    rest = rest.slice(sp + 1);
  }
  let prefix = "";
  if (rest.startsWith(":")) {
    const sp = rest.indexOf(" ");
    if (sp < 0) return null;
    prefix = rest.slice(1, sp);
    rest = rest.slice(sp + 1);
  }
  const params: string[] = [];
  while (rest.length) {
    if (rest.startsWith(":")) {
      params.push(rest.slice(1));
      break;
    }
    const sp = rest.indexOf(" ");
    if (sp < 0) {
      params.push(rest);
      break;
    }
    if (sp > 0) params.push(rest.slice(0, sp));
    rest = rest.slice(sp + 1);
  }
  const command = (params.shift() ?? "").toUpperCase();
  if (!command) return null;
  const nick = prefix.split("!")[0];
  return { tags, prefix, nick, command, params };
}

/** RFC 1459 case folding (what most networks, Undernet included, use). */
export function fold(s: string): string {
  return s.toLowerCase().replace(/\[/g, "{").replace(/\]/g, "}").replace(/\\/g, "|").replace(/~/g, "^");
}

export const isChannel = (t: string) => /^[#&!+]/.test(t);

/** Does `text` mention `nick` as a word? */
export function mentions(text: string, nick: string): boolean {
  if (!nick) return false;
  const f = fold(text);
  const n = fold(nick);
  let i = f.indexOf(n);
  const word = (c: string | undefined) => c !== undefined && /[\w\-[\]\\`^{}|]/.test(c);
  while (i >= 0) {
    if (!word(f[i - 1]) && !word(f[i + n.length])) return true;
    i = f.indexOf(n, i + 1);
  }
  return false;
}

/**
 * Turns what the user types into an IRC line. Returns null for an empty
 * input and { error } for an unknown command.
 */
export function inputToLine(input: string, target: string | null): { line: string } | { error: string } | null {
  const text = input.replace(/[\r\n]+/g, " ");
  if (!text.trim()) return null;
  if (!text.startsWith("/") || text.startsWith("//")) {
    if (!target) return { error: "no-target" };
    return { line: `PRIVMSG ${target} :${text.startsWith("//") ? text.slice(1) : text}` };
  }
  const [cmd, ...args] = text.slice(1).split(" ");
  const rest = args.join(" ");
  switch (cmd.toLowerCase()) {
    case "me":
      return target ? { line: `PRIVMSG ${target} :\x01ACTION ${rest}\x01` } : { error: "no-target" };
    case "msg":
    case "query": {
      const [to, ...m] = args;
      return to && m.length ? { line: `PRIVMSG ${to} :${m.join(" ")}` } : { error: "usage" };
    }
    case "notice": {
      const [to, ...m] = args;
      return to && m.length ? { line: `NOTICE ${to} :${m.join(" ")}` } : { error: "usage" };
    }
    case "join":
    case "j":
      return args[0] ? { line: `JOIN ${args[0]}${args[1] ? " " + args[1] : ""}` } : { error: "usage" };
    case "part":
    case "leave": {
      const chan = args[0] && isChannel(args[0]) ? args[0] : target;
      const why = args[0] && isChannel(args[0]) ? args.slice(1).join(" ") : rest;
      return chan ? { line: `PART ${chan}${why ? " :" + why : ""}` } : { error: "usage" };
    }
    case "topic":
      return target ? { line: rest ? `TOPIC ${target} :${rest}` : `TOPIC ${target}` } : { error: "no-target" };
    case "nick":
      return args[0] ? { line: `NICK ${args[0]}` } : { error: "usage" };
    case "away":
      return { line: rest ? `AWAY :${rest}` : "AWAY" };
    case "back":
      return { line: "AWAY" };
    case "mode":
      return { line: `MODE ${rest || target}` };
    case "op":
    case "deop":
    case "voice":
    case "devoice": {
      if (!target || !args[0]) return { error: "usage" };
      const m = { op: "+o", deop: "-o", voice: "+v", devoice: "-v" }[cmd.toLowerCase()]!;
      return { line: `MODE ${target} ${m} ${args[0]}` };
    }
    case "kick": {
      if (!target || !args[0]) return { error: "usage" };
      return { line: `KICK ${target} ${args[0]}${args.length > 1 ? " :" + args.slice(1).join(" ") : ""}` };
    }
    case "invite":
      return args[0] ? { line: `INVITE ${args[0]} ${args[1] ?? target ?? ""}`.trim() } : { error: "usage" };
    case "whois":
      return args[0] ? { line: `WHOIS ${args[0]}` } : { error: "usage" };
    default:
      return { error: "unknown" };
  }
}
