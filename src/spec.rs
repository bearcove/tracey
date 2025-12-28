//! Spec manifest fetching and parsing
//!
//! Fetches `_rules.json` from a spec URL or loads from a local file,
//! and parses the rule definitions.

use eyre::{Result, WrapErr};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// A rule definition from the spec manifest
#[derive(Debug, Clone, Deserialize)]
pub struct RuleInfo {
    /// URL fragment to link to this rule
    #[allow(dead_code)]
    pub url: String,
}

/// The spec manifest structure (from _rules.json)
#[derive(Debug, Clone, Deserialize)]
pub struct SpecManifest {
    /// Map of rule IDs to their info
    pub rules: HashMap<String, RuleInfo>,
}

impl SpecManifest {
    /// Fetch a spec manifest from a URL
    pub fn fetch(url: &str) -> Result<Self> {
        let mut response = ureq::get(url)
            .call()
            .wrap_err_with(|| format!("Failed to fetch spec manifest from {}", url))?;

        let manifest: SpecManifest = response
            .body_mut()
            .read_json()
            .wrap_err_with(|| format!("Failed to parse spec manifest from {}", url))?;

        Ok(manifest)
    }

    /// Load a spec manifest from a local file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("Failed to read spec manifest from {}", path.display()))?;

        let manifest: SpecManifest = serde_json::from_str(&content)
            .wrap_err_with(|| format!("Failed to parse spec manifest from {}", path.display()))?;

        Ok(manifest)
    }

    /// Get the set of all rule IDs in this manifest
    pub fn rule_ids(&self) -> impl Iterator<Item = &str> {
        self.rules.keys().map(|s| s.as_str())
    }

    /// Check if a rule ID exists in this manifest
    pub fn has_rule(&self, id: &str) -> bool {
        self.rules.contains_key(id)
    }

    /// Get the URL for a rule
    #[allow(dead_code)]
    pub fn get_rule_url(&self, id: &str) -> Option<&str> {
        self.rules.get(id).map(|r| r.url.as_str())
    }
}
