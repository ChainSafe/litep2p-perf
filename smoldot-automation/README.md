# smoldot-automation

Automated browser-based performance testing for smoldot.

## Usage

```bash
smoldot-automation [--capture] <peer> <upload_bytes> <download_bytes>
```

### Arguments

- `<peer>`: The peer address to connect to
- `<upload_bytes>`: Number of bytes to upload during the test
- `<download_bytes>`: Number of bytes to download during the test
- `--capture`: (Optional) Output a command for capturing WebRTC traffic instead of running the test

### Examples

**Run a performance test:**

```bash
smoldot-automation /ip4/127.0.0.1/tcp/30333/p2p/12D3KooW... 1000000 1000000
```

**Generate a traffic capture command:**

```bash
smoldot-automation --capture /ip4/127.0.0.1/tcp/30333/p2p/12D3KooW... 1000000 1000000
```

## Capture Mode

When the `--capture` flag is provided, the program outputs a command line that can be used to capture WebRTC SCTP traffic to a pcap file. This is useful for debugging network issues or analyzing protocol behavior.

### Requirements

The capture command requires:

1. **Chrome/Chromium browser** - The path `/path/to/chrome` in the output must be replaced with the actual path to your Chrome or Chromium binary
2. **Wireshark tools** - Specifically `text2pcap`, which is included with Wireshark installations
3. **grep** - Standard Unix tool for filtering output

### Important Notes

- **Replace `/path/to/chrome`**: The output command uses a placeholder. You must replace it with the actual path to your Chrome executable (e.g., `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` on macOS, or `google-chrome` on Linux if it's in your PATH)

- **Chrome must not be running**: The capture mode may only work correctly when Chrome is not already running. If Chrome is already open, the executable may delegate to the existing instance instead of starting with the required logging flags, which will prevent traffic capture from working.

- **Output file**: The capture is saved to `out-<timestamp>.pcapng` where `<timestamp>` is the Unix timestamp in seconds when the command is generated

### Example Workflow

1. Generate the capture command:
```bash
smoldot-automation --capture /ip4/127.0.0.1/tcp/30333/p2p/12D3KooW... 1000000 1000000
```

2. Copy the output command and replace `/path/to/chrome` with your Chrome path:
```bash
/Applications/Google\ Chrome.app/Contents/MacOS/Google\ Chrome --guest \
    --auto-open-devtools-for-tabs \
    --enable-logging=stderr --log-level=0 --v=0 \
    --vmodule='*/webrtc/*=1' "http://127.0.0.1:8082/index.html?peer=..." \
     2>&1 | grep -F SCTP_PACKET | text2pcap -D -t %H:%M:%S.%f -i 132 - out-1738876543.pcapng
```

3. Make sure Chrome is not running, then execute the command.

4. Quit Chrome once the `litep2p-perf` run has finished.
 
5. The resulting `.pcapng` file can be opened with Wireshark for analysis.
