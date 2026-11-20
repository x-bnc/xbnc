//! Notifications of mentions and private messages while the user is away:
//! Telegram, Discord, a web address of their own, e-mail and Web Push.

use crate::db;
use crate::server::Ctx;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tracing::warn;

/// A user's notification settings (stored encrypted: they hold tokens).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NotifySettings {
    pub enabled: bool,
    /// Only while no IRC client (or panel chat) of the user is connected.
    pub only_when_away: bool,
    pub privates: bool,
    pub mentions: bool,
    pub telegram_token: String,
    pub telegram_chat: String,
    pub discord_webhook: String,
    pub webhook_url: String,
    pub email: String,
    pub push: bool,
}

impl Default for NotifySettings {
    fn default() -> Self {
        NotifySettings {
            enabled: false,
            only_when_away: true,
            privates: true,
            mentions: true,
            telegram_token: String::new(),
            telegram_chat: String::new(),
            discord_webhook: String::new(),
            webhook_url: String::new(),
            email: String::new(),
            push: true,
        }
    }
}

impl NotifySettings {
    pub fn validate(&self) -> Result<()> {
        let tok_ok = self.telegram_token.is_empty()
            || (self.telegram_token.len() <= 100
                && self
                    .telegram_token
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '_' || c == '-'));
        if !tok_ok {
            bail!("invalid Telegram bot token");
        }
        if !self.telegram_chat.is_empty()
            && (self.telegram_chat.len() > 64
                || !self
                    .telegram_chat
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '@' || c == '_'))
        {
            bail!("invalid Telegram chat id");
        }
        if self.telegram_token.is_empty() != self.telegram_chat.is_empty() {
            bail!("Telegram needs both the bot token and the chat id");
        }
        if !self.discord_webhook.is_empty()
            && !(self
                .discord_webhook
                .starts_with("https://discord.com/api/webhooks/")
                || self
                    .discord_webhook
                    .starts_with("https://discordapp.com/api/webhooks/"))
        {
            bail!("the Discord webhook must start with https://discord.com/api/webhooks/");
        }
        for u in [&self.discord_webhook, &self.webhook_url] {
            if !u.is_empty() && (u.len() > 500 || crate::http::Url::parse(u).is_none_or(|p| !p.tls))
            {
                bail!("web addresses must be valid https:// addresses");
            }
        }
        if !self.email.is_empty()
            && (self.email.len() > 200
                || self.email.contains(['\r', '\n', ' ', '<', '>', ','])
                || !self.email.contains('@'))
        {
            bail!("invalid e-mail address");
        }
        Ok(())
    }
}

pub fn load(ctx: &Ctx, conn: &crate::sql::Conn, user_id: i64) -> Result<NotifySettings> {
    match db::notify_settings(conn, user_id)? {
        Some(enc) => {
            let json = ctx.secrets.decrypt(&enc)?;
            Ok(serde_json::from_str(json.expose()).unwrap_or_default())
        }
        None => Ok(NotifySettings::default()),
    }
}

pub fn save(ctx: &Ctx, conn: &crate::sql::Conn, user_id: i64, s: &NotifySettings) -> Result<()> {
    s.validate()?;
    let enc = ctx.secrets.encrypt(&serde_json::to_string(s)?);
    db::set_notify_settings(conn, user_id, &enc)
}

/// What to tell the user.
#[derive(Debug, Clone, Serialize)]
pub struct Highlight {
    pub network: String,
    pub network_id: i64,
    pub conversation: String,
    pub from: String,
    pub text: String,
    pub private: bool,
    pub ts: i64,
}

impl Highlight {
    pub fn title(&self) -> String {
        if self.private {
            format!("{} ({})", self.from, self.network)
        } else {
            format!("{} {} ({})", self.from, self.conversation, self.network)
        }
    }

    pub fn line(&self) -> String {
        if self.private {
            format!("[{}] <{}> {}", self.network, self.from, self.text)
        } else {
            format!(
                "[{} {}] <{}> {}",
                self.network, self.conversation, self.from, self.text
            )
        }
    }
}

/// At most this many notifications per user in RATE_WINDOW; the rest are
/// counted and mentioned in the next one.
const RATE_MAX: usize = 10;
const RATE_WINDOW: Duration = Duration::from_secs(300);

#[derive(Default)]
pub struct Notifier {
    sent: Mutex<HashMap<i64, (VecDeque<std::time::Instant>, u32)>>,
}

impl Notifier {
    /// Ok(Some(skipped)) when a notification may go out now.
    fn allow(&self, user_id: i64) -> Option<u32> {
        let mut m = self.sent.lock().unwrap_or_else(|e| e.into_inner());
        let (q, skipped) = m.entry(user_id).or_default();
        let now = std::time::Instant::now();
        while q
            .front()
            .is_some_and(|t| now.duration_since(*t) > RATE_WINDOW)
        {
            q.pop_front();
        }
        if q.len() >= RATE_MAX {
            *skipped += 1;
            return None;
        }
        q.push_back(now);
        Some(std::mem::take(skipped))
    }
}

/// Called by a network when a mention or a private message arrives.
pub fn highlight(ctx: &Arc<Ctx>, user_id: i64, h: Highlight) {
    let ctx = ctx.clone();
    tokio::spawn(async move {
        let c2 = ctx.clone();
        let settings = match ctx.db.call(move |c| load(&c2, c, user_id)).await {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "cannot read notification settings");
                return;
            }
        };
        if !settings.enabled
            || (h.private && !settings.privates)
            || (!h.private && !settings.mentions)
            || (settings.only_when_away && ctx.registry.clients_of(user_id) > 0)
        {
            return;
        }
        let Some(skipped) = ctx.notify.allow(user_id) else {
            return;
        };
        for (channel, r) in deliver(&ctx, user_id, &settings, &h, skipped).await {
            if let Err(e) = r {
                warn!(user_id, channel, error = %e, "notification failed");
            }
        }
    });
}

/// Sends to every configured channel; returns each result.
pub async fn deliver(
    ctx: &Arc<Ctx>,
    user_id: i64,
    s: &NotifySettings,
    h: &Highlight,
    skipped: u32,
) -> Vec<(&'static str, Result<()>)> {
    let mut text = h.line();
    if skipped > 0 {
        text.push_str(&format!(" (+{skipped})"));
    }
    let tls = ctx.tls_client.clone();
    let mut out = Vec::new();
    if !s.telegram_token.is_empty() {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            s.telegram_token
        );
        let body = serde_json::json!({ "chat_id": s.telegram_chat, "text": text, "disable_web_page_preview": true });
        out.push(("telegram", post_json(&url, &body, tls.clone()).await));
    }
    if !s.discord_webhook.is_empty() {
        let body = serde_json::json!({ "content": text, "allowed_mentions": { "parse": [] } });
        out.push((
            "discord",
            post_json(&s.discord_webhook, &body, tls.clone()).await,
        ));
    }
    if !s.webhook_url.is_empty() {
        let mut body = serde_json::to_value(h).unwrap_or_default();
        body["skipped"] = skipped.into();
        out.push((
            "webhook",
            post_json(&s.webhook_url, &body, tls.clone()).await,
        ));
    }
    if !s.email.is_empty() {
        let r = match &ctx.config.smtp {
            Some(cfg) => {
                send_mail(
                    cfg,
                    tls.clone(),
                    &s.email,
                    &format!("xBNC: {}", h.title()),
                    &text,
                )
                .await
            }
            None => Err(anyhow::anyhow!(
                "e-mail is not configured on this server ([smtp])"
            )),
        };
        out.push(("email", r));
    }
    if s.push {
        out.push(("push", push_all(ctx, user_id, h, &text).await));
    }
    out
}

async fn post_json(
    url: &str,
    body: &serde_json::Value,
    tls: Arc<rustls::ClientConfig>,
) -> Result<()> {
    let data = serde_json::to_vec(body)?;
    let r = crate::http::request_public(
        "POST",
        url,
        Some(("application/json", &data)),
        tls,
        64 * 1024,
        Duration::from_secs(15),
        &[],
    )
    .await?;
    if !(200..300).contains(&r.status) {
        bail!("the server answered {}", r.status);
    }
    Ok(())
}

/// The server's push key, created on first use.
pub async fn vapid(ctx: &Arc<Ctx>) -> Result<crate::push::Vapid> {
    let c2 = ctx.clone();
    ctx.db
        .call(move |c| {
            if let Some(enc) = db::get_setting(c, "push.vapid")? {
                use base64::Engine;
                let raw = base64::engine::general_purpose::STANDARD
                    .decode(c2.secrets.decrypt(&enc)?.expose())?;
                return crate::push::Vapid::from_pkcs8(&raw);
            }
            let v = crate::push::Vapid::generate()?;
            use base64::Engine;
            let enc = c2
                .secrets
                .encrypt(&base64::engine::general_purpose::STANDARD.encode(v.pkcs8()));
            db::set_setting(c, "push.vapid", &enc)?;
            Ok(v)
        })
        .await
}

async fn push_all(ctx: &Arc<Ctx>, user_id: i64, h: &Highlight, text: &str) -> Result<()> {
    let subs = ctx
        .db
        .call(move |c| db::push_subscriptions(c, user_id))
        .await?;
    if subs.is_empty() {
        return Ok(());
    }
    let v = vapid(ctx).await?;
    let payload = serde_json::json!({
        "title": h.title(),
        "body": text,
        "url": format!("/chat?network={}&target={}", h.network_id, urlencode(&h.conversation)),
        "tag": format!("{}:{}", h.network_id, h.conversation),
    })
    .to_string();
    let subject = format!("mailto:xbnc@{}", ctx.config.hostname);
    let mut last_err = None;
    for s in subs {
        match crate::push::send(
            ctx.tls_client.clone(),
            &v,
            &subject,
            &s.endpoint,
            &s.p256dh,
            &s.auth,
            payload.as_bytes(),
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => {
                let ep = s.endpoint.clone();
                let _ = ctx
                    .db
                    .call(move |c| db::delete_push_subscription(c, user_id, &ep))
                    .await;
            }
            Err(e) => last_err = Some(e),
        }
    }
    last_err.map_or(Ok(()), Err)
}

pub fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ------------------------------------------------------------ e-mail

/// `[smtp]` in xbnc.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    #[serde(default = "default_smtp_port")]
    pub port: u16,
    /// "starttls" (default), "tls" (port 465) or "none" (local relays only).
    #[serde(default = "default_security")]
    pub security: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    pub from: String,
}

fn default_smtp_port() -> u16 {
    587
}
fn default_security() -> String {
    "starttls".into()
}

impl SmtpConfig {
    pub fn validate(&self) -> Result<()> {
        if self.host.is_empty() || self.from.is_empty() || !self.from.contains('@') {
            bail!("smtp needs host and from");
        }
        if !["starttls", "tls", "none"].contains(&self.security.as_str()) {
            bail!("smtp.security must be starttls, tls or none");
        }
        if self.from.contains(['\r', '\n', '<', '>']) {
            bail!("invalid smtp.from");
        }
        Ok(())
    }
}

async fn reply<S: AsyncRead + AsyncWrite + Unpin>(s: &mut BufReader<S>) -> Result<(u16, String)> {
    let mut all = String::new();
    loop {
        let mut line = String::new();
        if s.read_line(&mut line).await? == 0 {
            bail!("the mail server closed the connection");
        }
        if line.len() > 1000 || all.len() > 16_000 {
            bail!("reply too long");
        }
        all.push_str(&line);
        if line.len() < 4 || line.as_bytes()[3] != b'-' {
            let code = line
                .get(..3)
                .and_then(|c| c.parse().ok())
                .context("invalid reply")?;
            return Ok((code, all));
        }
    }
}

async fn cmd<S: AsyncRead + AsyncWrite + Unpin>(
    s: &mut BufReader<S>,
    line: &str,
    want: u16,
) -> Result<String> {
    s.get_mut()
        .write_all(format!("{line}\r\n").as_bytes())
        .await?;
    let (code, text) = reply(s).await?;
    if code / 100 != want / 100 {
        let shown = if line.starts_with("AUTH") {
            "AUTH"
        } else {
            line
        };
        bail!("{shown}: {}", text.trim());
    }
    Ok(text)
}

async fn session<S: AsyncRead + AsyncWrite + Unpin>(
    s: &mut BufReader<S>,
    cfg: &SmtpConfig,
    to: &str,
    data: &str,
    greeted: bool,
) -> Result<()> {
    if !greeted {
        cmd(s, "EHLO xbnc", 250).await?;
    }
    if !cfg.username.is_empty() {
        use base64::Engine;
        let tok = base64::engine::general_purpose::STANDARD
            .encode(format!("\0{}\0{}", cfg.username, cfg.password));
        cmd(s, &format!("AUTH PLAIN {tok}"), 235).await?;
    }
    cmd(s, &format!("MAIL FROM:<{}>", cfg.from), 250).await?;
    cmd(s, &format!("RCPT TO:<{to}>"), 250).await?;
    cmd(s, "DATA", 354).await?;
    cmd(s, &format!("{data}\r\n."), 250).await?;
    let _ = cmd(s, "QUIT", 221).await;
    Ok(())
}

/// Sends a short plain-text mail.
pub async fn send_mail(
    cfg: &SmtpConfig,
    tls: Arc<rustls::ClientConfig>,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<()> {
    if to.contains(['\r', '\n', '<', '>']) {
        bail!("invalid address");
    }
    let clean = |t: &str| t.replace(['\r', '\n'], " ");
    let mut text = String::new();
    for l in body.lines() {
        // Dot-stuffing.
        if l.starts_with('.') {
            text.push('.');
        }
        text.push_str(l);
        text.push_str("\r\n");
    }
    let date = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc2822)
        .unwrap_or_default();
    let data = format!(
        "From: xBNC <{}>\r\nTo: <{to}>\r\nSubject: {}\r\nDate: {date}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{}",
        cfg.from,
        clean(subject),
        text.trim_end()
    );
    let fut = async {
        let tcp = tokio::net::TcpStream::connect((cfg.host.as_str(), cfg.port)).await?;
        let name = rustls_pki_types::ServerName::try_from(cfg.host.clone())?;
        if cfg.security == "tls" {
            let s = tokio_rustls::TlsConnector::from(tls)
                .connect(name, tcp)
                .await?;
            let mut s = BufReader::new(s);
            reply(&mut s).await?;
            return session(&mut s, cfg, to, &data, false).await;
        }
        let mut s = BufReader::new(tcp);
        reply(&mut s).await?;
        let ehlo = cmd(&mut s, "EHLO xbnc", 250).await?;
        if cfg.security == "none" {
            return session(&mut s, cfg, to, &data, true).await;
        }
        if !ehlo.to_ascii_uppercase().contains("STARTTLS") {
            bail!("the mail server does not offer STARTTLS");
        }
        cmd(&mut s, "STARTTLS", 220).await?;
        let tcp = s.into_inner();
        let t = tokio_rustls::TlsConnector::from(tls)
            .connect(name, tcp)
            .await?;
        let mut t = BufReader::new(t);
        session(&mut t, cfg, to, &data, false).await
    };
    tokio::time::timeout(Duration::from_secs(30), fut)
        .await
        .context("the mail server did not answer in time")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_validation() {
        let mut s = NotifySettings::default();
        assert!(s.validate().is_ok());
        s.telegram_token = "123:abc".into();
        assert!(s.validate().is_err(), "chat id missing");
        s.telegram_chat = "-100123".into();
        assert!(s.validate().is_ok());
        s.discord_webhook = "https://evil.example/api/webhooks/1".into();
        assert!(s.validate().is_err());
        s.discord_webhook = "https://discord.com/api/webhooks/1/x".into();
        s.webhook_url = "http://plain.example".into();
        assert!(s.validate().is_err());
        s.webhook_url.clear();
        s.email = "a@b.c\r\nBcc: x".into();
        assert!(s.validate().is_err());
    }

    #[test]
    fn rate_limit() {
        let n = Notifier::default();
        for _ in 0..RATE_MAX {
            assert_eq!(n.allow(1), Some(0));
        }
        assert_eq!(n.allow(1), None);
        assert_eq!(n.allow(1), None);
        assert_eq!(n.allow(2), Some(0));
    }

    /// A minimal SMTP server: checks what xBNC sends.
    #[tokio::test]
    async fn sends_mail_over_smtp() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (s, _) = l.accept().await.unwrap();
            let mut s = BufReader::new(s);
            s.get_mut().write_all(b"220 hi\r\n").await.unwrap();
            let mut got = String::new();
            let mut in_data = false;
            loop {
                let mut line = String::new();
                if s.read_line(&mut line).await.unwrap() == 0 {
                    break;
                }
                got.push_str(&line);
                if in_data {
                    if line == ".\r\n" {
                        in_data = false;
                        s.get_mut().write_all(b"250 queued\r\n").await.unwrap();
                    }
                    continue;
                }
                let reply: &[u8] = if line.starts_with("EHLO") {
                    b"250-hi\r\n250 AUTH PLAIN\r\n"
                } else if line.starts_with("AUTH") {
                    b"235 ok\r\n"
                } else if line.starts_with("DATA") {
                    in_data = true;
                    b"354 go\r\n"
                } else if line.starts_with("QUIT") {
                    s.get_mut().write_all(b"221 bye\r\n").await.unwrap();
                    break;
                } else {
                    b"250 ok\r\n"
                };
                s.get_mut().write_all(reply).await.unwrap();
            }
            got
        });
        let cfg = SmtpConfig {
            host: "127.0.0.1".into(),
            port,
            security: "none".into(),
            username: "u".into(),
            password: "p".into(),
            from: "xbnc@example.org".into(),
        };
        let tls = crate::server::tls_client_config().unwrap();
        send_mail(
            &cfg,
            tls,
            "alice@example.org",
            "xBNC: bob #xbnc-dev",
            "[undernet #xbnc-dev] <bob> alice: hola\n.dot",
        )
        .await
        .unwrap();
        let got = server.await.unwrap();
        assert!(got.contains("RCPT TO:<alice@example.org>"), "{got}");
        assert!(got.contains("Subject: xBNC: bob #xbnc-dev"), "{got}");
        assert!(got.contains("\r\n..dot\r\n"), "dot-stuffing: {got}");
    }
}
