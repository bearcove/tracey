//! Tracey daemon - persistent server for a workspace.
//!
//! The daemon owns the `DashboardData` and exposes the `TraceyDaemon` RPC service
//! over a Unix socket. HTTP, MCP, and LSP bridges connect as clients.
//!
//! ## Socket Location
//!
//! The daemon listens on `.tracey/daemon.sock` in the workspace root.
//!
//! ## Lifecycle
//!
//! - Daemon is started by the first bridge that needs it
//! - Daemon exits after idle timeout (no connections for N minutes)
//! - Stale socket files are cleaned up on connect failure

pub mod client;
mod connection;
pub mod engine;
mod framing;
pub mod service;

use eyre::{Result, WrapErr};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use roam_wire::Hello;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixListener;
use tracing::{debug, error, info, warn};

use connection::hello_exchange_acceptor;
use framing::CobsFramedUnix;

pub use client::{DaemonClient, ensure_daemon_running};
pub use engine::Engine;
pub use service::{TraceyDispatcher, TraceyService};

/// Default idle timeout in seconds (10 minutes)
#[allow(dead_code)]
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 600;

/// Socket file name within .tracey directory
const SOCKET_FILENAME: &str = "daemon.sock";

/// Get the socket path for a workspace.
pub fn socket_path(project_root: &Path) -> PathBuf {
    project_root.join(".tracey").join(SOCKET_FILENAME)
}

/// Ensure the .tracey directory exists.
pub fn ensure_tracey_dir(project_root: &Path) -> Result<PathBuf> {
    let dir = project_root.join(".tracey");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Run the daemon for the given workspace.
///
/// This function blocks until the daemon exits (idle timeout or signal).
pub async fn run(project_root: PathBuf, config_path: PathBuf) -> Result<()> {
    info!("Starting tracey daemon for {}", project_root.display());

    // Ensure .tracey directory exists
    ensure_tracey_dir(&project_root)?;

    // Create socket path
    let sock_path = socket_path(&project_root);

    // Remove stale socket if it exists
    if sock_path.exists() {
        info!("Removing stale socket at {}", sock_path.display());
        std::fs::remove_file(&sock_path)?;
    }

    // Create engine
    let engine = Arc::new(
        Engine::new(project_root.clone(), config_path.clone())
            .await
            .wrap_err("Failed to initialize engine")?,
    );

    // Create service
    let service = Arc::new(TraceyService::new(Arc::clone(&engine)));

    // Set up file watcher
    let (watcher_tx, mut watcher_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Spawn file watcher in a separate OS thread
    let config_path_for_watcher = config_path.clone();
    let project_root_for_watcher = project_root.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for watcher");

        rt.block_on(async {
            if let Err(e) = run_file_watcher(
                &project_root_for_watcher,
                &config_path_for_watcher,
                watcher_tx,
            )
            .await
            {
                error!("File watcher error: {}", e);
            }
        });
    });

    // Spawn rebuild task that listens for watcher events
    let engine_for_rebuild = Arc::clone(&engine);
    tokio::spawn(async move {
        while watcher_rx.recv().await.is_some() {
            debug!("File watcher triggered rebuild");
            if let Err(e) = engine_for_rebuild.rebuild().await {
                error!("Rebuild failed: {}", e);
            }
        }
    });

    // Bind Unix socket
    let listener = UnixListener::bind(&sock_path)
        .wrap_err_with(|| format!("Failed to bind socket at {}", sock_path.display()))?;

    info!("Daemon listening on {}", sock_path.display());

    // Default Hello configuration
    let hello = Hello::V1 {
        max_payload_size: 1024 * 1024,    // 1MB max payload
        initial_stream_credit: 64 * 1024, // 64KB stream credit
    };

    // Accept connections and handle roam RPC
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                info!("New connection accepted");
                let service = Arc::clone(&service);
                let hello = hello.clone();
                tokio::spawn(async move {
                    // Wrap in COBS framing
                    let io = CobsFramedUnix::new(stream);

                    // Perform Hello exchange
                    match hello_exchange_acceptor(io, hello).await {
                        Ok(mut conn) => {
                            info!("Hello exchange completed");
                            // Run the message loop
                            if let Err(e) = conn.run(&*service).await {
                                match e {
                                    connection::ConnectionError::Closed => {
                                        info!("Connection closed cleanly");
                                    }
                                    connection::ConnectionError::ProtocolViolation {
                                        rule_id,
                                        ..
                                    } => {
                                        warn!("Protocol violation: {}", rule_id);
                                    }
                                    connection::ConnectionError::Io(e) => {
                                        error!("IO error: {}", e);
                                    }
                                    connection::ConnectionError::Dispatch(e) => {
                                        error!("Dispatch error: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Hello exchange failed: {:?}", e);
                        }
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}

/// Run the file watcher, sending events to the channel.
async fn run_file_watcher(
    project_root: &Path,
    config_path: &Path,
    tx: tokio::sync::mpsc::Sender<()>,
) -> Result<()> {
    let tx_clone = tx.clone();
    let mut debouncer = new_debouncer(Duration::from_millis(200), move |_events| {
        // Just send a signal, the rebuild task will handle it
        let _ = tx_clone.blocking_send(());
    })?;

    // Watch project root recursively
    debouncer
        .watcher()
        .watch(project_root, RecursiveMode::Recursive)?;

    // Also watch config file specifically
    debouncer
        .watcher()
        .watch(config_path, RecursiveMode::NonRecursive)?;

    info!("File watcher started for {}", project_root.display());

    // Keep the watcher alive
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

/// Check if a daemon is running for the given workspace.
#[allow(dead_code)]
pub async fn is_running(project_root: &Path) -> bool {
    let sock = socket_path(project_root);
    if !sock.exists() {
        return false;
    }

    // Try to connect
    match tokio::net::UnixStream::connect(&sock).await {
        Ok(_) => true,
        Err(_) => {
            // Socket exists but can't connect - stale
            false
        }
    }
}

/// Connect to a running daemon, or return an error.
#[allow(dead_code)]
pub async fn connect(project_root: &Path) -> Result<tokio::net::UnixStream> {
    let sock = socket_path(project_root);
    tokio::net::UnixStream::connect(&sock)
        .await
        .wrap_err_with(|| format!("Failed to connect to daemon at {}", sock.display()))
}
