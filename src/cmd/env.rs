//! `opc env [show|setup|history|rollback]`: the Computer's environment, which
//! is what every new session starts with.

use super::{Ctx, print_json, println_out};
use crate::cli::{EnvArgs, EnvCommand, EnvRollbackArgs, EnvSetupArgs};
use crate::error::{Error, Result};
use crate::http::q;
use crate::render::{self, Style};
use serde_json::{Value, json};
use std::io::Write;

pub async fn run(ctx: &Ctx, a: EnvArgs) -> Result<i32> {
    ctx.require_key()?;
    match a.command {
        None => show(ctx, a.json).await,
        Some(EnvCommand::Show(s)) => show(ctx, s.json).await,
        Some(EnvCommand::Setup(s)) => setup(ctx, s).await,
        Some(EnvCommand::History(h)) => history(ctx, h.json).await,
        Some(EnvCommand::Rollback(r)) => rollback(ctx, r).await,
    }
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn ago(v: &Value) -> String {
    super::transfer::ago(s(v, "createdAt")).unwrap_or_default()
}

fn size(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1.0 { format!("{gb:.1} GB") } else { format!("{:.0} MB", bytes as f64 / 1e6) }
}

/// A script under its label, one command a line.
fn script(style: &Style, label: &str, text: &str) {
    let mut label = label;
    for line in text.lines() {
        println_out(&format!("{label:<10}{}", style.dim(line)));
        label = "";
    }
}

/// The scripts, repositories and secrets of a revision or a proposal.
fn contents(style: &Style, r: &Value) {
    let repos: Vec<String> = r["repos"].as_array().into_iter().flatten().map(|x| format!("{}{}", s(x, "name"), if x["default"] == true { " (default)" } else { "" })).collect();
    println_out(&format!("{:<10}{}", "Repos", repos.join(", ")));
    script(style, "Install", s(r, "install"));
    script(style, "Start", s(r, "start"));
    let secrets: Vec<&str> = r["secrets"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if !secrets.is_empty() {
        println_out(&format!("{:<10}{}", "Secrets", secrets.join(", ")));
    }
}

fn print_env(ctx: &Ctx, env: &Value) {
    let style = Style { color: ctx.color };
    let rev = &env["revision"];
    if rev.is_null() {
        println_out("No environment yet. Sessions start from the bare computer: no root, nothing extra installed.");
        println_out("Set one up: opc env setup");
        return;
    }
    let status = match s(env, "status") {
        "ready" => style.green("Ready"),
        "applying" => style.yellow("Applying"),
        other => style.red(other),
    };
    println_out(&format!("{:<10}{status}  {}", "Status", s(rev, "summary")));
    contents(&style, rev);
    let image = &rev["image"];
    println_out(&format!("{:<10}{}  {}  built in {}", "Image", s(image, "id"), size(image["sizeBytes"].as_u64().unwrap_or(0)), render::duration(image["buildMs"].as_u64().unwrap_or(0))));
    println_out(&format!("{:<10}{} by {}", "Changed", ago(rev), s(&rev["createdBy"], "name")));
    println_out("");
    println_out(&style.dim("New sessions start from this. Running sessions keep what they started with."));
}

async fn show(ctx: &Ctx, json: bool) -> Result<i32> {
    let got = ctx.http.get("/environment", &[]).await?;
    if json {
        print_json(ctx, &got);
    } else {
        print_env(ctx, &got["environment"]);
    }
    Ok(0)
}

async fn history(ctx: &Ctx, json: bool) -> Result<i32> {
    let got = ctx.http.get("/environment/history", &[]).await?;
    if json {
        print_json(ctx, &got);
        return Ok(0);
    }
    let style = Style { color: ctx.color };
    let all = got["revisions"].as_array().cloned().unwrap_or_default();
    if all.is_empty() {
        ctx.progress("no environment yet: opc env setup");
    }
    for (i, r) in all.iter().enumerate() {
        let mark = if i == 0 { style.green("current") } else { "       ".to_string() };
        println_out(&format!("{}  {mark}  {:<9}  {:<14}  {}  {}", s(r, "id"), ago(r), s(&r["createdBy"], "name"), s(&r["image"], "id"), s(r, "summary")));
    }
    Ok(0)
}

async fn rollback(ctx: &Ctx, a: EnvRollbackArgs) -> Result<i32> {
    let body = match &a.to {
        Some(id) => json!({"to": id}),
        None => json!({}),
    };
    let got = ctx.http.post("/environment/rollback", &body, None).await?;
    if a.json {
        print_json(ctx, &got);
        return Ok(0);
    }
    let rev = &got["environment"]["revision"];
    println_out(&format!("{}. New sessions start from {} now; nothing was rebuilt.", s(rev, "summary"), s(&rev["image"], "id")));
    Ok(0)
}

fn print_line(style: &Style, l: &Value) {
    let text = s(l, "text");
    println_out(&match s(l, "kind") {
        "step" => format!("{} {}", style.bold("⏺"), style.bold(text)),
        "cmd" => format!("  $ {text}"),
        "ok" => format!("  {}", style.green(&format!("✓ {text}"))),
        "err" => format!("  {}", style.red(&format!("✗ {text}"))),
        _ => format!("    {}", style.dim(text)),
    });
}

fn confirm(question: &str) -> Result<bool> {
    print!("{question} [y/N] ");
    std::io::stdout().flush().ok();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).map_err(|e| Error::new("internal", format!("cannot read the answer: {e}")))?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// An agent works out the setup in a container of its own, the scripts it
/// wrote are checked from scratch, and a person approves the result.
async fn setup(ctx: &Ctx, a: EnvSetupArgs) -> Result<i32> {
    if !a.yes && !ctx.tty.interactive() {
        return Err(Error::usage("opc env setup asks before it applies: run it in a terminal, or pass --yes"));
    }
    let style = Style { color: ctx.color };
    let text = !a.json;
    let body = if a.message.is_empty() { json!({}) } else { json!({"message": a.message.join(" ")}) };
    let got = ctx.http.post("/environment/setups", &body, None).await?;
    let id = s(&got["setup"], "id").to_string();
    let path = format!("/environment/setups/{id}");
    let mut after = 0u64;
    let mut asked = false;
    loop {
        let got = ctx.http.get(&path, &q(&[("after", &after.to_string())])).await?;
        let setup = &got["setup"];
        for l in setup["lines"].as_array().into_iter().flatten() {
            if text {
                print_line(&style, l);
            }
        }
        after = setup["next"].as_u64().unwrap_or(after);
        match s(setup, "status") {
            "needs_approval" if !asked => {
                asked = true;
                let p = &setup["proposal"];
                if text {
                    println_out("");
                    println_out(&style.bold(s(p, "summary")));
                    match p["changes"].as_array() {
                        Some(changes) => changes.iter().filter_map(Value::as_str).for_each(|c| println_out(&format!("  {c}"))),
                        None => contents(&style, p),
                    }
                    println_out("");
                }
                let yes = a.yes || confirm("Start new sessions from this?")?;
                ctx.http.post(&format!("{path}/{}", if yes { "approve" } else { "reject" }), &json!({}), None).await?;
                if !yes {
                    ctx.note("not applied; the environment is unchanged");
                    return Ok(1);
                }
            }
            "ready" => break,
            "rejected" => return Err(Error::new("cancelled", "the setup was abandoned; the environment is unchanged")),
            _ => {}
        }
    }
    let env = ctx.http.get("/environment", &[]).await?;
    if a.json {
        print_json(ctx, &env);
    } else {
        println_out(&format!("{}  New sessions start from this. Running sessions keep what they started with.", style.green("Ready.")));
    }
    Ok(0)
}
