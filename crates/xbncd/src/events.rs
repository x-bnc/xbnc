//! Process-wide events that sessions subscribe to (for example, to tell
//! clients with `soju.im/bouncer-networks-notify` that a network changed).

use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum Event {
    /// A network of this user was added, changed, removed, connected or
    /// disconnected.
    NetworksChanged { user_id: i64 },
}

pub struct Events {
    tx: broadcast::Sender<Event>,
}

impl Default for Events {
    fn default() -> Self {
        Events {
            tx: broadcast::channel(1024).0,
        }
    }
}

impl Events {
    pub fn send(&self, e: Event) {
        let _ = self.tx.send(e);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}
