#!/bin/sh
# herdr actions can only run a command, so opening the [[panes]] entry goes
# through the CLI.
set -eu

exec "${HERDR_BIN_PATH:-herdr}" plugin pane open \
  --plugin strays \
  --entrypoint strays \
  --placement tab \
  --focus
