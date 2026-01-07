//! Client for connecting to the tracey daemon.
//!
//! r[impl daemon.lifecycle.auto-start]
//!
//! Provides a roam RPC client that connects to the daemon's Unix socket
//! and calls TraceyDaemon methods.

use eyre::Result;
use std::path::Path;
use std::time::Duration;
use tokio::net::UnixStream;

use roam::__private::facet_postcard;
use roam_stream::{CobsFramed, Hello, Message};

use super::socket_path;
use tracey_proto::*;

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
    /// If the daemon is not running, this will return an error asking the user
    /// to start it manually. In the future, this could auto-spawn the daemon.
    pub async fn connect(project_root: &Path) -> Result<Self> {
        let sock = socket_path(project_root);

        // Try to connect
        let stream = match UnixStream::connect(&sock).await {
            Ok(s) => s,
            Err(_) => {
                // Connection failed - check if there's a stale socket
                if sock.exists() {
                    let _ = std::fs::remove_file(&sock);
                }
                // TODO: Auto-spawn daemon process here
                return Err(eyre::eyre!(
                    "Daemon not running. Start it with: tracey daemon"
                ));
            }
        };

        Self::complete_handshake(stream).await
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

    /// Add an include pattern
    pub async fn add_include(&mut self, req: AddPatternRequest) -> Result<Result<(), ConfigError>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.add_include, &(req,)).await
    }

    /// Add an exclude pattern
    pub async fn add_exclude(&mut self, req: AddPatternRequest) -> Result<Result<(), ConfigError>> {
        let ids = tracey_daemon_method_ids();
        self.call(ids.add_exclude, &(req,)).await
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
}
