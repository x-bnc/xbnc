//! Native module behaviour inside the network actor. Configuration and
//! validation live in `crate::modules`; this file holds the runtime state
//! and the hooks the actor calls.

use super::*;
use crate::i18n::Lang;
use crate::modules::{glob_match, mentions, NetModules, OpMode};
use std::collections::{BTreeSet, VecDeque};

/// How long a routed request may wait for its replies.
const ROUTE_TIMEOUT: Duration = Duration::from_secs(60);
const KEEPNICK_INTERVAL: Duration = Duration::from_secs(30);
const STICKY_INTERVAL: Duration = Duration::from_secs(60);
const CTCP_WINDOW: Duration = Duration::from_secs(10);
const KICK_WINDOW: Duration = Duration::from_secs(60);
const KICKS_BEFORE_GIVING_UP: usize = 3;
const CHALLENGE_TIMEOUT: Duration = Duration::from_secs(60);

/// A command whose numeric replies go only to the client that sent it.
#[derive(Debug)]
pub(super) struct Pending {
    /// None: a request made by xBNC itself (replies are consumed).
    pub client: Option<ClientId>,
    pub replies: &'static [&'static str],
    pub ends: &'static [&'static str],
    /// Folded target the replies must mention, when known.
    pub target: Option<String>,
    pub at: Instant,
}

#[derive(Debug)]
pub(super) struct Challenge {
    pub chan: String,
    pub mode: OpMode,
    pub challenge: String,
    pub key: String,
    pub at: Instant,
}

/// Runtime state of the modules for one network connection.
#[derive(Debug, Default)]
pub(super) struct ModRuntime {
    /// DCC offers relayed by the dcc module (for RESUME/ACCEPT).
    pub dcc_map: Vec<super::dcc::PortMap>,
    pub keepnick_paused: bool,
    pub keepnick_last: Option<Instant>,
    pub keepnick_attempt: bool,
    pub away_auto: bool,
    /// Nick to go back to when a client attaches (the away module changed it).
    pub away_nick_from: Option<String>,
    pub away_client: bool,
    pub detached_at: Option<Instant>,
    pub missed_private: u32,
    pub missed_mentions: u32,
    pub mention_chans: BTreeSet<String>,
    pub rejoins: Vec<(Instant, String)>,
    pub kicks: HashMap<String, Vec<Instant>>,
    pub autoop_queue: Vec<(Instant, String, String, OpMode)>,
    pub challenges: HashMap<String, Challenge>,
    pub route: VecDeque<Pending>,
    pub ctcp_times: VecDeque<Instant>,
    pub sticky_last: Option<Instant>,
    pub log_cleanup_last: Option<Instant>,
    /// MOTD of the current connection, replayed to clients on attach.
    pub motd: Vec<String>,
    pub motd_building: Vec<String>,
    /// State of the v1.0 modules.
    pub x: super::extra::ExtraRuntime,
}

const WHOIS_REPLIES: &[&str] = &[
    "311", "312", "313", "317", "319", "301", "330", "338", "378", "379", "671", "276", "307",
    "310", "320", "335", "275", "316", "325", "344",
];
const ERR_ENDS: &[&str] = &["401", "402", "403", "406", "431", "461", "481", "263"];

/// Which numerics answer a client command, or None if it is not routed.
fn route_spec(
    cmd: &str,
    params: &[String],
) -> Option<(
    &'static [&'static str],
    &'static [&'static str],
    Option<usize>,
)> {
    // (replies, ends, index of the param holding the target)
    Some(match cmd {
        "WHOIS" => (
            WHOIS_REPLIES,
            &["318", "401", "402", "431", "461"],
            Some(params.len().checked_sub(1)?),
        ),
        "WHOWAS" => (
            &["314", "312", "338", "330"],
            &["369", "406", "431", "461"],
            Some(0),
        ),
        "WHO" => (&["352", "354"], &["315", "461"], None),
        "LIST" => (&["321", "322"], &["323", "461"], None),
        "NAMES" if !params.is_empty() => (&["353"], &["366", "403"], Some(0)),
        "ISON" => (&[], &["303", "461"], None),
        "USERHOST" => (&[], &["302", "461"], None),
        "MOTD" => (&["375", "372"], &["376", "422"], None),
        "MODE" if params.len() == 2 => match params[1].trim_start_matches('+') {
            "b" => (&["367"], &["368", "403", "442", "482"], Some(0)),
            "e" => (&["348"], &["349", "403", "442", "482"], Some(0)),
            "I" => (&["346"], &["347", "403", "442", "482"], Some(0)),
            _ => return None,
        },
        _ => return None,
    })
}

/// Index of the parameter that names the target in a reply numeric.
fn reply_target_index(numeric: &str) -> usize {
    match numeric {
        "353" => 2,
        _ => 1,
    }
}

fn random_challenge() -> String {
    use chacha20poly1305::aead::rand_core::RngCore;
    const A: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut b = [0u8; 32];
    chacha20poly1305::aead::OsRng.fill_bytes(&mut b);
    b.iter().map(|x| A[*x as usize % A.len()] as char).collect()
}

use crate::modules::autoop_response;

impl NetActor {
    pub(super) fn t(&self, key: &str, args: &[&str]) -> String {
        crate::i18n::tr(self.lang, key, args)
    }

    /// Reloads the user's language and the network's module configuration.
    pub(super) async fn load_modules(&mut self) {
        let nid = self.net.id;
        let uid = self.net.user_id;
        let res = self
            .ctx
            .db
            .call(move |c| Ok((db::module_rows(c, nid)?, db::get_user_by_id(c, uid)?)))
            .await;
        match res {
            Ok((rows, user)) => {
                let secrets = self.ctx.secrets.clone();
                self.mods = NetModules::from_rows(&rows, &|enc| {
                    secrets.decrypt(enc).ok().map(|s| s.expose().to_string())
                });
                let legacy = self.charset_legacy();
                if let Some(r) = self.reader.as_mut() {
                    r.decoder_mut().legacy = legacy;
                }
                if self.phase == Phase::Connected && self.burst_done {
                    self.ignore_sync_silence();
                }
                let (ulang, role, tz) = user
                    .map(|u| (u.language, u.role, u.timezone))
                    .unwrap_or_default();
                self.zone = crate::tz::Zone::parse_or_utc(&tz);
                self.lang = self.ctx.settings.get().lang_for(&ulang);
                self.can_edit =
                    crate::rbac::Role::from_db(&role).can(crate::rbac::Perm::ManageOwnNetworks);
            }
            Err(e) => warn!(network = %self.net.name, error = %e, "cannot load modules"),
        }
        self.load_plugins().await;
        if let Ok(chans) = self.ctx.db.call(move |c| db::list_channels(c, nid)).await {
            self.no_store = chans
                .iter()
                .filter(|c| c.retention_days == Some(-1))
                .map(|c| self.fold(&c.name))
                .collect();
        }
    }

    /// Resets per-connection module state (called when connecting).
    pub(super) fn reset_module_state(&mut self) {
        let keep_counts = (
            self.rt.missed_private,
            self.rt.missed_mentions,
            std::mem::take(&mut self.rt.mention_chans),
            self.rt.detached_at,
        );
        self.rt = ModRuntime::default();
        self.rt.missed_private = keep_counts.0;
        self.rt.missed_mentions = keep_counts.1;
        self.rt.mention_chans = keep_counts.2;
        self.rt.detached_at = keep_counts.3;
    }

    /// Earliest module timer, so the actor wakes up in time.
    pub(super) fn next_module_timer(&self) -> Option<Instant> {
        let a = self.rt.rejoins.iter().map(|(t, _)| *t).min();
        let b = self.rt.autoop_queue.iter().map(|(t, ..)| *t).min();
        let c = match (&self.mods.away, self.rt.detached_at) {
            (Some(cfg), Some(t)) if !self.rt.away_auto && self.clients.is_empty() => {
                Some(t + Duration::from_secs(cfg.delay))
            }
            _ => None,
        };
        let d = if self.keepnick_active() {
            Some(
                self.rt
                    .keepnick_last
                    .map_or_else(Instant::now, |t| t + KEEPNICK_INTERVAL),
            )
        } else {
            None
        };
        [a, b, c, d, self.extra_next_timer()]
            .into_iter()
            .flatten()
            .min()
    }

    // ------------------------------------------------------------ ticks

    pub(super) async fn modules_tick(&mut self) {
        let now = Instant::now();
        self.rt
            .route
            .retain(|p| now.duration_since(p.at) < ROUTE_TIMEOUT);
        self.rt
            .challenges
            .retain(|_, c| now.duration_since(c.at) < CHALLENGE_TIMEOUT);
        if self.phase != Phase::Connected {
            return;
        }
        // keepnick: ask the server whether our nick is free.
        if self.keepnick_active()
            && self
                .rt
                .keepnick_last
                .is_none_or(|t| now.duration_since(t) >= KEEPNICK_INTERVAL)
        {
            self.rt.keepnick_last = Some(now);
            let want = self.net.nick.clone();
            self.send(Message::cmd("ISON", &[&want]));
            self.rt.route.push_back(Pending {
                client: None,
                replies: &[],
                ends: &["303"],
                target: None,
                at: now,
            });
        }
        // away: set after the configured delay with no client attached.
        if let Some(cfg) = self.mods.away.clone() {
            if self.clients.is_empty() && !self.rt.away_auto && !self.rt.away_client {
                if let Some(t) = self.rt.detached_at {
                    if now.duration_since(t) >= Duration::from_secs(cfg.delay) {
                        let msg = if cfg.message.is_empty() {
                            self.t("away_default", &[])
                        } else {
                            cfg.message.clone()
                        };
                        self.send(Message::cmd("AWAY", &[&msg]));
                        self.rt.away_auto = true;
                        if !cfg.nick.is_empty() {
                            let current = self.current_nick();
                            let mut away = cfg.nick.replace("{nick}", &current);
                            // Undernet allows 12 characters; follow NICKLEN.
                            if let Some(max) = self
                                .isupport
                                .get("NICKLEN")
                                .and_then(|v| v.parse::<usize>().ok())
                            {
                                away = away.chars().take(max).collect();
                            }
                            let valid = !away.is_empty()
                                && !away.contains([' ', ',', '*', '?', '!', '@', ':', '#']);
                            if valid && !self.is_me(&away) {
                                self.rt.away_nick_from = Some(current);
                                self.send(Message::cmd("NICK", &[&away]));
                            }
                        }
                    }
                }
            }
        }
        // kickrejoin / stickychan timers
        let due: Vec<String> = {
            let (d, keep): (Vec<_>, Vec<_>) =
                self.rt.rejoins.drain(..).partition(|(t, _)| *t <= now);
            self.rt.rejoins = keep;
            d.into_iter().map(|(_, c)| c).collect()
        };
        for chan in due {
            self.rejoin(&chan).await;
        }
        if !self.mods.sticky.is_empty()
            && self
                .rt
                .sticky_last
                .is_none_or(|t| now.duration_since(t) >= STICKY_INTERVAL)
        {
            self.rt.sticky_last = Some(now);
            let missing: Vec<String> = self
                .mods
                .sticky
                .iter()
                .filter(|c| !self.channels.contains_key(&self.fold(c)))
                .cloned()
                .collect();
            for chan in missing {
                self.rejoin(&chan).await;
            }
        }
        // autoop queue
        let due: Vec<(String, String, OpMode)> = {
            let (d, keep): (Vec<_>, Vec<_>) = self
                .rt
                .autoop_queue
                .drain(..)
                .partition(|(t, ..)| *t <= now);
            self.rt.autoop_queue = keep;
            d.into_iter().map(|(_, c, n, m)| (c, n, m)).collect()
        };
        for (chan, nick, mode) in due {
            self.give_mode(&chan, &nick, mode);
        }
        self.extra_tick().await;
        // log retention, once a day
        if let Some(log) = self.mods.log {
            if self
                .rt
                .log_cleanup_last
                .is_none_or(|t| now.duration_since(t) >= Duration::from_secs(86_400))
            {
                self.rt.log_cleanup_last = Some(now);
                if log.retention_days > 0 {
                    self.ctx
                        .logs
                        .cleanup(&self.owner, &self.net.name, log.retention_days);
                }
            }
        }
    }

    async fn rejoin(&mut self, chan: &str) {
        if self.phase != Phase::Connected || self.channels.contains_key(&self.fold(chan)) {
            return;
        }
        let nid = self.net.id;
        let cf = self.fold(chan);
        let key = match self.ctx.db.call(move |c| db::list_channels(c, nid)).await {
            Ok(list) => list
                .into_iter()
                .find(|c| self.isupport.casemapping.fold(&c.name) == cf)
                .and_then(|c| c.key_enc)
                .and_then(|k| self.ctx.secrets.decrypt(&k).ok())
                .map(|k| k.expose().to_string()),
            Err(_) => None,
        };
        match key {
            Some(k) => self.send(Message::cmd("JOIN", &[chan, &k])),
            None => self.send(Message::cmd("JOIN", &[chan])),
        }
    }

    // --------------------------------------------------------- keepnick

    pub(super) fn keepnick_active(&self) -> bool {
        self.mods.keepnick
            && self.phase == Phase::Connected
            && !self.rt.keepnick_paused
            && self.rt.away_nick_from.is_none()
            && !self.is_me(&self.net.nick)
    }

    pub(super) fn keepnick_try(&mut self) {
        if self.keepnick_active() {
            let want = self.net.nick.clone();
            self.rt.keepnick_attempt = true;
            self.send(Message::cmd("NICK", &[&want]));
        }
    }

    /// A user left or changed nick; if they held our nick, take it.
    pub(super) fn keepnick_on_release(&mut self, nick: &str) {
        if self.isupport.casemapping.eq(nick, &self.net.nick) {
            self.keepnick_try();
        }
    }

    // ---------------------------------------------------- route_replies

    /// Called for client commands before they are sent upstream.
    pub(super) fn route_track(&mut self, id: ClientId, msg: &Message) {
        if !self.mods.route_replies {
            return;
        }
        let cmd = msg.command.to_ascii_uppercase();
        if let Some((replies, ends, ti)) = route_spec(&cmd, &msg.params) {
            let target = ti.and_then(|i| msg.params.get(i)).map(|t| {
                // WHOIS server nick / WHOIS nick: the last param is the nick.
                self.fold(t.split(',').next().unwrap_or(t))
            });
            self.rt.route.push_back(Pending {
                client: Some(id),
                replies,
                ends,
                target,
                at: Instant::now(),
            });
        }
    }

    /// Decides where a numeric goes. Returns None to broadcast it as usual,
    /// Some(None) to consume it, Some(Some(id)) for one client.
    pub(super) fn route_numeric(&mut self, msg: &Message) -> Option<Option<ClientId>> {
        let cmd = msg.command.as_str();
        let target = msg
            .params
            .get(reply_target_index(cmd))
            .map(|t| self.fold(t));
        let pos = self.rt.route.iter().position(|p| {
            let known = p.replies.contains(&cmd)
                || p.ends.contains(&cmd)
                || (p.client.is_some() && ERR_ENDS.contains(&cmd));
            let target_ok = match (&p.target, &target) {
                (Some(want), Some(got)) => want == got || cmd == "303" || cmd == "302",
                _ => true,
            };
            known && target_ok
        })?;
        let p = &self.rt.route[pos];
        let client = p.client;
        let ends = p.ends.contains(&cmd) || ERR_ENDS.contains(&cmd);
        if ends {
            self.rt.route.remove(pos);
        }
        Some(client)
    }

    /// Consumes replies to xBNC's own requests (keepnick's ISON).
    pub(super) fn on_internal_reply(&mut self, msg: &Message) {
        if msg.command == "303" {
            let online = msg.param(1).unwrap_or("");
            if online.split_whitespace().next().is_none() {
                self.keepnick_try();
            }
        }
    }

    // ------------------------------------------------------------- away

    pub(super) fn away_on_attach(&mut self, id: ClientId) {
        self.rt.detached_at = None;
        if self.rt.away_auto && self.phase == Phase::Connected {
            self.send(Message::new("AWAY", vec![]));
        }
        if let Some(nick) = self.rt.away_nick_from.take() {
            if self.phase == Phase::Connected && !self.is_me(&nick) {
                self.send(Message::cmd("NICK", &[&nick]));
            }
        }
        self.rt.away_auto = false;
        let summary = self.mods.away.as_ref().is_some_and(|a| a.summary);
        if summary && (self.rt.missed_private > 0 || self.rt.missed_mentions > 0) {
            let chans = if self.rt.mention_chans.is_empty() {
                String::new()
            } else {
                let list: Vec<&str> = self
                    .rt
                    .mention_chans
                    .iter()
                    .take(10)
                    .map(String::as_str)
                    .collect();
                self.t("away_summary_in", &[&list.join(", ")])
            };
            let text = self.t(
                "away_summary",
                &[
                    &self.rt.missed_private.to_string(),
                    &self.rt.missed_mentions.to_string(),
                    &chans,
                ],
            );
            let line = self.status_line("NOTICE", &text);
            self.send_to(id, line);
        }
        self.rt.missed_private = 0;
        self.rt.missed_mentions = 0;
        self.rt.mention_chans.clear();
    }

    pub(super) fn away_on_detach(&mut self) {
        if self.clients.is_empty() {
            self.rt.detached_at = Some(Instant::now());
        }
    }

    /// Counts what arrives while nobody is attached, for the summary.
    pub(super) fn away_count(&mut self, is_chan: bool, conversation: &str, text: &str) {
        if !self.clients.is_empty() || self.mods.away.is_none() {
            return;
        }
        if !is_chan {
            self.rt.missed_private += 1;
        } else if mentions(text, &self.current_nick()) {
            self.rt.missed_mentions += 1;
            if self.rt.mention_chans.len() < 50 {
                self.rt.mention_chans.insert(conversation.to_string());
            }
        }
    }

    // ----------------------------------------------------------- autoop

    fn can_set(&self, chan_cf: &str, mode: OpMode) -> bool {
        let me = self.fold(&self.current_nick());
        let syms = self
            .channels
            .get(chan_cf)
            .and_then(|c| c.members.get(&me))
            .map(|m| m.symbols.clone())
            .unwrap_or_default();
        match mode {
            OpMode::Op | OpMode::HalfOp => syms.chars().any(|c| "~&@".contains(c)),
            OpMode::Voice => syms.chars().any(|c| "~&@%".contains(c)),
        }
    }

    pub(super) fn autoop_on_join(&mut self, chan: &str, prefix: &str) {
        let Some(cfg) = self.mods.autoop.clone() else {
            return;
        };
        let nick = prefix.split('!').next().unwrap_or(prefix).to_string();
        let ccf = self.fold(chan);
        let entry = cfg.entries.iter().find(|e| {
            glob_match(&e.mask, prefix)
                && (e.channels.is_empty() || e.channels.iter().any(|c| self.fold(c) == ccf))
        });
        let Some(e) = entry.cloned() else { return };
        if !self.can_set(&ccf, e.mode) {
            return;
        }
        match e.key {
            Some(key) => {
                let challenge = random_challenge();
                self.send(Message::cmd(
                    "NOTICE",
                    &[
                        &nick,
                        &format!("{} CHALLENGE {challenge}", crate::modules::AUTOOP_TAG),
                    ],
                ));
                self.rt.challenges.insert(
                    self.fold(&nick),
                    Challenge {
                        chan: chan.to_string(),
                        mode: e.mode,
                        challenge,
                        key,
                        at: Instant::now(),
                    },
                );
            }
            None => {
                let at = Instant::now() + Duration::from_secs(cfg.delay);
                self.rt
                    .autoop_queue
                    .push((at, chan.to_string(), nick, e.mode));
            }
        }
    }

    /// Handles `!XBNC-AUTOOP RESPONSE <hmac>`; returns true if the notice was ours.
    pub(super) fn autoop_on_notice(&mut self, nick: &str, text: &str) -> bool {
        let tag = crate::modules::AUTOOP_TAG;
        let Some(resp) = text
            .strip_prefix(tag)
            .and_then(|r| r.strip_prefix(" RESPONSE "))
        else {
            return text.starts_with(tag);
        };
        let ncf = self.fold(nick);
        if let Some(c) = self.rt.challenges.remove(&ncf) {
            if crate::web::ct_eq(resp.trim(), &autoop_response(&c.key, &c.challenge)) {
                self.give_mode(&c.chan, nick, c.mode);
            } else {
                info!(network = %self.net.name, nick, "autoop: wrong challenge response");
            }
        }
        true
    }

    fn give_mode(&mut self, chan: &str, nick: &str, mode: OpMode) {
        let ccf = self.fold(chan);
        let ncf = self.fold(nick);
        let Some(member) = self.channels.get(&ccf).and_then(|c| c.members.get(&ncf)) else {
            return;
        };
        let has = match mode {
            OpMode::Op => member.symbols.chars().any(|c| "~&@".contains(c)),
            OpMode::HalfOp => member.symbols.chars().any(|c| "~&@%".contains(c)),
            OpMode::Voice => !member.symbols.is_empty(),
        };
        if has || !self.can_set(&ccf, mode) {
            return;
        }
        let real = member.nick.clone();
        self.send(Message::cmd(
            "MODE",
            &[chan, &format!("+{}", mode.letter()), &real],
        ));
    }

    // ------------------------------------------- kickrejoin / stickychan

    pub(super) fn on_kicked(&mut self, chan: &str) {
        let ccf = self.fold(chan);
        let sticky = self.mods.sticky.iter().any(|c| self.fold(c) == ccf);
        let Some(delay) = self.mods.kickrejoin.or(sticky.then_some(5)) else {
            return;
        };
        let now = Instant::now();
        let list = self.rt.kicks.entry(ccf).or_default();
        list.retain(|t| now.duration_since(*t) < KICK_WINDOW);
        list.push(now);
        if list.len() > KICKS_BEFORE_GIVING_UP {
            let text = self.t("kickrejoin_giveup", &[chan]);
            self.notice_all(&text);
            return;
        }
        self.rt
            .rejoins
            .push((now + Duration::from_secs(delay), chan.to_string()));
    }

    pub(super) fn is_sticky(&self, chan: &str) -> bool {
        let cf = self.fold(chan);
        self.mods.sticky.iter().any(|c| self.fold(c) == cf)
    }

    // ------------------------------------------------------------- ctcp

    /// Handles a CTCP request addressed to us. Returns true if it must not
    /// be forwarded to clients.
    pub(super) fn ctcp_request(&mut self, from: &str, ctcp: &str, text: &str) -> bool {
        let Some(cfg) = self.mods.ctcp.clone() else {
            return false;
        };
        let upper = ctcp.to_ascii_uppercase();
        if upper == "ACTION" {
            return false;
        }
        let now = Instant::now();
        while self
            .rt
            .ctcp_times
            .front()
            .is_some_and(|t| now.duration_since(*t) > CTCP_WINDOW)
        {
            self.rt.ctcp_times.pop_front();
        }
        self.rt.ctcp_times.push_back(now);
        if self.rt.ctcp_times.len() > cfg.flood_limit as usize {
            debug!(network = %self.net.name, from, "CTCP flood; ignoring");
            return true;
        }
        if cfg.block.contains(&upper) {
            return true;
        }
        // VERSION is always answered by xBNC with the configured text (by
        // default "xBNC 1.0.0 Made by y2k - https://xbnc.org"); the other
        // CTCPs go to the user's client when one is connected.
        if upper != "VERSION" && !self.clients.is_empty() {
            return false; // let the user's client answer
        }
        let reply = match upper.as_str() {
            "VERSION" => super::ctcp_version_reply(&cfg.version),
            "PING" => text
                .trim_matches('\x01')
                .split_once(' ')
                .map(|(_, a)| a.to_string())
                .unwrap_or_default(),
            "TIME" => crate::util::format_server_time(db::now_ms()),
            "CLIENTINFO" => "ACTION CLIENTINFO PING TIME VERSION".into(),
            _ => return false,
        };
        let line = format!("\x01{upper} {reply}\x01");
        self.send(Message::cmd("NOTICE", &[from, &line]));
        true
    }

    // ------------------------------------------------------------ watch

    /// Copies matching messages to the *watch conversation.
    pub(super) async fn watch_check(&mut self, conversation: &str, sender: &str, text: &str) {
        let Some(cfg) = self.mods.watch.clone() else {
            return;
        };
        if conversation.eq_ignore_ascii_case("*watch") {
            return;
        }
        let nick = sender.split('!').next().unwrap_or(sender);
        let conv_cf = self.fold(conversation);
        let hit = (cfg.mentions
            && self.isupport.is_channel(conversation)
            && mentions(text, &self.current_nick()))
            || cfg.patterns.iter().any(|p| {
                glob_match(&p.pattern, text)
                    && p.source
                        .as_ref()
                        .is_none_or(|s| self.fold(s) == conv_cf || glob_match(s, sender))
            });
        if !hit {
            return;
        }
        let line = self.t("watch_hit", &[conversation, nick, text]);
        let host = self.host().to_string();
        let nm = NewMessage {
            network_id: self.net.id,
            target: "*watch".into(),
            target_cf: "*watch".into(),
            ts: db::now_ms(),
            sender: format!("*watch!xbnc@{host}"),
            command: "PRIVMSG".into(),
            text: line.clone(),
            outgoing: false,
            mention: false,
        };
        let id = self.record(nm).await;
        let mut m = Message::new("PRIVMSG", vec![self.current_nick(), line])
            .with_prefix(format!("*watch!xbnc@{host}"));
        m.set_tag("time", Some(format_server_time(db::now_ms())));
        if let Some(i) = id {
            m.set_tag("msgid", Some(format!("x{i}")));
        }
        self.broadcast(&m, id);
    }

    // ----------------------------------------------------------- events

    /// Builds the history rows for a channel event, before the actor's
    /// channel state is updated (QUIT and NICK need the old membership).
    pub(super) fn capture_events(&self, msg: &Message) -> Vec<(String, String)> {
        let Some(cfg) = self.mods.events else {
            return Vec::new();
        };
        let nick = msg.source_nick().unwrap_or("");
        let me = self.is_me(nick);
        let ncf = self.fold(nick);
        let member_chans = || -> Vec<String> {
            self.channels
                .values()
                .filter(|c| c.members.contains_key(&ncf))
                .map(|c| c.name.clone())
                .collect()
        };
        let p = |i: usize| msg.param(i).unwrap_or("").to_string();
        match msg.command.as_str() {
            "JOIN" if cfg.joins && !me => vec![(p(0), String::new())],
            "PART" if cfg.joins && !me => p(0).split(',').map(|c| (c.to_string(), p(1))).collect(),
            "QUIT" if cfg.joins && !me => member_chans().into_iter().map(|c| (c, p(0))).collect(),
            "KICK" if cfg.other => {
                vec![(p(0), format!("{} {}", p(1), p(2)).trim_end().to_string())]
            }
            "NICK" if cfg.other => {
                let mut chans = member_chans();
                if me {
                    chans = self.channels.values().map(|c| c.name.clone()).collect();
                }
                chans.into_iter().map(|c| (c, p(0))).collect()
            }
            "TOPIC" if cfg.other => vec![(p(0), p(1))],
            "MODE" if cfg.other && self.isupport.is_channel(&p(0)) => {
                vec![(p(0), msg.params[1..].join(" "))]
            }
            _ => Vec::new(),
        }
    }

    /// Stores captured events; returns the highest id for cursors.
    pub(super) async fn store_events(
        &mut self,
        msg: &Message,
        rows: Vec<(String, String)>,
    ) -> Option<i64> {
        let ts = msg
            .tag("time")
            .and_then(parse_server_time)
            .unwrap_or_else(db::now_ms);
        let sender = msg.prefix.clone().unwrap_or_default();
        let mut last = None;
        for (chan, text) in rows {
            if chan.is_empty() {
                continue;
            }
            let nm = NewMessage {
                network_id: self.net.id,
                target_cf: self.fold(&chan),
                target: chan,
                ts,
                sender: sender.clone(),
                command: msg.command.clone(),
                text,
                outgoing: false,
                mention: false,
            };
            if let Some(id) = self.record(nm).await {
                last = Some(id);
            }
        }
        last
    }

    /// Text form of a stored event, for clients without event playback.
    pub(super) fn event_text(&self, lang: Lang, command: &str, sender: &str, text: &str) -> String {
        let nick = sender.split('!').next().unwrap_or(sender);
        let reason = |t: &str| {
            if t.is_empty() {
                String::new()
            } else {
                format!(" ({t})")
            }
        };
        let tr = |k: &str, a: &[&str]| crate::i18n::tr(lang, k, a);
        match command {
            "JOIN" => tr("event_join", &[nick]),
            "PART" => tr("event_part", &[nick, &reason(text)]),
            "QUIT" => tr("event_quit", &[nick, &reason(text)]),
            "KICK" => {
                let (victim, why) = text.split_once(' ').unwrap_or((text, ""));
                tr("event_kick", &[nick, victim, &reason(why)])
            }
            "NICK" => tr("event_nick", &[nick, text]),
            "TOPIC" => tr("event_topic", &[nick, text]),
            _ => tr("event_mode", &[nick, text]),
        }
    }

    /// Real IRC message for a stored event (draft/event-playback).
    pub(super) fn event_message(&self, m: &StoredMessage) -> Message {
        let params = match m.command.as_str() {
            "JOIN" => vec![m.target.clone()],
            "PART" | "TOPIC" => vec![m.target.clone(), m.text.clone()],
            "QUIT" | "NICK" => vec![m.text.clone()],
            "KICK" => {
                let (victim, why) = m.text.split_once(' ').unwrap_or((&m.text, ""));
                vec![m.target.clone(), victim.to_string(), why.to_string()]
            }
            _ => {
                let mut v = vec![m.target.clone()];
                v.extend(m.text.split_whitespace().map(str::to_string));
                v
            }
        };
        Message::new(m.command.clone(), params).with_prefix(m.sender.clone())
    }

    // -------------------------------------------------------------- log

    /// Stores a message or event in the history (and the text log when the
    /// log module is on). Returns its id.
    pub(super) async fn record(&mut self, nm: NewMessage) -> Option<i64> {
        if self.no_store.contains(&nm.target_cf) {
            return None;
        }
        if let Some(log) = self.mods.log {
            if log.events || !db::is_event(&nm.command) {
                let line = crate::logs::format_line(&nm.command, &nm.sender, &nm.target, &nm.text);
                let local = nm.ts + self.zone.offset_secs(nm.ts) as i64 * 1000;
                self.ctx
                    .logs
                    .write(&self.owner, &self.net.name, &nm.target_cf, local, line);
            }
        }
        match self.ctx.db.insert_message(nm).await {
            Ok(i) => {
                crate::metrics::Metrics::add(&self.ctx.metrics.messages_stored, 1);
                Some(i)
            }
            Err(e) => {
                warn!(error = %e, "cannot store message");
                None
            }
        }
    }

    // ------------------------------------------------------------- motd

    pub(super) fn on_motd_numeric(&mut self, msg: &Message) {
        match msg.command.as_str() {
            "375" => self.rt.motd_building.clear(),
            "372" => {
                if self.rt.motd_building.len() < 300 {
                    let line = msg.param(1).unwrap_or("").to_string();
                    self.rt.motd_building.push(line);
                }
            }
            "376" => self.rt.motd = std::mem::take(&mut self.rt.motd_building),
            "422" => self.rt.motd.clear(),
            _ => {}
        }
    }

    pub(super) fn motd_lines(&self) -> Vec<String> {
        let host = self.host();
        let nick = self.current_nick();
        if self.mods.block_motd || self.rt.motd.is_empty() {
            let why = if self.mods.block_motd {
                self.t("motd_blocked", &[])
            } else {
                "MOTD File is missing".into()
            };
            return vec![format!(":{host} 422 {nick} :{why}")];
        }
        let mut v = vec![format!(
            ":{host} 375 {nick} :- {} Message of the day -",
            self.net.name
        )];
        for l in &self.rt.motd {
            v.push(format!(":{host} 372 {nick} :{l}"));
        }
        v.push(format!(":{host} 376 {nick} :End of /MOTD command."));
        v
    }

    // ------------------------------------------------------------ alias

    /// Expands a user alias. Returns true if the command was an alias.
    pub(super) fn try_alias(&mut self, id: ClientId, msg: &Message) -> bool {
        let name = msg.command.to_ascii_uppercase();
        let Some(alias) = self.mods.aliases.iter().find(|a| a.name == name).cloned() else {
            return false;
        };
        let args: Vec<&str> = msg
            .params
            .iter()
            .flat_map(|p| p.split_whitespace())
            .collect();
        let text = alias.expand(&args, &self.current_nick());
        match crate::perform::to_irc_lines(&text, &self.current_nick()) {
            Ok(lines) => {
                if self.phase != Phase::Connected {
                    let line = self.status_line("NOTICE", &self.t("not_connected", &[]));
                    self.send_to(id, line);
                    return true;
                }
                for l in lines {
                    if let Ok(m) = Message::parse(&l) {
                        self.send(m);
                    }
                }
            }
            Err(e) => {
                let line = self.status_line("NOTICE", &format!("alias {}: {e}", alias.name));
                self.send_to(id, line);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_specs() {
        let p = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(route_spec("WHOIS", &p(&["bob"])).is_some());
        assert_eq!(route_spec("WHOIS", &p(&["srv", "bob"])).unwrap().2, Some(1));
        assert!(route_spec("NAMES", &p(&[])).is_none());
        assert!(route_spec("MODE", &p(&["#c", "b"])).is_some());
        assert!(route_spec("MODE", &p(&["#c", "+o", "bob"])).is_none());
        assert!(route_spec("PRIVMSG", &p(&["#c", "hi"])).is_none());
    }
}
