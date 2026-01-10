//! Client for connecting to the tracey daemon.
//!
//! Provides a roam RPC client that connects to the daemon's Unix socket
//! and calls TraceyDaemon methods.

use eyre::{Result, WrapErr};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::net::UnixStream;
use tracing::{info, warn};

use roam::__private::facet_postcard;
use roam_stream::{CobsFramed, Hello, Message};

use super::socket_path;
use tracey_proto::*;

// ============================================================================
// Reconnecting Client Wrapper
// ============================================================================

/// A daemon client that automatically reconnects on connection failures.
///
/// r[impl daemon.bridge.reconnect]
///
/// Bridges (MCP, HTTP, LSP) use this wrapper to ensure they can survive
/// daemon restarts without requiring manual intervention.
pub struct ReconnectingClient {
    project_root: PathBuf,
    client: Option<DaemonClient>,
}

impl ReconnectingClient {
    /// Create a new reconnecting client for the given project root.
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            client: None,
        }
    }

    /// Get a connected client, reconnecting if necessary.
    ///
    /// This will:
    /// 1. Return the existing connection if valid
    /// 2. Reconnect (and auto-start daemon if needed) on first call or after disconnection
    pub async fn get_client(&mut self) -> Result<&mut DaemonClient> {
        if self.client.is_none() {
            info!("Connecting to daemon...");
            self.client = Some(DaemonClient::connect(&self.project_root).await?);
        }
        Ok(self.client.as_mut().unwrap())
    }

    /// Check if an error looks like a connection failure.
    pub fn is_connection_error(e: &eyre::Report) -> bool {
        let err_str = e.to_string();
        err_str.contains("closed")
            || err_str.contains("Goodbye")
            || err_str.contains("Broken pipe")
            || err_str.contains("Connection reset")
            || err_str.contains("os error 32") // EPIPE
            || err_str.contains("os error 104") // ECONNRESET
    }

    /// Mark the connection as broken so next call will reconnect.
    pub fn mark_disconnected(&mut self) {
        if self.client.is_some() {
            warn!("Daemon connection lost, will reconnect on next request");
            self.client = None;
        }
    }
}

// ============================================================================
// Low-level Client
// ============================================================================

/// Client for the tracey daemon.
///
/// Connects to the daemon's Unix socket and provides typed methods
/// for all TraceyDaemon RPC calls.
pub struct DaemonClient {
    io: CobsFramed<UnixStream>,
    request_id: u64,
}

impl DaemonClient {
    /// Connect to the daemon for the given workspace.
    ///
    /// r[impl daemon.lifecycle.auto-start]
    ///
    /// If the daemon is not running, this will automatically spawn it
    /// and wait for it to be ready before connecting.
    pub async fn connect(project_root: &Path) -> Result<Self> {
        let sock = socket_path(project_root);

        // Try to connect
        match UnixStream::connect(&sock).await {
            Ok(stream) => Self::complete_handshake(stream).await,
            Err(_) => {
                // r[impl daemon.lifecycle.stale-socket]
                // Connection failed - check if there's a stale socket
                if sock.exists() {
                    let _ = std::fs::remove_file(&sock);
                }

                // Auto-start the daemon
                Self::spawn_daemon(project_root).await?;

                // Wait for daemon to be ready and connect
                Self::wait_and_connect(project_root, &sock).await
            }
        }
    }

    /// Spawn the daemon process in the background.
    async fn spawn_daemon(project_root: &Path) -> Result<()> {
        // Find the tracey executable
        let exe = std::env::current_exe().wrap_err("Failed to get current executable path")?;

        // Determine config path
        let config_path = project_root.join(".config/tracey/config.kdl");

        info!("Auto-starting daemon for {}", project_root.display());

        // Spawn daemon process detached
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("daemon")
            .arg(project_root)
            .arg("--config")
            .arg(&config_path)
            // Detach from current process group
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        // On Unix, use setsid to create a new session
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // Create new process group so daemon survives parent exit
            cmd.process_group(0);
        }

        cmd.spawn().wrap_err("Failed to spawn daemon process")?;

        Ok(())
    }

    /// Wait for the daemon socket to appear and connect.
    async fn wait_and_connect(project_root: &Path, sock: &PathBuf) -> Result<Self> {
        // Wait up to 10 seconds for daemon to start
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(10);

        loop {
            // Try to connect
            if let Ok(stream) = UnixStream::connect(sock).await {
                info!("Connected to daemon");
                return Self::complete_handshake(stream).await;
            }

            // Check timeout
            if start.elapsed() > timeout {
                return Err(eyre::eyre!(
                    "Daemon failed to start within {} seconds. Check logs at {}/.tracey/daemon.log",
                    timeout.as_secs(),
                    project_root.display()
                ));
            }

            // Wait a bit before retrying
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Complete the handshake on an already-connected stream.
    async fn complete_handshake(stream: UnixStream) -> Result<Self> {
        let mut io = CobsFramed::new(stream);

        // Send Hello
        let our_hello = Hello::V1 {
            max_payload_size: 1024 * 1024,
            initial_stream_credit: 64 * 1024,
        };
        io.send(&Message::Hello(our_hello)).await?;

        // Wait for peer Hello
        match io.recv_timeout(Duration::from_secs(5)).await? {
            Some(Message::Hello(_)) => {}
            Some(_) => {
                return Err(eyre::eyre!("Expected Hello from daemon"));
            }
            None => {
                return Err(eyre::eyre!("Daemon closed connection during handshake"));
            }
        }

        Ok(Self { io, request_id: 0 })
    }

    /// Send a request and wait for response.
    async fn call<Req: for<'a> facet::Facet<'a>, Resp: for<'a> facet::Facet<'a>>(
        &mut self,
        method_id: u64,
        request: &Req,
    ) -> Result<Resp> {
        self.request_id += 1;
        let request_id = self.request_id;

        let payload = facet_postcard::to_vec(request)
            .map_err(|e| eyre::eyre!("Failed to encode request: {:?}", e))?;

        self.io
            .send(&Message::Request {
                request_id,
                method_id,
                metadata: vec![],
                payload,
            })
            .await?;

        // Wait for response
        loop {
            match self.io.recv_timeout(Duration::from_secs(30)).await? {
                Some(Message::Response {
                    request_id: resp_id,
                    payload,
                    ..
                }) if resp_id == request_id => {
                    // Decode response
                    let result: roam::session::CallResult<Resp, roam::session::Never> =
                        facet_postcard::from_slice(&payload)
                            .map_err(|e| eyre::eyre!("Failed to decode response: {:?}", e))?;

                    return result.map_err(|e| eyre::eyre!("RPC error: {:?}", e));
                }
                Some(Message::Goodbye { reason }) => {
                    return Err(eyre::eyre!("Daemon sent Goodbye: {}", reason));
                }
                Some(_) => {
                    // Ignore other messages, keep waiting
                    continue;
                }
                None => {
                    return Err(eyre::eyre!("Connection closed while waiting for response"));
                }
            }
        }
    }

    // === RPC Methods ===

    /// Get coverage status for all specs/impls
    pub async fn status(&mut self) -> Result<StatusResponse> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.status, &()).await
    }

    /// Get uncovered rules
    pub async fn uncovered(&mut self, req: UncoveredRequest) -> Result<UncoveredResponse> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.uncovered, &(req,)).await
    }

    /// Get untested rules
    pub async fn untested(&mut self, req: UntestedRequest) -> Result<UntestedResponse> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.untested, &(req,)).await
    }

    /// Get unmapped code
    pub async fn unmapped(&mut self, req: UnmappedRequest) -> Result<UnmappedResponse> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.unmapped, &(req,)).await
    }

    /// Get details for a specific rule
    pub async fn rule(&mut self, rule_id: String) -> Result<Option<RuleInfo>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.rule, &(rule_id,)).await
    }

    /// Get current configuration
    pub async fn config(&mut self) -> Result<ApiConfig> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.config, &()).await
    }

    /// VFS: file opened
    pub async fn vfs_open(&mut self, path: String, content: String) -> Result<()> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.vfs_open, &(path, content)).await
    }

    /// VFS: file changed
    pub async fn vfs_change(&mut self, path: String, content: String) -> Result<()> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.vfs_change, &(path, content)).await
    }

    /// VFS: file closed
    pub async fn vfs_close(&mut self, path: String) -> Result<()> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.vfs_close, &(path,)).await
    }

    /// Force a rebuild
    pub async fn reload(&mut self) -> Result<ReloadResponse> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.reload, &()).await
    }

    /// Get current version
    pub async fn version(&mut self) -> Result<u64> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.version, &()).await
    }

    /// Get daemon health status
    ///
    /// r[impl daemon.health]
    pub async fn health(&mut self) -> Result<HealthResponse> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.health, &()).await
    }

    /// Get forward traceability data
    pub async fn forward(
        &mut self,
        spec: String,
        impl_name: String,
    ) -> Result<Option<ApiSpecForward>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.forward, &(spec, impl_name)).await
    }

    /// Get reverse traceability data
    pub async fn reverse(
        &mut self,
        spec: String,
        impl_name: String,
    ) -> Result<Option<ApiReverseData>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.reverse, &(spec, impl_name)).await
    }

    /// Get rendered spec content
    pub async fn spec_content(
        &mut self,
        spec: String,
        impl_name: String,
    ) -> Result<Option<ApiSpecData>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.spec_content, &(spec, impl_name)).await
    }

    /// Get file content with syntax highlighting
    pub async fn file(&mut self, req: FileRequest) -> Result<Option<ApiFileData>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.file, &(req,)).await
    }

    /// Search rules and files
    pub async fn search(&mut self, query: String, limit: usize) -> Result<Vec<SearchResult>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.search, &(query, limit)).await
    }

    /// Check if a path is a test file
    #[allow(dead_code)]
    pub async fn is_test_file(&mut self, path: String) -> Result<bool> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.is_test_file, &(path,)).await
    }

    /// Validate the spec and implementation for errors
    pub async fn validate(&mut self, req: ValidateRequest) -> Result<ValidationResult> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.validate, &(req,)).await
    }

    // === LSP Support Methods ===

    /// Get hover info for a position in a file
    pub async fn lsp_hover(&mut self, req: LspPositionRequest) -> Result<Option<HoverInfo>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_hover, &(req,)).await
    }

    /// Get definition location for a reference at a position
    pub async fn lsp_definition(&mut self, req: LspPositionRequest) -> Result<Vec<LspLocation>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_definition, &(req,)).await
    }

    /// Get implementation locations for a reference at a position
    pub async fn lsp_implementation(
        &mut self,
        req: LspPositionRequest,
    ) -> Result<Vec<LspLocation>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_implementation, &(req,)).await
    }

    /// Get all references to a requirement
    pub async fn lsp_references(&mut self, req: LspReferencesRequest) -> Result<Vec<LspLocation>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_references, &(req,)).await
    }

    /// Get completions for a position
    pub async fn lsp_completions(
        &mut self,
        req: LspPositionRequest,
    ) -> Result<Vec<LspCompletionItem>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_completions, &(req,)).await
    }

    /// Get diagnostics for a file
    pub async fn lsp_diagnostics(&mut self, req: LspDocumentRequest) -> Result<Vec<LspDiagnostic>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_diagnostics, &(req,)).await
    }

    /// Get diagnostics for all files in the workspace
    pub async fn lsp_workspace_diagnostics(&mut self) -> Result<Vec<LspFileDiagnostics>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_workspace_diagnostics, &()).await
    }

    /// Get document symbols (requirement references) in a file
    pub async fn lsp_document_symbols(
        &mut self,
        req: LspDocumentRequest,
    ) -> Result<Vec<LspSymbol>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_document_symbols, &(req,)).await
    }

    /// Search workspace for requirement IDs
    pub async fn lsp_workspace_symbols(&mut self, query: String) -> Result<Vec<LspSymbol>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_workspace_symbols, &(query,)).await
    }

    /// Get semantic tokens for syntax highlighting
    pub async fn lsp_semantic_tokens(
        &mut self,
        req: LspDocumentRequest,
    ) -> Result<Vec<LspSemanticToken>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_semantic_tokens, &(req,)).await
    }

    /// Get code lens items
    pub async fn lsp_code_lens(&mut self, req: LspDocumentRequest) -> Result<Vec<LspCodeLens>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_code_lens, &(req,)).await
    }

    /// Get inlay hints for a range
    pub async fn lsp_inlay_hints(&mut self, req: InlayHintsRequest) -> Result<Vec<LspInlayHint>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_inlay_hints, &(req,)).await
    }

    /// Prepare rename (check if renaming is valid)
    pub async fn lsp_prepare_rename(
        &mut self,
        req: LspPositionRequest,
    ) -> Result<Option<PrepareRenameResult>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_prepare_rename, &(req,)).await
    }

    /// Execute rename
    pub async fn lsp_rename(&mut self, req: LspRenameRequest) -> Result<Vec<LspTextEdit>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_rename, &(req,)).await
    }

    /// Get code actions for a position
    pub async fn lsp_code_actions(
        &mut self,
        req: LspPositionRequest,
    ) -> Result<Vec<LspCodeAction>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_code_actions, &(req,)).await
    }

    /// Get document highlight ranges (same requirement references)
    pub async fn lsp_document_highlight(
        &mut self,
        req: LspPositionRequest,
    ) -> Result<Vec<LspLocation>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.lsp_document_highlight, &(req,)).await
    }

    // === Config Modification Methods (for MCP) ===

    /// Add an exclude pattern to an implementation
    pub async fn config_add_exclude(
        &mut self,
        req: ConfigPatternRequest,
    ) -> Result<Result<(), String>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.config_add_exclude, &(req,)).await
    }

    /// Add an include pattern to an implementation
    pub async fn config_add_include(
        &mut self,
        req: ConfigPatternRequest,
    ) -> Result<Result<(), String>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.config_add_include, &(req,)).await
    }
}
