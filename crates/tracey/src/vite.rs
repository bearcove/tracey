//! Vite dev server management
//!
//! Spawns and manages a Vite dev server process, proxying requests to it.

use eyre::{Result, WrapErr};
use owo_colors::OwoColorize;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

/// Information about a running Vite dev server
pub struct ViteServer {
    /// The port Vite is listening on
    pub port: u16,
    /// Handle to the child process
    _child: Child,
}

impl ViteServer {
    /// Start a Vite dev server in the given directory
    pub async fn start(dashboard_dir: &Path) -> Result<Self> {
        eprintln!(
            "   {} Vite dev server in {}",
            "Starting".blue().bold(),
            dashboard_dir.display()
        );

        // Run pnpm install first to ensure dependencies are installed
        let install_status = Command::new("pnpm")
            .arg("install")
            .current_dir(dashboard_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .await
            .wrap_err("Failed to run pnpm install")?;

        if !install_status.success() {
            eyre::bail!("pnpm install failed");
        }

        // Channel to receive the port from stdout parsing
        let (tx, mut rx) = mpsc::channel::<u16>(1);

        // Start vite dev server
        let mut child = Command::new("pnpm")
            .arg("run")
            .arg("dev")
            .current_dir(dashboard_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .wrap_err("Failed to spawn Vite dev server")?;

        // Spawn tasks to read stdout/stderr and extract port
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            relay_output(stdout, tx_clone).await;
        });

        tokio::spawn(async move {
            relay_output(stderr, tx).await;
        });

        // Wait for port with timeout
        let port = tokio::time::timeout(std::time::Duration::from_secs(30), rx.recv())
            .await
            .wrap_err("Timeout waiting for Vite to start")?
            .ok_or_else(|| eyre::eyre!("Vite process exited before reporting port"))?;

        eprintln!(
            "   {} Vite dev server running on port {}",
            "OK".green().bold(),
            port
        );

        Ok(ViteServer {
            port,
            _child: child,
        })
    }
}

/// Read lines from a reader and extract the Vite port, forwarding other output
async fn relay_output<R: tokio::io::AsyncRead + Unpin>(reader: R, tx: mpsc::Sender<u16>) {
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        // Try to extract port from Vite's "Local: http://localhost:PORT/" output
        if let Some(port) = extract_vite_port(&line) {
            let _ = tx.send(port).await;
            // Don't print the localhost line - we'll print our own message
            continue;
        }

        // Skip empty lines
        if !line.trim().is_empty() {
            // Strip ANSI codes for cleaner output, or pass through
            eprintln!("   {} {}", "[vite]".dimmed(), line);
        }
    }
}

/// Extract the port from a Vite server output line
///
/// Vite outputs lines like:
///   ➜  Local:   http://localhost:5173/
/// possibly with ANSI escape codes
fn extract_vite_port(line: &str) -> Option<u16> {
    // Strip ANSI escape codes
    let stripped = strip_ansi_escapes(line);

    // Look for localhost URL pattern
    // Match "http://localhost:" or "http://127.0.0.1:" followed by port
    for pattern in &["http://localhost:", "http://127.0.0.1:"] {
        if let Some(idx) = stripped.find(pattern) {
            let after_pattern = &stripped[idx + pattern.len()..];
            // Extract digits until non-digit
            let port_str: String = after_pattern
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(port) = port_str.parse::<u16>() {
                return Some(port);
            }
        }
    }

    None
}

/// Simple ANSI escape code stripper
fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Skip until we hit a letter (the command)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_vite_port() {
        // Plain output
        assert_eq!(
            extract_vite_port("  ➜  Local:   http://localhost:5173/"),
            Some(5173)
        );

        // With ANSI codes (simplified test)
        assert_eq!(
            extract_vite_port("Local: http://localhost:3000/"),
            Some(3000)
        );

        // 127.0.0.1 variant
        assert_eq!(
            extract_vite_port("  ➜  Local:   http://127.0.0.1:5174/"),
            Some(5174)
        );

        // No match
        assert_eq!(extract_vite_port("Some other output"), None);
    }

    #[test]
    fn test_strip_ansi() {
        assert_eq!(strip_ansi_escapes("\x1b[32m➜\x1b[0m  Local"), "➜  Local");
    }
}
