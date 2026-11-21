//! `xbncd`: run the bouncer and administer it from the command line.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::io::BufRead;
use std::path::PathBuf;
use xbncd::config::Config;
use xbncd::crypto::{self, SecretBox};
use xbncd::db::{self, Db, NewNetwork};

#[derive(Parser)]
#[command(
    name = "xbncd",
    version,
    long_version = concat!(env!("CARGO_PKG_VERSION"), " (xBNC v", env!("CARGO_PKG_VERSION"), " - https://xbnc.org)"),
    author = "xBNC",
    about = "xBNC IRC bouncer - https://xbnc.org"
)]
struct Cli {
    /// Configuration file.
    #[arg(
        short,
        long,
        env = "XBNC_CONFIG",
        default_value = "/etc/xbnc/xbnc.toml",
        global = true
    )]
    config: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the bouncer (foreground). SIGHUP reloads networks and channels.
    Run,
    /// Create the database and the secret key file.
    Init,
    /// Validate the configuration file and exit.
    CheckConfig,
    /// Manage bouncer users.
    #[command(subcommand)]
    User(UserCmd),
    /// Manage a user's IRC networks.
    #[command(subcommand)]
    Network(NetworkCmd),
    /// Manage saved channels.
    #[command(subcommand)]
    Channel(ChannelCmd),
    /// Manage and build WebAssembly plugins.
    #[command(subcommand)]
    Plugin(PluginCmd),
    /// Create release keys and sign or verify packages.
    #[command(subcommand)]
    Release(ReleaseCmd),
    /// Translations: check a language file, list the installed languages.
    #[command(subcommand)]
    Locales(LocalesCmd),
    /// Install an update queued from the panel (run as root by
    /// xbnc-update.service; verifies the signature again).
    ApplyUpdate,
    /// Show recent administrative actions.
    Audit {
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Write a consistent copy of the database (safe while running).
    Backup { dest: PathBuf },
    /// Show the panel address and how IRC clients connect.
    Info,
    /// Copy the SQLite database to a new, empty PostgreSQL database. Stop
    /// xBNC first; afterwards set database.url in xbnc.toml.
    MigrateToPostgres {
        /// postgres://user:pass@host/db
        url: String,
    },
    /// Print the one-time setup code for the first administrator.
    SetupCode,
    /// Move users, networks, channels and modules from another bouncer.
    /// Pass its data directory (the one with configs/ and users/) or its
    /// configuration file. Without --apply nothing is written.
    Import {
        path: PathBuf,
        /// Create what the preview shows.
        #[arg(long)]
        apply: bool,
        /// Also import the plain-text logs into the history.
        #[arg(long)]
        logs: bool,
        /// Time zone of the logs: an offset (+02:00) or a name with daylight
        /// saving time (Europe/Madrid).
        #[arg(long, default_value = "+00:00", allow_hyphen_values = true)]
        utc_offset: String,
    },
    /// Import plain-text logs (one folder per channel with YYYY-MM-DD.log
    /// files) into one network's history. Safe to repeat.
    ImportLogs {
        user: String,
        network: String,
        /// Folder holding one sub-folder per channel or query.
        dir: PathBuf,
        /// Time zone of the logs: an offset (-05:00) or a name with daylight
        /// saving time (America/New_York).
        #[arg(long, default_value = "+00:00", allow_hyphen_values = true)]
        utc_offset: String,
    },
}

#[derive(Subcommand)]
enum ReleaseCmd {
    /// Create a signing key pair. Keep the private key secret (for
    /// example as the XBNC_RELEASE_KEY secret of the CI); the public key
    /// goes in `[updates] public_key`.
    Keygen {
        /// Where to write the private key (mode 0600).
        #[arg(long, default_value = "xbnc-release.key")]
        out: PathBuf,
    },
    /// Write <file>.sig next to each package. The private key is read from
    /// XBNC_RELEASE_KEY or from --key.
    Sign {
        files: Vec<PathBuf>,
        #[arg(long)]
        key: Option<PathBuf>,
    },
    /// Check <file>.sig with a public key.
    Verify {
        #[arg(long)]
        public_key: String,
        files: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum UserCmd {
    /// Create a user. The password is prompted, read from stdin with
    /// --password-stdin, or taken from XBNC_PASSWORD.
    Add {
        name: String,
        /// Shortcut for --role superadmin.
        #[arg(long)]
        admin: bool,
        /// superadmin, admin, moderator, user or limited.
        #[arg(long, default_value = "user")]
        role: String,
        #[arg(long)]
        password_stdin: bool,
    },
    /// Turn off two-factor authentication (lost phone).
    #[command(name = "reset-2fa")]
    Reset2fa {
        name: String,
    },
    /// Change a user's role.
    Role {
        name: String,
        role: String,
    },
    List,
    Delete {
        name: String,
        /// Confirm deletion of the user, their networks and history.
        #[arg(long)]
        yes: bool,
    },
    Passwd {
        name: String,
        #[arg(long)]
        password_stdin: bool,
    },
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
}

#[derive(Subcommand)]
enum NetworkCmd {
    /// Add a network. SASL and server passwords are prompted or read from
    /// XBNC_SASL_PASSWORD / XBNC_SERVER_PASSWORD.
    Add {
        user: String,
        name: String,
        #[arg(long)]
        host: String,
        #[arg(long, default_value_t = 6697)]
        port: u16,
        /// Connect without TLS (not recommended).
        #[arg(long)]
        no_tls: bool,
        #[arg(long)]
        nick: Option<String>,
        #[arg(long)]
        username: Option<String>,
        #[arg(long)]
        realname: Option<String>,
        /// Ask for a server password (PASS).
        #[arg(long)]
        server_password: bool,
        /// Authenticate with SASL PLAIN using this account name.
        #[arg(long)]
        sasl_user: Option<String>,
    },
    List {
        user: String,
    },
    Delete {
        user: String,
        name: String,
        #[arg(long)]
        yes: bool,
    },
    Enable {
        user: String,
        name: String,
    },
    Disable {
        user: String,
        name: String,
    },
}

#[derive(Subcommand)]
enum LocalesCmd {
    /// Check language files (<code>.json) against English: texts missing,
    /// texts that xBNC does not use, broken {0} placeholders or <b> tags.
    /// Does not need the configuration file.
    Check {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Also list every missing text.
        #[arg(long)]
        missing: bool,
    },
    /// Languages this server offers, with how much of each is translated.
    List,
}

#[derive(Subcommand)]
enum PluginCmd {
    /// Embed a manifest (JSON) into a compiled plugin.
    Pack {
        wasm: PathBuf,
        manifest: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Check that a plugin file is valid and show its manifest.
    Check { wasm: PathBuf },
    /// Install (or upgrade) a plugin. Users enable it per network.
    Install {
        wasm: PathBuf,
        /// Accept the permissions the plugin asks for.
        #[arg(long)]
        yes: bool,
    },
    /// Remove a plugin and its settings on every network.
    Remove { id: String },
    /// List installed plugins.
    List,
    /// Catalog publishers: create a signing key (keep the private file safe).
    CatalogKeygen {
        /// Where to write the private key.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Catalog publishers: print an index entry for a plugin file.
    CatalogEntry {
        wasm: PathBuf,
        /// https:// address where the file will be published.
        url: String,
    },
    /// Catalog publishers: sign an index (writes <index>.sig).
    CatalogSign {
        index: PathBuf,
        #[arg(short, long)]
        key: PathBuf,
    },
}

/// Plugin commands that work without a configuration file.
fn plugin_offline(cmd: &PluginCmd) -> bool {
    matches!(
        cmd,
        PluginCmd::Pack { .. }
            | PluginCmd::Check { .. }
            | PluginCmd::CatalogKeygen { .. }
            | PluginCmd::CatalogEntry { .. }
            | PluginCmd::CatalogSign { .. }
    )
}

#[derive(Subcommand)]
enum ChannelCmd {
    /// Save a channel so it is joined automatically.
    Add {
        user: String,
        network: String,
        channel: String,
        /// Ask for the channel key.
        #[arg(long)]
        key: bool,
    },
    List {
        user: String,
        network: String,
    },
    Delete {
        user: String,
        network: String,
        channel: String,
    },
}

fn actor() -> String {
    format!(
        "cli:{}",
        std::env::var("USER").unwrap_or_else(|_| "unknown".into())
    )
}

fn reload_hint() {
    eprintln!("Changes take effect on the running daemon after a reload (systemctl reload xbnc, or send SIGHUP).");
}

fn read_password(prompt: &str, stdin: bool, env: &str, confirm: bool) -> Result<String> {
    if let Ok(v) = std::env::var(env) {
        if !v.is_empty() {
            return Ok(v);
        }
    }
    if stdin {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        return Ok(line.trim_end_matches(['\r', '\n']).to_string());
    }
    let p = rpassword::prompt_password(prompt).context("cannot read password from the terminal")?;
    if confirm {
        let again = rpassword::prompt_password("Repeat: ")?;
        if again != p {
            bail!("passwords do not match");
        }
    }
    Ok(p)
}

fn open(cfg: &Config) -> Result<(Db, SecretBox)> {
    let db = Db::open_config(cfg)?;
    let sb = SecretBox::load(&cfg.secret_key_file, false)?;
    Ok((db, sb))
}

fn user_id(db: &Db, name: &str) -> Result<i64> {
    db.with(|c| db::get_user(c, name))?
        .map(|u| u.id)
        .with_context(|| format!("user {name} not found"))
}

fn network_of(db: &Db, user: &str, name: &str) -> Result<db::Network> {
    let uid = user_id(db, user)?;
    db.with(|c| db::get_network(c, uid, name))?
        .with_context(|| format!("network {name} not found for user {user}"))
}

fn init_logging(level: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));
    use std::io::IsTerminal;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(std::io::stderr)
        .init();
}

fn main() {
    xbncd::util::tune_allocator();
    async_main();
}

#[tokio::main]
async fn async_main() {
    if let Err(e) = real_main().await {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

/// Commands that only print, often into `head`, `less` or `grep`.
fn prints_for_the_terminal(cmd: &Cmd) -> bool {
    matches!(
        cmd,
        Cmd::Info
            | Cmd::SetupCode
            | Cmd::Audit { .. }
            | Cmd::CheckConfig
            | Cmd::Locales(_)
            | Cmd::Release(_)
    )
}

/// `xbncd info | head -1` must stop quietly when the reader closes the pipe,
/// as other command-line tools do, instead of panicking with "failed
/// printing to stdout: Broken pipe". Only for the commands above: the
/// service keeps ignoring SIGPIPE, so a client that disconnects never stops it.
fn quiet_broken_pipe() {
    #[cfg(unix)]
    // SAFETY: called once at start, before any thread prints; restoring the
    // default action of a signal has no other effect on the process.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

/// `xbncd locales check`: exit status 1 when a file has problems (missing
/// texts alone are fine: they show in English).
fn locales_check(files: &[PathBuf], list_missing: bool) -> Result<()> {
    let mut bad = false;
    for f in files {
        let src =
            std::fs::read_to_string(f).with_context(|| format!("cannot read {}", f.display()))?;
        match xbncd::i18n::check(&src) {
            Err(e) => {
                println!("{}: cannot be used: {e:#}", f.display());
                bad = true;
            }
            Ok(xbncd::i18n::CheckReport {
                info,
                problems,
                missing,
                same,
            }) => {
                println!(
                    "{}: {} ({}), {}% translated, {} missing, {} identical to English, {} problems",
                    f.display(),
                    info.name,
                    info.code,
                    info.complete,
                    missing.len(),
                    same.len(),
                    problems.len()
                );
                let stem = f.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if !stem.eq_ignore_ascii_case(&info.code) {
                    println!("  the file must be called {}.json", info.code);
                    bad = true;
                }
                for p in &problems {
                    println!("  problem: {p}");
                }
                if list_missing {
                    for m in &missing {
                        println!("  missing: {m}");
                    }
                }
                bad |= !problems.is_empty();
            }
        }
    }
    if bad {
        std::process::exit(1);
    }
    Ok(())
}

async fn real_main() -> Result<()> {
    let cli = Cli::parse();
    if prints_for_the_terminal(&cli.cmd) {
        quiet_broken_pipe();
    }
    if let Cmd::Release(r) = cli.cmd {
        return release_cmd(r);
    }
    if let Cmd::Locales(LocalesCmd::Check { files, missing }) = &cli.cmd {
        return locales_check(files, *missing);
    }
    if let Cmd::Plugin(c) = &cli.cmd {
        if plugin_offline(c) {
            let Cmd::Plugin(c) = cli.cmd else {
                unreachable!()
            };
            return plugin_cmd(None, c);
        }
    }
    let cfg = Config::load(&cli.config)?;
    match cli.cmd {
        Cmd::Run => run(cfg).await,
        Cmd::Release(_) => unreachable!(),
        Cmd::Locales(LocalesCmd::Check { .. }) => unreachable!(),
        Cmd::Locales(LocalesCmd::List) => {
            println!("Languages (built in, plus {}):", cfg.locales_dir.display());
            for l in xbncd::i18n::languages() {
                println!(
                    "  {:<8} {:<14} {:>3}% translated{}",
                    l.code,
                    l.name,
                    l.complete,
                    if l.translators.is_empty() {
                        String::new()
                    } else {
                        format!("  ({})", l.translators.join(", "))
                    }
                );
            }
            Ok(())
        }
        Cmd::ApplyUpdate => apply_update_cmd(&cfg),
        Cmd::Init => {
            let db = Db::open_config(&cfg)?;
            let existed = cfg.secret_key_file.exists();
            SecretBox::load(&cfg.secret_key_file, true)?;
            let users = db.with(|c| db::list_users(c))?;
            println!("Database ready: {}", cfg.database.path.display());
            println!(
                "Secret key {}: {}",
                if existed { "found" } else { "created" },
                cfg.secret_key_file.display()
            );
            if users.is_empty() {
                println!("Next: create an administrator with `xbncd user add <name> --admin`.");
            }
            Ok(())
        }
        Cmd::CheckConfig => {
            println!("Configuration OK: {}", cli.config.display());
            Ok(())
        }
        Cmd::User(u) => user_cmd(&cfg, u),
        Cmd::Network(n) => network_cmd(&cfg, n),
        Cmd::Channel(c) => channel_cmd(&cfg, c),
        Cmd::Plugin(c) => plugin_cmd(Some(&cfg), c),
        Cmd::Audit { limit } => {
            let db = Db::open_config(&cfg)?;
            for (ts, who, action, detail) in db.with(|c| db::recent_audit(c, limit))? {
                println!(
                    "{}  {who:<16} {action:<16} {detail}",
                    xbncd::util::format_server_time(ts)
                );
            }
            Ok(())
        }
        Cmd::Info => info_cmd(&cfg),
        Cmd::Import {
            path,
            apply,
            logs,
            utc_offset,
        } => import_cmd(
            &cfg,
            &path,
            apply,
            logs,
            xbncd::import::LogZone::parse(&utc_offset)?,
        ),
        Cmd::ImportLogs {
            user,
            network,
            dir,
            utc_offset,
        } => {
            let off = xbncd::import::LogZone::parse(&utc_offset)?;
            let db = Db::open_config(&cfg)?;
            let net = network_of(&db, &user, &network)?;
            let r = db.with(|c| {
                xbncd::import::import_logs(
                    c,
                    net.id,
                    &net.nick,
                    std::slice::from_ref(&dir),
                    off.clone(),
                )
            })?;
            print_log_report(&format!("{user}/{network}"), &r);
            Ok(())
        }
        Cmd::SetupCode => {
            let path = xbncd::server::setup_code_path(&cfg);
            match std::fs::read_to_string(&path) {
                Ok(c) => {
                    let mut lines = c.lines();
                    let code = lines.next().unwrap_or_default();
                    println!("Setup code: {code}");
                    if let Some(url) = lines.next() {
                        println!("Open:       {url}");
                    }
                    Ok(())
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let users = Db::open_config(&cfg)?.with(|c| db::count_users(c))?;
                    if users > 0 {
                        println!("Setup is complete: an administrator already exists. Log in to the panel.");
                    } else {
                        println!("No setup code yet: start the service (systemctl start xbnc) and run this again.");
                    }
                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!(e))
                    .with_context(|| format!("cannot read {} (run with sudo)", path.display())),
            }
        }
        Cmd::MigrateToPostgres { url } => {
            let from = Db::open(&cfg.database.path)?;
            let to = Db::open_postgres(&url, xbncd::server::tls_client_config()?)?;
            db::copy_sqlite_to_postgres(&from, &to, |table, n| {
                if n > 0 && n % 50_000 == 0 {
                    println!("  {table}: {n} rows…");
                }
            })?;
            println!("Done. Put the same address in the [database] section of xbnc.toml (url = \"postgres://…\")");
            println!("and start xBNC again. The SQLite file is left as it was.");
            Ok(())
        }
        Cmd::Backup { dest } => {
            let db = Db::open_config(&cfg)?;
            db.with(|c| db::backup_to(c, &dest))?;
            println!(
                "Backup written to {} (integrity check passed).",
                dest.display()
            );
            println!("Also keep a copy of the secret key file {}: without it stored credentials cannot be decrypted.",
                     cfg.secret_key_file.display());
            Ok(())
        }
    }
}

fn plugin_dir(cfg: &Config) -> PathBuf {
    cfg.database
        .path
        .parent()
        .map(|p| p.join("plugins"))
        .unwrap_or_else(|| "plugins".into())
}

fn describe_plugin(p: &xbncd::plugins::Plugin) {
    let m = &p.manifest;
    println!("{} {} ({})", m.name, m.version, m.id);
    if !m.description.is_empty() {
        println!("  {}", m.description);
    }
    if !m.author.is_empty() {
        println!("  author: {}", m.author);
    }
    let perms = if m.permissions.is_empty() {
        "none (it can only talk to its own user)".to_string()
    } else {
        m.permissions
            .iter()
            .map(|p| match p.as_str() {
                "send" => "send: send messages to channels and people",
                "raw" => "raw: send any IRC command",
                "timer" => "timer: run every minute",
                "presence" => "presence: see who joins, leaves, quits or changes nick",
                other => other,
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    println!("  permissions: {perms}");
    if !m.settings.is_empty() {
        let keys: Vec<&str> = m.settings.iter().map(|s| s.key.as_str()).collect();
        println!("  settings: {}", keys.join(", "));
    }
    println!("  sha256: {}", p.sha256);
}

fn plugin_offline_cmd(cmd: PluginCmd) -> Result<()> {
    use xbncd::plugins;
    match cmd {
        PluginCmd::Pack {
            wasm,
            manifest,
            output,
        } => {
            let w =
                std::fs::read(&wasm).with_context(|| format!("cannot read {}", wasm.display()))?;
            let m = std::fs::read(&manifest)
                .with_context(|| format!("cannot read {}", manifest.display()))?;
            let packed = plugins::embed_manifest(&w, &m)?;
            let p = plugins::Plugin::compile(&plugins::engine(), &packed)?;
            std::fs::write(&output, &packed)?;
            println!("Wrote {} ({} bytes).", output.display(), packed.len());
            describe_plugin(&p);
            Ok(())
        }
        PluginCmd::Check { wasm } => {
            let w =
                std::fs::read(&wasm).with_context(|| format!("cannot read {}", wasm.display()))?;
            let p = plugins::Plugin::compile(&plugins::engine(), &w)?;
            // Starting it once catches a failing start function.
            plugins::PluginInstance::new(&plugins::engine(), std::sync::Arc::new(p))
                .map(|i| describe_plugin(&i.plugin))?;
            println!("The plugin is valid.");
            Ok(())
        }
        PluginCmd::CatalogKeygen { output } => {
            let (private, public) = xbncd::catalog::keygen()?;
            write_private(&output, private.as_bytes())?;
            println!(
                "Private key written to {} (keep it secret).",
                output.display()
            );
            println!("Public key for [catalog] public_key: {public}");
            Ok(())
        }
        PluginCmd::CatalogEntry { wasm, url } => {
            let w =
                std::fs::read(&wasm).with_context(|| format!("cannot read {}", wasm.display()))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&xbncd::catalog::entry_for(&w, &url)?)?
            );
            Ok(())
        }
        PluginCmd::CatalogSign { index, key } => {
            let i = std::fs::read(&index)
                .with_context(|| format!("cannot read {}", index.display()))?;
            let k = std::fs::read_to_string(&key)
                .with_context(|| format!("cannot read {}", key.display()))?;
            let sig = xbncd::catalog::sign(&k, &i)?;
            let out = PathBuf::from(format!("{}.sig", index.display()));
            std::fs::write(&out, &sig)?;
            println!("Signature written to {}.", out.display());
            Ok(())
        }
        _ => unreachable!(),
    }
}

/// Writes a file only the owner can read.
fn write_private(path: &std::path::Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o
        .open(path)
        .with_context(|| format!("cannot create {} (it must not exist)", path.display()))?;
    f.write_all(data)?;
    Ok(())
}

fn plugin_cmd(cfg: Option<&Config>, cmd: PluginCmd) -> Result<()> {
    use xbncd::plugins;
    if plugin_offline(&cmd) {
        return plugin_offline_cmd(cmd);
    }
    let cfg = cfg.context("this command needs the configuration file")?;
    match cmd {
        PluginCmd::Pack { .. }
        | PluginCmd::Check { .. }
        | PluginCmd::CatalogKeygen { .. }
        | PluginCmd::CatalogEntry { .. }
        | PluginCmd::CatalogSign { .. } => unreachable!(),
        PluginCmd::Install { wasm, yes } => {
            let w =
                std::fs::read(&wasm).with_context(|| format!("cannot read {}", wasm.display()))?;
            let p = plugins::Plugin::compile(&plugins::engine(), &w)?;
            describe_plugin(&p);
            if !p.manifest.permissions.is_empty() && !yes {
                bail!("this plugin asks for permissions; review them and run again with --yes");
            }
            let db = Db::open_config(cfg)?;
            plugins::store_file(&plugin_dir(cfg), &p.manifest.id, &w)?;
            db.with(|c| {
                db::upsert_plugin(c, &p.manifest, &p.sha256, w.len() as i64, "cli")?;
                db::set_plugin_wasm(c, &p.manifest.id, &w)
            })?;
            db.with(|c| {
                db::audit(
                    c,
                    "cli",
                    "plugin.install",
                    &format!("{} {}", p.manifest.id, p.manifest.version),
                )
            })?;
            println!("Installed. Enable it per network in the panel (Modules tab) or with /msg *status enable {}.", p.manifest.id);
            reload_hint();
            Ok(())
        }
        PluginCmd::Remove { id } => {
            let db = Db::open_config(cfg)?;
            if !db.with(|c| db::delete_plugin(c, &id))? {
                bail!("plugin {id} is not installed");
            }
            let _ = std::fs::remove_file(plugin_dir(cfg).join(format!("{id}.wasm")));
            db.with(|c| db::audit(c, "cli", "plugin.remove", &id))?;
            println!("Removed {id}.");
            reload_hint();
            Ok(())
        }
        PluginCmd::List => {
            let db = Db::open_config(cfg)?;
            let list = db.with(|c| db::list_plugins(c))?;
            if list.is_empty() {
                println!("No plugins installed.");
            }
            for p in list {
                println!(
                    "{:<20} {:<10} {:<8} {}",
                    p.id,
                    p.manifest.version,
                    if p.enabled { "enabled" } else { "disabled" },
                    p.manifest.name
                );
            }
            Ok(())
        }
    }
}

fn print_log_report(label: &str, r: &xbncd::import::LogReport) {
    println!(
        "  logs {label}: {} file(s), {} message(s) imported, {} already present, {} line(s) not recognised",
        r.files, r.messages, r.duplicates, r.unrecognised
    );
}

fn import_cmd(
    cfg: &Config,
    path: &std::path::Path,
    apply: bool,
    logs: bool,
    off: xbncd::import::LogZone,
) -> Result<()> {
    let (conf, data_dir) = xbncd::import::locate_config(path)?;
    let text = std::fs::read(&conf).with_context(|| format!("cannot read {}", conf.display()))?;
    if text.len() > xbncd::import::MAX_CONFIG_BYTES {
        bail!("{} is too large", conf.display());
    }
    let text = String::from_utf8_lossy(&text);
    let (db, sb) = open(cfg)?;
    let plan = db.with(|c| xbncd::import::parse(&text, data_dir.as_deref(), c))?;
    println!("Configuration: {}", conf.display());
    match &data_dir {
        Some(d) => println!("Data directory: {}", d.display()),
        None => println!("Data directory: not found (only the configuration file is read)"),
    }
    println!();
    for u in &plan.users {
        match &u.skip {
            Some(why) => {
                println!("- user {}: SKIPPED ({why})", u.name);
                continue;
            }
            None => println!(
                "- user {} ({}, password: {})",
                u.name,
                u.role,
                if u.password == "keep" {
                    "kept"
                } else {
                    "new one generated"
                }
            ),
        }
        for n in &u.networks {
            if let Some(why) = &n.skip {
                println!("    network {}: SKIPPED ({why})", n.name);
                continue;
            }
            println!(
                "    network {}: {}:{}{} as {}{}",
                n.name,
                n.host,
                n.port,
                if n.tls { " (TLS)" } else { "" },
                n.nick,
                if n.enabled { "" } else { ", disabled" }
            );
            println!(
                "      {} channel(s): {}",
                n.channels.len(),
                n.channels.join(" ")
            );
            if !n.modules.is_empty() {
                println!("      modules: {}", n.modules.join(", "));
            }
            let mut extra = Vec::new();
            if n.sasl {
                extra.push("SASL".to_string());
            }
            if n.client_cert {
                extra.push("client certificate".to_string());
            }
            if n.on_connect_lines > 0 {
                extra.push(format!("{} command(s) on connect", n.on_connect_lines));
            }
            if !extra.is_empty() {
                println!("      also: {}", extra.join(", "));
            }
        }
    }
    if !plan.warnings.is_empty() {
        println!();
        println!("Notes:");
        for w in &plan.warnings {
            println!("  * {w}");
        }
    }
    if !apply {
        println!();
        println!(
            "Nothing was changed. Run again with --apply to import{}.",
            if logs { " (and the logs)" } else { "" }
        );
        return Ok(());
    }
    let report =
        db.with(|c| xbncd::import::apply(c, &plan, &sb, cfg.limits.max_networks_per_user))?;
    println!();
    println!(
        "Imported {} user(s), {} network(s), {} channel(s).",
        report.users_created.len(),
        report.networks_created,
        report.channels_created
    );
    for s in &report.skipped {
        println!("  skipped: {s}");
    }
    if !report.new_passwords.is_empty() {
        println!("New passwords (shown only now; users can change them in the panel):");
        for (u, p) in &report.new_passwords {
            println!("  {u}: {p}");
        }
    }
    db.with(|c| {
        db::audit(
            c,
            "cli",
            "import",
            &format!(
                "{} users, {} networks from {}",
                report.users_created.len(),
                report.networks_created,
                conf.display()
            ),
        )
    })?;
    if logs {
        match &data_dir {
            None => println!("Logs: the data directory was not found, so no logs were imported."),
            Some(d) => {
                for u in plan
                    .users
                    .iter()
                    .filter(|u| report.users_created.contains(&u.name))
                {
                    for n in u.networks.iter().filter(|n| n.skip.is_none()) {
                        let dirs = xbncd::import::log_dirs(d, &u.name, &n.name);
                        if dirs.is_empty() {
                            continue;
                        }
                        let Ok(net) = network_of(&db, &u.name, &n.name) else {
                            continue;
                        };
                        let r = db.with(|c| {
                            xbncd::import::import_logs(c, net.id, &net.nick, &dirs, off.clone())
                        })?;
                        print_log_report(&format!("{}/{}", u.name, n.name), &r);
                    }
                }
            }
        }
    }
    reload_hint();
    Ok(())
}

fn info_cmd(cfg: &Config) -> Result<()> {
    use std::net::SocketAddr;
    if let Some(w) = &cfg.web {
        let addr: SocketAddr = w.address.parse().context("web.address")?;
        println!(
            "Web panel:   {}",
            xbncd::server::panel_url(w, addr, &cfg.hostname)
        );
        if addr.ip().is_loopback() && w.public_url.is_none() {
            println!("             (only reachable from this machine; see docs/INSTALL.md)");
        }
    } else {
        println!("Web panel:   disabled (no [web] section)");
    }
    println!("IRC clients (server and port to type in the IRC client):");
    let lan = xbncd::netinfo::primary_ip(false)
        .filter(|ip| xbncd::netinfo::is_local_network(*ip) && !ip.is_loopback());
    for l in &cfg.listen {
        let addr: SocketAddr = l.address.parse().context("listen.address")?;
        let tls = l.tls_cert.is_some();
        let mut notes = vec![if tls { "SSL/TLS on" } else { "SSL/TLS off" }];
        if l.lan_only {
            notes.push("local network only");
        } else if addr.ip().is_loopback() {
            notes.push("this machine only");
        }
        // The address users type: the configured hostname (domain or IP
        // given at installation), else the machine's address.
        let shown = if addr.ip().is_unspecified() {
            if l.lan_only {
                lan.map(|ip| ip.to_string())
                    .unwrap_or_else(|| xbncd::netinfo::display_host(addr))
            } else if cfg.hostname.contains('.') || cfg.hostname.contains(':') {
                cfg.hostname.clone()
            } else {
                xbncd::netinfo::display_host(addr)
            }
        } else {
            xbncd::netinfo::display_host(addr)
        };
        println!(
            "  server {}  port {}  ({})",
            shown,
            addr.port(),
            notes.join(", ")
        );
    }
    if let Some(ip) = lan {
        if cfg.hostname != ip.to_string() {
            println!("  From your local network, the server can also be {ip}.");
        }
    }
    println!();
    println!("Server password (in the IRC client), one line per network:");
    let lines: Vec<String> = Db::open_config(cfg)
        .and_then(|db| {
            db.with(|c| {
                let mut out = Vec::new();
                for u in db::list_users(c)? {
                    if !u.enabled {
                        continue;
                    }
                    for n in db::list_networks(c, u.id)? {
                        out.push(format!(
                            "  {}/{}@<device>:<password of {}>",
                            u.username, n.name, u.username
                        ));
                    }
                }
                Ok(out)
            })
        })
        .unwrap_or_default();
    if lines.is_empty() {
        println!(
            "  <user>/<network>@<device>:<password>   (no networks yet: add them in the panel)"
        );
    } else {
        for l in &lines {
            println!("{l}");
        }
    }
    println!("  <device> is a name for each device (pc, phone…); the password is the panel one.");
    println!(
        "  The panel shows it ready to copy: Networks → your network → Connect your IRC client."
    );
    Ok(())
}

async fn run(cfg: Config) -> Result<()> {
    init_logging(&cfg.log.level);
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting xBNC");
    let leader = match (&cfg.database.url, cfg.database.ha) {
        (Some(url), true) => {
            let tls = xbncd::server::tls_client_config()?;
            let l = xbncd::ha::Leadership::acquire(url, tls, || {
                tracing::info!(
                    "high availability: another node is active; this one waits to take over"
                )
            })
            .await?;
            tracing::info!("high availability: this node is now the active one");
            Some(std::sync::Arc::new(l))
        }
        _ => None,
    };
    let running = xbncd::start(cfg, false).await?;
    if let Some(l) = leader {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                if !l.alive().await {
                    tracing::error!("high availability: lost the database lock; stopping so another node takes over");
                    std::process::exit(75);
                }
            }
        });
    }
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate())?;
        let mut int = signal(SignalKind::interrupt())?;
        let mut hup = signal(SignalKind::hangup())?;
        loop {
            tokio::select! {
                _ = term.recv() => break,
                _ = int.recv() => break,
                _ = running.ctx.stop.notified() => {
                    tracing::info!("shutdown requested from IRC");
                    break;
                }
                _ = running.ctx.restart.notified() => {
                    tracing::info!("restart requested from the panel");
                    running.shutdown().await;
                    // A non-zero exit: systemd (Restart=on-failure) or the
                    // container runtime starts xBNC again.
                    std::process::exit(75);
                }
                _ = hup.recv() => {
                    if let Err(e) = running.reload().await {
                        tracing::error!(error = %e, "reload failed");
                    }
                }
            }
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");
    running.shutdown().await;
    Ok(())
}

fn user_cmd(cfg: &Config, cmd: UserCmd) -> Result<()> {
    let db = Db::open_config(cfg)?;
    match cmd {
        UserCmd::Add {
            name,
            admin,
            role,
            password_stdin,
        } => {
            db::validate_name("username", &name)?;
            let role = if admin {
                "superadmin".to_string()
            } else {
                role
            };
            if xbncd::rbac::Role::parse(&role).is_none() {
                bail!("unknown role {role:?}; use superadmin, admin, moderator, user or limited");
            }
            let pw = read_password(
                &format!("Password for {name}: "),
                password_stdin,
                "XBNC_PASSWORD",
                true,
            )?;
            crypto::check_password_policy(&pw)?;
            let hash = crypto::hash_password(&pw)?;
            db.with(|c| {
                db::create_user_with_role(c, &name, &hash, &role)?;
                db::audit(c, &actor(), "user.create", &format!("{name} role={role}"))
            })?;
            println!("User {name} created with role {role}.");
        }
        UserCmd::Reset2fa { name } => {
            let id = user_id(&db, &name)?;
            db.with(|c| {
                db::set_totp(c, id, None, false)?;
                db::delete_user_sessions(c, id, None)?;
                db::audit(c, &actor(), "user.reset_2fa", &name)
            })?;
            println!("Two-factor authentication turned off for {name}; their panel sessions were closed.");
        }
        UserCmd::Role { name, role } => {
            if xbncd::rbac::Role::parse(&role).is_none() {
                bail!("unknown role {role:?}; use superadmin, admin, moderator, user or limited");
            }
            if !db.with(|c| db::set_user_role(c, &name, &role))? {
                bail!("user {name} not found");
            }
            db.with(|c| db::audit(c, &actor(), "user.role", &format!("{name} role={role}")))?;
            println!("User {name} now has role {role}.");
        }
        UserCmd::List => {
            let users = db.with(|c| db::list_users(c))?;
            if users.is_empty() {
                println!("No users.");
            }
            for u in users {
                let nets = db.with(|c| db::list_networks(c, u.id))?;
                println!(
                    "{:<20} {:<10} {:<9} networks: {}",
                    u.username,
                    u.role,
                    if u.enabled { "enabled" } else { "disabled" },
                    nets.iter()
                        .map(|n| n.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        UserCmd::Delete { name, yes } => {
            if !yes {
                bail!("deleting {name} removes all their networks and history; repeat with --yes to confirm");
            }
            if !db.with(|c| db::delete_user(c, &name))? {
                bail!("user {name} not found");
            }
            db.with(|c| db::audit(c, &actor(), "user.delete", &name))?;
            println!("User {name} deleted.");
            reload_hint();
        }
        UserCmd::Passwd {
            name,
            password_stdin,
        } => {
            user_id(&db, &name)?;
            let pw = read_password(
                &format!("New password for {name}: "),
                password_stdin,
                "XBNC_PASSWORD",
                true,
            )?;
            crypto::check_password_policy(&pw)?;
            let hash = crypto::hash_password(&pw)?;
            db.with(|c| {
                db::set_user_password(c, &name, &hash)?;
                db::audit(c, &actor(), "user.passwd", &name)
            })?;
            println!("Password updated for {name}.");
        }
        UserCmd::Enable { name } => {
            if !db.with(|c| db::set_user_enabled(c, &name, true))? {
                bail!("user {name} not found");
            }
            db.with(|c| db::audit(c, &actor(), "user.enable", &name))?;
            println!("User {name} enabled.");
            reload_hint();
        }
        UserCmd::Disable { name } => {
            if !db.with(|c| db::set_user_enabled(c, &name, false))? {
                bail!("user {name} not found");
            }
            db.with(|c| db::audit(c, &actor(), "user.disable", &name))?;
            println!("User {name} disabled.");
            reload_hint();
        }
    }
    Ok(())
}

fn network_cmd(cfg: &Config, cmd: NetworkCmd) -> Result<()> {
    let (db, sb) = open(cfg)?;
    match cmd {
        NetworkCmd::Add {
            user,
            name,
            host,
            port,
            no_tls,
            nick,
            username,
            realname,
            server_password,
            sasl_user,
        } => {
            let uid = user_id(&db, &user)?;
            if no_tls {
                eprintln!("warning: {name} will be reached without TLS; credentials travel in clear text.");
            }
            let server_password_enc = if server_password {
                Some(sb.encrypt(&read_password(
                    "Server password: ",
                    false,
                    "XBNC_SERVER_PASSWORD",
                    false,
                )?))
            } else {
                None
            };
            let sasl_password_enc = match &sasl_user {
                Some(acc) => Some(sb.encrypt(&read_password(
                    &format!("SASL password for {acc}: "),
                    false,
                    "XBNC_SASL_PASSWORD",
                    false,
                )?)),
                None => None,
            };
            let nick = nick.unwrap_or_else(|| user.clone());
            let nn = NewNetwork {
                name: name.clone(),
                host,
                port,
                tls: !no_tls,
                username: username.unwrap_or_else(|| nick.clone()),
                realname: realname.unwrap_or_else(|| nick.clone()),
                nick,
                server_password_enc,
                sasl_username: sasl_user,
                sasl_password_enc,
            };
            db.with(|c| {
                db::create_network(c, uid, &nn, cfg.limits.max_networks_per_user)?;
                db::audit(
                    c,
                    &actor(),
                    "network.create",
                    &format!("{user}/{name} {}:{} tls={}", nn.host, nn.port, nn.tls),
                )
            })?;
            println!("Network {name} added for {user}. Log in as {user}/{name}.");
            reload_hint();
        }
        NetworkCmd::List { user } => {
            let uid = user_id(&db, &user)?;
            for n in db.with(|c| db::list_networks(c, uid))? {
                println!(
                    "{:<16} {}:{} tls={} nick={} sasl={} {}",
                    n.name,
                    n.host,
                    n.port,
                    if n.tls { "yes" } else { "no" },
                    n.nick,
                    n.sasl_username.as_deref().unwrap_or("-"),
                    if n.enabled { "enabled" } else { "disabled" }
                );
            }
        }
        NetworkCmd::Delete { user, name, yes } => {
            if !yes {
                bail!("deleting {user}/{name} removes its channels and history; repeat with --yes to confirm");
            }
            let uid = user_id(&db, &user)?;
            if !db.with(|c| db::delete_network(c, uid, &name))? {
                bail!("network {name} not found for {user}");
            }
            db.with(|c| db::audit(c, &actor(), "network.delete", &format!("{user}/{name}")))?;
            println!("Network {name} deleted.");
            reload_hint();
        }
        NetworkCmd::Enable { user, name } => {
            let uid = user_id(&db, &user)?;
            if !db.with(|c| db::set_network_enabled(c, uid, &name, true))? {
                bail!("network {name} not found for {user}");
            }
            db.with(|c| db::audit(c, &actor(), "network.enable", &format!("{user}/{name}")))?;
            println!("Network {name} enabled.");
            reload_hint();
        }
        NetworkCmd::Disable { user, name } => {
            let uid = user_id(&db, &user)?;
            if !db.with(|c| db::set_network_enabled(c, uid, &name, false))? {
                bail!("network {name} not found for {user}");
            }
            db.with(|c| db::audit(c, &actor(), "network.disable", &format!("{user}/{name}")))?;
            println!("Network {name} disabled.");
            reload_hint();
        }
    }
    Ok(())
}

fn channel_cmd(cfg: &Config, cmd: ChannelCmd) -> Result<()> {
    let (db, sb) = open(cfg)?;
    // The network's real case mapping is only known once connected; rfc1459
    // is the IRC default and matches what the daemon assumes before ISUPPORT.
    let cm = xbnc_proto::CaseMapping::Rfc1459;
    match cmd {
        ChannelCmd::Add {
            user,
            network,
            channel,
            key,
        } => {
            if !channel.starts_with(['#', '&', '!', '+']) || channel.contains([' ', ',', '\x07']) {
                bail!("invalid channel name {channel:?}");
            }
            let net = network_of(&db, &user, &network)?;
            let key_enc = if key {
                Some(sb.encrypt(&read_password(
                    "Channel key: ",
                    false,
                    "XBNC_CHANNEL_KEY",
                    false,
                )?))
            } else {
                None
            };
            db.with(|c| {
                db::upsert_channel(
                    c,
                    net.id,
                    &channel,
                    &cm.fold(&channel),
                    key_enc.as_deref(),
                    key,
                )?;
                db::audit(
                    c,
                    &actor(),
                    "channel.add",
                    &format!("{user}/{network} {channel}"),
                )
            })?;
            println!("Channel {channel} saved for {user}/{network}.");
            reload_hint();
        }
        ChannelCmd::List { user, network } => {
            let net = network_of(&db, &user, &network)?;
            for ch in db.with(|c| db::list_channels(c, net.id))? {
                println!(
                    "{}{}",
                    ch.name,
                    if ch.key_enc.is_some() { " (key)" } else { "" }
                );
            }
        }
        ChannelCmd::Delete {
            user,
            network,
            channel,
        } => {
            let net = network_of(&db, &user, &network)?;
            if !db.with(|c| db::delete_channel(c, net.id, &cm.fold(&channel)))? {
                bail!("channel {channel} not saved for {user}/{network}");
            }
            db.with(|c| {
                db::audit(
                    c,
                    &actor(),
                    "channel.delete",
                    &format!("{user}/{network} {channel}"),
                )
            })?;
            println!(
                "Channel {channel} removed. The bouncer stays in it until you /part or reconnect."
            );
            reload_hint();
        }
    }
    Ok(())
}

fn file_name(p: &std::path::Path) -> Result<String> {
    p.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .context("invalid file name")
}

fn release_cmd(cmd: ReleaseCmd) -> Result<()> {
    use xbncd::release;
    match cmd {
        ReleaseCmd::Keygen { out } => {
            if out.exists() {
                bail!(
                    "{} already exists; refusing to overwrite a key",
                    out.display()
                );
            }
            let (private, public) = release::keygen()?;
            let mut o = std::fs::OpenOptions::new();
            o.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                o.mode(0o600);
            }
            use std::io::Write;
            o.open(&out)?.write_all(format!("{private}\n").as_bytes())?;
            println!(
                "Private key: {} (keep it secret, never commit it)",
                out.display()
            );
            println!("Public key:  {public}");
            println!();
            println!("Put the public key in /etc/xbnc/xbnc.toml:");
            println!("  [updates]\n  public_key = \"{public}\"");
            Ok(())
        }
        ReleaseCmd::Sign { files, key } => {
            let private = match (std::env::var("XBNC_RELEASE_KEY"), key) {
                (Ok(k), _) if !k.trim().is_empty() => k,
                (_, Some(path)) => std::fs::read_to_string(path)?,
                _ => bail!("set XBNC_RELEASE_KEY or pass --key"),
            };
            if files.is_empty() {
                bail!("no files to sign");
            }
            for f in files {
                let name = file_name(&f)?;
                let sig = release::sign(private.trim(), &name, &std::fs::read(&f)?)?;
                let out = f.with_file_name(format!("{name}.sig"));
                std::fs::write(&out, sig)?;
                println!("signed {}", out.display());
            }
            Ok(())
        }
        ReleaseCmd::Verify { public_key, files } => {
            let mut bad = 0;
            for f in files {
                let name = file_name(&f)?;
                let sig = std::fs::read_to_string(f.with_file_name(format!("{name}.sig")))
                    .unwrap_or_default();
                match release::verify(&public_key, &name, &std::fs::read(&f)?, &sig) {
                    Ok(()) => println!("OK      {name}"),
                    Err(e) => {
                        bad += 1;
                        println!("FAILED  {name}: {e:#}");
                    }
                }
            }
            if bad > 0 {
                bail!("{bad} file(s) failed verification");
            }
            Ok(())
        }
    }
}

fn apply_update_cmd(cfg: &Config) -> Result<()> {
    let spool = cfg
        .database
        .path
        .parent()
        .map(|p| p.join("updates"))
        .context("no data directory")?;
    let res = xbncd::updates::apply_pending(cfg);
    let (ok, msg) = match &res {
        Ok(v) => (true, format!("installed {v}")),
        Err(e) => (false, format!("{e:#}")),
    };
    let result = serde_json::json!({ "ok": ok, "message": msg, "at": xbncd::db::now_ms() });
    // Readable by the daemon (user xbnc); never follows a link it left.
    if std::fs::symlink_metadata(&spool).is_ok_and(|m| m.is_dir()) {
        let _ = xbncd::updates::write_nofollow(
            &spool.join("result.json"),
            result.to_string().as_bytes(),
            0o644,
        );
    }
    println!("{msg}");
    res.map(|_| ())
}
