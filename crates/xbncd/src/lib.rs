//! xBNC — IRC bouncer.
//!
//! The binary `xbncd` is a thin CLI over this library; integration tests use
//! the library directly to run a complete bouncer in-process.

pub mod alerts;
pub mod auth;
pub mod backup;
pub mod bouncer;
pub mod catalog;
pub mod avatar;
pub mod certs;
pub mod config;
pub mod crypto;
pub mod db;
pub mod dcc;
pub mod downstream;
pub mod events;
pub mod fish;
pub mod ha;
pub mod http;
pub mod i18n;
pub mod import;
pub mod logs;
pub mod metrics;
pub mod modules;
pub mod netinfo;
pub mod network;
pub mod notify;
pub mod perform;
pub mod plugin_dev;
pub mod plugins;
pub mod push;
pub mod rbac;
pub mod release;
pub mod server;
pub mod settings;
pub mod sql;
pub mod sso;
pub mod totp;
pub mod tz;
pub mod updates;
pub mod util;
pub mod web;
pub mod webauthn;

pub use config::Config;
pub use server::{start, Running};
