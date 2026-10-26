# opc

**Your organization's Computer, in your terminal.**

[Computer](https://app.opencomputer.dev/computer) is the agent your team works with in Slack. It has its own cloud machine, it works in your repositories, and it keeps going after you look away. `opc` lets you work with the same agent from a terminal: start a session or pick up a Slack thread, watch the agent work as it writes, answer its questions with a key, and leave whenever you like.

```bash
curl -fsSL https://raw.githubusercontent.com/diggerhq/opc/main/ci/install.sh | sh
opc "fix the flaky test in segment.test.ts and open a PR"
```

> `opc` is a prerelease. Commands and output formats may change before 0.1.0.

## Why use it

- **One session, every surface.** A session started in a terminal shows up on the dashboard. `opc a <slack link>` attaches to a Slack thread, and what you type there shows up on the other side, and the other way round.
- **Fast.** `opc` is one static binary. The prompt appears before the network answers, and the agent's text streams as the model writes it.
- **You stay in control.** When the agent asks before running something, press `y` to allow, `n` to deny, or `a` to allow that kind of command for the rest of the session. Questions and plans are answered the same way, inline.
- **Files go both ways.** `opc push` sends a directory to the computer, `opc pull` brings results back, `opc diff` shows what the agent changed, and `opc shell` opens a shell on its machine.
- **Leave at any time.** `Ctrl+D` detaches and the session keeps running. `opc a -` brings you back.
- **Built for scripts too.** `opc -p` runs one turn and prints the result as text or JSON, with exit codes that say what happened. It works in CI and from other agents.

## Install

macOS (Apple silicon, Intel) and Linux (x86-64, arm64). On Windows, use WSL 2.

```bash
curl -fsSL https://raw.githubusercontent.com/diggerhq/opc/main/ci/install.sh | sh
```

The script downloads the latest [release](https://github.com/diggerhq/opc/releases) for your platform, checks its SHA-256 and installs it to `~/.local/bin`. It never uses `sudo`. To update, run it again. You can read what it does in [`ci/install.sh`](ci/install.sh), and [Install](docs/install.mdx) covers the options.

The macOS builds are not signed yet. If macOS blocks the first run, run `xattr -d com.apple.quarantine ~/.local/bin/opc`.

## Get started

You need an OpenComputer organization with Computer set up. If your team already uses Computer in Slack, you have one.

```bash
opc                     # log in, then start a new session
opc a -                 # back to your last session
opc a <slack link>      # attach to a Slack thread
opc ls                  # your sessions
```

The first run opens your browser to confirm a short code. If you already use the `opencomputer` CLI, you're already logged in.

In a session, press `Ctrl+O` to see the full detail of what the agent did, and `Ctrl+D` on an empty prompt to detach. The [Quickstart](docs/quickstart.mdx) and [Keys](docs/terminal/keys.mdx) cover the rest.

## Scripts and CI

```bash
opc -p "summarize this week's merged PRs"                      # print the reply
opc -p --json "is main green?" | jq -r .result                 # or the result as JSON
OPENCOMPUTER_API_KEY=... opc -p --timeout 20m "run the nightly checks"
```

Nothing prompts or opens a browser when there's no terminal. Errors are JSON with `--json`. The exit code tells you what happened: `0` done, `1` the turn failed, `5` the agent is waiting for a person, `124` timed out, and [a few more](docs/reference/exit-codes.mdx). See [Scripting](docs/cli/scripting.mdx), [CI](docs/cli/ci.mdx), and [For agents](docs/cli/agents.mdx) (also `opc help agents`).

## Documentation

`opc --help` and `opc help <topic>` work offline. The full docs live in [`docs/`](docs/):

- [Concepts](docs/concepts.mdx) and [the session screen](docs/terminal/session-screen.mdx)
- [Commands](docs/reference/commands.mdx), every option of every command
- [Troubleshooting](docs/troubleshooting.mdx)
- [The session API](docs/api/overview.mdx) that `opc` is built on, over HTTP and WebSocket, for building your own client. The JSON Schema is [`schema/computer-api.v1.json`](schema/computer-api.v1.json).

To preview the docs site locally, run `cd docs && npx mint dev`.

## Telemetry

`opc` sends a few timings and error classes over the connection it already has open. It never sends what you type, what the agent writes, file names or paths. [Telemetry](docs/reference/telemetry.mdx) lists every field. To turn it off, run `opc config telemetry off` or set `OPC_TELEMETRY=0`.

## Building from source

You need Rust 1.89 or later.

```bash
cargo build --release --locked      # target/release/opc
cargo test --locked                 # against a built-in fake server, no account needed
```

[AGENTS.md](AGENTS.md) describes the layout and the checks CI runs.

## License

[Apache-2.0](LICENSE)
