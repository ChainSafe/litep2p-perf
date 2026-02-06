# Network Traffic Capture

The `smoldot-automation` tool supports capturing and analyzing SCTP traffic from WebRTC DataChannels using Chrome's built-in logging capabilities.

## Usage

Run a listener, for example using litep2p:

```bash
litep2p-perf$ RUST_LOG="info" cargo run --bin litep2p-perf -- server --listen-address "/ip4/127.0.0.1/udp/8888/webrtc-direct" --node-key "secret" --transport-layer webrtc
```

Run `smoldot-automation` and add the `--capture` flag to enable traffic capture:

```bash
litep2p-perf$ cargo run --bin "smoldot-automation" -p smoldot-automation -- <peer> <upload_bytes> <download_bytes> --capture
```

## Output

When capture is enabled, the tool will:
1. Run the test with Chrome's SCTP logging enabled
2. Extract SCTP packets from Chrome's log
3. Convert them to pcapng format using `text2pcap`
4. Automatically launch Wireshark with the capture file

```
Uploaded 1.00 MiB in 0.5234s bandwidth 2.03 MiB/s
Downloaded 1.00 MiB in 0.4891s bandwidth 2.17 MiB/s

Capture files:
  SCTP log: /tmp/smoldot-sctp-1738527849.log
  PCAP: /tmp/smoldot-sctp-1738527849.pcapng

Wireshark launched successfully
```

## How It Works

This implementation uses Chrome's built-in SCTP packet dumping capabilities:

1. **Chrome Logging**: Launches Chrome with `--enable-logging` and `--vmodule=*/webrtc/*=1` flags
2. **SCTP Packet Export**: Chrome dumps SCTP packets to the log file with `SCTP_PACKET` markers
3. **Log Processing**: Extracts lines containing `SCTP_PACKET` from the Chrome log
4. **PCAP Conversion**: Uses `text2pcap` to convert the packet dump to pcapng format:
   ```bash
   text2pcap -n -l 248 -D -t '%H:%M:%S.' sctp.log sctp.pcapng
   ```
5. **Wireshark Launch**: Automatically opens the pcapng file in Wireshark

## ⚠️ Important Performance Impact

**Chrome's verbose WebRTC logging adds significant overhead** to WebRTC operations:

- **Symptom**: Test may timeout or fail to complete when using `--capture` with large data sizes
- **Root Cause**: The `--vmodule=*/webrtc/*=1` flag makes Chrome log synchronously on every WebRTC packet operation, adding ~100-200ms latency and reducing datachannel throughput
- **Impact**: Connection may close before data transfer completes

### Recommended Solutions

#### Option 1: Use Smaller Data Sizes (Recommended)

The SCTP protocol behavior is identical regardless of payload size:

```bash
# Instead of 1MB (may timeout with --capture):
litep2p-perf$ cargo run --bin "smoldot-automation" -p smoldot-automation -- <peer> 1048576 1048576 --capture

# Use 64KB for reliable capture:
litep2p-perf$ cargo run --bin "smoldot-automation" -p smoldot-automation -- <peer> 65536 65536 --capture

# Or 16KB for quick protocol inspection:
litep2p-perf$ cargo run --bin "smoldot-automation" -p smoldot-automation -- <peer> 16384 16384 --capture
```

#### Option 2: Separate Performance and Capture Runs

Run performance tests separately from traffic capture:

```bash
# 1. Run performance test without logging:
litep2p-perf$ cargo run --bin "smoldot-automation" -p smoldot-automation -- <peer> 1048576 1048576

# 2. Run capture with smaller size:
litep2p-perf$ cargo run --bin "smoldot-automation" -p smoldot-automation -- <peer> 16384 16384 --capture
```

## Advantages Over Network Capture

This approach is superior to traditional packet capture methods because:

- **No encryption barrier**: SCTP packets are logged after DTLS decryption
- **No SSLKEYLOGFILE needed**: Data is already decrypted in Chrome's logs
- **No root/sudo required**: No need for tcpdump privileges
- **Platform independent**: Works on macOS, Linux, and Windows
- **Complete visibility**: See actual SCTP packets, not encrypted DTLS records

## Analyzing Captures in Wireshark

The tool automatically launches Wireshark, but you can also open the file manually:

```bash
wireshark /tmp/smoldot-sctp-*.pcapng
```

### Useful Wireshark Filters

- `sctp` - Show all SCTP packets
- `sctp.chunk_type` - Filter by SCTP chunk type
- `sctp.data.payload_protocol_id == 51` - WebRTC DataChannel DCEP messages
- `sctp.data.payload_protocol_id == 53` - WebRTC DataChannel binary data

### Understanding the Output

The pcapng file contains:
- SCTP DATA chunks with WebRTC DataChannel payloads
- SCTP control chunks (INIT, SACK, HEARTBEAT, etc.)
- Timestamps from Chrome's log
- Packet direction indicators (inbound/outbound)

### Decoding Protobuf Messages

The WebRTC DataChannel messages between smoldot and the listener contain protobuf encoded data. Wireshark can decode these messages using schema file `smoldot-automation/protobuf/webrtc.proto`.

This defines the `webrtc.Message` structure with optional `flag` (FIN, STOP_SENDING, RESET_STREAM) and `message` (bytes) fields.

#### Step 2: Configure Wireshark Protobuf Dissector

1. Open Wireshark and load your capture file
2. Go to **Edit → Preferences**
3. Navigate to **Protocols → ProtoBuf**
4. Configure the following settings:
   - **Protobuf search paths**: Click "Edit" and add the absolute path to the directory containing `webrtc.proto`
     ```
     /path/to/litep2p-perf/smoldot-automation/protobuf
     ```
   - **Load .proto files on startup**: Enable this option
   - **Dissect Protobuf fields as Wireshark fields**: Enable this option
5. Click **OK** to save

#### Step 3: Enable Protobuf Decoding for SCTP Data

Wireshark needs to know which SCTP payload contains protobuf data:

**Method 1: Using "Decode As..."**
1. Right-click on an SCTP DATA packet in the packet list
2. Select **Decode As...**
3. In the dialog, add a new entry:
   - **Field**: SCTP Port (or PPID)
   - **Value**: Current
   - **Type**: ProtoBuf
4. Click **OK**

**Method 2: Permanent Preference**
1. Go to **Analyze → Decode As...**
2. Click the **+** button to add a new entry
3. Set:
   - **Field**: `sctp.ppi` (SCTP Payload Protocol ID)
   - **Value**: `53` (WebRTC DataChannel binary data)
   - **Current**: ProtoBuf
4. Click **OK**

#### Step 4: Specify the Message Type

For each protobuf dissection, you may need to specify the message type:

1. Right-click on a packet with protobuf data
2. Select **Protocol Preferences → ProtoBuf**
3. Set **Default message type**: `webrtc.Message`

Alternatively, you can configure this globally:
1. **Edit → Preferences → Protocols → ProtoBuf**
2. **Default message type**: `webrtc.Message`

#### Step 5: Verify Decoding

After configuration, you should see decoded protobuf fields in the packet details pane:

```
SCTP Payload Protocol Identifier: WebRTC Binary (53)
ProtoBuf Message
  ├─ webrtc.Message
  │  ├─ flag: FIN (0) [optional]
  │  └─ message: [bytes] [optional]
```

#### Useful Filters for Protobuf Analysis

Once protobuf decoding is enabled, you can filter by message fields:

- `protobuf` - Show all protobuf messages
- `protobuf.field.name == "flag"` - Messages with flag field
- `protobuf.field.name == "message"` - Messages with message payload
- `sctp.ppi == 53 && protobuf` - SCTP binary data with protobuf

#### Troubleshooting Protobuf Decoding

**Protobuf not shown in Decode As options**:
- Ensure Wireshark version >= 2.6 (protobuf support)
- Check that the protobuf search path is correct
- Restart Wireshark after changing protobuf settings

**"Malformed Packet" or decoding errors**:
- Verify the `.proto` file syntax is correct
- Check that the message type (`webrtc.Message`) matches
- Some packets may contain raw data or different encodings

**Empty or missing fields**:
- Protobuf fields are optional - not all messages will have all fields
- Check the actual protobuf wire format with "Show data as → Protocol Buffers"

## Requirements

- **text2pcap**: Part of Wireshark package, must be in PATH
- **Wireshark**: For viewing the captures (optional, will show command if not found)
- **Chrome/Chromium**: The browser must support WebRTC logging flags

## Installation of Dependencies

### macOS
```bash
brew install wireshark
```

### Linux (Debian/Ubuntu)
```bash
sudo apt-get install wireshark tshark
```

### Linux (Fedora)
```bash
sudo dnf install wireshark
```

## Technical Details

The implementation uses Chrome's debug logging infrastructure:

- **Chrome Flag**: `--vmodule=*/webrtc/*=1` enables verbose logging for WebRTC components
- **Log Format**: Chrome outputs SCTP packets in text2pcap-compatible format
- **Link Layer Type**: Uses 248 (SCTP_ASSOC) for proper Wireshark dissection
- **Timestamp Preservation**: Maintains original packet timing from Chrome logs

## Troubleshooting

### No SCTP_PACKET lines found

This usually means:
- The WebRTC connection didn't establish properly
- Chrome version doesn't support SCTP logging
- The verbosity level is too low

Try increasing verbosity:
```rust
// In the code, change:
OsStr::new("--vmodule=*/webrtc/*=3"),  // Increase from 1 to 3
```

### text2pcap not found

Install Wireshark which includes text2pcap:
```bash
# macOS
brew install wireshark

# Linux
sudo apt-get install wireshark
```

### Wireshark launch failed

The tool will print the command to open manually:
```bash
wireshark /tmp/smoldot-sctp-*.pcapng
```

### Empty or corrupt pcapng file

- Check that the Chrome log file exists and has content
- Verify text2pcap succeeded (check stderr output)
- Ensure the WebRTC connection completed successfully

## References

This implementation is based on the WebRTC debugging technique described in:
- [Google Groups: discuss-webrtc - SSLKEYLOGFILE for WebRTC DTLS](https://groups.google.com/g/discuss-webrtc/c/b8wNMUhP_K8)
- [GitHub Gist: Debug encrypted WebRTC DataChannel](https://gist.github.com/rskvazh/8e1551102a1d31b7cde82bc9a464ef41)
- [Mozilla: Debugging WebRTC Calls](https://firefox-source-docs.mozilla.org/contributing/debugging/debugging_webrtc_calls.html)
