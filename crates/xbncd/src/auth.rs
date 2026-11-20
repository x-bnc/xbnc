//! Client authentication: login string parsing, password verification and
//! brute-force protection.

use crate::crypto;
use crate::db::{self, Db, User};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What a client asked for in its login string.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoginSpec {
    pub username: String,
    pub network: Option<String>,
    pub client: Option<String>,
}

/// Parses `user`, `user/network`, `user@client`, `user/network@client` or
/// `user@client/network` (the order some other bouncers use).
pub fn parse_login(s: &str) -> LoginSpec {
    let mut spec = LoginSpec::default();
    let end = s.find(['/', '@']).unwrap_or(s.len());
    spec.username = s[..end].to_string();
    let mut rest = &s[end..];
    while let Some(sep) = rest.chars().next() {
        let body = &rest[1..];
        let next = body.find(['/', '@']).unwrap_or(body.len());
        let value = &body[..next];
        if !value.is_empty() {
            match sep {
                '/' => spec.network = Some(value.to_string()),
                _ => spec.client = Some(value.to_string()),
            }
        }
        rest = &body[next..];
    }
    spec
}

/// Splits a PASS argument of the form `login:password`. A PASS without `:` is
/// treated as a bare password.
pub fn split_pass(pass: &str) -> (Option<&str>, &str) {
    match pass.split_once(':') {
        Some((login, pw)) if !login.is_empty() => (Some(login), pw),
        _ => (None, pass),
    }
}

/// Whether a password is one of the placeholders shown in examples
/// ("YOUR_PASSWORD", "<password>"…), in any installed language: someone
/// copied the example without writing their password.
pub fn is_password_placeholder(pw: &str) -> bool {
    let p = pw
        .trim()
        .trim_matches(|c| c == '<' || c == '>' || c == '"' || c == '\'');
    if p.is_empty() {
        return false;
    }
    let lower = p.to_lowercase();
    const KNOWN: &[&str] = &[
        "your_password",
        "yourpassword",
        "password",
        "contraseña",
        "tu_contraseña",
    ];
    KNOWN.contains(&lower.as_str())
        || crate::i18n::panel_text_everywhere("YOUR_PASSWORD")
            .iter()
            .any(|t| t.to_lowercase() == lower)
}

/// Normalises a client name (used to keep a per-device history cursor).
pub fn normalise_client_name(name: Option<&str>) -> String {
    match name {
        Some(n) if !n.is_empty() => n
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(32)
            .collect::<String>()
            .to_ascii_lowercase(),
        _ => "default".to_string(),
    }
}

/// Per-IP tracker of failed logins with temporary blocking.
pub struct LoginLimiter {
    params: Mutex<(u32, Duration, Duration)>,
    state: Mutex<HashMap<IpAddr, Entry>>,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    failures: u32,
    window_start: Instant,
    blocked_until: Option<Instant>,
}

/// A currently blocked address, for the security page.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BlockedIp {
    pub ip: String,
    pub remaining_seconds: u64,
}

impl LoginLimiter {
    pub fn new(max_failures: u32, window: Duration, block: Duration) -> Self {
        LoginLimiter {
            params: Mutex::new((max_failures.max(1), window, block)),
            state: Mutex::new(HashMap::new()),
        }
    }

    /// Applies new thresholds (from the settings page) immediately.
    pub fn configure(&self, max_failures: u32, window: Duration, block: Duration) {
        *self.params.lock().expect("limiter lock") = (max_failures.max(1), window, block);
    }

    /// True if the address is currently blocked.
    pub fn is_blocked(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let map = self.state.lock().expect("limiter lock");
        map.get(&ip)
            .and_then(|e| e.blocked_until)
            .map(|t| t > now)
            .unwrap_or(false)
    }

    /// How long the address stays blocked, if it is blocked.
    pub fn blocked_for(&self, ip: IpAddr) -> Option<Duration> {
        let now = Instant::now();
        let map = self.state.lock().expect("limiter lock");
        map.get(&ip)
            .and_then(|e| e.blocked_until)
            .filter(|t| *t > now)
            .map(|t| t - now)
    }

    /// The English message for a blocked address (the panel translates it).
    pub fn blocked_message(&self, ip: IpAddr) -> String {
        let mins = self
            .blocked_for(ip)
            .map_or(1, |d| d.as_secs().div_ceil(60).max(1));
        format!(
            "Too many failed login attempts from your address ({ip}); blocked for {mins} more minutes"
        )
    }

    /// Minutes an address is blocked for after too many failures.
    pub fn block_minutes(&self) -> u64 {
        let (_, _, block) = *self.params.lock().expect("limiter lock");
        block.as_secs().div_ceil(60).max(1)
    }

    /// Records a failure. Returns true if the address is now blocked.
    pub fn record_failure(&self, ip: IpAddr) -> bool {
        let (max_failures, window, block) = *self.params.lock().expect("limiter lock");
        let now = Instant::now();
        let mut map = self.state.lock().expect("limiter lock");
        if map.len() > 10_000 {
            map.retain(|_, e| {
                e.blocked_until.map(|t| t > now).unwrap_or(false)
                    || now.duration_since(e.window_start) < window
            });
        }
        let e = map.entry(ip).or_insert(Entry {
            failures: 0,
            window_start: now,
            blocked_until: None,
        });
        if now.duration_since(e.window_start) > window {
            e.failures = 0;
            e.window_start = now;
        }
        e.failures += 1;
        if e.failures >= max_failures {
            e.blocked_until = Some(now + block);
            e.failures = 0;
            e.window_start = now;
            return true;
        }
        false
    }

    /// Clears the failure counter after a successful login.
    pub fn record_success(&self, ip: IpAddr) {
        self.state.lock().expect("limiter lock").remove(&ip);
    }

    /// Addresses blocked right now.
    pub fn blocked(&self) -> Vec<BlockedIp> {
        let now = Instant::now();
        let map = self.state.lock().expect("limiter lock");
        let mut v: Vec<BlockedIp> = map
            .iter()
            .filter_map(|(ip, e)| {
                e.blocked_until.filter(|t| *t > now).map(|t| BlockedIp {
                    ip: ip.to_string(),
                    remaining_seconds: t.duration_since(now).as_secs(),
                })
            })
            .collect();
        v.sort_by(|a, b| a.ip.cmp(&b.ip));
        v
    }

    /// Lifts a block. Returns false if the address was not blocked.
    pub fn unblock(&self, ip: IpAddr) -> bool {
        self.state
            .lock()
            .expect("limiter lock")
            .remove(&ip)
            .is_some()
    }
}

/// Outcome of a password check.
#[derive(Debug)]
pub enum AuthResult {
    Ok(Box<User>),
    BadCredentials,
    Disabled,
}

/// Verifies a username/password pair. Unknown users still pay the cost of a
/// hash verification so that timing does not reveal which accounts exist.
pub async fn authenticate(
    db: &Db,
    username: String,
    password: crypto::Secret,
) -> anyhow::Result<AuthResult> {
    let user = {
        let name = username.clone();
        db.call(move |c| db::get_user(c, &name)).await?
    };
    let dbc = db.clone();
    let result = tokio::task::spawn_blocking(move || match user {
        Some(u) => {
            if crypto::verify_password(&u.password_hash, password.expose()) {
                if crypto::needs_rehash(&u.password_hash) {
                    // Imported hash: replace it with Argon2id now that the
                    // password is known.
                    if let Ok(h) = crypto::hash_password(password.expose()) {
                        let name = u.username.clone();
                        let _ = dbc.with(|c| db::set_user_password(c, &name, &h));
                    }
                }
                if u.enabled {
                    AuthResult::Ok(Box::new(u))
                } else {
                    AuthResult::Disabled
                }
            } else {
                AuthResult::BadCredentials
            }
        }
        None => {
            let _ = crypto::verify_password(crypto::dummy_hash(), password.expose());
            AuthResult::BadCredentials
        }
    })
    .await?;
    Ok(result)
}

/// Like [`authenticate`], then (if configured) the LDAP directory: a user
/// whose local password does not match may still sign in with the
/// directory password of a linked (or automatically created) account.
pub async fn authenticate_ctx(
    ctx: &crate::server::Ctx,
    username: String,
    password: crypto::Secret,
) -> anyhow::Result<AuthResult> {
    let pw = password.expose().to_string();
    let local = authenticate(&ctx.db, username.clone(), password).await?;
    let Some(ldap) = ctx.config.ldap.as_ref() else {
        return Ok(local);
    };
    if !matches!(local, AuthResult::BadCredentials) {
        return Ok(local);
    }
    match crate::sso::ldap_bind(ldap, ctx.tls_client.clone(), &username, &pw).await {
        Ok(true) => {}
        Ok(false) => return Ok(AuthResult::BadCredentials),
        Err(e) => {
            tracing::warn!(error = %e, "LDAP sign-in failed");
            return Ok(AuthResult::BadCredentials);
        }
    }
    let dn = ldap.dn_for(&username).to_ascii_lowercase();
    match crate::sso::provision(
        &ctx.db,
        "ldap",
        &dn,
        Some(username.clone()),
        ldap.auto_create,
        ldap.link_existing,
        &ldap.default_role,
    )
    .await?
    {
        crate::sso::Provision::User(u) if u.enabled => Ok(AuthResult::Ok(u)),
        crate::sso::Provision::User(_) => Ok(AuthResult::Disabled),
        _ => {
            tracing::warn!(user = %username, "LDAP password is right but no xBNC user is linked");
            Ok(AuthResult::BadCredentials)
        }
    }
}

/// Returns a copy of a raw IRC line safe to write to logs: credentials in
/// PASS, AUTHENTICATE, OPER and NickServ/IDENTIFY-style messages are masked.
pub fn redact_line(line: &str) -> String {
    let parsed = match xbnc_proto::Message::parse(line) {
        Ok(m) => m,
        Err(_) => return "<unparseable line>".to_string(),
    };
    let mut m = parsed;
    match m.command.as_str() {
        "PASS" | "AUTHENTICATE" | "OPER" => {
            for p in m.params.iter_mut() {
                *p = "***".into();
            }
        }
        "PRIVMSG" | "NOTICE" => {
            let to_service = m
                .param(0)
                .map(|t| {
                    let t = t.to_ascii_lowercase();
                    t.ends_with("serv") || t.starts_with("nickserv@") || t == "*crypt"
                })
                .unwrap_or(false);
            let text_lower = m.param(1).unwrap_or("").to_ascii_lowercase();
            let sensitive = [
                "identify", "register", "ghost", "recover", "regain", "password", "login", "auth",
                "setkey",
            ];
            if (to_service || sensitive.iter().any(|k| text_lower.starts_with(k)))
                && m.params.len() > 1
            {
                m.params[1] = "***".into();
            }
        }
        _ => {}
    }
    m.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_forms() {
        assert_eq!(
            parse_login("alice"),
            LoginSpec {
                username: "alice".into(),
                network: None,
                client: None
            }
        );
        assert_eq!(
            parse_login("alice/libera@laptop"),
            LoginSpec {
                username: "alice".into(),
                network: Some("libera".into()),
                client: Some("laptop".into())
            }
        );
        assert_eq!(
            parse_login("alice@phone/oftc"),
            LoginSpec {
                username: "alice".into(),
                network: Some("oftc".into()),
                client: Some("phone".into())
            }
        );
        assert_eq!(parse_login("alice/").network, None);
    }

    #[test]
    fn pass_split() {
        assert_eq!(
            split_pass("alice/libera:p:w"),
            (Some("alice/libera"), "p:w")
        );
        assert_eq!(split_pass("justpassword"), (None, "justpassword"));
        assert_eq!(split_pass(":pw"), (None, ":pw"));
    }

    #[test]
    fn client_names() {
        assert_eq!(normalise_client_name(None), "default");
        assert_eq!(normalise_client_name(Some("My Phone!")), "myphone");
    }

    #[test]
    fn limiter_blocks_after_failures() {
        let l = LoginLimiter::new(3, Duration::from_secs(60), Duration::from_secs(60));
        let ip: IpAddr = "192.0.2.1".parse().unwrap();
        let other: IpAddr = "192.0.2.2".parse().unwrap();
        assert!(!l.record_failure(ip));
        assert!(!l.record_failure(ip));
        assert!(l.record_failure(ip));
        assert!(l.is_blocked(ip));
        assert!(!l.is_blocked(other));
        l.record_success(ip);
        assert!(!l.is_blocked(ip));
        l.configure(1, Duration::from_secs(60), Duration::from_secs(60));
        assert!(l.record_failure(other));
        assert_eq!(l.blocked().len(), 1);
        assert!(l.unblock(other));
        assert!(!l.is_blocked(other));
    }

    #[test]
    fn redaction() {
        assert_eq!(redact_line("PASS alice/x:secret"), "PASS ***");
        assert_eq!(
            redact_line("AUTHENTICATE Zm9vAGZvbwBiYXI="),
            "AUTHENTICATE ***"
        );
        assert_eq!(
            redact_line("PRIVMSG NickServ :IDENTIFY hunter2"),
            "PRIVMSG NickServ ***"
        );
        assert_eq!(
            redact_line("PRIVMSG #xbnc-dev :hello"),
            "PRIVMSG #xbnc-dev hello"
        );
        assert!(!redact_line("PRIVMSG Q@CServe.quakenet.org :AUTH me pw").contains("pw"));
    }

    #[tokio::test]
    async fn authenticate_paths() {
        let db = Db::open_in_memory().unwrap();
        let h = crypto::hash_password("a-good-password").unwrap();
        db.with(|c| db::create_user(c, "alice", &h, false)).unwrap();
        let ok = authenticate(&db, "alice".into(), crypto::Secret::new("a-good-password"))
            .await
            .unwrap();
        assert!(matches!(ok, AuthResult::Ok(_)));
        let bad = authenticate(&db, "alice".into(), crypto::Secret::new("nope"))
            .await
            .unwrap();
        assert!(matches!(bad, AuthResult::BadCredentials));
        let unknown = authenticate(&db, "bob".into(), crypto::Secret::new("x"))
            .await
            .unwrap();
        assert!(matches!(unknown, AuthResult::BadCredentials));
        db.with(|c| db::set_user_enabled(c, "alice", false))
            .unwrap();
        let dis = authenticate(&db, "alice".into(), crypto::Secret::new("a-good-password"))
            .await
            .unwrap();
        assert!(matches!(dis, AuthResult::Disabled));
    }
}
