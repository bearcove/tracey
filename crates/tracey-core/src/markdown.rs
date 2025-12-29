//! Markdown preprocessor for extracting rules from spec documents.
//!
//! This module provides functionality to:
//! 1. Parse markdown spec documents and extract rule definitions
//! 2. Generate `_rules.json` manifests from extracted rules
//! 3. Transform markdown with rules replaced by `<div>` elements for rendering
//!
//! # Rule Syntax
//!
//! Rules are defined using the `r[rule.id]` syntax on their own line:
//!
//! ```markdown
//! r[channel.id.allocation]
//! Channel IDs MUST be allocated sequentially starting from 0.
//! ```
//!
//! # Example
//!
//! ```
//! use tracey_core::markdown::{MarkdownProcessor, ProcessedMarkdown};
//!
//! let markdown = r#"
//! # My Spec
//!
//! r[my.rule.id]
//! This is the rule content.
//! "#;
//!
//! let result = MarkdownProcessor::process(markdown).unwrap();
//! assert_eq!(result.rules.len(), 1);
//! assert_eq!(result.rules[0].id, "my.rule.id");
//! ```

use std::collections::{BTreeMap, HashSet};

use eyre::{Result, bail};
use facet::Facet;

use crate::SourceSpan;

/// A rule extracted from a markdown document.
#[derive(Debug, Clone, Facet)]
pub struct MarkdownRule {
    /// The rule identifier (e.g., "channel.id.allocation")
    pub id: String,
    /// The anchor ID for HTML linking (e.g., "r-channel.id.allocation")
    pub anchor_id: String,
    /// Source location of this rule in the original markdown
    pub span: SourceSpan,
}

/// Result of processing a markdown document.
#[derive(Debug, Clone)]
pub struct ProcessedMarkdown {
    /// All rules found in the document
    pub rules: Vec<MarkdownRule>,
    /// Transformed markdown with rule markers replaced by HTML divs
    pub output: String,
}

/// A rule entry in the manifest, with its target URL.
///
/// [impl manifest.format.rule-entry]
#[derive(Debug, Clone, Facet)]
pub struct ManifestRuleEntry {
    /// The URL fragment to link to this rule (e.g., "#r-channel.id.allocation")
    pub url: String,
}

/// The rules manifest - maps rule IDs to their URLs.
///
/// [impl manifest.format.rules-key]
#[derive(Debug, Clone, Facet)]
pub struct RulesManifest {
    /// Map from rule ID to rule entry
    pub rules: BTreeMap<String, ManifestRuleEntry>,
}

impl RulesManifest {
    /// Create a new empty manifest.
    pub fn new() -> Self {
        Self {
            rules: BTreeMap::new(),
        }
    }

    /// Build a manifest from processed markdown rules.
    ///
    /// The `base_url` is prepended to the anchor (e.g., "/spec/core" -> "/spec/core#r-rule.id").
    pub fn from_rules(rules: &[MarkdownRule], base_url: &str) -> Self {
        let mut manifest = Self::new();
        for rule in rules {
            let url = format!("{}#{}", base_url, rule.anchor_id);
            manifest
                .rules
                .insert(rule.id.clone(), ManifestRuleEntry { url });
        }
        manifest
    }

    /// Merge another manifest into this one.
    ///
    /// Returns a list of duplicate rule IDs if any conflicts are found.
    ///
    /// [impl markdown.duplicates.cross-file]
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
    ///
    /// [impl manifest.format.json]
    pub fn to_json(&self) -> String {
        facet_json::to_string_pretty(self)
    }
}

impl Default for RulesManifest {
    fn default() -> Self {
        Self::new()
    }
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

/// Markdown processor for extracting and transforming rule definitions.
pub struct MarkdownProcessor;

impl MarkdownProcessor {
    /// Process markdown content to extract rules and transform the output.
    ///
    /// Rules are lines matching `r[rule.id]` on their own line.
    /// They are replaced with HTML div elements for rendering.
    ///
    /// # Errors
    ///
    /// Returns an error if duplicate rule IDs are found within the same document.
    pub fn process(markdown: &str) -> Result<ProcessedMarkdown> {
        let mut result = String::with_capacity(markdown.len());
        let mut rules = Vec::new();
        let mut seen_rule_ids: HashSet<String> = HashSet::new();

        let mut byte_offset = 0usize;

        for line in markdown.lines() {
            let trimmed = line.trim();
            let line_byte_len = line.len();

            // Check if this line is a rule identifier: r[rule.id]
            // [impl markdown.syntax.marker]
            // [impl markdown.syntax.standalone]
            if trimmed.starts_with("r[") && trimmed.ends_with(']') && trimmed.len() > 3 {
                let rule_id = &trimmed[2..trimmed.len() - 1];

                // Check for duplicates
                // [impl markdown.duplicates.same-file]
                if !seen_rule_ids.insert(rule_id.to_string()) {
                    bail!("duplicate rule identifier: r[{}]", rule_id);
                }

                let anchor_id = format!("r-{}", rule_id);

                // Calculate the span for this rule
                let span = SourceSpan {
                    offset: byte_offset,
                    length: line_byte_len,
                };

                rules.push(MarkdownRule {
                    id: rule_id.to_string(),
                    anchor_id: anchor_id.clone(),
                    span,
                });

                // Emit rule HTML directly
                // Add blank line after to ensure following text becomes a proper paragraph
                result.push_str(&rule_to_html(rule_id, &anchor_id));
                result.push_str("\n\n");
            } else {
                result.push_str(line);
                result.push('\n');
            }

            // Account for the newline character (or end of content)
            byte_offset += line_byte_len + 1;
        }

        Ok(ProcessedMarkdown {
            rules,
            output: result,
        })
    }

    /// Extract only the rules from markdown without transforming the output.
    ///
    /// This is a lighter-weight operation when you only need the rule list.
    pub fn extract_rules(markdown: &str) -> Result<Vec<MarkdownRule>> {
        let result = Self::process(markdown)?;
        Ok(result.rules)
    }
}

/// Generate HTML for a rule anchor badge.
///
/// [impl markdown.html.div]
/// [impl markdown.html.anchor]
/// [impl markdown.html.link]
/// [impl markdown.html.wbr]
fn rule_to_html(rule_id: &str, anchor_id: &str) -> String {
    // Insert <wbr> after dots for better line breaking
    let display_id = rule_id.replace('.', ".<wbr>");
    format!(
        "<div class=\"rule\" id=\"{anchor_id}\"><a class=\"rule-link\" href=\"#{anchor_id}\" title=\"{rule_id}\"><span>[{display_id}]</span></a></div>"
    )
}

/// Generate an HTML redirect page for a rule.
///
/// This creates a simple HTML page with a meta refresh redirect,
/// suitable for static hosting where server-side redirects aren't available.
pub fn generate_redirect_html(rule_id: &str, target_url: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="refresh" content="0; url={target_url}">
<link rel="canonical" href="{target_url}">
<title>Redirecting to {rule_id}</title>
</head>
<body>
Redirecting to <a href="{target_url}">{rule_id}</a>...
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_single_rule() {
        let markdown = r#"
# My Spec

r[my.rule.id]
This is the rule content.
"#;

        let result = MarkdownProcessor::process(markdown).unwrap();
        assert_eq!(result.rules.len(), 1);
        assert_eq!(result.rules[0].id, "my.rule.id");
        assert_eq!(result.rules[0].anchor_id, "r-my.rule.id");
    }

    #[test]
    fn test_extract_multiple_rules() {
        let markdown = r#"
r[first.rule]
First content.

r[second.rule]
Second content.

r[third.rule]
Third content.
"#;

        let result = MarkdownProcessor::process(markdown).unwrap();
        assert_eq!(result.rules.len(), 3);
        assert_eq!(result.rules[0].id, "first.rule");
        assert_eq!(result.rules[1].id, "second.rule");
        assert_eq!(result.rules[2].id, "third.rule");
    }

    #[test]
    fn test_duplicate_rule_error() {
        let markdown = r#"
r[duplicate.rule]
First occurrence.

r[duplicate.rule]
Second occurrence.
"#;

        let result = MarkdownProcessor::process(markdown);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate rule identifier"));
    }

    #[test]
    fn test_html_output() {
        let markdown = "r[test.rule]\nContent here.\n";

        let result = MarkdownProcessor::process(markdown).unwrap();
        assert!(result.output.contains("class=\"rule\""));
        assert!(result.output.contains("id=\"r-test.rule\""));
        assert!(result.output.contains("href=\"#r-test.rule\""));
        assert!(result.output.contains("[test.<wbr>rule]"));
    }

    #[test]
    fn test_manifest_json_format() {
        let rules = vec![
            MarkdownRule {
                id: "channel.id.allocation".to_string(),
                anchor_id: "r-channel.id.allocation".to_string(),
                span: SourceSpan {
                    offset: 0,
                    length: 10,
                },
            },
            MarkdownRule {
                id: "channel.id.parity".to_string(),
                anchor_id: "r-channel.id.parity".to_string(),
                span: SourceSpan {
                    offset: 20,
                    length: 10,
                },
            },
        ];

        let manifest = RulesManifest::from_rules(&rules, "/spec/core");
        let json = manifest.to_json();

        assert!(json.contains("channel.id.allocation"));
        assert!(json.contains("/spec/core#r-channel.id.allocation"));
    }

    #[test]
    fn test_no_rules() {
        let markdown = r#"
# Just a heading

Some regular content without any rules.
"#;

        let result = MarkdownProcessor::process(markdown).unwrap();
        assert!(result.rules.is_empty());
    }

    #[test]
    fn test_rule_like_but_not_rule() {
        // These shouldn't be parsed as rules
        // [verify markdown.syntax.inline-ignored]
        let markdown = r#"
This is r[not.a.rule] inline.
`r[code.block]`
    r[indented.line]
"#;

        let result = MarkdownProcessor::process(markdown).unwrap();
        // Only the indented one (when trimmed) would match
        assert_eq!(result.rules.len(), 1);
        assert_eq!(result.rules[0].id, "indented.line");
    }
}
