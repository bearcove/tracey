//! Source providers for rule extraction

use crate::lexer::{Rules, extract_from_content};
use eyre::Result;
use std::path::{Path, PathBuf};

/// Trait for providing source files to extract rules from
pub trait Sources {
    /// Extract rules from all sources
    fn extract(self) -> Result<Rules>;
}

/// Sources from an explicit list of file paths
pub struct PathSources(Vec<PathBuf>);

impl PathSources {
    /// Create from an iterator of paths
    pub fn new(paths: impl IntoIterator<Item = impl Into<PathBuf>>) -> Self {
        Self(paths.into_iter().map(Into::into).collect())
    }
}

impl Sources for PathSources {
    fn extract(self) -> Result<Rules> {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            use std::sync::Mutex;

            let rules_mutex = Mutex::new(Rules::new());

            self.0.par_iter().try_for_each(|path| -> Result<()> {
                let content = std::fs::read_to_string(path)?;
                let mut file_rules = Rules::new();
                extract_from_content(path, &content, &mut file_rules);

                let mut guard = rules_mutex.lock().unwrap();
                guard.extend(file_rules);
                Ok(())
            })?;

            Ok(rules_mutex.into_inner().unwrap())
        }

        #[cfg(not(feature = "parallel"))]
        {
            let mut rules = Rules::new();
            for path in self.0 {
                let content = std::fs::read_to_string(&path)?;
                extract_from_content(&path, &content, &mut rules);
            }
            Ok(rules)
        }
    }
}

/// In-memory sources (useful for testing, WASM, etc.)
pub struct MemorySources(Vec<(PathBuf, String)>);

impl MemorySources {
    /// Create empty memory sources
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Add a file with content
    pub fn add(mut self, path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        self.0.push((path.into(), content.into()));
        self
    }
}

impl Default for MemorySources {
    fn default() -> Self {
        Self::new()
    }
}

impl Sources for MemorySources {
    fn extract(self) -> Result<Rules> {
        let mut rules = Rules::new();
        for (path, content) in self.0 {
            extract_from_content(&path, &content, &mut rules);
        }
        Ok(rules)
    }
}

/// Gitignore-aware directory walker
#[cfg(feature = "walk")]
pub struct WalkSources {
    root: PathBuf,
    include: Vec<String>,
    exclude: Vec<String>,
}

#[cfg(feature = "walk")]
impl WalkSources {
    /// Create a walker for the given root directory
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }

    /// Add include patterns (e.g., `["**/*.rs"]`)
    pub fn include(mut self, patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.include.extend(patterns.into_iter().map(Into::into));
        self
    }

    /// Add exclude patterns (e.g., `["target/**"]`)
    pub fn exclude(mut self, patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.exclude.extend(patterns.into_iter().map(Into::into));
        self
    }
}

#[cfg(feature = "walk")]
impl Sources for WalkSources {
    fn extract(self) -> Result<Rules> {
        use ignore::WalkBuilder;
        use std::sync::Mutex;

        let rules = Mutex::new(Rules::new());

        // Build the walker
        let walker = WalkBuilder::new(&self.root)
            .follow_links(true)
            .hidden(false) // Don't skip hidden files (but .git is in .gitignore)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build_parallel();

        // Process files in parallel using ignore's parallel walker
        walker.run(|| {
            Box::new(|entry| {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => return ignore::WalkState::Continue,
                };

                let path = entry.path();

                // Only .rs files
                if path.extension().is_none_or(|ext| ext != "rs") {
                    return ignore::WalkState::Continue;
                }

                // Check include patterns
                if !self.include.is_empty() && !is_included(path, &self.root, &self.include) {
                    return ignore::WalkState::Continue;
                }

                // Check exclude patterns
                if is_excluded(path, &self.root, &self.exclude) {
                    return ignore::WalkState::Continue;
                }

                // Read and extract
                if let Ok(content) = std::fs::read_to_string(path) {
                    let mut file_rules = Rules::new();
                    extract_from_content(path, &content, &mut file_rules);

                    let mut guard = rules.lock().unwrap();
                    guard.extend(file_rules);
                }

                ignore::WalkState::Continue
            })
        });

        Ok(rules.into_inner().unwrap())
    }
}

#[cfg(feature = "walk")]
fn is_included(path: &Path, root: &Path, patterns: &[String]) -> bool {
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

#[cfg(feature = "walk")]
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

#[cfg(feature = "walk")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_sources() {
        let rules = Rules::extract(
            MemorySources::new()
                .add("foo.rs", "// [impl test.rule]")
                .add("bar.rs", "// [verify other.rule]"),
        )
        .unwrap();

        assert_eq!(rules.len(), 2);
    }
}
