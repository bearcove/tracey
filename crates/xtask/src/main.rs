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
    let status = Command::new("cargo")
        .args(["install", "--path", "./crates/tracey"])
        .status()
        .expect("Failed to run cargo install");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
