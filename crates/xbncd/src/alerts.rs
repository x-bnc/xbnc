//! Alerts: conditions an administrator wants to hear about (a network down
//! for too long, a burst of failed logins, memory or database too large).
//!
//! Rules come from the runtime settings and are evaluated every 30 seconds.
//! When an alert starts or ends it is written to the audit log, announced to
//! connected administrators through `*status`, and optionally POSTed as JSON
//! to a webhook (Slack, Discord and Mattermost incoming webhooks, or anything that takes
//! JSON). The panel shows the active alerts.

use crate::db;
use crate::network::NetCmd;
use crate::server::Ctx;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Alert rules (part of the runtime settings). 0 disables a rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AlertSettings {
    /// A network that should be connected has been down this many minutes.
    pub network_down_minutes: u32,
    /// Failed logins (IRC and panel) in the last 10 minutes.
    pub failed_logins_10min: u32,
    /// Resident memory of the process, in MB.
    pub memory_mb: u32,
    /// Size of the database file, in MB.
    pub database_mb: u32,
    /// Tell connected administrators through *status.
    pub notify_irc: bool,
    /// POST alerts as JSON to this http(s) URL; empty for none.
    pub webhook_url: String,
}

impl Default for AlertSettings {
    fn default() -> Self {
        AlertSettings {
            network_down_minutes: 15,
            failed_logins_10min: 30,
            memory_mb: 0,
            database_mb: 0,
            notify_irc: true,
            webhook_url: String::new(),
        }
    }
}

impl AlertSettings {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.network_down_minutes > 10_080 {
            anyhow::bail!("alerts.network_down_minutes must be at most 10080 (a week)");
        }
        if self.failed_logins_10min > 100_000
            || self.memory_mb > 1_000_000
            || self.database_mb > 10_000_000
        {
            anyhow::bail!("alert thresholds are out of range");
        }
        if !self.webhook_url.is_empty() {
            parse_url(&self.webhook_url).ok_or_else(|| {
                anyhow::anyhow!("alerts.webhook_url must be an http:// or https:// URL")
            })?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    /// network_down, failed_logins, memory, database
    pub kind: &'static str,
    /// What it is about: "user/network", an amount...
    pub subject: String,
    /// English description (the panel translates by kind).
    pub message: String,
    /// When it started (ms).
    pub since: i64,
}

#[derive(Default)]
pub struct Alerts {
    active: Mutex<Vec<Alert>>,
    down_since: Mutex<HashMap<i64, Instant>>,
}

impl Alerts {
    pub fn active(&self) -> Vec<Alert> {
        self.active.lock().expect("alerts lock").clone()
    }
}

fn key(a: &Alert) -> (String, String) {
    (a.kind.to_string(), a.subject.clone())
}

/// Evaluates the rules once and returns the alerts that should be active.
async fn evaluate(ctx: &Arc<Ctx>, rules: &AlertSettings) -> Vec<Alert> {
    let now = db::now_ms();
    let mut out = Vec::new();
    if rules.network_down_minutes > 0 {
        let users: HashMap<i64, String> = ctx
            .db
            .call(|c| {
                Ok(db::list_users(c)?
                    .into_iter()
                    .map(|u| (u.id, u.username))
                    .collect())
            })
            .await
            .unwrap_or_default();
        let limit = Duration::from_secs(rules.network_down_minutes as u64 * 60);
        let mut seen = Vec::new();
        for h in ctx.registry.handles() {
            seen.push(h.network_id);
            // Only a network that is trying to connect counts as down: one
            // the user disconnected on purpose does not.
            let up = !matches!(
                crate::server::network_status(ctx, h.network_id)
                    .await
                    .map(|s| s.state),
                Some("reconnecting" | "connecting")
            );
            let since = {
                let mut d = ctx.alerts.down_since.lock().expect("alerts lock");
                if up {
                    d.remove(&h.network_id);
                    None
                } else {
                    Some(*d.entry(h.network_id).or_insert_with(Instant::now))
                }
            };
            if let Some(t) = since {
                if t.elapsed() >= limit {
                    let owner = users.get(&h.user_id).cloned().unwrap_or_default();
                    out.push(Alert {
                        kind: "network_down",
                        subject: format!("{owner}/{}", h.name),
                        message: format!(
                            "Network {owner}/{} has been disconnected for more than {} minutes",
                            h.name, rules.network_down_minutes
                        ),
                        since: now - t.elapsed().as_millis() as i64,
                    });
                }
            }
        }
        ctx.alerts
            .down_since
            .lock()
            .expect("alerts lock")
            .retain(|id, _| seen.contains(id));
    }
    if rules.failed_logins_10min > 0 {
        let since = now - 600_000;
        let n = ctx
            .db
            .call(move |c| db::count_failed_logins_since(c, since))
            .await
            .unwrap_or(0);
        if n >= rules.failed_logins_10min as i64 {
            out.push(Alert {
                kind: "failed_logins",
                subject: String::new(),
                message: format!("{n} failed logins in the last 10 minutes"),
                since: now,
            });
        }
    }
    if rules.memory_mb > 0 {
        if let Some(rss) = crate::metrics::rss_bytes() {
            let mb = rss / 1_048_576;
            if mb >= rules.memory_mb as u64 {
                out.push(Alert {
                    kind: "memory",
                    subject: String::new(),
                    message: format!("xBNC uses {mb} MB of memory (limit {} MB)", rules.memory_mb),
                    since: now,
                });
            }
        }
    }
    if rules.database_mb > 0 {
        if let Ok(meta) = std::fs::metadata(&ctx.config.database.path) {
            let mb = meta.len() / 1_048_576;
            if mb >= rules.database_mb as u64 {
                out.push(Alert {
                    kind: "database",
                    subject: String::new(),
                    message: format!("The database is {mb} MB (limit {} MB)", rules.database_mb),
                    since: now,
                });
            }
        }
    }
    out
}

/// Runs forever, evaluating the rules every 30 seconds.
pub async fn alert_loop(ctx: Arc<Ctx>, mut shutdown: tokio::sync::watch::Receiver<bool>) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(30)) => {}
            _ = shutdown.changed() => return,
        }
        tick(&ctx).await;
    }
}

/// One evaluation round (public for tests).
pub async fn tick(ctx: &Arc<Ctx>) {
    let rules = ctx.settings.get().alerts.clone();
    let mut fresh = evaluate(ctx, &rules).await;
    let (started, ended) = {
        let mut active = ctx.alerts.active.lock().expect("alerts lock");
        // Keep the original start time of alerts that are still active.
        for a in fresh.iter_mut() {
            if let Some(old) = active.iter().find(|o| key(o) == key(a)) {
                a.since = old.since;
            }
        }
        let started: Vec<Alert> = fresh
            .iter()
            .filter(|a| !active.iter().any(|o| key(o) == key(a)))
            .cloned()
            .collect();
        let ended: Vec<Alert> = active
            .iter()
            .filter(|o| !fresh.iter().any(|a| key(a) == key(o)))
            .cloned()
            .collect();
        *active = fresh;
        (started, ended)
    };
    for (a, firing) in started
        .iter()
        .map(|a| (a, true))
        .chain(ended.iter().map(|a| (a, false)))
    {
        announce(ctx, &rules, a, firing).await;
    }
}

async fn announce(ctx: &Arc<Ctx>, rules: &AlertSettings, a: &Alert, firing: bool) {
    let state = if firing { "firing" } else { "resolved" };
    info!(kind = a.kind, subject = %a.subject, state, "{}", a.message);
    let detail = format!("{state}: {}", a.message);
    let db = ctx.db.clone();
    let _ = db
        .call(move |c| db::audit(c, "xbnc", "alert", &detail))
        .await;
    if rules.notify_irc {
        let admins: Vec<i64> = ctx
            .db
            .call(|c| {
                Ok(db::list_users(c)?
                    .into_iter()
                    .filter(|u| u.enabled && (u.role == "superadmin" || u.role == "admin"))
                    .map(|u| u.id)
                    .collect())
            })
            .await
            .unwrap_or_default();
        for h in ctx.registry.handles() {
            if admins.contains(&h.user_id) {
                let _ = h.tx.try_send(NetCmd::Alert {
                    kind: a.kind,
                    subject: a.subject.clone(),
                    message: a.message.clone(),
                    firing,
                });
            }
        }
    }
    if !rules.webhook_url.is_empty() {
        let body = serde_json::json!({
            "source": "xbnc",
            "server": ctx.config.hostname,
            "state": state,
            "alert": a.kind,
            "subject": a.subject,
            "message": a.message,
            "since": a.since,
            // Slack- and Discord-compatible text fields.
            "text": format!("[xBNC {}] {}: {}", ctx.config.hostname, state, a.message),
            "content": format!("[xBNC {}] {}: {}", ctx.config.hostname, state, a.message),
        })
        .to_string();
        let url = rules.webhook_url.clone();
        let tls = ctx.tls_client.clone();
        tokio::spawn(async move {
            if let Err(e) = post_json(&url, &body, tls).await {
                warn!(error = %e, "alert webhook failed");
            }
        });
    }
}

// ------------------------------------------------------------ webhook

fn parse_url(u: &str) -> Option<crate::http::Url> {
    crate::http::Url::parse(u)
}

/// POSTs the alert; an error unless the answer is 2xx.
async fn post_json(url: &str, body: &str, tls: Arc<rustls::ClientConfig>) -> anyhow::Result<()> {
    let r = crate::http::request(
        "POST",
        url,
        Some(("application/json", body.as_bytes())),
        tls,
        64 * 1024,
    )
    .await?;
    if (200..300).contains(&r.status) {
        Ok(())
    } else {
        anyhow::bail!("webhook answered {}", r.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn urls() {
        let u = parse_url("https://hooks.example.org/a/b?x=1").unwrap();
        assert!(u.tls && u.host == "hooks.example.org" && u.port == 443 && u.path == "/a/b?x=1");
        let u = parse_url("http://10.0.0.5:8080").unwrap();
        assert!(!u.tls && u.host == "10.0.0.5" && u.port == 8080 && u.path == "/");
        let u = parse_url("http://[fd00::1]:9000/x").unwrap();
        assert_eq!((u.host.as_str(), u.port), ("fd00::1", 9000));
        assert!(parse_url("ftp://x").is_none());
        assert!(parse_url("http://user:pw@x/").is_none());
        assert!(parse_url("http://x/\r\nHost: y").is_none());
        assert!(parse_url("http://:80/").is_none());
    }

    #[test]
    fn settings_validation() {
        assert!(AlertSettings::default().validate().is_ok());
        let bad = AlertSettings {
            webhook_url: "javascript:alert(1)".into(),
            ..Default::default()
        };
        assert!(bad.validate().is_err());
    }

    #[tokio::test]
    async fn webhook_post_reaches_a_server() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let srv = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = s.read(&mut buf).await.unwrap();
            s.write_all(b"HTTP/1.1 204 No Content\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8_lossy(&buf[..n]).to_string()
        });
        let tls = crate::server::tls_client_config().unwrap();
        post_json(&format!("http://127.0.0.1:{port}/hook"), "{\"a\":1}", tls)
            .await
            .unwrap();
        let req = srv.await.unwrap();
        assert!(req.starts_with("POST /hook HTTP/1.1"));
        assert!(req.contains("Content-Type: application/json"));
        assert!(req.ends_with("{\"a\":1}"));
    }
}
