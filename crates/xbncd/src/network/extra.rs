//! Runtime of the v1.0 modules: services login, invite/unban, encodings,
//! detached channels, flood detach, colour stripping, autocycle, FiSH
//! encryption, ignore lists and scheduled tasks. Configuration lives in
//! `crate::modules`; the actor calls the hooks below.

use super::*;
use crate::modules::{glob_match, mentions, Service, When};
use std::collections::{BTreeSet, VecDeque};

/// How often autocycle may cycle the same channel.
const CYCLE_EVERY: Duration = Duration::from_secs(300);
/// How often the invite module may ask for the same channel.
const INVITE_EVERY: Duration = Duration::from_secs(600);
const INVITE_TRIES: usize = 3;
/// DH1080 INITs answered per window, and session keys kept at most.
const KEYX_WINDOW: Duration = Duration::from_secs(60);
const KEYX_PER_WINDOW: usize = 5;
const KEYX_MAX_KEYS: usize = 100;
/// Seconds joins wait for the hidden host when no wait is configured.
const HIDE_WAIT_SECS: u64 = 30;
/// JOINs kept while joins are held.
pub(super) const HELD_JOINS_MAX: usize = 50;
/// Messages replayed when a detached channel comes back.
const REPLAY: usize = 100;

/// Runtime state of the v1.0 modules for one connection.
#[derive(Debug, Default)]
pub(super) struct ExtraRuntime {
    /// Channel joins wait for the services login until this instant.
    pub join_hold: Option<Instant>,
    /// The commands on connect set the user mode +x (hidden host).
    pub perform_hide: bool,
    /// Joins stay held after the wait because the host is still not
    /// hidden: joining would show the real address.
    pub joins_blocked: bool,
    /// JOINs sent while joins were held, sent when they are released.
    pub held_joins: Vec<Message>,
    pub host_hidden: bool,
    pub logged_in: bool,
    pub login_sent: bool,
    /// Detached channels brought back for this connection (folded).
    pub attached: HashSet<String>,
    /// Channels hidden because of flooding: (calm since, last id when hidden).
    pub flood_hidden: HashMap<String, (Instant, i64)>,
    pub flood_counts: HashMap<String, VecDeque<Instant>>,
    /// Last cycle per channel.
    pub cycled: HashMap<String, Instant>,
    /// Channel modes from 324 and MODE (only the letters we need: i, k).
    pub chan_modes: HashMap<String, String>,
    /// Requests to services per channel.
    pub invite_asks: HashMap<String, Vec<Instant>>,
    /// DH1080 exchanges waiting for an answer (folded nick).
    pub keyx_pending: HashMap<String, (crate::fish::Dh1080, bool)>,
    /// Keys agreed with DH1080 for this session (folded nick → key, cbc).
    pub keyx_keys: HashMap<String, (String, bool)>,
    /// Recent DH1080 INITs answered (rate limit).
    pub keyx_recent: VecDeque<Instant>,
    /// Masks currently on the server's SILENCE list.
    pub silenced: BTreeSet<String>,
    /// Scheduled tasks: last minute run (At) or last run (Every), by index.
    pub task_minute: HashMap<usize, i64>,
    pub task_last: HashMap<usize, Instant>,
}

/// Removes mIRC formatting: colours, bold, italics, underline, reverse,
/// monospace, strike-through and reset. CTCP markers are kept.
pub fn strip_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\x02' | '\x0f' | '\x11' | '\x16' | '\x1d' | '\x1e' | '\x1f' => {}
            '\x03' => {
                for _ in 0..2 {
                    if it.peek().is_some_and(|d| d.is_ascii_digit()) {
                        it.next();
                    }
                }
                if it.peek() == Some(&',') {
                    let mut look = it.clone();
                    look.next();
                    if look.peek().is_some_and(|d| d.is_ascii_digit()) {
                        it.next();
                        for _ in 0..2 {
                            if it.peek().is_some_and(|d| d.is_ascii_digit()) {
                                it.next();
                            }
                        }
                    }
                }
            }
            '\x04' => {
                for _ in 0..6 {
                    if it.peek().is_some_and(|d| d.is_ascii_hexdigit()) {
                        it.next();
                    }
                }
                if it.peek() == Some(&',') {
                    it.next();
                    for _ in 0..6 {
                        if it.peek().is_some_and(|d| d.is_ascii_hexdigit()) {
                            it.next();
                        }
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// True if `name` matches one of the names or globs in `list`.
fn listed(list: &[String], name: &str) -> bool {
    list.iter()
        .any(|p| p == "*" || p.eq_ignore_ascii_case(name) || glob_match(p, name))
}

impl NetActor {
    fn service(&self, s: Service) -> Service {
        s.resolve(
            self.isupport.network.as_deref().unwrap_or(""),
            &self.net.host,
        )
    }

    fn say_status(&mut self, key: &str, args: &[&str]) {
        let text = self.t(key, args);
        self.notice_all(&text);
    }

    // ---------------------------------------------------------- services

    /// Login-on-connect for Undernet's X: the account goes in the server
    /// password (`+x! account password`), so the real host is never shown.
    pub(super) fn services_pass(&self) -> Option<String> {
        let cfg = self.mods.services.as_ref()?;
        let svc = cfg.service.resolve("", &self.net.host);
        if svc != Service::X
            || !cfg.login_on_connect
            || cfg.account.is_empty()
            || cfg.password.is_empty()
            || self.net.server_password_enc.is_some()
        {
            return None;
        }
        let flag = if cfg.hide_host { "+x!" } else { "-x!" };
        Some(format!("{flag} {} {}", cfg.account, cfg.password))
    }

    /// Whether a hidden host was asked for: +x in the commands on connect,
    /// or the Services login module with X, Q or AuthServ and "hide host".
    pub(super) fn hide_wanted(&self) -> bool {
        if self.rt.x.perform_hide {
            return true;
        }
        self.mods.services.as_ref().is_some_and(|cfg| {
            cfg.hide_host
                && matches!(
                    self.service(cfg.service),
                    Service::X | Service::Q | Service::AuthServ
                )
        })
    }

    /// A hidden host was asked for and the server has not hidden it yet.
    pub(super) fn hide_pending(&self) -> bool {
        self.hide_wanted() && !self.rt.x.host_hidden
    }

    /// Whether JOINs to the server are held for now.
    pub(super) fn joins_held(&self) -> bool {
        self.rt.x.join_hold.is_some() || self.rt.x.joins_blocked
    }

    /// Whether channel joins must wait for the login or the hidden host.
    fn services_must_wait(&self) -> bool {
        if self.hide_pending() {
            return true;
        }
        let Some(cfg) = &self.mods.services else {
            return false;
        };
        if cfg.wait == 0 {
            return false;
        }
        let svc = self.service(cfg.service);
        if svc == Service::Auto {
            return false;
        }
        let host = cfg.hide_host && matches!(svc, Service::X | Service::Q | Service::AuthServ);
        let login = !cfg.password.is_empty()
            && svc == Service::NickServ
            && self.net.sasl_username.is_none();
        (host && !self.rt.x.host_hidden) || (login && !self.rt.x.logged_in)
    }

    /// Seconds channel joins wait for the login or the hidden host.
    fn join_wait(&self) -> u64 {
        match self.mods.services.as_ref().map_or(0, |c| c.wait) {
            0 if self.hide_wanted() => HIDE_WAIT_SECS,
            w => w,
        }
    }

    /// Called on 001: joins the channels now, or holds them until the
    /// login is done and, if asked for, the host is hidden. Channels are
    /// never joined with the real host showing when a hidden host was
    /// asked for.
    pub(super) async fn services_on_welcome(&mut self) {
        let wait = self.join_wait();
        if wait > 0 && self.services_must_wait() {
            self.rt.x.join_hold = Some(Instant::now() + Duration::from_secs(wait));
        } else {
            self.release_joins().await;
        }
    }

    /// Joins the saved channels and sends the JOINs that were held.
    pub(super) async fn release_joins(&mut self) {
        self.rt.x.join_hold = None;
        self.rt.x.joins_blocked = false;
        self.join_saved_channels().await;
        for m in std::mem::take(&mut self.rt.x.held_joins) {
            self.send(m);
        }
    }

    /// The server hid the host (396 or CHGHOST): joins held because of it
    /// go ahead.
    pub(super) async fn host_now_hidden(&mut self) {
        self.services_hidden_host();
        if self.rt.x.joins_blocked {
            info!(network = %self.net.name, "host hidden: joining the held channels");
            self.say_status("host_hidden_joining", &[]);
            self.release_joins().await;
            self.services_recover_nick();
        } else {
            self.services_release().await;
        }
    }

    /// Keeps the joins held after the wait: the host is still not hidden.
    fn block_joins(&mut self) {
        self.rt.x.join_hold = None;
        self.rt.x.joins_blocked = true;
        let wait = self.join_wait().to_string();
        warn!(network = %self.net.name, "host still not hidden: channels not joined");
        self.say_status("host_not_hidden", &[&wait]);
    }

    /// End of the registration burst (MOTD done): the network name is
    /// known, so log in, set +x and send SILENCE.
    pub(super) async fn on_burst_end(&mut self) {
        self.ignore_sync_silence();
        let Some(cfg) = self.mods.services.clone() else {
            return;
        };
        let svc = self.service(cfg.service);
        if svc == Service::Auto {
            if !cfg.password.is_empty() && !self.rt.x.login_sent {
                self.rt.x.login_sent = true;
                let name = self
                    .isupport
                    .network
                    .clone()
                    .unwrap_or_else(|| self.net.name.clone());
                warn!(network = %self.net.name, "no NickServ on this network: services password not sent");
                self.say_status("services_none", &[&name]);
            }
            self.rt.x.logged_in = true;
            self.services_release().await;
            return;
        }
        let nick = self.current_nick();
        let loc_used = self.services_pass().is_some();
        if !self.rt.x.login_sent && !cfg.password.is_empty() && !loc_used {
            self.rt.x.login_sent = true;
            let acc = cfg.account.clone();
            let pw = cfg.password.clone();
            let line = match svc {
                Service::X => Some((
                    "x@channels.undernet.org".to_string(),
                    format!("LOGIN {acc} {pw}"),
                )),
                Service::Q => Some(("Q@CServe.quakenet.org".into(), format!("AUTH {acc} {pw}"))),
                Service::AuthServ => {
                    // GameSurge wants the full address (so nobody can pose
                    // as AuthServ); AfterNet uses the nick.
                    let gs = self
                        .isupport
                        .network
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case("GameSurge"));
                    let to = if gs {
                        "AuthServ@Services.GameSurge.net"
                    } else {
                        "AuthServ"
                    };
                    Some((to.to_string(), format!("AUTH {acc} {pw}")))
                }
                Service::NickServ if self.net.sasl_username.is_none() => Some((
                    "NickServ".into(),
                    if acc.is_empty() {
                        format!("IDENTIFY {pw}")
                    } else {
                        format!("IDENTIFY {acc} {pw}")
                    },
                )),
                _ => None,
            };
            if let Some((to, text)) = line {
                self.send(Message::cmd("PRIVMSG", &[&to, &text]));
            }
        }
        if cfg.hide_host && matches!(svc, Service::X | Service::Q | Service::AuthServ) && !loc_used
        {
            self.send(Message::cmd("MODE", &[&nick, "+x"]));
        }
        self.services_release().await;
    }

    pub(super) fn services_hidden_host(&mut self) {
        self.rt.x.host_hidden = true;
        // On X, Q and AuthServ a hidden host means the login worked.
        if self.mods.services.as_ref().is_some_and(|c| {
            matches!(
                self.service(c.service),
                Service::X | Service::Q | Service::AuthServ
            )
        }) {
            self.rt.x.logged_in = true;
        }
    }

    /// Joins the saved channels once the login is done (or on timeout).
    pub(super) async fn services_release(&mut self) {
        let Some(until) = self.rt.x.join_hold else {
            return;
        };
        let timeout = Instant::now() >= until;
        if self.services_must_wait() && !timeout {
            return;
        }
        if timeout && self.hide_pending() {
            self.block_joins();
            return;
        }
        self.rt.x.join_hold = None;
        if timeout && self.services_must_wait() {
            let svc = self
                .mods
                .services
                .as_ref()
                .map(|c| format!("{:?}", self.service(c.service)))
                .unwrap_or_default();
            let wait = self
                .mods
                .services
                .as_ref()
                .map_or(0, |c| c.wait)
                .to_string();
            self.say_status("services_timeout", &[&svc, &wait]);
        }
        self.release_joins().await;
        self.services_recover_nick();
    }

    fn services_recover_nick(&mut self) {
        let Some(cfg) = self.mods.services.clone() else {
            return;
        };
        if !cfg.recover_nick || self.is_me(&self.net.nick) || !self.rt.x.logged_in {
            return;
        }
        if self.service(cfg.service) != Service::NickServ {
            return;
        }
        let want = self.net.nick.clone();
        let libera = self
            .isupport
            .network
            .as_deref()
            .is_some_and(|n| n.to_ascii_lowercase().contains("libera"));
        let cmd = if libera {
            format!("REGAIN {want}")
        } else {
            format!("GHOST {want}")
        };
        self.send(Message::cmd("PRIVMSG", &["NickServ", &cmd]));
    }

    /// Looks at notices from services for the login result. Returns true
    /// if the notice was about the login.
    pub(super) async fn services_notice(&mut self, from: &str, text: &str) -> bool {
        let Some(cfg) = self.mods.services.clone() else {
            return false;
        };
        if !self.rt.x.login_sent || self.rt.x.logged_in {
            return false;
        }
        let f = from.to_ascii_lowercase();
        if !matches!(f.as_str(), "x" | "nickserv" | "q" | "authserv") {
            return false;
        }
        let t = strip_codes(text).to_lowercase();
        const OK: &[&str] = &[
            "authentication successful",
            "you are now logged in",
            "you are now identified",
            "password accepted",
            "you are successfully identified",
            "ahora estás identificado",
            "contraseña aceptada",
        ];
        const BAD: &[&str] = &[
            "authentication failed",
            "invalid password",
            "password incorrect",
            "incorrect password",
            "username or password incorrect",
            "is not a registered",
            "isn't registered",
            "contraseña incorrecta",
        ];
        let svc = format!("{:?}", self.service(cfg.service));
        if OK.iter().any(|p| t.contains(p)) {
            self.rt.x.logged_in = true;
            info!(network = %self.net.name, service = %svc, "logged in with services");
            self.services_release().await;
            self.services_recover_nick();
            return true;
        }
        if BAD.iter().any(|p| t.contains(p)) {
            warn!(network = %self.net.name, service = %svc, "services login failed");
            self.say_status("services_failed", &[&svc, &strip_codes(text)]);
            // Do not keep the channels waiting for a login that will not
            // come, unless that would show the real host.
            if self.rt.x.join_hold.is_some() && self.hide_pending() {
                self.block_joins();
            } else if self.rt.x.join_hold.is_some() {
                self.rt.x.join_hold = Some(Instant::now());
                self.rt.x.logged_in = true;
                self.services_release().await;
                self.rt.x.logged_in = false;
            }
            return true;
        }
        false
    }

    // ------------------------------------------------------------ invite

    /// A JOIN failed (471 full, 473 invite only, 474 banned, 475 bad key,
    /// 477 registered only): ask the channel service for help.
    pub(super) fn invite_on_join_error(&mut self, numeric: &str, chan: &str) {
        let Some(cfg) = self.mods.invite else {
            return;
        };
        let cf = self.fold(chan);
        let now = Instant::now();
        let asks = self.rt.x.invite_asks.entry(cf).or_default();
        asks.retain(|t| now.duration_since(*t) < INVITE_EVERY);
        if asks.len() >= INVITE_TRIES {
            return;
        }
        asks.push(now);
        let svc = match self.service(cfg.service) {
            Service::X => Service::X,
            Service::Q => Service::Q,
            // No channel service on this network.
            Service::Auto => return,
            _ => Service::ChanServ,
        };
        let (to, invite, unban) = match svc {
            Service::X => ("X", format!("INVITE {chan}"), format!("UNBAN {chan}")),
            Service::Q => (
                "Q@CServe.quakenet.org",
                format!("INVITE {chan}"),
                format!("UNBANME {chan}"),
            ),
            _ => (
                "ChanServ",
                format!("INVITE {chan}"),
                format!("UNBAN {chan}"),
            ),
        };
        let (cmd, why) = match numeric {
            "474" if cfg.unban => (unban, "ban"),
            "474" => return,
            "471" => (invite, "+l"),
            "473" => (invite, "+i"),
            "475" => (invite, "+k"),
            _ => return,
        };
        self.send(Message::cmd("PRIVMSG", &[to, &cmd]));
        let to_name = to.split('@').next().unwrap_or(to).to_string();
        self.say_status("invite_asking", &[&to_name, chan, why]);
        if numeric == "474" {
            // After an unban, try again a little later.
            self.rt
                .rejoins
                .push((now + Duration::from_secs(4), chan.to_string()));
        }
    }

    /// INVITE to us: join if it is one of our saved channels.
    pub(super) async fn invite_on_invite(&mut self, chan: &str) {
        let Some(cfg) = self.mods.invite else {
            return;
        };
        if !cfg.join_invites || self.channels.contains_key(&self.fold(chan)) {
            return;
        }
        let nid = self.net.id;
        let cf = self.fold(chan);
        let saved = self
            .ctx
            .db
            .call(move |c| db::list_channels(c, nid))
            .await
            .map(|l| l.iter().any(|c| self.fold(&c.name) == cf))
            .unwrap_or(false);
        if saved {
            self.rt.rejoins.push((Instant::now(), chan.to_string()));
        }
    }

    // ----------------------------------------------------------- charset

    /// Marks lines for channels and nicks configured to receive the legacy
    /// encoding.
    pub(super) fn charset_mark(&self, msg: &Message, line: String) -> String {
        let Some(cfg) = &self.mods.charset else {
            return line;
        };
        if cfg.send_legacy.is_empty()
            || !matches!(
                msg.command.as_str(),
                "PRIVMSG" | "NOTICE" | "TOPIC" | "PART" | "KICK"
            )
        {
            return line;
        }
        match msg.param(0) {
            Some(t) if listed(&cfg.send_legacy, t) => cfg.legacy.mark(&line),
            _ => line,
        }
    }

    pub(super) fn charset_legacy(&self) -> crate::util::Legacy {
        self.mods
            .charset
            .as_ref()
            .map(|c| c.legacy)
            .unwrap_or_default()
    }

    // ---------------------------------------------------- detach / flood

    /// True if messages for `chan` must not reach the clients now.
    pub(super) fn is_hidden(&self, chan: &str) -> bool {
        if !self.isupport.is_channel(chan) {
            return false;
        }
        let cf = self.fold(chan);
        if self.rt.x.flood_hidden.contains_key(&cf) {
            return true;
        }
        match &self.mods.detach {
            Some(cfg) => listed(&cfg.channels, chan) && !self.rt.x.attached.contains(&cf),
            None => false,
        }
    }

    /// Channel a server message is about, for hiding detached channels.
    pub(super) fn message_channel<'a>(&self, msg: &'a Message) -> Option<&'a str> {
        let i = match msg.command.as_str() {
            "PRIVMSG" | "NOTICE" | "JOIN" | "PART" | "KICK" | "MODE" | "TOPIC" => 0,
            "332" | "333" | "366" | "324" | "329" | "367" | "368" => 1,
            "353" => 2,
            _ => return None,
        };
        let c = msg.param(i)?;
        let c = self.isupport.strip_statusmsg(c);
        self.isupport.is_channel(c).then_some(c)
    }

    /// Counts channel messages; hides a flooding channel from clients.
    pub(super) fn flood_count(&mut self, chan: &str, last_id: Option<i64>) {
        let Some(cfg) = self.mods.flooddetach else {
            return;
        };
        if self.clients.is_empty() {
            return;
        }
        let cf = self.fold(chan);
        let now = Instant::now();
        if let Some(h) = self.rt.x.flood_hidden.get_mut(&cf) {
            // Still noisy: the calm period starts again.
            h.0 = now;
            return;
        }
        let q = self.rt.x.flood_counts.entry(cf.clone()).or_default();
        q.push_back(now);
        while q
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(cfg.seconds))
        {
            q.pop_front();
        }
        if q.len() as u32 >= cfg.messages {
            q.clear();
            let from = last_id.unwrap_or(0);
            self.rt.x.flood_hidden.insert(cf, (now, from));
            self.say_status(
                "flood_hidden",
                &[chan, &cfg.messages.to_string(), &cfg.seconds.to_string()],
            );
        }
    }

    /// Hidden channels that calmed down come back.
    async fn flood_tick(&mut self) {
        let Some(cfg) = self.mods.flooddetach else {
            if !self.rt.x.flood_hidden.is_empty() {
                let all: Vec<(String, i64)> = self
                    .rt
                    .x
                    .flood_hidden
                    .drain()
                    .map(|(k, (_, id))| (k, id))
                    .collect();
                for (cf, id) in all {
                    self.show_channel(&cf, id).await;
                }
            }
            return;
        };
        let now = Instant::now();
        let calm: Vec<(String, i64)> = self
            .rt
            .x
            .flood_hidden
            .iter()
            .filter(|(_, (t, _))| now.duration_since(*t) >= Duration::from_secs(cfg.calm))
            .map(|(k, (_, id))| (k.clone(), *id))
            .collect();
        for (cf, id) in calm {
            self.rt.x.flood_hidden.remove(&cf);
            if let Some(name) = self.channels.get(&cf).map(|c| c.name.clone()) {
                self.say_status("flood_back", &[&name]);
            }
            self.show_channel(&cf, id).await;
        }
    }

    /// Sends a channel's state to every client again (JOIN, topic, names)
    /// and replays the messages stored since `from_id`.
    pub(super) async fn show_channel(&mut self, cf: &str, from_id: i64) {
        if self.is_hidden_cf(cf) {
            return;
        }
        let Some(name) = self.channels.get(cf).map(|c| c.name.clone()) else {
            return;
        };
        let nid = self.net.id;
        let target = cf.to_string();
        let msgs = self
            .ctx
            .db
            .call(move |c| {
                let f = db::HistoryFilter {
                    target_cf: Some(&target),
                    text: None,
                    sender: None,
                    before_id: None,
                    from_ts: None,
                    to_ts: None,
                    limit: REPLAY,
                };
                db::search_messages(c, nid, &f)
            })
            .await
            .unwrap_or_default();
        let ids: Vec<ClientId> = self.clients.iter().map(|c| c.h.id).collect();
        for id in ids {
            let Some(caps) = self.clients.iter().find(|c| c.h.id == id).map(|c| c.h.caps) else {
                continue;
            };
            let mut lines = self.channel_lines_for(&caps, Some(cf));
            for m in msgs.iter().rev().filter(|m| m.id > from_id) {
                lines.push(render_for(&self.stored_to_message(m, &caps, true), &caps));
            }
            for l in lines {
                self.send_to(id, l);
            }
        }
        debug!(network = %self.net.name, channel = %name, "channel shown again");
    }

    fn is_hidden_cf(&self, cf: &str) -> bool {
        self.channels
            .get(cf)
            .is_some_and(|c| self.is_hidden(&c.name))
    }

    /// A detached channel mentions us or a watched nick speaks: bring it
    /// back for this connection.
    pub(super) async fn detach_check(&mut self, chan: &str, sender_nick: &str, text: &str) {
        let Some(cfg) = self.mods.detach.clone() else {
            return;
        };
        let cf = self.fold(chan);
        if !listed(&cfg.channels, chan) || self.rt.x.attached.contains(&cf) {
            return;
        }
        let hit = (cfg.on_mention && mentions(text, &self.current_nick()))
            || cfg
                .nicks
                .iter()
                .any(|n| self.isupport.casemapping.eq(n, sender_nick));
        if hit && !self.clients.is_empty() {
            self.rt.x.attached.insert(cf.clone());
            self.say_status("detach_shown", &[chan]);
            // Context: the last messages, including the one that woke it.
            self.show_channel(&cf, 0).await;
        }
    }

    /// Client command `DETACH #chan` or `ATTACH #chan`.
    pub(super) async fn detach_command(&mut self, id: ClientId, attach: bool, chan: &str) {
        if chan.is_empty() || !self.isupport.is_channel(chan) {
            let t = self.t("detach_usage", &[]);
            self.reply_status(id, &t);
            return;
        }
        let cf = self.fold(chan);
        let name = self
            .channels
            .get(&cf)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| chan.to_string());
        let n2 = name.clone();
        self.edit_module(id, "detach", move |st| {
            st.enabled = true;
            let list = st
                .config
                .get("channels")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            let mut items: Vec<String> = list
                .lines()
                .filter(|l| !l.eq_ignore_ascii_case(&n2))
                .map(str::to_string)
                .collect();
            if !attach {
                items.push(n2.clone());
            }
            st.config.insert(
                "channels".into(),
                serde_json::Value::String(items.join("\n")),
            );
            Ok(())
        })
        .await;
        if attach {
            self.rt.x.attached.remove(&cf);
            self.say_status("detach_shown", &[&name]);
            self.show_channel(&cf, 0).await;
        } else if self.channels.contains_key(&cf) {
            self.rt.x.attached.remove(&cf);
            let me = self.self_prefix();
            let reason = self.t("detach_reason", &[]);
            let line = format!(":{me} PART {name} :{reason}");
            let ids: Vec<ClientId> = self.clients.iter().map(|c| c.h.id).collect();
            for cid in ids {
                self.send_to(cid, line.clone());
            }
            self.say_status("detach_hidden", &[&name]);
        }
    }

    /// A client asked to join a channel we are in but keep hidden.
    pub(super) async fn detach_on_client_join(&mut self, chan: &str) -> bool {
        let cf = self.fold(chan);
        if !self.channels.contains_key(&cf) || !self.is_hidden(chan) {
            return false;
        }
        self.rt.x.flood_hidden.remove(&cf);
        if let Some(cfg) = &self.mods.detach {
            if listed(&cfg.channels, chan) {
                self.rt.x.attached.insert(cf.clone());
            }
        }
        self.show_channel(&cf, 0).await;
        true
    }

    // --------------------------------------------------------- autocycle

    /// After someone leaves: if we are alone without op, cycle to get it.
    pub(super) fn autocycle_check(&mut self, chan: &str) {
        let Some(list) = self.mods.autocycle.clone() else {
            return;
        };
        if !list.is_empty() && !listed(&list, chan) {
            return;
        }
        let cf = self.fold(chan);
        let Some(ch) = self.channels.get(&cf) else {
            return;
        };
        if ch.members.len() != 1 {
            return;
        }
        let me = self.fold(&self.current_nick());
        let has_op = ch
            .members
            .get(&me)
            .is_some_and(|m| m.symbols.chars().any(|c| "~&@".contains(c)));
        if has_op {
            return;
        }
        // Never cycle a channel that we could not join again.
        let modes = self.rt.x.chan_modes.get(&cf).cloned().unwrap_or_default();
        if modes.contains('i') || modes.contains('k') {
            return;
        }
        let now = Instant::now();
        if self
            .rt
            .x
            .cycled
            .get(&cf)
            .is_some_and(|t| now.duration_since(*t) < CYCLE_EVERY)
        {
            return;
        }
        self.rt.x.cycled.insert(cf, now);
        let name = ch.name.clone();
        self.say_status("autocycle_doing", &[&name]);
        self.send(Message::cmd("PART", &[&name, "cycle"]));
        self.rt.rejoins.push((now + Duration::from_secs(2), name));
    }

    /// Asks for a channel's modes after joining (autocycle needs i/k).
    pub(super) fn autocycle_on_join(&mut self, chan: &str) {
        if self.mods.autocycle.is_none() {
            return;
        }
        self.send(Message::cmd("MODE", &[chan]));
        self.rt.route.push_back(super::mods::Pending {
            client: None,
            replies: &[],
            ends: &["324"],
            target: Some(self.fold(chan)),
            at: Instant::now(),
        });
    }

    /// 324 and MODE: keep the i and k flags of each channel.
    pub(super) fn track_chan_modes(&mut self, chan: &str, modes: &str) {
        let cf = self.fold(chan);
        let entry = self.rt.x.chan_modes.entry(cf).or_default();
        let mut adding = true;
        for c in modes.chars() {
            match c {
                '+' => adding = true,
                '-' => adding = false,
                'i' | 'k' => {
                    entry.retain(|x| x != c);
                    if adding {
                        entry.push(c);
                    }
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------- crypt

    fn crypt_key(&self, target: &str) -> Option<(String, bool)> {
        let cfg = self.mods.crypt.as_ref()?;
        let cf = self.fold(target);
        cfg.keys
            .iter()
            .find(|k| self.fold(&k.target) == cf)
            .map(|k| (k.key.clone(), k.cbc))
            .or_else(|| self.rt.x.keyx_keys.get(&cf).cloned())
    }

    /// Decrypts an encrypted channel topic (TOPIC and 332).
    pub(super) fn crypt_topic(&self, mut msg: Message) -> Message {
        if self.mods.crypt.is_none() {
            return msg;
        }
        let (ci, ti) = match msg.command.as_str() {
            "TOPIC" => (0, 1),
            "332" => (1, 2),
            _ => return msg,
        };
        let (Some(chan), Some(topic)) = (msg.param(ci), msg.param(ti)) else {
            return msg;
        };
        if let Some(plain) = self.crypt_incoming(chan, topic) {
            msg.params[ti] = plain;
        }
        msg
    }

    /// Decrypts an incoming FiSH message; returns the text to show.
    pub(super) fn crypt_incoming(&self, conversation: &str, text: &str) -> Option<String> {
        self.mods.crypt.as_ref()?;
        let (action, body) = match text.strip_prefix("\x01ACTION ") {
            Some(rest) => (true, rest.trim_end_matches('\x01')),
            None => (false, text),
        };
        let payload = body
            .strip_prefix("+OK ")
            .or_else(|| body.strip_prefix("mcps "))?;
        let (key, _) = self.crypt_key(conversation)?;
        let plain = crate::fish::decrypt(&key, payload)?;
        let plain: String = plain
            .chars()
            .filter(|c| !matches!(c, '\r' | '\n' | '\0'))
            .collect();
        let prefix = self
            .mods
            .crypt
            .as_ref()
            .map(|c| c.prefix.clone())
            .unwrap_or_default();
        Some(if action {
            format!("\x01ACTION {prefix}{plain}\x01")
        } else {
            format!("{prefix}{plain}")
        })
    }

    /// Encrypts an outgoing message. None: send it as it is.
    pub(super) fn crypt_outgoing(&self, target: &str, text: &str) -> Option<Vec<String>> {
        self.mods.crypt.as_ref()?;
        let (key, cbc) = self.crypt_key(self.isupport.strip_statusmsg(target))?;
        if text.starts_with("+OK ") {
            return None;
        }
        let (action, body) = match text.strip_prefix("\x01ACTION ") {
            Some(rest) => (true, rest.trim_end_matches('\x01')),
            None if text.starts_with('\x01') => return None,
            None => (false, text),
        };
        let mut out = Vec::new();
        for part in crate::fish::split(body) {
            let enc = crate::fish::encrypt(&key, cbc, &part)?;
            out.push(if action {
                format!("\x01ACTION {enc}\x01")
            } else {
                enc
            });
        }
        Some(out)
    }

    /// DH1080 notices. Returns true if consumed.
    pub(super) fn crypt_keyx_notice(&mut self, from: &str, text: &str) -> bool {
        let Some(cfg) = self.mods.crypt.clone() else {
            return false;
        };
        let Some((init, theirs, cbc)) = crate::fish::parse_dh(text) else {
            return false;
        };
        let cf = self.fold(from);
        if init {
            if !cfg.key_exchange {
                return false;
            }
            // Each INIT costs a 1080-bit exponentiation and a NOTICE: limit
            // how often strangers can ask, and how many keys are kept.
            let now = Instant::now();
            self.rt
                .x
                .keyx_recent
                .retain(|t| now.duration_since(*t) < KEYX_WINDOW);
            if self.rt.x.keyx_recent.len() >= KEYX_PER_WINDOW
                || (self.rt.x.keyx_keys.len() >= KEYX_MAX_KEYS
                    && !self.rt.x.keyx_keys.contains_key(&cf))
            {
                warn!(network = %self.net.name, "DH1080 key exchange ignored (too many)");
                return true;
            }
            self.rt.x.keyx_recent.push_back(now);
            let mine = crate::fish::Dh1080::new();
            match mine.shared_key(&theirs) {
                Some(key) => {
                    let reply = if cbc {
                        format!("DH1080_FINISH {} CBC", mine.public)
                    } else {
                        format!("DH1080_FINISH {}", mine.public)
                    };
                    self.send(Message::cmd("NOTICE", &[from, &reply]));
                    self.rt.x.keyx_keys.insert(cf, (key, cbc));
                    self.say_status("crypt_keyx_done", &[from]);
                }
                None => self.say_status("crypt_keyx_bad", &[from]),
            }
            return true;
        }
        match self.rt.x.keyx_pending.remove(&cf) {
            Some((mine, want_cbc)) => {
                match mine.shared_key(&theirs) {
                    Some(key) => {
                        self.rt.x.keyx_keys.insert(cf, (key, cbc || want_cbc));
                        self.say_status("crypt_keyx_done", &[from]);
                    }
                    None => self.say_status("crypt_keyx_bad", &[from]),
                }
                true
            }
            None => false,
        }
    }

    /// The `*crypt` service.
    pub(super) async fn crypt_command(&mut self, id: ClientId, text: &str) {
        let say = |me: &mut Self, key: &str, args: &[&str]| {
            let t = me.t(key, args);
            me.reply_from(id, "*crypt", &t);
        };
        if self.mods.crypt.is_none() {
            say(self, "crypt_off", &[]);
            return;
        }
        let mut w = text.split_whitespace();
        let cmd = w.next().unwrap_or("").to_ascii_lowercase();
        let target = w.next().unwrap_or("").to_string();
        let key = w.next().unwrap_or("").to_string();
        match cmd.as_str() {
            "setkey" | "key" if !target.is_empty() && !key.is_empty() => {
                let line = format!("{target} {key}");
                if let Err(e) = crate::modules::CryptKey::parse(&line) {
                    say(self, "module_error", &[&e.to_string()]);
                    return;
                }
                self.crypt_edit(id, &target, Some(line)).await;
                say(self, "crypt_set", &[&target]);
            }
            "delkey" | "del" if !target.is_empty() => {
                self.rt.x.keyx_keys.remove(&self.fold(&target));
                self.crypt_edit(id, &target, None).await;
                say(self, "crypt_del", &[&target]);
            }
            "keyx" if !target.is_empty() && !self.isupport.is_channel(&target) => {
                let mine = crate::fish::Dh1080::new();
                let line = format!("DH1080_INIT {} CBC", mine.public);
                self.send(Message::cmd("NOTICE", &[&target, &line]));
                let cf = self.fold(&target);
                self.rt.x.keyx_pending.insert(cf, (mine, true));
                say(self, "crypt_keyx_sent", &[&target]);
            }
            "list" => {
                let cfg = self.mods.crypt.clone().unwrap_or_else(|| unreachable!());
                let mut names: Vec<String> = cfg.keys.iter().map(|k| k.target.clone()).collect();
                names.extend(self.rt.x.keyx_keys.keys().cloned());
                if names.is_empty() {
                    say(self, "crypt_none", &[]);
                } else {
                    say(self, "crypt_list", &[&names.join(", ")]);
                }
            }
            _ => say(self, "crypt_help", &[]),
        }
    }

    /// Adds, replaces or removes one stored key (keys are kept encrypted).
    async fn crypt_edit(&mut self, id: ClientId, target: &str, line: Option<String>) {
        let current: Vec<String> = self
            .mods
            .crypt
            .as_ref()
            .map(|c| {
                c.keys
                    .iter()
                    .filter(|k| !k.target.eq_ignore_ascii_case(target))
                    .map(|k| {
                        let mode = if k.cbc { "cbc:" } else { "" };
                        format!("{} {mode}{}", k.target, k.key)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut all = current;
        if let Some(l) = line {
            all.push(l);
        }
        let plain = all.join("\n");
        self.edit_module(id, "crypt", move |st| {
            let v = if plain.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(plain)
            };
            st.config.insert("keys".into(), v);
            Ok(())
        })
        .await;
    }

    // ------------------------------------------------------------ ignore

    /// True if a message from `prefix` must be dropped.
    pub(super) fn is_ignored(&self, prefix: &str) -> bool {
        match &self.mods.ignore {
            Some(cfg) if prefix.contains('!') => cfg.masks.iter().any(|m| glob_match(m, prefix)),
            _ => false,
        }
    }

    /// Keeps the server's SILENCE list (Undernet and others) in line with
    /// the ignore list, so ignored messages are not even sent to us.
    pub(super) fn ignore_sync_silence(&mut self) {
        let max = self
            .isupport
            .get("SILENCE")
            .and_then(|v| v.parse::<usize>().ok());
        let Some(max) = max else {
            return;
        };
        let want: BTreeSet<String> = match &self.mods.ignore {
            Some(cfg) if cfg.silence => cfg.masks.iter().take(max).cloned().collect(),
            _ => BTreeSet::new(),
        };
        let remove: Vec<String> = self.rt.x.silenced.difference(&want).cloned().collect();
        let add: Vec<String> = want.difference(&self.rt.x.silenced).cloned().collect();
        for m in remove {
            self.send(Message::cmd("SILENCE", &[&format!("-{m}")]));
        }
        for m in add {
            self.send(Message::cmd("SILENCE", &[&format!("+{m}")]));
        }
        self.rt.x.silenced = want;
    }

    /// The `*ignore` service.
    pub(super) async fn ignore_command(&mut self, id: ClientId, text: &str) {
        let mut w = text.split_whitespace();
        let cmd = w.next().unwrap_or("").to_ascii_lowercase();
        let arg = w.next().unwrap_or("").to_string();
        let say = |me: &mut Self, key: &str, args: &[&str]| {
            let t = me.t(key, args);
            me.reply_from(id, "*ignore", &t);
        };
        match cmd.as_str() {
            "add" | "del" | "remove" if !arg.is_empty() => {
                let mask = crate::modules::normalise_mask(&arg);
                let add = cmd == "add";
                let m2 = mask.clone();
                self.edit_module(id, "ignore", move |st| {
                    st.enabled = true;
                    let cur = st
                        .config
                        .get("masks")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let mut items: Vec<String> = cur
                        .lines()
                        .filter(|l| crate::modules::normalise_mask(l) != m2)
                        .map(str::to_string)
                        .collect();
                    if add {
                        items.push(m2.clone());
                    }
                    st.config
                        .insert("masks".into(), serde_json::Value::String(items.join("\n")));
                    Ok(())
                })
                .await;
                self.ignore_sync_silence();
                say(
                    self,
                    if add {
                        "ignore_added"
                    } else {
                        "ignore_removed"
                    },
                    &[&mask],
                );
            }
            "list" => {
                let masks = self
                    .mods
                    .ignore
                    .as_ref()
                    .map(|c| c.masks.clone())
                    .unwrap_or_default();
                if masks.is_empty() {
                    say(self, "ignore_none", &[]);
                } else {
                    say(self, "ignore_list", &[&masks.join(", ")]);
                }
            }
            _ => say(self, "ignore_help", &[]),
        }
    }

    // ---------------------------------------------------------- schedule

    async fn schedule_tick(&mut self) {
        if self.mods.schedule.is_empty() || self.phase != Phase::Connected {
            return;
        }
        let now_ms = db::now_ms();
        let (_, _, _, h, mi, _, wd) = self.zone.local(now_ms);
        let minute_key = now_ms / 60_000;
        let now = Instant::now();
        let mut due = Vec::new();
        for (i, t) in self.mods.schedule.iter().enumerate() {
            match t.when {
                When::At { days, hour, minute } => {
                    let today = days & (1 << (wd.saturating_sub(1))) != 0;
                    if today
                        && hour == h
                        && minute == mi
                        && self.rt.x.task_minute.get(&i) != Some(&minute_key)
                    {
                        due.push(i);
                    }
                }
                When::Every { minutes } => {
                    let last = *self.rt.x.task_last.entry(i).or_insert(now);
                    if now.duration_since(last) >= Duration::from_secs(minutes as u64 * 60) {
                        due.push(i);
                    }
                }
            }
        }
        for i in due {
            self.rt.x.task_minute.insert(i, minute_key);
            self.rt.x.task_last.insert(i, now);
            let Some(cmd) = self.mods.schedule.get(i).map(|t| t.command.clone()) else {
                continue;
            };
            match crate::perform::to_irc_lines(&cmd, &self.current_nick()) {
                Ok(lines) => {
                    for l in lines {
                        if let Ok(m) = Message::parse(&l) {
                            self.send(m);
                        }
                    }
                    info!(network = %self.net.name, task = i + 1, "ran a scheduled task");
                }
                Err(e) => warn!(network = %self.net.name, error = %e, "invalid scheduled task"),
            }
        }
    }

    /// Timers of the v1.0 modules.
    pub(super) async fn extra_tick(&mut self) {
        if self.rt.x.join_hold.is_some() {
            self.services_release().await;
        }
        self.flood_tick().await;
        self.schedule_tick().await;
    }

    pub(super) fn extra_next_timer(&self) -> Option<Instant> {
        self.rt.x.join_hold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_formatting() {
        assert_eq!(strip_codes("\x0304,12rojo\x03 normal"), "rojo normal");
        assert_eq!(strip_codes("\x02negrita\x02 \x1fsub\x1f"), "negrita sub");
        assert_eq!(strip_codes("\x035,texto"), ",texto");
        assert_eq!(strip_codes("\x03123"), "3");
        assert_eq!(strip_codes("\x04FF0000hex\x0f"), "hex");
        assert_eq!(strip_codes("\x01ACTION baila\x01"), "\x01ACTION baila\x01");
    }

    #[test]
    fn lists() {
        let l = vec!["#a".to_string(), "#b-*".to_string()];
        assert!(listed(&l, "#A") && listed(&l, "#b-bots") && !listed(&l, "#c"));
        assert!(listed(&["*".to_string()], "#x"));
    }
}
