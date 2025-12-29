//! File system scanner for Rust files

use crate::lexer::{RuleReference, extract_rule_references};
use eyre::Result;
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::path::Path;

/// Scan a directory for Rust files and extract all rule references
pub fn scan_directory(
    root: &Path,
    include_patterns: &[String],
    exclude_patterns: &[String],
) -> Result<Vec<RuleReference>> {
    // Collect all matching .rs files first
    let rust_files: Vec<_> = WalkBuilder::new(root)
        .follow_links(true)
        .hidden(false) // Don't skip hidden files (but .git is in .gitignore)
        .git_ignore(true) // Respect .gitignore
        .git_global(true) // Respect global gitignore
        .git_exclude(true) // Respect .git/info/exclude
        .build()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            // Only .rs files
            path.extension().is_some_and(|ext| ext == "rs")
                && !is_excluded(path, root, exclude_patterns)
                && is_included(path, root, include_patterns)
        })
        .map(|entry| entry.into_path())
        .collect();

    // Process files in parallel
    let results: Vec<Result<Vec<RuleReference>>> = rust_files
        .par_iter()
        .map(|path| {
            let content = std::fs::read_to_string(path)?;
            extract_rule_references(path, &content)
        })
        .collect();

    // Collect all references, propagating any errors
    let mut all_references = Vec::new();
    for result in results {
        all_references.extend(result?);
    }

    Ok(all_references)
}

/// Check if a path matches any include pattern
fn is_included(path: &Path, root: &Path, patterns: &[String]) -> bool {
    // If no patterns specified, include everything
    if patterns.is_empty() {
        return true;
    }

    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_str = relative.to_string_lossy();

    for pattern in patterns {
        if matches_glob(&relative_str, pattern) {
            return true;
        }
    }

    false
}

/// Check if a path matches any exclude pattern
fn is_excluded(path: &Path, root: &Path, patterns: &[String]) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_str = relative.to_string_lossy();

    for pattern in patterns {
        if matches_glob(&relative_str, pattern) {
            return true;
        }
    }

    false
}

/// Simple glob matching (supports * and **)
fn matches_glob(path: &str, pattern: &str) -> bool {
    // Handle the common case of **/*.rs
    if pattern == "**/*.rs" {
        return path.ends_with(".rs");
    }

    // Handle target/** exclusion
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path.starts_with(prefix);
    }

    // Fallback: simple contains check for the non-wildcard parts
    let parts: Vec<&str> = pattern.split('*').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return true;
    }

    let mut remaining = path;
    for part in parts {
        if let Some(idx) = remaining.find(part) {
            remaining = &remaining[idx + part.len()..];
        } else {
            return false;
        }
    }

    true
}
