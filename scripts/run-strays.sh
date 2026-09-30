#!/bin/sh
# herdr starts plugin panes without a shell, so PATH is the bare system one and
# `claude` is not found. Borrow the PATH of the user's login shell instead.
#
# A login shell rather than an interactive one: an interactive shell may print
# prompts or ask for input. zsh reads no .zshrc as a login shell, so the
# default Claude Code install location is appended as a fallback.
set -u

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

# Tagged so that anything the profile prints to stdout can't leak into PATH.
login_path=$("${SHELL:-/bin/sh}" -lc 'printf "STRAYS_PATH=%s\n" "$PATH"' </dev/null 2>/dev/null |
  sed -n 's/^STRAYS_PATH=//p' | tail -n 1)

PATH="${login_path:+$login_path:}$PATH:$HOME/.local/bin"
export PATH

exec "$root/bin/strays"
