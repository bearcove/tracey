//! Rules manifest generation for tracey.
//!
//! This module handles the generation of `_rules.json` manifest files
//! that map rule IDs to their URLs and metadata.

use std::collections::BTreeMap;

use bearmark::RuleDefinition;
use tracey_core::RuleInfo;

/// The rules manifest - maps rule IDs to their info.
pub struct RulesManifest {
    /// Map from rule ID to rule info
    pub rules: BTreeMap<String, RuleInfo>,
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
    pub fn from_rules(rules: &[RuleDefinition], base_url: &str, source_file: &str) -> Self {
        let mut manifest = Self::new();
        for rule in rules {
            let url = format!("{}#{}", base_url, rule.anchor_id);
            manifest.rules.insert(
                rule.id.clone(),
                RuleInfo {
                    def: rule.clone(),
                    url,
                    source_file: Some(source_file.to_string()),
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
}

impl Default for RulesManifest {
    fn default() -> Self {
        Self::new()
    }
}
