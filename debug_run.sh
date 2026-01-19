#!/usr/bin/env bash

# Use a timestamp to ensure unique log files across different runs of the script
TIMESTAMP=$(date +%s)
LOG_DIR="debug_logs_${TIMESTAMP}"
mkdir -p "$LOG_DIR"

VALUES="1024 2048 4096 8192 16384 32768 65536 131072 262144 524288 1048576 2097152 4194304 8388608 16777216 33554432 67108864 134217728 268435456 536870912 1073741824"
SLEEP_TIME=5

# ---------------------------------------------------------
# Litep2p bandwidth test
# ---------------------------------------------------------

cd litep2p
# Start the server
RUST_LOG=debug cargo run -- server --listen-address "/ip6/::/tcp/33333" --node-key "secret" > "../$LOG_DIR/listener_litep2p_litep2p_tcp.log" 2>&1 &
SERVER_PID=$!

echo "Running litep2p (TCP) debug run. Server PID $SERVER_PID, logs in $LOG_DIR..."
sleep $SLEEP_TIME

for bytes in $VALUES; do
    echo "Testing $bytes bytes..."
    RUST_LOG=debug cargo run -- client --server-address "/ip6/::1/tcp/33333/p2p/12D3KooWBpZHDZu7YSbvPaPXKhkRNJvR7MkTJMQQAVBKx9mCqz3q" --upload-bytes $bytes --download-bytes $bytes > "../$LOG_DIR/dialer_litep2p_litep2p_tcp_$bytes.log" 2>&1
done

kill $SERVER_PID

# ---------------------------------------------------------
# Libp2p (TCP) bandwidth test
# ---------------------------------------------------------

cd ../libp2p
RUST_LOG=debug cargo run -- server --listen-address "/ip6/::/tcp/33333" --node-key "secret" > "../$LOG_DIR/listener_libp2p_libp2p_tcp.log" 2>&1 &
SERVER_PID=$!

echo "Running libp2p (TCP) debug run... Server PID $SERVER_PID"
sleep $SLEEP_TIME

for bytes in $VALUES; do
    echo "Testing $bytes bytes..."
    RUST_LOG=debug cargo run -- client --server-address "/ip6/::1/tcp/33333/p2p/12D3KooWBpZHDZu7YSbvPaPXKhkRNJvR7MkTJMQQAVBKx9mCqz3q" --upload-bytes $bytes --download-bytes $bytes > "../$LOG_DIR/dialer_libp2p_libp2p_tcp_$bytes.log" 2>&1
done

kill $SERVER_PID

# ---------------------------------------------------------
# Libp2p (WebRTC) bandwidth test
# ---------------------------------------------------------

cd ../libp2p
RUST_LOG=debug cargo run -- server --transport-layer "webrtc" --listen-address "/ip4/127.0.0.1/udp/8888/webrtc-direct" --node-key "secret" > "../$LOG_DIR/listener_libp2p_libp2p_webrtc.log" 2>&1 &
SERVER_PID=$!

echo "Running libp2p (WebRTC) debug run... Server PID $SERVER_PID"
sleep $SLEEP_TIME

CERT_HASH=$(grep "/certhash/" "../$LOG_DIR/listener_libp2p_libp2p_webrtc.log" | cut -d '/' -f 8 | cut -d ' ' -f 1 | head -n 1)

for bytes in $VALUES; do
    echo "Testing $bytes bytes..."
    RUST_LOG=debug cargo run -- client --transport-layer "webrtc" --server-address "/ip4/127.0.0.1/udp/8888/webrtc-direct/certhash/${CERT_HASH}/p2p/12D3KooWBpZHDZu7YSbvPaPXKhkRNJvR7MkTJMQQAVBKx9mCqz3q" --upload-bytes $bytes --download-bytes $bytes > "../$LOG_DIR/dialer_libp2p_libp2p_webrtc_$bytes.log" 2>&1
done

kill $SERVER_PID

# ---------------------------------------------------------
# Libp2p -> Litep2p (TCP)
# ---------------------------------------------------------

cd ../litep2p
RUST_LOG=debug cargo run -- server --listen-address "/ip6/::/tcp/33333" --node-key "secret" > "../$LOG_DIR/listener_litep2p_libp2p_litep2p_tcp.log" 2>&1 &
SERVER_PID=$!

echo "Running libp2p -> litep2p (TCP) debug run... Server PID $SERVER_PID"
sleep $SLEEP_TIME

cd ../libp2p
for bytes in $VALUES; do
    echo "Testing $bytes bytes..."
    RUST_LOG=debug cargo run -- client --server-address "/ip6/::1/tcp/33333/p2p/12D3KooWBpZHDZu7YSbvPaPXKhkRNJvR7MkTJMQQAVBKx9mCqz3q" --upload-bytes $bytes --download-bytes $bytes > "../$LOG_DIR/dialer_libp2p_litep2p_tcp_$bytes.log" 2>&1
done

kill $SERVER_PID

# ---------------------------------------------------------
# Libp2p -> Litep2p (WebRTC)
# ---------------------------------------------------------

cd ../litep2p
RUST_LOG=debug cargo run -- server --transport-layer "webrtc" --listen-address "/ip4/127.0.0.1/udp/8888/webrtc-direct" --node-key "secret" > "../$LOG_DIR/listener_litep2p_libp2p_litep2p_webrtc.log" 2>&1 &
SERVER_PID=$!

echo "Running libp2p -> litep2p (WebRTC) debug run... Server PID $SERVER_PID"
sleep $SLEEP_TIME

CERT_HASH=$(grep "/certhash/" "../$LOG_DIR/listener_litep2p_libp2p_litep2p_webrtc.log" | cut -d '/' -f 8 | cut -d ' ' -f 1 | head -n 1)
VALUES_ARRAY=($VALUES)

cd ../libp2p
for bytes in "${VALUES_ARRAY[@]:0:20}"; do
    echo "Testing $bytes bytes..."
    RUST_LOG=debug cargo run -- client --transport-layer "webrtc" --server-address "/ip4/127.0.0.1/udp/8888/webrtc-direct/certhash/${CERT_HASH}/p2p/12D3KooWBpZHDZu7YSbvPaPXKhkRNJvR7MkTJMQQAVBKx9mCqz3q" --upload-bytes $bytes --download-bytes $bytes > "../$LOG_DIR/dialer_libp2p_litep2p_webrtc_$bytes.log" 2>&1
done

kill $SERVER_PID

# ---------------------------------------------------------
# Litep2p -> Libp2p (TCP)
# ---------------------------------------------------------

cd ../libp2p
RUST_LOG=debug cargo run -- server --listen-address "/ip6/::/tcp/33333" --node-key "secret" > "../$LOG_DIR/listener_libp2p_litep2p_libp2p_tcp.log" 2>&1 &
SERVER_PID=$!

echo "Running litep2p -> libp2p (TCP) debug run... Server PID $SERVER_PID"
sleep $SLEEP_TIME

cd ../litep2p
for bytes in $VALUES; do
    echo "Testing $bytes bytes..."
    RUST_LOG=debug cargo run -- client --server-address "/ip6/::1/tcp/33333/p2p/12D3KooWBpZHDZu7YSbvPaPXKhkRNJvR7MkTJMQQAVBKx9mCqz3q" --upload-bytes $bytes --download-bytes $bytes > "../$LOG_DIR/dialer_litep2p_libp2p_tcp_$bytes.log" 2>&1
done

kill $SERVER_PID

# ---------------------------------------------------------
# Smoldot -> Litep2p (WebRTC)
# ---------------------------------------------------------

cd ../litep2p
RUST_LOG=debug cargo run -- server --transport-layer "webrtc" --listen-address "/ip4/127.0.0.1/udp/8888/webrtc-direct" --node-key "secret" > "../$LOG_DIR/listener_litep2p_smoldot_litep2p_webrtc.log" 2>&1 &
SERVER_PID=$!

echo "Running smoldot -> litep2p (WebRTC) debug run... Server PID $SERVER_PID"
sleep $SLEEP_TIME

CERT_HASH=$(grep "/certhash/" "../$LOG_DIR/listener_litep2p_smoldot_litep2p_webrtc.log" | cut -d '/' -f 8 | cut -d ' ' -f 1 | head -n 1)
cd ..

for bytes in $VALUES; do
    echo "Testing $bytes bytes..."
    RUST_LOG=debug cargo run --bin "smoldot-automation" -p smoldot-automation -- "/ip4/127.0.0.1/udp/8888/webrtc-direct/certhash/${CERT_HASH}/p2p/12D3KooWBpZHDZu7YSbvPaPXKhkRNJvR7MkTJMQQAVBKx9mCqz3q" $bytes $bytes > "$LOG_DIR/dialer_smoldot_litep2p_webrtc_$bytes.log" 2>&1
done

kill $SERVER_PID

echo "Debug run finished. Logs are available in $LOG_DIR"
