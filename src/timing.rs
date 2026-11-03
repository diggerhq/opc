//! `--timing`: where the time went, measured with a monotonic clock.
//!
//! Durations only cross between our clock and the servers' (`Server-Timing`,
//! `timing.serverUs`, `firstTokenMs`); the clocks are never compared.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default)]
pub struct Split {
    pub total: Duration,
    pub ours: Duration,
    pub server: Option<Duration>,
    pub model: Option<Duration>,
}

impl Split {
    pub fn network(&self) -> Duration {
        self.total.saturating_sub(self.ours).saturating_sub(self.server.unwrap_or_default()).saturating_sub(self.model.unwrap_or_default())
    }
}

#[derive(Debug, Clone, Default)]
pub struct TurnTiming {
    pub turn_id: String,
    pub accepted: Option<Split>,
    pub first_token: Option<Split>,
    pub streaming: Option<Duration>,
    /// `opc -p`: from launch to the turn's first output, and to its end.
    pub first_output: Option<Duration>,
    pub ended: Option<Duration>,
}

#[derive(Debug, Default)]
struct Data {
    prompt: Option<Duration>,
    create: Option<Duration>,
    connect: Option<(Duration, bool)>,
    ready: Option<Duration>,
    agent_start: Option<(String, Option<Duration>)>,
    turn: Option<TurnTiming>,
    worst_frame: Option<Duration>,
    pings: VecDeque<Duration>,
    edge: Option<String>,
    requests: Vec<(String, Duration, Option<Duration>)>,
}

pub struct Timing {
    pub start: Instant,
    data: Mutex<Data>,
}

static TIMING: OnceLock<Timing> = OnceLock::new();

/// Call first thing in `main`.
pub fn init() {
    let _ = TIMING.set(Timing { start: Instant::now(), data: Mutex::new(Data::default()) });
}

pub fn get() -> &'static Timing {
    TIMING.get_or_init(|| Timing { start: Instant::now(), data: Mutex::new(Data::default()) })
}

pub fn since_start() -> Duration {
    get().start.elapsed()
}

impl Timing {
    fn with<R>(&self, f: impl FnOnce(&mut Data) -> R) -> R {
        let mut d = self.data.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut d)
    }

    pub fn mark_prompt(&self) {
        let t = self.start.elapsed();
        self.with(|d| { d.prompt.get_or_insert(t); });
    }
    pub fn mark_create(&self, took: Duration) {
        self.with(|d| d.create = Some(took));
    }
    pub fn mark_connect(&self, took: Duration, resumed_tls: bool) {
        self.with(|d| d.connect = Some((took, resumed_tls)));
    }
    pub fn mark_ready(&self) {
        let t = self.start.elapsed();
        self.with(|d| { d.ready.get_or_insert(t); });
    }
    pub fn agent_start(&self, kind: &str, took: Option<Duration>) {
        self.with(|d| d.agent_start = Some((kind.to_string(), took)));
    }
    pub fn turn(&self, f: impl FnOnce(&mut TurnTiming)) {
        self.with(|d| f(d.turn.get_or_insert_with(TurnTiming::default)));
    }
    pub fn new_turn(&self, turn_id: &str) {
        self.with(|d| d.turn = Some(TurnTiming { turn_id: turn_id.to_string(), ..Default::default() }));
    }
    pub fn frame(&self, took: Duration) {
        self.with(|d| {
            if d.worst_frame.is_none_or(|w| took > w) {
                d.worst_frame = Some(took);
            }
        });
    }
    pub fn ping(&self, rtt: Duration) {
        self.with(|d| {
            if d.pings.len() == 20 {
                d.pings.pop_front();
            }
            d.pings.push_back(rtt);
        });
    }
    pub fn rtt(&self) -> Option<Duration> {
        self.with(|d| median(d.pings.iter().copied().collect()))
    }
    pub fn edge(&self, edge: &str) {
        let edge = edge.trim();
        if !edge.is_empty() {
            self.with(|d| d.edge = Some(edge.to_string()));
        }
    }
    pub fn request(&self, label: &str, took: Duration, server: Option<Duration>) {
        self.with(|d| {
            if d.requests.len() < 64 {
                d.requests.push((label.to_string(), took, server));
            }
        });
    }

    /// `opc -p`: an event or delta of `turn_id` arrived. Records the first
    /// output and the end of the turn `new_turn` named, from launch.
    pub fn turn_step(&self, turn_id: Option<&str>, kind: &str) {
        let t = self.start.elapsed();
        self.with(|d| {
            let Some(turn) = d.turn.as_mut().filter(|x| Some(x.turn_id.as_str()) == turn_id) else { return };
            if crate::telemetry::is_output(kind) {
                turn.first_output.get_or_insert(t);
            }
            if matches!(kind, "turn.completed" | "turn.failed" | "turn.cancelled") {
                turn.ended.get_or_insert(t);
            }
        });
    }

    /// The report printed on exit, as in `docs/reference/timing.mdx`.
    pub fn report(&self) -> String {
        self.with(|d| render(d, self.start.elapsed()))
    }
}

/// Where every value ends, so that values right-align in one column.
const VALUE_END: usize = 49;

/// A titled group of rows; printed only when it has rows.
struct Section {
    out: String,
}

impl Section {
    fn new() -> Section {
        Section { out: String::new() }
    }

    /// `label`, then `value` ending at [`VALUE_END`], then `note` after it.
    fn row(&mut self, indent: usize, label: &str, value: &str, note: &str) {
        let value = value.trim();
        if value.is_empty() {
            return;
        }
        let pad = VALUE_END.saturating_sub(indent + label.chars().count() + value.chars().count()).max(1);
        let line = format!("{:indent$}{label}{:pad$}{value}{note}", "", "");
        let _ = writeln!(self.out, "{}", line.trim_end());
    }

    /// A line under the row before it.
    fn detail(&mut self, text: &str) {
        let _ = writeln!(self.out, "    {}", text.trim_end());
    }

    fn print_under(self, out: &mut String, title: &str) {
        if !self.out.is_empty() {
            let _ = writeln!(out, "{title}");
            out.push_str(&self.out);
        }
    }
}

fn render(d: &Data, total: Duration) -> String {
    let mut out = String::new();

    let mut launch = Section::new();
    if let Some(p) = d.prompt {
        launch.row(2, "process start → prompt", &ms(p), "");
    }
    let parallel = d.create.is_some() && d.connect.is_some();
    if let Some(c) = d.create {
        launch.row(2, "session create (API)", &ms(c), if parallel { "   ┐ in parallel" } else { "" });
    }
    if let Some((c, resumed)) = d.connect {
        let note = if parallel { "   ┘" } else if resumed { "   TLS resumed" } else { "" };
        launch.row(2, "connection open + attach", &ms(c), note);
    }
    if let Some(r) = d.ready {
        launch.row(2, "launch → ready", &ms(r), "");
    }
    if let Some((kind, took)) = &d.agent_start {
        let v = match took {
            Some(t) => format!("{kind} · {}", ms(*t)),
            None => kind.clone(),
        };
        launch.row(2, "agent start", &v, "");
    }
    launch.print_under(&mut out, "launch");

    let mut turn = Section::new();
    if let Some(t) = &d.turn {
        let split = |sec: &mut Section, s: &Split| {
            let mut parts = vec![format!("ours {}", ms(s.ours))];
            if let Some(sv) = s.server {
                parts.push(format!("server {}", ms(sv)));
            }
            parts.push(format!("network {}", ms(s.network())));
            if let Some(m) = s.model {
                parts.push(format!("model {}", ms(m)));
            }
            sec.detail(&parts.join(" · "));
        };
        if let Some(a) = &t.accepted {
            turn.row(2, "enter → accepted", &ms(a.total), "");
            split(&mut turn, a);
        }
        if let Some(f) = &t.first_token {
            turn.row(2, "enter → first token", &ms(f.total), "");
            split(&mut turn, f);
        }
        if let Some(s) = t.streaming {
            turn.row(2, "first token → last token", &ms(s), "");
        }
        if let Some(f) = t.first_output {
            turn.row(2, "launch → first output", &ms(f), "");
        }
        if let Some(e) = t.ended {
            turn.row(2, "launch → turn ended", &ms(e), "");
        }
        if let Some(w) = d.worst_frame {
            turn.row(2, "render, worst frame", &ms(w), "");
        }
        turn.print_under(&mut out, &format!("last turn ({})", crate::ids::short(&t.turn_id)));
    } else if let Some(w) = d.worst_frame {
        let mut top = Section::new();
        top.row(0, "render, worst frame", &ms(w), "");
        out.push_str(&top.out);
    }

    let mut requests = Section::new();
    for (label, took, server) in &d.requests {
        let note = server.map(|s| format!(" (server {})", ms(s))).unwrap_or_default();
        requests.row(2, label, &ms(*took), &note);
    }
    requests.print_under(&mut out, "requests");

    let mut connection = Section::new();
    if let Some(r) = median(d.pings.iter().copied().collect()) {
        connection.row(2, &format!("round trip (median of last {})", d.pings.len()), &ms(r), "");
    }
    if let Some(e) = &d.edge {
        connection.row(2, "edge", e, "");
    }
    connection.print_under(&mut out, "connection");

    let mut last = Section::new();
    last.row(0, "total", &ms(total), "");
    out.push_str(&last.out);
    out
}

pub fn median(mut v: Vec<Duration>) -> Option<Duration> {
    if v.is_empty() {
        return None;
    }
    v.sort();
    Some(v[v.len() / 2])
}

/// `4 ms`, `92 ms`, `1.39 s`, `0.4 ms`.
pub fn ms(d: Duration) -> String {
    let m = d.as_secs_f64() * 1000.0;
    if m >= 1000.0 {
        format!("{:.2} s", m / 1000.0)
    } else if m >= 10.0 || m == 0.0 {
        format!("{m:.0} ms")
    } else {
        format!("{m:.1} ms")
    }
}

/// Parses `Server-Timing`: the `total` entry's `dur`, else the sum of all.
pub fn parse_server_timing(header: &str) -> Option<Duration> {
    let mut total = None;
    let mut sum = 0.0;
    let mut any = false;
    for entry in header.split(',') {
        let mut parts = entry.split(';').map(str::trim);
        let name = parts.next().unwrap_or("");
        let dur = parts.find_map(|p| p.strip_prefix("dur=")).and_then(|v| v.trim_matches('"').parse::<f64>().ok());
        if let Some(d) = dur {
            any = true;
            sum += d;
            if name == "total" {
                total = Some(d);
            }
        }
    }
    let msf = total.or(any.then_some(sum))?;
    Some(Duration::from_secs_f64((msf / 1000.0).max(0.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_timing() {
        assert_eq!(parse_server_timing("edge;dur=1, total;dur=3"), Some(Duration::from_millis(3)));
        assert_eq!(parse_server_timing("a;dur=1.5, b;dur=0.5"), Some(Duration::from_millis(2)));
        assert_eq!(parse_server_timing("cache;desc=hit"), None);
    }

    #[test]
    fn formats() {
        assert_eq!(ms(Duration::from_millis(92)), "92 ms");
        assert_eq!(ms(Duration::from_millis(1390)), "1.39 s");
        assert_eq!(ms(Duration::from_micros(4200)), "4.2 ms");
    }

    #[test]
    fn split_network_is_the_rest() {
        let s = Split {
            total: Duration::from_millis(84),
            ours: Duration::from_millis(1),
            server: Some(Duration::from_millis(3)),
            model: None,
        };
        assert_eq!(s.network(), Duration::from_millis(80));
    }

    fn ms_(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// The column where each line's value ends, ignoring the note after it.
    fn value_ends(report: &str) -> Vec<usize> {
        report
            .lines()
            .filter(|l| l.contains(" ms") || l.contains(" s"))
            .filter(|l| !l.starts_with("    "))
            .map(|l| {
                let at = l.find(" ms").map(|i| i + 3).or_else(|| l.find(" s").map(|i| i + 2)).unwrap();
                l[..at].chars().count()
            })
            .collect()
    }

    #[test]
    fn empty_sections_are_left_out() {
        // As `opc -p` reported it: a turn with no rows and an edge with no name.
        let d = Data {
            create: Some(ms_(398)),
            turn: Some(TurnTiming { turn_id: "tn_jke0".into(), ..Default::default() }),
            edge: Some(String::new()),
            requests: vec![("POST /sessions".into(), ms_(398), Some(ms_(241)))],
            ..Default::default()
        };
        let r = render(&d, ms_(1670));
        assert!(!r.contains("last turn"), "{r}");
        assert!(!r.contains("connection"), "{r}");
        assert!(!r.contains("edge"), "{r}");
        assert_eq!(r.lines().next(), Some("launch"));
        assert!(r.lines().all(|l| l == l.trim_end()), "trailing whitespace: {r:?}");

        let r = render(&Data::default(), ms_(5));
        assert_eq!(r.lines().count(), 1, "{r}");
        assert!(r.starts_with("total "));
    }

    #[test]
    fn values_right_align_in_one_column() {
        let d = Data {
            prompt: Some(ms_(4)),
            create: Some(ms_(398)),
            connect: Some((ms_(87), false)),
            ready: Some(ms_(414)),
            agent_start: Some(("warm".into(), Some(ms_(11)))),
            turn: Some(TurnTiming { turn_id: "tn_jke0".into(), first_output: Some(ms_(1200)), ended: Some(ms_(1600)), ..Default::default() }),
            pings: [ms_(80)].into(),
            edge: Some("fra".into()),
            requests: vec![("POST /sessions".into(), ms_(398), Some(ms_(241)))],
            ..Default::default()
        };
        let r = render(&d, ms_(1670));
        let ends = value_ends(&r);
        assert!(ends.len() >= 9, "{r}");
        assert!(ends.iter().all(|e| *e == VALUE_END), "{ends:?}\n{r}");
        assert!(r.contains("   ┐ in parallel\n"), "{r}");
        assert!(r.contains("last turn (jke0)\n  launch → first output"), "{r}");
        assert!(r.contains("\nconnection\n"), "{r}");
        let edge = r.lines().find(|l| l.trim_start().starts_with("edge")).unwrap();
        assert_eq!(edge.chars().count(), VALUE_END);
        assert!(r.lines().all(|l| l == l.trim_end()), "trailing whitespace: {r:?}");
    }

    #[test]
    fn turn_steps_record_only_the_named_turn() {
        let t = Timing { start: Instant::now(), data: Mutex::new(Data::default()) };
        t.turn_step(Some("tn_a"), "message.completed");
        assert!(t.with(|d| d.turn.is_none()));
        t.new_turn("tn_a");
        t.turn_step(Some("tn_b"), "message.completed");
        t.turn_step(Some("tn_a"), "turn.started");
        assert!(t.with(|d| d.turn.as_ref().unwrap().first_output.is_none()));
        t.turn_step(Some("tn_a"), "message.delta");
        t.turn_step(Some("tn_a"), "turn.completed");
        let (f, e) = t.with(|d| (d.turn.as_ref().unwrap().first_output, d.turn.as_ref().unwrap().ended));
        assert!(f.is_some() && e.is_some() && f <= e);
    }
}
