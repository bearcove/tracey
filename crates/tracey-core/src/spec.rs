//! Spec manifest loading and parsing

use eyre::{Result, WrapErr};
use facet::Facet;
use std::collections::HashMap;
use std::path::Path;

/// A rule definition from the spec manifest
#[derive(Debug, Clone, Facet)]
pub struct RuleInfo {
    /// URL fragment to link to this rule
    pub url: String,
}

/// The spec manifest structure (from _rules.json)
#[derive(Debug, Clone, Facet)]
pub struct SpecManifest {
    /// Map of rule IDs to their info
    pub rules: HashMap<String, RuleInfo>,
}

impl SpecManifest {
    /// Parse a spec manifest from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        facet_json::from_str(json).wrap_err("Failed to parse spec manifest JSON")
    }

    /// Load a spec manifest from a local file
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("Failed to read spec manifest from {}", path.display()))?;
        Self::from_json(&content)
            .wrap_err_with(|| format!("Failed to parse spec manifest from {}", path.display()))
    }

    /// Fetch a spec manifest from a URL
    #[cfg(feature = "fetch")]
    pub fn fetch(url: &str) -> Result<Self> {
        let mut response = ureq::get(url)
            .call()
            .wrap_err_with(|| format!("Failed to fetch spec manifest from {}", url))?;

        let body = response
            .body_mut()
            .read_to_string()
            .wrap_err_with(|| format!("Failed to read response body from {}", url))?;

        Self::from_json(&body)
            .wrap_err_with(|| format!("Failed to parse spec manifest from {}", url))
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
    pub fn get_rule_url(&self, id: &str) -> Option<&str> {
        self.rules.get(id).map(|r| r.url.as_str())
    }

    /// Number of rules in this manifest
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Whether this manifest has no rules
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}
