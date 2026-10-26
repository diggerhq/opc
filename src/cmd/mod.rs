//! Commands. [`Ctx`] is what every command gets: the configuration, the
//! shared HTTP connection, the global options and what the terminal is.

pub mod ask;
pub mod attach;
pub mod auth;
pub mod doctor;
pub mod follow;
pub mod help;
pub mod log;
pub mod ls;
pub mod manage;
pub mod new;
pub mod pull;
pub mod push;
pub mod resolve;
pub mod run;
pub mod secret;
pub mod send;
pub mod shell;
pub mod subagents;
pub mod transfer;
pub mod wait;
pub mod watch;

use crate::cli::{Cli, Command, Global};
use crate::config::{Config, DEFAULT_API_URL, DEFAULT_WIRE_URL};
use crate::error::{Error, Result};
use crate::http::Http;
use crate::wire::{Attach, Wire, WireEvent, WireHandle, WireOptions};
use serde_json::Value;
use std::io::{IsTerminal, Read, Write};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

/// Which standard streams are terminals.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ttys {
    pub stdin: bool,
    pub stdout: bool,
    pub stderr: bool,
}

impl Ttys {
    pub fn detect() -> Ttys {
        Ttys { stdin: std::io::stdin().is_terminal(), stdout: std::io::stdout().is_terminal(), stderr: std::io::stderr().is_terminal() }
    }
    pub fn interactive(&self) -> bool {
        self.stdin && self.stdout
    }
}

pub struct Ctx {
    pub config: Config,
    pub http: Http,
    pub global: Global,
    pub tty: Ttys,
    /// Colors on: a terminal on stdout, no `NO_COLOR`, no `--no-color`.
    pub color: bool,
}

impl Ctx {
    pub fn new(global: Global, tty: Ttys) -> Result<Ctx> {
        let config = Config::load();
        let http = Http::new(&config)?;
        let no_color = global.no_color || std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        Ok(Ctx { config, http, global, tty, color: tty.stdout && !no_color })
    }

    /// Reloads the configuration (after `opc login`).
    pub fn reload(&mut self) -> Result<()> {
        self.config = Config::load();
        self.http = Http::new(&self.config)?;
        Ok(())
    }

    /// A progress message for people: stderr, only when it is a terminal and
    /// not `-q`.
    pub fn progress(&self, msg: &str) {
        if self.tty.stderr && !self.global.quiet {
            eprintln!("opc: {msg}");
        }
    }

    /// A message for people that is printed even when stderr is not a
    /// terminal (but not with `-q`).
    pub fn note(&self, msg: &str) {
        if !self.global.quiet {
            eprintln!("opc: {msg}");
        }
    }

    pub fn require_key(&self) -> Result<&str> {
        self.config.require_key()
    }

    /// The session's dashboard link.
    pub fn url_for(&self, session: Option<&crate::api::types::Session>, id: &str) -> String {
        session.and_then(|s| s.urls.dashboard.clone()).unwrap_or_else(|| self.config.dashboard_url(id))
    }

    /// The WebSocket URL: `OPC_WIRE_URL`, then the one `/me` gave last time,
    /// then `/me` (only for an API other than production), then the default.
    pub async fn wire_url(&self) -> String {
        if let Ok(u) = std::env::var("OPC_WIRE_URL")
            && !u.trim().is_empty() {
                return u.trim().to_string();
            }
        if let Some(u) = crate::cache::wire_url(&self.config.api_url) {
            return u;
        }
        if self.config.api_url != DEFAULT_API_URL && self.http.has_key()
            && let Ok(me) = self.http.get("/me", &[]).await
                && let Some(u) = crate::wire::reconnect::wire_url_from_me(&me) {
                    crate::cache::set_wire_url(&self.config.api_url, &u);
                    return u;
                }
        DEFAULT_WIRE_URL.to_string()
    }

    /// Options for a non-interactive command's connection: gives up after
    /// two minutes of continuous disconnection.
    pub async fn wire_options(&self, deltas: bool) -> Result<WireOptions> {
        Ok(WireOptions {
            url: self.wire_url().await,
            key: self.require_key()?.to_string(),
            deltas,
            resume: false,
            read_only: false,
            give_up: Some(Duration::from_secs(120)),
        })
    }

    /// Opens the connection with these sessions attached.
    pub async fn open_wire(&self, attaches: Vec<Attach>, deltas: bool) -> Result<(WireHandle, UnboundedReceiver<WireEvent>)> {
        let opts = self.wire_options(deltas).await?;
        Ok(Wire::open(opts, attaches, Some(self.http.clone())))
    }
}

/// Writes JSON to stdout: indented for a terminal, one line otherwise.
pub fn print_json(ctx: &Ctx, v: &Value) {
    let s = if ctx.tty.stdout { serde_json::to_string_pretty(v) } else { serde_json::to_string(v) }.unwrap_or_default();
    println_out(&s);
}

/// One JSON line to stdout, flushed (for `stream-json`).
pub fn print_line(v: &Value) {
    println_out(&serde_json::to_string(v).unwrap_or_default());
}

/// Prints a line to stdout and flushes; a closed pipe ends the process
/// quietly, as `head` expects.
pub fn println_out(s: &str) {
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{s}").and_then(|_| out.flush()).is_err() {
        std::process::exit(0);
    }
}

/// Standard input as a message (UTF-8, as given).
pub fn read_stdin() -> Result<String> {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).map_err(|e| Error::usage(format!("cannot read standard input: {e}")))?;
    Ok(s)
}

/// Whether stdin is a pipe or a file (something that will end), rather than
/// a terminal or something inherited that may never close.
pub fn stdin_has_data() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        let Ok(meta) = std::fs::metadata("/dev/stdin") else { return false };
        let ft = meta.file_type();
        ft.is_fifo() || ft.is_file() || ft.is_socket()
    }
    #[cfg(not(unix))]
    {
        !std::io::stdin().is_terminal()
    }
}

/// The message from words, `-` and standard input:
/// - `-` alone reads standard input;
/// - words and piped input: the words, a blank line, then the input
///   (`git diff | opc -p "review this"`), when `with_stdin`.
pub fn message_from(words: &[String], with_stdin: bool, tty: Ttys) -> Result<Option<String>> {
    if words.len() == 1 && words[0] == "-" {
        let s = read_stdin()?;
        return Ok(Some(s));
    }
    let arg = (!words.is_empty()).then(|| words.join(" "));
    let piped = if with_stdin && !tty.stdin && (arg.is_none() || stdin_has_data()) { Some(read_stdin()?) } else { None };
    Ok(match (arg, piped) {
        (Some(a), Some(p)) if !p.trim().is_empty() => Some(format!("{a}\n\n{p}")),
        (Some(a), _) => Some(a),
        (None, Some(p)) if !p.trim().is_empty() => Some(p),
        _ => None,
    })
}

pub fn check_message(m: &str) -> Result<()> {
    if m.trim().is_empty() {
        return Err(Error::usage("the message is empty"));
    }
    if m.chars().count() > 100_000 {
        return Err(Error::usage("the message is longer than 100,000 characters"));
    }
    Ok(())
}

/// Runs a parsed command line.
pub async fn dispatch(cli: Cli, ctx: &mut Ctx) -> Result<i32> {
    let Some(command) = cli.command else {
        return run::root(ctx, cli.run, cli.message).await;
    };
    match command {
        Command::New(a) => new::run(ctx, a).await,
        Command::A(a) => attach::run(ctx, a).await,
        Command::Ls(a) => ls::run(ctx, a).await,
        Command::Log(a) => log::run(ctx, a).await,
        Command::Send(a) => send::run(ctx, a).await,
        Command::Wait(a) => wait::run(ctx, a).await,
        Command::Watch(a) => watch::run(ctx, a).await,
        Command::Interrupt(a) => manage::interrupt(ctx, a).await,
        Command::Ask(a) => ask::run(ctx, a).await,
        Command::Subagents(a) => subagents::run(ctx, a).await,
        Command::Rename(a) => manage::rename(ctx, a).await,
        Command::Archive(a) => manage::archive(ctx, a, true).await,
        Command::Unarchive(a) => manage::archive(ctx, a, false).await,
        Command::Open(a) => manage::open(ctx, a).await,
        Command::Diff(a) => manage::diff(ctx, a).await,
        Command::Push(a) => push::run(ctx, a).await,
        Command::Pull(a) => pull::run(ctx, a).await,
        Command::Shell(a) => shell::run(ctx, a).await,
        Command::Secret(a) => secret::run(ctx, a).await,
        Command::Login(a) => auth::login(ctx, a).await,
        Command::Logout => auth::logout(ctx).await,
        Command::Whoami(a) => auth::whoami(ctx, a).await,
        Command::Doctor(a) => doctor::run(ctx, a).await,
        Command::Schema(a) => help::schema(a),
        Command::Help(a) => help::help(a),
        Command::Completion(a) => help::completion(a),
    }
}
