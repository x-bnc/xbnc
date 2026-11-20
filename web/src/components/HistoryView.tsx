import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { Download, Search, Trash2 } from "lucide-react";
import { api, errorText, type HistoryMessage } from "../lib/api";
import { useToast } from "../lib/toast";
import { colorFor, formatDay, formatTime, stripFormatting, ago } from "../lib/format";
import { Confirm, Empty, Spinner } from "./ui";
import { locale, t } from "../lib/i18n";

interface Conversation {
  target: string;
  messages: number;
  last_ts: number;
}

function highlight(text: string, q: string): ReactNode {
  if (!q) return text;
  const lower = text.toLowerCase();
  const needle = q.toLowerCase();
  const out: ReactNode[] = [];
  let i = 0;
  let k = 0;
  for (;;) {
    const j = lower.indexOf(needle, i);
    if (j < 0) break;
    out.push(text.slice(i, j), <mark key={k++}>{text.slice(j, j + q.length)}</mark>);
    i = j + q.length;
  }
  out.push(text.slice(i));
  return out;
}

/** Readable text of a stored channel event (JOIN, PART, ...). */
function eventText(m: HistoryMessage): string | null {
  const why = (r: string) => (r ? ` (${r})` : "");
  switch (m.command) {
    case "JOIN":
      return t("{nick} joined", { nick: m.nick });
    case "PART":
      return t("{nick} left", { nick: m.nick }) + why(m.text);
    case "QUIT":
      return t("{nick} quit", { nick: m.nick }) + why(m.text);
    case "KICK": {
      const [victim, ...rest] = m.text.split(" ");
      return t("{nick} kicked {victim}", { nick: m.nick, victim }) + why(rest.join(" "));
    }
    case "NICK":
      return t("{nick} is now known as {new}", { nick: m.nick, new: m.text });
    case "TOPIC":
      return t("{nick} changed the topic to: {topic}", { nick: m.nick, topic: m.text });
    case "MODE":
      return t("{nick} set mode {mode}", { nick: m.nick, mode: m.text });
    default:
      return null;
  }
}

function Line({ m, q, showTarget }: { m: HistoryMessage; q: string; showTarget: boolean }) {
  const ev = eventText(m);
  if (ev) {
    return (
      <div className="log-line event">
        <time dateTime={new Date(m.ts).toISOString()}>{formatTime(m.ts)}</time>
        <span className="who">*</span>
        <span className="text">
          {showTarget && <span className="log-target">{m.target}</span>}
          {highlight(ev, q)}
        </span>
      </div>
    );
  }
  const raw = stripFormatting(m.text);
  const action = raw.startsWith("\x01ACTION ");
  const text = action ? raw.slice(8).replace(/\x01$/, "") : raw;
  const notice = m.command === "NOTICE";
  return (
    <div className={`log-line${action ? " action" : ""}${notice ? " notice" : ""}`}>
      <time dateTime={new Date(m.ts).toISOString()}>{formatTime(m.ts)}</time>
      <span className="who" style={{ color: colorFor(m.nick) }}>
        {action ? `* ${m.nick}` : notice ? `-${m.nick}-` : m.nick}
      </span>
      <span className="text">
        {showTarget && <span className="log-target">{m.target}</span>}
        {highlight(text, q)}
      </span>
    </div>
  );
}

export function HistoryView({ nid }: { nid: number }) {
  const toast = useToast();
  const [convs, setConvs] = useState<Conversation[] | null>(null);
  const [target, setTarget] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [q, setQ] = useState("");
  const [msgs, setMsgs] = useState<HistoryMessage[]>([]);
  const [more, setMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [clearing, setClearing] = useState(false);

  const loadConvs = useCallback(
    () =>
      api
        .get<Conversation[]>(`/networks/${nid}/conversations`)
        .then((c) => {
          setConvs(c);
          setTarget((cur) => cur ?? c[0]?.target ?? null);
        })
        .catch((e) => toast.error(errorText(e))),
    [nid],
  );
  useEffect(() => {
    loadConvs();
  }, [loadConvs]);

  // Debounce the search box.
  useEffect(() => {
    const timer = setTimeout(() => setQ(query.trim()), 300);
    return () => clearTimeout(timer);
  }, [query]);

  const fetchPage = useCallback(
    async (before?: number) => {
      const p = new URLSearchParams({ limit: "200" });
      // A search looks through every conversation of the network.
      if (q) p.set("q", q);
      else if (target) p.set("target", target);
      if (before) p.set("before_id", String(before));
      return api.get<{ messages: HistoryMessage[]; more: boolean }>(`/networks/${nid}/history?${p}`);
    },
    [nid, target, q],
  );

  useEffect(() => {
    if (!target && !q) return;
    let alive = true;
    setLoading(true);
    fetchPage()
      .then((r) => {
        if (!alive) return;
        setMsgs(r.messages);
        setMore(r.more);
      })
      .catch((e) => toast.error(errorText(e)))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [fetchPage, target]);

  const older = async () => {
    const last = msgs[msgs.length - 1];
    if (!last) return;
    try {
      const r = await fetchPage(last.id);
      setMsgs((v) => [...v, ...r.messages]);
      setMore(r.more);
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  // API returns newest first; show oldest first, grouped by day.
  const grouped = useMemo(() => {
    const out: { day: string; items: HistoryMessage[] }[] = [];
    for (const m of [...msgs].reverse()) {
      const day = formatDay(m.ts);
      const g = out[out.length - 1];
      if (g && g.day === day) g.items.push(m);
      else out.push({ day, items: [m] });
    }
    return out;
  }, [msgs]);

  const exportUrl = (format: "txt" | "json") =>
    `/api/v1/networks/${nid}/history/export?${new URLSearchParams({ ...(target ? { target } : {}), format })}`;

  const clear = async () => {
    try {
      const r = await api.del<{ deleted: number }>(`/networks/${nid}/history?${new URLSearchParams(target ? { target } : {})}`);
      toast.ok(t("{deleted} messages deleted.", { deleted: r.deleted }));
      setClearing(false);
      setTarget(null);
      setMsgs([]);
      loadConvs();
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  if (convs && convs.length === 0) {
    return <Empty title={t("No history yet")}>{t("Messages from your channels and private chats will show up here as soon as they arrive.")}</Empty>;
  }

  return (
    <div className="history">
      <div className="conv-list" role="list" aria-label={t("Conversations")}>
        {!convs ? (
          <Spinner />
        ) : (
          convs.map((c) => (
            <button
              key={c.target}
              role="listitem"
              className="conv"
              aria-pressed={!q && c.target === target}
              onClick={() => {
                setQuery("");
                setTarget(c.target);
              }}
            >
              <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: /^[#&]/.test(c.target) ? undefined : colorFor(c.target) }}>
                {c.target}
              </span>
              <small title={t("Last message {ago}", { ago: ago(c.last_ts) })}>{c.messages.toLocaleString(locale())}</small>
            </button>
          ))
        )}
      </div>
      <div className="log">
        <div className="log-tools">
          <div style={{ position: "relative", flex: 1, minWidth: 180 }}>
            <Search size={16} style={{ position: "absolute", left: 12, top: 13, color: "var(--ink-3)" }} aria-hidden />
            <input
              className="input"
              style={{ paddingLeft: 36 }}
              placeholder={t("Search all conversations")}
              aria-label={t("Search the history")}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          <a className="btn" href={exportUrl("txt")} download>
            <Download size={16} />{" "}{t("Text")}{" "}</a>
          <a className="btn" href={exportUrl("json")} download>
            <Download size={16} /> JSON
          </a>
          <button className="btn btn-danger" onClick={() => setClearing(true)} disabled={!target}>
            <Trash2 size={16} />{" "}{t("Delete")}{" "}</button>
        </div>
        <div className="log-lines" aria-live="polite">
          {loading ? (
            <div style={{ padding: 16 }}>
              <Spinner />
            </div>
          ) : msgs.length === 0 ? (
            <p style={{ padding: 16 }} className="muted">
              {q ? t("No message on this network contains “{q}”.", { q }) : t("No messages.")}
            </p>
          ) : (
            <>
              {more && (
                <div style={{ padding: "6px 16px" }}>
                  <button className="btn btn-sm" onClick={older}>{t("Load earlier messages")}</button>
                </div>
              )}
              {grouped.map((g) => (
                <div key={g.day}>
                  <div className="log-day">{g.day}</div>
                  {g.items.map((m) => (
                    <Line key={m.id} m={m} q={q} showTarget={!!q} />
                  ))}
                </div>
              ))}
            </>
          )}
        </div>
      </div>
      {clearing && target && (
        <Confirm title={t("Delete the history of {target}", { target })} confirmLabel={t("Delete messages")} onClose={() => setClearing(false)} onConfirm={clear}>
          <p>{t("All stored messages of {target} on this network will be deleted. Your devices will no longer be able to get them back.", { target: target ?? "" })}</p>
        </Confirm>
      )}
    </div>
  );
}
