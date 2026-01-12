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
use tokio::sync::Mutex;

use crate::daemon::{DaemonClient, new_client};
use tracey_proto::*;

/// Flatten double-Result from roam RPC calls into Result<T, String>
fn rpc<T, E: std::fmt::Debug>(
    res: Result<Result<T, roam::session::RoamError<E>>, roam_stream::ConnectError>,
) -> Result<T, String> {
    match res {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(format!("RPC error: {:?}", e)),
        Err(e) => Err(format!("Connection error: {}", e)),
    }
}

/// Format config error as a warning banner to prepend to responses
fn format_config_error_banner(error: &str) -> String {
    format!(
        "⚠️  CONFIG ERROR ⚠️\n{}\n\nFix the config file and the daemon will automatically reload.\n\n---\n\n",
        error
    )
}

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
/// r[impl daemon.bridge.reconnect]
struct TraceyHandler {
    client: Arc<Mutex<DaemonClient>>,
}

impl TraceyHandler {
    /// Check for config errors and return a warning banner if present.
    async fn get_config_error_banner(&self) -> Option<String> {
        let client = self.client.lock().await;
        match rpc(client.health().await) {
            Ok(health) => health.config_error.map(|e| format_config_error_banner(&e)),
            Err(_) => None,
        }
    }

    /// r[impl mcp.tool.status]
    /// r[impl mcp.response.hints]
    async fn handle_status(&self) -> String {
        let client = self.client.lock().await;
        match rpc(client.status().await) {
            Ok(status) => {
                let mut output = String::new();
                for impl_status in &status.impls {
                    let impl_pct = if impl_status.total_rules > 0 {
                        impl_status.covered_rules as f64 / impl_status.total_rules as f64 * 100.0
                    } else {
                        0.0
                    };
                    let verify_pct = if impl_status.total_rules > 0 {
                        impl_status.verified_rules as f64 / impl_status.total_rules as f64 * 100.0
                    } else {
                        0.0
                    };
                    output.push_str(&format!(
                        "{}/{}: impl {:.0}%, verify {:.0}% ({}/{} rules)\n",
                        impl_status.spec,
                        impl_status.impl_name,
                        impl_pct,
                        verify_pct,
                        impl_status.covered_rules,
                        impl_status.total_rules
                    ));
                }
                if output.is_empty() {
                    "No specs configured".to_string()
                } else {
                    output.push_str("\n---\n");
                    output.push_str("→ Use tracey_uncovered to see rules without implementation\n");
                    output.push_str("→ Use tracey_untested to see rules without verification\n");
                    output.push_str("→ Use tracey_unmapped to see code without requirements\n");
                    output
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// r[impl mcp.tool.uncovered]
    /// r[impl mcp.response.hints]
    async fn handle_uncovered(&self, spec_impl: Option<&str>, prefix: Option<&str>) -> String {
        let client = self.client.lock().await;
        let (spec, impl_name) = parse_spec_impl(spec_impl);

        let req = UncoveredRequest {
            spec,
            impl_name,
            prefix: prefix.map(String::from),
        };

        match rpc(client.uncovered(req).await) {
            Ok(response) => {
                let mut output = format!(
                    "{}/{}: {} uncovered out of {} rules\n\n",
                    response.spec,
                    response.impl_name,
                    response.uncovered_count,
                    response.total_rules
                );

                for section in &response.by_section {
                    if !section.rules.is_empty() {
                        output.push_str(&format!("## {}\n", section.section));
                        for rule in &section.rules {
                            output.push_str(&format!("  - {}\n", rule.id));
                        }
                        output.push('\n');
                    }
                }

                output.push_str("---\n");
                output.push_str("→ Use tracey_rule to see details about a specific rule\n");
                output.push_str("→ Use prefix parameter to filter by rule ID prefix\n");

                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// r[impl mcp.tool.untested]
    /// r[impl mcp.response.hints]
    async fn handle_untested(&self, spec_impl: Option<&str>, prefix: Option<&str>) -> String {
        let client = self.client.lock().await;
        let (spec, impl_name) = parse_spec_impl(spec_impl);

        let req = UntestedRequest {
            spec,
            impl_name,
            prefix: prefix.map(String::from),
        };

        match rpc(client.untested(req).await) {
            Ok(response) => {
                let mut output = format!(
                    "{}/{}: {} untested (impl but no verify) out of {} rules\n\n",
                    response.spec,
                    response.impl_name,
                    response.untested_count,
                    response.total_rules
                );

                for section in &response.by_section {
                    if !section.rules.is_empty() {
                        output.push_str(&format!("## {}\n", section.section));
                        for rule in &section.rules {
                            output.push_str(&format!("  - {}\n", rule.id));
                        }
                        output.push('\n');
                    }
                }

                output.push_str("---\n");
                output.push_str("→ Use tracey_rule to see details about a specific rule\n");
                output.push_str("→ Use prefix parameter to filter by rule ID prefix\n");

                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// r[impl mcp.tool.unmapped]
    /// r[impl mcp.tool.unmapped-zoom]
    /// r[impl mcp.tool.unmapped-tree]
    /// r[impl mcp.tool.unmapped-file]
    async fn handle_unmapped(&self, spec_impl: Option<&str>, path: Option<&str>) -> String {
        let client = self.client.lock().await;
        let (spec, impl_name) = parse_spec_impl(spec_impl);

        let req = UnmappedRequest {
            spec,
            impl_name,
            path: path.map(String::from),
        };

        match rpc(client.unmapped(req).await) {
            Ok(response) => {
                let mut output = format!(
                    "{}/{}: {} unmapped code units out of {} total\n\n",
                    response.spec,
                    response.impl_name,
                    response.unmapped_count,
                    response.total_units
                );

                // Check if we're zoomed into a file with unit details
                let has_unit_details = response.entries.iter().any(|e| !e.units.is_empty());

                if has_unit_details {
                    // File zoom view - show unmapped code units with line numbers
                    for entry in &response.entries {
                        if !entry.units.is_empty() {
                            output.push_str(&format!("## {}\n\n", entry.path));
                            for unit in &entry.units {
                                let name = unit.name.as_deref().unwrap_or("<anonymous>");
                                output.push_str(&format!(
                                    "  L{}-{}: {} `{}`\n",
                                    unit.start_line, unit.end_line, unit.kind, name
                                ));
                            }
                            output.push('\n');
                        }
                    }
                } else {
                    // Tree view - format as ASCII tree with progress bars
                    for (i, entry) in response.entries.iter().enumerate() {
                        let pct = if entry.total_units > 0 {
                            (entry.total_units - entry.unmapped_units) as f64
                                / entry.total_units as f64
                                * 100.0
                        } else {
                            100.0
                        };

                        // Progress bar (10 chars)
                        let filled = (pct / 10.0).round() as usize;
                        let bar: String = "█".repeat(filled) + &"░".repeat(10 - filled);

                        // Tree connector
                        let is_last = i == response.entries.len() - 1;
                        let connector = if is_last { "└── " } else { "├── " };

                        output.push_str(&format!(
                            "{}{:<30} {:>3.0}% {}\n",
                            connector, entry.path, pct, bar
                        ));
                    }
                }

                // r[impl mcp.response.hints]
                output.push_str("\n---\n");
                output.push_str("→ Use path parameter to zoom into a directory or file\n");

                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    async fn handle_rule(&self, rule_id: &str) -> String {
        let client = self.client.lock().await;
        match rpc(client.rule(rule_id.to_string()).await) {
            Ok(Some(info)) => {
                let mut output = format!("# {}\n\n{}\n\n", info.id, info.raw);

                if let Some(file) = &info.source_file
                    && let Some(line) = info.source_line
                {
                    output.push_str(&format!("Defined in: {}:{}\n\n", file, line));
                }

                for cov in &info.coverage {
                    output.push_str(&format!("\n## {}/{}\n", cov.spec, cov.impl_name));
                    if !cov.impl_refs.is_empty() {
                        output.push_str("Impl references:\n");
                        for r in &cov.impl_refs {
                            output.push_str(&format!("  - {}:{}\n", r.file, r.line));
                        }
                    }
                    if !cov.verify_refs.is_empty() {
                        output.push_str("Verify references:\n");
                        for r in &cov.verify_refs {
                            output.push_str(&format!("  - {}:{}\n", r.file, r.line));
                        }
                    }
                }

                output
            }
            Ok(None) => format!("Rule not found: {}", rule_id),
            Err(e) => format!("Error: {}", e),
        }
    }

    /// r[impl mcp.config.list]
    async fn handle_config(&self) -> String {
        let client = self.client.lock().await;
        match rpc(client.config().await) {
            Ok(config) => {
                let mut output = String::from("# Tracey Configuration\n\n");

                for spec in &config.specs {
                    output.push_str(&format!("## Spec: {}\n", spec.name));
                    output.push_str(&format!("  Prefix: {}\n", spec.prefix));
                    if let Some(source) = &spec.source {
                        output.push_str(&format!("  Source: {}\n", source));
                    }
                    output.push_str(&format!(
                        "  Implementations: {}\n\n",
                        spec.implementations.join(", ")
                    ));
                }

                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    async fn handle_reload(&self) -> String {
        let client = self.client.lock().await;
        match rpc(client.reload().await) {
            Ok(response) => {
                format!(
                    "Reload complete (version {}, took {}ms)",
                    response.version, response.rebuild_time_ms
                )
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    async fn handle_validate(&self, spec_impl: Option<&str>) -> String {
        let client = self.client.lock().await;

        // If a specific spec/impl was requested, validate just that one
        if spec_impl.is_some() {
            let (spec, impl_name) = parse_spec_impl(spec_impl);
            let req = ValidateRequest { spec, impl_name };
            return match rpc(client.validate(req).await) {
                Ok(result) => format_validation_result(&result),
                Err(e) => format!("Error: {}", e),
            };
        }

        // No filter provided: validate ALL spec/impl combinations
        let status = match rpc(client.status().await) {
            Ok(s) => s,
            Err(e) => return format!("Error getting status: {}", e),
        };

        if status.impls.is_empty() {
            return "No spec/impl combinations configured.".to_string();
        }

        let mut output = String::new();
        let mut total_errors = 0;

        for impl_status in &status.impls {
            let req = ValidateRequest {
                spec: Some(impl_status.spec.clone()),
                impl_name: Some(impl_status.impl_name.clone()),
            };

            match rpc(client.validate(req).await) {
                Ok(result) => {
                    total_errors += result.error_count;
                    output.push_str(&format_validation_result(&result));
                    output.push('\n');
                }
                Err(e) => {
                    output.push_str(&format!(
                        "✗ {}/{}: Error: {}\n\n",
                        impl_status.spec, impl_status.impl_name, e
                    ));
                }
            }
        }

        // Summary
        output.push_str("---\n");
        output.push_str(&format!(
            "Validated {} spec/impl combination(s), {} total error(s)\n",
            status.impls.len(),
            total_errors
        ));
        output.push_str(
            "→ Use spec_impl parameter to validate a specific one (e.g., \"my-spec/rust\")\n",
        );

        output
    }

    async fn handle_config_exclude(&self, spec_impl: Option<&str>, pattern: &str) -> String {
        let client = self.client.lock().await;
        let (spec, impl_name) = parse_spec_impl(spec_impl);

        let req = ConfigPatternRequest {
            spec,
            impl_name,
            pattern: pattern.to_string(),
        };

        match rpc(client.config_add_exclude(req).await) {
            Ok(()) => format!("Added exclude pattern: {}", pattern),
            Err(e) => format!("Error: {}", e),
        }
    }

    async fn handle_config_include(&self, spec_impl: Option<&str>, pattern: &str) -> String {
        let client = self.client.lock().await;
        let (spec, impl_name) = parse_spec_impl(spec_impl);

        let req = ConfigPatternRequest {
            spec,
            impl_name,
            pattern: pattern.to_string(),
        };

        match rpc(client.config_add_include(req).await) {
            Ok(()) => format!("Added include pattern: {}", pattern),
            Err(e) => format!("Error: {}", e),
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

        // Check for config errors to prepend to response
        let config_error_banner = self.get_config_error_banner().await;

        let response = match params.name.as_str() {
            "tracey_status" => self.handle_status().await,
            "tracey_uncovered" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let prefix = args.get("prefix").and_then(|v| v.as_str());
                self.handle_uncovered(spec_impl, prefix).await
            }
            "tracey_untested" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let prefix = args.get("prefix").and_then(|v| v.as_str());
                self.handle_untested(spec_impl, prefix).await
            }
            "tracey_unmapped" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let path = args.get("path").and_then(|v| v.as_str());
                self.handle_unmapped(spec_impl, path).await
            }
            "tracey_rule" => {
                let rule_id = args.get("rule_id").and_then(|v| v.as_str());
                match rule_id {
                    Some(id) => self.handle_rule(id).await,
                    None => "Error: rule_id is required".to_string(),
                }
            }
            "tracey_config" => self.handle_config().await,
            "tracey_reload" => self.handle_reload().await,
            "tracey_validate" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                self.handle_validate(spec_impl).await
            }
            "tracey_config_exclude" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let pattern = args.get("pattern").and_then(|v| v.as_str());
                match pattern {
                    Some(p) => self.handle_config_exclude(spec_impl, p).await,
                    None => "Error: pattern is required".to_string(),
                }
            }
            "tracey_config_include" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let pattern = args.get("pattern").and_then(|v| v.as_str());
                match pattern {
                    Some(p) => self.handle_config_include(spec_impl, p).await,
                    None => "Error: pattern is required".to_string(),
                }
            }
            other => format!("Unknown tool: {}", other),
        };

        // Prepend config error banner if present
        let final_response = match config_error_banner {
            Some(banner) => format!("{}{}", banner, response),
            None => response,
        };

        Ok(CallToolResult::text_content(vec![final_response.into()]))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse "spec/impl" format into `(Option<spec>, Option<impl>)`.
///
/// r[impl mcp.select.single]
/// r[impl mcp.select.full]
/// r[impl mcp.select.spec-only]
/// r[impl mcp.select.ambiguous]
fn parse_spec_impl(spec_impl: Option<&str>) -> (Option<String>, Option<String>) {
    match spec_impl {
        Some(s) if s.contains('/') => {
            let parts: Vec<&str> = s.splitn(2, '/').collect();
            (Some(parts[0].to_string()), Some(parts[1].to_string()))
        }
        Some(s) => (Some(s.to_string()), None),
        None => (None, None),
    }
}

/// Format a validation result for display.
fn format_validation_result(result: &tracey_proto::ValidationResult) -> String {
    if result.errors.is_empty() {
        format!(
            "✓ {}/{}: No validation errors found",
            result.spec, result.impl_name
        )
    } else {
        let mut output = format!(
            "✗ {}/{}: {} error(s) found\n",
            result.spec, result.impl_name, result.error_count
        );

        for error in &result.errors {
            let location = match (&error.file, error.line) {
                (Some(f), Some(l)) => format!(" at {}:{}", f, l),
                (Some(f), None) => format!(" in {}", f),
                _ => String::new(),
            };

            output.push_str(&format!(
                "  - [{:?}] {}{}\n",
                error.code, error.message, location
            ));

            if !error.related_rules.is_empty() {
                output.push_str(&format!(
                    "    Related rules: {}\n",
                    error.related_rules.join(", ")
                ));
            }
        }

        output
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

    // Create client (connects lazily, auto-reconnects)
    // r[impl daemon.bridge.reconnect]
    let client = new_client(project_root);

    // Create handler
    let handler = TraceyHandler {
        client: Arc::new(Mutex::new(client)),
    };

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
