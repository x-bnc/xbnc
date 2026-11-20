import { useEffect, useRef, useState } from "react";
import { Terminal } from "lucide-react";
import { api, errorText } from "../lib/api";
import { useToast } from "../lib/toast";
import { Switch } from "./ui";
import { t } from "../lib/i18n";

interface LogLine {
  ts: number;
  plugin: string;
  network: string;
  level: "info" | "warn" | "error";
  text: string;
}

const MAX_LINES = 500;

/** Developer mode and the live plugin log (administrators). */
export function PluginDev({ onInstalled }: { onInstalled: () => void }) {
  const toast = useToast();
  const [dev, setDev] = useState<{ enabled: boolean; folder: string } | null>(null);
  const [lines, setLines] = useState<LogLine[]>([]);
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const box = useRef<HTMLDivElement>(null);
  const installed = useRef(onInstalled);
  installed.current = onInstalled;

  useEffect(() => {
    api.get<{ enabled: boolean; folder: string }>("/plugins/dev").then((d) => {
      setDev(d);
      if (d.enabled) setOpen(true);
    }).catch(() => {});
  }, []);

  useEffect(() => {
    if (!open) return;
    const proto = window.location.protocol === "https:" ? "wss" : "ws";
    let ws: WebSocket | null = null;
    let stop = false;
    let retry: number | undefined;
    const connect = () => {
      ws = new WebSocket(`${proto}://${window.location.host}/api/v1/plugins/logs`);
      ws.onopen = () => setLines([]);
      ws.onmessage = (e) => {
        const l = JSON.parse(e.data) as LogLine;
        setLines((prev) => [...prev.slice(-(MAX_LINES - 1)), l]);
        if (l.level === "info" && l.text.startsWith("installed ")) installed.current();
      };
      ws.onclose = () => {
        if (!stop) retry = window.setTimeout(connect, 3000);
      };
    };
    connect();
    return () => {
      stop = true;
      window.clearTimeout(retry);
      ws?.close();
    };
  }, [open]);

  useEffect(() => {
    const el = box.current;
    if (el && el.scrollHeight - el.scrollTop - el.clientHeight < 80) el.scrollTop = el.scrollHeight;
  }, [lines]);

  if (!dev) return null;

  const toggle = async (on: boolean) => {
    try {
      const d = await api.put<{ enabled: boolean; folder: string }>("/plugins/dev", { enabled: on });
      setDev(d);
      if (on) setOpen(true);
      toast.ok(on ? t("Developer mode on.") : t("Developer mode off."));
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  const shown = filter ? lines.filter((l) => l.plugin.includes(filter) || l.network.includes(filter) || l.text.includes(filter)) : lines;

  return (
    <section className="panel stack-sm">
      <div className="spread">
        <div>
          <h2 style={{ margin: 0 }}>
            <Terminal size={20} aria-hidden /> {t("Plugin development")}
          </h2>
          <p className="small muted" style={{ margin: "4px 0 0" }}>
            {t("With developer mode on, each .wasm you save in the folder is checked and installed at once, and every network reloads it. A .json with the same name next to it (the manifest) is packed in by itself. A new plugin is turned on in your networks.")}
          </p>
        </div>
        <Switch checked={dev.enabled} onChange={toggle} label={t("Developer mode")} />
      </div>
      {dev.enabled && (
        <div className="small stack-sm">
          <p style={{ margin: 0 }}>
            {t("Folder")}: <code style={{ overflowWrap: "anywhere" }}>{dev.folder}</code>
          </p>
          <p className="muted" style={{ margin: 0 }}>
            {t("Example on the server:")}{" "}
            <code style={{ overflowWrap: "anywhere" }}>{`sudo cp miplugin.wasm miplugin.json ${dev.folder}/`}</code>
          </p>
          <p className="muted" style={{ margin: 0 }}>{t("The C and Rust SDK, with examples, is in the sdk/ folder of the xBNC source code.")}</p>
        </div>
      )}
      <div className="row" style={{ justifyContent: "space-between" }}>
        <button className="btn btn-sm" onClick={() => setOpen(!open)}>
          {open ? t("Hide the log") : t("Show the live log")}
        </button>
        {open && <input className="input input-sm" placeholder={t("Filter")} value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("Filter")} />}
      </div>
      {open && (
        <div ref={box} className="plugin-log mono" role="log" aria-live="polite">
          {shown.length === 0 && <div className="faint">{t("No plugin messages yet.")}</div>}
          {shown.map((l, i) => (
            <div key={i} className={`log-${l.level}`}>
              <span className="faint">{new Date(l.ts).toLocaleTimeString()}</span> <strong>{l.plugin || "—"}</strong>
              {l.network && <span className="faint"> @{l.network}</span>} {l.text}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
