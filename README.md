# strays

[![CI](https://github.com/m1sk9/strays/actions/workflows/ci.yaml/badge.svg)](https://github.com/m1sk9/strays/actions/workflows/ci.yaml)
[![Release strays](https://github.com/m1sk9/strays/actions/workflows/release.yaml/badge.svg)](https://github.com/m1sk9/strays/actions/workflows/release.yaml)
[![MIT LICENSE](https://img.shields.io/github/license/m1sk9/strays?color=%239944ee)](https://github.com/m1sk9/strays/blob/main/LICENSE)
[![codecov](https://codecov.io/github/m1sk9/strays/graph/badge.svg?token=QA075J11S8)](https://codecov.io/github/m1sk9/strays)

A TUI for Centralized Management of Claude Code.

## Todo

- [ ] herdr integration: match sessions to herdr panes by `cwd` and focus them with `zoom --on/--off`
- [ ] Transcript preview pane (tail of the session's JSONL)
- [ ] Search and resume completed/past sessions (`--all`, scanning `~/.claude/projects/`)
- [ ] Bulk cleanup: sort stray sessions by age and kill several at once
- [ ] Asynchronous `App::refresh()` so a slow `claude agents --json` can't freeze the event loop
- [ ] Bracketed paste support for the new-session path input
- [ ] Grapheme-cluster aware cursor movement (currently Unicode scalar values)

## Features

- **Session table**: lists every session `claude agents --json` reports — background and interactive — with state, elapsed time, working directory, and name, color-coded by whether it's blocked, busy, or idle.
- **Attach**: opens a running background session in the current terminal (`claude attach <id>`).
- **Fork**: branches a session into a brand-new one via `--fork-session`, leaving the original untouched — works even while the original is still running.
- **Kill**: ends a stray background session after a confirmation prompt; sessions without a `pid` are shown dimmed since there's nothing to signal.
- **Open a new session anywhere**: starts a fresh `claude` session in any directory, prefilled with the selected row's `cwd` so reopening the same project is one keystroke.

## Installation

### Rust (crates.io)

strays targets **Linux and macOS**. Windows is not supported.

```shell
cargo install strays
```

## Usage

Run it from anywhere:

```shell
strays
```

### Keybindings

| Key | Action |
| --- | --- |
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `r` | Refresh the session list |
| `Enter` / `o` | Attach to the selected session |
| `f` | Fork the selected session |
| `x` | Request to kill the selected session (`y` confirms, any other key cancels) |
| `n` | Open a new session in a directory (prefilled with the selected row's `cwd`) |
| `q` / `Esc` / `Ctrl-C` | Quit |

Only `background`-kind sessions can be attached to directly — an `interactive` session is already open in some other terminal, and jumping to that needs a pane manager like herdr (see Todo).

## LICENSE

strays is published under [MIT License](./LICENSE).

<sub>
    © 2026 m1sk9
</sub>
