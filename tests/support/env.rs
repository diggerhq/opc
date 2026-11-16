//! The Computer's environment: what every new session starts with. Not in
//! the vendored schema yet (the server does not have it), so these objects
//! are not validated; the shapes here are the proposal `opc env` is built
//! against. Nothing is installed: a setup is a script of timed lines.

use super::model::*;
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
pub struct EnvState {
    /// Oldest first. The last one is what new sessions start from.
    pub revisions: Vec<Value>,
    pub setups: Vec<Setup>,
}

pub struct Setup {
    pub id: String,
    /// working, checking, needs_approval, applying, ready, rejected.
    pub status: String,
    pub lines: Vec<Value>,
    pub proposal: Value,
    pub by: Value,
}

impl EnvState {
    pub fn current(&self) -> Option<&Value> {
        self.revisions.last()
    }
    /// Whether the scripted agent finds `tool` installed.
    pub fn has(&self, tool: &str) -> bool {
        self.current().and_then(|r| r["install"].as_str()).is_some_and(|i| i.contains(tool))
    }
    fn setup(&self, id: &str) -> ApiResult<&Setup> {
        self.setups.iter().find(|s| s.id == id).ok_or_else(|| ApiErr::new(404, "not_found", format!("no setup {id}")))
    }
    fn setup_mut(&mut self, id: &str) -> Option<&mut Setup> {
        self.setups.iter_mut().find(|s| s.id == id)
    }
    fn to_json(&self) -> Value {
        let applying = self.setups.iter().any(|s| s.status == "applying");
        let status = match (self.current(), applying) {
            (_, true) => "applying",
            (Some(_), false) => "ready",
            (None, false) => "none",
        };
        json!({"status": status, "revision": self.current(), "revisions": self.revisions.len()})
    }
}

fn setup_json(s: &Setup, after: usize) -> Value {
    let lines: Vec<&Value> = s.lines.iter().skip(after).collect();
    json!({"id": s.id, "status": s.status, "lines": lines, "next": s.lines.len(), "proposal": s.proposal})
}

const INSTALL: &str = "apt-get update\napt-get install -y postgresql-16\ncorepack enable pnpm\ncd acme-service && pnpm install --frozen-lockfile";
const START: &str = "service postgresql start\ncreatedb acme_test 2>/dev/null || true";

pub async fn route(sh: &Arc<Shared>, method: &str, segs: &[&str], q: &Map<String, Value>, body: &Value, actor: &Value) -> ApiResult<(u16, Value)> {
    match (method, segs) {
        ("GET", ["environment"]) => Ok((200, json!({"environment": sh.read(|w| w.env.to_json())}))),
        ("GET", ["environment", "history"]) => {
            let mut all = sh.read(|w| w.env.revisions.clone());
            all.reverse();
            Ok((200, json!({"revisions": all})))
        }
        ("POST", ["environment", "rollback"]) => {
            let to = body.get("to").and_then(Value::as_str).map(str::to_string);
            sh.with(|w| {
                let e = &mut w.env;
                let Some(cur) = e.current().cloned() else { return Err(ApiErr::new(409, "no_environment", "there is no environment to roll back")) };
                // The target: the named revision, else the last one with another image.
                let target = match &to {
                    Some(id) => e.revisions.iter().find(|r| r["id"] == id.as_str()).cloned().ok_or_else(|| ApiErr::new(404, "not_found", format!("no revision {id}")))?,
                    None => e
                        .revisions
                        .iter()
                        .rev()
                        .find(|r| r["image"]["id"] != cur["image"]["id"])
                        .cloned()
                        .ok_or_else(|| ApiErr::new(409, "nothing_to_roll_back", "this is the first setup: there is no earlier one"))?,
                };
                let mut rev = target.clone();
                rev["id"] = json!(new_id("rev_"));
                rev["summary"] = json!(format!("Rolled back to: {}", target["summary"].as_str().unwrap_or("")));
                rev["via"] = json!("rollback");
                rev["createdAt"] = json!(now());
                rev["createdBy"] = actor.clone();
                e.revisions.push(rev);
                Ok((200, json!({"environment": e.to_json()})))
            })
        }
        ("POST", ["environment", "setups"]) => {
            let message = body.get("message").and_then(Value::as_str).map(str::to_string);
            let id = new_id("set_");
            sh.with(|w| {
                if w.env.setups.iter().any(|s| s.status == "applying") {
                    return Err(ApiErr::new(409, "setup_in_progress", "a setup is being applied"));
                }
                // A setup nobody approved yet is abandoned for the new one.
                for s in w.env.setups.iter_mut().filter(|s| s.status != "ready") {
                    s.status = "rejected".into();
                }
                w.env.setups.push(Setup { id: id.clone(), status: "working".into(), lines: vec![], proposal: Value::Null, by: actor.clone() });
                Ok(())
            })?;
            sh.handle.spawn(run_setup(sh.clone(), id.clone(), message));
            Ok((201, json!({"setup": sh.read(|w| w.env.setup(&id).map(|s| setup_json(s, 0)))?})))
        }
        ("GET", ["environment", "setups", id]) => {
            let after = q.get("after").and_then(Value::as_str).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
            let id = id.to_string();
            sh.read(|w| w.env.setup(&id).map(|_| ()))?;
            // A long poll: until there is a line after `after`, or the setup stops moving.
            let wait = id.clone();
            sh.wait_until(Some(Duration::from_secs(2)), move |w| w.env.setup(&wait).is_ok_and(|s| s.lines.len() > after || matches!(s.status.as_str(), "needs_approval" | "ready" | "rejected")))
                .await;
            Ok((200, json!({"setup": sh.read(|w| w.env.setup(&id).map(|s| setup_json(s, after)))?})))
        }
        ("POST", ["environment", "setups", id, verdict @ ("approve" | "reject")]) => sh.with(|w| {
            let s = w.env.setup_mut(id).ok_or_else(|| ApiErr::new(404, "not_found", format!("no setup {id}")))?;
            if s.status != "needs_approval" {
                return Err(ApiErr::new(409, "not_waiting", "this setup is not waiting for approval"));
            }
            s.status = if *verdict == "approve" { "applying" } else { "rejected" }.into();
            s.by = actor.clone();
            Ok((200, json!({"setup": setup_json(s, s.lines.len())})))
        }),
        _ => Err(ApiErr::new(404, "not_found", format!("no route {method} /{}", segs.join("/")))),
    }
}

struct Script {
    sh: Arc<Shared>,
    id: String,
    unit: Duration,
}

impl Script {
    /// Appends a line, then waits `units` steps.
    async fn line(&self, kind: &str, text: &str, units: u32) {
        self.sh.with(|w| {
            if let Some(s) = w.env.setup_mut(&self.id) {
                let n = s.lines.len() + 1;
                s.lines.push(json!({"n": n, "kind": kind, "text": text}));
            }
        });
        tokio::time::sleep(self.unit * units).await;
    }
    fn status(&self, status: &str) {
        self.sh.with(|w| {
            if let Some(s) = w.env.setup_mut(&self.id).filter(|s| s.status != "rejected") {
                s.status = status.into();
            }
        });
    }
}

async fn run_setup(sh: Arc<Shared>, id: String, message: Option<String>) {
    let unit = Duration::from_millis(sh.knobs.lock().unwrap().step_ms);
    let s = Script { sh: sh.clone(), id: id.clone(), unit };
    let base = sh.read(|w| w.env.current().cloned());
    let (summary, install, start, changes, tests) = match &base {
        None => {
            s.line("step", "Reading diggerhq/acme-service", 6).await;
            s.line("out", "package.json         node 22, pnpm, test: vitest run", 3).await;
            s.line("out", "docker-compose.yml   postgres:16", 3).await;
            s.line("out", ".env.example         DATABASE_URL", 5).await;
            s.line("step", "Installing what it needs", 4).await;
            s.line("cmd", "apt-get update && apt-get install -y postgresql-16", 14).await;
            s.line("out", "Setting up postgresql-16 (16.6-1.pgdg24.04+1) ...", 4).await;
            s.line("cmd", "corepack enable pnpm && pnpm install --frozen-lockfile", 12).await;
            s.line("out", "Done in 9.3s (412 packages)", 4).await;
            s.line("step", "Running the tests", 4).await;
            s.line("cmd", "pnpm test", 10).await;
            s.line("err", "14 failed, 28 passed: connect ECONNREFUSED 127.0.0.1:5432", 8).await;
            s.line("step", "Postgres is installed but not running: starting it belongs in the start script", 8).await;
            s.line("cmd", "service postgresql start && createdb acme_test", 8).await;
            s.line("cmd", "pnpm test", 12).await;
            s.line("ok", "42 passed (6.1s)", 6).await;
            ("Postgres 16 and Node dependencies for acme-service".to_string(), INSTALL.to_string(), START.to_string(), Value::Null, 42)
        }
        Some(rev) => {
            let what = message.clone().unwrap_or_else(|| "add redis".into());
            s.line("step", "Starting from the current setup", 5).await;
            s.line("step", &format!("Change asked for: {what}"), 5).await;
            s.line("cmd", "apt-get install -y redis-server", 12).await;
            s.line("out", "Setting up redis-server (5:7.0.15-1build2) ...", 4).await;
            s.line("cmd", "service redis-server start && redis-cli ping", 6).await;
            s.line("out", "PONG", 4).await;
            s.line("cmd", "pnpm test", 12).await;
            s.line("ok", "42 passed (6.0s)", 6).await;
            let install = format!("{}\napt-get install -y redis-server", rev["install"].as_str().unwrap_or(""));
            let start = format!("{}\nservice redis-server start", rev["start"].as_str().unwrap_or(""));
            let changes = json!(["install  + apt-get install -y redis-server", "start    + service redis-server start"]);
            ("Add Redis".to_string(), install, start, changes, 42)
        }
    };
    // The check that counts: the scripts alone, from the base image, without secrets.
    s.status("checking");
    s.line("step", "Checking the scripts from scratch: a clean container, no secrets", 6).await;
    s.line("out", "install   1m 38s", 10).await;
    s.line("out", "start     2s", 4).await;
    s.line("out", "pnpm test", 8).await;
    s.line("ok", &format!("{tests} passed in the clean container"), 4).await;
    let proposal = json!({
        "summary": summary, "install": install, "start": start, "changes": changes,
        "repos": [{"name": "diggerhq/acme-service", "default": true}],
        "secrets": ["DATABASE_URL"],
        "check": {"command": "pnpm test", "passed": tests, "durationMs": 112_000},
    });
    sh.with(|w| {
        if let Some(x) = w.env.setup_mut(&id).filter(|x| x.status != "rejected") {
            x.proposal = proposal.clone();
            x.status = "needs_approval".into();
        }
    });
    let wait = id.clone();
    sh.wait_until(None, move |w| w.env.setup(&wait).is_ok_and(|x| x.status != "needs_approval")).await;
    if sh.read(|w| w.env.setup(&id).is_ok_and(|x| x.status == "rejected")) {
        return;
    }
    let image = format!("img_{}", rand_str(8));
    s.line("step", "Saving the image", 12).await;
    s.line("out", &format!("{image}   1.4 GB"), 4).await;
    sh.with(|w| {
        let by = w.env.setup(&id).map(|x| x.by.clone()).unwrap_or(Value::Null);
        let mut rev = proposal.clone();
        rev["id"] = json!(new_id("rev_"));
        rev["via"] = json!("setup");
        rev["createdAt"] = json!(now());
        rev["createdBy"] = by;
        rev["image"] = json!({"id": image, "sizeBytes": 1_400_000_000u64, "buildMs": 98_000});
        w.env.revisions.push(rev);
        if let Some(x) = w.env.setup_mut(&id) {
            x.status = "ready".into();
        }
    });
}
