#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
printf -v start '%q' "$ROOT/scripts/start.sh"
command="exec $start configure #{q:socket_path}"
TAG=tmux-actions-load-hook
indices="$(tmux show-hooks -g client-attached | awk -v tag="$TAG" 'index($0,tag) {i=index($0,"[");j=index($0,"]"); print substr($0,i+1,j-i-1)}')"
for index in $indices; do tmux set-hook -gu "client-attached[$index]"; done
version="$(cat "$ROOT/VERSION")"
tmux set-hook -ga client-attached "if -F '#{!=:#{@tmux-actions-version},$version}' \"run-shell -b ': $TAG; $command'\""
tmux run-shell -b "$command"
