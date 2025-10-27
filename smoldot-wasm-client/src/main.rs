use std::process::{Command, Stdio};
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_pack = std::env::var("WASM_PACK").unwrap_or_else(|_| "wasm-pack".to_string());

    // Build the wasm from this crate's lib using `wasm-pack build --target web --release`
    let mut cmd = Command::new(wasm_pack);
    cmd.args(["build", "--target", "web", "--release"]).stderr(Stdio::inherit()).stdout(Stdio::inherit());

    let status = cmd.status()?;
    if !status.success() {
        eprintln!("wasm-pack build failed");
        return Err("wasm-pack build failed".into());
    }

    println!("WASM build completed");

    io::stdout().flush().ok();
    Ok(())
}

