//! MCP (Model Context Protocol) server for tracey
//!
//! Exposes tracey functionality as tools for AI assistants.
//! Run with `tracey mcp` to start the MCP server over stdio.

#![allow(clippy::enum_variant_names)]

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use eyre::Result;
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::mcp_server::server_runtime;
use rust_mcp_sdk::mcp_server::{McpServerOptions, ServerHandler, ToMcpServerHandler};
use rust_mcp_sdk::schema::{
    CallToolError, CallToolRequestParams, CallToolResult, Implementation, InitializeResult,
    LATEST_PROTOCOL_VERSION, ListToolsResult, PaginatedRequestParams, RpcError, ServerCapabilities,
    ServerCapabilitiesTools,
};
use rust_mcp_sdk::{McpServer, StdioTransport, TransportOptions, tool_box};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::serve::DashboardData;
use crate::server::{Delta, QueryEngine, format_delta_section, format_status_header};

// ============================================================================
// Tool Definitions
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
    description = "List rules that have no implementation references ([impl ...] comments). Optionally filter by spec/impl or section."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UncoveredTool {
    /// Spec/impl to query (e.g., "my-spec/rust"). Optional if only one exists.
    #[serde(default)]
    pub spec_impl: Option<String>,
    /// Filter to a specific section
    #[serde(default)]
    pub section: Option<String>,
}

/// Get rules without verification references
#[mcp_tool(
    name = "tracey_untested",
    description = "List rules that have implementation but no verification references ([verify ...] comments). These rules are implemented but not tested."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UntestedTool {
    /// Spec/impl to query (e.g., "my-spec/rust"). Optional if only one exists.
    #[serde(default)]
    pub spec_impl: Option<String>,
    /// Filter to a specific section
    #[serde(default)]
    pub section: Option<String>,
}

/// Get code units without rule references
#[mcp_tool(
    name = "tracey_unmapped",
    description = "Show source tree with coverage percentages. Code units (functions, structs, etc.) without any rule references are 'unmapped'. Pass a path to zoom into a specific directory or file."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UnmappedTool {
    /// Spec/impl to query (e.g., "my-spec/rust"). Optional if only one exists.
    #[serde(default)]
    pub spec_impl: Option<String>,
    /// Path to zoom into (directory or file)
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
    /// The rule ID to look up (e.g., "channel.id.parity")
    pub rule_id: String,
}

// Generate the toolbox enum
tool_box!(
    TraceyTools,
    [
        StatusTool,
        UncoveredTool,
        UntestedTool,
        UnmappedTool,
        RuleTool
    ]
);

// ============================================================================
// MCP Handler
// ============================================================================

/// Handler for MCP requests
pub struct TraceyHandler {
    /// Current dashboard data
    data: watch::Receiver<Arc<DashboardData>>,
    /// Last delta shown to the user (for tracking what's changed since last query)
    #[allow(dead_code)]
    last_delta: std::sync::Mutex<Delta>,
}

impl TraceyHandler {
    pub fn new(data: watch::Receiver<Arc<DashboardData>>) -> Self {
        Self {
            data,
            last_delta: std::sync::Mutex::new(Delta::default()),
        }
    }

    fn get_data(&self) -> Arc<DashboardData> {
        self.data.borrow().clone()
    }

    /// Parse spec/impl from string like "my-spec/rust" or just "my-spec"
    // [impl mcp.select.single]
    // [impl mcp.select.spec-only]
    // [impl mcp.select.full]
    // [impl mcp.select.ambiguous]
    fn parse_spec_impl(
        &self,
        spec_impl: Option<&str>,
    ) -> std::result::Result<(String, String), String> {
        let data = self.get_data();
        let keys: Vec<_> = data.forward_by_impl.keys().collect();

        if keys.is_empty() {
            return Err("No specs configured".to_string());
        }

        // [impl mcp.select.single] - If only one spec/impl, use it by default
        if keys.len() == 1 && spec_impl.is_none() {
            let key = keys[0];
            return Ok((key.0.clone(), key.1.clone()));
        }

        match spec_impl {
            Some(s) => {
                // [impl mcp.select.full] - Parse spec/impl format
                if let Some((spec, impl_name)) = s.split_once('/') {
                    Ok((spec.to_string(), impl_name.to_string()))
                } else {
                    // [impl mcp.select.spec-only] - Just spec name - find the first impl
                    for key in &keys {
                        if key.0 == s {
                            return Ok((key.0.clone(), key.1.clone()));
                        }
                    }
                    Err(format!("Spec '{}' not found. Available: {:?}", s, keys))
                }
            }
            // [impl mcp.select.ambiguous] - Multiple specs, require explicit selection
            None => {
                let available: Vec<String> =
                    keys.iter().map(|k| format!("{}/{}", k.0, k.1)).collect();
                Err(format!(
                    "Multiple specs available, please specify one: {}",
                    available.join(", ")
                ))
            }
        }
    }

    /// Format the standard response header with status and delta
    fn format_header(&self) -> String {
        let data = self.get_data();
        let delta = &data.delta;

        let mut header = format_status_header(&data, delta);
        header.push('\n');
        header.push_str(&format_delta_section(delta));
        header.push('\n');
        header
    }

    // [impl mcp.tool.status]
    // [impl mcp.response.hints]
    fn handle_status(&self) -> String {
        let data = self.get_data();
        let engine = QueryEngine::new(&data);
        let status = engine.status();

        let mut out = self.format_header();
        out.push_str("# Tracey Status\n\n");

        for (spec, impl_name, stats) in &status {
            out.push_str(&format!("## {}/{}\n", spec, impl_name));
            out.push_str(&format!(
                "- Implementation coverage: {:.0}% ({}/{} rules)\n",
                stats.impl_percent, stats.impl_covered, stats.total_rules
            ));
            out.push_str(&format!(
                "- Verification coverage: {:.0}% ({}/{} rules)\n",
                stats.verify_percent, stats.verify_covered, stats.total_rules
            ));
            out.push_str(&format!(
                "- Fully covered (impl + verify): {} rules\n\n",
                stats.fully_covered
            ));
        }

        out.push_str("---\n");
        out.push_str("Available commands:\n");
        out.push_str("→ tracey_uncovered - Rules without implementation\n");
        out.push_str("→ tracey_untested - Rules without verification\n");
        out.push_str("→ tracey_unmapped - Code without requirements\n");
        out.push_str("→ tracey_rule <id> - Details about a specific rule\n");

        out
    }

    // [impl mcp.tool.uncovered]
    // [impl mcp.tool.uncovered-section]
    fn handle_uncovered(&self, spec_impl: Option<&str>, _section: Option<&str>) -> String {
        let mut out = self.format_header();

        let (spec, impl_name) = match self.parse_spec_impl(spec_impl) {
            Ok(v) => v,
            Err(e) => return format!("{}{}", out, e),
        };

        let data = self.get_data();
        let engine = QueryEngine::new(&data);

        match engine.uncovered(&spec, &impl_name) {
            Some(result) => {
                out.push_str(&result.format_text());
            }
            None => {
                out.push_str(&format!("Spec/impl '{}/{}' not found", spec, impl_name));
            }
        }

        out
    }

    // [impl mcp.tool.untested]
    // [impl mcp.tool.untested-section]
    fn handle_untested(&self, spec_impl: Option<&str>, _section: Option<&str>) -> String {
        let mut out = self.format_header();

        let (spec, impl_name) = match self.parse_spec_impl(spec_impl) {
            Ok(v) => v,
            Err(e) => return format!("{}{}", out, e),
        };

        let data = self.get_data();
        let engine = QueryEngine::new(&data);

        match engine.untested(&spec, &impl_name) {
            Some(result) => {
                out.push_str(&result.format_text());
            }
            None => {
                out.push_str(&format!("Spec/impl '{}/{}' not found", spec, impl_name));
            }
        }

        out
    }

    // [impl mcp.tool.unmapped]
    // [impl mcp.tool.unmapped-zoom]
    // [impl mcp.tool.unmapped-tree]
    // [impl mcp.tool.unmapped-file]
    fn handle_unmapped(&self, spec_impl: Option<&str>, path: Option<&str>) -> String {
        let mut out = self.format_header();

        let (spec, impl_name) = match self.parse_spec_impl(spec_impl) {
            Ok(v) => v,
            Err(e) => return format!("{}{}", out, e),
        };

        let data = self.get_data();
        let engine = QueryEngine::new(&data);

        match engine.unmapped(&spec, &impl_name, path) {
            Some(result) => {
                out.push_str(&result.format_output());
            }
            None => {
                out.push_str(&format!("Spec/impl '{}/{}' not found", spec, impl_name));
            }
        }

        out
    }

    // [impl mcp.tool.rule]
    fn handle_rule(&self, rule_id: &str) -> String {
        let mut out = self.format_header();

        let data = self.get_data();
        let engine = QueryEngine::new(&data);

        match engine.rule(rule_id) {
            Some(rule) => {
                out.push_str(&rule.format_text());
            }
            None => {
                out.push_str(&format!("Rule '{}' not found", rule_id));
            }
        }

        out
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
        // Parse arguments, defaulting to empty object if missing
        let args = params.arguments.unwrap_or_default();

        let response = match params.name.as_str() {
            "tracey_status" => self.handle_status(),
            "tracey_uncovered" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let section = args.get("section").and_then(|v| v.as_str());
                self.handle_uncovered(spec_impl, section)
            }
            "tracey_untested" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let section = args.get("section").and_then(|v| v.as_str());
                self.handle_untested(spec_impl, section)
            }
            "tracey_unmapped" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let path = args.get("path").and_then(|v| v.as_str());
                self.handle_unmapped(spec_impl, path)
            }
            "tracey_rule" => {
                let rule_id = args.get("rule_id").and_then(|v| v.as_str());
                match rule_id {
                    Some(id) => self.handle_rule(id),
                    None => "Error: rule_id is required".to_string(),
                }
            }
            other => format!("Unknown tool: {}", other),
        };

        Ok(CallToolResult::text_content(vec![response.into()]))
    }
}

// ============================================================================
// Server Entry Point
// ============================================================================

/// Run the MCP server
pub async fn run(root: Option<PathBuf>, config_path: Option<PathBuf>) -> Result<()> {
    use crate::serve::build_dashboard_data;
    use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use tokio::sync::mpsc;

    // Determine project root
    let project_root = match root {
        Some(r) => r,
        None => crate::find_project_root()?,
    };

    // Load config
    let config_path = config_path.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));
    let config = crate::load_config(&config_path)?;

    // Build initial dashboard data
    let initial_data: DashboardData = build_dashboard_data(&project_root, &config, 1, true).await?;

    // [impl server.state.shared] - Create watch channel for data updates
    let (data_tx, data_rx) = watch::channel(Arc::new(initial_data));

    // Create channel for file watcher debouncing
    let (debounce_tx, mut debounce_rx) = mpsc::channel::<()>(1);

    // [impl server.watch.sources]
    // [impl server.watch.specs]
    // [impl server.watch.config]
    // [impl server.watch.debounce] - Start file watcher
    let watch_root = project_root.clone();
    let rt = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let tx = debounce_tx;

        // [impl server.watch.debounce] - 200ms debounce
        let mut debouncer = match new_debouncer(
            Duration::from_millis(200),
            move |res: std::result::Result<
                Vec<notify_debouncer_mini::DebouncedEvent>,
                notify_debouncer_mini::notify::Error,
            >| {
                if let Ok(events) = res {
                    let dominated_by_exclusions = events.iter().all(|e| {
                        e.path.components().any(|c: std::path::Component| {
                            let comp = c.as_os_str().to_string_lossy();
                            comp.starts_with("node_modules")
                                || comp.starts_with("target")
                                || comp.starts_with(".git")
                                || comp.starts_with("dashboard")
                                || comp.starts_with(".vite")
                        })
                    });

                    if !dominated_by_exclusions {
                        let _ = rt.block_on(async { tx.send(()).await });
                    }
                }
            },
        ) {
            Ok(d) => d,
            Err(_) => return,
        };

        let _ = debouncer
            .watcher()
            .watch(&watch_root, RecursiveMode::Recursive);

        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    });

    // Start rebuild task
    let rebuild_project_root = project_root.clone();
    let rebuild_config_path = config_path.clone();
    let rebuild_tx = data_tx;
    let rebuild_rx = data_rx.clone();
    // [impl server.state.version]
    let version = Arc::new(AtomicU64::new(1));

    tokio::spawn(async move {
        while debounce_rx.recv().await.is_some() {
            // [impl server.watch.config] - Reload config on changes
            let config = match crate::load_config(&rebuild_config_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let old_data = rebuild_rx.borrow().clone();

            if let Ok(mut data) =
                build_dashboard_data(&rebuild_project_root, &config, 0, true).await
                && data.content_hash != old_data.content_hash
            {
                // [impl server.state.version] - Increment version on data changes
                let new_version = version.fetch_add(1, Ordering::SeqCst) + 1;
                data.version = new_version;
                data.delta = crate::server::Delta::compute(&old_data, &data);
                let _ = rebuild_tx.send(Arc::new(data));
            }
        }
    });

    // Create MCP handler
    let handler = TraceyHandler::new(data_rx);

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
            "Tracey is a spec coverage tool. Use tracey_status to see coverage overview, \
             tracey_uncovered to find unimplemented rules, tracey_untested to find untested rules, \
             tracey_unmapped to find code without requirements, and tracey_rule to get rule details."
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
