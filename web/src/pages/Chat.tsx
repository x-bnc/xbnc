import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { useSearchParams } from "react-router-dom";
import { Hash, MessageSquare, Send, Users as UsersIcon, ChevronLeft } from "lucide-react";
import { api, errorText, type HistoryMessage, type Network } from "../lib/api";
import { colorFor, formatDay, formatTime, stripFormatting } from "../lib/format";
import { fold, inputToLine, isChannel, mentions, parseIrc, type IrcMsg } from "../lib/irc";
import { useMe } from "../lib/session";
import { useToast } from "../lib/toast";
import { Empty, Loading } from "../components/ui";
import { t } from "../lib/i18n";

interface Line {
  key: string;
  id?: number;
  ts: number;
  nick: string;
  command: string;
  text: string;
  self: boolean;
  mention: boolean;
}

interface Conv {
  name: string;
  topic?: string;
  members: Map<string, string>; // folded nick -> nick with prefix symbol
  joined: boolean;
  lines: Line[];
  loaded: boolean;
  more: boolean;
  unread: number;
  highlight: number;
  marker: number;
}

function newConv(name: string): Conv {
  return { name, members: new Map(), joined: false, lines: [], loaded: false, more: false, unread: 0, highlight: 0, marker: 0 };
}

const eventCommands = new Set(["JOIN", "PART", "QUIT", "KICK", "NICK", "TOPIC", "MODE"]);

function eventText(l: Line): string {
  const why = (r: string) => (r ? ` (${r})` : "");
  switch (l.command) {
    case "JOIN":
      return t("{nick} joined", { nick: l.nick });
    case "PART":
      return t("{nick} left", { nick: l.nick }) + why(l.text);
    case "QUIT":
      return t("{nick} quit", { nick: l.nick }) + why(l.text);
    case "KICK": {
      const [victim, ...rest] = l.text.split(" ");
      return t("{nick} kicked {victim}", { nick: l.nick, victim }) + why(rest.join(" "));
    }
    case "NICK":
      return t("{nick} is now known as {new}", { nick: l.nick, new: l.text });
    case "TOPIC":
      return t("{nick} changed the topic to: {topic}", { nick: l.nick, topic: l.text });
    case "MODE":
      return t("{nick} set mode {mode}", { nick: l.nick, mode: l.text });
    default:
      return l.text;
  }
}

const URL_RE = /(https?:\/\/[^\s<>"']+)/g;

function Linkified({ text }: { text: string }) {
  const parts = text.split(URL_RE);
  return (
    <>
      {parts.map((p, i) =>
        i % 2 === 1 ? (
          <a key={i} href={p} target="_blank" rel="noopener noreferrer nofollow">
            {p}
          </a>
        ) : (
          p
        ),
      )}
    </>
  );
}

function ChatLine({ l }: { l: Line }) {
  if (eventCommands.has(l.command)) {
    return (
      <div className="log-line event">
        <time dateTime={new Date(l.ts).toISOString()}>{formatTime(l.ts)}</time>
        <span className="who">*</span>
        <span className="text">{eventText(l)}</span>
      </div>
    );
  }
  const raw = stripFormatting(l.text);
  const action = raw.startsWith("\x01ACTION ");
  const text = action ? raw.slice(8).replace(/\x01$/, "") : raw;
  const notice = l.command === "NOTICE";
  return (
    <div className={`log-line${action ? " action" : ""}${notice ? " notice" : ""}${l.mention ? " mention" : ""}`}>
      <time dateTime={new Date(l.ts).toISOString()}>{formatTime(l.ts)}</time>
      <span className="who" style={{ color: colorFor(l.nick) }}>
        {action ? `* ${l.nick}` : notice ? `-${l.nick}-` : l.nick}
      </span>
      <span className="text">
        <Linkified text={text} />
      </span>
    </div>
  );
}

export function ChatPage() {
  const me = useMe();
  const toast = useToast();
  const [params, setParams] = useSearchParams();
  const [nets, setNets] = useState<Network[] | null>(null);
  const nid = Number(params.get("network")) || nets?.find((n) => n.enabled)?.id || null;
  const wanted = params.get("target");

  useEffect(() => {
    api
      .get<Network[]>("/networks")
      .then((all) => setNets(all.filter((n) => n.user_id === me.user.id)))
      .catch((e) => toast.error(errorText(e)));
  }, [me.user.id]);

  if (!nets) return <Loading />;
  if (!nets.length) {
    return (
      <div className="page">
        <Empty title={t("You have no networks")}>{t("Add a network to chat from the panel.")}</Empty>
      </div>
    );
  }
  return (
    <ChatNetwork
      key={nid ?? 0}
      nets={nets}
      nid={nid}
      wanted={wanted}
      onPick={(id, target) => {
        const p = new URLSearchParams();
        p.set("network", String(id));
        if (target) p.set("target", target);
        setParams(p, { replace: true });
      }}
    />
  );
}

function ChatNetwork({
  nets,
  nid,
  wanted,
  onPick,
}: {
  nets: Network[];
  nid: number | null;
  wanted: string | null;
  onPick: (id: number, target?: string) => void;
}) {
  const toast = useToast();
  const [convs, setConvs] = useState<Map<string, Conv>>(new Map());
  const [current, setCurrent] = useState<string | null>(wanted ? fold(wanted) : null);
  const [status, setStatus] = useState<"connecting" | "open" | "closed">("connecting");
  const [nick, setNick] = useState("");
  const [input, setInput] = useState("");
  const [showList, setShowList] = useState(true);
  // The people column: open on wide screens; on a phone it replaces the chat.
  const [showMembers, setShowMembers] = useState(() => window.matchMedia("(min-width: 861px)").matches);
  const ws = useRef<WebSocket | null>(null);
  const linesEnd = useRef<HTMLDivElement | null>(null);
  const nickRef = useRef("");
  const currentRef = useRef<string | null>(current);
  currentRef.current = current;
  const history = useRef<string[]>([]);
  const histPos = useRef(-1);
  const markTimer = useRef<number | undefined>(undefined);
  const net = nets.find((n) => n.id === nid);

  const update = useCallback((fn: (m: Map<string, Conv>) => void) => {
    setConvs((prev) => {
      const next = new Map(prev);
      fn(next);
      return next;
    });
  }, []);

  const conv = (m: Map<string, Conv>, name: string): Conv => {
    const k = fold(name);
    let c = m.get(k);
    if (!c) {
      c = newConv(name);
      m.set(k, c);
    } else {
      c = { ...c, members: new Map(c.members), lines: [...c.lines] };
      m.set(k, c);
    }
    return c;
  };

  const addLine = (m: Map<string, Conv>, target: string, l: Line) => {
    const c = conv(m, target);
    if (l.id && c.lines.some((x) => x.id === l.id)) return;
    c.lines.push(l);
    if (c.lines.length > 2000) c.lines.splice(0, c.lines.length - 2000);
    const viewing = currentRef.current === fold(target) && document.visibilityState === "visible";
    if (!viewing && !l.self && !eventCommands.has(l.command) && l.ts > c.marker) {
      c.unread += 1;
      if (l.mention || !isChannel(target)) c.highlight += 1;
    }
  };

  const handle = useCallback((msg: IrcMsg) => {
    const ts = msg.tags.time ? Date.parse(msg.tags.time) : Date.now();
    const id = msg.tags.msgid?.startsWith("x") ? Number(msg.tags.msgid.slice(1)) : undefined;
    const myNick = nickRef.current;
    const self = fold(msg.nick) === fold(myNick);
    const p = msg.params;
    switch (msg.command) {
      case "001":
        nickRef.current = p[0];
        setNick(p[0]);
        break;
      case "NICK":
        if (self) {
          nickRef.current = p[0];
          setNick(p[0]);
        }
        update((m) => {
          for (const [k, c0] of m) {
            if (!c0.members.has(fold(msg.nick))) continue;
            const c = conv(m, c0.name);
            const sym = c.members.get(fold(msg.nick))!.replace(msg.nick, "");
            c.members.delete(fold(msg.nick));
            c.members.set(fold(p[0]), sym + p[0]);
            addLine(m, c.name, { key: `${ts}-${k}-nick`, ts, nick: msg.nick, command: "NICK", text: p[0], self, mention: false });
          }
        });
        break;
      case "JOIN":
        update((m) => {
          const c = conv(m, p[0]);
          if (self) {
            c.joined = true;
            c.members = new Map();
          } else {
            c.members.set(fold(msg.nick), msg.nick);
            addLine(m, p[0], { key: `${ts}-join-${msg.nick}`, ts, nick: msg.nick, command: "JOIN", text: "", self, mention: false });
          }
        });
        break;
      case "PART":
      case "KICK": {
        const who = msg.command === "KICK" ? p[1] : msg.nick;
        update((m) => {
          const c = conv(m, p[0]);
          if (fold(who) === fold(myNick)) c.joined = false;
          c.members.delete(fold(who));
          const text = msg.command === "KICK" ? `${p[1]} ${p[2] ?? ""}`.trim() : (p[1] ?? "");
          addLine(m, p[0], { key: `${ts}-${msg.command}-${who}`, ts, nick: msg.nick, command: msg.command, text, self, mention: false });
        });
        break;
      }
      case "QUIT":
        update((m) => {
          for (const [, c0] of m) {
            if (!c0.members.has(fold(msg.nick))) continue;
            const c = conv(m, c0.name);
            c.members.delete(fold(msg.nick));
            addLine(m, c.name, { key: `${ts}-quit-${msg.nick}`, ts, nick: msg.nick, command: "QUIT", text: p[0] ?? "", self, mention: false });
          }
        });
        break;
      case "TOPIC":
      case "332":
        update((m) => {
          const chan = msg.command === "TOPIC" ? p[0] : p[1];
          const topic = msg.command === "TOPIC" ? p[1] : p[2];
          const c = conv(m, chan);
          c.topic = topic;
          if (msg.command === "TOPIC") addLine(m, chan, { key: `${ts}-topic`, ts, nick: msg.nick, command: "TOPIC", text: topic, self, mention: false });
        });
        break;
      case "353":
        update((m) => {
          const c = conv(m, p[2]);
          for (const item of (p[3] ?? "").split(" ").filter(Boolean)) {
            const bare = item.replace(/^[~&@%+]+/, "").split("!")[0];
            c.members.set(fold(bare), item.split("!")[0]);
          }
        });
        break;
      case "MARKREAD": {
        const when = p[1]?.startsWith("timestamp=") ? Date.parse(p[1].slice(10)) : 0;
        update((m) => {
          const c = conv(m, p[0]);
          c.marker = Math.max(c.marker, when || 0);
          const left = c.lines.filter((l) => l.ts > c.marker && !l.self && !eventCommands.has(l.command));
          c.unread = c.loaded ? left.length : Math.min(c.unread, left.length);
          c.highlight = c.loaded ? left.filter((l) => l.mention || !isChannel(c.name)).length : Math.min(c.highlight, c.unread);
        });
        break;
      }
      case "PRIVMSG":
      case "NOTICE": {
        if (!msg.nick.includes(".") && msg.nick.startsWith("*")) {
          // *status and other services of the bouncer: show in their own tab.
        }
        const target = isChannel(p[0]) ? p[0] : self ? p[0] : msg.nick;
        if (!target || target === "*") break;
        const text = p[1] ?? "";
        update((m) =>
          addLine(m, target, {
            key: id ? `id-${id}` : `${ts}-${msg.nick}-${text.length}-${Math.random()}`,
            id,
            ts,
            nick: msg.nick,
            command: msg.command,
            text,
            self,
            mention: !self && isChannel(target) && mentions(text, myNick),
          }),
        );
        break;
      }
      default:
        if (/^\d{3}$/.test(msg.command) && Number(msg.command) >= 400 && msg.command !== "422") {
          toast.error(p.slice(1).join(" "));
        }
    }
  }, []);

  // The connection: the panel is one more client of the network.
  useEffect(() => {
    if (!nid) return;
    let closed = false;
    let retry = 1000;
    let timer: number | undefined;
    const open = () => {
      setStatus("connecting");
      const proto = location.protocol === "https:" ? "wss" : "ws";
      const s = new WebSocket(`${proto}://${location.host}/api/v1/networks/${nid}/chat`);
      ws.current = s;
      s.onopen = () => {
        setStatus("open");
        retry = 1000;
      };
      s.onmessage = (ev) => {
        const data = JSON.parse(ev.data);
        if (data.type === "line") {
          const m = parseIrc(data.line);
          if (m) handle(m);
        } else if (data.type === "error") {
          toast.error(data.text);
        }
      };
      s.onclose = () => {
        setStatus("closed");
        if (!closed) timer = window.setTimeout(open, (retry = Math.min(retry * 2, 30000)));
      };
    };
    open();
    // Private conversations stored in the history.
    api
      .get<{ target: string; last_ts: number }[]>(`/networks/${nid}/conversations`)
      .then((list) =>
        update((m) => {
          for (const c of list) {
            if (!c.target.startsWith("*") && !m.has(fold(c.target))) m.set(fold(c.target), newConv(c.target));
          }
        }),
      )
      .catch(() => {});
    return () => {
      closed = true;
      window.clearTimeout(timer);
      ws.current?.close();
    };
  }, [nid]);

  const send = (line: string) => {
    if (ws.current?.readyState === WebSocket.OPEN) {
      ws.current.send(JSON.stringify({ line }));
      return true;
    }
    toast.error(t("No connection to xBNC; reconnecting…"));
    return false;
  };

  // Load the history of the conversation being viewed.
  const cur = current ? convs.get(current) : undefined;
  useEffect(() => {
    if (!nid || !cur || cur.loaded) return;
    const p = new URLSearchParams({ target: cur.name, limit: "150" });
    api
      .get<{ messages: HistoryMessage[]; more: boolean }>(`/networks/${nid}/history?${p}`)
      .then((r) =>
        update((m) => {
          const c = conv(m, cur.name);
          const old: Line[] = r.messages
            .slice()
            .reverse()
            .map((h) => ({
              key: `id-${h.id}`,
              id: h.id,
              ts: h.ts,
              nick: h.nick,
              command: h.command,
              text: h.text,
              self: h.outgoing,
              mention: !h.outgoing && isChannel(h.target) && mentions(h.text, nickRef.current),
            }));
          const ids = new Set(old.map((l) => l.id));
          c.lines = [...old, ...c.lines.filter((l) => !l.id || !ids.has(l.id))];
          c.loaded = true;
          c.more = r.more;
        }),
      )
      .catch((e) => toast.error(errorText(e)));
  }, [nid, cur?.name, cur?.loaded]);

  const loadOlder = async () => {
    if (!nid || !cur) return;
    const first = cur.lines.find((l) => l.id);
    if (!first?.id) return;
    const p = new URLSearchParams({ target: cur.name, limit: "150", before_id: String(first.id) });
    try {
      const r = await api.get<{ messages: HistoryMessage[]; more: boolean }>(`/networks/${nid}/history?${p}`);
      update((m) => {
        const c = conv(m, cur.name);
        const older: Line[] = r.messages
          .slice()
          .reverse()
          .map((h) => ({ key: `id-${h.id}`, id: h.id, ts: h.ts, nick: h.nick, command: h.command, text: h.text, self: h.outgoing, mention: false }));
        c.lines = [...older, ...c.lines];
        c.more = r.more;
      });
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  // Viewing a conversation marks it read (for every device).
  useEffect(() => {
    if (!cur || !cur.lines.length) return;
    const last = cur.lines[cur.lines.length - 1].ts;
    if (cur.unread) update((m) => {
      const c = conv(m, cur.name);
      c.unread = 0;
      c.highlight = 0;
    });
    if (last <= cur.marker) return;
    window.clearTimeout(markTimer.current);
    markTimer.current = window.setTimeout(() => {
      send(`MARKREAD ${cur.name} timestamp=${new Date(last).toISOString()}`);
    }, 800);
  }, [cur?.name, cur?.lines.length]);

  useEffect(() => {
    linesEnd.current?.scrollIntoView({ block: "end" });
  }, [cur?.name, cur?.lines.length]);

  // Pick the wanted conversation, or the first channel, once known.
  useEffect(() => {
    if (current && convs.has(current)) return;
    const first = [...convs.values()].find((c) => c.joined) ?? [...convs.values()][0];
    if (first && !current) setCurrent(fold(first.name));
  }, [convs, current]);

  const list = useMemo(() => {
    const all = [...convs.values()];
    const chans = all.filter((c) => isChannel(c.name)).sort((a, b) => a.name.localeCompare(b.name));
    const privs = all
      .filter((c) => !isChannel(c.name))
      .sort((a, b) => (b.lines.at(-1)?.ts ?? 0) - (a.lines.at(-1)?.ts ?? 0));
    return { chans, privs };
  }, [convs]);

  const pick = (c: Conv) => {
    setCurrent(fold(c.name));
    setShowList(false);
    if (nid) onPick(nid, c.name);
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const r = inputToLine(input, cur?.name ?? null);
    if (!r) return;
    if ("error" in r) {
      toast.error(
        r.error === "unknown"
          ? t("Unknown command. Available: /me /msg /join /part /topic /nick /away /op /voice /kick /invite /whois")
          : r.error === "no-target"
            ? t("Choose a conversation first.")
            : t("The command is missing something."),
      );
      return;
    }
    if (send(r.line)) {
      history.current.unshift(input);
      history.current = history.current.slice(0, 50);
      histPos.current = -1;
      setInput("");
      // A message to someone new opens the conversation.
      const m = r.line.match(/^PRIVMSG (\S+) :/);
      if (m && fold(m[1]) !== current) {
        update((all) => void conv(all, m[1]));
        setCurrent(fold(m[1]));
      }
    }
  };

  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowUp" && history.current.length) {
      e.preventDefault();
      histPos.current = Math.min(histPos.current + 1, history.current.length - 1);
      setInput(history.current[histPos.current]);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      histPos.current = Math.max(histPos.current - 1, -1);
      setInput(histPos.current < 0 ? "" : history.current[histPos.current]);
    } else if (e.key === "Tab" && cur && isChannel(cur.name)) {
      // Nick completion.
      const words = input.split(" ");
      const lastWord = words.at(-1) ?? "";
      if (!lastWord) return;
      const match = [...cur.members.values()]
        .map((n) => n.replace(/^[~&@%+]+/, ""))
        .find((n) => fold(n).startsWith(fold(lastWord)));
      if (match) {
        e.preventDefault();
        words[words.length - 1] = words.length === 1 ? `${match}: ` : `${match} `;
        setInput(words.join(" "));
      }
    }
  };

  const members = cur && isChannel(cur.name) ? [...cur.members.values()].sort(memberOrder) : [];

  let lastDay = "";
  return (
    <div className="page chat-page">
      <header className="chat-head">
        <button className="icon-btn chat-back" onClick={() => setShowList(true)} aria-label={t("Conversations")}>
          <ChevronLeft size={20} />
        </button>
        <select
          className="input chat-net"
          value={nid ?? ""}
          onChange={(e) => onPick(Number(e.target.value))}
          aria-label={t("Network")}
        >
          {nets.map((n) => (
            <option key={n.id} value={n.id} disabled={!n.enabled}>
              {n.name}
            </option>
          ))}
        </select>
        <div className="chat-title">
          <strong>{cur?.name ?? t("Chat")}</strong>
          {cur?.topic && <span className="chat-topic" title={cur.topic}>{stripFormatting(cur.topic)}</span>}
        </div>
        <span className={`pill pill-${status === "open" ? "connected" : status === "connecting" ? "connecting" : "disconnected"}`}>
          <span className="dot" />
          {status === "open" ? nick || net?.nick : status === "connecting" ? t("Connecting") : t("Offline")}
        </span>
        {members.length > 0 && (
          <button className="icon-btn" onClick={() => setShowMembers((v) => !v)} aria-pressed={showMembers} aria-label={t("People in the channel")}>
            <UsersIcon size={18} />
            <span className="small">{members.length}</span>
          </button>
        )}
      </header>
      <div className={`chat${showList ? " list-open" : ""}${showMembers && members.length ? " members-open" : ""}`}>
        <aside className="chat-convs" aria-label={t("Conversations")}>
          {list.chans.length > 0 && <div className="chat-group">{t("Channels")}</div>}
          {list.chans.map((c) => (
            <ConvButton key={c.name} c={c} active={fold(c.name) === current} onClick={() => pick(c)} />
          ))}
          {list.privs.length > 0 && <div className="chat-group">{t("Private")}</div>}
          {list.privs.map((c) => (
            <ConvButton key={c.name} c={c} active={fold(c.name) === current} onClick={() => pick(c)} />
          ))}
          {!list.chans.length && !list.privs.length && <p className="small faint" style={{ padding: 10 }}>{t("Type /join #channel to join a channel.")}</p>}
        </aside>
        <section className="chat-main">
          <div className="log-lines chat-lines" role="log" aria-live="polite">
            {cur?.more && (
              <button className="btn btn-ghost chat-older" onClick={loadOlder}>
                {t("Load earlier messages")}
              </button>
            )}
            {cur?.lines.map((l) => {
              const day = formatDay(l.ts);
              const showDay = day !== lastDay;
              lastDay = day;
              return (
                <div key={l.key}>
                  {showDay && <div className="log-day">{day}</div>}
                  <ChatLine l={l} />
                </div>
              );
            })}
            {cur && !cur.lines.length && cur.loaded && <p className="small faint" style={{ padding: 12 }}>{t("No messages here yet.")}</p>}
            <div ref={linesEnd} />
          </div>
          <form className="chat-input" onSubmit={submit}>
            <input
              className="input"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={onKey}
              placeholder={cur ? t("Write in {target}…", { target: cur.name }) : t("Type /join #channel")}
              aria-label={t("Message")}
              maxLength={2000}
              autoComplete="off"
            />
            <button className="btn btn-primary" type="submit" disabled={status !== "open"} aria-label={t("Send")}>
              <Send size={17} />
            </button>
          </form>
        </section>
        {showMembers && members.length > 0 && (
          <aside className="chat-members" aria-label={t("People in the channel")}>
            {members.map((m) => {
              const bare = m.replace(/^[~&@%+]+/, "");
              return (
                <button
                  key={m}
                  className="chat-member"
                  onClick={() => {
                    update((all) => void conv(all, bare));
                    setCurrent(fold(bare));
                  }}
                  title={t("Write to {nick} privately", { nick: bare })}
                >
                  <span className="faint">{m.slice(0, m.length - bare.length)}</span>
                  <span style={{ color: colorFor(bare) }}>{bare}</span>
                </button>
              );
            })}
          </aside>
        )}
      </div>
    </div>
  );
}

function memberOrder(a: string, b: string): number {
  const rank = (s: string) => "~&@%+".indexOf(s[0]) + 1 || 9;
  return rank(a) - rank(b) || a.replace(/^[~&@%+]+/, "").localeCompare(b.replace(/^[~&@%+]+/, ""));
}

function ConvButton({ c, active, onClick }: { c: Conv; active: boolean; onClick: () => void }) {
  return (
    <button className="conv" aria-pressed={active} onClick={onClick}>
      <span className="row" style={{ gap: 6, minWidth: 0 }}>
        {isChannel(c.name) ? <Hash size={14} aria-hidden /> : <MessageSquare size={14} aria-hidden />}
        <span className="chat-conv-name" style={{ opacity: isChannel(c.name) && !c.joined ? 0.55 : 1 }}>
          {isChannel(c.name) ? c.name.replace(/^#/, "") : c.name}
        </span>
      </span>
      {c.unread > 0 && (
        <span className={`chat-badge${c.highlight ? " hot" : ""}`} aria-label={t("{n} unread", { n: c.unread })}>
          {c.unread > 99 ? "99+" : c.unread}
        </span>
      )}
    </button>
  );
}
