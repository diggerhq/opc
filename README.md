# opc

**Your organization's Computer, in your terminal.**

[Computer](https://app.opencomputer.dev/computer) is the agent your team already works with in Slack. `opc` brings it to your terminal: start a session or pick up a thread, watch it work, answer when it asks, and walk away while it keeps going.

```bash
curl -fsSL https://raw.githubusercontent.com/diggerhq/opc/main/ci/install.sh | sh
opc "fix the flaky test in segment.test.ts and open a PR"
```

## Why opc

- **One conversation, anywhere.** Start in the terminal and continue in Slack or the dashboard, or the other way round. `opc a <slack link>` joins a thread live.
- **Instant.** One small binary. The prompt is up before the network answers, and replies stream as they are written.
- **You decide.** Approve with `y`, deny with `n`, or press `a` to stop being asked for the rest of the session.
- **Files both ways.** `opc push` and `opc pull` move files, `opc diff` shows what changed, and `opc shell` lets you look around the agent's workspace.
- **Walk away.** `Ctrl+D` detaches and the work goes on. `opc a -` picks up where you left off.
- **Scriptable.** `opc -p` runs one turn and prints the result as text or JSON, with exit codes you can branch on. It suits CI and other agents.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/diggerhq/opc/main/ci/install.sh | sh
```

This works on macOS and Linux, and on Windows under WSL 2. The script verifies the checksum, installs to `~/.local/bin` and never asks for `sudo`. To update, run it again. [Install](docs/install.mdx) has the options.

The macOS builds are not signed yet. If macOS blocks the first run, use `xattr -d com.apple.quarantine ~/.local/bin/opc`.

`opc` is a prerelease, so commands and output may change before 0.1.0.

## First steps

```bash
opc                   # log in, then start a session
opc ls                # your sessions
opc a -               # back to the last one
opc a <slack link>    # join a Slack thread
```

The first run logs you in through your browser. If you already use the `opencomputer` CLI, you are logged in. In a session, `Ctrl+O` expands what the agent did and `Ctrl+D` detaches. See the [Quickstart](docs/quickstart.mdx) and [Keys](docs/terminal/keys.mdx).

## Scripts and CI

```bash
opc -p "summarize this week's merged PRs"
opc -p --json "is main green?" | jq -r .result
```

Without a terminal, `opc` never prompts. The exit codes are `0` done, `1` failed, `5` waiting for a person and `124` timed out; [Exit codes](docs/reference/exit-codes.mdx) lists them all. See also [Scripting](docs/cli/scripting.mdx), [CI](docs/cli/ci.mdx) and [For agents](docs/cli/agents.mdx).

## Docs

`opc help <topic>` works offline. Everything else is in [`docs/`](docs/):

- [Concepts](docs/concepts.mdx)
- [Commands](docs/reference/commands.mdx)
- [Troubleshooting](docs/troubleshooting.mdx)
- [Session API](docs/api/overview.mdx), for building your own client

## Privacy

`opc` reports timings and error types. It never sends your messages, the agent's output or file paths, and [Telemetry](docs/reference/telemetry.mdx) lists every field it does send. To turn it off, run `opc config telemetry off`.

## Build from source

```bash
cargo build --release --locked    # Rust 1.89+; the binary is target/release/opc
cargo test --locked               # no account needed
```

[AGENTS.md](AGENTS.md) describes the layout and the checks.

## License

[Apache-2.0](LICENSE)
