#!/usr/bin/env bash
set -euo pipefail

#
# Builds the smoldot wasm client
# starts the libp2p server (WebRTC),
# opens the browser to the demo page with the server multiaddr prefilled,
# triggers the demo automatically.


ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
CLIENT_DIR="$ROOT_DIR/smoldot-wasm-client"
LIBP2P_DIR="$ROOT_DIR/libp2p"

echo "[1/4] Building wasm with wasm-pack"
cd "$CLIENT_DIR"
if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "wasm-pack not found. Installing..."
  cargo install wasm-pack --locked >/dev/null 2>&1 || {
    echo "Failed to install wasm-pack automatically" >&2
    exit 1
  }
fi
rm -f Cargo.lock
cargo run --bin smoldot-wasm-build

echo "[2/4] Starting libp2p server (WebRTC)"
cd "$LIBP2P_DIR"

# Start server listening on 0.0.0.0:0/udp/webrtc-direct
# We use a fixed node key so PeerId is deterministic.

SERVER_LOG=$(mktemp)
RUST_LOG=debug cargo run -- server \
  --listen-address "/ip4/127.0.0.1/udp/0/webrtc-direct" \
  --node-key "secret" \
  --transport-layer webrtc \
  >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!

cleanup() {
  if kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" || true
  fi
  if [ -n "${TAIL_PID:-}" ] && kill -0 "$TAIL_PID" 2>/dev/null; then
    kill "$TAIL_PID" || true
  fi
}
trap cleanup EXIT

echo "Waiting for server to print its listening address..."
ATTEMPTS=50
SLEEP=0.2
SERVER_ADDR=""

# Stream server logs to terminal as they arrive
tail -n +1 -f "$SERVER_LOG" &
TAIL_PID=$!

while (( ATTEMPTS > 0 )); do
  # Prefer LISTEN_ADDR if printed by server
  if grep -E "^LISTEN_ADDR=" "$SERVER_LOG" >/dev/null 2>&1; then
    line=$(grep -E "^LISTEN_ADDR=" "$SERVER_LOG" | tail -n 1)
    line=${line#LISTEN_ADDR=}
  elif grep -E "NewListenAddr:" "$SERVER_LOG" >/dev/null 2>&1; then
    # Fallback: parse NewListenAddr line
    line=$(grep -E "NewListenAddr:" "$SERVER_LOG" | tail -n 1 | sed -E 's/.*NewListenAddr: //')
  else
    line=""
  fi

  if [[ -n "$line" && "$line" =~ (/ip4/[0-9\.]+/udp/[0-9]+/webrtc-direct/certhash/u[A-Za-z0-9_-]+) ]]; then
    SERVER_ADDR="${BASH_REMATCH[1]}"
    if [[ "$SERVER_ADDR" == /ip4/0.0.0.0/* ]]; then
      SERVER_ADDR=${SERVER_ADDR/\/ip4\/0.0.0.0\//\/ip4\/127.0.0.1\/}
    fi
    break
  fi

  # If no certhash, try to derive it from a fingerprint log line
  if [[ -n "$line" && "$line" =~ (/ip4/[0-9\.]+/udp/[0-9]+/webrtc-direct) ]]; then
    base_addr="${BASH_REMATCH[1]}"
    fp_line=$(grep -E "fingerprint:|Fingerprint:" "$SERVER_LOG" | tail -n 1 || true)
    if [[ -n "$fp_line" ]]; then
      hex_fp=$(echo "$fp_line" | sed -E 's/.*fingerprint:[^A-Fa-f0-9]*([A-Fa-f0-9:]{63,}).*/\1/' | tr '[:lower:]' '[:upper:]')
      if [[ "$hex_fp" =~ ^([0-9A-F]{2}:){31}[0-9A-F]{2}$ ]]; then
        b64u=$(python3 - "$hex_fp" <<'PY'
import sys, base64
hex_fp = sys.argv[1].replace(':','')
digest = bytes.fromhex(hex_fp)
mh = bytes([0x12, 0x20]) + digest
print(base64.urlsafe_b64encode(mh).decode().rstrip('='))
PY
)
        if [[ -n "$b64u" ]]; then
          SERVER_ADDR="$base_addr/certhash/u$b64u"
          if [[ "$SERVER_ADDR" == /ip4/0.0.0.0/* ]]; then
            SERVER_ADDR=${SERVER_ADDR/\/ip4\/0.0.0.0\//\/ip4\/127.0.0.1\/}
          fi
          break
        fi
      fi
    fi
  fi
  sleep "$SLEEP"
  ATTEMPTS=$((ATTEMPTS-1))
done

if [[ -z "$SERVER_ADDR" ]]; then
  echo "Could not determine server address from logs" >&2
  tail -n 100 "$SERVER_LOG" >&2
fi

echo "[3/4] Launching local HTTP server and opening demo page"
cd "$CLIENT_DIR"

# Start a HTTP server on an available port
serve_port="8081"
PY_LOG=$(mktemp)
(python3 -m http.server "$serve_port" >"$PY_LOG" 2>&1 &)
HTTP_PID=$!
sleep 0.3 || true
if ! kill -0 "$HTTP_PID" 2>/dev/null; then
  serve_port="8082"
  (python3 -m http.server "$serve_port" >"$PY_LOG" 2>&1 &)
  HTTP_PID=$!
  sleep 0.3 || true
fi

cleanup_http() {
  if kill -0 "$HTTP_PID" 2>/dev/null; then
    kill "$HTTP_PID" || true
  fi
}
trap cleanup_http EXIT

# Pass peer via query param and trigger autorun=true
if [[ -n "$SERVER_ADDR" ]]; then
  URL="http://localhost:${serve_port}/index.html?peer=$(python3 -c 'import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1]))' "$SERVER_ADDR")&autorun=true"
else
  URL="http://localhost:${serve_port}/index.html?autorun=true"
fi

if command -v open >/dev/null 2>&1; then
  open "$URL"
else
  echo "Open this URL in your browser: $URL"
fi

echo "[4/4] Demo running."
wait "$SERVER_PID"


