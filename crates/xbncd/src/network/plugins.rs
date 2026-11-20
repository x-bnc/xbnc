//! Running WebAssembly plugins inside the network actor: events in,
//! actions out. The sandbox and the ABI are in `crate::plugins`.

use super::*;
use crate::plugins::{Action, PluginInstance, MAX_FAILURES};
use serde_json::{json, Map, Value};

/// How often plugins with the `timer` permission get a `timer` event.
/// `XBNC_PLUGIN_TIMER_SECS` shortens it (for plugin development and tests).
/// `XBNC_PLUGIN_TIMER_SECS` (tests) overrides every plugin's interval.
fn timer_every(manifest_secs: Option<u32>) -> Duration {
    static EVERY: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();
    let forced = *EVERY.get_or_init(|| {
        std::env::var("XBNC_PLUGIN_TIMER_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|s| (1..=60).contains(s))
            .map(Duration::from_secs)
    });
    forced.unwrap_or_else(|| Duration::from_secs(manifest_secs.unwrap_or(60).clamp(5, 60) as u64))
}
/// Messages a plugin may send in answer to one event.
const MAX_SENDS_PER_EVENT: usize = 5;

/// A plugin enabled on this network.
pub(super) struct ActivePlugin {
    pub id: String,
    pub sha256: String,
    /// Taken while a call runs on the blocking pool.
    pub inst: Option<Box<PluginInstance>>,
    pub config: Map<String, Value>,
    pub state: Value,
    pub last_timer: Instant,
    /// Stopped after repeated failures (until the next reload).
    pub stopped: bool,
    /// Domains approved for web requests.
    pub http_approved: Vec<String>,
    pub http_inflight: usize,
    /// Start of the current minute and requests made in it.
    pub http_window: (Instant, u32),
}

impl NetActor {
    /// (Re)loads the plugins enabled on this network, keeping running
    /// instances whose plugin file did not change.
    pub(super) async fn load_plugins(&mut self) {
        let nid = self.net.id;
        let rows = match self
            .ctx
            .db
            .call(move |c| {
                let mut enabled = Vec::new();
                for p in db::list_plugins(c)?.into_iter().filter(|p| p.enabled) {
                    let approved = db::plugin_http_approved(c, &p.id)?;
                    enabled.push((p.id, approved));
                }
                Ok((enabled, db::plugin_config_rows(c, nid)?))
            })
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!(network = %self.net.name, error = %e, "cannot load plugins");
                return;
            }
        };
        let (globally_enabled, rows) = rows;
        let mut next = Vec::new();
        let mut started = Vec::new();
        for (id, enabled, cfg, state) in rows {
            let Some(approved) = globally_enabled
                .iter()
                .find(|(g, _)| g == &id)
                .map(|(_, a)| a.clone())
            else {
                continue;
            };
            if !enabled {
                continue;
            }
            let Some(plugin) = self.ctx.plugins.get(&id) else {
                continue;
            };
            let config = crate::plugins::validate_config(&plugin.manifest.settings, &cfg)
                .unwrap_or_else(|_| {
                    crate::plugins::validate_config(&plugin.manifest.settings, &Map::new())
                        .unwrap_or_default()
                });
            let keep = self
                .plugins
                .iter()
                .position(|p| p.id == id && p.sha256 == plugin.sha256);
            if let Some(i) = keep {
                let mut p = self.plugins.remove(i);
                p.config = config;
                p.stopped = false;
                p.http_approved = approved;
                next.push(p);
                continue;
            }
            let engine = self.ctx.plugins.engine.clone();
            let pl = plugin.clone();
            let db = self.ctx.db.clone();
            let inst = tokio::task::spawn_blocking(move || {
                let mut i = PluginInstance::new(&engine, pl)?;
                i.attach_storage(db, nid);
                Ok::<_, anyhow::Error>(i)
            })
            .await;
            match inst {
                Ok(Ok(i)) => {
                    next.push(ActivePlugin {
                        id: id.clone(),
                        sha256: plugin.sha256.clone(),
                        inst: Some(Box::new(i)),
                        config,
                        state: state.unwrap_or(Value::Null),
                        last_timer: Instant::now(),
                        stopped: false,
                        http_approved: approved,
                        http_inflight: 0,
                        http_window: (Instant::now(), 0),
                    });
                    started.push(id);
                }
                Ok(Err(e)) => {
                    warn!(network = %self.net.name, plugin = %id, error = %e, "plugin failed to start");
                    self.ctx.plugins.logs.push(
                        &id,
                        &self.net.name,
                        "error",
                        &format!("failed to start: {e}"),
                    );
                    let text = self.t("plugin_failed_start", &[&id, &e.to_string()]);
                    self.notice_all(&text);
                }
                Err(e) => warn!(plugin = %id, error = %e, "plugin start panicked"),
            }
        }
        self.plugins = next;
        for id in started {
            let ev = json!({"event": "load"});
            self.plugin_event(Some(&id), ev, None).await;
        }
    }

    /// Delivers an event to one plugin (`only`) or to all of them. `reply_to`
    /// is the conversation a `reply` action answers in.
    pub(super) async fn plugin_event(
        &mut self,
        only: Option<&str>,
        mut event: Value,
        reply_to: Option<String>,
    ) -> usize {
        let mut produced = 0;
        if self.plugins.is_empty() {
            return 0;
        }
        if let Some(o) = event.as_object_mut() {
            o.insert("network".into(), Value::String(self.net.name.clone()));
            o.insert("nick".into(), Value::String(self.current_nick()));
            o.insert(
                "connected".into(),
                Value::Bool(self.phase == Phase::Connected),
            );
            o.insert("clients".into(), Value::from(self.clients.len()));
            // The user's language, for plugins that answer in it.
            o.insert("lang".into(), Value::String(self.lang.code()));
        }
        let event_name = event["event"].as_str().unwrap_or("").to_string();
        for idx in 0..self.plugins.len() {
            let p = &mut self.plugins[idx];
            if p.stopped || only.is_some_and(|o| o != p.id) {
                continue;
            }
            if crate::plugins::PRESENCE_EVENTS.contains(&event_name.as_str())
                && !p
                    .inst
                    .as_ref()
                    .is_some_and(|i| i.plugin.manifest.has("presence"))
            {
                continue;
            }
            if event_name == "timer" {
                let has_timer = p
                    .inst
                    .as_ref()
                    .is_some_and(|i| i.plugin.manifest.has("timer"));
                let secs = p.inst.as_ref().and_then(|i| i.plugin.manifest.timer_secs);
                if !has_timer || p.last_timer.elapsed() < timer_every(secs) {
                    continue;
                }
                p.last_timer = Instant::now();
            }
            let Some(mut inst) = p.inst.take() else {
                continue;
            };
            let mut ev = event.clone();
            ev["config"] = Value::Object(p.config.clone());
            ev["state"] = p.state.clone();
            let res = tokio::task::spawn_blocking(move || {
                let r = inst.call(&ev);
                (inst, r)
            })
            .await;
            let (inst, result) = match res {
                Ok(v) => v,
                Err(e) => {
                    // A panic in the host side of the call: drop the instance.
                    warn!(plugin = %self.plugins[idx].id, error = %e, "plugin call panicked");
                    self.plugins[idx].stopped = true;
                    continue;
                }
            };
            let id = self.plugins[idx].id.clone();
            let failures = inst.failures;
            self.plugins[idx].inst = Some(inst);
            match result {
                Ok(out) => {
                    for l in &out.logs {
                        info!(plugin = %id, network = %self.net.name, "{l}");
                        self.ctx.plugins.logs.push(&id, &self.net.name, "info", l);
                    }
                    for r in &out.refused {
                        warn!(plugin = %id, network = %self.net.name, "action refused: {r}");
                        self.ctx.plugins.logs.push(
                            &id,
                            &self.net.name,
                            "warn",
                            &format!("action refused: {r}"),
                        );
                    }
                    if let Some(st) = out.state {
                        self.plugins[idx].state = st.clone();
                        let (nid, pid) = (self.net.id, id.clone());
                        let _ = self
                            .ctx
                            .db
                            .call(move |c| db::set_plugin_state(c, nid, &pid, &st))
                            .await;
                    }
                    produced += out.actions.len();
                    self.plugin_actions(&id, out.actions, reply_to.as_deref())
                        .await;
                }
                Err(e) => {
                    warn!(plugin = %id, network = %self.net.name, event = %event_name, error = %e, "plugin call failed");
                    self.ctx.plugins.logs.push(
                        &id,
                        &self.net.name,
                        "error",
                        &format!("{event_name}: {e}"),
                    );
                    if failures >= MAX_FAILURES {
                        self.plugins[idx].stopped = true;
                        let text = self.t("plugin_stopped", &[&id, &e.to_string()]);
                        self.notice_all(&text);
                    }
                }
            }
        }
        produced
    }

    async fn plugin_actions(&mut self, id: &str, actions: Vec<Action>, reply_to: Option<&str>) {
        let mut sends = 0;
        for a in actions {
            if let Action::Http {
                id: rid,
                method,
                url,
                body,
                content_type,
            } = a
            {
                self.plugin_http(id, rid, method, url, body, content_type);
                continue;
            }
            let is_send = !matches!(a, Action::Tell(_));
            if is_send {
                if self.phase != Phase::Connected {
                    continue;
                }
                sends += 1;
                if sends > MAX_SENDS_PER_EVENT {
                    warn!(plugin = %id, "too many messages for one event; the rest are dropped");
                    break;
                }
            }
            match a {
                Action::Tell(text) => {
                    let line = format!(":*{id}!xbnc@xbnc PRIVMSG {} :{text}", self.current_nick());
                    let ids: Vec<ClientId> = self.clients.iter().map(|c| c.h.id).collect();
                    for cid in ids {
                        self.send_to(cid, line.clone());
                    }
                }
                Action::Reply(text) => {
                    if let Some(t) = reply_to {
                        let t = t.to_string();
                        self.send_as_user("PRIVMSG", &t, &text).await;
                    }
                }
                Action::Privmsg { target, text } => {
                    self.send_as_user("PRIVMSG", &target, &text).await
                }
                Action::Notice { target, text } => {
                    self.send_as_user("NOTICE", &target, &text).await
                }
                Action::Raw(line) => match Message::parse(&line) {
                    Ok(m) => self.send(m),
                    Err(_) => warn!(plugin = %id, "invalid raw line"),
                },
                Action::Http { .. } => {}
            }
        }
    }

    /// Starts a plugin's web request in the background; the answer comes
    /// back as `NetCmd::PluginHttp`.
    fn plugin_http(
        &mut self,
        plugin: &str,
        rid: String,
        method: String,
        url: String,
        body: Option<String>,
        content_type: Option<String>,
    ) {
        let Some(p) = self.plugins.iter_mut().find(|p| p.id == plugin) else {
            return;
        };
        let refuse =
            |why: &str| json!({"event": "http", "id": rid, "status": 0, "body": "", "error": why});
        let host = crate::http::Url::parse(&url)
            .map(|u| u.host)
            .unwrap_or_default();
        let early = if !crate::plugins::domain_allowed(&p.http_approved, &host) {
            Some(format!("{host} is not an approved domain for this plugin"))
        } else if p.http_inflight >= crate::plugins::HTTP_MAX_INFLIGHT {
            Some("too many requests at once".to_string())
        } else {
            if p.http_window.0.elapsed() >= Duration::from_secs(60) {
                p.http_window = (Instant::now(), 0);
            }
            if p.http_window.1 >= crate::plugins::HTTP_PER_MINUTE {
                Some("too many requests this minute".to_string())
            } else {
                None
            }
        };
        let Some(tx) = self.self_tx.upgrade() else {
            return;
        };
        let plugin = plugin.to_string();
        if let Some(why) = early {
            warn!(plugin = %plugin, network = %self.net.name, "web request refused: {why}");
            self.ctx.plugins.logs.push(
                &plugin,
                &self.net.name,
                "warn",
                &format!("web request refused: {why}"),
            );
            let ev = refuse(&why);
            p.http_inflight += 1;
            tokio::spawn(async move {
                let _ = tx.send(NetCmd::PluginHttp { plugin, event: ev }).await;
            });
            return;
        }
        p.http_inflight += 1;
        p.http_window.1 += 1;
        let tls = self.ctx.tls_client.clone();
        let approved = p.http_approved.clone();
        tokio::spawn(async move {
            let b = body.as_deref().map(|b| {
                (
                    content_type
                        .as_deref()
                        .unwrap_or("text/plain; charset=utf-8"),
                    b.as_bytes(),
                )
            });
            // A GET follows up to 3 redirects, only over https and only to
            // domains approved for this plugin (a moved feed, a Wikipedia
            // article that is a redirect).
            let mut url = url;
            let mut hops = 0;
            let r = loop {
                let r = crate::http::request_public(
                    &method,
                    &url,
                    b,
                    tls.clone(),
                    crate::plugins::HTTP_MAX_BODY,
                    Duration::from_secs(10),
                    &[],
                )
                .await;
                let next = match &r {
                    Ok(resp) if method == "GET" && hops < 3 && matches!(resp.status, 301 | 302 | 303 | 307 | 308) => resp
                        .location
                        .as_deref()
                        .and_then(|l| crate::http::redirect_target(&url, l))
                        .filter(|n| {
                            crate::http::Url::parse(n)
                                .is_some_and(|u| crate::plugins::domain_allowed(&approved, &u.host))
                        }),
                    _ => None,
                };
                match next {
                    Some(n) => {
                        url = n;
                        hops += 1;
                    }
                    None => break r,
                }
            };
            let ev = match r {
                Ok(r) => json!({
                    "event": "http",
                    "id": rid,
                    "status": r.status,
                    "body": String::from_utf8_lossy(&r.body),
                    "error": null,
                }),
                Err(e) => {
                    json!({"event": "http", "id": rid, "status": 0, "body": "", "error": e.to_string()})
                }
            };
            let _ = tx.send(NetCmd::PluginHttp { plugin, event: ev }).await;
        });
    }

    pub(super) fn plugin_http_done(&mut self, plugin: &str) {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == plugin) {
            p.http_inflight = p.http_inflight.saturating_sub(1);
        }
    }

    /// Sends a message as the user: to the server, to the history and to
    /// every connected client.
    pub(super) async fn send_as_user(&mut self, command: &str, target: &str, text: &str) {
        self.send(Message::new(
            command,
            vec![target.to_string(), text.to_string()],
        ));
        let mut echo = Message::new(command, vec![target.to_string(), text.to_string()])
            .with_prefix(self.self_prefix());
        let ts = db::now_ms();
        echo.set_tag("time", Some(format_server_time(ts)));
        let conv = self.isupport.strip_statusmsg(target).to_string();
        let nm = NewMessage {
            network_id: self.net.id,
            target_cf: self.fold(&conv),
            target: conv,
            ts,
            sender: self.self_prefix(),
            command: command.to_string(),
            text: text.to_string(),
            outgoing: true,
            mention: false,
        };
        let id = self.record(nm).await;
        if let Some(i) = id {
            echo.set_tag("msgid", Some(format!("x{i}")));
        }
        self.broadcast(&echo, id);
    }

    /// `/msg *<plugin> ...` from a client: true if a plugin owns that name.
    pub(super) async fn plugin_command(
        &mut self,
        client: ClientId,
        target: &str,
        text: &str,
    ) -> bool {
        let Some(name) = target.strip_prefix('*') else {
            return false;
        };
        let Some(p) = self
            .plugins
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(name))
        else {
            return false;
        };
        let id = p.id.clone();
        if p.stopped {
            let line = format!(
                ":*{id}!xbnc@xbnc PRIVMSG {} :{}",
                self.current_nick(),
                self.t("plugin_is_stopped", &[&id])
            );
            self.send_to(client, line);
            return true;
        }
        let n = self
            .plugin_event(Some(&id), json!({"event": "command", "text": text}), None)
            .await;
        if n == 0 {
            // The plugin has nothing to say: explain what it is and how to use it.
            let m = self
                .plugins
                .iter()
                .find(|p| p.id == id)
                .and_then(|p| p.inst.as_ref().map(|i| i.plugin.manifest.clone()));
            if let Some(m) = m {
                let lang = self.lang.code().to_string();
                let tr = m.translations.get(&lang);
                let name = tr
                    .map(|t| t.name.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(m.name.clone());
                let desc = tr
                    .map(|t| t.description.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(m.description.clone());
                let usage = tr
                    .map(|t| t.usage.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(m.usage.clone());
                let mut lines = vec![format!("{name} {}: {desc}", m.version)];
                lines.extend(
                    usage
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(str::to_string),
                );
                for l in lines {
                    let line = format!(":*{id}!xbnc@xbnc PRIVMSG {} :{l}", self.current_nick());
                    self.send_to(client, line);
                }
            }
        }
        true
    }

    /// Message seen on the network (or sent by the user from a client).
    pub(super) async fn plugin_message(
        &mut self,
        command: &str,
        conversation: &str,
        sender: &str,
        text: &str,
        outgoing: bool,
    ) {
        if self.plugins.is_empty() {
            return;
        }
        let action = text
            .strip_prefix("\x01ACTION ")
            .map(|t| t.trim_end_matches('\x01').to_string());
        let from = sender.split('!').next().unwrap_or("").to_string();
        let ev = json!({
            "event": "message",
            "command": command,
            "conversation": conversation,
            "channel": self.isupport.is_channel(conversation),
            "from": from,
            "from_mask": sender,
            "text": action.clone().unwrap_or_else(|| text.to_string()),
            "action": action.is_some(),
            "outgoing": outgoing,
        });
        self.plugin_event(None, ev, Some(conversation.to_string()))
            .await;
    }

    /// Whether any running plugin wants presence events (so the JSON is
    /// only built when someone will read it).
    pub(super) fn wants_presence(&self) -> bool {
        self.plugins.iter().any(|p| {
            !p.stopped
                && p.inst
                    .as_ref()
                    .is_some_and(|i| i.plugin.manifest.has("presence"))
        })
    }

    /// A presence event (`join`, `part`, `kick`, `quit`, `nick`, `names`).
    pub(super) async fn plugin_presence(&mut self, event: Value) {
        if self.wants_presence() {
            self.plugin_event(None, event, None).await;
        }
    }

    /// Plugins installed and globally enabled: (id, on here, name).
    pub(super) async fn plugin_states(&mut self) -> Vec<(String, bool, String)> {
        let nid = self.net.id;
        let res = self
            .ctx
            .db
            .call(move |c| Ok((db::list_plugins(c)?, db::plugin_config_rows(c, nid)?)))
            .await;
        let Ok((all, rows)) = res else {
            return Vec::new();
        };
        all.into_iter()
            .filter(|p| p.enabled)
            .map(|p| {
                let on = rows.iter().any(|(id, en, _, _)| id == &p.id && *en);
                (p.id, on, p.manifest.name)
            })
            .collect()
    }

    /// `*status enable/disable/set/addline/dellines` on a plugin.
    pub(super) async fn edit_plugin(
        &mut self,
        id: ClientId,
        plugin: &str,
        f: impl FnOnce(&mut crate::modules::ModState) -> anyhow::Result<()>,
    ) {
        let Some(p) = self.ctx.plugins.get(plugin) else {
            return;
        };
        let nid = self.net.id;
        let pid = plugin.to_string();
        let rows = self
            .ctx
            .db
            .call(move |c| db::plugin_config_rows(c, nid))
            .await
            .unwrap_or_default();
        let (enabled, config) = rows
            .into_iter()
            .find(|(i, ..)| i == &pid)
            .map(|(_, e, c, _)| (e, c))
            .unwrap_or((false, Map::new()));
        let mut st = crate::modules::ModState { enabled, config };
        let was = st.enabled;
        if let Err(e) = f(&mut st) {
            self.say(id, "module_error", &[&e.to_string()]);
            return;
        }
        let cfg = match crate::plugins::validate_config(&p.manifest.settings, &st.config) {
            Ok(c) => c,
            Err(e) => {
                self.say(id, "module_error", &[&e.to_string()]);
                return;
            }
        };
        let pid = plugin.to_string();
        let on = st.enabled;
        if let Err(e) = self
            .ctx
            .db
            .call(move |c| db::set_plugin_config(c, nid, &pid, on, &cfg))
            .await
        {
            self.say(id, "module_error", &[&e.to_string()]);
            return;
        }
        self.load_modules().await;
        if on != was {
            self.say(
                id,
                if on {
                    "module_enabled"
                } else {
                    "module_disabled"
                },
                &[plugin],
            );
        } else {
            self.say(id, "module_saved", &[]);
        }
        let (owner, net, db, plugin) = (
            self.owner.clone(),
            self.net.name.clone(),
            self.ctx.db.clone(),
            plugin.to_string(),
        );
        tokio::spawn(async move {
            let detail = format!("{owner}/{net} plugin {plugin} (from IRC)");
            let _ = db
                .call(move |c| db::audit(c, &owner, "network.plugin", &detail))
                .await;
        });
    }

    pub(super) async fn plugin_get(&mut self, id: ClientId, plugin: &str) {
        let Some(p) = self.ctx.plugins.get(plugin) else {
            return;
        };
        let nid = self.net.id;
        let pid = plugin.to_string();
        let rows = self
            .ctx
            .db
            .call(move |c| db::plugin_config_rows(c, nid))
            .await
            .unwrap_or_default();
        let (enabled, config) = rows
            .into_iter()
            .find(|(i, ..)| i == &pid)
            .map(|(_, e, c, _)| (e, c))
            .unwrap_or((false, Map::new()));
        let cfg =
            crate::plugins::validate_config(&p.manifest.settings, &config).unwrap_or_default();
        let state = self.t(if enabled { "on" } else { "off" }, &[]);
        self.say(id, "module_settings", &[plugin, &state]);
        if p.manifest.settings.is_empty() {
            self.say(id, "module_no_settings", &[plugin]);
        }
        for s in &p.manifest.settings {
            match cfg.get(&s.key) {
                Some(Value::String(t)) if s.kind == "lines" => {
                    if t.is_empty() {
                        self.say(id, "module_setting_line", &[&s.key, "—"]);
                    }
                    for l in t.lines() {
                        self.say(id, "module_setting_line", &[&s.key, l]);
                    }
                }
                Some(Value::String(t)) => self.say(id, "module_setting_line", &[&s.key, t]),
                Some(v) => self.say(id, "module_setting_line", &[&s.key, &v.to_string()]),
                None => {}
            }
        }
    }
}
