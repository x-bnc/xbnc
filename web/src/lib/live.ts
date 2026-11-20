import { useEffect, useRef, useState } from "react";
import type { NetState } from "./api";

export interface LiveNetwork {
  id: number;
  name: string;
  owner: string;
  state: NetState;
  nick: string | null;
  channels: number;
  clients: number;
  last_error: string | null;
}

export interface Counters {
  bytes_from_servers: number;
  bytes_to_servers: number;
  bytes_from_clients: number;
  bytes_to_clients: number;
  messages_stored: number;
  lines_from_servers: number;
  reconnects: number;
  client_logins: number;
  failed_logins: number;
}

export interface LiveSystem {
  rss_bytes: number | null;
  cpu_percent: number | null;
  irc_clients: number;
  connections: number;
  blocked_ips: number;
  counters: Counters;
}

export interface LiveFrame {
  ts: number;
  uptime_seconds: number;
  networks: LiveNetwork[];
  system?: LiveSystem;
}

/** Per-second rates derived from consecutive frames (for the sparklines). */
export interface Rates {
  ts: number;
  messages: number;
  traffic_in: number;
  traffic_out: number;
  rss: number;
  cpu: number;
}

const HISTORY = 60; // two minutes at one frame every 2 s

/**
 * Subscribes to the dashboard WebSocket. Reconnects with backoff and keeps
 * a short history of rates for charts.
 */
export function useLive() {
  const [frame, setFrame] = useState<LiveFrame | null>(null);
  const [rates, setRates] = useState<Rates[]>([]);
  const [connected, setConnected] = useState(false);
  const prev = useRef<LiveFrame | null>(null);

  useEffect(() => {
    let ws: WebSocket | null = null;
    let stopped = false;
    let retry = 1000;
    let timer: number | undefined;

    const open = () => {
      const proto = location.protocol === "https:" ? "wss" : "ws";
      ws = new WebSocket(`${proto}://${location.host}/api/v1/ws`);
      ws.onopen = () => {
        setConnected(true);
        retry = 1000;
      };
      ws.onmessage = (ev) => {
        const data = JSON.parse(ev.data);
        if (data.type === "logout") {
          location.reload();
          return;
        }
        if (data.type !== "stats") return;
        const f = data as LiveFrame;
        const p = prev.current;
        if (p?.system && f.system) {
          const dt = Math.max(0.5, (f.ts - p.ts) / 1000);
          const c = f.system.counters;
          const o = p.system.counters;
          const r: Rates = {
            ts: f.ts,
            messages: Math.max(0, (c.messages_stored - o.messages_stored) / dt),
            traffic_in: Math.max(0, (c.bytes_from_servers + c.bytes_from_clients - o.bytes_from_servers - o.bytes_from_clients) / dt),
            traffic_out: Math.max(0, (c.bytes_to_servers + c.bytes_to_clients - o.bytes_to_servers - o.bytes_to_clients) / dt),
            rss: f.system.rss_bytes ?? 0,
            cpu: f.system.cpu_percent ?? 0,
          };
          setRates((v) => [...v.slice(-(HISTORY - 1)), r]);
        }
        prev.current = f;
        setFrame(f);
      };
      ws.onclose = () => {
        setConnected(false);
        if (!stopped) {
          timer = window.setTimeout(open, retry);
          retry = Math.min(retry * 2, 15000);
        }
      };
    };
    open();
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
      ws?.close();
    };
  }, []);

  return { frame, rates, connected };
}
