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
    // Build the dashboard frontend
    let dashboard_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/tracey/src/bridge/http/dashboard");
    let status = Command::new("pnpm")
        .args(["run", "build"])
        .current_dir(&dashboard_dir)
        .status()
        .expect("Failed to run pnpm run build (is pnpm installed?)");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    // Build release binary
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "tracey"])
        .status()
        .expect("Failed to run cargo build");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    let src = "target/release/tracey";

    // Copy to ~/.cargo/bin
    let home = std::env::var("HOME").expect("HOME not set");
    let dst = format!("{}/.cargo/bin/tracey", home);

    std::fs::copy(src, &dst).expect("Failed to copy binary");
    println!("Copied tracey to {}", dst);

    // On macOS, codesign the installed binary to avoid AMFI issues
    // (signing must happen AFTER copy, not before)
    #[cfg(target_os = "macos")]
    {
        println!("Signing installed binary...");
        let status = Command::new("codesign")
            .args(["--sign", "-", "--force", &dst])
            .status()
            .expect("Failed to run codesign");

        if !status.success() {
            eprintln!("Warning: codesign failed, continuing anyway");
        }
    }

    // Verify the installed binary works
    println!("Verifying installation...");
    let output = Command::new(&dst)
        .arg("--version")
        .output()
        .expect("Failed to run tracey --version");

    if !output.status.success() {
        eprintln!("Error: tracey --version failed");
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    let version = String::from_utf8_lossy(&output.stdout);
    println!("Installed: {}", version.trim());
}
