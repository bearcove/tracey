//! Source providers for requirement extraction

use crate::lexer::{Reqs, extract_from_content};
use eyre::Result;
use std::ffi::OsStr;
#[cfg(feature = "walk")]
use std::path::Path;
use std::path::PathBuf;

/// r[impl ref.cross-workspace.missing-paths]
/// Result of extracting requirements, including any warnings about missing files
#[derive(Debug, Default)]
pub struct ExtractionResult {
    pub reqs: Reqs,
    pub warnings: Vec<String>,
}

/// File extensions that tracey knows how to scan for requirement references.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "rs",     // Rust
    "swift",  // Swift
    "ts",     // TypeScript
    "tsx",    // TypeScript JSX
    "js",     // JavaScript
    "jsx",    // JavaScript JSX
    "go",     // Go
    "c",      // C
    "h",      // C headers
    "cpp",    // C++
    "hpp",    // C++ headers
    "cc",     // C++
    "cxx",    // C++
    "m",      // Objective-C
    "mm",     // Objective-C++
    "java",   // Java
    "kt",     // Kotlin
    "kts",    // Kotlin script
    "scala",  // Scala
    "groovy", // Groovy
    "cs",     // C#
    "zig",    // Zig
    "php",    // PHP
    "py",     // Python
    "rb",     // Ruby
    "r",      // R
    "R",      // R (uppercase)
    "dart",   // Dart
    "lua",    // Lua
    "asm",    // Assembly
    "s",      // Assembly
    "S",      // Assembly (uppercase)
    "pl",     // Perl
    "pm",     // Perl module
    "hs",     // Haskell
    "lhs",    // Literate Haskell
    "ex",     // Elixir
    "exs",    // Elixir script
    "erl",    // Erlang
    "hrl",    // Erlang header
    "clj",    // Clojure
    "cljs",   // ClojureScript
    "cljc",   // Clojure common
    "edn",    // EDN
    "fs",     // F#
    "fsi",    // F# script
    "fsx",    // F# script
    "vb",     // Visual Basic
    "vbs",    // VBScript
    "cob",    // COBOL
    "cbl",    // COBOL
    "cpy",    // COBOL copybook
    "jl",     // Julia
    "d",      // D
    "ps1",    // PowerShell
    "psm1",   // PowerShell module
    "psd1",   // PowerShell data
    "cmake",  // CMake
    "ml",     // OCaml
    "mli",    // OCaml interface
    "sh",     // Shell/Bash
    "bash",   // Bash
    "zsh",    // Zsh
];

/// Check if a file extension is supported for scanning
pub fn is_supported_extension(ext: &OsStr) -> bool {
    ext.to_str()
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e))
        .unwrap_or(false)
}

/// Trait for providing source files to extract requirements from
pub trait Sources {
    /// Extract requirements from all sources
    fn extract(self) -> Result<ExtractionResult>;
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
    fn extract(self) -> Result<ExtractionResult> {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            use std::sync::Mutex;

            let reqs_mutex = Mutex::new(Reqs::new());

            self.0.par_iter().try_for_each(|path| -> Result<()> {
                let content = std::fs::read_to_string(path)?;
                let mut file_reqs = Reqs::new();
                extract_from_content(path, &content, &mut file_reqs);

                let mut guard = reqs_mutex.lock().unwrap();
                guard.extend(file_reqs);
                Ok(())
            })?;

            Ok(ExtractionResult {
                reqs: reqs_mutex.into_inner().unwrap(),
                warnings: Vec::new(),
            })
        }

        #[cfg(not(feature = "parallel"))]
        {
            let mut reqs = Reqs::new();
            for path in self.0 {
                let content = std::fs::read_to_string(&path)?;
                extract_from_content(&path, &content, &mut reqs);
            }
            Ok(ExtractionResult {
                reqs,
                warnings: Vec::new(),
            })
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
    fn extract(self) -> Result<ExtractionResult> {
        let mut reqs = Reqs::new();
        for (path, content) in self.0 {
            extract_from_content(&path, &content, &mut reqs);
        }
        Ok(ExtractionResult {
            reqs,
            warnings: Vec::new(),
        })
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
    fn extract(self) -> Result<ExtractionResult> {
        use ignore::WalkBuilder;
        use std::sync::Mutex;

        let reqs = Mutex::new(Reqs::new());
        let warnings = Mutex::new(Vec::new());

        // r[impl ref.cross-workspace.paths]
        // Separate include patterns into local and cross-workspace
        let (local_includes, cross_workspace_includes): (Vec<_>, Vec<_>) =
            self.include.iter().partition(|p| !p.starts_with("../"));

        // Helper to walk a directory with patterns
        let walk_with_patterns = |root: &Path,
                                  include_patterns: &[String],
                                  exclude_patterns: &[String]| {
            // Build the walker
            // r[impl walk.gitignore]
            let walker = WalkBuilder::new(root)
                .follow_links(true)
                .hidden(false) // Don't skip hidden files (but .git is in .gitignore)
                .git_ignore(true)
                .git_global(true)
                .git_exclude(true)
                .build_parallel();

            // Process files in parallel using ignore's parallel walker
            walker.run(|| {
                let reqs_ref = &reqs;
                let include_patterns = include_patterns.to_vec();
                let exclude_patterns = exclude_patterns.to_vec();
                let root = root.to_path_buf();

                Box::new(move |entry| {
                    let entry = match entry {
                        Ok(e) => e,
                        Err(_) => return ignore::WalkState::Continue,
                    };

                    let path = entry.path();

                    // Only supported file extensions
                    if path
                        .extension()
                        .is_none_or(|ext| !is_supported_extension(ext))
                    {
                        return ignore::WalkState::Continue;
                    }

                    // Check include patterns
                    if !include_patterns.is_empty() && !is_included(path, &root, &include_patterns)
                    {
                        return ignore::WalkState::Continue;
                    }

                    // Check exclude patterns
                    if is_excluded(path, &root, &exclude_patterns) {
                        return ignore::WalkState::Continue;
                    }

                    // Read and extract
                    if let Ok(content) = std::fs::read_to_string(path) {
                        let mut file_reqs = Reqs::new();
                        extract_from_content(path, &content, &mut file_reqs);

                        let mut guard = reqs_ref.lock().unwrap();
                        guard.extend(file_reqs);
                    }

                    ignore::WalkState::Continue
                })
            });
        };

        // Walk local patterns with the project root
        if !local_includes.is_empty() || self.include.is_empty() {
            let patterns: Vec<String> = local_includes.iter().map(|s| s.to_string()).collect();
            walk_with_patterns(&self.root, &patterns, &self.exclude);
        }

        // r[impl ref.cross-workspace.path-resolution]
        // Walk cross-workspace patterns
        for pattern in cross_workspace_includes {
            // Extract the base path from the pattern (e.g., "../dodeca" from "../dodeca/**/*.rs")
            let base_path = extract_cross_workspace_base(pattern);
            let resolved_path = self.root.join(&base_path);

            // r[impl ref.cross-workspace.missing-paths]
            // r[impl ref.cross-workspace.graceful-degradation]
            // Check if the path exists
            if !resolved_path.exists() {
                let warning = format!(
                    "Warning: Cross-workspace path not found: {}\n  Pattern: {}",
                    base_path, pattern
                );
                warnings.lock().unwrap().push(warning);
                continue;
            }

            // Create a single-pattern include for this cross-workspace walk
            // We need to adjust the pattern to be relative to the resolved path
            let adjusted_pattern = adjust_pattern_for_root(pattern, &base_path);
            walk_with_patterns(&resolved_path, &[adjusted_pattern], &self.exclude);
        }

        Ok(ExtractionResult {
            reqs: reqs.into_inner().unwrap(),
            warnings: warnings.into_inner().unwrap(),
        })
    }
}

#[cfg(feature = "walk")]
fn is_included(path: &Path, root: &Path, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return true;
    }

    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_str = relative.to_string_lossy().replace('\\', "/");

    for pattern in patterns {
        let pattern = pattern.replace('\\', "/");
        if matches_glob(&relative_str, &pattern) {
            return true;
        }
    }

    false
}

#[cfg(feature = "walk")]
fn is_excluded(path: &Path, root: &Path, patterns: &[String]) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_str = relative.to_string_lossy().replace('\\', "/");

    for pattern in patterns {
        let pattern = pattern.replace('\\', "/");
        if matches_glob(&relative_str, &pattern) {
            return true;
        }
    }

    false
}

#[cfg(feature = "walk")]
fn matches_glob(path: &str, pattern: &str) -> bool {
    assert!(!path.contains('\\'));
    assert!(!pattern.contains('\\'));

    // Handle **/*.ext patterns (e.g., **/*.rs, **/*.swift, **/*.ts)
    if let Some(ext) = pattern.strip_prefix("**/*.") {
        return path.ends_with(&format!(".{}", ext));
    }

    // Handle prefix/**/*.ext patterns (e.g., src/**/*.rs, Sources/**/*.swift)
    if let Some(rest) = pattern.strip_prefix("**/") {
        // Pattern like "**/foo/*.rs" - just check the suffix part
        return matches_glob(path, rest);
    }

    // Handle prefix/** patterns (e.g., target/**)
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path.starts_with(prefix) || path.starts_with(&format!("{}/", prefix));
    }

    // Handle prefix/**/*.ext patterns (e.g., src/**/*.rs)
    if let Some((prefix, suffix)) = pattern.split_once("/**/") {
        if !path.starts_with(prefix) && !path.starts_with(&format!("{}/", prefix)) {
            return false;
        }
        let after_prefix = path.strip_prefix(prefix).unwrap_or(path);
        let after_prefix = after_prefix.strip_prefix('/').unwrap_or(after_prefix);
        return matches_glob(after_prefix, suffix);
    }

    // Handle *.ext patterns (e.g., *.rs)
    if let Some(ext) = pattern.strip_prefix("*.") {
        return path.ends_with(&format!(".{}", ext));
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

/// r[impl ref.cross-workspace.path-resolution]
/// Extract the base directory from a cross-workspace pattern
/// e.g., "../dodeca/crates/bearmark/**/*.rs" -> "../dodeca/crates/bearmark"
#[cfg(feature = "walk")]
fn extract_cross_workspace_base(pattern: &str) -> String {
    // Find the first occurrence of "**" or "*"
    if let Some(wildcard_pos) = pattern.find("**").or_else(|| pattern.find('*')) {
        // Get everything before the wildcard, then trim trailing slash
        let base = &pattern[..wildcard_pos];
        base.trim_end_matches('/').to_string()
    } else {
        // No wildcards, use the pattern as-is
        pattern.to_string()
    }
}

/// Adjust a cross-workspace pattern to be relative to its resolved base
/// e.g., "../dodeca/crates/bearmark/**/*.rs" with base "../dodeca/crates/bearmark" -> "**/*.rs"
#[cfg(feature = "walk")]
fn adjust_pattern_for_root(pattern: &str, base: &str) -> String {
    if let Some(suffix) = pattern.strip_prefix(base) {
        suffix.trim_start_matches('/').to_string()
    } else {
        pattern.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_sources() {
        let result = Reqs::extract(
            MemorySources::new()
                .add("foo.rs", "// r[impl test.req]")
                .add("bar.rs", "// r[verify other.req]"),
        )
        .unwrap();

        assert_eq!(result.reqs.len(), 2);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_memory_sources_swift() {
        let result = Reqs::extract(
            MemorySources::new()
                .add("Foo.swift", "// r[impl swift.req.one]")
                .add("Bar.swift", "/* r[verify swift.req.two] */"),
        )
        .unwrap();

        assert_eq!(result.reqs.len(), 2);
        assert_eq!(result.reqs.references[0].req_id, "swift.req.one");
        assert_eq!(result.reqs.references[1].req_id, "swift.req.two");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_memory_sources_typescript() {
        let result = Reqs::extract(
            MemorySources::new()
                .add("app.ts", "// r[impl ts.req.one]")
                .add("component.tsx", "// r[verify ts.req.two]")
                .add("utils.js", "/* r[impl js.req] */"),
        )
        .unwrap();

        assert_eq!(result.reqs.len(), 3);
        assert_eq!(result.reqs.references[0].req_id, "ts.req.one");
        assert_eq!(result.reqs.references[1].req_id, "ts.req.two");
        assert_eq!(result.reqs.references[2].req_id, "js.req");
    }

    #[test]
    fn test_memory_sources_jsdoc_comments() {
        // JSDoc-style comments (/** */) should work too
        let result = Reqs::extract(MemorySources::new().add(
            "api.ts",
            r#"
                    /**
                     * Handles user authentication.
                     * r[impl auth.login]
                     */
                    function login() {}
                "#,
        ))
        .unwrap();
        let reqs = result.reqs;

        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs.references[0].req_id, "auth.login");
    }

    #[test]
    fn test_memory_sources_php() {
        let result = Reqs::extract(
            MemorySources::new()
                .add("Foo.php", "<?php\n// r[impl php.req.one]")
                .add("Bar.php", "<?php\n/* r[verify php.req.two] */")
                .add("Baz.php", "<?php\n/** r[verify php.req.three] */"),
        )
        .unwrap();

        assert_eq!(result.reqs.len(), 3);
        assert_eq!(result.reqs.references[0].req_id, "php.req.one");
        assert_eq!(result.reqs.references[1].req_id, "php.req.two");
        assert_eq!(result.reqs.references[2].req_id, "php.req.three");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_memory_sources_mixed_languages() {
        let result = Reqs::extract(
            MemorySources::new()
                .add("lib.rs", "// r[impl core.rust]")
                .add("App.swift", "// r[impl core.swift]")
                .add("index.ts", "// r[impl core.typescript]"),
        )
        .unwrap();

        assert_eq!(result.reqs.len(), 3);
    }

    #[test]
    fn test_supported_extensions() {
        use std::ffi::OsStr;

        assert!(is_supported_extension(OsStr::new("rs")));
        assert!(is_supported_extension(OsStr::new("swift")));
        assert!(is_supported_extension(OsStr::new("ts")));
        assert!(is_supported_extension(OsStr::new("tsx")));
        assert!(is_supported_extension(OsStr::new("js")));
        assert!(is_supported_extension(OsStr::new("go")));
        assert!(is_supported_extension(OsStr::new("php")));

        assert!(!is_supported_extension(OsStr::new("md")));
        assert!(!is_supported_extension(OsStr::new("txt")));
        assert!(!is_supported_extension(OsStr::new("json")));
    }

    #[cfg(feature = "walk")]
    mod glob_tests {
        use super::super::matches_glob;

        #[test]
        fn test_matches_glob_star_star_ext() {
            assert!(matches_glob("foo.rs", "**/*.rs"));
            assert!(matches_glob("src/foo.rs", "**/*.rs"));
            assert!(matches_glob("src/bar/baz.rs", "**/*.rs"));
            assert!(!matches_glob("foo.swift", "**/*.rs"));

            assert!(matches_glob("App.swift", "**/*.swift"));
            assert!(matches_glob("Sources/App.swift", "**/*.swift"));
            assert!(!matches_glob("App.rs", "**/*.swift"));

            assert!(matches_glob("index.ts", "**/*.ts"));
            assert!(matches_glob("src/components/Button.tsx", "**/*.tsx"));
        }

        #[test]
        fn test_matches_glob_prefix_star_star() {
            assert!(matches_glob("target/debug/foo", "target/**"));
            assert!(matches_glob("target/release/bar", "target/**"));
            assert!(!matches_glob("src/main.rs", "target/**"));
        }

        #[test]
        fn test_matches_glob_prefix_star_star_ext() {
            assert!(matches_glob("src/main.rs", "src/**/*.rs"));
            assert!(matches_glob("src/foo/bar.rs", "src/**/*.rs"));
            assert!(!matches_glob("tests/main.rs", "src/**/*.rs"));
            assert!(!matches_glob("src/main.swift", "src/**/*.rs"));

            assert!(matches_glob("Sources/App.swift", "Sources/**/*.swift"));
            assert!(!matches_glob("Tests/AppTests.swift", "Sources/**/*.swift"));
        }

        #[test]
        fn test_matches_glob_exact() {
            assert!(matches_glob("foo.rs", "foo.rs"));
            assert!(!matches_glob("bar.rs", "foo.rs"));
        }

        #[test]
        fn test_matches_glob_dashboard_tsx() {
            // Test the specific dashboard pattern that wasn't working
            assert!(matches_glob(
                "crates/tracey/dashboard/src/main.tsx",
                "crates/tracey/dashboard/src/**/*.tsx"
            ));
            assert!(matches_glob(
                "crates/tracey/dashboard/src/router.ts",
                "crates/tracey/dashboard/src/**/*.ts"
            ));
            assert!(matches_glob(
                "crates/tracey/dashboard/src/views/spec.tsx",
                "crates/tracey/dashboard/src/**/*.tsx"
            ));
        }

        #[test]
        fn test_walk_typescript_files() {
            // This test verifies that WalkSources actually finds TypeScript files
            // in the dashboard directory when configured with the right patterns
            use super::super::*;
            use std::path::PathBuf;

            // Get the project root (tracey-core's parent's parent)
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let project_root = manifest_dir.parent().unwrap().parent().unwrap();

            // Check if the dashboard directory exists
            let dashboard_src = project_root.join("crates/tracey/dashboard/src");
            if !dashboard_src.exists() {
                // Skip test if dashboard doesn't exist
                return;
            }

            // Create WalkSources with the same patterns as config.yaml
            let result = Reqs::extract(
                WalkSources::new(project_root)
                    .include([
                        "crates/**/*.rs".to_string(),
                        "crates/tracey/dashboard/src/**/*.ts".to_string(),
                        "crates/tracey/dashboard/src/**/*.tsx".to_string(),
                    ])
                    .exclude(["target/**".to_string()]),
            )
            .unwrap();

            // We should find at least one TypeScript reference
            let ts_refs: Vec<_> = result
                .reqs
                .references
                .iter()
                .filter(|r| r.file.to_string_lossy().contains("dashboard/src"))
                .collect();

            // Print what we found for debugging
            eprintln!("Found {} total references", result.reqs.references.len());
            eprintln!("Found {} TypeScript references:", ts_refs.len());
            for r in &ts_refs {
                eprintln!("  - {} in {:?}", r.req_id, r.file);
            }

            // We added annotations to main.tsx and router.ts, so we should find them
            assert!(
                !ts_refs.is_empty(),
                "Expected to find TypeScript references in dashboard/src, but found none"
            );
        }
    }
}
