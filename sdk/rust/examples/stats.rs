//! Example plugin: counts messages per channel and answers "!stats".
//! The counts survive restarts thanks to the persisted state.
//!
//! Build: cargo build -p xbnc-plugin --release --target wasm32-unknown-unknown --example stats
//! then: xbncd plugin pack target/wasm32-unknown-unknown/release/examples/stats.wasm \
//!         sdk/rust/examples/stats.json -o stats.wasm

use xbnc_plugin::{plugin, Event, Plugin, Response};

#[derive(Default)]
struct Stats;

impl Plugin for Stats {
    fn on_event(&mut self, ev: &Event) -> Response {
        let mut r = Response::new();
        let mut counts = ev.state().as_object().cloned().unwrap_or_default();
        match ev.kind() {
            "message" if ev.is_channel() && !ev.outgoing() => {
                let chan = ev.conversation().to_lowercase();
                let n = counts.get(&chan).and_then(|v| v.as_u64()).unwrap_or(0) + 1;
                counts.insert(chan, n.into());
                if ev.text().trim() == ev.config_str("trigger") {
                    r.reply(format!("{} messages counted in {}", n, ev.conversation()));
                }
                r.state(counts.into());
            }
            "command" => {
                if counts.is_empty() {
                    r.tell("Nothing counted yet.");
                }
                let mut v: Vec<_> = counts.iter().collect();
                v.sort_by_key(|e| std::cmp::Reverse(e.1.as_u64()));
                for (chan, n) in v.into_iter().take(10) {
                    r.tell(format!("{chan}: {n}"));
                }
            }
            _ => {}
        }
        r
    }
}

plugin!(Stats);
