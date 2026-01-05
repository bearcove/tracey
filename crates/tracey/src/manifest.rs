//! Rules manifest generation for tracey.
//!
//! This module handles the generation of `_rules.json` manifest files
//! that map rule IDs to their URLs and metadata.

use std::collections::BTreeMap;

use bearmark::RuleDefinition;
use facet::Facet;

/// A rule entry in the manifest, with its target URL and metadata.
#[derive(Debug, Clone, Facet)]
pub struct ManifestRuleEntry {
    /// The URL fragment to link to this rule (e.g., "#r-channel.id.allocation")
    pub url: String,
    /// The source file where this rule is defined (relative path)
    #[facet(default)]
    pub source_file: Option<String>,
    /// The line number where this rule is defined (1-indexed)
    #[facet(default)]
    pub source_line: Option<usize>,
    /// The text content of the rule (first paragraph after the marker)
    #[facet(default)]
    pub text: Option<String>,
    /// Lifecycle status (draft, stable, deprecated, removed)
    #[facet(default)]
    pub status: Option<String>,
    /// RFC 2119 requirement level (must, should, may)
    #[facet(default)]
    pub level: Option<String>,
    /// Version when this rule was introduced
    #[facet(default)]
    pub since: Option<String>,
    /// Version when this rule will be/was deprecated or removed
    #[facet(default)]
    pub until: Option<String>,
    /// Custom tags for categorization
    #[facet(default)]
    pub tags: Vec<String>,
}

/// The rules manifest - maps rule IDs to their URLs.
#[derive(Debug, Clone, Facet)]
pub struct RulesManifest {
    /// Map from rule ID to rule entry
    pub rules: BTreeMap<String, ManifestRuleEntry>,
}

/// A duplicate rule ID found across different files.
#[derive(Debug, Clone)]
pub struct DuplicateRule {
    /// The rule ID that was duplicated
    pub id: String,
    /// URL where the rule was first defined
    pub first_url: String,
    /// URL where the duplicate was found
    pub second_url: String,
}

impl RulesManifest {
    /// Create a new empty manifest.
    pub fn new() -> Self {
        Self {
            rules: BTreeMap::new(),
        }
    }

    /// Build a manifest from extracted rules.
    ///
    /// The `base_url` is prepended to the anchor (e.g., "/spec/core" -> "/spec/core#r-rule.id").
    /// The `source_file` is the relative path to the markdown file containing these rules.
    pub fn from_rules(rules: &[RuleDefinition], base_url: &str, source_file: Option<&str>) -> Self {
        let mut manifest = Self::new();
        for rule in rules {
            let url = format!("{}#{}", base_url, rule.anchor_id);
            manifest.rules.insert(
                rule.id.clone(),
                ManifestRuleEntry {
                    url,
                    source_file: source_file.map(|s| s.to_string()),
                    source_line: Some(rule.line),
                    text: if rule.text.is_empty() {
                        None
                    } else {
                        Some(rule.text.clone())
                    },
                    status: rule.metadata.status.map(|s| s.as_str().to_string()),
                    level: rule.metadata.level.map(|l| l.as_str().to_string()),
                    since: rule.metadata.since.clone(),
                    until: rule.metadata.until.clone(),
                    tags: rule.metadata.tags.clone(),
                },
            );
        }
        manifest
    }

    /// Merge another manifest into this one.
    ///
    /// Returns a list of duplicate rule IDs if any conflicts are found.
    pub fn merge(&mut self, other: &RulesManifest) -> Vec<DuplicateRule> {
        let mut duplicates = Vec::new();
        for (id, entry) in &other.rules {
            if let Some(existing) = self.rules.get(id) {
                duplicates.push(DuplicateRule {
                    id: id.clone(),
                    first_url: existing.url.clone(),
                    second_url: entry.url.clone(),
                });
            } else {
                self.rules.insert(id.clone(), entry.clone());
            }
        }
        duplicates
    }

    /// Serialize the manifest to pretty-printed JSON.
    pub fn to_json(&self) -> String {
        facet_json::to_string_pretty(self).expect("RulesManifest should serialize to JSON")
    }
}

impl Default for RulesManifest {
    fn default() -> Self {
        Self::new()
    }
}
