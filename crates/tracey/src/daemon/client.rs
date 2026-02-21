//! Client for connecting to the tracey daemon.
//!
//! Uses roam's `connect()` with auto-reconnection.

use roam_stream::{Connector, HandshakeConfig, NoDispatcher, connect};
use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing::{info, warn};

use super::{local_endpoint, pid_file_path};

// Re-export the generated client from tracey-proto
pub use tracey_proto::TraceyDaemonClient;

/// Type alias for the full daemon client type.
pub type DaemonClient = TraceyDaemonClient<roam_stream::Client<DaemonConnector, NoDispatcher>>;

/// Create a new daemon client for the given project root.
///
/// The client will automatically:
/// - Connect to the daemon on first use (lazy)
/// - Start the daemon if it's not running
/// - Reconnect transparently if the connection drops
pub fn new_client(project_root: PathBuf) -> DaemonClient {
    let connector = DaemonConnector::new(project_root);
    let client = connect(connector, HandshakeConfig::default(), NoDispatcher);
    TraceyDaemonClient::new(client)
}

/// Connector that establishes connections to the tracey daemon.
///
/// r[impl daemon.lifecycle.auto-start]
///
/// If the daemon is not running, this will automatically spawn it
/// and wait for it to be ready before connecting.
pub struct DaemonConnector {
    project_root: PathBuf,
}

struct StartupLock {
    path: PathBuf,
    #[allow(dead_code)]
    file: std::fs::File,
}

impl Drop for StartupLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl DaemonConnector {
    /// Create a new connector for the given project root.
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Spawn the daemon process in the background.
    fn spawn_daemon(&self) -> io::Result<()> {
        let exe = std::env::current_exe().map_err(io::Error::other)?;
        let config_path = self.project_root.join(".config/tracey/config.styx");

        info!("Auto-starting daemon for {}", self.project_root.display());

        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("daemon")
            .arg(&self.project_root)
            .arg("--config")
            .arg(&config_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x00000200 | 0x00000008);
        }

        cmd.spawn()
            .map_err(|e| io::Error::other(format!("Failed to spawn daemon: {e}")))?;

        Ok(())
    }

    fn startup_lock_path(&self) -> PathBuf {
        self.project_root.join(".tracey").join("daemon-start.lock")
    }

    fn acquire_startup_lock(&self, timeout: Duration) -> io::Result<StartupLock> {
        super::ensure_tracey_dir(&self.project_root).map_err(io::Error::other)?;

        let lock_path = self.startup_lock_path();
        let started = Instant::now();

        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    writeln!(file, "pid={}", std::process::id())?;
                    return Ok(StartupLock {
                        path: lock_path,
                        file,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if let Ok(meta) = std::fs::metadata(&lock_path)
                        && let Ok(modified) = meta.modified()
                        && modified.elapsed().unwrap_or_default() > Duration::from_secs(30)
                    {
                        let _ = std::fs::remove_file(&lock_path);
                        continue;
                    }

                    if started.elapsed() > timeout {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!(
                                "Timed out waiting for daemon startup lock at {}",
                                lock_path.display()
                            ),
                        ));
                    }

                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Wait for the daemon endpoint to appear and connect.
    async fn wait_and_connect(&self) -> io::Result<roam_local::LocalStream> {
        let endpoint = local_endpoint(&self.project_root);
        let start = Instant::now();
        let timeout = Duration::from_secs(5);
        let mut last_print_secs = 0u64;

        loop {
            if let Ok(stream) = roam_local::connect(&endpoint).await {
                return Ok(stream);
            }

            let elapsed = start.elapsed();

            if elapsed > timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "Daemon failed to start within {}s. Check logs at {}/.tracey/daemon.log",
                        timeout.as_secs(),
                        self.project_root.display()
                    ),
                ));
            }

            // Print a progress line once per second so CLI users know we're waiting.
            let secs = elapsed.as_secs();
            if secs > last_print_secs {
                last_print_secs = secs;
                let dots = ".".repeat(secs as usize);
                info!("Starting daemon{dots}");
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

/// Read the PID file and return `(pid, protocol_version)` if it parses correctly.
/// Returns `None` if the file doesn't exist. Logs a warning and returns `None`
/// if the file exists but is malformed.
fn read_pid_file(project_root: &Path) -> Option<(u32, u32)> {
    let path = pid_file_path(project_root);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => {
            warn!("Failed to read PID file {}: {e}", path.display());
            return None;
        }
    };

    let mut pid = None;
    let mut version = None;
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("pid=") {
            pid = v.parse().ok();
        } else if let Some(v) = line.strip_prefix("version=") {
            version = v.parse().ok();
        }
    }

    match (pid, version) {
        (Some(p), Some(v)) => Some((p, v)),
        _ => {
            warn!(
                "PID file {} has unexpected format, ignoring it",
                path.display()
            );
            None
        }
    }
}

/// Check whether a process with the given PID is alive.
#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    // Signal 0 doesn't send a signal; it just checks whether the process exists.
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe { kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn is_pid_alive(_pid: u32) -> bool {
    true // best-effort on non-Unix; rely on socket connect to detect dead daemon
}

/// Send SIGTERM to a process.
#[cfg(unix)]
fn kill_pid(pid: u32) {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe {
        kill(pid as i32, 15); // SIGTERM
    }
}

#[cfg(not(unix))]
fn kill_pid(_pid: u32) {}

impl Connector for DaemonConnector {
    type Transport = roam_local::LocalStream;

    async fn connect(&self) -> io::Result<Self::Transport> {
        let endpoint = local_endpoint(&self.project_root);

        match read_pid_file(&self.project_root) {
            Some((pid, version)) => {
                let alive = is_pid_alive(pid);
                let version_ok = version == tracey_proto::PROTOCOL_VERSION;

                if alive && version_ok {
                    // Happy path: daemon should be running.
                    if let Ok(stream) = roam_local::connect(&endpoint).await {
                        return Ok(stream);
                    }
                    // Socket connect failed despite live PID — stale socket.
                    let _ = roam_local::remove_endpoint(&endpoint);
                    let _ = std::fs::remove_file(pid_file_path(&self.project_root));
                } else {
                    // Kill if alive but wrong version, then clean up.
                    if alive {
                        info!(
                            running = version,
                            current = tracey_proto::PROTOCOL_VERSION,
                            "Daemon protocol version mismatch, restarting",
                        );
                        kill_pid(pid);
                    }
                    let _ = roam_local::remove_endpoint(&endpoint);
                    let _ = std::fs::remove_file(pid_file_path(&self.project_root));
                }
            }
            None => {
                // No PID file — remove stale socket if present.
                // r[impl daemon.lifecycle.stale-socket]
                if roam_local::endpoint_exists(&endpoint) {
                    let _ = roam_local::remove_endpoint(&endpoint);
                }
            }
        }

        // Daemon is not running. Serialize startup across concurrent connectors.
        let _startup_lock = self.acquire_startup_lock(Duration::from_secs(5))?;

        // Re-check: another process may have started the daemon while we waited for the lock.
        if let Some((pid, version)) = read_pid_file(&self.project_root) {
            if is_pid_alive(pid) && version == tracey_proto::PROTOCOL_VERSION {
                if let Ok(stream) = roam_local::connect(&endpoint).await {
                    return Ok(stream);
                }
            }
        }

        self.spawn_daemon()?;
        self.wait_and_connect().await
    }
}
