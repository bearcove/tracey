//! TraceyDaemon service implementation.
//!
//! Implements the roam RPC service by delegating to the Engine.

use roam::Pull;
use std::sync::Arc;
use tracey_proto::*;

use super::engine::Engine;
use crate::server::QueryEngine;

/// Service implementation wrapping the Engine.
pub struct TraceyService {
    engine: Arc<Engine>,
}

impl TraceyService {
    /// Create a new service wrapping the given engine.
    pub fn new(engine: Arc<Engine>) -> Self {
        Self { engine }
    }

    // Helper: resolve spec/impl from optional parameters
    fn resolve_spec_impl(
        &self,
        spec: Option<&str>,
        impl_name: Option<&str>,
        config: &ApiConfig,
    ) -> (String, String) {
        // If spec not provided, use first spec
        let spec_name = spec.map(String::from).unwrap_or_else(|| {
            config
                .specs
                .first()
                .map(|s| s.name.clone())
                .unwrap_or_default()
        });

        // If impl not provided, use first impl for that spec
        let impl_name = impl_name.map(String::from).unwrap_or_else(|| {
            config
                .specs
                .iter()
                .find(|s| s.name == spec_name)
                .and_then(|s| s.implementations.first().cloned())
                .unwrap_or_default()
        });

        (spec_name, impl_name)
    }
}

/// Implementation of the TraceyDaemon trait from tracey-proto.
impl TraceyDaemon for TraceyService {
    /// Get coverage status for all specs/impls
    async fn status(&self) -> StatusResponse {
        let data = self.engine.data().await;
        let query = QueryEngine::new(&data);
        let stats = query.status();

        StatusResponse {
            impls: stats
                .into_iter()
                .map(|(spec, impl_name, s)| ImplStatus {
                    spec,
                    impl_name,
                    total_rules: s.total_rules,
                    covered_rules: s.impl_covered,
                    verified_rules: s.verify_covered,
                })
                .collect(),
        }
    }

    /// Get uncovered rules
    async fn uncovered(&self, req: UncoveredRequest) -> UncoveredResponse {
        let data = self.engine.data().await;
        let query = QueryEngine::new(&data);

        // Find the spec/impl to query
        let (spec, impl_name) =
            self.resolve_spec_impl(req.spec.as_deref(), req.impl_name.as_deref(), &data.config);

        if let Some(result) = query.uncovered(&spec, &impl_name, req.prefix.as_deref()) {
            UncoveredResponse {
                spec: result.spec,
                impl_name: result.impl_name,
                total_rules: result.stats.total_rules,
                uncovered_count: result.total_uncovered,
                by_section: result
                    .by_section
                    .into_iter()
                    .map(|(section, rules)| SectionRules {
                        section,
                        rules: rules
                            .into_iter()
                            .map(|r| tracey_proto::RuleRef {
                                id: r.id,
                                text: None, // RuleRef in server.rs doesn't have text
                            })
                            .collect(),
                    })
                    .collect(),
            }
        } else {
            UncoveredResponse {
                spec,
                impl_name,
                total_rules: 0,
                uncovered_count: 0,
                by_section: vec![],
            }
        }
    }

    /// Get untested rules
    async fn untested(&self, req: UntestedRequest) -> UntestedResponse {
        let data = self.engine.data().await;
        let query = QueryEngine::new(&data);

        let (spec, impl_name) =
            self.resolve_spec_impl(req.spec.as_deref(), req.impl_name.as_deref(), &data.config);

        if let Some(result) = query.untested(&spec, &impl_name, req.prefix.as_deref()) {
            UntestedResponse {
                spec: result.spec,
                impl_name: result.impl_name,
                total_rules: result.stats.total_rules,
                untested_count: result.total_untested,
                by_section: result
                    .by_section
                    .into_iter()
                    .map(|(section, rules)| SectionRules {
                        section,
                        rules: rules
                            .into_iter()
                            .map(|r| tracey_proto::RuleRef {
                                id: r.id,
                                text: None,
                            })
                            .collect(),
                    })
                    .collect(),
            }
        } else {
            UntestedResponse {
                spec,
                impl_name,
                total_rules: 0,
                untested_count: 0,
                by_section: vec![],
            }
        }
    }

    /// Get unmapped code
    async fn unmapped(&self, req: UnmappedRequest) -> UnmappedResponse {
        let data = self.engine.data().await;
        let query = QueryEngine::new(&data);

        let (spec, impl_name) =
            self.resolve_spec_impl(req.spec.as_deref(), req.impl_name.as_deref(), &data.config);

        if let Some(result) = query.unmapped(&spec, &impl_name, req.path.as_deref()) {
            // Convert tree nodes to flat entries
            let mut entries = Vec::new();
            fn flatten_tree(node: &crate::server::FileTreeNode, entries: &mut Vec<UnmappedEntry>) {
                entries.push(UnmappedEntry {
                    path: node.path.clone(),
                    is_dir: node.is_dir,
                    total_units: node.total_units,
                    unmapped_units: node.total_units.saturating_sub(node.covered_units),
                    units: vec![], // Tree nodes don't have unit details
                });
                for child in &node.children {
                    flatten_tree(child, entries);
                }
            }
            for node in &result.tree {
                flatten_tree(node, &mut entries);
            }

            // If we have file details, add those units
            if let Some(details) = &result.file_details {
                // Find the entry for this file and update its units
                if let Some(entry) = entries.iter_mut().find(|e| e.path == details.path) {
                    entry.units = details
                        .units
                        .iter()
                        .filter(|u| !u.is_covered)
                        .map(|u| UnmappedUnit {
                            kind: u.kind.clone(),
                            name: u.name.clone(),
                            start_line: u.start_line,
                            end_line: u.end_line,
                        })
                        .collect();
                }
            }

            UnmappedResponse {
                spec: result.spec,
                impl_name: result.impl_name,
                total_units: result.total_units,
                unmapped_count: result.total_units.saturating_sub(result.covered_units),
                entries,
            }
        } else {
            UnmappedResponse {
                spec,
                impl_name,
                total_units: 0,
                unmapped_count: 0,
                entries: vec![],
            }
        }
    }

    /// Get details for a specific rule
    async fn rule(&self, rule_id: String) -> Option<RuleInfo> {
        let data = self.engine.data().await;
        let query = QueryEngine::new(&data);

        query.rule(&rule_id).map(|info| RuleInfo {
            id: info.id,
            text: info.text,
            html: info.html,
            source_file: info.source_file,
            source_line: info.source_line,
            coverage: info
                .coverage
                .into_iter()
                .map(|c| RuleCoverage {
                    spec: c.spec,
                    impl_name: c.impl_name,
                    impl_refs: c.impl_refs,
                    verify_refs: c.verify_refs,
                })
                .collect(),
        })
    }

    /// Get current configuration
    async fn config(&self) -> ApiConfig {
        let data = self.engine.data().await;
        data.config.clone()
    }

    /// Add an include pattern
    async fn add_include(&self, _req: AddPatternRequest) -> Result<(), ConfigError> {
        // TODO: Implement config modification
        Err(ConfigError {
            message: "Not implemented".to_string(),
        })
    }

    /// Add an exclude pattern
    async fn add_exclude(&self, _req: AddPatternRequest) -> Result<(), ConfigError> {
        // TODO: Implement config modification
        Err(ConfigError {
            message: "Not implemented".to_string(),
        })
    }

    /// VFS: file opened
    async fn vfs_open(&self, path: String, content: String) {
        self.engine
            .vfs_open(std::path::PathBuf::from(path), content)
            .await;
    }

    /// VFS: file changed
    async fn vfs_change(&self, path: String, content: String) {
        self.engine
            .vfs_change(std::path::PathBuf::from(path), content)
            .await;
    }

    /// VFS: file closed
    async fn vfs_close(&self, path: String) {
        self.engine.vfs_close(std::path::PathBuf::from(path)).await;
    }

    /// Force a rebuild
    async fn reload(&self) -> ReloadResponse {
        match self.engine.rebuild().await {
            Ok((version, duration)) => ReloadResponse {
                version,
                rebuild_time_ms: duration.as_millis() as u64,
            },
            Err(e) => {
                tracing::error!("Reload failed: {}", e);
                ReloadResponse {
                    version: self.engine.version(),
                    rebuild_time_ms: 0,
                }
            }
        }
    }

    /// Get current version
    async fn version(&self) -> u64 {
        self.engine.version()
    }

    /// Subscribe to data updates
    async fn subscribe(&self, _updates: Pull<DataUpdate>) {
        // TODO: Implement streaming updates
        // This requires integrating with the engine's watch channel
    }

    /// Get forward traceability data
    async fn forward(&self, spec: String, impl_name: String) -> Option<ApiSpecForward> {
        let data = self.engine.data().await;
        data.forward_by_impl.get(&(spec, impl_name)).cloned()
    }

    /// Get reverse traceability data
    async fn reverse(&self, spec: String, impl_name: String) -> Option<ApiReverseData> {
        let data = self.engine.data().await;
        data.reverse_by_impl.get(&(spec, impl_name)).cloned()
    }

    /// Get file with syntax highlighting
    async fn file(&self, _req: FileRequest) -> Option<ApiFileData> {
        // TODO: Implement file loading with syntax highlighting
        // This requires the highlighter from serve.rs
        None
    }

    /// Get rendered spec content
    async fn spec_content(&self, spec: String, impl_name: String) -> Option<ApiSpecData> {
        let data = self.engine.data().await;
        data.specs_content_by_impl.get(&(spec, impl_name)).cloned()
    }

    /// Search rules and files
    async fn search(&self, query: String, limit: usize) -> Vec<SearchResult> {
        let data = self.engine.data().await;
        data.search_index
            .search(&query, limit)
            .into_iter()
            .map(|r| {
                use crate::search::ResultKind;
                let (kind, text, path) = match r.kind {
                    ResultKind::Rule => ("rule".to_string(), Some(r.content), None),
                    ResultKind::Source => {
                        ("file".to_string(), Some(r.highlighted), Some(r.id.clone()))
                    }
                };
                SearchResult {
                    kind,
                    id: r.id,
                    text,
                    path,
                    score: r.score,
                }
            })
            .collect()
    }

    /// Update a file range
    async fn update_file_range(&self, _req: UpdateFileRangeRequest) -> Result<(), UpdateError> {
        // TODO: Implement file editing
        Err(UpdateError {
            message: "Not implemented".to_string(),
        })
    }

    /// Check if a path is a test file
    async fn is_test_file(&self, path: String) -> bool {
        let data = self.engine.data().await;
        let path = std::path::PathBuf::from(path);
        data.test_files.contains(&path)
    }

    /// Validate the spec and implementation
    ///
    /// r[impl mcp.validation.check]
    async fn validate(&self, req: ValidateRequest) -> ValidationResult {
        let data = self.engine.data().await;

        let (spec, impl_name) =
            self.resolve_spec_impl(req.spec.as_deref(), req.impl_name.as_deref(), &data.config);

        let mut errors = Vec::new();

        // Get all rules for this spec/impl
        if let Some(forward_data) = data.forward_by_impl.get(&(spec.clone(), impl_name.clone())) {
            // Build a map of rule IDs for quick lookup
            let _rule_ids: std::collections::HashSet<_> =
                forward_data.rules.iter().map(|r| r.id.as_str()).collect();

            // Check each rule
            for rule in &forward_data.rules {
                // Check naming convention (dot-separated segments)
                if !is_valid_rule_id(&rule.id) {
                    errors.push(ValidationError {
                        code: ValidationErrorCode::InvalidNaming,
                        message: format!(
                            "Rule ID '{}' doesn't follow naming convention (use dot-separated lowercase segments)",
                            rule.id
                        ),
                        file: rule.source_file.clone(),
                        line: rule.source_line,
                        column: rule.source_column,
                        related_rules: vec![],
                    });
                }

                // Check depends references exist
                for dep_ref in &rule.depends_refs {
                    // Extract rule ID from the file path (this is a simplification)
                    // In a full implementation, we'd track what rule ID each depends ref points to
                    // For now, we just note that depends references exist
                    let _ = dep_ref;
                }
            }

            // Check for circular dependencies
            // Build dependency graph and detect cycles
            let cycles = detect_circular_dependencies(forward_data);
            for cycle in cycles {
                errors.push(ValidationError {
                    code: ValidationErrorCode::CircularDependency,
                    message: format!("Circular dependency detected: {}", cycle.join(" → ")),
                    file: None,
                    line: None,
                    column: None,
                    related_rules: cycle,
                });
            }

            // Check for unknown references in impl/verify comments
            // This would require scanning the source files for references
            // to non-existent rule IDs, which is already done during parsing
        }

        let error_count = errors.len();

        ValidationResult {
            spec,
            impl_name,
            errors,
            warning_count: 0,
            error_count,
        }
    }
}

/// Check if a rule ID follows the naming convention
fn is_valid_rule_id(id: &str) -> bool {
    // Must have at least one segment
    if id.is_empty() {
        return false;
    }

    // Split by dots and check each segment
    for segment in id.split('.') {
        if segment.is_empty() {
            return false;
        }
        // Each segment must contain only lowercase letters, digits, or hyphens
        if !segment
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return false;
        }
        // Segment must start with a letter
        if !segment
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase())
        {
            return false;
        }
    }

    true
}

/// Detect circular dependencies in the rule dependency graph
fn detect_circular_dependencies(forward_data: &ApiSpecForward) -> Vec<Vec<String>> {
    use std::collections::{HashMap, HashSet};

    // Build adjacency list from depends_refs
    // Note: This is a simplified version - in a full implementation,
    // we'd need to track which rule ID each depends ref points to
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();

    for rule in &forward_data.rules {
        // Initialize empty adjacency list for each rule
        graph.entry(rule.id.as_str()).or_default();

        // For now, we can't easily extract dependency targets from depends_refs
        // since they only contain file:line references, not rule IDs.
        // A proper implementation would require parsing the depends comments
        // to extract the target rule IDs.
    }

    // Detect cycles using DFS
    let mut cycles = Vec::new();
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut path = Vec::new();

    fn dfs<'a>(
        node: &'a str,
        graph: &HashMap<&'a str, Vec<&'a str>>,
        visited: &mut HashSet<&'a str>,
        rec_stack: &mut HashSet<&'a str>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node);
        rec_stack.insert(node);
        path.push(node.to_string());

        if let Some(neighbors) = graph.get(node) {
            for &neighbor in neighbors {
                if !visited.contains(neighbor) {
                    dfs(neighbor, graph, visited, rec_stack, path, cycles);
                } else if rec_stack.contains(neighbor) {
                    // Found a cycle
                    let cycle_start = path.iter().position(|n| n == neighbor).unwrap();
                    let mut cycle: Vec<String> = path[cycle_start..].to_vec();
                    cycle.push(neighbor.to_string());
                    cycles.push(cycle);
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
    }

    for &node in graph.keys() {
        if !visited.contains(node) {
            dfs(
                node,
                &graph,
                &mut visited,
                &mut rec_stack,
                &mut path,
                &mut cycles,
            );
        }
    }

    cycles
}

/// Dispatcher that wraps TraceyService and implements roam-tcp's ServiceDispatcher.
#[allow(dead_code)]
pub struct TraceyDispatcher {
    service: Arc<TraceyService>,
}

#[allow(dead_code)]
impl TraceyDispatcher {
    pub fn new(service: Arc<TraceyService>) -> Self {
        Self { service }
    }
}

impl roam_tcp::ServiceDispatcher for TraceyDispatcher {
    async fn dispatch_unary(&self, method_id: u64, payload: &[u8]) -> Result<Vec<u8>, String> {
        tracey_daemon_dispatch_unary(&*self.service, method_id, payload)
            .await
            .map_err(|e| format!("Dispatch error: {:?}", e))
    }
}
