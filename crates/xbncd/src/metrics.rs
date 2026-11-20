//! Process metrics for the dashboard (and, later, Prometheus).

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Monotonic counters updated on the hot path with relaxed atomics.
#[derive(Default)]
pub struct Metrics {
    pub bytes_from_servers: AtomicU64,
    pub bytes_to_servers: AtomicU64,
    pub bytes_from_clients: AtomicU64,
    pub bytes_to_clients: AtomicU64,
    pub messages_stored: AtomicU64,
    pub lines_from_servers: AtomicU64,
    pub reconnects: AtomicU64,
    pub client_logins: AtomicU64,
    pub failed_logins: AtomicU64,
    cpu: Mutex<Option<(u64, std::time::Instant)>>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Counters {
    pub bytes_from_servers: u64,
    pub bytes_to_servers: u64,
    pub bytes_from_clients: u64,
    pub bytes_to_clients: u64,
    pub messages_stored: u64,
    pub lines_from_servers: u64,
    pub reconnects: u64,
    pub client_logins: u64,
    pub failed_logins: u64,
}

impl Metrics {
    pub fn add(c: &AtomicU64, n: u64) {
        c.fetch_add(n, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Counters {
        let g = |c: &AtomicU64| c.load(Ordering::Relaxed);
        Counters {
            bytes_from_servers: g(&self.bytes_from_servers),
            bytes_to_servers: g(&self.bytes_to_servers),
            bytes_from_clients: g(&self.bytes_from_clients),
            bytes_to_clients: g(&self.bytes_to_clients),
            messages_stored: g(&self.messages_stored),
            lines_from_servers: g(&self.lines_from_servers),
            reconnects: g(&self.reconnects),
            client_logins: g(&self.client_logins),
            failed_logins: g(&self.failed_logins),
        }
    }

    /// CPU usage of this process in percent of one core since the last call
    /// (Linux only; `None` elsewhere or on the first call).
    pub fn cpu_percent(&self) -> Option<f64> {
        let ticks = proc_cpu_ticks()?;
        let now = std::time::Instant::now();
        let mut last = self.cpu.lock().ok()?;
        let result = last.and_then(|(t0, i0)| {
            let secs = now.duration_since(i0).as_secs_f64();
            (secs > 0.0).then(|| (ticks.saturating_sub(t0) as f64 / clock_ticks()) / secs * 100.0)
        });
        *last = Some((ticks, now));
        result
    }
}

fn clock_ticks() -> f64 {
    // USER_HZ is 100 on every mainstream Linux configuration.
    100.0
}

fn proc_cpu_ticks() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // Fields after the parenthesised command name; utime and stime are 14 and 15.
    let rest = &stat[stat.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    let utime: u64 = f.get(11)?.parse().ok()?;
    let stime: u64 = f.get(12)?.parse().ok()?;
    Some(utime + stime)
}

/// Resident memory of this process in bytes (Linux only).
pub fn rss_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_and_process_figures() {
        let m = Metrics::default();
        Metrics::add(&m.messages_stored, 3);
        assert_eq!(m.snapshot().messages_stored, 3);
        if cfg!(target_os = "linux") {
            assert!(rss_bytes().unwrap() > 0);
            assert!(m.cpu_percent().is_none(), "first sample has no baseline");
            assert!(m.cpu_percent().is_some());
        }
    }
}
