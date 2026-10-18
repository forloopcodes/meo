# meo

Headless fast Rust-built subagent for your main agent.

Meo takes a task as a CLI argument, drives an OpenAI-compatible model through a tight
agent loop with parallel tool execution, streams the output, and exits. Hand it work
from a parent agent, a script, or a shell pipeline — no TUI required.

## Features

- **Headless mode** — `meo "task"` runs one task and exits, ready for piping
- **Interactive mode** — `meo` alone opens a REPL with `/help`, `/clear`, `/context`, `/login`, `/logout`, `/exit`
- **Parallel tool calls** — sibling tool calls run concurrently in a single batch
- **Tool result cache** — identical calls within a task are deduplicated and counted
- **SSE streaming** — tokens print as they arrive, reasoning traces shown as `[think]`
- **Provider agnostic** — works with any OpenAI-compatible endpoint
- **Guardrails** — destructive shell commands are blocked before execution

## Tools

| Tool | Purpose |
|------|---------|
| `read` | Read a file with optional line range |
| `write` | Write a file, creates parent directories |
| `find` | Search files with ripgrep |
| `sh`   | Run a shell command |

## Install

```sh
cargo install --path .
```

## Usage

```sh
# Headless: run a task and exit
meo "add unit tests for src/config.rs"

# Interactive REPL
meo
```

Interactive session:

```
Meo - Minimal coding agent
Commands: /exit /help /clear /login /logout /context
> summarize this codebase
  read     src/main.rs
  find     fn main
  sh       cargo build
Meo: this crate is a minimal coding agent...
```

## Configuration

Credentials are stored in `%APPDATA%\meo\config.json` on Windows and
`~/.config/meo/config.json` elsewhere. Run `/login` to create one interactively.

Environment variables override the config file:

| Variable | Default |
|----------|---------|
| `OPENAI_API_KEY` | none |
| `OPENAI_BASE_URL` | `https://api.openai.com` |
| `MODEL_NAME` | `gpt-4o-mini` |

```sh
export OPENAI_API_KEY=sk-...
export OPENAI_BASE_URL=https://opencode.ai/zen
export MODEL_NAME=gpt-4o-mini
```

## Integrating with a parent agent

Meo is designed to be spawned as a subprocess. It writes tool activity to stderr and
the model's final answer to stdout, so capture stdout cleanly:

```sh
result=$(meo "fix the failing test in tests/api.rs")
```

## License

MIT
