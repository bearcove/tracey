//! Source providers for rule extraction

use crate::lexer::{Rules, extract_from_content};
use eyre::Result;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// File extensions that tracey knows how to scan for rule references.
/// These all use `//` and `/* */` comment syntax.
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
];

/// Check if a file extension is supported for scanning
pub fn is_supported_extension(ext: &OsStr) -> bool {
    ext.to_str()
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e))
        .unwrap_or(false)
}

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
        // [impl walk.gitignore]
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

                // Only supported file extensions
                if path
                    .extension()
                    .is_none_or(|ext| !is_supported_extension(ext))
                {
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

    #[test]
    fn test_memory_sources_swift() {
        let rules = Rules::extract(
            MemorySources::new()
                .add("Foo.swift", "// [impl swift.rule.one]")
                .add("Bar.swift", "/* [verify swift.rule.two] */"),
        )
        .unwrap();

        assert_eq!(rules.len(), 2);
        assert_eq!(rules.references[0].rule_id, "swift.rule.one");
        assert_eq!(rules.references[1].rule_id, "swift.rule.two");
    }

    #[test]
    fn test_memory_sources_typescript() {
        let rules = Rules::extract(
            MemorySources::new()
                .add("app.ts", "// [impl ts.rule.one]")
                .add("component.tsx", "// [verify ts.rule.two]")
                .add("utils.js", "/* [impl js.rule] */"),
        )
        .unwrap();

        assert_eq!(rules.len(), 3);
        assert_eq!(rules.references[0].rule_id, "ts.rule.one");
        assert_eq!(rules.references[1].rule_id, "ts.rule.two");
        assert_eq!(rules.references[2].rule_id, "js.rule");
    }

    #[test]
    fn test_memory_sources_jsdoc_comments() {
        // JSDoc-style comments (/** */) should work too
        let rules = Rules::extract(MemorySources::new().add(
            "api.ts",
            r#"
                    /**
                     * Handles user authentication.
                     * [impl auth.login]
                     */
                    function login() {}
                "#,
        ))
        .unwrap();

        assert_eq!(rules.len(), 1);
        assert_eq!(rules.references[0].rule_id, "auth.login");
    }

    #[test]
    fn test_memory_sources_mixed_languages() {
        let rules = Rules::extract(
            MemorySources::new()
                .add("lib.rs", "// [impl core.rust]")
                .add("App.swift", "// [impl core.swift]")
                .add("index.ts", "// [impl core.typescript]"),
        )
        .unwrap();

        assert_eq!(rules.len(), 3);
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
    }
}
