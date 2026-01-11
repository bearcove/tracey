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
pub mod engine;
pub mod service;
pub mod watcher;

use eyre::{Result, WrapErr};
use roam_stream::{CobsFramed, ConnectionError, Hello, hello_exchange_acceptor};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::net::UnixListener;
use tracing::{debug, error, info, warn};

use service::TraceyDaemonDispatcher;
use watcher::{WatcherEvent, WatcherManager, WatcherState};

pub use client::{DaemonClient, ReconnectingClient};
pub use engine::Engine;
pub use service::TraceyService;
pub use watcher::WatcherState as DaemonWatcherState;

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

    // r[impl daemon.lifecycle.stale-socket]
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

    // r[impl daemon.state.file-watcher]
    // r[impl daemon.watcher.smart-watch]
    // Set up file watcher with smart directory watching
    let watcher_state = WatcherState::new();

    // Create service with watcher state for health monitoring
    // TraceyService is cheap to clone (holds Arc internally)
    let service = TraceyService::new_with_watcher(Arc::clone(&engine), Arc::clone(&watcher_state));
    let (watcher_tx, mut watcher_rx) = tokio::sync::mpsc::channel::<WatcherEvent>(16);

    // Spawn file watcher in a separate OS thread with auto-restart
    // r[impl daemon.watcher.auto-restart]
    let config_path_for_watcher = config_path.clone();
    let project_root_for_watcher = project_root.clone();
    let watcher_state_for_thread = Arc::clone(&watcher_state);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for watcher");

        rt.block_on(async {
            loop {
                watcher_state_for_thread.mark_active();
                info!(
                    "Starting file watcher for {}",
                    project_root_for_watcher.display()
                );

                match run_smart_watcher(
                    &project_root_for_watcher,
                    &config_path_for_watcher,
                    watcher_tx.clone(),
                    Arc::clone(&watcher_state_for_thread),
                )
                .await
                {
                    Ok(()) => {
                        // Clean shutdown (channel closed)
                        info!("File watcher stopped cleanly");
                        break;
                    }
                    Err(e) => {
                        let error_msg = format!("{}", e);
                        error!("File watcher failed: {}, restarting in 5s", error_msg);
                        watcher_state_for_thread.mark_failed(error_msg);
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });
    });

    // Spawn rebuild task that listens for watcher events
    let engine_for_rebuild = Arc::clone(&engine);
    let project_root_for_rebuild = project_root.clone();
    let config_path_for_rebuild = config_path.clone();
    tokio::spawn(async move {
        // r[impl server.watch.respect-gitignore]
        // Build gitignore matcher for filtering file watcher events
        let mut gitignore = build_gitignore(&project_root_for_rebuild);

        while let Some(event) = watcher_rx.recv().await {
            match event {
                // r[impl daemon.watcher.reconfigure]
                WatcherEvent::Reconfigure => {
                    info!("Config or gitignore changed, reconfiguring watcher");

                    // Rebuild gitignore matcher
                    gitignore = build_gitignore(&project_root_for_rebuild);
                    debug!("Rebuilt gitignore matcher");

                    // Trigger rebuild (watcher reconfiguration happens in the watcher thread)
                    if let Err(e) = engine_for_rebuild.rebuild().await {
                        error!("Rebuild failed: {}", e);
                    }
                }

                WatcherEvent::FilesChanged(changed_files) => {
                    // r[impl server.watch.patterns-from-config]
                    // Collect all include patterns from config
                    let mut include_patterns: Vec<String> = Vec::new();
                    let mut exclude_patterns: Vec<String> = Vec::new();

                    // Get patterns from the raw config file if available
                    if let Ok(config) = crate::load_config(&config_path_for_rebuild) {
                        for spec in &config.specs {
                            for pattern in &spec.include {
                                include_patterns.push(pattern.clone());
                            }
                            for impl_ in &spec.impls {
                                for pattern in &impl_.include {
                                    include_patterns.push(pattern.clone());
                                }
                                // r[impl server.watch.respect-excludes]
                                for pattern in &impl_.exclude {
                                    exclude_patterns.push(pattern.clone());
                                }
                            }
                        }
                    } else {
                        // If config not available, get patterns from engine data
                        let data = engine_for_rebuild.data().await;
                        for spec in &data.config.specs {
                            // Add spec include patterns (markdown files)
                            if let Some(source) = &spec.source {
                                include_patterns.push(source.clone());
                            }
                        }
                    }

                    // Filter changed files
                    let relative_paths: Vec<_> = changed_files
                        .iter()
                        .filter_map(|p| p.strip_prefix(&project_root_for_rebuild).ok())
                        .filter(|p| {
                            // Keep paths that are NOT ignored by gitignore
                            let full_path = project_root_for_rebuild.join(p);
                            !gitignore
                                .matched_path_or_any_parents(&full_path, full_path.is_dir())
                                .is_ignore()
                        })
                        .filter(|p| {
                            let path_str = p.to_string_lossy();

                            // r[impl server.watch.respect-excludes]
                            // Reject paths that match exclude patterns
                            for pattern in &exclude_patterns {
                                if crate::data::glob_match(&path_str, pattern.as_str()) {
                                    return false;
                                }
                            }

                            // r[impl server.watch.patterns-from-config]
                            // Accept paths that match include patterns
                            // If no include patterns, accept all non-excluded files
                            if include_patterns.is_empty() {
                                return true;
                            }
                            for pattern in &include_patterns {
                                if crate::data::glob_match(&path_str, pattern.as_str()) {
                                    return true;
                                }
                            }
                            false
                        })
                        .collect();

                    // Skip rebuild if no relevant files changed
                    if relative_paths.is_empty() {
                        debug!(
                            "Filtered out {} file changes (no relevant files)",
                            changed_files.len()
                        );
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
            }
        }
    });

    // Bind Unix socket
    let listener = UnixListener::bind(&sock_path)
        .wrap_err_with(|| format!("Failed to bind socket at {}", sock_path.display()))?;

    info!("Daemon listening on {}", sock_path.display());

    // Default Hello configuration
    let hello = Hello::V1 {
        max_payload_size: 1024 * 1024,     // 1MB max payload
        initial_channel_credit: 64 * 1024, // 64KB channel credit
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

                let service = service.clone();
                let hello = hello.clone();
                let active_connections = Arc::clone(&active_connections);
                let last_activity = Arc::clone(&last_activity);

                tokio::spawn(async move {
                    // r[impl daemon.roam.framing]
                    // Wrap in COBS framing (roam-stream is generic over any AsyncRead+AsyncWrite)
                    let io = CobsFramed::new(stream);

                    // Create dispatcher (wraps service with generated dispatch + tracing)
                    let dispatcher = TraceyDaemonDispatcher::new(service);

                    // Perform Hello exchange
                    match hello_exchange_acceptor(io, hello).await {
                        Ok(mut conn) => {
                            info!("Hello exchange completed");
                            // Run the message loop with the dispatcher
                            if let Err(e) = conn.run(&dispatcher).await {
                                match e {
                                    ConnectionError::Closed => {
                                        info!("Connection closed cleanly");
                                    }
                                    ConnectionError::ProtocolViolation { rule_id, .. } => {
                                        warn!("Protocol violation: {}", rule_id);
                                    }
                                    ConnectionError::Io(e) => {
                                        error!("IO error: {}", e);
                                    }
                                    ConnectionError::Dispatch(e) => {
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

/// Build a gitignore matcher for the project.
///
/// r[impl server.watch.respect-gitignore]
fn build_gitignore(project_root: &Path) -> ignore::gitignore::Gitignore {
    let mut builder = ignore::gitignore::GitignoreBuilder::new(project_root);

    // Add .gitignore from project root if it exists
    let gitignore_path = project_root.join(".gitignore");
    if gitignore_path.exists() {
        let _ = builder.add(&gitignore_path);
    }

    // Always ignore .git directory
    let _ = builder.add_line(None, ".git/");

    builder.build().unwrap_or_else(|e| {
        warn!("Failed to build gitignore matcher: {}", e);
        ignore::gitignore::Gitignore::empty()
    })
}

/// Run the smart file watcher, sending events to the channel.
///
/// r[impl daemon.watcher.smart-watch]
/// r[impl server.watch.debounce]
/// r[impl server.watch.config-file]
///
/// This watcher only watches directories derived from config patterns,
/// rather than the entire project root. It also handles reconfiguration
/// when config or gitignore changes.
async fn run_smart_watcher(
    project_root: &Path,
    config_path: &Path,
    tx: tokio::sync::mpsc::Sender<WatcherEvent>,
    state: Arc<WatcherState>,
) -> Result<()> {
    use std::sync::Mutex;

    // Load initial config
    let config_path_buf = config_path.to_path_buf();
    let config = crate::load_config(&config_path_buf).unwrap_or_default();

    // Shared state for the event handler
    let config_path_owned = config_path.to_path_buf();
    let gitignore_path = project_root.join(".gitignore");
    let tx_for_handler = tx.clone();
    let state_for_handler = Arc::clone(&state);

    // Track paths that trigger reconfiguration
    let reconfigure_paths: Arc<Mutex<(PathBuf, PathBuf)>> = Arc::new(Mutex::new((
        config_path_owned.clone(),
        gitignore_path.clone(),
    )));
    let reconfigure_paths_for_handler = Arc::clone(&reconfigure_paths);

    // r[impl server.watch.debounce]
    let mut watcher_manager = WatcherManager::new(
        project_root.to_path_buf(),
        config_path_owned,
        Duration::from_millis(200),
        move |events| {
            let paths: Vec<PathBuf> = match events {
                Ok(events) => events.into_iter().map(|e| e.path).collect(),
                Err(e) => {
                    warn!("File watcher error: {:?}", e);
                    vec![]
                }
            };

            if paths.is_empty() {
                return;
            }

            // Record event in state
            state_for_handler.record_event();

            // Check if any path triggers reconfiguration
            let (config_path, gitignore_path) =
                reconfigure_paths_for_handler.lock().unwrap().clone();
            let needs_reconfigure = paths
                .iter()
                .any(|p| p == &config_path || p == &gitignore_path);

            let event = if needs_reconfigure {
                debug!("Config or gitignore changed, sending Reconfigure event");
                WatcherEvent::Reconfigure
            } else {
                debug!("File changes detected: {} files", paths.len());
                WatcherEvent::FilesChanged(paths)
            };

            if tx_for_handler.blocking_send(event).is_err() {
                // Channel closed, watcher should stop
                debug!("Watcher channel closed");
            }
        },
    )?;

    // Configure initial watches based on config
    watcher_manager.reconfigure(&config)?;

    // Update state with watched directories
    state.set_watched_dirs(watcher_manager.watched_dirs());

    info!(
        "Smart file watcher started: {} directories",
        watcher_manager.watched_dirs().len()
    );

    // Keep the watcher alive and handle reconfiguration requests
    // We use a simple loop here; reconfiguration is triggered by the rebuild loop
    // sending a message back (not implemented yet - for now we just reload on any config change)
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;

        // Periodically check if we need to reconfigure (e.g., if directories were created)
        // This is a simple approach; a more sophisticated one would use inotify for directory creation
        if let Ok(new_config) = crate::load_config(&config_path_buf) {
            let old_dirs = watcher_manager.watched_dirs();
            if let Err(e) = watcher_manager.reconfigure(&new_config) {
                warn!("Failed to reconfigure watcher: {}", e);
            } else {
                let new_dirs = watcher_manager.watched_dirs();
                if old_dirs != new_dirs {
                    state.set_watched_dirs(new_dirs);
                    debug!("Updated watched directories");
                }
            }
        }
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
