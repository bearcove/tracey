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
    pub fn uncovered(&self, spec: &str, impl_name: &str) -> Option<UncoveredResult> {
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
        let by_section = group_rules_by_section(&uncovered_rules, &spec_data.outline);

        Some(UncoveredResult {
            spec: spec.to_string(),
            impl_name: impl_name.to_string(),
            stats,
            by_section,
            total_uncovered: uncovered_rules.len(),
        })
    }

    /// Get untested rules (have impl but no verify refs) for a spec/impl
    pub fn untested(&self, spec: &str, impl_name: &str) -> Option<UntestedResult> {
        let key: ImplKey = (spec.to_string(), impl_name.to_string());
        let forward = self.data.forward_by_impl.get(&key)?;
        let spec_data = self.data.specs_content_by_impl.get(&key)?;

        let stats = CoverageStats::from_rules(&forward.rules);

        let untested_rules: Vec<&ApiRule> = forward
            .rules
            .iter()
            .filter(|r| !r.impl_refs.is_empty() && r.verify_refs.is_empty())
            .collect();

        let by_section = group_rules_by_section(&untested_rules, &spec_data.outline);

        Some(UntestedResult {
            spec: spec.to_string(),
            impl_name: impl_name.to_string(),
            stats,
            by_section,
            total_untested: untested_rules.len(),
        })
    }

    /// Get unmapped code tree for a spec/impl
    pub fn unmapped(&self, spec: &str, impl_name: &str) -> Option<UnmappedResult> {
        let key: ImplKey = (spec.to_string(), impl_name.to_string());
        let reverse = self.data.reverse_by_impl.get(&key)?;

        // Build tree from flat file list
        let tree = build_file_tree(&reverse.files);

        Some(UnmappedResult {
            spec: spec.to_string(),
            impl_name: impl_name.to_string(),
            total_units: reverse.total_units,
            covered_units: reverse.covered_units,
            tree,
        })
    }

    /// Get a specific rule by ID
    pub fn rule(&self, rule_id: &str) -> Option<RuleInfo> {
        // Search across all impls for the rule
        for (key, forward) in &self.data.forward_by_impl {
            if let Some(rule) = forward.rules.iter().find(|r| r.id == rule_id) {
                return Some(RuleInfo {
                    id: rule.id.clone(),
                    html: rule.html.clone(),
                    source_file: rule.source_file.clone(),
                    source_line: rule.source_line,
                    status: rule.status.clone(),
                    level: rule.level.clone(),
                    impl_refs: rule.impl_refs.clone(),
                    verify_refs: rule.verify_refs.clone(),
                    spec: key.0.clone(),
                    impl_name: key.1.clone(),
                });
            }
        }
        None
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

#[derive(Debug, Clone)]
pub struct RuleInfo {
    pub id: String,
    pub html: String,
    pub source_file: Option<String>,
    pub source_line: Option<usize>,
    pub status: Option<String>,
    pub level: Option<String>,
    pub impl_refs: Vec<ApiCodeRef>,
    pub verify_refs: Vec<ApiCodeRef>,
    pub spec: String,
    pub impl_name: String,
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
// [impl mcp.response.header]
// [impl mcp.response.header-format]
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
// [impl mcp.response.delta]
// [impl mcp.response.delta-format]
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
    // [impl mcp.response.text]
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

        for (section, rules) in &self.by_section {
            out.push_str(&format!("## {} ({} uncovered)\n", section, rules.len()));
            for rule in rules {
                out.push_str(&format!("  {}\n", rule.id));
            }
            out.push('\n');
        }

        out.push_str("---\n→ tracey_rule <id> to see rule details\n");

        out
    }
}

impl UntestedResult {
    /// Format as text for MCP response
    // [impl mcp.response.text]
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

        out.push_str("---\n→ tracey_rule <id> to see where rule is implemented\n");

        out
    }
}

impl UnmappedResult {
    /// Format as ASCII tree for MCP response
    // [impl mcp.response.text]
    pub fn format_tree(&self) -> String {
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
        out.push_str(&format!(
            "Overall: {:.0}% ({}/{} code units mapped to requirements)\n\n",
            overall_percent, self.covered_units, self.total_units
        ));

        for (i, node) in self.tree.iter().enumerate() {
            format_tree_node(node, "", i == self.tree.len() - 1, &mut out);
        }

        out.push_str("\n---\n→ tracey_unmapped <path> to zoom into a directory\n");

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
    // [impl mcp.response.text]
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

        out.push_str("\n## Implementations\n");
        if self.impl_refs.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for r in &self.impl_refs {
                out.push_str(&format!("  {}:{}\n", r.file, r.line));
            }
        }

        out.push_str("\n## Verifications\n");
        if self.verify_refs.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for r in &self.verify_refs {
                out.push_str(&format!("  {}:{}\n", r.file, r.line));
            }
        }

        // Strip HTML tags from rule text for display
        let text = strip_html(&self.html);
        out.push_str(&format!("\n## Rule Text\n{}\n", text));

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
