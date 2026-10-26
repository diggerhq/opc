//! The interactive screen end to end: opc in a pseudo-terminal against the
//! fake Computer, driven with keys, as a person would.

mod support;

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::{Fake, Seed};

const T: Duration = Duration::from_secs(10);

struct Pty {
    child: Child,
    master: std::fs::File,
    out: Arc<Mutex<Vec<u8>>>,
}

impl Pty {
    fn spawn(fake: &Fake, args: &[&str]) -> Pty {
        let (mut m, mut s) = (0, 0);
        let ws = libc::winsize { ws_row: 40, ws_col: 120, ws_xpixel: 0, ws_ypixel: 0 };
        // SAFETY: openpty fills in two descriptors we then own.
        let r = unsafe { libc::openpty(&mut m, &mut s, std::ptr::null_mut(), std::ptr::null(), &ws) };
        assert_eq!(r, 0, "openpty");
        // SAFETY: both are fresh descriptors from openpty.
        let (master, slave) = unsafe { (OwnedFd::from_raw_fd(m), OwnedFd::from_raw_fd(s)) };
        let mut c = Command::new(env!("CARGO_BIN_EXE_opc"));
        c.args(args).env_remove("OPC_LOG").env("TERM", "xterm-256color");
        for (k, v) in fake.env() {
            c.env(k, v);
        }
        c.stdin(Stdio::from(slave.try_clone().unwrap())).stdout(Stdio::from(slave.try_clone().unwrap())).stderr(Stdio::from(slave));
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            c.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = c.spawn().expect("spawn opc");
        let master = std::fs::File::from(master);
        let out = Arc::new(Mutex::new(Vec::new()));
        let (mut rd, mut wr, o2) = (master.try_clone().unwrap(), master.try_clone().unwrap(), out.clone());
        std::thread::spawn(move || {
            let mut buf = [0u8; 65536];
            while let Ok(n) = rd.read(&mut buf) {
                if n == 0 {
                    break;
                }
                // Answer the device-attributes query, as terminals do.
                if buf[..n].windows(3).any(|w| w == b"\x1b[c") {
                    let _ = wr.write_all(b"\x1b[?62;22c");
                }
                o2.lock().unwrap().extend_from_slice(&buf[..n]);
            }
        });
        Pty { child, master, out }
    }

    /// Everything printed so far, escape sequences removed.
    fn screen(&self) -> String {
        let raw = String::from_utf8_lossy(&self.out.lock().unwrap()).to_string();
        strip(&raw)
    }

    fn wait_for(&self, what: &str) {
        let end = Instant::now() + T;
        while Instant::now() < end {
            if self.screen().contains(what) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("{what:?} never appeared on the screen:\n{}", self.screen());
    }

    /// Waits for a line holding `what` that is not the echo of a message
    /// typed as `reply <what>`.
    fn wait_reply(&self, what: &str) {
        let end = Instant::now() + T;
        while Instant::now() < end {
            if self.screen().lines().any(|l| l.contains(what) && !l.contains("reply ")) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the reply {what:?} never appeared on the screen:\n{}", self.screen());
    }

    fn keys(&mut self, k: &[u8]) {
        self.master.write_all(k).unwrap();
        self.master.flush().unwrap();
    }

    fn wait_exit(&mut self) -> i32 {
        let end = Instant::now() + T;
        while Instant::now() < end {
            if let Some(s) = self.child.try_wait().unwrap() {
                return s.code().unwrap_or(-1);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        panic!("opc did not exit:\n{}", self.screen());
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = self.master.as_raw_fd();
    }
}

fn strip(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            match it.next() {
                Some('[') => {
                    for c in it.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(c) = it.next() {
                        if c == '\x07' || (c == '\x1b' && it.peek() == Some(&'\\')) {
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

/// The one session the fake has, waiting for it to be created.
fn only_session(fake: &Fake) -> String {
    let end = Instant::now() + T;
    loop {
        let ids: Vec<String> = fake.read(|w| w.sessions.iter().map(|s| s.id.clone()).collect());
        assert!(ids.len() <= 1, "sessions: {ids:?}");
        if let Some(i) = ids.first() {
            return i.clone();
        }
        assert!(Instant::now() < end, "no session was created");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn new_session_replies_and_detaches() {
    let fake = Fake::start();
    let mut p = Pty::spawn(&fake, &["reply hello from the fake"]);
    let sid = only_session(&fake);
    p.wait_reply("hello from the fake");
    assert!(fake.wait_idle(&sid, T));
    p.keys(b"\x04");
    assert_eq!(p.wait_exit(), 0);
    p.wait_for("[detached from");
    assert_eq!(fake.turns(&sid).len(), 1);
    fake.assert_no_panics();
}

#[test]
fn typed_message_is_sent_as_a_turn() {
    let fake = Fake::start();
    let mut p = Pty::spawn(&fake, &[]);
    p.wait_for("type a message");
    std::thread::sleep(Duration::from_millis(300));
    p.keys(b"reply typed and sent\r");
    let sid = only_session(&fake);
    p.wait_reply("typed and sent");
    assert!(fake.wait_idle(&sid, T));
    p.keys(b"\x04");
    assert_eq!(p.wait_exit(), 0);
    assert_eq!(fake.turns(&sid).len(), 1);
}

#[test]
fn approval_is_answered_with_a_key() {
    let fake = Fake::start();
    let mut p = Pty::spawn(&fake, &["approve"]);
    let sid = only_session(&fake);
    let ask = fake.wait_ask(&sid, T).expect("an ask");
    p.wait_for("rm -rf build");
    // Past the guard that keeps typing from answering by accident.
    std::thread::sleep(Duration::from_millis(900));
    p.keys(b"y");
    assert!(fake.wait_idle(&sid, T), "the turn did not finish; asks {:?} frames {:?} calls {:?}", fake.asks(&sid), fake.ws_frames().iter().filter(|f| f.dir != support::Dir::In).filter_map(|f| f.text.clone()).filter(|t| !t.contains("\"method\"")).collect::<Vec<_>>(), fake.ws_calls().iter().map(|(m, p)| format!("{m} {p}")).collect::<Vec<_>>());
    let a = fake.asks(&sid).into_iter().find(|a| a["id"] == ask["id"]).unwrap();
    assert_eq!(a["status"], "answered", "{a}");
    p.keys(b"\x04");
    assert_eq!(p.wait_exit(), 0);
}

#[test]
fn attach_shows_history_and_ctrl_c_twice_detaches() {
    let fake = Fake::start();
    let sid = fake.seed_session(Seed { turns: vec!["reply the earlier answer".into()], ..Seed::new("seeded") });
    let mut p = Pty::spawn(&fake, &["a", &sid]);
    p.wait_for("the earlier answer");
    p.keys(b"\x03");
    std::thread::sleep(Duration::from_millis(100));
    p.keys(b"\x03");
    assert_eq!(p.wait_exit(), 0);
    p.wait_for("[detached from \"seeded\"");
}

#[test]
fn plan_is_approved_with_y() {
    let fake = Fake::start();
    let mut p = Pty::spawn(&fake, &["plan"]);
    let sid = only_session(&fake);
    fake.wait_ask(&sid, T).expect("a plan ask");
    p.wait_for("do it");
    std::thread::sleep(Duration::from_millis(900));
    p.keys(b"y");
    assert!(fake.wait_idle(&sid, T), "the turn did not finish:\n{}", p.screen());
    let a = fake.asks(&sid).pop().unwrap();
    assert_eq!(a["status"], "answered", "{a}");
    assert_eq!(a["answer"]["decision"], "allow", "{a}");
    p.keys(b"\x04");
    assert_eq!(p.wait_exit(), 0);
}

#[test]
fn esc_interrupts_the_running_turn() {
    let fake = Fake::start();
    let mut p = Pty::spawn(&fake, &["slow 20000"]);
    let sid = only_session(&fake);
    assert!(fake.wait_for(T, {
        let sid = sid.clone();
        move |w| w.sess(&sid).is_ok_and(|s| s.active().is_some())
    }));
    std::thread::sleep(Duration::from_millis(300));
    p.keys(b"\x1b");
    assert!(fake.wait_idle(&sid, T), "Esc did not interrupt:\n{}", p.screen());
    let t = fake.turns(&sid).pop().unwrap();
    assert_eq!(t["status"], "cancelled", "{t}");
    p.keys(b"\x04");
    assert_eq!(p.wait_exit(), 0);
}
