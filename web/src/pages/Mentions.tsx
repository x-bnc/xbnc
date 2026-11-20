import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { AtSign } from "lucide-react";
import { api, errorText } from "../lib/api";
import { ago, colorFor, formatDate, stripFormatting } from "../lib/format";
import { Callout, Empty, Loading } from "../components/ui";
import { t } from "../lib/i18n";

interface Mention {
  id: number;
  network_id: number;
  network: string;
  target: string;
  ts: number;
  nick: string;
  text: string;
}

export function MentionsPage() {
  const [list, setList] = useState<Mention[] | null>(null);
  const [more, setMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = async (before?: number) => {
    setLoading(true);
    try {
      const q = before ? `?before=${before}` : "";
      const r = await api.get<{ mentions: Mention[]; more: boolean }>(`/mentions${q}`);
      setList((cur) => (before && cur ? [...cur, ...r.mentions] : r.mentions));
      setMore(r.more);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    load();
  }, []);

  if (error) return <div className="page"><Callout kind="bad">{error}</Callout></div>;
  if (!list) return <Loading />;
  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>{t("Mentions")}</h1>
          <p>{t("Every time someone wrote your nick in a channel, on all your networks. Select one to open the conversation.")}</p>
        </div>
      </header>
      {list.length === 0 ? (
        <Empty title={t("Nobody has mentioned you yet")}>{t("Mentions show up here even when you are not connected.")}</Empty>
      ) : (
        <div className="mention-list">
          {list.map((m) => (
            <Link
              key={m.id}
              className="mention-item"
              to={`/chat?network=${m.network_id}&target=${encodeURIComponent(m.target)}`}
            >
              <strong style={{ color: colorFor(m.nick) }}>
                <AtSign size={14} aria-hidden /> {m.nick}
              </strong>
              <span className="where" title={formatDate(m.ts)}>
                {m.target} · {m.network} · {ago(m.ts)}
              </span>
              <span className="text">{stripFormatting(m.text)}</span>
            </Link>
          ))}
          {more && (
            <button className="btn btn-ghost" disabled={loading} onClick={() => load(list[list.length - 1].ts)}>
              {t("Load more")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
