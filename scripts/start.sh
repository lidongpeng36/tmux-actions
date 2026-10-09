#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="$1" SOCKET="$2"; shift 2
BIN="$(tmux -S "$SOCKET" show-option -gqv @tmux-actions-bin)"
if [ -z "$BIN" ]; then
  SERVER_PID="$(tmux -S "$SOCKET" display-message -p '#{pid}')"
  BIN="$(TMUX_ACTIONS_SERVER_PID="$SERVER_PID" "$ROOT/scripts/install.sh")"
fi
exec "$BIN" "$MODE" --socket "$SOCKET" --launcher "$ROOT/scripts/start.sh" "$@"
