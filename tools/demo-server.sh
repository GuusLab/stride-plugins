#!/bin/sh
# Usage: tools/demo-server.sh <port> <workdir>
#
# Starts a throwaway Stride server (built with the plugin host) on
# 127.0.0.1:<port> with a fresh SQLite database in <workdir>, in the
# background, and waits until /health answers. Prints the PID; stop it with
#   kill $(cat <workdir>/stride.pid)
# Owner login: demo@stride.test / stride-demo-password-2026
# Log: <workdir>/stride.log
#
# Override the binary with STRIDE_BIN=... (default: stride on PATH, else
# $STRIDE_REPO/target/debug/stride) and the Stride checkout with STRIDE_REPO=...
# (default: ../Stride next to this repository).
set -eu
[ $# -eq 2 ] || { echo "usage: $0 <port> <workdir>" >&2; exit 2; }
port=$1
mkdir -p "$2"
work=$(cd "$2" && pwd)
root=$(cd "$(dirname "$0")/.." && pwd)
STRIDE_REPO=${STRIDE_REPO:-$root/../Stride}
STRIDE_BIN=${STRIDE_BIN:-$(command -v stride || echo "$STRIDE_REPO/target/debug/stride")}
[ -x "$STRIDE_BIN" ] || { echo "no stride binary at $STRIDE_BIN" >&2; exit 1; }

if [ ! -f "$STRIDE_REPO/apps/editor/dist/index.html" ]; then
  (cd "$STRIDE_REPO" && npm run build:editor)
fi

if [ -f "$work/stride.pid" ] && kill -0 "$(cat "$work/stride.pid")" 2>/dev/null; then
  kill "$(cat "$work/stride.pid")" || true
  sleep 1
fi
rm -f "$work"/stride.db "$work"/stride.db-* 

cd "$work"
STRIDE_ADDR="127.0.0.1:$port" \
STRIDE_DB="$work/stride.db" \
STRIDE_OWNER_EMAIL=demo@stride.test \
STRIDE_OWNER_PASSWORD=stride-demo-password-2026 \
STRIDE_EDITOR_DIR="$STRIDE_REPO/apps/editor/dist" \
  nohup "$STRIDE_BIN" serve >"$work/stride.log" 2>&1 &
pid=$!
echo "$pid" > "$work/stride.pid"

i=0
until curl -fs "http://127.0.0.1:$port/health" >/dev/null 2>&1; do
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "stride exited; see $work/stride.log" >&2
    tail -20 "$work/stride.log" >&2
    exit 1
  fi
  i=$((i + 1))
  [ $i -lt 120 ] || { echo "stride did not become ready" >&2; exit 1; }
  sleep 0.5
done
echo "stride ready on http://127.0.0.1:$port (pid $pid, db $work/stride.db)"
