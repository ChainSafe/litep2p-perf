use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::io::Write;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_ansi(false)
        .init();

    let args: Vec<String> = env::args().collect();
    let params = parse_args(&args)?;
    let host = "127.0.0.1:8082";

    build_wasm()?;

    let (done_tx, done_rx) = mpsc::channel();

    thread::spawn(move || {
        run_server(host, done_tx);
    });

    tracing::debug!("Server is running. Press Ctrl+C to stop.");

    thread::sleep(Duration::from_secs(2));

    let url = format!(
        "http://{}/index.html?peer={}&upload_bytes={}&download_bytes={}&autorun=true",
        host, params.peer, params.upload_bytes, params.download_bytes,
    );

    let capture_files = run_browser(&url, params.capture)?;

    let durations = done_rx.recv()?;

    if let Some(files) = capture_files {
        pcap_from_log(&files.log_path, &files.pcap_path)?;

        println!("\nCapture files:");
        println!("  SCTP log: {}", files.log_path);
        println!("  PCAP: {}", files.pcap_path);

        if let Err(e) = launch_wireshark(&files.pcap_path) {
            eprintln!("Warning: Could not launch Wireshark: {}", e);
            println!("\nOpen the pcap file manually with:");
            println!("  wireshark {}", files.pcap_path);
        } else {
            println!("\nWireshark launched successfully");
        }
    } else {
        println!(
            "Uploaded {} bytes in {:.4}s bandwidth {}",
            utils::format_bytes(params.upload_bytes as usize),
            durations.upload_seconds,
            utils::format_bandwidth(
                Duration::from_secs_f64(durations.upload_seconds),
                params.upload_bytes as usize,
            )
        );

        println!(
            "Downloaded {} bytes in {:.4}s bandwidth {}",
            utils::format_bytes(params.download_bytes as usize),
            durations.download_seconds,
            utils::format_bandwidth(
                Duration::from_secs_f64(durations.download_seconds),
                params.download_bytes as usize,
            )
        );
    }

    Ok(())
}

#[derive(serde::Deserialize)]
struct Durations {
    upload_seconds: f64,
    download_seconds: f64,
}

fn run_server(host: &str, tx: mpsc::Sender<Durations>) {
    tracing::debug!("Starting web server on {}", host);

    rouille::start_server(host, move |request| {
        if request.method() == "POST" && request.url() == "/results" {
            let durations: Durations = rouille::try_or_400!(rouille::input::json_input(request));
            let _ = tx.send(durations);
            return rouille::Response::empty_204();
        }

        let response = rouille::match_assets(request, "./smoldot-perf");

        if response.is_success() {
            response
        } else {
            rouille::Response::html("<h1>404 Not Found</h1>").with_status_code(404)
        }
    });
}

struct CaptureFiles {
    log_path: String,
    pcap_path: String,
}

fn run_browser(url: &str, capture: bool) -> Result<Option<CaptureFiles>> {
    tracing::debug!("Opening browser at {}", url);

    let mut capture_files = None;
    let mut options = headless_chrome::LaunchOptions::default();
    options.idle_browser_timeout = Duration::from_secs(120);

    if capture {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let log_path = format!("/tmp/smoldot-sctp-{}.log", timestamp);
        let pcap_path = format!("/tmp/smoldot-sctp-{}.pcapng", timestamp);

        capture_files = Some(CaptureFiles {
            log_path: log_path.clone(),
            pcap_path,
        });

        options.devtools = true; // turns off "headless"

        options.process_envs = Some(HashMap::from([
            ("CHROME_LOG_FILE".into(), log_path.into())
        ]));

        options.args = vec![
            OsStr::new("--guest"),
            OsStr::new("--enable-logging"),
            OsStr::new("--log-level=0"),
            OsStr::new("--v=0"),
            OsStr::new("--vmodule=\"*/webrtc/*=1\""),
        ];
    }

    let browser = headless_chrome::Browser::new(options)?;
    let tab = browser.new_tab()?;

    tab.navigate_to(url)?;
    tab.wait_until_navigated()?;
    tab.wait_for_element("#perf-finished")?;
    Ok(capture_files)
}

fn pcap_from_log(log_path: &str, pcap_path: &str) -> Result<()> {
    use std::{fs, io};

    let log = fs::read_to_string(log_path).map_err(|e| {
        io::Error::new(e.kind(), format!("Failed to read Chrome log file at '{log_path}': {e}"))
    })?;

    let sctp_lines: Vec<&str> = log
        .lines()
        .filter(|line| line.contains("SCTP_PACKET"))
        .collect();

    if sctp_lines.is_empty() {
        return Err(format!("No SCTP_PACKET lines found in Chrome log file at '{log_path}'").into());
    }

    // Write SCTP lines to text2pcap via stdin
    let mut child = Command::new("text2pcap")
        .args(&[
            "-n",           // No IP header
            "-l", "248",    // Link layer type 248 (SCTP)
            "-D",           // Indicate packet direction
            "-t", "%H:%M:%S.",  // Timestamp format
            "-",            // Read from stdin
            pcap_path,
        ])
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        for line in sctp_lines {
            writeln!(stdin, "{}", line)?;
        }
    }

    let output = child.wait_with_output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("text2pcap failed: {}", stderr).into());
    }

    tracing::info!("Created pcap file: {}", pcap_path);
    Ok(())
}

fn launch_wireshark(pcap_path: &str) -> Result<()> {
    tracing::debug!("Launching Wireshark with {}", pcap_path);

    // Try to launch Wireshark (path varies by OS)
    let wireshark_paths = vec![
        "/Applications/Wireshark.app/Contents/MacOS/Wireshark",  // macOS
        "wireshark",  // Linux/PATH
        "/usr/bin/wireshark",  // Linux
        "/usr/local/bin/wireshark",  // Alternative
    ];

    for path in wireshark_paths {
        if let Ok(child) = Command::new(path)
            .arg(pcap_path)
            .spawn()
        {
            tracing::debug!("Launched Wireshark successfully");
            // Detach from the child process
            let _ = child.id();
            return Ok(());
        }
    }

    Err("Could not launch Wireshark. Is it installed?".into())
}

fn build_wasm() -> Result<()> {
    if !is_wasm_target_installed()? {
        return Err(
            "wasm32-unknown-unknown target is not installed. Run 'rustup target add \
        wasm32-unknown-unknown'"
                .into(),
        );
    }

    check_wasm_bindgen_version()?;

    let cwd = env::current_dir()?;

    let smoldot_dir = cwd.join("smoldot-perf");
    let output_dir = smoldot_dir.join("pkg");

    let target_dir = cwd.join("target/wasm32-unknown-unknown/release");
    let wasm_path = target_dir.join("smoldot_perf.wasm");

    // build wasm from rust
    let output = Command::new("cargo")
        .current_dir(smoldot_dir)
        .args(&["build", "--release", "--target", "wasm32-unknown-unknown"])
        .output()?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("cargo build failed: \n{}\n{}", stdout, stderr);
        return Err("build failed".into());
    }

    // generate wasm/js bindings
    let output = Command::new("wasm-bindgen")
        .current_dir(cwd)
        .arg("--target")
        .arg("web")
        .arg("--out-dir")
        .arg(&output_dir)
        .arg(&wasm_path)
        .output()?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!("{}\n{}", stdout, stderr);
        return Err("build failed".into());
    }

    Ok(())
}

struct Params {
    peer: String,
    upload_bytes: u64,
    download_bytes: u64,
    capture: bool,
}

fn parse_args(args: &[String]) -> Result<Params> {
    let mut capture = false;
    let mut positional = Vec::new();

    for arg in &args[1..] {
        if arg == "--capture" {
            capture = true;
        } else {
            positional.push(arg.as_str());
        }
    }

    if positional.len() < 3 {
        eprintln!(
            "Usage: {} [--capture] <peer> <upload_bytes> <download_bytes>",
            args[0]
        );
        return Err("Missing required arguments".into());
    }

    let peer = positional[0];

    let upload_bytes = positional[1].parse::<u64>().map_err(|_| {
        format!(
            "Error: 'upload_bytes' must be a valid positive integer (found: '{}')",
            positional[1]
        )
    })?;

    let download_bytes = positional[2].parse::<u64>().map_err(|_| {
        format!(
            "Error: 'download_bytes' must be a valid positive integer (found: '{}')",
            positional[2]
        )
    })?;

    Ok(Params {
        peer: peer.to_string(),
        upload_bytes,
        download_bytes,
        capture,
    })
}

fn is_wasm_target_installed() -> Result<bool> {
    let output = Command::new("rustup")
        .args(&["target", "list", "--installed"])
        .output()?;

    if !output.status.success() {
        return Err("failed to execute rustup".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .any(|line| line.contains("wasm32-unknown-unknown")))
}

fn check_wasm_bindgen_version() -> Result<()> {
    let manifest_content = std::fs::read_to_string("smoldot-perf/Cargo.toml")?;
    let expected_version = manifest_content
        .lines()
        .find(|line| line.trim().starts_with("wasm-bindgen ="))
        .and_then(|line| line.split('"').nth(1))
        .ok_or("Could not find wasm-bindgen version in smoldot-perf/Cargo.toml")?;

    let output = Command::new("wasm-bindgen")
        .arg("--version")
        .output()
        .map_err(|_| {
            format!(
                "wasm-bindgen-cli is not installed. Run 'cargo install wasm-bindgen-cli@={}'",
                expected_version,
            )
        })?;

    let actual_version_output = String::from_utf8_lossy(&output.stdout);
    let actual_version = actual_version_output
        .split_whitespace()
        .nth(1)
        .ok_or("Could not parse wasm-bindgen version")?;

    if actual_version != expected_version {
        return Err(format!(
            "wasm-bindgen-cli version mismatch. Expected {}, found {}. Run 'cargo install \
            wasm-bindgen-cli@={}'",
            expected_version, actual_version, expected_version,
        )
        .into());
    }

    Ok(())
}
