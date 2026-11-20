//! High availability with PostgreSQL: every node runs `xbncd run` against
//! the same database; the one holding a PostgreSQL advisory lock is active,
//! the others wait. If the active node dies its connection closes, the lock
//! is released and a waiting node takes over within seconds. An active node
//! that loses its database connection stops at once, so two nodes never
//! serve at the same time.

use crate::sql::{Conn, PgWorker};
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

/// Advisory lock key ("xBNC" in ASCII).
const LOCK_KEY: i64 = 0x7842_4e43;

pub struct Leadership {
    conn: std::sync::Mutex<Conn>,
}

impl Leadership {
    /// Waits until this node is the active one. `on_wait` is called once
    /// when another node holds the lock.
    pub async fn acquire(
        url: &str,
        tls: Arc<rustls::ClientConfig>,
        on_wait: impl Fn() + Send,
    ) -> Result<Leadership> {
        let mut told = false;
        loop {
            let (u, t) = (url.to_string(), tls.clone());
            let attempt = tokio::task::spawn_blocking(move || -> Result<Option<Conn>> {
                let conn = Conn::Pg(PgWorker::connect(&u, t)?);
                let got: bool =
                    conn.query_row("SELECT pg_try_advisory_lock(?1)", [LOCK_KEY], |r| r.get(0))?;
                Ok(got.then_some(conn))
            })
            .await?;
            match attempt {
                Ok(Some(conn)) => {
                    return Ok(Leadership {
                        conn: std::sync::Mutex::new(conn),
                    })
                }
                Ok(None) => {
                    if !told {
                        on_wait();
                        told = true;
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "high availability: cannot reach the database; retrying")
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    /// Whether the lock is still held (its database session is alive).
    pub async fn alive(self: &Arc<Self>) -> bool {
        let me = self.clone();
        tokio::task::spawn_blocking(move || {
            // The lock lives as long as this session: a working session
            // still holds it.
            me.conn
                .lock()
                .map(|c| c.query_row("SELECT 1", (), |r| r.get::<i64>(0)).is_ok())
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }
}
