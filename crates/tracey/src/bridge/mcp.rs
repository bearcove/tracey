//! MCP bridge for the tracey daemon.
//!
//! This module provides an MCP server that translates MCP tool calls
//! to daemon RPC calls. It connects to the daemon as a client and
//! forwards requests.
//!
//! r[impl daemon.bridge.mcp]

#![allow(clippy::enum_variant_names)]

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use eyre::Result;
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::mcp_server::{McpServerOptions, ServerHandler, server_runtime};
use rust_mcp_sdk::schema::{
    CallToolError, CallToolRequestParams, CallToolResult, Implementation, InitializeResult,
    LATEST_PROTOCOL_VERSION, ListToolsResult, NotificationParams, PaginatedRequestParams, Root,
    RpcError, ServerCapabilities, ServerCapabilitiesTools,
};
use rust_mcp_sdk::{McpServer, StdioTransport, ToMcpServerHandler, TransportOptions, tool_box};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use url::Url;

use crate::bridge::query;

// ============================================================================
// Tool Definitions (same as mcp.rs)
// ============================================================================

/// Get coverage status for all specs/implementations
#[mcp_tool(
    name = "tracey_status",
    description = "Get coverage overview for all specs and implementations. Shows current coverage percentages and what changed since last rebuild."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct StatusTool {}

/// Get rules without implementation references
#[mcp_tool(
    name = "tracey_uncovered",
    description = "List rules that have no implementation references ([impl ...] comments). Optionally filter by spec/impl or rule ID prefix."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UncoveredTool {
    #[serde(default)]
    pub spec_impl: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
}

/// Get rules without verification references
#[mcp_tool(
    name = "tracey_untested",
    description = "List rules that have implementation but no verification references ([verify ...] comments). These rules are implemented but not tested."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UntestedTool {
    #[serde(default)]
    pub spec_impl: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
}

/// List stale references (code pointing to older rule versions)
#[mcp_tool(
    name = "tracey_stale",
    description = "List references that point to older rule versions. These need code updates to match the current spec, then annotation bumps."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct StaleTool {
    #[serde(default)]
    pub spec_impl: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
}

/// Get code units without rule references
#[mcp_tool(
    name = "tracey_unmapped",
    description = "Show source tree with coverage percentages. Code units (functions, structs, etc.) without any rule references are 'unmapped'. Pass a path to zoom into a specific directory or file."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UnmappedTool {
    #[serde(default)]
    pub spec_impl: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

/// Get details about a specific rule
#[mcp_tool(
    name = "tracey_rule",
    description = "Get full details about a specific rule: its text, where it's defined, and all implementation/verification references."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RuleTool {
    pub rule_id: String,
}

/// Display current configuration
#[mcp_tool(
    name = "tracey_config",
    description = "Display the current configuration for all specs and implementations, including include/exclude patterns."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConfigTool {}

/// Force a rebuild
#[mcp_tool(
    name = "tracey_reload",
    description = "Reload the configuration file and rebuild all data. Use this after creating or modifying the config file."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReloadTool {}

/// r[impl mcp.validation.check]
///
/// Validate the spec and implementation for errors
#[mcp_tool(
    name = "tracey_validate",
    description = "Validate the spec and implementation for errors such as circular dependencies, naming violations, and unknown references."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ValidateTool {
    /// Spec/impl to validate (e.g., "my-spec/rust"). Optional if only one exists.
    #[serde(default)]
    pub spec_impl: Option<String>,
}

/// Add an exclude pattern to filter out files from scanning
///
/// r[impl mcp.config.exclude]
#[mcp_tool(
    name = "tracey_config_exclude",
    description = "Add an exclude pattern to filter out files from scanning for a specific implementation."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConfigExcludeTool {
    /// Spec/impl to modify (e.g., "my-spec/rust"). Optional if only one exists.
    #[serde(default)]
    pub spec_impl: Option<String>,
    /// Glob pattern to exclude (e.g., "**/*_test.rs")
    pub pattern: String,
}

/// Add an include pattern to expand the set of scanned files
///
/// r[impl mcp.config.include]
#[mcp_tool(
    name = "tracey_config_include",
    description = "Add an include pattern to expand the set of scanned files for a specific implementation."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConfigIncludeTool {
    /// Spec/impl to modify (e.g., "my-spec/rust"). Optional if only one exists.
    #[serde(default)]
    pub spec_impl: Option<String>,
    /// Glob pattern to include (e.g., "src/**/*.rs")
    pub pattern: String,
}

// Create toolbox
tool_box!(
    TraceyTools,
    [
        StatusTool,
        UncoveredTool,
        UntestedTool,
        StaleTool,
        UnmappedTool,
        RuleTool,
        ConfigTool,
        ReloadTool,
        ValidateTool,
        ConfigExcludeTool,
        ConfigIncludeTool
    ]
);

// ============================================================================
// MCP Handler
// ============================================================================

#[derive(Debug, Default, Clone)]
struct RootRefreshState {
    client_supports_root_list: Option<bool>,
    last_client_roots: Vec<String>,
    last_selected_root: Option<PathBuf>,
    last_refresh_error: Option<String>,
}

/// MCP handler that delegates to the daemon.
struct TraceyHandler {
    startup_cwd: Option<PathBuf>,
    startup_project_root: PathBuf,
    config_path: PathBuf,
    active_project_root: Arc<RwLock<PathBuf>>,
    root_refresh_state: Arc<RwLock<RootRefreshState>>,
}

impl TraceyHandler {
    pub fn new(project_root: PathBuf, config_path: PathBuf) -> Self {
        Self {
            startup_cwd: std::env::current_dir().ok(),
            startup_project_root: project_root.clone(),
            config_path,
            active_project_root: Arc::new(RwLock::new(project_root)),
            root_refresh_state: Arc::new(RwLock::new(RootRefreshState::default())),
        }
    }

    async fn current_client(&self) -> query::QueryClient {
        let root = self.active_project_root.read().await.clone();
        query::QueryClient::new(root, query::Caller::Mcp)
    }

    async fn refresh_project_root_from_client_roots(&self, runtime: Arc<dyn McpServer>) {
        let supports_roots = runtime.client_supports_root_list();
        {
            let mut state = self.root_refresh_state.write().await;
            state.client_supports_root_list = supports_roots;
        }

        if supports_roots != Some(true) {
            return;
        }

        match runtime.request_root_list(None).await {
            Ok(result) => {
                let roots = result
                    .roots
                    .iter()
                    .map(format_root_entry)
                    .collect::<Vec<_>>();
                let selected = select_project_root_from_roots(&result.roots);

                let mut state = self.root_refresh_state.write().await;
                state.last_client_roots = roots;

                if let Some(project_root) = selected {
                    *self.active_project_root.write().await = project_root.clone();
                    state.last_selected_root = Some(project_root);
                    state.last_refresh_error = None;
                } else {
                    state.last_refresh_error =
                        Some("roots/list returned no usable file:// root".to_string());
                }
            }
            Err(e) => {
                let mut state = self.root_refresh_state.write().await;
                state.last_refresh_error = Some(format!("roots/list request failed: {e}"));
            }
        }
    }

    async fn mcp_routing_diagnostics(&self) -> String {
        let active_root = self.active_project_root.read().await.clone();
        let state = self.root_refresh_state.read().await.clone();

        let startup_cwd = self
            .startup_cwd
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(unknown)".to_string());

        let supports_roots = match state.client_supports_root_list {
            Some(true) => "yes",
            Some(false) => "no",
            None => "unknown",
        };

        let roots_summary = if state.last_client_roots.is_empty() {
            "(none)".to_string()
        } else {
            let mut entries = state.last_client_roots;
            if entries.len() > 8 {
                entries.truncate(8);
                entries.push("...".to_string());
            }
            entries.join(" | ")
        };

        let selected_root = state
            .last_selected_root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".to_string());
        let refresh_error = state
            .last_refresh_error
            .as_deref()
            .unwrap_or("(none)")
            .to_string();

        format!(
            "\n---\nMCP routing diagnostics:\n- pid: {}\n- startup cwd: {}\n- startup project root: {}\n- startup config path: {}\n- active project root: {}\n- client supports roots/list: {}\n- last client roots: {}\n- last selected root: {}\n- last root refresh error: {}\n",
            std::process::id(),
            startup_cwd,
            self.startup_project_root.display(),
            self.config_path.display(),
            active_root.display(),
            supports_roots,
            roots_summary,
            selected_root,
            refresh_error
        )
    }
}

#[async_trait]
impl ServerHandler for TraceyHandler {
    async fn handle_initialized_notification(
        &self,
        _params: Option<NotificationParams>,
        runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<(), RpcError> {
        self.refresh_project_root_from_client_roots(runtime).await;
        Ok(())
    }

    async fn handle_roots_list_changed_notification(
        &self,
        _params: Option<NotificationParams>,
        runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<(), RpcError> {
        self.refresh_project_root_from_client_roots(runtime).await;
        Ok(())
    }

    async fn handle_list_tools_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: TraceyTools::tools(),
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        self.refresh_project_root_from_client_roots(runtime).await;
        let client = self.current_client().await;
        let tool_name = params.name.clone();
        let args = params.arguments.unwrap_or_default();

        let mut response = match tool_name.as_str() {
            "tracey_status" => client.status().await,
            "tracey_uncovered" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let prefix = args.get("prefix").and_then(|v| v.as_str());
                client.uncovered(spec_impl, prefix).await
            }
            "tracey_untested" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let prefix = args.get("prefix").and_then(|v| v.as_str());
                client.untested(spec_impl, prefix).await
            }
            "tracey_stale" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let prefix = args.get("prefix").and_then(|v| v.as_str());
                client.stale(spec_impl, prefix).await
            }
            "tracey_unmapped" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let path = args.get("path").and_then(|v| v.as_str());
                client.unmapped(spec_impl, path).await
            }
            "tracey_rule" => {
                let rule_id = args.get("rule_id").and_then(|v| v.as_str());
                match rule_id {
                    Some(id) => client.rule(id).await,
                    None => {
                        client
                            .with_config_banner("Error: rule_id is required".to_string())
                            .await
                    }
                }
            }
            "tracey_config" => client.config().await,
            "tracey_reload" => client.reload().await,
            "tracey_validate" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                client.validate(spec_impl).await
            }
            "tracey_config_exclude" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let pattern = args.get("pattern").and_then(|v| v.as_str());
                match pattern {
                    Some(p) => client.config_exclude(spec_impl, p).await,
                    None => {
                        client
                            .with_config_banner("Error: pattern is required".to_string())
                            .await
                    }
                }
            }
            "tracey_config_include" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let pattern = args.get("pattern").and_then(|v| v.as_str());
                match pattern {
                    Some(p) => client.config_include(spec_impl, p).await,
                    None => {
                        client
                            .with_config_banner("Error: pattern is required".to_string())
                            .await
                    }
                }
            }
            other => {
                client
                    .with_config_banner(format!("Unknown tool: {}", other))
                    .await
            }
        };

        if matches!(tool_name.as_str(), "tracey_status" | "tracey_config") {
            response.push_str(&self.mcp_routing_diagnostics().await);
        }

        Ok(CallToolResult::text_content(vec![response.into()]))
    }
}

fn root_uri_to_project_root(uri: &str) -> Option<PathBuf> {
    let url = Url::parse(uri).ok()?;
    let path = url.to_file_path().ok()?;
    Some(crate::find_project_root_from(&path))
}

fn select_project_root_from_roots(roots: &[Root]) -> Option<PathBuf> {
    roots
        .iter()
        .find_map(|root| root_uri_to_project_root(root.uri.as_str()))
}

fn format_root_entry(root: &Root) -> String {
    match root.name.as_deref() {
        Some(name) if !name.is_empty() => format!("{name}: {}", root.uri),
        _ => root.uri.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_project_root_from_roots_skips_non_file_uris() {
        let roots = vec![
            Root {
                name: Some("bad".to_string()),
                uri: "https://example.com/repo".to_string(),
                meta: None,
            },
            Root {
                name: Some("good".to_string()),
                uri: "file:///tmp/workspace".to_string(),
                meta: None,
            },
        ];

        let selected = select_project_root_from_roots(&roots);
        assert_eq!(selected, Some(PathBuf::from("/tmp/workspace")));
    }

    #[test]
    fn select_project_root_from_roots_returns_none_when_unusable() {
        let roots = vec![Root {
            name: None,
            uri: "not-a-uri".to_string(),
            meta: None,
        }];

        assert_eq!(select_project_root_from_roots(&roots), None);
    }

    #[test]
    fn format_root_entry_prefers_name_when_available() {
        let root = Root {
            name: Some("trame".to_string()),
            uri: "file:///Users/amos/bearcove/trame".to_string(),
            meta: None,
        };
        assert_eq!(
            format_root_entry(&root),
            "trame: file:///Users/amos/bearcove/trame"
        );
    }

    #[test]
    fn format_root_entry_uses_uri_when_name_missing() {
        let root = Root {
            name: None,
            uri: "file:///Users/amos/bearcove/trame".to_string(),
            meta: None,
        };
        assert_eq!(
            format_root_entry(&root),
            "file:///Users/amos/bearcove/trame"
        );
    }
}

// ============================================================================
// Entry Point
// ============================================================================

/// Run the MCP bridge server over stdio.
pub async fn run(root: Option<PathBuf>, config_path: PathBuf) -> Result<()> {
    // Determine project root
    let project_root = match root {
        Some(r) => r,
        None => crate::find_project_root()?,
    };

    // Create handler
    let handler = TraceyHandler::new(project_root, config_path);

    // Configure server
    let server_details = InitializeResult {
        server_info: Implementation {
            name: "tracey".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: Some("Spec coverage tool for Rust codebases".into()),
            title: Some("Tracey".into()),
            icons: vec![],
            website_url: Some("https://github.com/bearcove/tracey".into()),
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            ..Default::default()
        },
        protocol_version: LATEST_PROTOCOL_VERSION.into(),
        instructions: Some(
            "Tracey is a spec coverage tool. Use the MCP tools to query coverage status, \
             uncovered rules, untested rules, unmapped code, and rule details."
                .into(),
        ),
        meta: None,
    };

    // Start server
    let transport = StdioTransport::new(TransportOptions::default())
        .map_err(|e| eyre::eyre!("Failed to create stdio transport: {:?}", e))?;
    let options = McpServerOptions {
        server_details,
        transport,
        handler: handler.to_mcp_server_handler(),
        task_store: None,
        client_task_store: None,
    };

    let server = server_runtime::create_server(options);
    server
        .start()
        .await
        .map_err(|e| eyre::eyre!("MCP server error: {:?}", e))?;

    Ok(())
}
