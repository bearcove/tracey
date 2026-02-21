//! tracey library - Measure spec coverage in Rust codebases
//!
//! This library exposes the core functionality of tracey for testing
//! and embedding purposes.

pub mod bridge;
pub mod config;
pub mod daemon;
pub mod data;
pub mod search;
pub mod server;
pub mod vite;

use config::Config;
use eyre::{Result, WrapErr};
use std::path::PathBuf;
use tracey_core::ReqDefinition;

// Re-export from marq for rule extraction
use marq::{RenderOptions, render};

/// Extracted rule with source location info
pub struct ExtractedRule {
    pub def: ReqDefinition,
    pub source_file: String,
    /// Marker prefix used by this requirement definition (e.g., "r" in r[foo.bar])
    pub prefix: String,
    /// 1-indexed column where the rule marker starts
    pub column: Option<usize>,
    /// Section slug (heading ID) that this rule belongs to
    pub section: Option<String>,
    /// Section title (heading text) that this rule belongs to
    pub section_title: Option<String>,
}

/// Compute 1-indexed column from byte offset in content
fn compute_column(content: &str, byte_offset: usize) -> usize {
    // Find the start of the line containing this offset
    let before = &content[..byte_offset.min(content.len())];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    // Column is the number of characters from line start to offset (1-indexed)
    before[line_start..].chars().count() + 1
}

fn extract_marker_prefix(content: &str, marker_span: marq::SourceSpan) -> Option<String> {
    let start = marker_span.offset;
    let end = start.checked_add(marker_span.length)?;
    let marker = content.get(start..end)?;
    let bracket = marker.find('[')?;
    let prefix = marker[..bracket].trim();
    if prefix.is_empty() {
        return None;
    }
    Some(prefix.to_string())
}

/// Load rules from markdown files matching a glob pattern.
///
/// marq implements markdown rule extraction:
/// r[impl markdown.syntax.marker+2]
/// r[impl markdown.syntax.inline-ignored]
pub async fn load_rules_from_glob(
    root: &std::path::Path,
    pattern: &str,
    quiet: bool,
) -> Result<Vec<ExtractedRule>> {
    use ignore::WalkBuilder;
    use owo_colors::OwoColorize;
    use std::collections::HashSet;

    let mut rules: Vec<ExtractedRule> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    // Handle external paths (patterns starting with ..)
    // For these, we resolve the walk root and adjust the pattern
    let (walk_root, effective_pattern, is_external) = if pattern.starts_with("..") {
        // Find the directory prefix before any glob metacharacters
        let mut prefix_parts = Vec::new();
        let mut remaining_parts = Vec::new();
        let mut found_glob = false;

        for part in pattern.split('/') {
            if found_glob || part.contains('*') || part.contains('?') || part.contains('[') {
                found_glob = true;
                remaining_parts.push(part);
            } else {
                prefix_parts.push(part);
            }
        }

        let prefix = prefix_parts.join("/");
        let resolved_root = root.join(&prefix).canonicalize().wrap_err_with(|| {
            format!(
                "External spec path '{}' does not exist (resolved from '{}')",
                prefix, pattern
            )
        })?;

        let effective = if remaining_parts.is_empty() {
            "**/*.md".to_string()
        } else {
            remaining_parts.join("/")
        };

        (resolved_root, effective, true)
    } else {
        (root.to_path_buf(), pattern.to_string(), false)
    };

    // Walk the directory tree
    let walker = WalkBuilder::new(&walk_root)
        .follow_links(true)
        .hidden(false)
        .git_ignore(true)
        .build();

    for entry in walker {
        let entry = entry?;
        let path = entry.path();

        // Only process .md files
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }

        // Check if the path matches the glob pattern
        let relative = path.strip_prefix(&walk_root).unwrap_or(path);
        let relative_str = relative.to_string_lossy().to_string();

        // For display purposes, show the original pattern prefix for external paths
        let display_path = if is_external {
            // Reconstruct the path with the original prefix for display
            let prefix_end = pattern.find('*').unwrap_or(pattern.len());
            let prefix = &pattern[..prefix_end].trim_end_matches('/');
            format!("{}/{}", prefix, relative_str)
        } else {
            relative_str.clone()
        };

        if !matches_glob(&relative_str, &effective_pattern) {
            continue;
        }

        // Read and render markdown to extract rules with HTML
        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("Failed to read {}", path.display()))?;

        let doc = render(&content, &RenderOptions::default())
            .await
            .map_err(|e| eyre::eyre!("Failed to process {}: {}", path.display(), e))?;

        if !doc.reqs.is_empty() {
            if !quiet {
                eprintln!(
                    "   {} {} requirements from {}",
                    "Found".green(),
                    doc.reqs.len(),
                    display_path
                );
            }

            // Check for duplicates
            // r[impl markdown.duplicates.same-file] - caught when marq returns duplicate reqs from single file
            // r[impl markdown.duplicates.cross-file] - caught via seen_ids persisting across files
            for req in &doc.reqs {
                let req_id = req.id.to_string();
                if seen_ids.contains(&req_id) {
                    eyre::bail!(
                        "Duplicate requirement '{}' found in {}",
                        req.id.red(),
                        display_path
                    );
                }
                seen_ids.insert(req_id);
            }

            // Build a mapping from rule ID to section info by processing elements in order
            use marq::DocElement;
            use std::collections::HashMap;
            let mut rule_sections: HashMap<String, (Option<String>, Option<String>)> =
                HashMap::new();
            let mut current_section: Option<(String, String)> = None; // (slug, title)

            for element in &doc.elements {
                match element {
                    DocElement::Heading(h) => {
                        current_section = Some((h.id.clone(), h.title.clone()));
                    }
                    DocElement::Req(r) => {
                        if let Some((slug, title)) = &current_section {
                            rule_sections.insert(
                                r.id.to_string(),
                                (Some(slug.clone()), Some(title.clone())),
                            );
                        }
                    }
                    DocElement::Paragraph(_) => {}
                }
            }

            // Add requirements with their source file, computed column, and section
            for req in doc.reqs {
                let column = Some(compute_column(&content, req.span.offset));
                let prefix = extract_marker_prefix(&content, req.marker_span).ok_or_else(|| {
                    eyre::eyre!(
                        "Failed to determine requirement marker prefix in {} at line {}",
                        display_path,
                        req.line
                    )
                })?;
                let (section, section_title) = rule_sections
                    .remove(&req.id.to_string())
                    .unwrap_or((None, None));
                rules.push(ExtractedRule {
                    def: req,
                    source_file: display_path.clone(),
                    prefix,
                    column,
                    section,
                    section_title,
                });
            }
        }
    }

    Ok(rules)
}

/// Load rules from multiple glob patterns
pub async fn load_rules_from_globs(
    root: &std::path::Path,
    patterns: &[&str],
    quiet: bool,
) -> Result<Vec<ExtractedRule>> {
    use owo_colors::OwoColorize;
    use std::collections::HashSet;

    let mut all_rules: Vec<ExtractedRule> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    for pattern in patterns {
        let rules = load_rules_from_glob(root, pattern, quiet).await?;

        // r[impl validation.duplicates]
        // Check for duplicates across patterns
        for extracted in rules {
            let def_id = extracted.def.id.to_string();
            if seen_ids.contains(&def_id) {
                eyre::bail!(
                    "Duplicate requirement '{}' found in {}",
                    extracted.def.id.red(),
                    extracted.source_file
                );
            }
            seen_ids.insert(def_id);
            all_rules.push(extracted);
        }
    }

    Ok(all_rules)
}

/// Simple glob pattern matching
fn matches_glob(path: &str, pattern: &str) -> bool {
    // Make path separators consistent in case of windows
    let path = path.replace('\\', "/");
    let pattern = pattern.replace('\\', "/");

    // Handle **/*.md pattern
    if pattern == "**/*.md" {
        return path.ends_with(".md");
    }

    // Handle prefix/**/*.md patterns like "docs/**/*.md"
    if let Some(rest) = pattern.strip_suffix("/**/*.md") {
        return path.starts_with(rest) && path.ends_with(".md");
    }

    // Handle prefix/** patterns
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path.starts_with(prefix);
    }

    // Handle exact matches
    if !pattern.contains('*') {
        return path == pattern;
    }

    // Fallback: simple contains check for the non-wildcard parts
    let parts: Vec<&str> = pattern.split('*').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return true;
    }

    let mut remaining = path.as_str();
    for part in parts {
        if let Some(idx) = remaining.find(part) {
            remaining = &remaining[idx + part.len()..];
        } else {
            return false;
        }
    }

    true
}

pub fn find_project_root() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        if current.join("Cargo.toml").exists() {
            return Ok(current);
        }

        if !current.pop() {
            // No Cargo.toml found, use current directory
            return std::env::current_dir().wrap_err("Failed to get current directory");
        }
    }
}

pub fn load_config(path: &PathBuf) -> Result<Config> {
    if !path.exists() {
        eyre::bail!(
            "Config file not found at {}\n\n\
             Create a config file with your spec configuration:\n\n\
             specs (\n  \
               {{\n    \
                 name my-spec\n    \
                 include (docs/**/*.md)\n    \
                 impls (\n      \
                   {{\n        \
                     name main\n        \
                     include (src/**/*.rs)\n      \
                   }}\n    \
                 )\n  \
               }}\n\
             )",
            path.display()
        );
    }

    let content = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("Failed to read config file: {}", path.display()))?;

    let config: Config = facet_styx::from_str(&content)
        .wrap_err_with(|| format!("Failed to parse config file: {}", path.display()))?;

    Ok(config)
}

/// r[impl config.optional]
/// Load config if it exists, otherwise return default empty config.
/// This allows services to start without a config file.
pub fn load_config_or_default(path: &PathBuf) -> Config {
    if !path.exists() {
        return Config::default();
    }

    match std::fs::read_to_string(path) {
        Ok(content) => facet_styx::from_str(&content).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}
