//! The full `*status` command set: the help (grouped, with the command
//! names in bold and `help <command>` with an example), and the commands for
//! servers, certificates, bind hosts, channels, buffers, modules and
//! administration.

use super::*;
use crate::rbac::{Perm, Role};

/// IRC bold.
const B: &str = "\x02";
/// Most backup servers, trusted fingerprints and lines replayed at once.
const MAX_SERVERS: usize = 20;
const MAX_FPS: usize = 10;
const PLAY_DEFAULT: usize = 50;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Level {
    /// Anyone.
    User,
    /// Allowed to change the network (not the `limited` role).
    Edit,
    /// Administrators.
    Admin,
    /// Superadministrators.
    Super,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Group {
    General,
    Networks,
    Servers,
    Channels,
    History,
    Modules,
    Admin,
}

impl Group {
    /// Locale key of the section title.
    fn key(self) -> &'static str {
        match self {
            Group::General => "help.group.general",
            Group::Networks => "help.group.networks",
            Group::Servers => "help.group.servers",
            Group::Channels => "help.group.channels",
            Group::History => "help.group.history",
            Group::Modules => "help.group.modules",
            Group::Admin => "help.group.admin",
        }
    }
}

pub(super) struct Cmd {
    pub name: &'static str,
    /// Other names accepted (the names other bouncers use).
    pub aliases: &'static [&'static str],
    pub group: Group,
    pub level: Level,
}

/// The texts of each command live in the locale files: `help.<name>.args`,
/// `help.<name>.desc`, `help.<name>.detail` (optional) and
/// `help.<name>.example`.
macro_rules! cmd {
    ($name:expr, [$($a:expr),*], $g:ident, $l:ident) => {
        Cmd { name: $name, aliases: &[$($a),*], group: Group::$g, level: Level::$l }
    };
}

pub(super) const COMMANDS: &[Cmd] = &[
    cmd!("help", [], General, User),
    cmd!("version", [], General, User),
    cmd!("uptime", [], General, User),
    cmd!("language", [], General, User),
    cmd!("seen", [], General, User),
    cmd!("showmotd", [], General, User),
    cmd!("status", [], Networks, User),
    cmd!("networks", ["listnetworks"], Networks, User),
    cmd!("addnetwork", [], Networks, User),
    cmd!("delnetwork", [], Networks, User),
    cmd!("setnick", [], Networks, Edit),
    cmd!("connect", [], Networks, Edit),
    cmd!("disconnect", [], Networks, Edit),
    cmd!("jump", [], Networks, Edit),
    cmd!("servers", ["listservers"], Servers, User),
    cmd!("addserver", [], Servers, Edit),
    cmd!("delserver", [], Servers, Edit),
    cmd!(
        "addfingerprint",
        ["addtrustedserverfingerprint"],
        Servers,
        Edit
    ),
    cmd!(
        "delfingerprint",
        ["deltrustedserverfingerprint"],
        Servers,
        Edit
    ),
    cmd!(
        "fingerprints",
        ["listtrustedserverfingerprints"],
        Servers,
        User
    ),
    cmd!("showbindhost", [], Servers, User),
    cmd!("setbindhost", [], Servers, Admin),
    cmd!("clearbindhost", [], Servers, Admin),
    cmd!("setuserbindhost", [], Servers, Admin),
    cmd!("clearuserbindhost", [], Servers, Admin),
    cmd!("channels", ["listchans"], Channels, User),
    cmd!("nicks", ["listnicks"], Channels, User),
    cmd!("topics", [], Channels, User),
    cmd!("enablechan", [], Channels, Edit),
    cmd!("joinnow", [], Channels, User),
    cmd!("disablechan", [], Channels, Edit),
    cmd!("attach", [], Channels, User),
    cmd!("detach", [], Channels, Edit),
    cmd!("movechan", [], Channels, Edit),
    cmd!("swapchans", [], Channels, Edit),
    cmd!("playbuffer", [], History, User),
    cmd!("clearbuffer", ["clearhistory"], History, User),
    cmd!("clearallbuffers", [], History, User),
    cmd!("clearallchannelbuffers", [], History, User),
    cmd!("clearallquerybuffers", [], History, User),
    cmd!("setbuffer", [], History, Edit),
    cmd!("modules", ["listmods"], Modules, User),
    cmd!("availmods", ["listavailmods"], Modules, User),
    cmd!("enable", ["loadmod"], Modules, Edit),
    cmd!("disable", ["unloadmod"], Modules, Edit),
    cmd!("reloadmod", ["updatemod"], Modules, Edit),
    cmd!("get", [], Modules, User),
    cmd!("set", [], Modules, Edit),
    cmd!("addline", [], Modules, Edit),
    cmd!("dellines", [], Modules, Edit),
    cmd!("clients", ["listclients"], General, User),
    cmd!("users", ["listusers"], Admin, Admin),
    cmd!("allnetworks", ["listallusernetworks"], Admin, Admin),
    cmd!("traffic", [], Admin, Admin),
    cmd!("broadcast", [], Admin, Admin),
    cmd!("movenetwork", [], Admin, Admin),
    cmd!("rehash", ["reload"], Admin, Admin),
    cmd!("listports", [], Admin, Admin),
    cmd!("saveconfig", [], Admin, Admin),
    cmd!("setmotd", [], Admin, Super),
    cmd!("addmotd", [], Admin, Super),
    cmd!("clearmotd", [], Admin, Super),
    cmd!("restart", [], Admin, Super),
    cmd!("shutdown", [], Admin, Super),
];

/// The command a word names (its name or one of its aliases).
pub(super) fn find(word: &str) -> Option<&'static Cmd> {
    let w = word.to_ascii_lowercase();
    COMMANDS
        .iter()
        .find(|c| c.name == w || c.aliases.contains(&w.as_str()))
}

/// "2026-11-12 09:47 UTC".
pub(super) fn human_time(ms: i64) -> String {
    let t = crate::util::format_short_time(ms);
    format!("{} UTC", t.get(..16).unwrap_or(&t))
}

/// "1 network" / "2 networks", in the owner's language.
/// "3 days, 4 h 05 min" style duration.
fn format_duration(ms: i64, lang: crate::i18n::Lang) -> String {
    let secs = (ms / 1000).max(0);
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    if d == 0 && h == 0 && m == 0 {
        return crate::i18n::tr(lang, "time.seconds", &[&secs.to_string()]);
    }
    let hm = crate::i18n::tr(
        lang,
        "time.hours_minutes",
        &[&h.to_string(), &format!("{m:02}")],
    );
    if d == 0 {
        return hm;
    }
    let days = crate::i18n::tr(
        lang,
        if d == 1 {
            "unit.day.one"
        } else {
            "unit.day.other"
        },
        &[&d.to_string()],
    );
    format!("{days}, {hm}")
}

/// Visible width of a text (without IRC formatting codes).
fn width(s: &str) -> usize {
    s.chars()
        .filter(|c| !matches!(c, '\x02' | '\x1f' | '\x0f'))
        .count()
}

fn pad(s: &str, w: usize) -> String {
    let n = width(s);
    format!("{s}{}", " ".repeat(w.saturating_sub(n) + 1))
}

/// Channels matching patterns (comma or space separated, * and ?).
fn matches_any(patterns: &[String], name_cf: &str, fold: impl Fn(&str) -> String) -> bool {
    patterns
        .iter()
        .flat_map(|p| p.split(','))
        .filter(|p| !p.is_empty())
        .any(|p| crate::modules::glob_match(&fold(p), name_cf))
}

/// "+6697" → (6697, true); "6667" → (6667, false).
fn parse_port(s: &str) -> Option<(u16, bool)> {
    let (tls, num) = match s.strip_prefix('+') {
        Some(n) => (true, n),
        None => (false, s),
    };
    num.parse::<u16>().ok().filter(|p| *p > 0).map(|p| (p, tls))
}

fn valid_host(host: &str) -> bool {
    db::validate_network_fields("x", host, 1, "x", "x", "x").is_ok()
}

fn human_bytes(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

impl NetActor {
    /// A network state ("connected"...) in the owner's language.
    pub(super) fn state_label(&self, state: &str) -> String {
        match state {
            "connected" => self.t("phase_connected", &[]),
            "connecting" => self.t("phase_registering", &[]),
            "reconnecting" => self.t("st.reconnecting", &[]),
            "disconnected" => self.t("phase_disconnected", &[]),
            other => other.to_string(),
        }
    }

    /// A text of a command (`args`, `desc`, `detail`, `example`); empty
    /// when the locale has none.
    fn cmd_text(&self, c: &Cmd, part: &str) -> String {
        let key = format!("help.{}.{part}", c.name);
        crate::i18n::lookup(self.lang, &key).unwrap_or_default()
    }

    /// "1 network" / "2 networks" (`unit.<x>.one` / `unit.<x>.other`).
    pub(super) fn cnt(&self, n: usize, unit: &str) -> String {
        let form = if n == 1 { "one" } else { "other" };
        let key = format!("{unit}.{form}");
        crate::i18n::tr(self.lang, &key, &[&n.to_string()])
    }

    /// The owner's role, read fresh (it can change while connected).
    pub(super) async fn owner_role(&mut self) -> Role {
        let uid = self.net.user_id;
        self.ctx
            .db
            .call(move |c| db::get_user_by_id(c, uid))
            .await
            .ok()
            .flatten()
            .map(|u| Role::from_db(&u.role))
            .unwrap_or(Role::Limited)
    }

    fn allowed(&self, level: Level, role: Role) -> bool {
        match level {
            Level::User => true,
            Level::Edit => self.can_edit,
            Level::Admin => role.can(Perm::ManageUsers),
            Level::Super => role.can(Perm::EditSettings),
        }
    }

    /// Checks that the owner may run a command; says why not otherwise.
    pub(super) async fn may(&mut self, id: ClientId, c: &Cmd) -> bool {
        let role = self.owner_role().await;
        if self.allowed(c.level, role) {
            return true;
        }
        let msg = match c.level {
            Level::Super => self.t("st.only_a_superadministrator_can_use", &[]),
            Level::Admin => self.t("st.only_an_administrator_can_use", &[]),
            _ => self.t("st.your_account_cannot_change_this", &[]),
        };
        self.reply_status(id, &msg);
        false
    }

    pub(super) fn usage(&mut self, id: ClientId, c: &Cmd) {
        let text = format!(
            "{} {B}{}{B} {}",
            self.t("st.usage", &[]),
            c.name,
            self.cmd_text(c, "args")
        );
        self.reply_status(id, text.trim_end());
    }

    // ------------------------------------------------------------- help

    pub(super) async fn status_help(&mut self, id: ClientId, topic: Option<&str>) {
        let role = self.owner_role().await;
        if let Some(t) = topic {
            if let Some(c) = find(t) {
                self.help_one(id, c, role);
                return;
            }
            // A search: commands whose name or description contains the word.
            let w = t.to_ascii_lowercase();
            let hits: Vec<&Cmd> = COMMANDS
                .iter()
                .filter(|c| self.allowed(c.level, role))
                .filter(|c| {
                    c.name.contains(&w)
                        || c.aliases.iter().any(|a| a.contains(&w))
                        || self.cmd_text(c, "desc").to_lowercase().contains(&w)
                })
                .collect();
            if hits.is_empty() {
                let msg = tr!(self, "st.no_command_matches_type_help", t);
                self.reply_status(id, &msg);
                return;
            }
            self.help_rows(id, &hits);
            return;
        }
        let title = self.t("st.xbnc_status_commands", &[]);
        self.reply_status(id, &title);
        let tip = self.t("st.write_them_as_msg_status", &[]);
        self.reply_status(id, &tip);
        for g in [
            Group::General,
            Group::Networks,
            Group::Servers,
            Group::Channels,
            Group::History,
            Group::Modules,
            Group::Admin,
        ] {
            let list: Vec<&Cmd> = COMMANDS
                .iter()
                .filter(|c| c.group == g && self.allowed(c.level, role))
                .collect();
            if list.is_empty() {
                continue;
            }
            self.reply_status(id, " ");
            let t = self.t(g.key(), &[]);
            self.reply_status(id, &format!("{B}\x1f{t}\x1f{B}"));
            self.help_rows(id, &list);
        }
        self.reply_status(id, " ");
        let more = self.t("st.also_msg_notes_help_msg", &[]);
        self.reply_status(id, &more);
    }

    fn help_rows(&mut self, id: ClientId, list: &[&Cmd]) {
        let rows: Vec<(String, String)> = list
            .iter()
            .map(|c| {
                let args = self.cmd_text(c, "args");
                let left = if args.is_empty() {
                    format!("{B}{}{B}", c.name)
                } else {
                    format!("{B}{}{B} {args}", c.name)
                };
                (left, self.cmd_text(c, "desc"))
            })
            .collect();
        // Rows wider than this put the description on the next line, so the
        // columns of the group stay narrow and aligned.
        const MAX: usize = 36;
        let w = rows
            .iter()
            .map(|(l, _)| width(l))
            .filter(|n| *n <= MAX)
            .max()
            .unwrap_or(0);
        for (l, d) in rows {
            if width(&l) > MAX {
                self.reply_status(id, &format!("  {l}"));
                self.reply_status(id, &format!("  {}{d}", " ".repeat(w + 1)));
            } else {
                self.reply_status(id, &format!("  {}{d}", pad(&l, w)));
            }
        }
    }

    fn help_one(&mut self, id: ClientId, c: &Cmd, role: Role) {
        let args = self.cmd_text(c, "args");
        let head = format!("{B}{}{B} {args}", c.name);
        self.reply_status(id, head.trim_end());
        let d = self.cmd_text(c, "desc");
        self.reply_status(id, &format!("  {d}."));
        let detail = self.cmd_text(c, "detail");
        if !detail.is_empty() {
            self.reply_status(id, &format!("  {detail}"));
        }
        let ex = self.t("st.example", &[]);
        let example = self.cmd_text(c, "example");
        self.reply_status(id, &format!("  {ex} /msg *status {example}"));
        if !c.aliases.is_empty() {
            let also = self.t("st.also_accepted", &[]);
            self.reply_status(id, &format!("  {also} {}", c.aliases.join(", ")));
        }
        if !self.allowed(c.level, role) {
            let who = match c.level {
                Level::Super => self.t("st.only_for_superadministrators", &[]),
                Level::Admin => self.t("st.only_for_administrators", &[]),
                _ => self.t("st.your_account_cannot_use_it", &[]),
            };
            self.reply_status(id, &format!("  {who}"));
        }
    }

    // ------------------------------------------------------------ general

    pub(super) fn cmd_uptime(&mut self, id: ClientId) {
        let now = db::now_ms();
        let up = format_duration(now - self.ctx.started_at, self.lang);
        let since = human_time(self.ctx.started_at);
        let mut text = tr!(self, "st.xbnc_has_been_running_for", up, since);
        if let Some(t) = self.connected_since {
            let net = format_duration(now - t, self.lang);
            text.push(' ');
            text.push_str(&tr!(self, "st.connected_to_for", self.net.name, net));
        }
        self.reply_status(id, &text);
    }

    pub(super) fn cmd_showmotd(&mut self, id: ClientId) {
        let lines = self.ctx.server_motd();
        if lines.is_empty() {
            let t = self.t("st.this_server_has_no_message", &[]);
            self.reply_status(id, &t);
            return;
        }
        for l in lines {
            self.reply_status(id, &l);
        }
    }

    // ----------------------------------------------------------- networks

    pub(super) async fn cmd_addnetwork(&mut self, id: ClientId, args: &[String]) {
        let c = find("addnetwork").expect("command");
        if !self.owner_role().await.can(Perm::ManageOwnNetworks) {
            let t = self.t("st.your_account_cannot_add_networks", &[]);
            self.reply_status(id, &t);
            return;
        }
        let (Some(name), Some(host)) = (args.first(), args.get(1)) else {
            self.usage(id, c);
            return;
        };
        let (port, tls) = match args.get(2) {
            None => (6697, true),
            Some(p) => match parse_port(p) {
                Some(v) => v,
                None => {
                    self.usage(id, c);
                    return;
                }
            },
        };
        let nn = db::NewNetwork {
            name: name.clone(),
            host: host.clone(),
            port,
            tls,
            nick: self.net.nick.clone(),
            username: self.net.username.clone(),
            realname: self.net.realname.clone(),
            server_password_enc: None,
            sasl_username: None,
            sasl_password_enc: None,
        };
        let uid = self.net.user_id;
        let default_max = self.ctx.settings.get().limits.max_networks_per_user;
        let owner = self.owner.clone();
        let res = self
            .ctx
            .db
            .call(move |c| {
                let u = db::get_user_by_id(c, uid)?.ok_or_else(|| anyhow::anyhow!("no user"))?;
                let max = u
                    .max_networks
                    .map(|m| m.max(0) as usize)
                    .unwrap_or(default_max);
                let nid = db::create_network(c, uid, &nn, max)?;
                db::audit(
                    c,
                    &owner,
                    "network.create",
                    &format!(
                        "{owner}/{} {}:{} tls={} (IRC)",
                        nn.name, nn.host, nn.port, nn.tls
                    ),
                )?;
                Ok(nid)
            })
            .await;
        match res {
            Ok(_) => {
                self.spawn_sync();
                let login = format!("{}/{}", self.owner, name);
                let onoff = self.t(if tls { "on" } else { "off" }, &[]);
                let t = tr!(
                    self,
                    "st.network_added_tls_it_is",
                    name,
                    host,
                    port,
                    onoff,
                    login
                );
                self.reply_status(id, &t);
            }
            Err(e) => {
                let t = tr!(self, "st.cannot_add_the_network", e);
                self.reply_status(id, &t);
            }
        }
    }

    pub(super) async fn cmd_delnetwork(&mut self, id: ClientId, args: &[String]) {
        let c = find("delnetwork").expect("command");
        if !self.owner_role().await.can(Perm::ManageOwnNetworks) {
            let t = self.t("st.your_account_cannot_delete_networks", &[]);
            self.reply_status(id, &t);
            return;
        }
        let Some(name) = args.first().cloned() else {
            self.usage(id, c);
            return;
        };
        let confirmed = args.get(1).is_some_and(|w| {
            matches!(
                w.to_ascii_lowercase().as_str(),
                "confirm" | "confirmar" | "yes" | "si" | "sí"
            )
        });
        let uid = self.net.user_id;
        let n2 = name.clone();
        let Some(net) = self
            .ctx
            .db
            .call(move |c| db::get_network(c, uid, &n2))
            .await
            .ok()
            .flatten()
        else {
            let t = tr!(self, "st.you_have_no_network_called", name);
            self.reply_status(id, &t);
            return;
        };
        if !confirmed {
            let t = tr!(self, "st.this_deletes_with_all_its", name);
            self.reply_status(id, &t);
            return;
        }
        let nid = net.id;
        let owner = self.owner.clone();
        let res = self
            .ctx
            .db
            .call(move |c| {
                db::delete_network_by_id(c, nid)?;
                db::audit(
                    c,
                    &owner,
                    "network.delete",
                    &format!("{owner}/{name} (IRC)"),
                )
            })
            .await;
        let text = match res {
            Ok(()) => tr!(self, "st.network_deleted", net.name),
            Err(e) => tr!(self, "st.cannot_delete_it", e),
        };
        self.reply_status(id, &text);
        self.spawn_sync();
    }

    /// Applies network changes in the background (this actor may be one of
    /// those reloaded or stopped).
    fn spawn_sync(&self) {
        let ctx = self.ctx.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::server::sync_networks(&ctx).await {
                warn!(error = %e, "cannot apply network changes");
            }
        });
    }

    // ------------------------------------------------------------ servers

    async fn save_opts(&mut self, o: db::NetOptions) -> bool {
        let nid = self.net.id;
        let o2 = o.clone();
        match self
            .ctx
            .db
            .call(move |c| db::set_net_options(c, nid, &o2))
            .await
        {
            Ok(()) => {
                self.opts = o;
                true
            }
            Err(e) => {
                warn!(network = %self.net.name, error = %e, "cannot save network options");
                false
            }
        }
    }

    async fn load_opts(&mut self) -> db::NetOptions {
        let nid = self.net.id;
        let o = self
            .ctx
            .db
            .call(move |c| db::get_net_options(c, nid))
            .await
            .unwrap_or_default();
        self.opts = o.clone();
        o
    }

    /// The network as saved (the main server), not the one in use.
    async fn saved_network(&mut self) -> Option<Network> {
        let nid = self.net.id;
        self.ctx
            .db
            .call(move |c| db::get_network_by_id(c, nid))
            .await
            .ok()
            .flatten()
    }

    pub(super) async fn cmd_servers(&mut self, id: ClientId) {
        let Some(main) = self.saved_network().await else {
            return;
        };
        let o = self.load_opts().await;
        let total = 1 + o.servers.len();
        let current = if self.phase == Phase::Disconnected {
            None
        } else {
            Some(self.server_idx % total)
        };
        let title = tr!(self, "st.servers_of_tried_in_this", main.name);
        self.reply_status(id, &title);
        let mut list = vec![(
            main.host.clone(),
            main.port,
            main.tls,
            main.server_password_enc.is_some(),
        )];
        list.extend(
            o.servers
                .iter()
                .map(|s| (s.host.clone(), s.port, s.tls, s.password_enc.is_some())),
        );
        for (i, (h, p, tls, pw)) in list.into_iter().enumerate() {
            let mut extra = Vec::new();
            extra.push(if i == 0 {
                self.t("st.main", &[])
            } else {
                self.t("st.backup", &[])
            });
            extra.push(if tls {
                "TLS".to_string()
            } else {
                self.t("st.no_tls", &[])
            });
            if pw {
                extra.push(self.t("st.password", &[]));
            }
            let mark = if current == Some(i) {
                format!(" {B}{}{B}", self.t("st.in_use", &[]))
            } else {
                String::new()
            };
            let line = format!(
                "  {}. {h}:{}{p} ({}){mark}",
                i + 1,
                if tls { "+" } else { "" },
                extra.join(", ")
            );
            self.reply_status(id, &line);
        }
    }

    pub(super) async fn cmd_addserver(&mut self, id: ClientId, args: &[String]) {
        let c = find("addserver").expect("command");
        let Some(host) = args.first().cloned() else {
            self.usage(id, c);
            return;
        };
        let Some(main) = self.saved_network().await else {
            return;
        };
        let (port, tls) = match args.get(1) {
            None => (main.port, main.tls),
            Some(p) => match parse_port(p) {
                Some(v) => v,
                None => {
                    self.usage(id, c);
                    return;
                }
            },
        };
        if !valid_host(&host) {
            let t = tr!(self, "st.is_not_a_valid_server", host);
            self.reply_status(id, &t);
            return;
        }
        let mut o = self.load_opts().await;
        let dup = (main.host.eq_ignore_ascii_case(&host) && main.port == port)
            || o.servers
                .iter()
                .any(|s| s.host.eq_ignore_ascii_case(&host) && s.port == port);
        if dup {
            let t = tr!(self, "st.is_already_in_the_list", host, port);
            self.reply_status(id, &t);
            return;
        }
        if o.servers.len() >= MAX_SERVERS {
            let t = self.t("st.this_network_already_has_20", &[]);
            self.reply_status(id, &t);
            return;
        }
        let password_enc = args.get(2).map(|p| self.ctx.secrets.encrypt(p));
        o.servers.push(db::AltServer {
            host: host.clone(),
            port,
            tls,
            password_enc,
        });
        if self.save_opts(o).await {
            let t = tr!(
                self,
                "st.backup_server_added_if_the",
                host,
                if tls { "+" } else { "" },
                port
            );
            self.reply_status(id, &t);
        }
    }

    pub(super) async fn cmd_delserver(&mut self, id: ClientId, args: &[String]) {
        let c = find("delserver").expect("command");
        let Some(host) = args.first().cloned() else {
            self.usage(id, c);
            return;
        };
        let port: Option<u16> = args.get(1).and_then(|p| parse_port(p)).map(|(p, _)| p);
        let Some(main) = self.saved_network().await else {
            return;
        };
        let mut o = self.load_opts().await;
        let hit = |h: &str, p: u16| h.eq_ignore_ascii_case(&host) && port.is_none_or(|x| x == p);
        let before = o.servers.len();
        o.servers.retain(|s| !hit(&s.host, s.port));
        if o.servers.len() != before {
            self.save_opts(o).await;
            let t = tr!(self, "st.server_removed", host);
            self.reply_status(id, &t);
            return;
        }
        if !hit(&main.host, main.port) {
            let t = tr!(self, "st.is_not_in_the_list", host);
            self.reply_status(id, &t);
            return;
        }
        if o.servers.is_empty() {
            let t = self.t("st.it_is_the_only_server", &[]);
            self.reply_status(id, &t);
            return;
        }
        // The first backup becomes the main server.
        let next = o.servers.remove(0);
        let nid = self.net.id;
        let upd = db::NetworkUpdate {
            host: Some(next.host.clone()),
            port: Some(next.port),
            tls: Some(next.tls),
            server_password_enc: Some(next.password_enc.clone()),
            ..Default::default()
        };
        let o2 = o.clone();
        let res = self
            .ctx
            .db
            .call(move |c| {
                db::update_network(c, nid, &upd)?;
                db::set_net_options(c, nid, &o2)
            })
            .await;
        if res.is_ok() {
            self.opts = o;
            self.server_idx = 0;
            let t = tr!(
                self,
                "st.main_server_removed_is_now",
                host,
                next.host,
                next.port
            );
            self.reply_status(id, &t);
        }
    }

    /// `jump [server]`: picks the next server, or the one named.
    pub(super) async fn pick_server(&mut self, id: ClientId, wanted: Option<&str>) -> bool {
        let o = self.load_opts().await;
        let Some(main) = self.saved_network().await else {
            return false;
        };
        let total = 1 + o.servers.len();
        match wanted {
            None => {
                if total > 1 && self.phase != Phase::Disconnected {
                    self.server_idx = (self.server_idx % total) + 1;
                }
                true
            }
            Some(w) => {
                let (host, port) = match w.rsplit_once(':') {
                    Some((h, p)) => (h, parse_port(p).map(|x| x.0)),
                    None => (w, None),
                };
                let mut list = vec![(main.host.clone(), main.port)];
                list.extend(o.servers.iter().map(|s| (s.host.clone(), s.port)));
                match list
                    .iter()
                    .position(|(h, p)| h.eq_ignore_ascii_case(host) && port.is_none_or(|x| x == *p))
                {
                    Some(i) => {
                        self.server_idx = i;
                        true
                    }
                    None => {
                        let t = tr!(self, "st.is_not_a_server_of", w);
                        self.reply_status(id, &t);
                        false
                    }
                }
            }
        }
    }

    pub(super) async fn cmd_fingerprint(&mut self, id: ClientId, add: bool, args: &[String]) {
        let c = find(if add {
            "addfingerprint"
        } else {
            "delfingerprint"
        })
        .expect("command");
        let Some(fp) = args.first().and_then(|a| {
            crate::certs::normalize_sha256(&args[1..].iter().fold(a.clone(), |s, x| s + x))
        }) else {
            self.usage(id, c);
            return;
        };
        let mut o = self.load_opts().await;
        let pretty = crate::certs::colon_hex(&fp);
        if add {
            if o.trusted_fps.contains(&fp) {
                let t = self.t("st.that_certificate_is_already_trusted", &[]);
                self.reply_status(id, &t);
                return;
            }
            if o.trusted_fps.len() >= MAX_FPS {
                let t = self.t("st.this_network_already_has_10", &[]);
                self.reply_status(id, &t);
                return;
            }
            o.trusted_fps.push(fp);
        } else {
            let before = o.trusted_fps.len();
            o.trusted_fps.retain(|x| *x != fp);
            if o.trusted_fps.len() == before {
                let t = self.t("st.that_certificate_was_not_trusted", &[]);
                self.reply_status(id, &t);
                return;
            }
        }
        if self.save_opts(o).await {
            let nid = self.net.id;
            let owner = self.owner.clone();
            let action = if add {
                "network.fingerprint_add"
            } else {
                "network.fingerprint_del"
            };
            let detail = format!("{}/{} {pretty}", self.owner, self.net.name);
            let _ = self
                .ctx
                .db
                .call(move |c| db::audit(c, &owner, action, &detail))
                .await;
            let _ = nid;
            let t = if add {
                tr!(self, "st.certificate_trusted_it_applies_on", pretty)
            } else {
                tr!(self, "st.certificate_removed", pretty)
            };
            self.reply_status(id, &t);
        }
    }

    pub(super) async fn cmd_fingerprints(&mut self, id: ClientId) {
        let o = self.load_opts().await;
        if o.trusted_fps.is_empty() {
            let t = self.t("st.no_trusted_certificates_every_server", &[]);
            self.reply_status(id, &t);
            return;
        }
        let t = self.t("st.trusted_server_certificates_sha_256", &[]);
        self.reply_status(id, &t);
        for fp in o.trusted_fps {
            self.reply_status(id, &format!("  {}", crate::certs::colon_hex(&fp)));
        }
    }

    pub(super) async fn cmd_showbindhost(&mut self, id: ClientId) {
        let o = self.load_opts().await;
        let uid = self.net.user_id;
        let user = self
            .ctx
            .db
            .call(move |c| db::get_user_bind_host(c, uid))
            .await
            .unwrap_or_default();
        let auto = self.t("st.automatic", &[]);
        let net = if o.bind_host.is_empty() {
            auto.clone()
        } else {
            o.bind_host.clone()
        };
        let usr = if user.is_empty() {
            auto.clone()
        } else {
            user.clone()
        };
        let used = self
            .upstream_local
            .map(|i| i.to_string())
            .unwrap_or_else(|| "—".into());
        let t = tr!(self, "st.local_address_this_network_your", net, usr, used);
        self.reply_status(id, &t);
    }

    pub(super) async fn cmd_bindhost(&mut self, id: ClientId, user: bool, value: Option<&str>) {
        let name = match (user, value.is_some()) {
            (false, true) => "setbindhost",
            (false, false) => "clearbindhost",
            (true, true) => "setuserbindhost",
            (true, false) => "clearuserbindhost",
        };
        let c = find(name).expect("command");
        let ip = match value {
            None => String::new(),
            Some(v) => match v.parse::<std::net::IpAddr>() {
                Ok(ip) if std::net::UdpSocket::bind((ip, 0)).is_ok() => ip.to_string(),
                Ok(ip) => {
                    let t = tr!(self, "st.is_not_an_address_of", ip);
                    self.reply_status(id, &t);
                    return;
                }
                Err(_) => {
                    self.usage(id, c);
                    return;
                }
            },
        };
        let ok = if user {
            let uid = self.net.user_id;
            let ip2 = ip.clone();
            self.ctx
                .db
                .call(move |c| db::set_user_bind_host(c, uid, &ip2))
                .await
                .is_ok()
        } else {
            let mut o = self.load_opts().await;
            o.bind_host = ip.clone();
            self.save_opts(o).await
        };
        if !ok {
            return;
        }
        let owner = self.owner.clone();
        let detail = format!(
            "{}/{} {}",
            self.owner,
            self.net.name,
            if ip.is_empty() { "-" } else { &ip }
        );
        let _ = self
            .ctx
            .db
            .call(move |c| db::audit(c, &owner, "network.bind_host", &detail))
            .await;
        let t = if ip.is_empty() {
            self.t("st.local_address_removed_xbnc_chooses", &[])
        } else {
            tr!(self, "st.local_address_saved_it_applies", ip)
        };
        self.reply_status(id, &t);
    }

    // ----------------------------------------------------------- channels

    pub(super) async fn cmd_channels(&mut self, id: ClientId) {
        let nid = self.net.id;
        let saved = self
            .ctx
            .db
            .call(move |c| db::list_channels(c, nid))
            .await
            .unwrap_or_default();
        let mut rows: Vec<(String, String)> = Vec::new();
        let mut seen = HashSet::new();
        for ch in &saved {
            let cf = self.fold(&ch.name);
            seen.insert(cf.clone());
            rows.push((ch.name.clone(), self.chan_state(&cf, Some(ch))));
        }
        let mut extra: Vec<String> = self
            .channels
            .keys()
            .filter(|cf| !seen.contains(*cf))
            .cloned()
            .collect();
        extra.sort();
        for cf in extra {
            let name = self.channels[&cf].name.clone();
            rows.push((name, self.chan_state(&cf, None)));
        }
        if rows.is_empty() {
            self.say(id, "no_channels", &[]);
            return;
        }
        let t = tr!(self, "st.channels_of_in_join_order", self.net.name);
        self.reply_status(id, &t);
        let w = rows
            .iter()
            .map(|(n, _)| width(n))
            .max()
            .unwrap_or(0)
            .min(30);
        for (i, (n, st)) in rows.into_iter().enumerate() {
            self.reply_status(
                id,
                &format!("  {:>2}. {}{st}", i + 1, pad(&format!("{B}{n}{B}"), w)),
            );
        }
    }

    fn chan_state(&self, cf: &str, saved: Option<&db::Channel>) -> String {
        let mut parts = Vec::new();
        match self.channels.get(cf) {
            Some(c) => {
                let ops = c
                    .members
                    .values()
                    .filter(|m| m.symbols.contains(['@', '~', '&']))
                    .count();
                parts.push(tr!(self, "st.users_op", c.members.len(), ops));
                if let Some(me) = c.members.get(&self.fold(&self.current_nick())) {
                    if !me.symbols.is_empty() {
                        parts.push(tr!(self, "st.you", me.symbols));
                    }
                }
                if self.is_hidden(&c.name) {
                    parts.push(self.t("st.detached", &[]));
                }
            }
            None => parts.push(self.t("st.not_joined", &[])),
        }
        match saved {
            Some(s) if s.disabled => parts.push(self.t("st.disabled", &[])),
            Some(_) => parts.push(self.t("st.saved", &[])),
            None => parts.push(self.t("st.not_saved", &[])),
        }
        parts.join(", ")
    }

    pub(super) fn cmd_nicks(&mut self, id: ClientId, chan: Option<&str>) {
        let Some(chan) = chan else {
            self.usage(id, find("nicks").expect("command"));
            return;
        };
        let cf = self.fold(chan);
        let Some(c) = self.channels.get(&cf) else {
            let t = tr!(self, "st.you_are_not_in", chan);
            self.reply_status(id, &t);
            return;
        };
        let mut list: Vec<(usize, String)> = c
            .members
            .values()
            .map(|m| {
                let rank = "~&@%+"
                    .find(m.symbols.chars().next().unwrap_or(' '))
                    .unwrap_or(9);
                (rank, format!("{}{}", m.symbols, m.nick))
            })
            .collect();
        list.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
        });
        let name = c.name.clone();
        let n = list.len();
        let t = tr!(self, "st.users", name, n);
        self.reply_status(id, &t);
        let names: Vec<String> = list.into_iter().map(|(_, s)| s).collect();
        for chunk in names.chunks(15) {
            self.reply_status(id, &format!("  {}", chunk.join(" ")));
        }
    }

    /// Joins the held channels now, even with the real host showing.
    pub(super) async fn cmd_joinnow(&mut self, id: ClientId) {
        if self.phase != Phase::Connected {
            self.say(id, "not_connected", &[]);
            return;
        }
        if !self.joins_held() {
            self.say(id, "st.joinnow_nothing", &[]);
            return;
        }
        info!(network = %self.net.name, "channels joined by hand before the host was hidden");
        self.release_joins().await;
        self.say(id, "st.joinnow_done", &[]);
    }

    pub(super) fn cmd_topics(&mut self, id: ClientId) {
        let mut list: Vec<(String, Option<String>)> = self
            .channels
            .values()
            .map(|c| (c.name.clone(), c.topic.clone()))
            .collect();
        if list.is_empty() {
            self.say(id, "no_channels", &[]);
            return;
        }
        list.sort();
        let none = self.t("st.no_topic", &[]);
        for (n, t) in list {
            let t = t.filter(|t| !t.is_empty()).unwrap_or_else(|| none.clone());
            self.reply_status(id, &format!("{B}{n}{B}: {t}"));
        }
    }

    /// Saved and joined channels matching the patterns, as (name, folded).
    async fn chans_matching(&mut self, patterns: &[String]) -> Vec<(String, String)> {
        let nid = self.net.id;
        let saved = self
            .ctx
            .db
            .call(move |c| db::list_channels(c, nid))
            .await
            .unwrap_or_default();
        let mut out: Vec<(String, String)> = Vec::new();
        let names = saved
            .iter()
            .map(|c| c.name.clone())
            .chain(self.channels.values().map(|c| c.name.clone()));
        for n in names {
            let cf = self.fold(&n);
            if out.iter().any(|(_, x)| *x == cf) {
                continue;
            }
            if matches_any(patterns, &cf, |p| self.fold(p)) {
                out.push((n, cf));
            }
        }
        out
    }

    pub(super) async fn cmd_enablechan(&mut self, id: ClientId, enable: bool, args: &[String]) {
        let c = find(if enable { "enablechan" } else { "disablechan" }).expect("command");
        if args.is_empty() {
            self.usage(id, c);
            return;
        }
        let list = self.chans_matching(args).await;
        let nid = self.net.id;
        let mut done = Vec::new();
        for (name, cf) in list {
            let cf2 = cf.clone();
            let saved = self
                .ctx
                .db
                .call(move |c| db::set_channel_disabled(c, nid, &cf2, !enable))
                .await
                .unwrap_or(false);
            if !saved {
                continue;
            }
            if !enable && self.channels.contains_key(&cf) {
                let reason = self.t("st.channel_disabled", &[]);
                self.send(Message::cmd("PART", &[&name, &reason]));
            }
            done.push(name);
        }
        if enable && !done.is_empty() && self.phase == Phase::Connected {
            self.join_saved_channels().await;
        }
        let t = if done.is_empty() {
            self.t("st.no_saved_channel_matches", &[])
        } else if enable {
            tr!(self, "st.enabled", done.join(", "))
        } else {
            tr!(self, "st.disabled_still_saved", done.join(", "))
        };
        self.reply_status(id, &t);
    }

    pub(super) async fn cmd_attach(&mut self, id: ClientId, attach: bool, args: &[String]) {
        let c = find(if attach { "attach" } else { "detach" }).expect("command");
        if args.is_empty() {
            self.usage(id, c);
            return;
        }
        let list: Vec<(String, String)> = self
            .chans_matching(args)
            .await
            .into_iter()
            .filter(|(_, cf)| self.channels.contains_key(cf))
            .collect();
        if list.is_empty() {
            let t = self.t("st.you_are_not_in_any", &[]);
            self.reply_status(id, &t);
            return;
        }
        for (name, _) in list {
            self.detach_command(id, attach, &name).await;
        }
    }

    pub(super) async fn cmd_movechan(&mut self, id: ClientId, swap: bool, args: &[String]) {
        let c = find(if swap { "swapchans" } else { "movechan" }).expect("command");
        let (Some(a), Some(b)) = (args.first(), args.get(1)) else {
            self.usage(id, c);
            return;
        };
        let nid = self.net.id;
        let saved = self
            .ctx
            .db
            .call(move |c| db::list_channels(c, nid))
            .await
            .unwrap_or_default();
        let mut order: Vec<String> = saved.iter().map(|c| self.fold(&c.name)).collect();
        let Some(ia) = order.iter().position(|x| *x == self.fold(a)) else {
            let t = tr!(self, "st.is_not_a_saved_channel", a);
            self.reply_status(id, &t);
            return;
        };
        if swap {
            let Some(ib) = order.iter().position(|x| *x == self.fold(b)) else {
                let t = tr!(self, "st.is_not_a_saved_channel", b);
                self.reply_status(id, &t);
                return;
            };
            order.swap(ia, ib);
        } else {
            let Some(pos) = b.parse::<usize>().ok().filter(|p| *p >= 1) else {
                self.usage(id, c);
                return;
            };
            let item = order.remove(ia);
            order.insert((pos - 1).min(order.len()), item);
        }
        let o2 = order.clone();
        if self
            .ctx
            .db
            .call(move |c| db::set_channel_order(c, nid, &o2))
            .await
            .is_ok()
        {
            self.cmd_channels(id).await;
        }
    }

    // ------------------------------------------------------------ history

    pub(super) async fn cmd_playbuffer(&mut self, id: ClientId, args: &[String]) {
        let Some(target) = args.first().cloned() else {
            self.usage(id, find("playbuffer").expect("command"));
            return;
        };
        let max = self.ctx.settings.get().history.playback_max.max(1);
        let n = args
            .get(1)
            .and_then(|x| x.parse::<usize>().ok())
            .unwrap_or(PLAY_DEFAULT)
            .clamp(1, max.min(1000));
        let nid = self.net.id;
        let cf = self.fold(&target);
        let msgs = self
            .ctx
            .db
            .call(move |c| {
                let f = db::HistoryFilter {
                    target_cf: Some(&cf),
                    text: None,
                    sender: None,
                    before_id: None,
                    from_ts: None,
                    to_ts: None,
                    limit: n,
                };
                db::search_messages(c, nid, &f)
            })
            .await
            .unwrap_or_default();
        if msgs.is_empty() {
            let t = tr!(self, "st.there_is_no_history_of", target);
            self.reply_status(id, &t);
            return;
        }
        let Some(caps) = self.clients.iter().find(|c| c.h.id == id).map(|c| c.h.caps) else {
            return;
        };
        let t = tr!(self, "st.playing_back_the_last_lines", msgs.len(), target);
        self.reply_status(id, &t);
        for m in msgs.iter().rev() {
            let line = render_for(&self.stored_to_message(m, &caps, true), &caps);
            self.send_to(id, line);
        }
        let t = self.t("st.end_of_the_playback", &[]);
        self.reply_status(id, &t);
    }

    pub(super) async fn cmd_clearbuffer(&mut self, id: ClientId, args: &[String]) {
        if args.is_empty() {
            self.usage(id, find("clearbuffer").expect("command"));
            return;
        }
        let nid = self.net.id;
        let convs = self
            .ctx
            .db
            .call(move |c| db::conversation_list(c, nid))
            .await
            .unwrap_or_default();
        let targets: Vec<String> = convs
            .into_iter()
            .map(|(t, _, _)| t)
            .filter(|t| matches_any(args, &self.fold(t), |p| self.fold(p)))
            .collect();
        if targets.is_empty() {
            let t = self.t("st.no_conversation_matches", &[]);
            self.reply_status(id, &t);
            return;
        }
        let mut total = 0;
        for t in &targets {
            let cf = self.fold(t);
            total += self
                .ctx
                .db
                .call(move |c| db::delete_history(c, nid, Some(&cf)))
                .await
                .unwrap_or(0);
        }
        let t = tr!(
            self,
            "st.from",
            self.cnt(total, "unit.message_deleted"),
            targets.join(", ")
        );
        self.reply_status(id, &t);
    }

    pub(super) async fn cmd_clearall(&mut self, id: ClientId, which: Option<bool>) {
        let nid = self.net.id;
        let chantypes = self.isupport.chantypes.clone();
        let n = self
            .ctx
            .db
            .call(move |c| match which {
                None => db::delete_history(c, nid, None),
                Some(ch) => db::delete_history_kind(c, nid, ch, &chantypes),
            })
            .await
            .unwrap_or(0);
        let what = match which {
            None => self.t("st.this_network", &[]),
            Some(true) => self.t("st.the_channels", &[]),
            Some(false) => self.t("st.the_private_conversations", &[]),
        };
        let t = tr!(self, "st.from", self.cnt(n, "unit.message_deleted"), what);
        self.reply_status(id, &t);
    }

    pub(super) async fn cmd_setbuffer(&mut self, id: ClientId, args: &[String]) {
        let c = find("setbuffer").expect("command");
        let (Some(chan), Some(v)) = (args.first(), args.get(1)) else {
            self.usage(id, c);
            return;
        };
        let days: Option<i64> = match v.to_lowercase().as_str() {
            "forever" | "siempre" | "0" => Some(0),
            "none" | "nada" | "off" | "-1" => Some(-1),
            "default" | "servidor" | "server" => None,
            n => match n.parse::<i64>() {
                Ok(d) if (1..=36_500).contains(&d) => Some(d),
                _ => {
                    self.usage(id, c);
                    return;
                }
            },
        };
        let nid = self.net.id;
        let cf = self.fold(chan);
        let cf2 = cf.clone();
        let ok = self
            .ctx
            .db
            .call(move |c| db::set_channel_retention(c, nid, &cf2, days))
            .await
            .unwrap_or(false);
        if !ok {
            let t = tr!(self, "st.is_not_a_saved_channel_2", chan);
            self.reply_status(id, &t);
            return;
        }
        if days == Some(-1) {
            self.no_store.insert(cf);
        } else {
            self.no_store.remove(&cf);
        }
        let what = match days {
            Some(0) => self.t("st.kept_forever", &[]),
            Some(-1) => self.t("st.not_stored", &[]),
            None => self.t("st.kept_as_the_server_setting", &[]),
            Some(d) => tr!(self, "st.kept_days", d),
        };
        let t = tr!(self, "st.history_of", chan, what);
        self.reply_status(id, &t);
    }

    // ------------------------------------------------------------ modules

    pub(super) async fn cmd_availmods(&mut self, id: ClientId) {
        let t = self.t("st.modules_built_in", &[]);
        self.reply_status(id, &t);
        let w = crate::modules::MODULES
            .iter()
            .map(|m| m.id.len())
            .max()
            .unwrap_or(0);
        for m in crate::modules::MODULES {
            let mid = m.id;
            let d = crate::i18n::lookup(self.lang, &format!("module.{mid}.summary"))
                .unwrap_or_default();
            let line = format!("  {}{}", pad(&format!("{B}{mid}{B}"), w), d);
            self.reply_status(id, &line);
        }
        let code = self.lang.code().to_string();
        let mut plugins: Vec<(String, String)> = self
            .ctx
            .db
            .call(|c| db::list_plugins(c))
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.enabled)
            .map(|p| {
                let m = p.manifest;
                let d = m
                    .translations
                    .get(&code)
                    .map(|t| t.description.clone())
                    .filter(|d| !d.is_empty())
                    .unwrap_or(m.description);
                (m.id, d)
            })
            .collect();
        if !plugins.is_empty() {
            plugins.sort();
            self.reply_status(id, " ");
            let t = self.t("st.plugins_installed", &[]);
            self.reply_status(id, &t);
            let w = plugins.iter().map(|p| p.0.len()).max().unwrap_or(0);
            for (pid, d) in plugins {
                self.reply_status(id, &format!("  {}{d}", pad(&format!("{B}{pid}{B}"), w)));
            }
        }
        let t = self.t("st.turn_one_on_with_enable", &[]);
        self.reply_status(id, &t);
    }

    pub(super) async fn cmd_reloadmod(&mut self, id: ClientId, args: &[String]) {
        let Some(name) = args.first().map(|s| s.to_ascii_lowercase()) else {
            self.usage(id, find("reloadmod").expect("command"));
            return;
        };
        if crate::modules::info(&name).is_some() {
            self.load_modules().await;
        } else if self.plugins.iter().any(|p| p.id == name) {
            self.plugins.retain(|p| p.id != name);
            self.load_plugins().await;
        } else {
            self.say(id, "module_unknown", &[&name]);
            return;
        }
        let t = tr!(self, "st.restarted", name);
        self.reply_status(id, &t);
    }

    // ------------------------------------------------------------ clients

    pub(super) async fn cmd_clients_of(&mut self, id: ClientId, user: &str) {
        let u = user.to_string();
        let Some(target) = self
            .ctx
            .db
            .call(move |c| db::get_user(c, &u))
            .await
            .ok()
            .flatten()
        else {
            let t = tr!(self, "st.there_is_no_user_called", user);
            self.reply_status(id, &t);
            return;
        };
        let tid = target.id;
        let nets = self
            .ctx
            .db
            .call(move |c| db::list_networks(c, tid))
            .await
            .unwrap_or_default();
        let t = tr!(self, "st.devices_of", user);
        self.reply_status(id, &t);
        let mut any = false;
        for n in nets {
            let st = if n.id == self.net.id {
                Some(self.status_snapshot())
            } else {
                crate::server::network_status(&self.ctx, n.id).await
            };
            for c in st.map(|s| s.clients).unwrap_or_default() {
                any = true;
                let since = human_time(c.connected_at);
                self.reply_status(
                    id,
                    &format!("  {B}{}{B} {} ({}, {since})", n.name, c.name, c.peer),
                );
            }
        }
        if !any {
            let t = self.t("st.no_device_connected", &[]);
            self.reply_status(id, &t);
        }
    }

    // -------------------------------------------------------------- admin

    pub(super) async fn cmd_users(&mut self, id: ClientId, with_networks: bool) {
        let users = self
            .ctx
            .db
            .call(|c| db::list_users(c))
            .await
            .unwrap_or_default();
        let t = if with_networks {
            self.t("st.networks_of_every_user", &[])
        } else {
            tr!(self, "st.text", self.cnt(users.len(), "unit.account"))
        };
        self.reply_status(id, &t);
        for u in users {
            let uid = u.id;
            let nets = self
                .ctx
                .db
                .call(move |c| db::list_networks(c, uid))
                .await
                .unwrap_or_default();
            let mut clients = 0;
            let mut rows = Vec::new();
            for n in &nets {
                let st = if n.id == self.net.id {
                    Some(self.status_snapshot())
                } else {
                    crate::server::network_status(&self.ctx, n.id).await
                };
                let (state, cl, server) = match &st {
                    Some(s) => (self.state_label(s.state), s.clients.len(), s.server.clone()),
                    None if !n.enabled => {
                        (self.t("st.off", &[]), 0, format!("{}:{}", n.host, n.port))
                    }
                    None => (
                        self.t("st.stopped", &[]),
                        0,
                        format!("{}:{}", n.host, n.port),
                    ),
                };
                clients += cl;
                rows.push(format!(
                    "    {B}{}/{}{B} {server}, {state}, {}",
                    u.username,
                    n.name,
                    self.cnt(cl, "unit.client")
                ));
            }
            if with_networks {
                if rows.is_empty() {
                    let t = tr!(self, "st.no_networks", u.username);
                    self.reply_status(id, &t);
                }
                for r in rows {
                    self.reply_status(id, &r);
                }
            } else {
                let state = if !u.enabled {
                    self.t("st.suspended", &[])
                } else if clients > 0 {
                    self.t("st.connected", &[])
                } else {
                    self.t("st.not_connected", &[])
                };
                let line = tr!(
                    self,
                    "st.text_2",
                    u.username,
                    u.role,
                    self.cnt(nets.len(), "unit.network"),
                    self.cnt(clients, "unit.client"),
                    state
                );
                self.reply_status(id, &line);
            }
        }
    }

    pub(super) fn cmd_traffic(&mut self, id: ClientId) {
        let m = self.ctx.metrics.snapshot();
        let up = format_duration(db::now_ms() - self.ctx.started_at, self.lang);
        let t = tr!(self, "st.traffic_in_the_last", up);
        self.reply_status(id, &t);
        let rows = [
            (
                self.t("st.from_irc_servers", &[]),
                human_bytes(m.bytes_from_servers),
            ),
            (
                self.t("st.to_irc_servers", &[]),
                human_bytes(m.bytes_to_servers),
            ),
            (
                self.t("st.from_clients", &[]),
                human_bytes(m.bytes_from_clients),
            ),
            (
                self.t("st.to_clients", &[]),
                human_bytes(m.bytes_to_clients),
            ),
            (
                self.t("st.messages_stored", &[]),
                m.messages_stored.to_string(),
            ),
        ];
        let w = rows.iter().map(|r| width(&r.0)).max().unwrap_or(0);
        for (k, v) in rows {
            self.reply_status(id, &format!("  {}{B}{v}{B}", pad(&k, w)));
        }
    }

    pub(super) async fn cmd_broadcast(&mut self, id: ClientId, text: &str) {
        if text.is_empty() {
            self.usage(id, find("broadcast").expect("command"));
            return;
        }
        let n = self.broadcast_all(text);
        let owner = self.owner.clone();
        let detail = text.to_string();
        let _ = self
            .ctx
            .db
            .call(move |c| db::audit(c, &owner, "broadcast", &detail))
            .await;
        let t = tr!(self, "st.notice_sent_to", self.cnt(n, "unit.network"));
        self.reply_status(id, &t);
    }

    /// Sends an administrator's notice to every running network (this one
    /// included). Returns how many networks got it.
    fn broadcast_all(&mut self, text: &str) -> usize {
        let handles = self.ctx.registry.handles();
        let mut n = 0;
        for h in handles {
            if h.network_id == self.net.id {
                let t = self.t("broadcast_notice", &[text]);
                self.notice_all(&t);
                n += 1;
            } else if h.tx.try_send(NetCmd::Broadcast(text.to_string())).is_ok() {
                n += 1;
            }
        }
        n
    }

    pub(super) async fn cmd_movenetwork(&mut self, id: ClientId, args: &[String]) {
        let c = find("movenetwork").expect("command");
        let (Some(from), Some(net), Some(to)) = (args.first(), args.get(1), args.get(2)) else {
            self.usage(id, c);
            return;
        };
        let new_name = args.get(3).cloned().unwrap_or_else(|| net.clone());
        let (from, net, to) = (from.clone(), net.clone(), to.clone());
        let default_max = self.ctx.settings.get().limits.max_networks_per_user;
        let owner = self.owner.clone();
        let nn = new_name.clone();
        let res = self
            .ctx
            .db
            .call(move |c| {
                let fu = db::get_user(c, &from)?
                    .ok_or_else(|| anyhow::anyhow!("no user called {from}"))?;
                let tu =
                    db::get_user(c, &to)?.ok_or_else(|| anyhow::anyhow!("no user called {to}"))?;
                let n = db::get_network(c, fu.id, &net)?
                    .ok_or_else(|| anyhow::anyhow!("{from} has no network called {net}"))?;
                let max = tu
                    .max_networks
                    .map(|m| m.max(0) as usize)
                    .unwrap_or(default_max);
                db::move_network(c, n.id, tu.id, &nn, max)?;
                db::audit(
                    c,
                    &owner,
                    "network.move",
                    &format!("{from}/{net} -> {to}/{nn}"),
                )?;
                Ok(n.id)
            })
            .await;
        match res {
            Ok(nid) => {
                let t = tr!(self, "st.network_moved_to_it_reconnects", args[2], new_name);
                self.reply_status(id, &t);
                let ctx = self.ctx.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::server::restart_network(&ctx, nid).await {
                        warn!(error = %e, "cannot restart the moved network");
                    }
                });
            }
            Err(e) => {
                let t = tr!(self, "st.cannot_move_it", e);
                self.reply_status(id, &t);
            }
        }
    }

    pub(super) fn cmd_rehash(&mut self, id: ClientId) {
        crate::server::reload_languages(&self.ctx);
        self.spawn_sync();
        let t = self.t("st.users_networks_and_channels_re", &[]);
        self.reply_status(id, &t);
    }

    pub(super) fn cmd_listports(&mut self, id: ClientId) {
        let t = self.t("st.ports_xbnc_listens_on", &[]);
        self.reply_status(id, &t);
        let listen = self.ctx.config.listen.clone();
        for l in listen {
            let kind = if l.tls_cert.is_some() {
                self.t("st.irc_with_tls", &[])
            } else if l.lan_only {
                self.t("st.irc_without_tls_local_network", &[])
            } else {
                self.t("st.irc_without_tls", &[])
            };
            self.reply_status(id, &format!("  {B}{}{B} {kind}", l.address));
        }
        if let Some(w) = self.ctx.config.web.clone() {
            let kind = if w.uses_tls() {
                self.t("st.web_panel_and_api_with", &[])
            } else {
                self.t("st.web_panel_and_api_without", &[])
            };
            self.reply_status(id, &format!("  {B}{}{B} {kind}", w.address));
        }
        let t = self.t("st.to_change_them_etc_xbnc", &[]);
        self.reply_status(id, &t);
    }

    pub(super) async fn cmd_motd(&mut self, id: ClientId, op: &str, text: &str) {
        let mut lines = self.ctx.server_motd();
        match op {
            "setmotd" | "addmotd" => {
                if text.is_empty() {
                    self.usage(id, find(op).expect("command"));
                    return;
                }
                let line: String = text
                    .chars()
                    .filter(|c| !c.is_control() || *c == '\x02')
                    .take(crate::server::MOTD_MAX_LEN)
                    .collect();
                if op == "setmotd" {
                    lines.clear();
                }
                if lines.len() >= crate::server::MOTD_MAX_LINES {
                    let t = self.t("st.the_message_of_the_day", &[]);
                    self.reply_status(id, &t);
                    return;
                }
                lines.push(line);
            }
            _ => lines.clear(),
        }
        let n = lines.len();
        if let Err(e) = self.ctx.set_server_motd(lines).await {
            warn!(error = %e, "cannot save the message of the day");
            return;
        }
        let owner = self.owner.clone();
        let detail = format!("{op} ({n} lines)");
        let _ = self
            .ctx
            .db
            .call(move |c| db::audit(c, &owner, "server.motd", &detail))
            .await;
        let t = if n == 0 {
            self.t("st.message_of_the_day_removed", &[])
        } else {
            tr!(
                self,
                "st.message_of_the_day_saved",
                self.cnt(n, "unit.line")
            )
        };
        self.reply_status(id, &t);
    }

    pub(super) async fn cmd_stop(&mut self, id: ClientId, restart: bool, text: &str) {
        if !text.is_empty() {
            self.broadcast_all(text);
        }
        let owner = self.owner.clone();
        let action = if restart {
            "system.restart"
        } else {
            "system.shutdown"
        };
        let _ = self
            .ctx
            .db
            .call(move |c| db::audit(c, &owner, action, "IRC"))
            .await;
        let t = if restart {
            self.t("st.restarting_xbnc", &[])
        } else {
            self.t("st.stopping_xbnc_start_it_again", &[])
        };
        self.reply_status(id, &t);
        let ctx = self.ctx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(800)).await;
            if restart {
                ctx.restart.notify_one();
            } else {
                ctx.stop.notify_one();
            }
        });
    }
}
