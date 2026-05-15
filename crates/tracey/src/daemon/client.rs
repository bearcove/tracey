//! Client for connecting to the tracey daemon.
//!
//! Uses roam v7 session builders.

use std::fs::OpenOptions;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use super::{is_pid_alive, local_endpoint, pid_file_path, read_pid_file_at};

// Re-export the generated client from tracey-proto
pub use tracey_proto::TraceyDaemonClient;

/// Daemon client facade.
#[derive(Clone)]
pub struct DaemonClient {
    project_root: PathBuf,
}

/// Create a new daemon client for the given project root.
pub fn new_client(project_root: PathBuf) -> DaemonClient {
    DaemonClient { project_root }
}

impl DaemonClient {
    async fn connect_inner(&self) -> io::Result<roam_stream::LocalLink> {
        let start = Instant::now();
        debug!(
            project_root = %self.project_root.display(),
            "daemon client: connect_inner start"
        );
        let connector = DaemonConnector::new(self.project_root.clone());
        let stream = connector.connect().await?;
        debug!(
            elapsed_ms = start.elapsed().as_millis(),
            "daemon client: local transport connected"
        );
        Ok(stream)
    }

    async fn with_client<T, E, F, Fut>(&self, f: F) -> Result<T, roam::RoamError<E>>
    where
        F: FnOnce(TraceyDaemonClient) -> Fut,
        Fut: Future<Output = Result<T, roam::RoamError<E>>>,
    {
        let start = Instant::now();
        let callsite = std::panic::Location::caller();
        debug!(
            callsite = format_args!("{}:{}", callsite.file(), callsite.line()),
            "daemon client: with_client start"
        );
        let stream = match self.connect_inner().await {
            Ok(stream) => stream,
            Err(e) => {
                warn!(
                    elapsed_ms = start.elapsed().as_millis(),
                    error = %e,
                    callsite = format_args!("{}:{}", callsite.file(), callsite.line()),
                    "daemon client: connect_inner failed"
                );
                return Err(roam::RoamError::Cancelled);
            }
        };
        let (client, _session_handle) = match roam::initiator(stream)
            .establish::<TraceyDaemonClient>(())
            .await
        {
            Ok(parts) => {
                debug!(
                    elapsed_ms = start.elapsed().as_millis(),
                    callsite = format_args!("{}:{}", callsite.file(), callsite.line()),
                    "daemon client: roam session established"
                );
                parts
            }
            Err(e) => {
                warn!(
                    elapsed_ms = start.elapsed().as_millis(),
                    error = %e,
                    callsite = format_args!("{}:{}", callsite.file(), callsite.line()),
                    "daemon client: roam session establish failed"
                );
                return Err(roam::RoamError::Cancelled);
            }
        };
        let result = f(client).await;
        match &result {
            Ok(_) => {
                debug!(
                    elapsed_ms = start.elapsed().as_millis(),
                    callsite = format_args!("{}:{}", callsite.file(), callsite.line()),
                    "daemon client: with_client ok"
                );
            }
            Err(_e) => {
                warn!(
                    elapsed_ms = start.elapsed().as_millis(),
                    callsite = format_args!("{}:{}", callsite.file(), callsite.line()),
                    "daemon client: with_client returned roam error"
                );
            }
        }
        result
    }

    pub async fn status(&self) -> Result<tracey_proto::StatusResponse, roam::RoamError> {
        self.with_client(|c| async move { c.status().await }).await
    }
    pub async fn uncovered(
        &self,
        req: tracey_proto::UncoveredRequest,
    ) -> Result<tracey_proto::UncoveredResponse, roam::RoamError> {
        self.with_client(|c| async move { c.uncovered(req).await })
            .await
    }
    pub async fn untested(
        &self,
        req: tracey_proto::UntestedRequest,
    ) -> Result<tracey_proto::UntestedResponse, roam::RoamError> {
        self.with_client(|c| async move { c.untested(req).await })
            .await
    }
    pub async fn stale(
        &self,
        req: tracey_proto::StaleRequest,
    ) -> Result<tracey_proto::StaleResponse, roam::RoamError> {
        self.with_client(|c| async move { c.stale(req).await })
            .await
    }
    pub async fn unmapped(
        &self,
        req: tracey_proto::UnmappedRequest,
    ) -> Result<tracey_proto::UnmappedResponse, roam::RoamError> {
        self.with_client(|c| async move { c.unmapped(req).await })
            .await
    }
    pub async fn rule(
        &self,
        rule_id: tracey_core::RuleId,
    ) -> Result<Option<tracey_proto::RuleInfo>, roam::RoamError> {
        self.with_client(|c| async move { c.rule(rule_id).await })
            .await
    }
    pub async fn config(&self) -> Result<tracey_api::ApiConfig, roam::RoamError> {
        self.with_client(|c| async move { c.config().await }).await
    }
    pub async fn vfs_open(&self, path: String, content: String) -> Result<(), roam::RoamError> {
        self.with_client(|c| async move { c.vfs_open(path, content).await })
            .await
    }
    pub async fn vfs_change(&self, path: String, content: String) -> Result<(), roam::RoamError> {
        self.with_client(|c| async move { c.vfs_change(path, content).await })
            .await
    }
    pub async fn vfs_close(&self, path: String) -> Result<(), roam::RoamError> {
        self.with_client(|c| async move { c.vfs_close(path).await })
            .await
    }
    pub async fn reload(&self) -> Result<tracey_proto::ReloadResponse, roam::RoamError> {
        self.with_client(|c| async move { c.reload().await }).await
    }
    pub async fn version(&self) -> Result<u64, roam::RoamError> {
        self.with_client(|c| async move { c.version().await }).await
    }
    pub async fn health(&self) -> Result<tracey_proto::HealthResponse, roam::RoamError> {
        self.with_client(|c| async move { c.health().await }).await
    }
    pub async fn shutdown(&self) -> Result<(), roam::RoamError> {
        self.with_client(|c| async move { c.shutdown().await })
            .await
    }
    pub async fn subscribe(
        &self,
        updates: roam::Tx<tracey_proto::DataUpdate>,
    ) -> Result<(), roam::RoamError> {
        self.with_client(|c| async move { c.subscribe(updates).await })
            .await
    }
    pub async fn forward(
        &self,
        spec: String,
        impl_name: String,
    ) -> Result<Option<tracey_api::ApiSpecForward>, roam::RoamError> {
        self.with_client(|c| async move { c.forward(spec, impl_name).await })
            .await
    }
    pub async fn reverse(
        &self,
        spec: String,
        impl_name: String,
    ) -> Result<Option<tracey_api::ApiReverseData>, roam::RoamError> {
        self.with_client(|c| async move { c.reverse(spec, impl_name).await })
            .await
    }
    pub async fn file(
        &self,
        req: tracey_proto::FileRequest,
    ) -> Result<Option<tracey_api::ApiFileData>, roam::RoamError> {
        self.with_client(|c| async move { c.file(req).await }).await
    }
    pub async fn spec_content(
        &self,
        spec: String,
        impl_name: String,
    ) -> Result<Option<tracey_api::ApiSpecData>, roam::RoamError> {
        self.with_client(|c| async move { c.spec_content(spec, impl_name).await })
            .await
    }
    pub async fn search(
        &self,
        query: String,
        limit: u32,
    ) -> Result<Vec<tracey_proto::SearchResult>, roam::RoamError> {
        self.with_client(|c| async move { c.search(query, limit).await })
            .await
    }
    pub async fn update_file_range(
        &self,
        req: tracey_proto::UpdateFileRangeRequest,
    ) -> Result<(), roam::RoamError<tracey_proto::UpdateError>> {
        self.with_client(|c| async move { c.update_file_range(req).await })
            .await
    }
    pub async fn is_test_file(&self, path: String) -> Result<bool, roam::RoamError> {
        self.with_client(|c| async move { c.is_test_file(path).await })
            .await
    }
    pub async fn lsp_hover(
        &self,
        req: tracey_proto::LspPositionRequest,
    ) -> Result<Option<tracey_proto::HoverInfo>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_hover(req).await })
            .await
    }
    pub async fn lsp_definition(
        &self,
        req: tracey_proto::LspPositionRequest,
    ) -> Result<Vec<tracey_proto::LspLocation>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_definition(req).await })
            .await
    }
    pub async fn lsp_implementation(
        &self,
        req: tracey_proto::LspPositionRequest,
    ) -> Result<Vec<tracey_proto::LspLocation>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_implementation(req).await })
            .await
    }
    pub async fn lsp_references(
        &self,
        req: tracey_proto::LspReferencesRequest,
    ) -> Result<Vec<tracey_proto::LspLocation>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_references(req).await })
            .await
    }
    pub async fn lsp_completions(
        &self,
        req: tracey_proto::LspPositionRequest,
    ) -> Result<Vec<tracey_proto::LspCompletionItem>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_completions(req).await })
            .await
    }
    pub async fn lsp_workspace_diagnostics(
        &self,
    ) -> Result<Vec<tracey_proto::LspFileDiagnostics>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_workspace_diagnostics().await })
            .await
    }
    pub async fn lsp_document_symbols(
        &self,
        req: tracey_proto::LspDocumentRequest,
    ) -> Result<Vec<tracey_proto::LspSymbol>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_document_symbols(req).await })
            .await
    }
    pub async fn lsp_workspace_symbols(
        &self,
        query: String,
    ) -> Result<Vec<tracey_proto::LspSymbol>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_workspace_symbols(query).await })
            .await
    }
    pub async fn lsp_semantic_tokens(
        &self,
        req: tracey_proto::LspDocumentRequest,
    ) -> Result<Vec<tracey_proto::LspSemanticToken>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_semantic_tokens(req).await })
            .await
    }
    pub async fn lsp_code_lens(
        &self,
        req: tracey_proto::LspDocumentRequest,
    ) -> Result<Vec<tracey_proto::LspCodeLens>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_code_lens(req).await })
            .await
    }
    pub async fn lsp_inlay_hints(
        &self,
        req: tracey_proto::InlayHintsRequest,
    ) -> Result<Vec<tracey_proto::LspInlayHint>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_inlay_hints(req).await })
            .await
    }
    pub async fn lsp_prepare_rename(
        &self,
        req: tracey_proto::LspPositionRequest,
    ) -> Result<Option<tracey_proto::PrepareRenameResult>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_prepare_rename(req).await })
            .await
    }
    pub async fn lsp_rename(
        &self,
        req: tracey_proto::LspRenameRequest,
    ) -> Result<Vec<tracey_proto::LspTextEdit>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_rename(req).await })
            .await
    }
    pub async fn lsp_code_actions(
        &self,
        req: tracey_proto::LspPositionRequest,
    ) -> Result<Vec<tracey_proto::LspCodeAction>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_code_actions(req).await })
            .await
    }
    pub async fn lsp_document_highlight(
        &self,
        req: tracey_proto::LspPositionRequest,
    ) -> Result<Vec<tracey_proto::LspLocation>, roam::RoamError> {
        self.with_client(|c| async move { c.lsp_document_highlight(req).await })
            .await
    }
    pub async fn validate(
        &self,
        req: tracey_proto::ValidateRequest,
    ) -> Result<tracey_api::ValidationResult, roam::RoamError> {
        self.with_client(|c| async move { c.validate(req).await })
            .await
    }
    pub async fn config_add_exclude(
        &self,
        req: tracey_proto::ConfigPatternRequest,
    ) -> Result<(), roam::RoamError<String>> {
        self.with_client(|c| async move { c.config_add_exclude(req).await })
            .await
    }
    pub async fn config_add_include(
        &self,
        req: tracey_proto::ConfigPatternRequest,
    ) -> Result<(), roam::RoamError<String>> {
        self.with_client(|c| async move { c.config_add_include(req).await })
            .await
    }
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
    async fn wait_for_existing_daemon(
        &self,
        pid: u32,
        endpoint: &str,
        timeout: Duration,
    ) -> io::Result<Option<roam_stream::LocalLink>> {
        let start = Instant::now();
        let mut last_error: Option<String> = None;

        while start.elapsed() < timeout {
            if !is_pid_alive(pid) {
                debug!(
                    "Daemon PID {} exited while waiting for socket {:?}",
                    pid, endpoint
                );
                return Ok(None);
            }

            match roam_stream::LocalLink::connect(endpoint).await {
                Ok(stream) => return Ok(Some(stream)),
                Err(e) => {
                    last_error = Some(e.to_string());
                }
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        warn!(
            "Daemon PID {} remained alive but socket {:?} stayed unavailable for {}s (last error: {})",
            pid,
            endpoint,
            timeout.as_secs(),
            last_error.unwrap_or_else(|| "unknown".to_string())
        );
        Ok(None)
    }

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
        super::state_dir(&self.project_root).join("daemon-start.lock")
    }

    fn acquire_startup_lock(&self, timeout: Duration) -> io::Result<StartupLock> {
        super::ensure_state_dir(&self.project_root).map_err(io::Error::other)?;

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
                    debug!("Acquired daemon startup lock at {}", lock_path.display());
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
                        warn!(
                            "Removing stale daemon startup lock at {}",
                            lock_path.display()
                        );
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
    async fn wait_and_connect(&self) -> io::Result<roam_stream::LocalLink> {
        let endpoint = local_endpoint(&self.project_root);
        let start = Instant::now();
        let timeout = Duration::from_secs(5);
        let mut last_print_secs = 0u64;
        let mut last_connect_error: Option<String> = None;

        loop {
            let elapsed = start.elapsed();

            if elapsed > timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "Daemon failed to start within {}s (last connect error: {}). \
                         Check logs at {}/daemon.log",
                        timeout.as_secs(),
                        last_connect_error.as_deref().unwrap_or("unavailable"),
                        super::state_dir(&self.project_root).display()
                    ),
                ));
            }

            match roam_stream::LocalLink::connect(&endpoint).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    last_connect_error = Some(e.to_string());
                }
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

fn read_pid_file(project_root: &Path) -> Option<(u32, u32)> {
    read_pid_file_at(&pid_file_path(project_root))
}

fn pid_file_age(project_root: &Path) -> Option<Duration> {
    let path = pid_file_path(project_root);
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    modified.elapsed().ok()
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

impl DaemonConnector {
    pub async fn connect(&self) -> io::Result<roam_stream::LocalLink> {
        let endpoint = local_endpoint(&self.project_root);
        debug!(
            "DaemonConnector::connect project_root={} endpoint={:?}",
            self.project_root.display(),
            endpoint
        );

        match read_pid_file(&self.project_root) {
            Some((pid, version)) => {
                let alive = is_pid_alive(pid);
                let version_ok = version == tracey_proto::PROTOCOL_VERSION;
                debug!(
                    "PID file found pid={} version={} alive={} version_ok={}",
                    pid, version, alive, version_ok
                );

                if alive && version_ok {
                    // Happy path: daemon should be running.
                    match roam_stream::LocalLink::connect(&endpoint).await {
                        Ok(stream) => return Ok(stream),
                        Err(e) => {
                            let age = pid_file_age(&self.project_root);
                            let startup_grace = Duration::from_secs(20);
                            if let Some(age) = age
                                && age < startup_grace
                            {
                                let wait_for = startup_grace - age;
                                debug!(
                                    "Daemon PID {} alive but socket connect failed ({}); PID file age {:?}, waiting {:?} for startup",
                                    pid, e, age, wait_for
                                );
                                if let Some(stream) = self
                                    .wait_for_existing_daemon(pid, &endpoint, wait_for)
                                    .await?
                                {
                                    return Ok(stream);
                                }
                            }

                            warn!(
                                "Daemon PID {} alive but socket unavailable (connect error: {}); removing endpoint+pid and restarting",
                                pid, e
                            );
                        }
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
                debug!("No PID file found for {}", self.project_root.display());
                // No PID file — remove stale socket if present.
                // r[impl daemon.lifecycle.stale-socket]
                if roam_local::endpoint_exists(&endpoint) {
                    warn!(
                        "No PID file but endpoint exists at {:?}; removing stale endpoint",
                        endpoint
                    );
                    let _ = roam_local::remove_endpoint(&endpoint);
                }
            }
        }

        // Daemon is not running. Serialize startup across concurrent connectors.
        debug!("Acquiring startup lock for {}", self.project_root.display());
        let _startup_lock = self.acquire_startup_lock(Duration::from_secs(5))?;

        // Re-check: another process may have started the daemon while we waited for the lock.
        if let Some((pid, version)) = read_pid_file(&self.project_root)
            && is_pid_alive(pid)
            && version == tracey_proto::PROTOCOL_VERSION
            && let Ok(stream) = roam_stream::LocalLink::connect(&endpoint).await
        {
            debug!(
                "Daemon became available while waiting for startup lock (pid={})",
                pid
            );
            return Ok(stream);
        }

        debug!(
            "Daemon still unavailable after startup lock; spawning process for {}",
            self.project_root.display()
        );
        self.spawn_daemon()?;
        self.wait_and_connect().await
    }
}
