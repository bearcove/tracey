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
    LATEST_PROTOCOL_VERSION, ListToolsResult, PaginatedRequestParams, RpcError, ServerCapabilities,
    ServerCapabilitiesTools,
};
use rust_mcp_sdk::{McpServer, StdioTransport, ToMcpServerHandler, TransportOptions, tool_box};
use serde::{Deserialize, Serialize};

use crate::bridge::query;
use tracey_proto::*;

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

/// MCP handler that delegates to the daemon.
struct TraceyHandler {
    client: query::QueryClient,
}

impl TraceyHandler {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            client: query::QueryClient::new(project_root),
        }
    }
}

#[async_trait]
impl ServerHandler for TraceyHandler {
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
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        let args = params.arguments.unwrap_or_default();

        let response = match params.name.as_str() {
            "tracey_status" => self.client.status().await,
            "tracey_uncovered" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let prefix = args.get("prefix").and_then(|v| v.as_str());
                self.client.uncovered(spec_impl, prefix).await
            }
            "tracey_untested" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let prefix = args.get("prefix").and_then(|v| v.as_str());
                self.client.untested(spec_impl, prefix).await
            }
            "tracey_unmapped" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let path = args.get("path").and_then(|v| v.as_str());
                self.client.unmapped(spec_impl, path).await
            }
            "tracey_rule" => {
                let rule_id = args.get("rule_id").and_then(|v| v.as_str());
                match rule_id {
                    Some(id) => self.client.rule(id).await,
                    None => "Error: rule_id is required".to_string(),
                }
            }
            "tracey_config" => self.client.config().await,
            "tracey_reload" => self.client.reload().await,
            "tracey_validate" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                self.client.validate(spec_impl).await
            }
            "tracey_config_exclude" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let pattern = args.get("pattern").and_then(|v| v.as_str());
                match pattern {
                    Some(p) => self.client.config_exclude(spec_impl, p).await,
                    None => "Error: pattern is required".to_string(),
                }
            }
            "tracey_config_include" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let pattern = args.get("pattern").and_then(|v| v.as_str());
                match pattern {
                    Some(p) => self.client.config_include(spec_impl, p).await,
                    None => "Error: pattern is required".to_string(),
                }
            }
            other => format!("Unknown tool: {}", other),
        };

        Ok(CallToolResult::text_content(vec![response.into()]))
    }
}

// ============================================================================
// Entry Point
// ============================================================================

/// Run the MCP bridge server over stdio.
pub async fn run(root: Option<PathBuf>, _config_path: PathBuf) -> Result<()> {
    // Determine project root
    let project_root = match root {
        Some(r) => r,
        None => crate::find_project_root()?,
    };

    // Create handler
    let handler = TraceyHandler::new(project_root);

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
