#!/usr/bin/env python3
"""opc latency gates: every row of the budget table in docs/reference/timing.mdx.

Runs a built `opc` against an OpenComputer API (staging in CI) N times per
row and checks that the median meets the target and the 95th percentile is
within 1.5 times it. Targets that depend on the network are expressed in
round trips, measured first with `opc doctor --json`.

    OPENCOMPUTER_API_KEY=... OPENCOMPUTER_API_URL=https://staging... \\
        bench/bench.py --opc target/release/opc --runs 200 --region lhr

The key is read from the environment only, never from the command line.
Results are printed as one JSON object per row, and sent to Tinybird when
TINYBIRD_URL and TINYBIRD_TOKEN are set. Exit status 1 when a gate fails.

`--local` runs only the rows that need no server (launch → prompt), against
an address that never answers: usable on any machine and in every CI run.
"""

import argparse
import fcntl
import json
import os
import pty
import re
import select
import shutil
import signal
import statistics
import struct
import subprocess
import sys
import tempfile
import termios
import time
import urllib.request


def pct(values, p):
    s = sorted(values)
    if not s:
        return None
    k = min(len(s) - 1, max(0, round(p / 100 * (len(s) - 1))))
    return s[k]


class Bench:
    def __init__(self, args):
        self.args = args
        self.opc = os.path.abspath(args.opc)
        self.tmp = tempfile.mkdtemp(prefix="opc-bench-")
        self.env = dict(os.environ)
        self.env.setdefault("NO_COLOR", "1")
        if args.local:
            # A black hole: the prompt must not wait for the network.
            self.env["OPENCOMPUTER_API_URL"] = "https://10.255.255.1"
            self.env["OPENCOMPUTER_API_KEY"] = "bench-local-not-a-key"
            self.env["OPENCOMPUTER_CONFIG"] = os.path.join(self.tmp, "config.json")
        self.rows = []
        self.failed = False
        self.rtt = None
        self.session = None

    # ---- running opc -------------------------------------------------

    def run(self, argv, stdin=None, timeout=300):
        tfile = os.path.join(self.tmp, "timing.json")
        if os.path.exists(tfile):
            os.unlink(tfile)
        env = dict(self.env, OPC_TIMING_FILE=tfile)
        t0 = time.monotonic()
        p = subprocess.run([self.opc, *argv], input=stdin, capture_output=True, env=env, timeout=timeout)
        wall = (time.monotonic() - t0) * 1000
        timing = json.load(open(tfile)) if os.path.exists(tfile) else {}
        return p, wall, timing

    def interactive(self, argv, settle=0.0, timeout=30):
        """Runs opc in a pseudo-terminal, waits for the prompt (and for
        `settle` more seconds), detaches with Ctrl+D and returns the timing."""
        tfile = os.path.join(self.tmp, "timing.json")
        if os.path.exists(tfile):
            os.unlink(tfile)
        env = dict(self.env, OPC_TIMING_FILE=tfile, TERM="xterm-256color", COLUMNS="120", LINES="40")
        t0 = time.monotonic()
        pid, fd = pty.fork()
        if pid == 0:
            os.execve(self.opc, [self.opc, *argv], env)
        # A terminal of a usual size that answers the device-attributes query
        # crossterm sends, as real terminals do.
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
        out = b""
        deadline = time.monotonic() + timeout
        seen_at = None
        first = None
        try:
            while time.monotonic() < deadline:
                r, _, _ = select.select([fd], [], [], 0.05)
                if r:
                    try:
                        chunk = os.read(fd, 65536)
                    except OSError:
                        break
                    if not chunk:
                        break
                    out += chunk
                    if b"\x1b[c" in chunk:
                        os.write(fd, b"\x1b[?62;22c")
                    if seen_at is None and b"type a message" in out:
                        seen_at = time.monotonic()
                        first = (seen_at - t0) * 1000
                if seen_at is not None and time.monotonic() - seen_at >= max(settle, 0.2):
                    os.write(fd, b"\x04")
                    seen_at = float("inf")
                    deadline = min(deadline, time.monotonic() + 5)
        finally:
            try:
                done, _ = os.waitpid(pid, os.WNOHANG)
                if done == 0:
                    os.kill(pid, signal.SIGTERM)
                    os.waitpid(pid, 0)
            except (ChildProcessError, ProcessLookupError):
                pass
            os.close(fd)
        t = json.load(open(tfile)) if os.path.exists(tfile) else {}
        t["wallPrompt"] = first
        return t

    # ---- gates -------------------------------------------------------

    def gate(self, row, values, target, unit="ms", higher_is_better=False):
        values = [v for v in values if v is not None]
        rec = {
            "row": row,
            "n": len(values),
            "unit": unit,
            "target": round(target, 2) if target is not None else None,
            "p50": round(statistics.median(values), 2) if values else None,
            "p95": round(pct(values, 95), 2) if values else None,
            "rtt": round(self.rtt, 2) if self.rtt else None,
            "region": self.args.region,
            "version": self.version(),
            "ts": int(time.time()),
        }
        if not values or target is None:
            ok = False
        elif higher_is_better:
            ok = rec["p50"] >= target and pct(values, 5) >= target / 1.5
        else:
            ok = rec["p50"] <= target and rec["p95"] <= 1.5 * target
        rec["pass"] = ok
        self.failed |= not ok
        self.rows.append(rec)
        print(json.dumps(rec, ensure_ascii=False), flush=True)

    def version(self):
        if not hasattr(self, "_version"):
            self._version = subprocess.run([self.opc, "--version"], capture_output=True, text=True).stdout.strip()
        return self._version

    # ---- rows --------------------------------------------------------

    def measure_rtt(self):
        p, _, _ = self.run(["doctor", "--json"], timeout=60)
        try:
            steps = json.loads(p.stdout)["steps"]
        except (ValueError, KeyError):
            sys.exit(f"opc doctor failed: {p.stderr.decode(errors='replace')}")
        bad = [s for s in steps if not s["ok"]]
        if bad:
            sys.exit(f"opc doctor: {bad[0]['name']}: {bad[0]['detail']}")
        rt = next(s for s in steps if s["name"] == "round trip")
        self.rtt = float(re.match(r"(\d+)", rt["detail"]).group(1))

    def row_prompt(self):
        values, wall = [], []
        for _ in range(self.args.runs):
            t = self.interactive([])
            values.append(t.get("prompt"))
            wall.append(t.get("wallPrompt"))
        self.gate("launch → prompt", values, 10)
        # The same, seen from outside: fork and exec included, as a person sees it.
        self.gate("launch → prompt on screen (wall)", wall, 10)

    def row_live(self):
        values = []
        for _ in range(self.args.runs):
            t = self.interactive([], settle=1.5)
            values.append(t.get("ready"))
        self.gate("launch → attached and live (new session)", values, self.rtt + 2 * self.args.edge_rtt + 10)

    def ensure_session(self):
        if self.session:
            return self.session
        p, _, _ = self.run(["new", "--title", "opc bench", "--json"])
        if p.returncode != 0:
            sys.exit(f"opc new failed: {p.stderr.decode(errors='replace')}")
        self.session = json.loads(p.stdout)["session"]["id"] if p.stdout.strip().startswith(b"{") else p.stdout.decode().strip()
        return self.session

    def row_print(self):
        sid = self.ensure_session()
        accepted, overhead = [], []
        for _ in range(self.args.runs):
            p, wall, t = self.run(["-p", "-s", sid, "--json", "Reply with the single word ok."])
            try:
                res = json.loads(p.stdout)
            except ValueError:
                continue
            turn = t.get("turn") or {}
            a = (turn.get("accepted") or {}).get("total")
            accepted.append(a)
            if res.get("durationMs") is not None:
                overhead.append(wall - res["durationMs"])
        self.gate("Enter → accepted", accepted, self.rtt + 5)
        self.gate("opc -p, overhead over the turn", overhead, self.rtt + 20)

    def row_ls(self):
        values = []
        for _ in range(self.args.runs):
            _, wall, t = self.run(["ls", "--json"])
            req = [r["ms"] for r in t.get("requests", [])]
            values.append(max(req) if req else wall)
        self.gate("opc ls, refreshed", values, self.rtt + 10)

    def row_interrupt(self):
        sid = self.ensure_session()
        values = []
        for _ in range(max(1, self.args.runs // 10)):
            self.run(["send", "-s", sid, "Count from 1 to 2000, one number per line, slowly."])
            time.sleep(2)
            _, wall, _ = self.run(["interrupt", "-s", sid])
            values.append(wall)
        self.gate("Esc → turn cancelled", values, self.rtt + 300)

    def row_shell(self):
        sid = self.ensure_session()
        values = []
        for _ in range(max(1, self.args.runs // 4)):
            _, wall, _ = self.run(["shell", "-s", sid, "-c", "true"])
            values.append(wall)
        self.gate("opc shell → remote command", values, self.rtt + 20)

    def row_files(self):
        if not self.args.corpus_mb:
            return
        sid = self.ensure_session()
        corpus = os.path.join(self.tmp, "corpus")
        os.makedirs(corpus, exist_ok=True)
        files = 1000
        size = self.args.corpus_mb * 1024 * 1024 // files
        for i in range(files):
            with open(os.path.join(corpus, f"f{i:05}.bin"), "wb") as f:
                f.write(os.urandom(size))
        total = size * files / 1e6
        push, pull = [], []
        for n in range(3):
            name = f"bench-{int(time.time())}-{n}"
            _, wall, _ = self.run(["push", "-s", sid, corpus, "--to", name])
            push.append(total / (wall / 1000))
            dest = os.path.join(self.tmp, f"pulled-{n}")
            _, wall, _ = self.run(["pull", "-s", sid, name, dest])
            pull.append(total / (wall / 1000))
            shutil.rmtree(dest, ignore_errors=True)
        self.gate("opc push", push, 50, unit="MB/s", higher_is_better=True)
        self.gate("opc pull", pull, 50, unit="MB/s", higher_is_better=True)

    def finish(self):
        if self.session:
            self.run(["archive", self.session])
        url, token = os.environ.get("TINYBIRD_URL"), os.environ.get("TINYBIRD_TOKEN")
        if url and token and self.rows:
            body = "\n".join(json.dumps(r) for r in self.rows).encode()
            req = urllib.request.Request(f"{url.rstrip('/')}/v0/events?name=opc_bench", data=body, method="POST")
            req.add_header("Authorization", f"Bearer {token}")
            try:
                urllib.request.urlopen(req, timeout=30)
            except Exception as e:  # noqa: BLE001 - reporting must not hide the gates
                print(f"bench: could not send results to Tinybird: {e}", file=sys.stderr)
        shutil.rmtree(self.tmp, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--opc", default="target/release/opc")
    ap.add_argument("--runs", type=int, default=200)
    ap.add_argument("--region", default=os.environ.get("BENCH_REGION", "unknown"))
    ap.add_argument("--edge-rtt", type=float, default=10.0, help="round trip to the nearest edge, ms")
    ap.add_argument("--corpus-mb", type=int, default=0, help="push/pull a corpus of this size")
    ap.add_argument("--local", action="store_true", help="only the rows that need no server")
    ap.add_argument("--rows", default="", help="comma-separated subset: prompt,live,print,ls,interrupt,shell,files")
    args = ap.parse_args()
    b = Bench(args)
    try:
        if args.local:
            b.row_prompt()
        else:
            if not os.environ.get("OPENCOMPUTER_API_KEY"):
                sys.exit("set OPENCOMPUTER_API_KEY")
            b.measure_rtt()
            rows = [r for r in args.rows.split(",") if r] or ["prompt", "live", "print", "ls", "interrupt", "shell", "files"]
            for r in rows:
                getattr(b, f"row_{r}")()
    finally:
        b.finish()
    sys.exit(1 if b.failed else 0)


if __name__ == "__main__":
    main()
