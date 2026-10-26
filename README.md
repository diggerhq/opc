# opc

`opc` is a terminal client for your organization's [Computer](https://opencomputer.dev/computer), the agent that works for your organization on its own cloud machine. It is one static binary that shows a prompt before the network answers and streams the agent's output as the model writes it. Its sessions are the same sessions as Slack and the dashboard: a session started in a terminal shows up on the dashboard, and a Slack thread can be attached from a terminal, with what each side types showing on the other.

This repository also holds the documentation for `opc` and for the streaming session API it is built on (HTTP and the `opc.v1` WebSocket protocol), in [`docs/`](docs/).

**Status:** prerelease. The latest build is [`v0.1.0-rc.5`](https://github.com/diggerhq/opc/releases/tag/v0.1.0-rc.5). Commands and output formats may still change before 0.1.0.

## Install

Builds exist for macOS (Apple silicon, Intel) and Linux (x86-64, arm64, statically linked). On Windows, use the Linux build under WSL 2.

Download a release (here with the [GitHub CLI](https://cli.github.com)), check it and put `opc` on your `PATH`. Pick the archive for your platform:

| Platform | Archive |
| --- | --- |
| macOS, Apple silicon | `opc-v0.1.0-rc.5-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `opc-v0.1.0-rc.5-x86_64-apple-darwin.tar.gz` |
| Linux, x86-64 | `opc-v0.1.0-rc.5-x86_64-unknown-linux-musl.tar.gz` |
| Linux, arm64 | `opc-v0.1.0-rc.5-aarch64-unknown-linux-musl.tar.gz` |

```bash
name=opc-v0.1.0-rc.5-aarch64-apple-darwin
gh release download v0.1.0-rc.5 --repo diggerhq/opc --pattern "$name.tar.gz" --pattern SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS     # sha256sum --ignore-missing -c on Linux
tar -xzf "$name.tar.gz"
mkdir -p ~/.local/bin && install -m 755 "$name/opc" ~/.local/bin/opc
opc --version
# opc 0.1.0 (protocol opc.v1, schema computer-api.v1)
```

The macOS builds are not signed: if macOS blocks the first run, `xattr -d com.apple.quarantine ~/.local/bin/opc`.

The install script does the same in one line: it picks the archive, checks it against `SHA256SUMS` and installs to `~/.local/bin` (`OPC_INSTALL_DIR` and `OPC_VERSION` change that). Its source is [`ci/install.sh`](ci/install.sh). See [Install](docs/install.mdx).

```bash
curl -fsSL https://raw.githubusercontent.com/diggerhq/opc/main/ci/install.sh | sh
```

## Quickstart

You need an OpenComputer organization with Computer set up. If your team uses Computer in Slack, you have one.

```bash
opc                                     # log in the first time, then a new session
opc "fix the flaky test in segment.test.ts and open a PR"
opc a -                                 # back to the last session
opc a https://acme.slack.com/archives/C07ABC/p1761400000000100   # a Slack thread
opc ls                                  # your sessions
opc -p "summarize this week's merged PRs"   # one turn, print the result, exit
```

The first run opens your browser to confirm a short code. `opc login` does only that; `OPENCOMPUTER_API_KEY` is used instead when it is set, as in CI. If you already use the `opencomputer` CLI you are logged in: both read `~/.opencomputer/config.json`.

In a session, `Ctrl+O` shows the full detail of the agent's steps, and `Ctrl+D` on an empty prompt detaches. The session keeps running on the computer. See [Quickstart](docs/quickstart.mdx) and [Keys](docs/terminal/keys.mdx).

## Commands

| | |
| --- | --- |
| `opc [message]` | Start a session and attach to it |
| `opc -p <message>` | Non-interactive: send, wait, print the result (`--json`, `--output-format stream-json`) |
| `opc new`, `send`, `wait`, `watch`, `log`, `interrupt` | Create, drive and follow sessions from scripts |
| `opc a`, `ls`, `open`, `rename`, `archive`, `unarchive` | Find, attach to and manage sessions |
| `opc ask`, `subagents` | Answer the agent's questions; list, stop and read subagents |
| `opc push`, `pull`, `diff`, `shell` | Move files, see the agent's changes, open a shell in the workspace |
| `opc secret` | Manage secrets the agent can use |
| `opc login`, `logout`, `whoami`, `doctor` | Credentials and connectivity |
| `opc schema`, `completion`, `config`, `help` | JSON Schema of the API objects, shell completions, settings, help topics |

`opc --help`, `opc <command> --help` and `opc help <topic>` print the details. [Commands](docs/reference/commands.mdx) lists every option.

## For agents and scripts

`opc` runs the same without a person at the keyboard: nothing prompts or opens a browser, results are JSON on standard output, errors are JSON on standard error with `--json`, and the exit code says what happened (`0` success, `1` turn failed, `2` usage, `3` auth, `4` retry, `5` waiting for a person, `6` out of credits, `124` timed out, `130` interrupted).

- [For agents and scripts](docs/cli/agents.mdx): every non-interactive command, what it prints, and recipes. Also `opc help agents`.
- [Scripting](docs/cli/scripting.mdx), [Exit codes](docs/reference/exit-codes.mdx), [CI](docs/cli/ci.mdx).
- `opc schema [result|event|session|turn|ask]` prints the JSON Schema of what `opc` prints.
- The published docs site serves `/llms.txt` (an index of pages) and `/llms-full.txt` (every page in one file), generated by Mintlify.

## Configuration and telemetry

`opc` shares `~/.opencomputer/config.json` with the `opencomputer` CLI and keeps its caches in `~/.opencomputer/opc/`. Environment variables (`OPENCOMPUTER_API_KEY`, `OPENCOMPUTER_API_URL`, `OPC_LOG`, `NO_COLOR` and others) are listed in [Environment](docs/reference/environment.mdx).

`opc` sends a few usage timings and error classes on the connection it already has open, never what you type, what the agent writes, file names or paths. To turn it off:

```bash
opc config telemetry off     # kept in ~/.opencomputer/opc/settings.json
export OPC_TELEMETRY=0       # this shell only
```

[Telemetry](docs/reference/telemetry.mdx) lists every field it sends.

## Documentation

The docs are a [Mintlify](https://mintlify.com) site in [`docs/`](docs/), in two parts:

- **opc:** [Introduction](docs/index.mdx), [Concepts](docs/concepts.mdx), [the session screen](docs/terminal/session-screen.mdx), [files](docs/cli/files.mdx), [sharing](docs/cli/sharing.mdx), [timing](docs/reference/timing.mdx), [troubleshooting](docs/troubleshooting.mdx), [changelog](docs/changelog.mdx).
- **Session API:** [Overview](docs/api/overview.mdx), [Quickstart](docs/api/quickstart.mdx), [HTTP](docs/api/http/), [WebSocket](docs/api/websocket/), [Objects](docs/api/reference/objects.mdx). The JSON Schema is [`schema/computer-api.v1.json`](schema/computer-api.v1.json).

To preview the site locally:

```bash
cd docs
npx mint dev
```

## Building from source

Rust 1.89 or later.

```bash
cargo build --release --locked      # target/release/opc
cargo test --locked
```

The tests run against an in-process fake server and need no account. [AGENTS.md](AGENTS.md) describes the layout and the checks CI runs.

## License

Apache-2.0. See [LICENSE](LICENSE).
