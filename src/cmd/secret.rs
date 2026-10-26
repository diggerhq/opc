//! `opc secret set|ls|rm`. A value is read from a hidden prompt, standard
//! input or a file: never from the command line, never printed.

use super::{Ctx, print_json, println_out};
use crate::cli::{Scope, SecretCommand, SecretLsArgs, SecretRmArgs, SecretSetArgs, SessionOpt};
use crate::error::{Error, Result};
use crate::http::{q, session_path};
use http::Method;
use serde_json::{Value, json};

const MAX_VALUE: usize = 64 * 1024;

pub async fn run(ctx: &Ctx, a: SecretCommand) -> Result<i32> {
    ctx.require_key()?;
    match a {
        SecretCommand::Set(a) => set(ctx, a).await,
        SecretCommand::Ls(a) => ls(ctx, a).await,
        SecretCommand::Rm(a) => rm(ctx, a).await,
    }
}

pub fn check_name(name: &str) -> Result<()> {
    let ok = (1..=128).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        && !name.as_bytes()[0].is_ascii_digit();
    if ok { Ok(()) } else { Err(Error::usage(format!("{name:?} is not a secret name: capital letters, digits and _, not starting with a digit"))) }
}

async fn route(ctx: &Ctx, scope: Scope, session: &SessionOpt, name: &str) -> Result<(String, Option<String>)> {
    Ok(match scope {
        Scope::Session => {
            let sid = super::resolve::session_or_last(ctx, session.session.as_deref(), true).await?.id;
            (session_path(&sid, &format!("/secrets/{name}")), Some(sid))
        }
        Scope::Me => {
            no_session(session)?;
            (format!("/me/secrets/{name}"), None)
        }
        Scope::Organization => {
            no_session(session)?;
            (format!("/organization/secrets/{name}"), None)
        }
    })
}

fn no_session(session: &SessionOpt) -> Result<()> {
    if session.session.is_some() { Err(Error::usage("-s goes with --scope session")) } else { Ok(()) }
}

async fn set(ctx: &Ctx, a: SecretSetArgs) -> Result<i32> {
    check_name(&a.name)?;
    let (value, file) = match &a.file {
        Some(p) => (std::fs::read_to_string(p).map_err(|e| Error::usage(format!("cannot read {}: {e}", p.display())))?, true),
        None if ctx.tty.stdin => (read_hidden(&format!("{}: ", a.name))?, false),
        None => {
            let mut s = super::read_stdin()?;
            // A value piped from `echo` ends with one newline that is not part of it.
            if s.ends_with('\n') {
                s.pop();
                if s.ends_with('\r') {
                    s.pop();
                }
            }
            (s, false)
        }
    };
    if value.is_empty() {
        return Err(Error::usage("the value is empty"));
    }
    if value.len() > MAX_VALUE {
        return Err(Error::usage("the value is larger than 64 KiB"));
    }
    let (path, _) = route(ctx, a.scope, &a.session, &a.name).await?;
    let got = ctx.http.call(Method::PUT, &path, &[], Some(&json!({"value": value, "file": file})), None).await?.body;
    drop(value);
    let answered = got.get("answeredAsks").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
    let mut msg = format!("{} set ({})", a.name, a.scope.as_str());
    if answered > 0 {
        msg.push_str(&format!(" · answered {answered} ask{}", if answered == 1 { "" } else { "s" }));
    }
    ctx.progress(&msg);
    Ok(0)
}

/// A line read without echo.
fn read_hidden(prompt: &str) -> Result<String> {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, read};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
    eprint!("{prompt}");
    enable_raw_mode().map_err(|e| Error::new("internal", format!("cannot read from the terminal: {e}")))?;
    let mut s = String::new();
    let outcome = loop {
        match read() {
            Ok(Event::Key(k)) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Enter => break Ok(()),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break Err(Error::new("interrupted", "cancelled")),
                KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => break Ok(()),
                KeyCode::Backspace => {
                    s.pop();
                }
                KeyCode::Char(c) => s.push(c),
                _ => {}
            },
            Ok(Event::Paste(p)) => s.push_str(&p),
            Ok(_) => {}
            Err(e) => break Err(Error::new("internal", format!("cannot read from the terminal: {e}"))),
        }
    };
    let _ = disable_raw_mode();
    eprintln!();
    outcome.map(|_| s)
}

async fn ls(ctx: &Ctx, a: SecretLsArgs) -> Result<i32> {
    let sid = match &a.session.session {
        Some(s) => Some(super::resolve::resolve(ctx, s).await?.id),
        None if ctx.tty.stdin => super::resolve::session_or_last(ctx, None, true).await.ok().map(|r| r.id),
        None => None,
    };
    let mut all: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut query = q(&[("limit", "100")]);
        if let Some(s) = &sid {
            query.push(("sessionId".into(), s.clone()));
        }
        if let Some(c) = &cursor {
            query.push(("cursor".into(), c.clone()));
        }
        let got = ctx.http.get("/secrets", &query).await?;
        all.extend(got.get("secrets").and_then(Value::as_array).cloned().unwrap_or_default());
        match got.get("nextCursor").and_then(Value::as_str) {
            Some(n) => cursor = Some(n.to_string()),
            None => break,
        }
    }
    if a.json {
        print_json(ctx, &json!({"secrets": all}));
        return Ok(0);
    }
    if all.is_empty() {
        ctx.progress("no secrets");
    }
    for s in &all {
        let name = s.get("name").and_then(Value::as_str).unwrap_or("");
        let scope = s.get("scope").and_then(Value::as_str).unwrap_or("");
        let mut line = format!("{name:<32} {scope}");
        if s.get("file").and_then(Value::as_bool) == Some(true) {
            line.push_str(" · file");
        }
        if s.get("effective").and_then(Value::as_bool) == Some(false) {
            line.push_str(" · overridden");
        }
        println_out(&line);
    }
    Ok(0)
}

async fn rm(ctx: &Ctx, a: SecretRmArgs) -> Result<i32> {
    check_name(&a.name)?;
    let (path, _) = route(ctx, a.scope, &a.session, &a.name).await?;
    ctx.http.call(Method::DELETE, &path, &[], None, None).await?;
    ctx.progress(&format!("{} removed ({})", a.name, a.scope.as_str()));
    Ok(0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn names() {
        assert!(super::check_name("STRIPE_API_KEY").is_ok());
        assert!(super::check_name("1KEY").is_err());
        assert!(super::check_name("lower").is_err());
    }
}
