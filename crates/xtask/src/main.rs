use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("install") => install(),
        Some(cmd) => {
            eprintln!("Unknown command: {}", cmd);
            eprintln!("Available commands: install");
            std::process::exit(1);
        }
        None => {
            eprintln!("Usage: cargo xtask <command>");
            eprintln!("Available commands: install");
            std::process::exit(1);
        }
    }
}

fn install() {
    // Build release binary
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "tracey"])
        .status()
        .expect("Failed to run cargo build");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    let src = "target/release/tracey";

    // On macOS, codesign the binary to avoid Gatekeeper issues
    #[cfg(target_os = "macos")]
    {
        println!("Signing binary...");
        let status = Command::new("codesign")
            .args(["--sign", "-", "--force", src])
            .status()
            .expect("Failed to run codesign");

        if !status.success() {
            eprintln!("Warning: codesign failed, continuing anyway");
        }
    }

    // Verify the binary works before installing
    println!("Verifying binary...");
    let output = Command::new(src)
        .arg("--version")
        .output()
        .expect("Failed to run tracey --version");

    if !output.status.success() {
        eprintln!("Error: tracey --version failed");
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    let version = String::from_utf8_lossy(&output.stdout);
    println!("Built: {}", version.trim());

    // Copy to ~/.cargo/bin
    let home = std::env::var("HOME").expect("HOME not set");
    let dst = format!("{}/.cargo/bin/tracey", home);

    std::fs::copy(src, &dst).expect("Failed to copy binary");
    println!("Installed tracey to {}", dst);
}
