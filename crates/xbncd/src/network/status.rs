//! `*status`, `*notes` and `*watch`: managing xBNC from the IRC client.

use super::status_more;
use super::*;
use crate::modules::{self, Kind, ModState};
use serde_json::{Map, Value};

impl NetActor {
    pub(super) fn say(&mut self, id: ClientId, key: &str, args: &[&str]) {
        let text = self.t(key, args);
        self.reply_status(id, &text);
    }

    pub(super) fn reply_from(&mut self, id: ClientId, from: &str, text: &str) {
        let line = format!(
            ":{from}!xbnc@{} PRIVMSG {} :{text}",
            self.host(),
            self.current_nick()
        );
        self.send_to(id, line);
    }

    pub(super) async fn status_command(&mut self, id: ClientId, text: &str) {
        let text = text.trim();
        let (word, rest) = match text.split_once(char::is_whitespace) {
            Some((w, r)) => (w, r.trim()),
            None => (if text.is_empty() { "help" } else { text }, ""),
        };
        let args: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
        let Some(c) = status_more::find(word) else {
            let t = tr!(self, "st.unknown_command_type_help_to", word);
            self.reply_status(id, &t);
            return;
        };
        if !self.may(id, c).await {
            return;
        }
        let cmd = c.name.to_string();
        match c.name {
            "help" => self.status_help(id, args.first().map(String::as_str)).await,
            "uptime" => self.cmd_uptime(id),
            "showmotd" => self.cmd_showmotd(id),
            "addnetwork" => self.cmd_addnetwork(id, &args).await,
            "delnetwork" => self.cmd_delnetwork(id, &args).await,
            "servers" => self.cmd_servers(id).await,
            "addserver" => self.cmd_addserver(id, &args).await,
            "delserver" => self.cmd_delserver(id, &args).await,
            "addfingerprint" => self.cmd_fingerprint(id, true, &args).await,
            "delfingerprint" => self.cmd_fingerprint(id, false, &args).await,
            "fingerprints" => self.cmd_fingerprints(id).await,
            "showbindhost" => self.cmd_showbindhost(id).await,
            "setbindhost" | "setuserbindhost" => {
                let Some(ip) = args.first() else {
                    self.usage(id, c);
                    return;
                };
                self.cmd_bindhost(id, c.name == "setuserbindhost", Some(ip))
                    .await;
            }
            "clearbindhost" => self.cmd_bindhost(id, false, None).await,
            "clearuserbindhost" => self.cmd_bindhost(id, true, None).await,
            "nicks" => self.cmd_nicks(id, args.first().map(String::as_str)),
            "topics" => self.cmd_topics(id),
            "enablechan" => self.cmd_enablechan(id, true, &args).await,
            "joinnow" => self.cmd_joinnow(id).await,
            "disablechan" => self.cmd_enablechan(id, false, &args).await,
            "attach" => self.cmd_attach(id, true, &args).await,
            "detach" => self.cmd_attach(id, false, &args).await,
            "movechan" => self.cmd_movechan(id, false, &args).await,
            "swapchans" => self.cmd_movechan(id, true, &args).await,
            "playbuffer" => self.cmd_playbuffer(id, &args).await,
            "clearbuffer" => self.cmd_clearbuffer(id, &args).await,
            "clearallbuffers" => self.cmd_clearall(id, None).await,
            "clearallchannelbuffers" => self.cmd_clearall(id, Some(true)).await,
            "clearallquerybuffers" => self.cmd_clearall(id, Some(false)).await,
            "setbuffer" => self.cmd_setbuffer(id, &args).await,
            "availmods" => self.cmd_availmods(id).await,
            "reloadmod" => self.cmd_reloadmod(id, &args).await,
            "users" => self.cmd_users(id, false).await,
            "allnetworks" => self.cmd_users(id, true).await,
            "traffic" => self.cmd_traffic(id),
            "broadcast" => self.cmd_broadcast(id, rest).await,
            "movenetwork" => self.cmd_movenetwork(id, &args).await,
            "rehash" => self.cmd_rehash(id),
            "listports" => self.cmd_listports(id),
            "saveconfig" => {
                let t = self.t("st.nothing_to_save_xbnc_saves", &[]);
                self.reply_status(id, &t);
            }
            "setmotd" | "addmotd" | "clearmotd" => self.cmd_motd(id, c.name, rest).await,
            "restart" => self.cmd_stop(id, true, rest).await,
            "shutdown" => self.cmd_stop(id, false, rest).await,
            "status" => {
                let state = match self.phase {
                    Phase::Connected => self.t("phase_connected", &[]),
                    Phase::Registering => self.t("phase_registering", &[]),
                    Phase::Disconnected => self.t("phase_disconnected", &[]),
                };
                let tls = self.t(if self.net.tls { "on" } else { "off" }, &[]);
                let line = self.t(
                    "status_line",
                    &[
                        &self.net.name,
                        &state,
                        &self.net.host,
                        &self.net.port.to_string(),
                        &tls,
                        &self.current_nick(),
                        &self.channels.len().to_string(),
                        &self.clients.len().to_string(),
                    ],
                );
                self.reply_status(id, &line);
                if let Some(e) = self.last_error.clone() {
                    self.say(id, "last_error", &[&e]);
                }
                if !self.caps_acked.is_empty() {
                    let mut caps: Vec<&String> = self.caps_acked.iter().collect();
                    caps.sort();
                    let caps = caps.into_iter().cloned().collect::<Vec<_>>().join(" ");
                    self.say(id, "server_caps", &[&caps]);
                }
            }
            "networks" => {
                let uid = self.net.user_id;
                let nets = self
                    .ctx
                    .db
                    .call(move |c| db::list_networks(c, uid))
                    .await
                    .unwrap_or_default();
                self.say(id, "networks_title", &[]);
                for n in nets {
                    let state = if n.id == self.net.id {
                        match self.phase {
                            Phase::Connected => self.t("phase_connected", &[]),
                            Phase::Registering => self.t("phase_registering", &[]),
                            Phase::Disconnected => self.t("phase_disconnected", &[]),
                        }
                    } else {
                        match crate::server::network_status(&self.ctx, n.id).await {
                            Some(s) => self.state_label(s.state),
                            None if !n.enabled => self.t("off", &[]),
                            None => self.t("phase_disconnected", &[]),
                        }
                    };
                    let label = format!("{} ({}:{})", n.name, n.host, n.port);
                    self.say(id, "networks_line", &[&label, &state]);
                }
            }
            "channels" => self.cmd_channels(id).await,
            "clients" if !args.is_empty() => {
                if self.owner_role().await.can(crate::rbac::Perm::ManageUsers) {
                    self.cmd_clients_of(id, &args[0]).await;
                } else {
                    let t = self.t("st.only_an_administrator_can_see", &[]);
                    self.reply_status(id, &t);
                }
            }
            "clients" => {
                let names: Vec<String> = self.clients.iter().map(|c| c.h.name.clone()).collect();
                self.say(
                    id,
                    "clients_list",
                    &[&names.len().to_string(), &names.join(", ")],
                );
            }
            "jump" => {
                if !self.pick_server(id, args.first().map(String::as_str)).await {
                    return;
                }
                self.say(id, "reconnecting", &[]);
                self.close_upstream(Some(super::BRAND));
                self.backoff = BACKOFF_MIN;
                self.next_connect = Some(Instant::now());
            }
            "disconnect" => {
                self.close_upstream(Some(super::BRAND));
                self.next_connect = None;
                self.say(id, "disconnected_cmd", &[]);
            }
            "connect" => {
                if self.phase == Phase::Disconnected {
                    self.backoff = BACKOFF_MIN;
                    self.next_connect = Some(Instant::now());
                    self.say(id, "connecting_cmd", &[]);
                } else {
                    self.say(id, "already_connected", &[]);
                }
            }
            "version" => self.reply_status(id, super::BRAND_SITE),
            "modules" => self.cmd_modules(id).await,
            "enable" | "disable" => {
                let Some(m) = args.first() else {
                    self.say(id, "usage", &[&format!("{cmd} <module>")]);
                    return;
                };
                let on = cmd == "enable";
                self.edit_module(id, m, |st| {
                    st.enabled = on;
                    Ok(())
                })
                .await;
            }
            "get" => {
                let Some(m) = args.first() else {
                    self.say(id, "usage", &["get <module>"]);
                    return;
                };
                self.cmd_get(id, m).await;
            }
            "set" => {
                if args.len() < 2 {
                    self.say(id, "usage", &["set <module> <setting> <value>"]);
                    return;
                }
                let key = args[1].clone();
                let value = args[2..].join(" ");
                self.edit_module(id, &args[0], move |st| {
                    st.config.insert(key, Value::String(value));
                    Ok(())
                })
                .await;
            }
            "addline" => {
                if args.len() < 3 {
                    self.say(id, "usage", &["addline <module> <setting> <text>"]);
                    return;
                }
                let key = args[1].clone();
                let line = args[2..].join(" ");
                let secret = is_secret(&args[0], &key);
                self.edit_module(id, &args[0], move |st| {
                    if secret {
                        anyhow::bail!("{key} is secret: replace it with `set` or in the panel");
                    }
                    let cur = st.config.get(&key).and_then(Value::as_str).unwrap_or("");
                    let next = if cur.is_empty() {
                        line
                    } else {
                        format!("{cur}\n{line}")
                    };
                    st.config.insert(key, Value::String(next));
                    Ok(())
                })
                .await;
            }
            "dellines" => {
                if args.len() < 2 {
                    self.say(id, "usage", &["dellines <module> <setting>"]);
                    return;
                }
                let key = args[1].clone();
                let secret = is_secret(&args[0], &key);
                self.edit_module(id, &args[0], move |st| {
                    let empty = if secret {
                        Value::Null
                    } else {
                        Value::String(String::new())
                    };
                    st.config.insert(key, empty);
                    Ok(())
                })
                .await;
            }
            "setnick" => {
                let Some(n) = args.first().cloned() else {
                    self.say(id, "usage", &["setnick <nick>"]);
                    return;
                };
                if !self.can_edit {
                    self.say(id, "permission_denied", &[]);
                    return;
                }
                let nid = self.net.id;
                let upd = db::NetworkUpdate {
                    nick: Some(n.clone()),
                    ..Default::default()
                };
                match self
                    .ctx
                    .db
                    .call(move |c| db::update_network(c, nid, &upd))
                    .await
                {
                    Ok(Some(net)) => {
                        self.net = net;
                        self.rt.keepnick_paused = false;
                        self.say(id, "nick_saved", &[&n]);
                        self.keepnick_try();
                    }
                    Ok(None) => {}
                    Err(e) => self.say(id, "module_error", &[&e.to_string()]),
                }
            }
            "language" => {
                let codes = crate::i18n::codes();
                let Some(lang) = args.first().and_then(|a| crate::i18n::Lang::parse(a)) else {
                    self.say(id, "language_unknown", &[&codes.join(", ")]);
                    return;
                };
                let uid = self.net.user_id;
                let code = lang.code();
                if let Err(e) = self
                    .ctx
                    .db
                    .call(move |c| db::set_language(c, uid, &code))
                    .await
                {
                    self.say(id, "module_error", &[&e.to_string()]);
                    return;
                }
                // Every network of this user picks up the new language.
                for h in self.ctx.registry.handles() {
                    if h.user_id == uid && h.network_id != self.net.id {
                        let _ = h.tx.try_send(NetCmd::Reload);
                    }
                }
                self.lang = lang;
                self.say(id, "language_set", &[]);
            }
            "seen" => {
                let uid = self.net.user_id;
                let seen = self
                    .ctx
                    .db
                    .call(move |c| db::get_user_by_id(c, uid))
                    .await
                    .ok()
                    .flatten()
                    .and_then(|u| u.last_seen_at);
                let when = match seen {
                    Some(ms) => status_more::human_time(ms),
                    None => self.t("seen_never", &[]),
                };
                self.say(id, "seen_now", &[&when]);
            }
            _ => {}
        }
    }

    async fn module_rows(&mut self) -> std::collections::HashMap<String, ModState> {
        let nid = self.net.id;
        self.ctx
            .db
            .call(move |c| db::module_rows(c, nid))
            .await
            .unwrap_or_default()
    }

    async fn cmd_modules(&mut self, id: ClientId) {
        let rows = self.module_rows().await;
        let name = self.net.name.clone();
        self.say(id, "modules_title", &[&name]);
        for (m, st) in modules::effective(&rows) {
            let state = self.t(if st.enabled { "on" } else { "off" }, &[]);
            self.say(id, "module_line", &[&format!("{:<14}", m.id), &state]);
        }
        let plugins = self.plugin_states().await;
        if !plugins.is_empty() {
            self.say(id, "plugins_title", &[]);
            for (pid, on, name) in plugins {
                let state = self.t(if on { "on" } else { "off" }, &[]);
                self.say(id, "plugin_line", &[&format!("{pid:<14}"), &state, &name]);
            }
        }
    }

    async fn cmd_get(&mut self, id: ClientId, module: &str) {
        let Some(info) = modules::info(&module.to_ascii_lowercase()) else {
            if self.ctx.plugins.get(&module.to_ascii_lowercase()).is_some() {
                self.plugin_get(id, &module.to_ascii_lowercase()).await;
                return;
            }
            self.say(id, "module_unknown", &[module]);
            return;
        };
        let rows = self.module_rows().await;
        let Some((_, st)) = modules::effective(&rows)
            .into_iter()
            .find(|(m, _)| m.id == info.id)
        else {
            return;
        };
        let state = self.t(if st.enabled { "on" } else { "off" }, &[]);
        if info.settings.is_empty() {
            self.say(id, "module_settings", &[info.id, &state]);
            self.say(id, "module_no_settings", &[info.id]);
            return;
        }
        self.say(id, "module_settings", &[info.id, &state]);
        for s in info.settings {
            let v = st.config.get(s.key).cloned().unwrap_or(Value::Null);
            if matches!(s.kind, Kind::Secret { .. } | Kind::SecretLines { .. }) {
                let set = v.as_str().is_some_and(|t| !t.is_empty());
                let shown = self.t(if set { "secret_set" } else { "secret_unset" }, &[]);
                self.say(id, "module_setting_line", &[s.key, &shown]);
                continue;
            }
            match (s.kind, v) {
                (Kind::Lines { .. }, Value::String(t)) => {
                    if t.is_empty() {
                        self.say(id, "module_setting_line", &[s.key, "—"]);
                    }
                    for l in t.lines() {
                        self.say(id, "module_setting_line", &[s.key, l]);
                    }
                }
                (_, Value::String(t)) => self.say(id, "module_setting_line", &[s.key, &t]),
                (_, other) => self.say(id, "module_setting_line", &[s.key, &other.to_string()]),
            }
        }
    }

    /// Loads a module's stored state, applies `f`, validates and saves it.
    pub(super) async fn edit_module(
        &mut self,
        id: ClientId,
        module: &str,
        f: impl FnOnce(&mut ModState) -> anyhow::Result<()>,
    ) {
        if !self.can_edit {
            self.say(id, "permission_denied", &[]);
            return;
        }
        let module = module.to_ascii_lowercase();
        if modules::info(&module).is_none() {
            if self.ctx.plugins.get(&module).is_some() {
                self.edit_plugin(id, &module, f).await;
                return;
            }
            self.say(id, "module_unknown", &[&module]);
            return;
        }
        let rows = self.module_rows().await;
        let mut st = modules::effective(&rows)
            .into_iter()
            .find(|(m, _)| m.id == module)
            .map(|(_, s)| s)
            .unwrap_or(ModState {
                enabled: false,
                config: Map::new(),
            });
        let was = st.enabled;
        let old = st.config.clone();
        if let Err(e) = f(&mut st) {
            self.say(id, "module_error", &[&e.to_string()]);
            return;
        }
        if let Err(e) = modules::validate_plain(&module, &st.config) {
            self.say(id, "module_error", &[&e.to_string()]);
            return;
        }
        let secrets = self.ctx.secrets.clone();
        st.config = modules::seal(&module, &st.config, Some(&old), &|p| secrets.encrypt(p));
        match modules::validate(&module, &st.config) {
            Ok(cfg) => st.config = cfg,
            Err(e) => {
                self.say(id, "module_error", &[&e.to_string()]);
                return;
            }
        }
        let nid = self.net.id;
        let saved = st.clone();
        let m2 = module.clone();
        if let Err(e) = self
            .ctx
            .db
            .call(move |c| db::set_module(c, nid, &m2, &saved))
            .await
        {
            self.say(id, "module_error", &[&e.to_string()]);
            return;
        }
        self.load_modules().await;
        if st.enabled != was {
            let key = if st.enabled {
                "module_enabled"
            } else {
                "module_disabled"
            };
            self.say(id, key, &[&module]);
        } else {
            self.say(id, "module_saved", &[]);
        }
        let owner = self.owner.clone();
        let net = self.net.name.clone();
        let db = self.ctx.db.clone();
        tokio::spawn(async move {
            let detail = format!("{owner}/{net} module {module} (from IRC)");
            let _ = db
                .call(move |c| db::audit(c, &owner, "network.module", &detail))
                .await;
        });
    }

    /// `*notes` and `*watch`.
    pub(super) async fn service_command(&mut self, id: ClientId, service: &str, text: &str) {
        let from = service.to_string();
        let say = |me: &mut Self, key: &str, args: &[&str]| {
            let t = me.t(key, args);
            me.reply_from(id, &from, &t);
        };
        if service == "*watch" {
            say(self, "watch_help", &[]);
            return;
        }
        let mut words = text.splitn(3, char::is_whitespace);
        let cmd = words.next().unwrap_or("").to_ascii_lowercase();
        let key = words.next().unwrap_or("").to_string();
        let body = words.next().unwrap_or("").trim().to_string();
        let uid = self.net.user_id;
        let valid_key = !key.is_empty()
            && key.len() <= 32
            && key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
        match cmd.as_str() {
            "list" => {
                let notes = self
                    .ctx
                    .db
                    .call(move |c| db::list_notes(c, uid))
                    .await
                    .unwrap_or_default();
                if notes.is_empty() {
                    say(self, "notes_empty", &[]);
                }
                for (k, t, _) in notes {
                    say(self, "notes_line", &[&k, &t]);
                }
            }
            "get" if valid_key => {
                let notes = self
                    .ctx
                    .db
                    .call(move |c| db::list_notes(c, uid))
                    .await
                    .unwrap_or_default();
                match notes
                    .into_iter()
                    .find(|(k, _, _)| k.eq_ignore_ascii_case(&key))
                {
                    Some((k, t, _)) => say(self, "notes_line", &[&k, &t]),
                    None => say(self, "notes_missing", &[&key]),
                }
            }
            "add" | "set" if valid_key && !body.is_empty() && body.chars().count() <= 400 => {
                let (k, b) = (key.clone(), body.clone());
                match self
                    .ctx
                    .db
                    .call(move |c| db::set_note(c, uid, &k, &b))
                    .await
                {
                    Ok(()) => say(self, "notes_saved", &[&key]),
                    Err(e) => say(self, "module_error", &[&e.to_string()]),
                }
            }
            "del" | "delete" if valid_key => {
                let k = key.clone();
                match self.ctx.db.call(move |c| db::delete_note(c, uid, &k)).await {
                    Ok(true) => say(self, "notes_deleted", &[&key]),
                    _ => say(self, "notes_missing", &[&key]),
                }
            }
            "get" | "add" | "set" | "del" | "delete" => say(self, "notes_invalid", &[]),
            _ => say(self, "notes_help", &[]),
        }
    }
}

/// True if a module setting holds a secret (stored encrypted).
fn is_secret(module: &str, key: &str) -> bool {
    modules::info(&module.to_ascii_lowercase())
        .and_then(|m| m.settings.iter().find(|s| s.key == key))
        .is_some_and(|s| matches!(s.kind, Kind::Secret { .. } | Kind::SecretLines { .. }))
}
