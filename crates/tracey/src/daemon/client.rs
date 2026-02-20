//! Client for connecting to the tracey daemon.
//!
//! Uses roam's `connect()` with auto-reconnection.

use roam_stream::{Connector, HandshakeConfig, NoDispatcher, connect};
use std::fs::OpenOptions;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::info;

use super::local_endpoint;

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
        // Find the tracey executable
        let exe = std::env::current_exe().map_err(io::Error::other)?;

        // Determine config path
        let config_path = self.project_root.join(".config/tracey/config.styx");

        info!("Auto-starting daemon for {}", self.project_root.display());

        // Spawn daemon process detached
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
            // CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS
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
                    // Recover from stale lock files left behind by crashed startup attempts.
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
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(10);

        loop {
            if let Ok(stream) = roam_local::connect(&endpoint).await {
                info!("Connected to daemon");
                return Ok(stream);
            }

            if start.elapsed() > timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "Daemon failed to start within {} seconds. Check logs at {}/.tracey/daemon.log",
                        timeout.as_secs(),
                        self.project_root.display()
                    ),
                ));
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

impl Connector for DaemonConnector {
    type Transport = roam_local::LocalStream;

    async fn connect(&self) -> io::Result<Self::Transport> {
        let endpoint = local_endpoint(&self.project_root);

        // Try to connect to existing daemon
        match roam_local::connect(&endpoint).await {
            Ok(stream) => {
                info!("Connected to daemon");
                Ok(stream)
            }
            Err(_) => {
                // Serialize startup so multiple bridges don't all spawn daemons at once.
                let _startup_lock = self.acquire_startup_lock(Duration::from_secs(15))?;

                // Another process may have started the daemon while we waited for lock.
                if let Ok(stream) = roam_local::connect(&endpoint).await {
                    info!("Connected to daemon");
                    return Ok(stream);
                }

                // r[impl daemon.lifecycle.stale-socket]
                if roam_local::endpoint_exists(&endpoint)
                    && roam_local::connect(&endpoint).await.is_err()
                {
                    let _ = roam_local::remove_endpoint(&endpoint);
                }

                // Auto-start the daemon
                self.spawn_daemon()?;

                // Wait for daemon to be ready
                self.wait_and_connect().await
            }
        }
    }
}
