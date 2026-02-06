use std::env;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let pcap_filename = format!("out-{}.pcapng", timestamp);

    if params.capture {
        let cmd = browser_command_line(&url, &pcap_filename)?;
        println!("Run this command: {}", cmd);
    } else {
        run_browser(&url)?;
    }

    let durations = done_rx.recv()?;

    if params.capture {
        println!("You can now open {} in Wireshark", pcap_filename);
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

fn run_browser(url: &str) -> Result<()> {
    tracing::debug!("Opening browser at {}", url);

    let mut options = headless_chrome::LaunchOptions::default();
    options.idle_browser_timeout = Duration::from_secs(120);

    let browser = headless_chrome::Browser::new(options)?;
    let tab = browser.new_tab()?;

    tab.navigate_to(url)?;
    tab.wait_until_navigated()?;
    tab.wait_for_element("#perf-finished")?;
    Ok(())
}

fn browser_command_line(url: &str, pcap_filename: &str) -> Result<String> {
    Ok(format!(
        "/path/to/chrome --guest \\\n    \
             --auto-open-devtools-for-tabs \\\n    \
             --enable-logging=stderr --log-level=0 --v=0 \\\n    \
             --vmodule='*/webrtc/*=1' \"{}\" \\\n     \
             2>&1 | grep -F SCTP_PACKET | text2pcap -D -t %H:%M:%S.%f -i 132 - {}",
        url, pcap_filename,
    ))
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
