//! IRCv3 `draft/read-marker`: the last read message of each conversation,
//! shared between the user's devices (and the panel's chat). Messages
//! already read on another device are not played back again.

use super::*;

impl NetActor {
    pub(super) async fn ensure_markers(&mut self) {
        if self.markers_loaded {
            return;
        }
        let nid = self.net.id;
        match self.ctx.db.call(move |c| db::read_markers(c, nid)).await {
            Ok(rows) => {
                for (target, cf, ts) in rows {
                    self.read_markers.insert(cf, (target, ts));
                }
                self.markers_loaded = true;
            }
            Err(e) => warn!(error = %e, "cannot load read markers"),
        }
    }

    /// Read marker of a conversation (ms), if any.
    pub(super) fn marker_of(&self, target: &str) -> Option<i64> {
        self.read_markers.get(&self.fold(target)).map(|m| m.1)
    }

    fn markread_line(&self, target: &str, ts: Option<i64>) -> String {
        let value = ts.map_or("*".to_string(), |t| {
            format!("timestamp={}", crate::util::format_server_time(t))
        });
        format!(":{} MARKREAD {} {}", self.host(), target, value)
    }

    /// MARKREAD lines for a client that just attached.
    pub(super) fn marker_lines(&self) -> Vec<String> {
        let mut v: Vec<&(String, i64)> = self.read_markers.values().collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v.into_iter()
            .map(|(t, ts)| self.markread_line(t, Some(*ts)))
            .collect()
    }

    /// Moves the marker of `target` to `ts` if it is newer and tells every
    /// device that supports read markers.
    pub(super) async fn mark_read(&mut self, target: &str, ts: i64) -> bool {
        self.ensure_markers().await;
        let cf = self.fold(target);
        if self.read_markers.get(&cf).is_some_and(|m| m.1 >= ts) {
            return false;
        }
        self.read_markers
            .insert(cf.clone(), (target.to_string(), ts));
        let (nid, t) = (self.net.id, target.to_string());
        if let Err(e) = self
            .ctx
            .db
            .call(move |c| db::set_read_marker(c, nid, &t, &cf, ts))
            .await
        {
            warn!(error = %e, "cannot store read marker");
        }
        let line = self.markread_line(target, Some(ts));
        let ids: Vec<ClientId> = self
            .clients
            .iter()
            .filter(|c| c.h.caps.read_marker)
            .map(|c| c.h.id)
            .collect();
        for id in ids {
            self.send_to(id, line.clone());
        }
        true
    }

    /// `MARKREAD <target> [timestamp=...]` from a client.
    pub(super) async fn client_markread(&mut self, id: ClientId, msg: &Message) {
        let Some(target) = msg.param(0).filter(|t| !t.is_empty()) else {
            let line = format!(
                ":{} FAIL MARKREAD NEED_MORE_PARAMS :Missing target",
                self.host()
            );
            self.send_to(id, line);
            return;
        };
        let target = target.to_string();
        self.ensure_markers().await;
        if let Some(v) = msg.param(1) {
            let ts = v
                .strip_prefix("timestamp=")
                .and_then(crate::util::parse_server_time);
            let Some(ts) = ts else {
                let line = format!(
                    ":{} FAIL MARKREAD INVALID_PARAMS {} :Invalid timestamp",
                    self.host(),
                    target
                );
                self.send_to(id, line);
                return;
            };
            if self.mark_read(&target, ts).await {
                return; // already sent to every device, this one included
            }
        }
        let line = self.markread_line(&target, self.marker_of(&target));
        self.send_to(id, line);
    }
}

impl NetActor {
    /// A mention or a private message: notifications while away.
    pub(super) async fn notify_highlight(
        &mut self,
        conversation: &str,
        sender: &str,
        text: &str,
        private: bool,
        ts: i64,
    ) {
        let from = sender.split('!').next().unwrap_or(sender).to_string();
        let text = text
            .strip_prefix("\x01ACTION ")
            .map(|t| format!("* {from} {}", t.trim_end_matches('\x01')))
            .unwrap_or_else(|| text.to_string());
        crate::notify::highlight(
            &self.ctx,
            self.net.user_id,
            crate::notify::Highlight {
                network: self.net.name.clone(),
                network_id: self.net.id,
                conversation: conversation.to_string(),
                from,
                text,
                private,
                ts,
            },
        );
    }
}
