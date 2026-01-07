//! Headless server core shared between HTTP and MCP modes
//!
//! This module provides:
//! - Delta tracking between rebuilds
//! - Query interface for coverage data
//! - Text/markdown formatting for MCP responses
//!
//! The actual data building happens in `serve.rs`. This module wraps that
//! data and provides query methods + formatting.

#![allow(dead_code)] // TODO: Remove once wired up

use std::collections::BTreeMap;

use crate::serve::{ApiCodeRef, ApiFileEntry, ApiRule, DashboardData, ImplKey, OutlineEntry};

// ============================================================================
// Delta Tracking
// ============================================================================

/// A rule that changed coverage status
#[derive(Debug, Clone)]
pub struct CoverageChange {
    /// The rule ID
    pub rule_id: String,
    /// Where the reference was added (if newly covered)
    pub file: String,
    /// Line number
    pub line: usize,
    /// Type of reference (impl, verify)
    pub ref_type: String,
}

/// Coverage statistics for a spec/impl pair
#[derive(Debug, Clone, Default)]
pub struct CoverageStats {
    pub total_rules: usize,
    pub impl_covered: usize,
    pub verify_covered: usize,
    pub fully_covered: usize, // both impl and verify
    pub impl_percent: f64,
    pub verify_percent: f64,
}

impl CoverageStats {
    pub fn from_rules(rules: &[ApiRule]) -> Self {
        let total = rules.len();
        let impl_covered = rules.iter().filter(|r| !r.impl_refs.is_empty()).count();
        let verify_covered = rules.iter().filter(|r| !r.verify_refs.is_empty()).count();
        let fully_covered = rules
            .iter()
            .filter(|r| !r.impl_refs.is_empty() && !r.verify_refs.is_empty())
            .count();

        Self {
            total_rules: total,
            impl_covered,
            verify_covered,
            fully_covered,
            impl_percent: if total > 0 {
                (impl_covered as f64 / total as f64) * 100.0
            } else {
                0.0
            },
            verify_percent: if total > 0 {
                (verify_covered as f64 / total as f64) * 100.0
            } else {
                0.0
            },
        }
    }
}

/// Delta for a single spec/impl pair
#[derive(Debug, Clone, Default)]
pub struct ImplDelta {
    /// Rules that became covered (had no refs, now have refs)
    pub newly_covered: Vec<CoverageChange>,
    /// Rules that lost coverage (had refs, now have none)
    pub newly_uncovered: Vec<String>,
    /// Previous stats
    pub prev_stats: CoverageStats,
    /// Current stats
    pub curr_stats: CoverageStats,
}

impl ImplDelta {
    pub fn is_empty(&self) -> bool {
        self.newly_covered.is_empty() && self.newly_uncovered.is_empty()
    }

    pub fn coverage_change(&self) -> f64 {
        self.curr_stats.impl_percent - self.prev_stats.impl_percent
    }
}

/// Delta across all spec/impl pairs since last rebuild
#[derive(Debug, Clone, Default)]
pub struct Delta {
    /// Changes keyed by "spec/impl"
    pub by_impl: BTreeMap<String, ImplDelta>,
}

impl Delta {
    pub fn is_empty(&self) -> bool {
        self.by_impl.values().all(|d| d.is_empty())
    }

    /// Compute delta between old and new data
    pub fn compute(old: &DashboardData, new: &DashboardData) -> Self {
        let mut by_impl = BTreeMap::new();

        for (key, new_forward) in &new.forward_by_impl {
            let impl_key = format!("{}/{}", key.0, key.1);

            let old_forward = old.forward_by_impl.get(key);
            let old_rules: BTreeMap<&str, &ApiRule> = old_forward
                .map(|f| f.rules.iter().map(|r| (r.id.as_str(), r)).collect())
                .unwrap_or_default();

            let mut newly_covered = Vec::new();
            let mut newly_uncovered = Vec::new();

            for new_rule in &new_forward.rules {
                let old_rule = old_rules.get(new_rule.id.as_str());

                let was_impl_covered = old_rule.is_some_and(|r| !r.impl_refs.is_empty());
                let is_impl_covered = !new_rule.impl_refs.is_empty();

                let was_verify_covered = old_rule.is_some_and(|r| !r.verify_refs.is_empty());
                let is_verify_covered = !new_rule.verify_refs.is_empty();

                // Check for newly covered (impl)
                if !was_impl_covered
                    && is_impl_covered
                    && let Some(r) = new_rule.impl_refs.first()
                {
                    newly_covered.push(CoverageChange {
                        rule_id: new_rule.id.clone(),
                        file: r.file.clone(),
                        line: r.line,
                        ref_type: "impl".to_string(),
                    });
                }

                // Check for newly covered (verify)
                if !was_verify_covered
                    && is_verify_covered
                    && let Some(r) = new_rule.verify_refs.first()
                {
                    newly_covered.push(CoverageChange {
                        rule_id: new_rule.id.clone(),
                        file: r.file.clone(),
                        line: r.line,
                        ref_type: "verify".to_string(),
                    });
                }

                // Check for coverage lost
                if was_impl_covered && !is_impl_covered {
                    newly_uncovered.push(new_rule.id.clone());
                }
            }

            let prev_stats = old_forward
                .map(|f| CoverageStats::from_rules(&f.rules))
                .unwrap_or_default();
            let curr_stats = CoverageStats::from_rules(&new_forward.rules);

            by_impl.insert(
                impl_key,
                ImplDelta {
                    newly_covered,
                    newly_uncovered,
                    prev_stats,
                    curr_stats,
                },
            );
        }

        Delta { by_impl }
    }

    /// Format as a summary string for display
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "(no changes)".to_string();
        }

        let mut parts = Vec::new();
        for (key, delta) in &self.by_impl {
            if !delta.is_empty() {
                let covered = delta.newly_covered.len();
                let uncovered = delta.newly_uncovered.len();
                let change = delta.coverage_change();
                let sign = if change >= 0.0 { "+" } else { "" };
                parts.push(format!(
                    "{}: {}{:.1}% ({} newly covered, {} lost)",
                    key, sign, change, covered, uncovered
                ));
            }
        }
        parts.join("; ")
    }
}

// ============================================================================
// Query Interface
// ============================================================================

/// Provides query methods over DashboardData
pub struct QueryEngine<'a> {
    data: &'a DashboardData,
}

impl<'a> QueryEngine<'a> {
    pub fn new(data: &'a DashboardData) -> Self {
        Self { data }
    }

    /// Get coverage stats for all spec/impl pairs
    pub fn status(&self) -> Vec<(String, String, CoverageStats)> {
        self.data
            .forward_by_impl
            .iter()
            .map(|(key, forward)| {
                let stats = CoverageStats::from_rules(&forward.rules);
                (key.0.clone(), key.1.clone(), stats)
            })
            .collect()
    }

    /// Get uncovered rules (no impl refs) for a spec/impl
    // r[impl mcp.discovery.pagination] - Section filtering provides pagination
    // r[impl validation.orphaned]
    pub fn uncovered(
        &self,
        spec: &str,
        impl_name: &str,
        section_filter: Option<&str>,
    ) -> Option<UncoveredResult> {
        let key: ImplKey = (spec.to_string(), impl_name.to_string());
        let forward = self.data.forward_by_impl.get(&key)?;
        let spec_data = self.data.specs_content_by_impl.get(&key)?;

        let stats = CoverageStats::from_rules(&forward.rules);

        // Group uncovered rules by section using the outline
        let uncovered_rules: Vec<&ApiRule> = forward
            .rules
            .iter()
            .filter(|r| r.impl_refs.is_empty())
            .collect();

        // Build section mapping from outline
        let mut by_section = group_rules_by_section(&uncovered_rules, &spec_data.outline);

        // Filter by section if specified
        if let Some(filter) = section_filter {
            by_section.retain(|section, _| section == filter);
        }

        Some(UncoveredResult {
            spec: spec.to_string(),
            impl_name: impl_name.to_string(),
            stats,
            by_section,
            total_uncovered: uncovered_rules.len(),
        })
    }

    /// Get untested rules (have impl but no verify refs) for a spec/impl
    // r[impl mcp.discovery.pagination] - Section filtering provides pagination
    pub fn untested(
        &self,
        spec: &str,
        impl_name: &str,
        section_filter: Option<&str>,
    ) -> Option<UntestedResult> {
        let key: ImplKey = (spec.to_string(), impl_name.to_string());
        let forward = self.data.forward_by_impl.get(&key)?;
        let spec_data = self.data.specs_content_by_impl.get(&key)?;

        let stats = CoverageStats::from_rules(&forward.rules);

        let untested_rules: Vec<&ApiRule> = forward
            .rules
            .iter()
            .filter(|r| !r.impl_refs.is_empty() && r.verify_refs.is_empty())
            .collect();

        let mut by_section = group_rules_by_section(&untested_rules, &spec_data.outline);

        // Filter by section if specified
        if let Some(filter) = section_filter {
            by_section.retain(|section, _| section == filter);
        }

        Some(UntestedResult {
            spec: spec.to_string(),
            impl_name: impl_name.to_string(),
            stats,
            by_section,
            total_untested: untested_rules.len(),
        })
    }

    /// Get unmapped code tree for a spec/impl, optionally filtered by path
    pub fn unmapped(
        &self,
        spec: &str,
        impl_name: &str,
        path: Option<&str>,
    ) -> Option<UnmappedResult> {
        let key: ImplKey = (spec.to_string(), impl_name.to_string());
        let reverse = self.data.reverse_by_impl.get(&key)?;

        // Check if path points to a specific file
        let file_details = if let Some(filter_path) = path {
            // Check if this exact path exists as a file
            let matching_file = reverse.files.iter().find(|f| f.path == filter_path);

            if matching_file.is_some() {
                // Get code units for this file
                // The code_units_by_impl map uses absolute paths, so we need to find
                // the full path that ends with our relative path
                let code_units_by_file = self.data.code_units_by_impl.get(&key)?;

                // Find the absolute path that ends with the relative path
                let abs_path = code_units_by_file
                    .keys()
                    .find(|p| p.ends_with(filter_path))?;

                if let Some(units) = code_units_by_file.get(abs_path) {
                    let unit_infos: Vec<CodeUnitInfo> = units
                        .iter()
                        .map(|u| CodeUnitInfo {
                            kind: format!("{:?}", u.kind).to_lowercase(),
                            name: u.name.clone(),
                            start_line: u.start_line,
                            end_line: u.end_line,
                            is_covered: !u.req_refs.is_empty(),
                        })
                        .collect();

                    Some(FileDetails {
                        path: filter_path.to_string(),
                        units: unit_infos,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // If we're showing file details, don't build a tree
        if file_details.is_some() {
            return Some(UnmappedResult {
                spec: spec.to_string(),
                impl_name: impl_name.to_string(),
                total_units: 0, // Will be calculated from file_details
                covered_units: 0,
                tree: vec![],
                file_details,
            });
        }

        // Otherwise, filter files to those matching the path prefix
        let filtered_files: Vec<_> = if let Some(filter_path) = path {
            reverse
                .files
                .iter()
                .filter(|f| f.path.starts_with(filter_path))
                .cloned()
                .collect()
        } else {
            reverse.files.clone()
        };

        // Build tree from (possibly filtered) file list
        let tree = build_file_tree(&filtered_files);

        // Recalculate totals for filtered view
        let total_units = filtered_files.iter().map(|f| f.total_units).sum();
        let covered_units = filtered_files.iter().map(|f| f.covered_units).sum();

        Some(UnmappedResult {
            spec: spec.to_string(),
            impl_name: impl_name.to_string(),
            total_units,
            covered_units,
            tree,
            file_details: None,
        })
    }

    /// Get a specific rule by ID
    // r[impl mcp.tool.req]
    // r[impl mcp.tool.req.all-impls]
    pub fn rule(&self, rule_id: &str) -> Option<RuleInfo> {
        // Collect coverage from all impls for this rule
        let mut coverage = Vec::new();
        let mut result: Option<RuleInfo> = None;

        for (key, forward) in &self.data.forward_by_impl {
            if let Some(rule) = forward.rules.iter().find(|r| r.id == rule_id) {
                // Capture rule metadata from first match
                if result.is_none() {
                    result = Some(RuleInfo {
                        id: rule_id.to_string(),
                        text: rule.text.clone(),
                        html: rule.html.clone(),
                        source_file: rule.source_file.clone(),
                        source_line: rule.source_line,
                        status: rule.status.clone(),
                        level: rule.level.clone(),
                        coverage: Vec::new(), // Will be set at the end
                    });
                }

                // Add coverage for this impl
                coverage.push(ImplCoverage {
                    spec: key.0.clone(),
                    impl_name: key.1.clone(),
                    impl_refs: rule.impl_refs.clone(),
                    verify_refs: rule.verify_refs.clone(),
                });
            }
        }

        // Set the coverage on the result
        if let Some(ref mut info) = result {
            info.coverage = coverage;
        }

        result
    }
}

// ============================================================================
// Query Results
// ============================================================================

#[derive(Debug, Clone)]
pub struct UncoveredResult {
    pub spec: String,
    pub impl_name: String,
    pub stats: CoverageStats,
    pub by_section: BTreeMap<String, Vec<RuleRef>>,
    pub total_uncovered: usize,
}

#[derive(Debug, Clone)]
pub struct UntestedResult {
    pub spec: String,
    pub impl_name: String,
    pub stats: CoverageStats,
    pub by_section: BTreeMap<String, Vec<RuleRef>>,
    pub total_untested: usize,
}

#[derive(Debug, Clone)]
pub struct RuleRef {
    pub id: String,
    pub impl_refs: Vec<ApiCodeRef>,
}

#[derive(Debug, Clone)]
pub struct UnmappedResult {
    pub spec: String,
    pub impl_name: String,
    pub total_units: usize,
    pub covered_units: usize,
    pub tree: Vec<FileTreeNode>,
    pub file_details: Option<FileDetails>,
}

#[derive(Debug, Clone)]
pub struct FileDetails {
    pub path: String,
    pub units: Vec<CodeUnitInfo>,
}

#[derive(Debug, Clone)]
pub struct CodeUnitInfo {
    pub kind: String,
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub is_covered: bool,
}

#[derive(Debug, Clone)]
pub struct FileTreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub total_units: usize,
    pub covered_units: usize,
    pub children: Vec<FileTreeNode>,
}

impl FileTreeNode {
    pub fn coverage_percent(&self) -> f64 {
        if self.total_units == 0 {
            100.0
        } else {
            (self.covered_units as f64 / self.total_units as f64) * 100.0
        }
    }
}

/// Coverage information for a rule in a specific implementation
#[derive(Debug, Clone)]
pub struct ImplCoverage {
    pub spec: String,
    pub impl_name: String,
    pub impl_refs: Vec<ApiCodeRef>,
    pub verify_refs: Vec<ApiCodeRef>,
}

#[derive(Debug, Clone)]
pub struct RuleInfo {
    pub id: String,
    pub text: String,
    pub html: String,
    pub source_file: Option<String>,
    pub source_line: Option<usize>,
    pub status: Option<String>,
    pub level: Option<String>,
    /// Coverage across all implementations
    pub coverage: Vec<ImplCoverage>,
}

impl RuleInfo {
    /// Check if this rule has any implementation references across all impls
    pub fn has_any_impl(&self) -> bool {
        self.coverage.iter().any(|c| !c.impl_refs.is_empty())
    }

    /// Check if this rule has any verification references across all impls
    pub fn has_any_verify(&self) -> bool {
        self.coverage.iter().any(|c| !c.verify_refs.is_empty())
    }

    /// Get all impl refs across all implementations
    pub fn all_impl_refs(&self) -> Vec<&ApiCodeRef> {
        self.coverage.iter().flat_map(|c| &c.impl_refs).collect()
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn group_rules_by_section(
    rules: &[&ApiRule],
    _outline: &[OutlineEntry],
) -> BTreeMap<String, Vec<RuleRef>> {
    // For now, just group all under "All Rules"
    // TODO: Use outline to determine which section each rule belongs to
    let mut result = BTreeMap::new();

    if !rules.is_empty() {
        let refs: Vec<RuleRef> = rules
            .iter()
            .map(|r| RuleRef {
                id: r.id.clone(),
                impl_refs: r.impl_refs.clone(),
            })
            .collect();
        result.insert("All Rules".to_string(), refs);
    }

    result
}

fn build_file_tree(files: &[ApiFileEntry]) -> Vec<FileTreeNode> {
    // Build a tree structure from flat file paths
    let mut root_children: BTreeMap<String, FileTreeNode> = BTreeMap::new();

    for file in files {
        let parts: Vec<&str> = file.path.split('/').collect();
        insert_into_tree(&mut root_children, &parts, file);
    }

    root_children.into_values().collect()
}

fn insert_into_tree(
    children: &mut BTreeMap<String, FileTreeNode>,
    parts: &[&str],
    file: &ApiFileEntry,
) {
    if parts.is_empty() {
        return;
    }

    let name = parts[0].to_string();
    let is_leaf = parts.len() == 1;

    let node = children
        .entry(name.clone())
        .or_insert_with(|| FileTreeNode {
            name: name.clone(),
            path: if is_leaf {
                file.path.clone()
            } else {
                parts[0].to_string()
            },
            is_dir: !is_leaf,
            total_units: 0,
            covered_units: 0,
            children: Vec::new(),
        });

    if is_leaf {
        node.total_units = file.total_units;
        node.covered_units = file.covered_units;
    } else {
        // Recurse and accumulate stats
        let mut child_map: BTreeMap<String, FileTreeNode> = node
            .children
            .drain(..)
            .map(|n| (n.name.clone(), n))
            .collect();

        insert_into_tree(&mut child_map, &parts[1..], file);

        node.children = child_map.into_values().collect();

        // Accumulate stats from children
        node.total_units = node.children.iter().map(|c| c.total_units).sum();
        node.covered_units = node.children.iter().map(|c| c.covered_units).sum();
    }
}

// ============================================================================
// MCP Text Formatting
// ============================================================================

/// Format status header for MCP responses
// r[impl mcp.response.header]
// r[impl mcp.response.header-format]
pub fn format_status_header(data: &DashboardData, delta: &Delta) -> String {
    let status_parts: Vec<String> = data
        .forward_by_impl
        .iter()
        .map(|(key, forward)| {
            let stats = CoverageStats::from_rules(&forward.rules);
            let impl_key = format!("{}/{}", key.0, key.1);

            // Check if there's a delta for this impl
            let change_str = if let Some(impl_delta) = delta.by_impl.get(&impl_key) {
                let change = impl_delta.coverage_change();
                if change.abs() > 0.1 {
                    let sign = if change >= 0.0 { "+" } else { "" };
                    format!(" ({}{:.1}%)", sign, change)
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            format!("{}: {:.0}%{}", impl_key, stats.impl_percent, change_str)
        })
        .collect();

    format!("tracey | {}", status_parts.join(" | "))
}

/// Format delta section for MCP responses
// r[impl mcp.response.delta]
// r[impl mcp.response.delta-format]
pub fn format_delta_section(delta: &Delta) -> String {
    if delta.is_empty() {
        return "(no changes since last query)\n".to_string();
    }

    let mut out = String::new();
    out.push_str("Since last rebuild:\n");

    for impl_delta in delta.by_impl.values() {
        if !impl_delta.is_empty() {
            for change in &impl_delta.newly_covered {
                out.push_str(&format!(
                    "  ✓ {} → {}:{} ({})\n",
                    change.rule_id, change.file, change.line, change.ref_type
                ));
            }
            for rule_id in &impl_delta.newly_uncovered {
                out.push_str(&format!("  ✗ {} (coverage lost)\n", rule_id));
            }
        }
    }

    out
}

impl UncoveredResult {
    /// Format as text for MCP response
    // r[impl mcp.response.text]
    pub fn format_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "# Uncovered Rules in {}/{}\n\n",
            self.spec, self.impl_name
        ));
        out.push_str(&format!(
            "Implementation coverage: {:.0}% ({}/{} rules)\n\n",
            self.stats.impl_percent, self.stats.impl_covered, self.stats.total_rules
        ));

        if self.total_uncovered == 0 {
            out.push_str("All rules have implementation references! 🎉\n");
            return out;
        }

        // r[impl mcp.discovery.overview-first] - Show sections with counts
        for (section, rules) in &self.by_section {
            out.push_str(&format!("## {} ({} uncovered)\n", section, rules.len()));
            for rule in rules {
                out.push_str(&format!("  {}\n", rule.id));
            }
            out.push('\n');
        }

        // r[impl mcp.discovery.drill-down] - Provide hints for drilling down
        out.push_str("---\n→ Use mcp__tracey__tracey_rule to see rule details\n");

        out
    }
}

impl UntestedResult {
    /// Format as text for MCP response
    // r[impl mcp.response.text]
    pub fn format_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "# Untested Rules in {}/{}\n\n",
            self.spec, self.impl_name
        ));
        out.push_str(&format!(
            "Verification coverage: {:.0}% ({}/{} rules)\n\n",
            self.stats.verify_percent, self.stats.verify_covered, self.stats.total_rules
        ));

        if self.total_untested == 0 {
            out.push_str("All implemented rules have verification! 🎉\n");
            return out;
        }

        // r[impl mcp.discovery.overview-first] - Show sections with counts
        for (section, rules) in &self.by_section {
            out.push_str(&format!("## {} ({} untested)\n", section, rules.len()));
            for rule in rules {
                out.push_str(&format!("  {}", rule.id));
                if !rule.impl_refs.is_empty() {
                    let loc = &rule.impl_refs[0];
                    out.push_str(&format!(" (impl: {}:{})", loc.file, loc.line));
                }
                out.push('\n');
            }
            out.push('\n');
        }

        // r[impl mcp.discovery.drill-down] - Provide hints for drilling down
        out.push_str("---\n→ Use mcp__tracey__tracey_rule to see where rule is implemented\n");

        out
    }
}

impl UnmappedResult {
    /// Format output for MCP response (either tree or file details)
    // r[impl mcp.response.text]
    pub fn format_output(&self) -> String {
        // If we have file details, format them
        if let Some(ref details) = self.file_details {
            return self.format_file_details(details);
        }

        // Otherwise, format as tree
        self.format_tree()
    }

    fn format_tree(&self) -> String {
        let mut out = String::new();
        let overall_percent = if self.total_units > 0 {
            (self.covered_units as f64 / self.total_units as f64) * 100.0
        } else {
            100.0
        };

        out.push_str(&format!(
            "# Code Traceability for {}/{}\n\n",
            self.spec, self.impl_name
        ));
        // r[impl mcp.discovery.overview-first] - Show overall stats first
        out.push_str(&format!(
            "Overall: {:.0}% ({}/{} code units mapped to requirements)\n\n",
            overall_percent, self.covered_units, self.total_units
        ));

        for (i, node) in self.tree.iter().enumerate() {
            format_tree_node(node, "", i == self.tree.len() - 1, &mut out);
        }

        // r[impl mcp.discovery.drill-down] - Provide hints for drilling down
        out.push_str("\n---\n→ Use mcp__tracey__tracey_unmapped to zoom into a directory\n");

        out
    }

    fn format_file_details(&self, details: &FileDetails) -> String {
        let mut out = String::new();

        let total = details.units.len();
        let covered = details.units.iter().filter(|u| u.is_covered).count();
        let percent = if total > 0 {
            (covered as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        out.push_str(&format!("# Code Units in {}\n\n", details.path));
        out.push_str(&format!(
            "Coverage: {:.0}% ({}/{} units mapped to requirements)\n\n",
            percent, covered, total
        ));

        // List unmapped units first
        let unmapped: Vec<_> = details.units.iter().filter(|u| !u.is_covered).collect();
        if !unmapped.is_empty() {
            out.push_str("## Unmapped Units\n\n");
            for unit in unmapped {
                let name_part = unit
                    .name
                    .as_ref()
                    .map(|n| format!(" {}", n))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- {}:{}-{} {}{}\n",
                    details.path, unit.start_line, unit.end_line, unit.kind, name_part
                ));
            }
            out.push('\n');
        }

        // Then list covered units
        let covered_units: Vec<_> = details.units.iter().filter(|u| u.is_covered).collect();
        if !covered_units.is_empty() {
            out.push_str("## Mapped Units\n\n");
            for unit in covered_units {
                let name_part = unit
                    .name
                    .as_ref()
                    .map(|n| format!(" {}", n))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- {}:{}-{} {}{}\n",
                    details.path, unit.start_line, unit.end_line, unit.kind, name_part
                ));
            }
            out.push('\n');
        }

        out
    }
}

fn format_tree_node(node: &FileTreeNode, prefix: &str, is_last: bool, out: &mut String) {
    let connector = if is_last { "└── " } else { "├── " };
    let percent = node.coverage_percent();
    let bar = coverage_bar(percent);

    out.push_str(&format!(
        "{}{}{:<24} {:>3.0}% {}\n",
        prefix, connector, node.name, percent, bar
    ));

    if !node.children.is_empty() {
        let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
        let len = node.children.len();
        for (i, child) in node.children.iter().enumerate() {
            format_tree_node(child, &child_prefix, i == len - 1, out);
        }
    }
}

fn coverage_bar(percent: f64) -> String {
    let filled = (percent / 10.0).round() as usize;
    let empty = 10usize.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

impl RuleInfo {
    /// Format as text for MCP response
    // r[impl mcp.response.text]
    pub fn format_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# Rule: {}\n\n", self.id));

        if let Some(ref file) = self.source_file
            && let Some(line) = self.source_line
        {
            out.push_str(&format!("Defined in: {}:{}\n", file, line));
        }

        if let Some(ref status) = self.status {
            out.push_str(&format!("Status: {}\n", status));
        }
        if let Some(ref level) = self.level {
            out.push_str(&format!("Level: {}\n", level));
        }

        // Show coverage per implementation
        out.push_str("\n## Coverage by Implementation\n\n");

        if self.coverage.is_empty() {
            out.push_str("  (no implementations configured)\n");
        } else {
            for cov in &self.coverage {
                let impl_label = format!("{}/{}", cov.spec, cov.impl_name);
                let has_impl = !cov.impl_refs.is_empty();
                let has_verify = !cov.verify_refs.is_empty();

                let status_icon = match (has_impl, has_verify) {
                    (true, true) => "✓✓",
                    (true, false) => "✓ ",
                    (false, true) => " ✓",
                    (false, false) => "  ",
                };

                out.push_str(&format!("### {} [{}]\n", impl_label, status_icon));

                if cov.impl_refs.is_empty() && cov.verify_refs.is_empty() {
                    out.push_str("  (not covered)\n");
                } else {
                    if !cov.impl_refs.is_empty() {
                        out.push_str("  **Implementations:**\n");
                        for r in &cov.impl_refs {
                            out.push_str(&format!("    - {}:{}\n", r.file, r.line));
                        }
                    }
                    if !cov.verify_refs.is_empty() {
                        out.push_str("  **Verifications:**\n");
                        for r in &cov.verify_refs {
                            out.push_str(&format!("    - {}:{}\n", r.file, r.line));
                        }
                    }
                }
                out.push('\n');
            }
        }

        // Strip HTML tags from rule text for display
        let text = strip_html(&self.html);
        out.push_str(&format!("## Rule Text\n{}\n", text));

        out
    }
}

fn strip_html(html: &str) -> String {
    // Simple HTML stripping - remove tags
    let mut result = String::new();
    let mut in_tag = false;

    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }

    // Decode common entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Create a test fixture with spec, impl files, and config
    async fn create_test_fixture() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        // Create directory structure
        fs::create_dir_all(root.join(".config/tracey")).unwrap();
        fs::create_dir_all(root.join("docs/spec")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();

        // Write spec file with some rules
        let spec_content = r#"# Test Spec

r[foo.bar]
This is the foo.bar rule.

r[foo.baz]
This is the foo.baz rule.

r[uncovered.rule]
This rule has no implementation.
"#;
        fs::write(root.join("docs/spec/spec.md"), spec_content).unwrap();

        // Write impl file with annotations
        let impl_content = r#"// r[impl foo.bar]
fn do_foo_bar() {}

// r[impl foo.baz]
fn do_foo_baz() {}
"#;
        fs::write(root.join("src/lib.rs"), impl_content).unwrap();

        // Write config
        let config_content = r#"spec {
    name "test-spec"
    prefix "r"
    include "docs/spec/**/*.md"

    impl {
        name "main"
        include "src/**/*.rs"
    }
}
"#;
        fs::write(root.join(".config/tracey/config.kdl"), config_content).unwrap();

        (tmp, root)
    }

    #[tokio::test]
    async fn test_status_and_rule_consistency() {
        let (_tmp, root) = create_test_fixture().await;
        let config_path = root.join(".config/tracey/config.kdl");
        let config = crate::load_config(&config_path).unwrap();

        // Build dashboard data
        let data = crate::serve::build_dashboard_data(&root, &config, 1, true)
            .await
            .unwrap();

        let engine = QueryEngine::new(&data);

        // Get status
        let status = engine.status();
        assert_eq!(status.len(), 1, "Should have one spec/impl pair");

        let (spec, impl_name, stats) = &status[0];
        assert_eq!(spec, "test-spec");
        assert_eq!(impl_name, "main");
        assert_eq!(stats.total_rules, 3, "Should have 3 total rules");
        assert_eq!(stats.impl_covered, 2, "Should have 2 covered rules");

        // Now look up each covered rule and verify impl_refs are populated
        let foo_bar = engine.rule("foo.bar").expect("foo.bar should exist");
        assert!(
            foo_bar.has_any_impl(),
            "foo.bar should have impl_refs, got: {:?}",
            foo_bar.coverage
        );

        let foo_baz = engine.rule("foo.baz").expect("foo.baz should exist");
        assert!(
            foo_baz.has_any_impl(),
            "foo.baz should have impl_refs, got: {:?}",
            foo_baz.coverage
        );

        // Uncovered rule should exist but have no impl_refs
        let uncovered = engine
            .rule("uncovered.rule")
            .expect("uncovered.rule should exist");
        assert!(
            !uncovered.has_any_impl(),
            "uncovered.rule should have no impl_refs"
        );
    }

    #[tokio::test]
    async fn test_rule_lookup_finds_covered_rules() {
        let (_tmp, root) = create_test_fixture().await;
        let config_path = root.join(".config/tracey/config.kdl");
        let config = crate::load_config(&config_path).unwrap();

        let data = crate::serve::build_dashboard_data(&root, &config, 1, true)
            .await
            .unwrap();

        let engine = QueryEngine::new(&data);

        // The bug report: status shows coverage, but rule lookup shows none
        // Let's verify they're consistent
        let status = engine.status();
        let (_, _, stats) = &status[0];

        // Count covered rules via status
        let status_covered = stats.impl_covered;

        // Count covered rules via individual lookups
        let mut lookup_covered = 0;
        for rule_id in ["foo.bar", "foo.baz", "uncovered.rule"] {
            if let Some(rule) = engine.rule(rule_id)
                && rule.has_any_impl()
            {
                lookup_covered += 1;
            }
        }

        assert_eq!(
            status_covered, lookup_covered,
            "Status covered count ({}) should match lookup covered count ({})",
            status_covered, lookup_covered
        );
    }

    /// Test with multiple impls to check if rule lookup returns the right one
    #[tokio::test]
    async fn test_multiple_impls() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();

        // Create directory structure
        fs::create_dir_all(root.join(".config/tracey")).unwrap();
        fs::create_dir_all(root.join("docs/spec")).unwrap();
        fs::create_dir_all(root.join("impl-a/src")).unwrap();
        fs::create_dir_all(root.join("impl-b/src")).unwrap();

        // Write spec file
        let spec_content = r#"# Test Spec

r[shared.rule]
This rule is implemented in both impls.

r[only.in.a]
This rule is only in impl-a.

r[only.in.b]
This rule is only in impl-b.
"#;
        fs::write(root.join("docs/spec/spec.md"), spec_content).unwrap();

        // Write impl-a with annotations
        let impl_a_content = r#"// r[impl shared.rule]
fn shared_a() {}

// r[impl only.in.a]
fn only_a() {}
"#;
        fs::write(root.join("impl-a/src/lib.rs"), impl_a_content).unwrap();

        // Write impl-b with annotations
        let impl_b_content = r#"// r[impl shared.rule]
fn shared_b() {}

// r[impl only.in.b]
fn only_b() {}
"#;
        fs::write(root.join("impl-b/src/lib.rs"), impl_b_content).unwrap();

        // Write config with two impls
        let config_content = r#"spec {
    name "test-spec"
    prefix "r"
    include "docs/spec/**/*.md"

    impl {
        name "impl-a"
        include "impl-a/**/*.rs"
    }

    impl {
        name "impl-b"
        include "impl-b/**/*.rs"
    }
}
"#;
        fs::write(root.join(".config/tracey/config.kdl"), config_content).unwrap();

        let config_path = root.join(".config/tracey/config.kdl");
        let config = crate::load_config(&config_path).unwrap();

        let data = crate::serve::build_dashboard_data(&root, &config, 1, true)
            .await
            .unwrap();

        let engine = QueryEngine::new(&data);

        // Get status for each impl
        let status = engine.status();
        assert_eq!(status.len(), 2, "Should have two spec/impl pairs");

        // Check impl-a coverage
        let impl_a_stats = status.iter().find(|(_, name, _)| name == "impl-a").unwrap();
        assert_eq!(
            impl_a_stats.2.impl_covered, 2,
            "impl-a should have 2 covered rules"
        );

        // Check impl-b coverage
        let impl_b_stats = status.iter().find(|(_, name, _)| name == "impl-b").unwrap();
        assert_eq!(
            impl_b_stats.2.impl_covered, 2,
            "impl-b should have 2 covered rules"
        );

        // Now check individual rule lookups
        // rule() now returns coverage for ALL impls, so we can see where it's implemented
        let shared = engine
            .rule("shared.rule")
            .expect("shared.rule should exist");
        // shared.rule is implemented in both impls
        assert!(shared.has_any_impl(), "shared.rule should have impl_refs");
        assert_eq!(
            shared.coverage.len(),
            2,
            "shared.rule should have coverage for 2 impls"
        );

        let only_a = engine.rule("only.in.a").expect("only.in.a should exist");
        // This rule is only in impl-a, but coverage shows both impls
        assert!(only_a.has_any_impl(), "only.in.a should have impl_refs");
        // Verify it's covered in impl-a
        let impl_a_cov = only_a
            .coverage
            .iter()
            .find(|c| c.impl_name == "impl-a")
            .unwrap();
        assert!(
            !impl_a_cov.impl_refs.is_empty(),
            "only.in.a should be covered in impl-a"
        );
        // And not covered in impl-b
        let impl_b_cov = only_a
            .coverage
            .iter()
            .find(|c| c.impl_name == "impl-b")
            .unwrap();
        assert!(
            impl_b_cov.impl_refs.is_empty(),
            "only.in.a should not be covered in impl-b"
        );

        let only_b = engine.rule("only.in.b").expect("only.in.b should exist");
        // This rule is only in impl-b - with the fix, has_any_impl() should be true
        assert!(
            only_b.has_any_impl(),
            "only.in.b should have impl_refs (the bug we fixed!)"
        );
        // Verify it's NOT covered in impl-a
        let impl_a_cov = only_b
            .coverage
            .iter()
            .find(|c| c.impl_name == "impl-a")
            .unwrap();
        assert!(
            impl_a_cov.impl_refs.is_empty(),
            "only.in.b should not be covered in impl-a"
        );
        // And IS covered in impl-b
        let impl_b_cov = only_b
            .coverage
            .iter()
            .find(|c| c.impl_name == "impl-b")
            .unwrap();
        assert!(
            !impl_b_cov.impl_refs.is_empty(),
            "only.in.b should be covered in impl-b"
        );
    }
}
