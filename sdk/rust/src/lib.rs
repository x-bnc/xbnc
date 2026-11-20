//! SDK to write xBNC plugins in Rust.
//!
//! ```ignore
//! use xbnc_plugin::{plugin, Event, Plugin, Response};
//!
//! #[derive(Default)]
//! struct Hello;
//!
//! impl Plugin for Hello {
//!     fn on_event(&mut self, ev: &Event) -> Response {
//!         let mut r = Response::new();
//!         if ev.is_message() && !ev.outgoing() && ev.text() == "!hello" {
//!             r.reply(format!("Hello, {}!", ev.from()));
//!         }
//!         r
//!     }
//! }
//!
//! plugin!(Hello);
//! ```
//!
//! Build with `cargo build --release --target wasm32-unknown-unknown` (crate
//! type `cdylib`) and embed the manifest with
//! `xbncd plugin pack target/wasm32-unknown-unknown/release/hello.wasm hello.json -o hello.wasm`.
//! The ABI is described in docs/PLUGINS.md.

use serde_json::{json, Map, Value};

/// ABI version this SDK implements.
pub const ABI_VERSION: u32 = 1;

/// An event from xBNC.
#[derive(Debug, Clone)]
pub struct Event(pub Value);

impl Event {
    pub fn parse(bytes: &[u8]) -> Event {
        Event(serde_json::from_slice(bytes).unwrap_or(Value::Null))
    }

    fn s(&self, k: &str) -> &str {
        self.0.get(k).and_then(Value::as_str).unwrap_or("")
    }

    fn b(&self, k: &str) -> bool {
        self.0.get(k).and_then(Value::as_bool).unwrap_or(false)
    }

    /// `load`, `connected`, `message`, `command` or `timer`.
    pub fn kind(&self) -> &str {
        self.s("event")
    }
    pub fn is_message(&self) -> bool {
        self.kind() == "message"
    }
    pub fn is_command(&self) -> bool {
        self.kind() == "command"
    }
    /// Network name (as configured in xBNC).
    pub fn network(&self) -> &str {
        self.s("network")
    }
    /// The user's current nick on the network.
    pub fn nick(&self) -> &str {
        self.s("nick")
    }
    /// IRC clients the user has connected right now.
    pub fn clients(&self) -> u64 {
        self.0.get("clients").and_then(Value::as_u64).unwrap_or(0)
    }
    /// Channel or nick the message belongs to.
    pub fn conversation(&self) -> &str {
        self.s("conversation")
    }
    pub fn is_channel(&self) -> bool {
        self.b("channel")
    }
    /// Nick that wrote the message.
    pub fn from(&self) -> &str {
        self.s("from")
    }
    /// Message text (for `message`) or what the user wrote to `*plugin`
    /// (for `command`).
    pub fn text(&self) -> &str {
        self.s("text")
    }
    /// True for messages the user sent.
    pub fn outgoing(&self) -> bool {
        self.b("outgoing")
    }
    /// True for /me actions.
    pub fn is_action(&self) -> bool {
        self.b("action")
    }
    /// PRIVMSG or NOTICE.
    pub fn command(&self) -> &str {
        self.s("command")
    }
    /// The plugin's settings for this network.
    pub fn config(&self) -> &Map<String, Value> {
        static EMPTY: std::sync::OnceLock<Map<String, Value>> = std::sync::OnceLock::new();
        self.0
            .get("config")
            .and_then(Value::as_object)
            .unwrap_or_else(|| EMPTY.get_or_init(Map::new))
    }
    pub fn config_str(&self, k: &str) -> &str {
        self.config().get(k).and_then(Value::as_str).unwrap_or("")
    }
    pub fn config_int(&self, k: &str) -> i64 {
        self.config().get(k).and_then(Value::as_i64).unwrap_or(0)
    }
    pub fn config_bool(&self, k: &str) -> bool {
        self.config()
            .get(k)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
    /// What the plugin last saved with [`Response::state`] (null at first).
    pub fn state(&self) -> &Value {
        self.0.get("state").unwrap_or(&Value::Null)
    }
    /// Any field of the event.
    pub fn get(&self, k: &str) -> &Value {
        self.0.get(k).unwrap_or(&Value::Null)
    }
    /// Channel of a `join`, `part`, `kick` or `names` event.
    pub fn channel(&self) -> &str {
        self.s("channel")
    }
    /// Who was kicked (`kick`).
    pub fn target(&self) -> &str {
        self.s("target")
    }
    /// The new nick (`nick`).
    pub fn new_nick(&self) -> &str {
        self.s("new")
    }
    /// Whether a presence event is about the user themselves.
    pub fn is_self(&self) -> bool {
        self.b("self")
    }
    /// Channels the person was in (`quit`, `nick`).
    pub fn channels(&self) -> Vec<&str> {
        self.list("channels")
    }
    /// Nicks in the channel (`names`).
    pub fn members(&self) -> Vec<&str> {
        self.list("members")
    }
    fn list(&self, k: &str) -> Vec<&str> {
        self.0
            .get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }
}

/// Key-value storage of the plugin on this network (xBNC 0.8+): keys up to
/// 200 bytes, values up to 64 KB, 16 MB in total. Writes made during an
/// event are saved together when it ends.
pub mod kv {
    #[cfg(target_arch = "wasm32")]
    #[link(wasm_import_module = "xbnc")]
    extern "C" {
        fn kv_get(k: *const u8, kl: i32, out: *mut u8, max: i32) -> i32;
        fn kv_set(k: *const u8, kl: i32, v: *const u8, vl: i32) -> i32;
        fn kv_del(k: *const u8, kl: i32) -> i32;
        fn kv_list(p: *const u8, pl: i32, a: *const u8, al: i32, out: *mut u8, max: i32) -> i32;
    }

    #[cfg(not(target_arch = "wasm32"))]
    thread_local! {
        static MEM: std::cell::RefCell<std::collections::BTreeMap<String, Vec<u8>>> = Default::default();
    }

    pub fn get(key: &str) -> Option<Vec<u8>> {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            let mut buf = vec![0u8; 4096];
            let n = kv_get(
                key.as_ptr(),
                key.len() as i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
            );
            if n < 0 {
                return None;
            }
            if n as usize > buf.len() {
                buf.resize(n as usize, 0);
                kv_get(
                    key.as_ptr(),
                    key.len() as i32,
                    buf.as_mut_ptr(),
                    buf.len() as i32,
                );
            }
            buf.truncate(n as usize);
            Some(buf)
        }
        #[cfg(not(target_arch = "wasm32"))]
        MEM.with(|m| m.borrow().get(key).cloned())
    }

    pub fn get_str(key: &str) -> Option<String> {
        get(key).map(|v| String::from_utf8_lossy(&v).into_owned())
    }

    /// False when the value is too big or the plugin is over its quota.
    pub fn set(key: &str, value: impl AsRef<[u8]>) -> bool {
        let v = value.as_ref();
        #[cfg(target_arch = "wasm32")]
        unsafe {
            kv_set(key.as_ptr(), key.len() as i32, v.as_ptr(), v.len() as i32) == 0
        }
        #[cfg(not(target_arch = "wasm32"))]
        MEM.with(|m| {
            m.borrow_mut().insert(key.to_string(), v.to_vec());
            true
        })
    }

    pub fn delete(key: &str) -> bool {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            kv_del(key.as_ptr(), key.len() as i32) == 0
        }
        #[cfg(not(target_arch = "wasm32"))]
        MEM.with(|m| m.borrow_mut().remove(key).is_some())
    }

    /// Up to 1,000 keys starting with `prefix` that sort after `after`.
    pub fn list(prefix: &str, after: &str) -> Vec<String> {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            let mut buf = vec![0u8; 64 * 1024];
            let n = kv_list(
                prefix.as_ptr(),
                prefix.len() as i32,
                after.as_ptr(),
                after.len() as i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
            );
            buf.truncate(n.max(0) as usize);
            String::from_utf8_lossy(&buf)
                .lines()
                .map(str::to_string)
                .collect()
        }
        #[cfg(not(target_arch = "wasm32"))]
        MEM.with(|m| {
            m.borrow()
                .keys()
                .filter(|k| k.starts_with(prefix) && k.as_str() > after)
                .take(1000)
                .cloned()
                .collect()
        })
    }
}

/// The answer to an event: actions and, optionally, new persisted state.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Response {
    actions: Vec<Value>,
    state: Option<Value>,
}

impl Response {
    pub fn new() -> Response {
        Response::default()
    }
    /// Answers in the conversation the event came from (permission `send`).
    pub fn reply(&mut self, text: impl Into<String>) -> &mut Self {
        self.actions.push(json!({ "reply": text.into() }));
        self
    }
    /// Message from `*plugin` to the user's own clients.
    pub fn tell(&mut self, text: impl Into<String>) -> &mut Self {
        self.actions.push(json!({ "tell": text.into() }));
        self
    }
    /// PRIVMSG to a channel or nick (permission `send`).
    pub fn privmsg(&mut self, target: impl Into<String>, text: impl Into<String>) -> &mut Self {
        self.actions
            .push(json!({ "privmsg": { "target": target.into(), "text": text.into() } }));
        self
    }
    /// NOTICE to a channel or nick (permission `send`).
    pub fn notice(&mut self, target: impl Into<String>, text: impl Into<String>) -> &mut Self {
        self.actions
            .push(json!({ "notice": { "target": target.into(), "text": text.into() } }));
        self
    }
    /// Any IRC line (permission `raw`).
    pub fn raw(&mut self, line: impl Into<String>) -> &mut Self {
        self.actions.push(json!({ "raw": line.into() }));
        self
    }
    /// Web GET to an approved domain (permission `http`); the answer comes
    /// back as an `http` event with the same `id`.
    pub fn http_get(&mut self, id: impl Into<String>, url: impl Into<String>) -> &mut Self {
        self.actions
            .push(json!({ "http": { "id": id.into(), "url": url.into() } }));
        self
    }
    /// Web POST of a JSON body (permission `http`).
    pub fn http_post_json(
        &mut self,
        id: impl Into<String>,
        url: impl Into<String>,
        body: Value,
    ) -> &mut Self {
        self.actions.push(
            json!({ "http": { "id": id.into(), "method": "POST", "url": url.into(), "body": body } }),
        );
        self
    }
    /// Replaces the persisted state (at most 64 KB of JSON).
    pub fn state(&mut self, state: Value) -> &mut Self {
        self.state = Some(state);
        self
    }
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty() && self.state.is_none()
    }
    pub fn actions(&self) -> &[Value] {
        &self.actions
    }
    pub fn to_json(&self) -> Vec<u8> {
        let mut o = Map::new();
        if !self.actions.is_empty() {
            o.insert("actions".into(), Value::Array(self.actions.clone()));
        }
        if let Some(s) = &self.state {
            o.insert("state".into(), s.clone());
        }
        serde_json::to_vec(&Value::Object(o)).unwrap_or_default()
    }
}

/// A plugin: one instance per network, kept between events.
pub trait Plugin: Default {
    fn on_event(&mut self, ev: &Event) -> Response;
}

#[doc(hidden)]
pub mod __rt {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        pub static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        pub static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    pub fn alloc(len: i32) -> *mut u8 {
        INPUT.with(|b| {
            let mut b = b.borrow_mut();
            b.clear();
            b.resize(len.max(0) as usize, 0);
            b.as_mut_ptr()
        })
    }

    pub fn dispatch<P: Plugin>(slot: &RefCell<Option<P>>, len: i32) -> i64 {
        let ev = INPUT.with(|b| Event::parse(&b.borrow()[..len.max(0) as usize]));
        let resp = slot
            .borrow_mut()
            .get_or_insert_with(P::default)
            .on_event(&ev);
        if resp.is_empty() {
            return 0;
        }
        OUTPUT.with(|o| {
            let mut o = o.borrow_mut();
            *o = resp.to_json();
            ((o.as_ptr() as usize as i64) << 32) | o.len() as i64
        })
    }
}

/// Exports the ABI functions for a [`Plugin`] type.
#[macro_export]
macro_rules! plugin {
    ($ty:ty) => {
        thread_local! {
            static __XBNC_PLUGIN: ::std::cell::RefCell<Option<$ty>> = const { ::std::cell::RefCell::new(None) };
        }

        #[no_mangle]
        pub extern "C" fn xbnc_alloc(len: i32) -> *mut u8 {
            $crate::__rt::alloc(len)
        }

        #[no_mangle]
        pub extern "C" fn xbnc_on_event(_ptr: *const u8, len: i32) -> i64 {
            __XBNC_PLUGIN.with(|p| $crate::__rt::dispatch::<$ty>(p, len))
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Counter {
        seen: u64,
    }

    impl Plugin for Counter {
        fn on_event(&mut self, ev: &Event) -> Response {
            let mut r = Response::new();
            if ev.is_message() && !ev.outgoing() {
                self.seen += 1;
                if ev.text() == "!count" {
                    r.reply(format!("{} messages", self.seen));
                }
            }
            r
        }
    }

    #[test]
    fn events_and_responses() {
        let ev = Event::parse(
            br#"{"event":"message","text":"!count","from":"bob","config":{"n":3},"state":{"a":1}}"#,
        );
        assert!(ev.is_message());
        assert_eq!(ev.config_int("n"), 3);
        assert_eq!(ev.state()["a"], 1);
        let mut c = Counter::default();
        let r = c.on_event(&ev);
        assert_eq!(r.actions(), &[json!({"reply": "1 messages"})]);
        let mut r = Response::new();
        r.tell("x").state(json!({"k": 2}));
        assert_eq!(
            String::from_utf8(r.to_json()).unwrap(),
            r#"{"actions":[{"tell":"x"}],"state":{"k":2}}"#
        );
    }

    #[test]
    fn presence_and_storage() {
        let ev =
            Event::parse(br##"{"event":"quit","from":"bob","channels":["#a","#b"],"self":false}"##);
        assert_eq!(ev.channels(), vec!["#a", "#b"]);
        assert!(!ev.is_self());
        assert!(kv::set("u:bob", "hola"));
        assert_eq!(kv::get_str("u:bob").as_deref(), Some("hola"));
        assert_eq!(kv::list("u:", ""), vec!["u:bob".to_string()]);
        assert!(kv::delete("u:bob"));
        assert!(kv::get("u:bob").is_none());
    }

    #[test]
    fn runtime_round_trip() {
        let input = br#"{"event":"message","text":"!count","outgoing":false}"#;
        let p = __rt::alloc(input.len() as i32);
        unsafe { std::ptr::copy_nonoverlapping(input.as_ptr(), p, input.len()) };
        let slot = std::cell::RefCell::new(None::<Counter>);
        let r = __rt::dispatch(&slot, input.len() as i32);
        let len = (r & 0xffff_ffff) as usize;
        assert!(len > 0);
        __rt::OUTPUT.with(|o| {
            assert_eq!(
                &o.borrow()[..len],
                br#"{"actions":[{"reply":"1 messages"}]}"#
            )
        });
    }
}
