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

/// Display current configuration
#[mcp_tool(
    name = "tracey_config",
    description = "Display the current configuration for all specs and implementations, including include/exclude patterns."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConfigTool {}

/// Add exclude pattern to implementation
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

/// Add include pattern to implementation
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

// Generate the toolbox enum
tool_box!(
    TraceyTools,
    [
        StatusTool,
        UncoveredTool,
        UntestedTool,
        UnmappedTool,
        RuleTool,
        ConfigTool,
        ConfigExcludeTool,
        ConfigIncludeTool
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
    /// Path to config file for persistence
    config_path: PathBuf,
}

impl TraceyHandler {
    pub fn new(data: watch::Receiver<Arc<DashboardData>>, config_path: PathBuf) -> Self {
        Self {
            data,
            last_delta: std::sync::Mutex::new(Delta::default()),
            config_path,
        }
    }

    fn get_data(&self) -> Arc<DashboardData> {
        self.data.borrow().clone()
    }

    /// Parse spec/impl from string like "my-spec/rust" or just "my-spec"
    // r[impl mcp.select.single]
    // r[impl mcp.select.spec-only]
    // r[impl mcp.select.full]
    // r[impl mcp.select.ambiguous]
    fn parse_spec_impl(
        &self,
        spec_impl: Option<&str>,
    ) -> std::result::Result<(String, String), String> {
        let data = self.get_data();
        let keys: Vec<_> = data.forward_by_impl.keys().collect();

        if keys.is_empty() {
            return Err("No specs configured".to_string());
        }

        // r[impl mcp.select.single] - If only one spec/impl, use it by default
        if keys.len() == 1 && spec_impl.is_none() {
            let key = keys[0];
            return Ok((key.0.clone(), key.1.clone()));
        }

        match spec_impl {
            Some(s) => {
                // r[impl mcp.select.full] - Parse spec/impl format
                if let Some((spec, impl_name)) = s.split_once('/') {
                    Ok((spec.to_string(), impl_name.to_string()))
                } else {
                    // r[impl mcp.select.spec-only] - Just spec name - find the first impl
                    for key in &keys {
                        if key.0 == s {
                            return Ok((key.0.clone(), key.1.clone()));
                        }
                    }
                    Err(format!("Spec '{}' not found. Available: {:?}", s, keys))
                }
            }
            // r[impl mcp.select.ambiguous] - Multiple specs, require explicit selection
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

    // r[impl mcp.tool.status]
    // r[impl mcp.response.hints]
    fn handle_status(&self) -> String {
        let data = self.get_data();
        let engine = QueryEngine::new(&data);
        let status = engine.status();

        let mut out = self.format_header();
        out.push_str("# Tracey Status\n\n");

        // Show configured specs with prefixes
        out.push_str("## Configured Specs\n\n");
        for spec_info in &data.config.specs {
            out.push_str(&format!(
                "- **{}** (prefix: `{}`)\n",
                spec_info.name, spec_info.prefix
            ));
            out.push_str(&format!(
                "  - Implementations: {}\n",
                spec_info.implementations.join(", ")
            ));
            out.push_str(&format!(
                "  - When annotating code, use: `{}[impl rule.id]` or `{}[verify rule.id]`\n\n",
                spec_info.prefix, spec_info.prefix
            ));
        }

        out.push_str("---\n\n");
        out.push_str("## Coverage by Implementation\n\n");

        for (spec, impl_name, stats) in &status {
            out.push_str(&format!("### {}/{}\n", spec, impl_name));
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

    // r[impl mcp.tool.uncovered]
    // r[impl mcp.tool.uncovered-section]
    fn handle_uncovered(&self, spec_impl: Option<&str>, section: Option<&str>) -> String {
        let mut out = self.format_header();

        let (spec, impl_name) = match self.parse_spec_impl(spec_impl) {
            Ok(v) => v,
            Err(e) => return format!("{}{}", out, e),
        };

        let data = self.get_data();
        let engine = QueryEngine::new(&data);

        match engine.uncovered(&spec, &impl_name, section) {
            Some(result) => {
                out.push_str(&result.format_text());
            }
            None => {
                out.push_str(&format!("Spec/impl '{}/{}' not found", spec, impl_name));
            }
        }

        out
    }

    // r[impl mcp.tool.untested]
    // r[impl mcp.tool.untested-section]
    fn handle_untested(&self, spec_impl: Option<&str>, section: Option<&str>) -> String {
        let mut out = self.format_header();

        let (spec, impl_name) = match self.parse_spec_impl(spec_impl) {
            Ok(v) => v,
            Err(e) => return format!("{}{}", out, e),
        };

        let data = self.get_data();
        let engine = QueryEngine::new(&data);

        match engine.untested(&spec, &impl_name, section) {
            Some(result) => {
                out.push_str(&result.format_text());
            }
            None => {
                out.push_str(&format!("Spec/impl '{}/{}' not found", spec, impl_name));
            }
        }

        out
    }

    // r[impl mcp.tool.unmapped]
    // r[impl mcp.tool.unmapped-zoom]
    // r[impl mcp.tool.unmapped-tree]
    // r[impl mcp.tool.unmapped-file]
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

    // r[impl mcp.tool.rule]
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

    // r[impl mcp.config.list]
    fn handle_config(&self) -> String {
        let mut out = self.format_header();

        // Load current config
        let config = match crate::load_config(&self.config_path) {
            Ok(c) => c,
            Err(e) => {
                out.push_str(&format!("Error loading config: {}", e));
                return out;
            }
        };

        out.push_str("# Tracey Configuration\n\n");
        out.push_str(&format!("Config file: {}\n\n", self.config_path.display()));

        for spec in &config.specs {
            out.push_str(&format!(
                "## Spec: {} (prefix: `{}`)\n",
                spec.name.value, spec.prefix.value
            ));
            out.push_str("**Requirement files:**\n");
            for include in &spec.include {
                out.push_str(&format!("- `{}`\n", include.pattern));
            }
            out.push('\n');

            for impl_ in &spec.impls {
                out.push_str(&format!("### Implementation: {}\n", impl_.name.value));

                if !impl_.include.is_empty() {
                    out.push_str("**Include patterns:**\n");
                    for include in &impl_.include {
                        out.push_str(&format!("- `{}`\n", include.pattern));
                    }
                    out.push('\n');
                }

                if !impl_.exclude.is_empty() {
                    out.push_str("**Exclude patterns:**\n");
                    for exclude in &impl_.exclude {
                        out.push_str(&format!("- `{}`\n", exclude.pattern));
                    }
                    out.push('\n');
                }
            }
        }

        out.push_str("---\n");
        out.push_str("Use tracey_config_include to add include patterns\n");
        out.push_str("Use tracey_config_exclude to add exclude patterns\n");

        out
    }

    // r[impl mcp.config.exclude]
    fn handle_config_exclude(&self, spec_impl: Option<&str>, pattern: &str) -> String {
        let mut out = self.format_header();

        let (spec_name, impl_name) = match self.parse_spec_impl(spec_impl) {
            Ok(v) => v,
            Err(e) => return format!("{}{}", out, e),
        };

        // Load current config
        let mut config = match crate::load_config(&self.config_path) {
            Ok(c) => c,
            Err(e) => {
                out.push_str(&format!("Error loading config: {}", e));
                return out;
            }
        };

        // Find the spec and impl
        let mut found = false;
        for spec in &mut config.specs {
            if spec.name.value == spec_name {
                for impl_ in &mut spec.impls {
                    if impl_.name.value == impl_name {
                        impl_.exclude.push(crate::config::Exclude {
                            pattern: pattern.to_string(),
                        });
                        found = true;
                        break;
                    }
                }
                break;
            }
        }

        if !found {
            out.push_str(&format!(
                "Spec/impl '{}/{}' not found",
                spec_name, impl_name
            ));
            return out;
        }

        // r[impl mcp.config.persist] - Save config back to file
        if let Err(e) = self.save_config(&config) {
            out.push_str(&format!("Error saving config: {}", e));
            return out;
        }

        out.push_str(&format!(
            "Added exclude pattern '{}' to {}/{}\n\n",
            pattern, spec_name, impl_name
        ));
        out.push_str("Configuration saved successfully.\n");

        out
    }

    // r[impl mcp.config.include]
    fn handle_config_include(&self, spec_impl: Option<&str>, pattern: &str) -> String {
        let mut out = self.format_header();

        let (spec_name, impl_name) = match self.parse_spec_impl(spec_impl) {
            Ok(v) => v,
            Err(e) => return format!("{}{}", out, e),
        };

        // Load current config
        let mut config = match crate::load_config(&self.config_path) {
            Ok(c) => c,
            Err(e) => {
                out.push_str(&format!("Error loading config: {}", e));
                return out;
            }
        };

        // Find the spec and impl
        let mut found = false;
        for spec in &mut config.specs {
            if spec.name.value == spec_name {
                for impl_ in &mut spec.impls {
                    if impl_.name.value == impl_name {
                        impl_.include.push(crate::config::Include {
                            pattern: pattern.to_string(),
                        });
                        found = true;
                        break;
                    }
                }
                break;
            }
        }

        if !found {
            out.push_str(&format!(
                "Spec/impl '{}/{}' not found",
                spec_name, impl_name
            ));
            return out;
        }

        // r[impl mcp.config.persist] - Save config back to file
        if let Err(e) = self.save_config(&config) {
            out.push_str(&format!("Error saving config: {}", e));
            return out;
        }

        out.push_str(&format!(
            "Added include pattern '{}' to {}/{}\n\n",
            pattern, spec_name, impl_name
        ));
        out.push_str("Configuration saved successfully.\n");

        out
    }

    // r[impl mcp.config.persist]
    fn save_config(&self, config: &crate::config::Config) -> Result<()> {
        use std::io::Write;

        let kdl_string = facet_kdl::to_string(config)?;
        let mut file = std::fs::File::create(&self.config_path)?;
        file.write_all(kdl_string.as_bytes())?;
        Ok(())
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
            "tracey_config" => self.handle_config(),
            "tracey_config_exclude" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let pattern = args.get("pattern").and_then(|v| v.as_str());
                match pattern {
                    Some(p) => self.handle_config_exclude(spec_impl, p),
                    None => "Error: pattern is required".to_string(),
                }
            }
            "tracey_config_include" => {
                let spec_impl = args.get("spec_impl").and_then(|v| v.as_str());
                let pattern = args.get("pattern").and_then(|v| v.as_str());
                match pattern {
                    Some(p) => self.handle_config_include(spec_impl, p),
                    None => "Error: pattern is required".to_string(),
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

    // r[impl server.state.shared] - Create watch channel for data updates
    let (data_tx, data_rx) = watch::channel(Arc::new(initial_data));

    // Create channel for file watcher debouncing
    let (debounce_tx, mut debounce_rx) = mpsc::channel::<()>(1);

    // r[impl server.watch.sources]
    // r[impl server.watch.specs]
    // r[impl server.watch.config]
    // r[impl server.watch.debounce] - Start file watcher
    let watch_root = project_root.clone();
    let rt = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let tx = debounce_tx;

        // r[impl server.watch.debounce] - 200ms debounce
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
    // r[impl server.state.version]
    let version = Arc::new(AtomicU64::new(1));

    tokio::spawn(async move {
        while debounce_rx.recv().await.is_some() {
            // r[impl server.watch.config] - Reload config on changes
            let config = match crate::load_config(&rebuild_config_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let old_data = rebuild_rx.borrow().clone();

            if let Ok(mut data) =
                build_dashboard_data(&rebuild_project_root, &config, 0, true).await
                && data.content_hash != old_data.content_hash
            {
                // r[impl server.state.version] - Increment version on data changes
                let new_version = version.fetch_add(1, Ordering::SeqCst) + 1;
                data.version = new_version;
                data.delta = crate::server::Delta::compute(&old_data, &data);
                let _ = rebuild_tx.send(Arc::new(data));
            }
        }
    });

    // Create MCP handler
    let handler = TraceyHandler::new(data_rx, config_path.clone());

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
