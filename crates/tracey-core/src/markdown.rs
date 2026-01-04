//! Markdown preprocessor for extracting rules from spec documents.
//!
//! This module provides functionality to:
//! 1. Parse markdown spec documents and extract rule definitions
//! 2. Generate `_rules.json` manifests from extracted rules
//! 3. Transform markdown with rules replaced by `<div>` elements for rendering
//! 4. Strip frontmatter from markdown documents
//!
//! # Rule Types from bearmark
//!
//! This module re-exports rule-related types from the [`bearmark`] crate,
//! which provides the canonical definitions with [`facet::Facet`] derivations
//! for JSON serialization.
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
use std::path::PathBuf;

use eyre::{Result, bail};
use facet::Facet;

// Re-export types from bearmark - these are the canonical definitions
pub use bearmark::{
    // Frontmatter utilities
    Frontmatter,
    FrontmatterFormat,
    RequirementLevel,
    // RFC 2119 types and detection
    Rfc2119Keyword,
    // Rule definition - re-export as MarkdownRule for backwards compat
    RuleDefinition as MarkdownRule,
    RuleMetadata,
    // Rule lifecycle types
    RuleStatus,
    // Warning types
    RuleWarning as MarkdownWarning,
    RuleWarningKind as MarkdownWarningKind,
    // Source location tracking
    SourceSpan,
    detect_rfc2119_keywords,
    parse_frontmatter,
    strip_frontmatter as bearmark_strip_frontmatter,
};

/// Result of processing a markdown document.
#[derive(Debug, Clone)]
pub struct ProcessedMarkdown {
    /// All rules found in the document
    pub rules: Vec<MarkdownRule>,
    /// Transformed markdown with rule markers replaced by HTML divs
    pub output: String,
    /// Warnings about rule quality (missing RFC 2119 keywords, etc.)
    pub warnings: Vec<MarkdownWarning>,
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
        Self::process_with_path(markdown, None)
    }

    /// Process markdown content with an optional source file path.
    ///
    /// The path is used for warnings to indicate where issues were found.
    pub fn process_with_path(
        markdown: &str,
        source_path: Option<&std::path::Path>,
    ) -> Result<ProcessedMarkdown> {
        let mut result = String::with_capacity(markdown.len());
        let mut rules = Vec::new();
        let mut warnings = Vec::new();
        let mut seen_rule_ids: HashSet<String> = HashSet::new();

        let file_path = source_path
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("<unknown>"));

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

                // Render paragraph to HTML (simple version - bearmark does full rendering)
                let paragraph_html = if text.is_empty() {
                    String::new()
                } else {
                    format!("<p>{}</p>\n", html_escape::encode_text(&text))
                };

                // Check for RFC 2119 keywords and emit warnings
                let keywords = detect_rfc2119_keywords(&text);

                if keywords.is_empty() && !text.is_empty() {
                    // No RFC 2119 keywords found - this may be an underspecified rule
                    warnings.push(MarkdownWarning {
                        file: file_path.clone(),
                        rule_id: rule_id.to_string(),
                        line: i + 1,
                        span,
                        kind: MarkdownWarningKind::NoRfc2119Keyword,
                    });
                }

                // Check for negative keywords (MUST NOT, SHOULD NOT)
                for keyword in &keywords {
                    if keyword.is_negative() {
                        warnings.push(MarkdownWarning {
                            file: file_path.clone(),
                            rule_id: rule_id.to_string(),
                            line: i + 1,
                            span,
                            kind: MarkdownWarningKind::NegativeRequirement(*keyword),
                        });
                    }
                }

                rules.push(MarkdownRule {
                    id: rule_id.to_string(),
                    anchor_id: anchor_id.clone(),
                    span,
                    line: i + 1, // 1-indexed line number
                    metadata,
                    text,
                    paragraph_html,
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
            warnings,
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

/// Result of stripping frontmatter from a markdown document.
#[derive(Debug, Clone)]
pub struct StrippedMarkdown<'a> {
    /// The frontmatter content (without delimiters), if present
    pub frontmatter: Option<&'a str>,
    /// The markdown content after the frontmatter
    pub content: &'a str,
    /// The type of frontmatter delimiter used
    pub frontmatter_type: Option<FrontmatterType>,
}

/// Type of frontmatter delimiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontmatterType {
    /// YAML frontmatter delimited by `---`
    Yaml,
    /// TOML frontmatter delimited by `+++`
    Toml,
}

/// Strip frontmatter from a markdown document.
///
/// Supports both YAML (`---`) and TOML (`+++`) frontmatter delimiters.
/// Frontmatter must start at the very beginning of the document.
///
/// [impl markdown.frontmatter.strip]
///
/// # Examples
///
/// ```
/// use tracey_core::markdown::{strip_frontmatter, FrontmatterType};
///
/// // YAML frontmatter
/// let md = "---\ntitle: Hello\n---\n# Content";
/// let result = strip_frontmatter(md);
/// assert_eq!(result.frontmatter, Some("title: Hello"));
/// assert_eq!(result.content, "# Content");
/// assert_eq!(result.frontmatter_type, Some(FrontmatterType::Yaml));
///
/// // TOML frontmatter
/// let md = "+++\ntitle = \"Hello\"\n+++\n# Content";
/// let result = strip_frontmatter(md);
/// assert_eq!(result.frontmatter, Some("title = \"Hello\""));
/// assert_eq!(result.content, "# Content");
/// assert_eq!(result.frontmatter_type, Some(FrontmatterType::Toml));
///
/// // No frontmatter
/// let md = "# Just content";
/// let result = strip_frontmatter(md);
/// assert_eq!(result.frontmatter, None);
/// assert_eq!(result.content, "# Just content");
/// ```
pub fn strip_frontmatter(markdown: &str) -> StrippedMarkdown<'_> {
    // [impl markdown.frontmatter.yaml]
    // [impl markdown.frontmatter.toml]
    let (delimiter, fm_type) = if markdown.starts_with("---\n") || markdown.starts_with("---\r\n") {
        ("---", FrontmatterType::Yaml)
    } else if markdown.starts_with("+++\n") || markdown.starts_with("+++\r\n") {
        ("+++", FrontmatterType::Toml)
    } else {
        // No frontmatter
        return StrippedMarkdown {
            frontmatter: None,
            content: markdown,
            frontmatter_type: None,
        };
    };

    // Find the closing delimiter
    // Skip the opening delimiter (3 chars) and the newline
    let start_offset = if markdown.starts_with(&format!("{}\r\n", delimiter)) {
        5 // delimiter (3) + \r\n (2)
    } else {
        4 // delimiter (3) + \n (1)
    };

    // Look for the closing delimiter on its own line
    let search_area = &markdown[start_offset..];

    // Check if closing delimiter is immediately after opening (empty frontmatter)
    let immediate_close_patterns = [format!("{}\n", delimiter), format!("{}\r\n", delimiter)];

    for pattern in &immediate_close_patterns {
        if search_area.starts_with(pattern.as_str()) {
            let content_start = start_offset + pattern.len();
            let content = &markdown[content_start..];
            return StrippedMarkdown {
                frontmatter: Some(""),
                content,
                frontmatter_type: Some(fm_type),
            };
        }
    }

    // Find closing delimiter: must be at start of a line
    let closing_patterns = [
        format!("\n{}\n", delimiter),
        format!("\n{}\r\n", delimiter),
        format!("\r\n{}\n", delimiter),
        format!("\r\n{}\r\n", delimiter),
    ];

    let mut best_match: Option<(usize, usize)> = None; // (position in search_area, pattern_len)

    for pattern in &closing_patterns {
        if let Some(pos) = search_area.find(pattern)
            && (best_match.is_none() || pos < best_match.unwrap().0)
        {
            best_match = Some((pos, pattern.len()));
        }
    }

    if let Some((pos, pattern_len)) = best_match {
        let frontmatter = search_area[..pos].trim();
        let content_start = start_offset + pos + pattern_len;
        let content = &markdown[content_start..];

        StrippedMarkdown {
            frontmatter: Some(frontmatter),
            content,
            frontmatter_type: Some(fm_type),
        }
    } else {
        // Opening delimiter but no closing - treat as no frontmatter
        // (the delimiter might be part of the content, e.g., a horizontal rule)
        StrippedMarkdown {
            frontmatter: None,
            content: markdown,
            frontmatter_type: None,
        }
    }
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
                paragraph_html: "<p>Channel IDs must be allocated.</p>\n".to_string(),
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
                paragraph_html: String::new(),
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

    // RFC 2119 keyword detection tests

    #[test]
    fn test_detect_rfc2119_must() {
        let keywords = detect_rfc2119_keywords("Channel IDs MUST be allocated sequentially.");
        assert_eq!(keywords, vec![Rfc2119Keyword::Must]);

        let keywords = detect_rfc2119_keywords("The server SHALL respond within 100ms.");
        assert_eq!(keywords, vec![Rfc2119Keyword::Must]);

        let keywords = detect_rfc2119_keywords("This field is REQUIRED.");
        assert_eq!(keywords, vec![Rfc2119Keyword::Must]);
    }

    #[test]
    fn test_detect_rfc2119_must_not() {
        let keywords = detect_rfc2119_keywords("Clients MUST NOT send invalid data.");
        assert_eq!(keywords, vec![Rfc2119Keyword::MustNot]);

        let keywords = detect_rfc2119_keywords("Servers SHALL NOT close unexpectedly.");
        assert_eq!(keywords, vec![Rfc2119Keyword::MustNot]);
    }

    #[test]
    fn test_detect_rfc2119_should() {
        let keywords = detect_rfc2119_keywords("Implementations SHOULD use TLS.");
        assert_eq!(keywords, vec![Rfc2119Keyword::Should]);

        let keywords = detect_rfc2119_keywords("This approach is RECOMMENDED.");
        assert_eq!(keywords, vec![Rfc2119Keyword::Should]);
    }

    #[test]
    fn test_detect_rfc2119_should_not() {
        let keywords = detect_rfc2119_keywords("Clients SHOULD NOT retry immediately.");
        assert_eq!(keywords, vec![Rfc2119Keyword::ShouldNot]);

        let keywords = detect_rfc2119_keywords("This pattern is NOT RECOMMENDED.");
        assert_eq!(keywords, vec![Rfc2119Keyword::ShouldNot]);
    }

    #[test]
    fn test_detect_rfc2119_may() {
        let keywords = detect_rfc2119_keywords("Implementations MAY cache responses.");
        assert_eq!(keywords, vec![Rfc2119Keyword::May]);

        let keywords = detect_rfc2119_keywords("This feature is OPTIONAL.");
        assert_eq!(keywords, vec![Rfc2119Keyword::May]);
    }

    #[test]
    fn test_detect_rfc2119_multiple() {
        let keywords =
            detect_rfc2119_keywords("Clients MUST validate input and SHOULD log errors.");
        assert_eq!(keywords, vec![Rfc2119Keyword::Must, Rfc2119Keyword::Should]);
    }

    #[test]
    fn test_detect_rfc2119_case_sensitive() {
        // Only uppercase keywords should match per RFC 2119
        let keywords = detect_rfc2119_keywords("The server must respond.");
        assert!(keywords.is_empty());

        let keywords = detect_rfc2119_keywords("You should read the docs.");
        assert!(keywords.is_empty());
    }

    #[test]
    fn test_detect_rfc2119_none() {
        let keywords = detect_rfc2119_keywords("This is just a description.");
        assert!(keywords.is_empty());
    }

    #[test]
    fn test_warning_no_keyword() {
        let markdown = r#"
r[missing.keyword]
This rule has no RFC 2119 keyword.
"#;

        let result = MarkdownProcessor::process(markdown).unwrap();
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].rule_id, "missing.keyword");
        assert!(matches!(
            result.warnings[0].kind,
            MarkdownWarningKind::NoRfc2119Keyword
        ));
    }

    #[test]
    fn test_warning_negative_requirement() {
        let markdown = r#"
r[negative.must.not]
Clients MUST NOT send invalid data.
"#;

        let result = MarkdownProcessor::process(markdown).unwrap();
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].rule_id, "negative.must.not");
        assert!(matches!(
            result.warnings[0].kind,
            MarkdownWarningKind::NegativeRequirement(Rfc2119Keyword::MustNot)
        ));
    }

    #[test]
    fn test_no_warning_for_positive_must() {
        let markdown = r#"
r[positive.must]
Clients MUST validate input.
"#;

        let result = MarkdownProcessor::process(markdown).unwrap();
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_no_warning_for_empty_text() {
        // Rules without text shouldn't warn about missing keywords
        let markdown = r#"
r[empty.text]

r[another.rule]
Some content.
"#;

        let result = MarkdownProcessor::process(markdown).unwrap();
        // Only the second rule should have a warning (no keyword)
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].rule_id, "another.rule");
    }

    // Frontmatter stripping tests

    #[test]
    fn test_strip_yaml_frontmatter() {
        let markdown = "---\ntitle: Hello World\nauthor: Test\n---\n# Content\n\nBody text.";
        let result = strip_frontmatter(markdown);

        assert_eq!(result.frontmatter, Some("title: Hello World\nauthor: Test"));
        assert_eq!(result.content, "# Content\n\nBody text.");
        assert_eq!(result.frontmatter_type, Some(FrontmatterType::Yaml));
    }

    #[test]
    fn test_strip_toml_frontmatter() {
        let markdown = "+++\ntitle = \"Hello World\"\nweight = 10\n+++\n# Content\n\nBody text.";
        let result = strip_frontmatter(markdown);

        assert_eq!(
            result.frontmatter,
            Some("title = \"Hello World\"\nweight = 10")
        );
        assert_eq!(result.content, "# Content\n\nBody text.");
        assert_eq!(result.frontmatter_type, Some(FrontmatterType::Toml));
    }

    #[test]
    fn test_strip_frontmatter_none() {
        let markdown = "# Just Content\n\nNo frontmatter here.";
        let result = strip_frontmatter(markdown);

        assert_eq!(result.frontmatter, None);
        assert_eq!(result.content, "# Just Content\n\nNo frontmatter here.");
        assert_eq!(result.frontmatter_type, None);
    }

    #[test]
    fn test_strip_frontmatter_unclosed() {
        // Opening delimiter but no closing - treat as regular content
        let markdown = "---\nThis looks like frontmatter\nbut has no closing delimiter";
        let result = strip_frontmatter(markdown);

        assert_eq!(result.frontmatter, None);
        assert_eq!(result.content, markdown);
        assert_eq!(result.frontmatter_type, None);
    }

    #[test]
    fn test_strip_frontmatter_empty() {
        let markdown = "---\n---\n# Content";
        let result = strip_frontmatter(markdown);

        assert_eq!(result.frontmatter, Some(""));
        assert_eq!(result.content, "# Content");
        assert_eq!(result.frontmatter_type, Some(FrontmatterType::Yaml));
    }

    #[test]
    fn test_strip_frontmatter_with_rules() {
        // [verify markdown.frontmatter.strip]
        let markdown = r#"---
title: My Spec
---
# Specification

r[my.rule]
This rule MUST be followed.
"#;
        let result = strip_frontmatter(markdown);

        assert_eq!(result.frontmatter, Some("title: My Spec"));
        assert!(result.content.starts_with("# Specification"));

        // Verify rules can still be extracted from stripped content
        let processed = MarkdownProcessor::process(result.content).unwrap();
        assert_eq!(processed.rules.len(), 1);
        assert_eq!(processed.rules[0].id, "my.rule");
    }

    #[test]
    fn test_strip_frontmatter_horizontal_rule_not_frontmatter() {
        // A horizontal rule (---) in the middle of content should not be treated as frontmatter
        let markdown = "# Heading\n\n---\n\nMore content";
        let result = strip_frontmatter(markdown);

        assert_eq!(result.frontmatter, None);
        assert_eq!(result.content, markdown);
    }
}
