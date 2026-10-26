//! opc against the in-process fake Computer: exit codes, JSON validated
//! against the schema, behaviour without a terminal, reconnects and backoff.

mod support;

use serde_json::Value;
use std::time::Duration;
use support::{Failure, Fake, Seed};

const T: Duration = Duration::from_secs(10);

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Out {
    fn json(&self) -> Value {
        serde_json::from_str(self.stdout.trim()).unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?}\nstderr: {}", self.stdout, self.stderr))
    }
    fn lines(&self) -> Vec<Value> {
        self.stdout.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l}"))).collect()
    }
    fn err_json(&self) -> Value {
        let line = self.stderr.lines().find(|l| l.starts_with("{\"error\"")).unwrap_or_else(|| panic!("no error JSON on stderr: {}", self.stderr));
        serde_json::from_str(line).unwrap()
    }
}

fn run(fake: &Fake, args: &[&str]) -> Out {
    run_in(fake, args, None)
}

fn run_in(fake: &Fake, args: &[&str], stdin: Option<&str>) -> Out {
    let mut c = fake.opc();
    c.args(args);
    if let Some(s) = stdin {
        c.write_stdin(s);
    }
    let o = c.output().expect("run opc");
    Out { code: o.status.code().unwrap_or(-1), stdout: String::from_utf8_lossy(&o.stdout).into(), stderr: String::from_utf8_lossy(&o.stderr).into() }
}

/// opc's own output must match the published schema.
fn check(name: &str, v: &Value) {
    if let Err(e) = support::check(name, v) {
        panic!("opc printed an object that does not match the schema: {e}");
    }
}

#[test]
fn print_replies_and_exits_zero() {
    let fake = Fake::start();
    let o = run(&fake, &["-p", "reply hello there"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.stdout.trim(), "hello there");
    fake.assert_no_panics();
}

// ---- -p and the Result --------------------------------------------------

#[test]
fn print_json_result_validates() {
    let fake = Fake::start();
    let o = run(&fake, &["-p", "--output-format", "json", "reply ok"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let r = o.json();
    check("Result", &r);
    assert_eq!(r["subtype"], "success");
    assert_eq!(r["result"], "ok");
    assert!(r["sessionId"].as_str().unwrap().starts_with("cs_"));
    // The session id is minted on the client.
    let creates = fake.requests_to("POST /sessions");
    assert_eq!(creates.len(), 1);
    assert_eq!(creates[0].json()["id"], r["sessionId"]);
    fake.assert_no_panics();
}

#[test]
fn print_stream_json_has_init_and_result_lines() {
    let fake = Fake::start();
    let o = run(&fake, &["-p", "--output-format", "stream-json", "tool"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let lines = o.lines();
    assert_eq!(lines[0]["type"], "system", "{:?}", lines[0]);
    let last = lines.last().unwrap();
    assert_eq!(last["type"], "result");
    check("Result", last);
    for l in &lines[1..lines.len() - 1] {
        if l["type"] == "event" || l.get("seq").is_some() {
            let e = l.get("event").unwrap_or(l);
            check("Event", e);
        }
    }
}

#[test]
fn failed_turn_exits_one_with_error_result() {
    let fake = Fake::start();
    let o = run(&fake, &["-p", "--output-format", "json", "fail broken on purpose"]);
    assert_eq!(o.code, 1, "{}", o.stderr);
    let r = o.json();
    check("Result", &r);
    assert_eq!(r["isError"], true);
}

#[test]
fn an_ask_exits_five_with_the_ask() {
    let fake = Fake::start();
    let o = run(&fake, &["-p", "--output-format", "json", "approve"]);
    assert_eq!(o.code, 5, "{}", o.stderr);
    let r = o.json();
    check("Result", &r);
    assert!(r["ask"]["id"].as_str().unwrap().starts_with("ak_"), "{r}");
}

#[test]
fn silent_turn_prints_nothing() {
    let fake = Fake::start();
    let o = run(&fake, &["-p", "silent"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.stdout.trim(), "");
}

#[test]
fn message_from_stdin() {
    let fake = Fake::start();
    let o = run_in(&fake, &["-p"], Some("reply from stdin\n"));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.stdout.trim(), "from stdin");
}

#[test]
fn idempotency_key_makes_one_turn() {
    let fake = Fake::start();
    let a = run(&fake, &["-p", "--idempotency-key", "job-42", "--output-format", "json", "reply once"]);
    let b = run(&fake, &["-p", "--idempotency-key", "job-42", "--output-format", "json", "reply once"]);
    assert_eq!(a.code, 0, "{}", a.stderr);
    assert_eq!(b.code, 0, "{}", b.stderr);
    assert_eq!(a.json()["sessionId"], b.json()["sessionId"]);
    let sid = a.json()["sessionId"].as_str().unwrap().to_string();
    assert_eq!(fake.turns(&sid).len(), 1);
    assert_eq!(fake.session_ids().len(), 1);
}

#[test]
fn timeout_exits_124_and_names_the_turn() {
    let fake = Fake::start();
    let o = run(&fake, &["-p", "--timeout", "300ms", "slow 5000"]);
    assert_eq!(o.code, 124, "{}", o.stderr);
    assert!(o.stderr.contains("opc a") || o.stderr.contains("still running"), "{}", o.stderr);
}

#[test]
fn continue_a_session_with_dash_s() {
    let fake = Fake::start();
    let sid = fake.seed_session(Seed::new("existing"));
    let o = run(&fake, &["-p", "-s", &sid, "reply again"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.stdout.trim(), "again");
    assert_eq!(fake.turns(&sid).len(), 1);
    assert!(fake.requests_to("POST /sessions").is_empty());
}

// ---- errors and exit codes ------------------------------------------------

#[test]
fn rejected_key_exits_three_with_error_json() {
    let fake = Fake::start();
    fake.revoke_key();
    let o = run(&fake, &["-p", "--output-format", "json", "reply x"]);
    assert_eq!(o.code, 3, "{}", o.stderr);
    let e = o.err_json();
    check("Error", &e["error"]);
    assert_eq!(e["error"]["code"], "unauthenticated");
}

#[test]
fn no_credits_exits_six() {
    let fake = Fake::start();
    fake.fail_next("POST /sessions", 1, Failure::insufficient_credits());
    let o = run(&fake, &["-p", "--output-format", "json", "reply x"]);
    assert_eq!(o.code, 6, "{}", o.stderr);
    check("Error", &o.err_json()["error"]);
}

#[test]
fn unknown_session_is_a_usage_error() {
    let fake = Fake::start();
    let o = run(&fake, &["send", "-s", "cs_0000000000000000", "hi"]);
    assert_eq!(o.code, 2, "{}", o.stderr);
}

#[test]
fn bad_flags_exit_two() {
    let fake = Fake::start();
    let o = run(&fake, &["ls", "--no-such-flag"]);
    assert_eq!(o.code, 2);
}

#[test]
fn retries_through_unavailable_with_backoff() {
    let fake = Fake::start();
    fake.fail_next("POST /sessions", 2, Failure::unavailable(50));
    let o = run(&fake, &["-p", "reply survived"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.stdout.trim(), "survived");
    assert!(fake.requests_to("POST /sessions").len() >= 3);
}

#[test]
fn cold_computer_is_retried_with_the_same_key() {
    let fake = Fake::start();
    fake.fail_next("POST /sessions", 2, Failure::new(503, "computer_unavailable").with("retryAfterMs", 50));
    let o = run(&fake, &["-p", "--idempotency-key", "cold-1", "reply warm now"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.stdout.trim(), "warm now");
    let reqs = fake.requests_to("POST /sessions");
    assert!(reqs.len() >= 3, "{}", reqs.len());
    let keys: Vec<_> = reqs.iter().map(|r| r.idempotency_key().map(str::to_string)).collect();
    assert!(keys[0].is_some() && keys.iter().all(|k| k == &keys[0]), "{keys:?}");
    assert!(reqs.iter().all(|r| r.json() == reqs[0].json()), "the retried body changed");
}

#[test]
fn rate_limit_honours_retry_after() {
    let fake = Fake::start();
    fake.fail_next("POST /sessions", 1, Failure::rate_limited(200));
    let t = std::time::Instant::now();
    let o = run(&fake, &["-p", "reply later"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(t.elapsed() >= Duration::from_millis(200));
}

#[test]
fn reconnects_and_catches_up_without_gaps() {
    let fake = Fake::start();
    fake.drop_ws_after(6);
    let o = run(&fake, &["-p", "--output-format", "stream-json", "slow 600"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(fake.ws_connections() >= 2, "expected a reconnect");
    let seqs: Vec<u64> = o.lines().iter().filter_map(|l| l.get("event").unwrap_or(l).get("seq").and_then(Value::as_u64)).collect();
    let mut sorted = seqs.clone();
    sorted.dedup();
    assert_eq!(seqs, sorted, "duplicate or out-of-order events: {seqs:?}");
    assert_eq!(o.lines().last().unwrap()["subtype"], "success");
}

// ---- without a terminal ----------------------------------------------------

#[test]
fn login_without_a_terminal_exits_three() {
    let fake = Fake::start();
    let mut c = fake.opc();
    c.env_remove("OPENCOMPUTER_API_KEY").arg("login");
    let o = c.output().unwrap();
    assert_eq!(o.status.code(), Some(3));
}

#[test]
fn nothing_logged_in_exits_three() {
    let fake = Fake::start();
    let mut c = fake.opc();
    c.env_remove("OPENCOMPUTER_API_KEY").args(["-p", "reply x"]);
    let o = c.output().unwrap();
    assert_eq!(o.status.code(), Some(3), "{}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn no_progress_on_stderr_without_a_terminal() {
    let fake = Fake::start();
    let o = run(&fake, &["-p", "tool"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(!o.stderr.contains('⏺'), "{}", o.stderr);
    assert!(!o.stderr.contains("\x1b["), "escape codes without a terminal");
}

#[test]
fn interactive_without_a_terminal_refuses_to_guess() {
    let fake = Fake::start();
    let o = run(&fake, &[]);
    assert_ne!(o.code, 0);
    assert!(fake.session_ids().is_empty(), "no session without a terminal or a message");
}

// ---- sessions --------------------------------------------------------------

#[test]
fn new_send_wait() {
    let fake = Fake::start();
    let o = run(&fake, &["new", "--title", "scripted"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let sid = o.stdout.trim().to_string();
    assert!(sid.starts_with("cs_"), "{sid}");
    let o = run(&fake, &["send", "-s", &sid, "reply sent"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let tid = o.stdout.trim().to_string();
    assert!(tid.starts_with("tn_"), "{tid}");
    let o = run(&fake, &["wait", &tid, "--json"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let r = o.json();
    check("Result", &r);
    assert_eq!(r["result"], "sent");
}

#[test]
fn send_wait_reports_the_turn() {
    let fake = Fake::start();
    let sid = fake.seed_session(Seed::new("x"));
    let o = run(&fake, &["send", "-s", &sid, "--wait", "reply waited"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.stdout.contains("waited"), "{}", o.stdout);
}

#[test]
fn wait_any_over_many_sessions_uses_one_connection() {
    let fake = Fake::start();
    let mut ids = Vec::new();
    for i in 0..5 {
        let sid = fake.seed_session(Seed::new(&format!("s{i}")));
        fake.send(&sid, if i == 3 { "reply first" } else { "slow 3000" }, "queue").unwrap();
        ids.push(sid);
    }
    let before = fake.ws_connections();
    let mut args = vec!["wait", "--any", "--json"];
    args.extend(ids.iter().map(String::as_str));
    let o = run(&fake, &args);
    assert_eq!(o.code, 0, "{}", o.stderr);
    check("Result", &o.json());
    assert_eq!(o.json()["sessionId"], ids[3].as_str());
    assert_eq!(fake.ws_connections() - before, 1);
}

#[test]
fn ls_json_validates() {
    let fake = Fake::start();
    fake.seed_session(Seed::new("one").meta("ticket", "LIN-1"));
    fake.seed_session(Seed::new("two"));
    let o = run(&fake, &["ls", "--json"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let v = o.json();
    let arr = v.as_array().or_else(|| v["sessions"].as_array()).expect("an array of sessions").clone();
    assert_eq!(arr.len(), 2);
    for s in &arr {
        check("Session", s);
    }
    let o = run(&fake, &["ls", "--metadata", "ticket=LIN-1", "--json"]);
    let v = o.json();
    let arr = v.as_array().or_else(|| v["sessions"].as_array()).unwrap().clone();
    assert_eq!(arr.len(), 1);
}

#[test]
fn log_json_events_validate() {
    let fake = Fake::start();
    let o = run(&fake, &["-p", "--output-format", "json", "tool"]);
    let sid = o.json()["sessionId"].as_str().unwrap().to_string();
    let o = run(&fake, &["log", "-s", &sid, "--json"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let v = o.json();
    let events = v["events"].as_array().unwrap();
    assert!(events.iter().any(|e| e["type"] == "tool.completed"));
    for e in events {
        check("Event", e);
    }
}

#[test]
fn rename_and_archive() {
    let fake = Fake::start();
    let sid = fake.seed_session(Seed::new("old"));
    assert_eq!(run(&fake, &["rename", &sid, "new title"]).code, 0);
    assert_eq!(fake.session(&sid)["title"], "new title");
    assert_eq!(run(&fake, &["archive", &sid]).code, 0);
    assert_eq!(fake.session(&sid)["archived"], true);
}

#[test]
fn interrupt_cancels_the_running_turn() {
    let fake = Fake::start();
    let sid = fake.seed_session(Seed::new("busy"));
    let (tid, _) = fake.send(&sid, "slow 10000", "queue").unwrap();
    fake.wait_for(T, {
        let sid = sid.clone();
        move |w| w.sessions.iter().any(|s| s.id == sid && s.status == "running")
    });
    let o = run(&fake, &["interrupt", "-s", &sid]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(fake.wait_turn(&sid, &tid, T));
    assert_eq!(fake.turn(&sid, &tid)["status"], "cancelled");
}

// ---- asks ------------------------------------------------------------------

#[test]
fn allow_an_approval() {
    let fake = Fake::start();
    let sid = fake.seed_session(Seed::new("needs ok"));
    fake.send(&sid, "approve", "queue").unwrap();
    let ask = fake.wait_ask(&sid, T).expect("an ask");
    let id = ask["id"].as_str().unwrap();
    let o = run(&fake, &["ask", "ls", "-s", &sid, "--json"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let o = run(&fake, &["ask", "allow", id, "-s", &sid]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(fake.wait_idle(&sid, T));
    let a = fake.asks(&sid).into_iter().find(|a| a["id"] == id).unwrap();
    assert_eq!(a["status"], "answered");
    // A second answer finds it closed.
    let o = run(&fake, &["ask", "deny", id, "-s", &sid]);
    assert_ne!(o.code, 0);
    assert!(o.stderr.contains("already"), "{}", o.stderr);
}

#[test]
fn approve_a_plan_with_a_mode() {
    let fake = Fake::start();
    let sid = fake.seed_session(Seed::new("plan").mode("plan"));
    fake.send(&sid, "plan", "queue").unwrap();
    let ask = fake.wait_ask(&sid, T).expect("an ask");
    let o = run(&fake, &["ask", "allow", ask["id"].as_str().unwrap(), "-s", &sid, "--mode", "acceptEdits"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(fake.wait_for(T, {
        let sid = sid.clone();
        move |w| w.sessions.iter().any(|s| s.id == sid && s.mode == "acceptEdits")
    }));
}

#[test]
fn answer_a_question() {
    let fake = Fake::start();
    let sid = fake.seed_session(Seed::new("q"));
    fake.send(&sid, "question", "queue").unwrap();
    let ask = fake.wait_ask(&sid, T).expect("an ask");
    let o = run(&fake, &["ask", "answer", ask["id"].as_str().unwrap(), "-s", &sid, "--choice", "blue"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(fake.wait_idle(&sid, T));
}

// ---- subagents -------------------------------------------------------------

#[test]
fn wait_subagents_returns_every_subagent_ended() {
    let fake = Fake::start();
    let o = run(&fake, &["-p", "--wait-subagents", "--output-format", "json", "subagent bg 300"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let r = o.json();
    check("Result", &r);
    let subs = r["subagents"].as_array().expect("Result.subagents");
    assert!(!subs.is_empty());
    for s in subs {
        assert_ne!(s["status"], "running", "{s}");
    }
}

#[test]
fn subagents_ls_and_stop() {
    let fake = Fake::start();
    let sid = fake.seed_session(Seed::new("sa"));
    fake.send(&sid, "subagent bg 10000", "queue").unwrap();
    assert!(fake.wait_for(T, {
        let sid = sid.clone();
        move |w| w.sessions.iter().any(|s| s.id == sid && s.subagents.iter().any(|a| a["status"] == "running"))
    }));
    let o = run(&fake, &["subagents", "ls", "-s", &sid, "--json"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let v = o.json();
    let list = v["subagents"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    check("Subagent", &list[0]);
    let id = list[0]["id"].as_str().unwrap();
    let o = run(&fake, &["subagents", "stop", id, "-s", &sid]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let o = run(&fake, &["subagents", "result", id, "-s", &sid]);
    assert_eq!(o.code, 1, "a stopped subagent has no result: {}", o.stderr);
}

// ---- secrets ---------------------------------------------------------------

#[test]
fn secret_from_stdin_never_echoed() {
    let fake = Fake::start();
    let sid = fake.seed_session(Seed::new("s"));
    let o = run_in(&fake, &["secret", "set", "STRIPE_KEY", "-s", &sid], Some("sk_test_value\n"));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(!o.stdout.contains("sk_test_value") && !o.stderr.contains("sk_test_value"));
    let put = fake.requests_to("PUT /sessions/*/secrets/*");
    assert_eq!(put.len(), 1);
    assert_eq!(put[0].json()["value"], "sk_test_value");
    let o = run(&fake, &["secret", "ls", "-s", &sid, "--json"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(!o.stdout.contains("sk_test_value"));
}

// ---- offline ---------------------------------------------------------------

#[test]
fn schema_and_help_work_offline() {
    let fake = Fake::start();
    let o = run(&fake, &["schema", "result"]);
    assert_eq!(o.code, 0);
    assert_eq!(o.json()["$ref"], "#/$defs/Result");
    let o = run(&fake, &["help", "scripting"]);
    assert!(o.stdout.starts_with("Scripting"));
    assert!(fake.requests().is_empty());
}

#[test]
fn whoami_and_doctor() {
    let fake = Fake::start();
    let o = run(&fake, &["whoami", "--json"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.json()["person"].is_object());
    let o = run(&fake, &["doctor", "--json"]);
    assert_eq!(o.code, 0, "{}\n{}", o.stdout, o.stderr);
    let steps = o.json()["steps"].as_array().unwrap().clone();
    assert!(steps.iter().all(|s| s["ok"] == true), "{steps:?}");
    assert_eq!(steps.last().unwrap()["name"], "round trip");
}

#[test]
fn messages_that_look_like_commands() {
    let fake = Fake::start();
    let o = run(&fake, &["-p", "ls the files"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
}
