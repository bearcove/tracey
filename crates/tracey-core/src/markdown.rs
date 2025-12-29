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
//! Rules can also include metadata attributes:
//!
//! ```markdown
//! r[channel.id.allocation status=stable level=must since=1.0]
//! Channel IDs MUST be allocated sequentially.
//!
//! r[experimental.feature status=draft]
//! This feature is under development.
//!
//! r[old.behavior status=deprecated until=3.0]
//! This behavior is deprecated and will be removed.
//!
//! r[optional.feature level=may tags=optional,experimental]
//! This feature is optional.
//! ```
//!
//! ## Supported Metadata Attributes
//!
//! | Attribute | Values | Description |
//! |-----------|--------|-------------|
//! | `status`  | `draft`, `stable`, `deprecated`, `removed` | Lifecycle stage |
//! | `level`   | `must`, `should`, `may` | RFC 2119 requirement level |
//! | `since`   | version string | When the rule was introduced |
//! | `until`   | version string | When the rule will be deprecated/removed |
//! | `tags`    | comma-separated | Custom tags for categorization |
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

/// Lifecycle status of a rule.
///
/// Rules progress through these states as the specification evolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(u8)]
pub enum RuleStatus {
    /// Rule is proposed but not yet finalized
    Draft,
    /// Rule is active and enforced
    #[default]
    Stable,
    /// Rule is being phased out
    Deprecated,
    /// Rule has been removed (kept for historical reference)
    Removed,
}

impl RuleStatus {
    /// Parse a status from its string representation.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(RuleStatus::Draft),
            "stable" => Some(RuleStatus::Stable),
            "deprecated" => Some(RuleStatus::Deprecated),
            "removed" => Some(RuleStatus::Removed),
            _ => None,
        }
    }

    /// Get the string representation of this status.
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleStatus::Draft => "draft",
            RuleStatus::Stable => "stable",
            RuleStatus::Deprecated => "deprecated",
            RuleStatus::Removed => "removed",
        }
    }
}

impl std::fmt::Display for RuleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// RFC 2119 requirement level for a rule.
///
/// See <https://www.ietf.org/rfc/rfc2119.txt> for the specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(u8)]
pub enum RequirementLevel {
    /// Absolute requirement (MUST, SHALL, REQUIRED)
    #[default]
    Must,
    /// Recommended but not required (SHOULD, RECOMMENDED)
    Should,
    /// Truly optional (MAY, OPTIONAL)
    May,
}

impl RequirementLevel {
    /// Parse a level from its string representation.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "must" | "shall" | "required" => Some(RequirementLevel::Must),
            "should" | "recommended" => Some(RequirementLevel::Should),
            "may" | "optional" => Some(RequirementLevel::May),
            _ => None,
        }
    }

    /// Get the string representation of this level.
    pub fn as_str(&self) -> &'static str {
        match self {
            RequirementLevel::Must => "must",
            RequirementLevel::Should => "should",
            RequirementLevel::May => "may",
        }
    }
}

impl std::fmt::Display for RequirementLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Metadata attributes for a rule.
#[derive(Debug, Clone, Default, Facet)]
pub struct RuleMetadata {
    /// Lifecycle status (draft, stable, deprecated, removed)
    pub status: Option<RuleStatus>,
    /// RFC 2119 requirement level (must, should, may)
    pub level: Option<RequirementLevel>,
    /// Version when this rule was introduced
    pub since: Option<String>,
    /// Version when this rule will be/was deprecated or removed
    pub until: Option<String>,
    /// Custom tags for categorization
    pub tags: Vec<String>,
}

impl RuleMetadata {
    /// Returns true if this rule should be counted in coverage by default.
    ///
    /// Draft and removed rules are excluded from coverage by default.
    pub fn counts_for_coverage(&self) -> bool {
        !matches!(
            self.status,
            Some(RuleStatus::Draft) | Some(RuleStatus::Removed)
        )
    }

    /// Returns true if this rule is required (must be covered for passing builds).
    ///
    /// Only `must` level rules are required; `should` and `may` are optional.
    pub fn is_required(&self) -> bool {
        match self.level {
            Some(RequirementLevel::Must) | None => true,
            Some(RequirementLevel::Should) | Some(RequirementLevel::May) => false,
        }
    }
}

/// A rule extracted from a markdown document.
#[derive(Debug, Clone, Facet)]
pub struct MarkdownRule {
    /// The rule identifier (e.g., "channel.id.allocation")
    pub id: String,
    /// The anchor ID for HTML linking (e.g., "r-channel.id.allocation")
    pub anchor_id: String,
    /// Source location of this rule in the original markdown
    pub span: SourceSpan,
    /// Line number where this rule is defined (1-indexed)
    pub line: usize,
    /// Rule metadata (status, level, since, until, tags)
    pub metadata: RuleMetadata,
    /// The text content following the rule marker (first paragraph)
    pub text: String,
}

/// Result of processing a markdown document.
#[derive(Debug, Clone)]
pub struct ProcessedMarkdown {
    /// All rules found in the document
    pub rules: Vec<MarkdownRule>,
    /// Transformed markdown with rule markers replaced by HTML divs
    pub output: String,
}

/// A rule entry in the manifest, with its target URL and metadata.
///
/// [impl manifest.format.rule-entry]
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
    /// The `source_file` is the relative path to the markdown file containing these rules.
    pub fn from_rules(rules: &[MarkdownRule], base_url: &str, source_file: Option<&str>) -> Self {
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
        facet_json::to_string_pretty(self).expect("RulesManifest should serialize to JSON")
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
    /// Rules are lines matching `r[rule.id]` or `r[rule.id attr=value ...]` on their own line.
    /// They are replaced with HTML div elements for rendering.
    ///
    /// # Rule Syntax
    ///
    /// Basic rule:
    /// ```text
    /// r[channel.id.allocation]
    /// ```
    ///
    /// Rule with metadata attributes:
    /// ```text
    /// r[channel.id.allocation status=stable level=must since=1.0]
    /// r[experimental.feature status=draft]
    /// r[old.behavior status=deprecated until=3.0]
    /// r[optional.feature level=may tags=optional,experimental]
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if duplicate rule IDs are found within the same document.
    pub fn process(markdown: &str) -> Result<ProcessedMarkdown> {
        let mut result = String::with_capacity(markdown.len());
        let mut rules = Vec::new();
        let mut seen_rule_ids: HashSet<String> = HashSet::new();

        // Collect all lines for lookahead
        let lines: Vec<&str> = markdown.lines().collect();
        let mut byte_offset = 0usize;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let line_byte_len = line.len();

            // Check if this line is a rule identifier: r[rule.id] or r[rule.id attrs...]
            // [impl markdown.syntax.marker]
            // [impl markdown.syntax.standalone]
            // [impl markdown.syntax.inline-ignored]
            // By requiring the trimmed line to START with "r[", inline occurrences are ignored
            if trimmed.starts_with("r[") && trimmed.ends_with(']') && trimmed.len() > 3 {
                let inner = &trimmed[2..trimmed.len() - 1];

                // Parse the rule ID and optional attributes
                // Format: "rule.id" or "rule.id attr=value attr=value"
                let (rule_id, metadata) = parse_rule_marker(inner)?;

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

                // Extract the rule text: collect lines until we hit a blank line,
                // another rule marker, or a heading
                let text = extract_rule_text(&lines[i + 1..]);

                rules.push(MarkdownRule {
                    id: rule_id.to_string(),
                    anchor_id: anchor_id.clone(),
                    span,
                    line: i + 1, // 1-indexed line number
                    metadata,
                    text,
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

/// Extract the rule text from lines following a rule marker.
///
/// Collects text until we hit:
/// - A blank line
/// - Another rule marker (r[...])
/// - A heading (# ...)
/// - End of content
fn extract_rule_text(lines: &[&str]) -> String {
    let mut text_lines = Vec::new();

    for line in lines {
        let trimmed = line.trim();

        // Stop at blank line
        if trimmed.is_empty() {
            break;
        }

        // Stop at another rule marker
        if trimmed.starts_with("r[") && trimmed.ends_with(']') {
            break;
        }

        // Stop at headings
        if trimmed.starts_with('#') {
            break;
        }

        text_lines.push(trimmed);
    }

    text_lines.join(" ")
}

/// Parse a rule marker content (inside r[...]).
///
/// Supports formats:
/// - `rule.id` - simple rule ID
/// - `rule.id status=stable level=must` - rule ID with attributes
///
/// Returns the rule ID and parsed metadata.
fn parse_rule_marker(inner: &str) -> Result<(&str, RuleMetadata)> {
    let inner = inner.trim();

    // Find where the rule ID ends (at first space or end of string)
    let (rule_id, attrs_str) = match inner.find(' ') {
        Some(idx) => (&inner[..idx], inner[idx + 1..].trim()),
        None => (inner, ""),
    };

    if rule_id.is_empty() {
        bail!("empty rule identifier");
    }

    // Parse attributes if present
    let mut metadata = RuleMetadata::default();

    if !attrs_str.is_empty() {
        for attr in attrs_str.split_whitespace() {
            if let Some((key, value)) = attr.split_once('=') {
                match key {
                    "status" => {
                        metadata.status = Some(RuleStatus::parse(value).ok_or_else(|| {
                            eyre::eyre!(
                                "invalid status '{}' for rule '{}', expected: draft, stable, deprecated, removed",
                                value,
                                rule_id
                            )
                        })?);
                    }
                    "level" => {
                        metadata.level = Some(RequirementLevel::parse(value).ok_or_else(|| {
                            eyre::eyre!(
                                "invalid level '{}' for rule '{}', expected: must, should, may",
                                value,
                                rule_id
                            )
                        })?);
                    }
                    "since" => {
                        metadata.since = Some(value.to_string());
                    }
                    "until" => {
                        metadata.until = Some(value.to_string());
                    }
                    "tags" => {
                        metadata.tags = value.split(',').map(|s| s.trim().to_string()).collect();
                    }
                    _ => {
                        bail!(
                            "unknown attribute '{}' for rule '{}', expected: status, level, since, until, tags",
                            key,
                            rule_id
                        );
                    }
                }
            } else {
                bail!(
                    "invalid attribute format '{}' for rule '{}', expected: key=value",
                    attr,
                    rule_id
                );
            }
        }
    }

    Ok((rule_id, metadata))
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
        assert_eq!(result.rules[0].text, "This is the rule content.");
    }

    #[test]
    fn test_extract_rule_text() {
        let markdown = r#"
r[simple.rule]
Single line description.

r[multiline.rule]
This rule spans
multiple lines until blank.

r[stops.at.heading]
Text before heading.

# Next Section

r[stops.at.rule]
Text before another rule.
r[another.rule]
Another rule text.
"#;

        let result = MarkdownProcessor::process(markdown).unwrap();
        assert_eq!(result.rules.len(), 5);

        assert_eq!(result.rules[0].id, "simple.rule");
        assert_eq!(result.rules[0].text, "Single line description.");

        assert_eq!(result.rules[1].id, "multiline.rule");
        assert_eq!(
            result.rules[1].text,
            "This rule spans multiple lines until blank."
        );

        assert_eq!(result.rules[2].id, "stops.at.heading");
        assert_eq!(result.rules[2].text, "Text before heading.");

        assert_eq!(result.rules[3].id, "stops.at.rule");
        assert_eq!(result.rules[3].text, "Text before another rule.");

        assert_eq!(result.rules[4].id, "another.rule");
        assert_eq!(result.rules[4].text, "Another rule text.");
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
                line: 1,
                metadata: RuleMetadata::default(),
                text: "Channel IDs must be allocated.".to_string(),
            },
            MarkdownRule {
                id: "channel.id.parity".to_string(),
                anchor_id: "r-channel.id.parity".to_string(),
                span: SourceSpan {
                    offset: 20,
                    length: 10,
                },
                line: 5,
                metadata: RuleMetadata::default(),
                text: String::new(),
            },
        ];

        let manifest = RulesManifest::from_rules(&rules, "/spec/core", Some("spec.md"));
        let json = manifest.to_json();

        assert!(json.contains("channel.id.allocation"));
        assert!(json.contains("/spec/core#r-channel.id.allocation"));
        assert!(json.contains("Channel IDs must be allocated."));
        assert!(json.contains("\"source_file\": \"spec.md\""));
        assert!(json.contains("\"source_line\": 1"));
    }

    #[test]
    fn test_rule_with_metadata() {
        let markdown = r#"
r[stable.rule status=stable level=must since=1.0]
This is a stable, required rule.

r[draft.feature status=draft]
This feature is under development.

r[deprecated.api status=deprecated until=3.0]
This API is deprecated.

r[optional.feature level=may tags=optional,experimental]
This feature is optional.
"#;

        let result = MarkdownProcessor::process(markdown).unwrap();
        assert_eq!(result.rules.len(), 4);

        // Check stable rule
        assert_eq!(result.rules[0].id, "stable.rule");
        assert_eq!(result.rules[0].metadata.status, Some(RuleStatus::Stable));
        assert_eq!(result.rules[0].metadata.level, Some(RequirementLevel::Must));
        assert_eq!(result.rules[0].metadata.since, Some("1.0".to_string()));

        // Check draft rule
        assert_eq!(result.rules[1].id, "draft.feature");
        assert_eq!(result.rules[1].metadata.status, Some(RuleStatus::Draft));
        assert!(!result.rules[1].metadata.counts_for_coverage());

        // Check deprecated rule
        assert_eq!(result.rules[2].id, "deprecated.api");
        assert_eq!(
            result.rules[2].metadata.status,
            Some(RuleStatus::Deprecated)
        );
        assert_eq!(result.rules[2].metadata.until, Some("3.0".to_string()));

        // Check optional rule with tags
        assert_eq!(result.rules[3].id, "optional.feature");
        assert_eq!(result.rules[3].metadata.level, Some(RequirementLevel::May));
        assert_eq!(
            result.rules[3].metadata.tags,
            vec!["optional", "experimental"]
        );
        assert!(!result.rules[3].metadata.is_required());
    }

    #[test]
    fn test_manifest_with_metadata() {
        let markdown = r#"
r[api.stable status=stable level=must since=1.0]
Stable API rule.

r[api.optional level=should]
Optional API rule.
"#;

        let result = MarkdownProcessor::process(markdown).unwrap();
        let manifest = RulesManifest::from_rules(&result.rules, "/spec", None);
        let json = manifest.to_json();

        // Check that metadata is included in JSON
        assert!(json.contains("\"status\": \"stable\""));
        assert!(json.contains("\"level\": \"must\""));
        assert!(json.contains("\"since\": \"1.0\""));
        assert!(json.contains("\"level\": \"should\""));
    }

    #[test]
    fn test_invalid_status() {
        let markdown = "r[bad.rule status=invalid]\nContent.";
        let result = MarkdownProcessor::process(markdown);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid status"));
    }

    #[test]
    fn test_invalid_level() {
        let markdown = "r[bad.rule level=invalid]\nContent.";
        let result = MarkdownProcessor::process(markdown);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid level"));
    }

    #[test]
    fn test_unknown_attribute() {
        let markdown = "r[bad.rule unknown=value]\nContent.";
        let result = MarkdownProcessor::process(markdown);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unknown attribute")
        );
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
