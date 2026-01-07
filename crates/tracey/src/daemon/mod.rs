//! Tracey daemon - persistent server for a workspace.
//!
//! r[impl daemon.state.single-source]
//!
//! The daemon owns the `DashboardData` and exposes the `TraceyDaemon` RPC service
//! over a Unix socket. HTTP, MCP, and LSP bridges connect as clients.
//!
//! ## Socket Location
//!
//! r[impl daemon.lifecycle.socket]
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
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::net::UnixListener;
use tracing::{error, info, warn};

use connection::hello_exchange_acceptor;
use framing::CobsFramedUnix;

pub use client::{DaemonClient, ensure_daemon_running};
pub use engine::Engine;
pub use service::TraceyService;

/// Default idle timeout in seconds (10 minutes)
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 600;

/// Socket file name within .tracey directory
const SOCKET_FILENAME: &str = "daemon.sock";

/// Get the socket path for a workspace.
///
/// r[impl daemon.roam.unix-socket]
pub fn socket_path(project_root: &Path) -> PathBuf {
    project_root.join(".tracey").join(SOCKET_FILENAME)
}

/// Ensure the .tracey directory exists and is gitignored.
pub fn ensure_tracey_dir(project_root: &Path) -> Result<PathBuf> {
    let dir = project_root.join(".tracey");
    std::fs::create_dir_all(&dir)?;

    // Ensure .tracey/ is in .gitignore
    let gitignore_path = project_root.join(".gitignore");
    let needs_entry = if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
        !content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == ".tracey" || trimmed == ".tracey/" || trimmed == "/.tracey/"
        })
    } else {
        true
    };

    if needs_entry {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&gitignore_path)?;
        // Add newline before if file exists and doesn't end with newline
        if gitignore_path.exists() {
            let content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
            if !content.is_empty() && !content.ends_with('\n') {
                writeln!(file)?;
            }
        }
        writeln!(file, ".tracey/")?;
        info!("Added .tracey/ to .gitignore");
    }

    Ok(dir)
}

/// Run the daemon for the given workspace.
///
/// r[impl daemon.roam.protocol]
///
/// This function blocks until the daemon exits (idle timeout or signal).
pub async fn run(project_root: PathBuf, config_path: PathBuf) -> Result<()> {
    // r[impl daemon.logs.file]
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

    // r[impl daemon.state.file-watcher]
    // Set up file watcher - channel sends list of changed files
    let (watcher_tx, mut watcher_rx) = tokio::sync::mpsc::channel::<Vec<PathBuf>>(16);

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
    let project_root_for_rebuild = project_root.clone();
    tokio::spawn(async move {
        // Build gitignore matcher for filtering file watcher events
        let gitignore = {
            let mut builder = ignore::gitignore::GitignoreBuilder::new(&project_root_for_rebuild);
            // Add .gitignore from project root if it exists
            let gitignore_path = project_root_for_rebuild.join(".gitignore");
            if gitignore_path.exists() {
                let _ = builder.add(&gitignore_path);
            }
            // Always ignore .git directory
            let _ = builder.add_line(None, ".git/");
            builder.build().unwrap_or_else(|e| {
                warn!("Failed to build gitignore matcher: {}", e);
                ignore::gitignore::Gitignore::empty()
            })
        };

        while let Some(changed_files) = watcher_rx.recv().await {
            // Filter out paths that match gitignore patterns
            let relative_paths: Vec<_> = changed_files
                .iter()
                .filter_map(|p| p.strip_prefix(&project_root_for_rebuild).ok())
                .filter(|p| {
                    // Keep paths that are NOT ignored
                    let full_path = project_root_for_rebuild.join(p);
                    !gitignore
                        .matched_path_or_any_parents(&full_path, full_path.is_dir())
                        .is_ignore()
                })
                .collect();

            // Skip rebuild if no relevant files changed
            if relative_paths.is_empty() {
                continue;
            }

            if relative_paths.len() <= 3 {
                info!(
                    "File change detected: {}",
                    relative_paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            } else {
                info!(
                    "File changes detected: {} and {} more",
                    relative_paths
                        .iter()
                        .take(2)
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    relative_paths.len() - 2
                );
            }

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

    // r[impl daemon.lifecycle.idle-timeout]
    // Track active connections and last activity for idle timeout
    let active_connections = Arc::new(AtomicUsize::new(0));
    let last_activity = Arc::new(AtomicU64::new(
        Instant::now().elapsed().as_secs(), // Will be updated on each connection
    ));
    let start_time = Instant::now();

    // Accept connections and handle roam RPC
    loop {
        // Check idle timeout every 30 seconds
        let accept_result = tokio::time::timeout(Duration::from_secs(30), listener.accept()).await;

        match accept_result {
            Ok(Ok((stream, _addr))) => {
                // Update last activity
                last_activity.store(start_time.elapsed().as_secs(), Ordering::Relaxed);
                active_connections.fetch_add(1, Ordering::Relaxed);

                info!(
                    "New connection accepted (active: {})",
                    active_connections.load(Ordering::Relaxed)
                );

                let service = Arc::clone(&service);
                let hello = hello.clone();
                let active_connections = Arc::clone(&active_connections);
                let last_activity = Arc::clone(&last_activity);

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

                    // Connection done, update counters
                    let remaining = active_connections.fetch_sub(1, Ordering::Relaxed) - 1;
                    last_activity.store(start_time.elapsed().as_secs(), Ordering::Relaxed);
                    info!("Connection closed (active: {})", remaining);
                });
            }
            Ok(Err(e)) => {
                error!("Failed to accept connection: {}", e);
            }
            Err(_) => {
                // Timeout - check if we should exit due to idle
                let current_connections = active_connections.load(Ordering::Relaxed);
                if current_connections == 0 {
                    let last = last_activity.load(Ordering::Relaxed);
                    let now = start_time.elapsed().as_secs();
                    let idle_secs = now.saturating_sub(last);

                    if idle_secs >= DEFAULT_IDLE_TIMEOUT_SECS {
                        info!("No connections for {} seconds, shutting down", idle_secs);
                        // Clean up socket
                        let _ = std::fs::remove_file(&sock_path);
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Run the file watcher, sending events to the channel.
async fn run_file_watcher(
    project_root: &Path,
    config_path: &Path,
    tx: tokio::sync::mpsc::Sender<Vec<PathBuf>>,
) -> Result<()> {
    use notify_debouncer_mini::DebounceEventResult;

    let tx_clone = tx.clone();
    let mut debouncer = new_debouncer(
        Duration::from_millis(200),
        move |events: DebounceEventResult| {
            // Extract paths from the events
            let paths: Vec<PathBuf> = match events {
                Ok(events) => events.into_iter().map(|e| e.path).collect(),
                Err(_) => vec![],
            };
            if !paths.is_empty() {
                let _ = tx_clone.blocking_send(paths);
            }
        },
    )?;

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
