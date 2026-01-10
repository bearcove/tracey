//! File watcher with smart directory watching and health monitoring.
//!
//! r[impl daemon.watcher.smart-watch]
//!
//! Instead of watching the entire project root and filtering events,
//! this module extracts directory prefixes from config glob patterns
//! and only watches those specific directories.
//!
//! ## Architecture
//!
//! - `WatcherManager` handles watch setup and reconfiguration
//! - `WatcherState` tracks health status for monitoring
//! - `WatcherEvent` is sent to the rebuild loop
//!
//! ## Reconfiguration
//!
//! r[impl daemon.watcher.reconfigure]
//!
//! When config.kdl or .gitignore changes, the watcher sends a
//! `Reconfigure` event. The rebuild loop then:
//! 1. Rebuilds the gitignore matcher
//! 2. Calls `WatcherManager::reconfigure()` to update watches
//! 3. Triggers a rebuild

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eyre::{Result, WrapErr};
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use tracing::{debug, info, warn};

use crate::config::Config;

// ============================================================================
// Watcher Events
// ============================================================================

/// Events sent from the watcher to the rebuild loop.
#[derive(Debug)]
pub enum WatcherEvent {
    /// Files changed - contains the list of changed file paths.
    FilesChanged(Vec<PathBuf>),

    /// Config or gitignore changed - triggers reconfiguration.
    ///
    /// r[impl daemon.watcher.reconfigure]
    Reconfigure,
}

// ============================================================================
// Watcher State (Health Monitoring)
// ============================================================================

/// Shared state for monitoring watcher health.
///
/// r[impl daemon.health]
///
/// This struct is shared between the watcher thread and the main daemon.
/// It allows the health endpoint to report on watcher status.
pub struct WatcherState {
    /// Whether the watcher is currently active and running.
    active: AtomicBool,

    /// Timestamp of last file change event (millis since UNIX epoch).
    last_event_ms: AtomicU64,

    /// Count of file change events received.
    event_count: AtomicU64,

    /// Currently watched directories (for health reporting).
    watched_dirs: RwLock<Vec<PathBuf>>,

    /// Error message if watcher failed (None if healthy).
    error: RwLock<Option<String>>,
}

impl WatcherState {
    /// Create new watcher state.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            active: AtomicBool::new(false),
            last_event_ms: AtomicU64::new(0),
            event_count: AtomicU64::new(0),
            watched_dirs: RwLock::new(Vec::new()),
            error: RwLock::new(None),
        })
    }

    /// Mark the watcher as active (called on successful startup).
    pub fn mark_active(&self) {
        self.active.store(true, Ordering::SeqCst);
        *self.error.write().unwrap() = None;
    }

    /// Mark the watcher as failed with an error message.
    ///
    /// r[impl daemon.watcher.auto-restart]
    pub fn mark_failed(&self, error: String) {
        self.active.store(false, Ordering::SeqCst);
        *self.error.write().unwrap() = Some(error);
    }

    /// Record that a file change event was received.
    pub fn record_event(&self) {
        self.event_count.fetch_add(1, Ordering::SeqCst);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_event_ms.store(now, Ordering::SeqCst);
    }

    /// Update the list of watched directories.
    pub fn set_watched_dirs(&self, dirs: Vec<PathBuf>) {
        *self.watched_dirs.write().unwrap() = dirs;
    }

    /// Check if the watcher is currently active.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Get the last event timestamp (millis since epoch), or None if no events.
    pub fn last_event_ms(&self) -> Option<u64> {
        let ms = self.last_event_ms.load(Ordering::SeqCst);
        if ms == 0 { None } else { Some(ms) }
    }

    /// Get the total event count.
    pub fn event_count(&self) -> u64 {
        self.event_count.load(Ordering::SeqCst)
    }

    /// Get a copy of the watched directories.
    pub fn watched_dirs(&self) -> Vec<PathBuf> {
        self.watched_dirs.read().unwrap().clone()
    }

    /// Get the error message, if any.
    pub fn error(&self) -> Option<String> {
        self.error.read().unwrap().clone()
    }
}

impl Default for WatcherState {
    fn default() -> Self {
        Self {
            active: AtomicBool::new(false),
            last_event_ms: AtomicU64::new(0),
            event_count: AtomicU64::new(0),
            watched_dirs: RwLock::new(Vec::new()),
            error: RwLock::new(None),
        }
    }
}

// ============================================================================
// Glob Pattern Utilities
// ============================================================================

/// Extract the directory prefix from a glob pattern.
///
/// r[impl daemon.watcher.smart-watch]
///
/// This finds the longest path prefix before any glob metacharacter.
///
/// # Examples
///
/// ```ignore
/// glob_to_watch_dir("foo/bar/**/*.rs") => "foo/bar"
/// glob_to_watch_dir("src/*.rs") => "src"
/// glob_to_watch_dir("*.rs") => "."
/// glob_to_watch_dir("docs/spec/**/*.md") => "docs/spec"
/// ```
pub fn glob_to_watch_dir(pattern: &str) -> PathBuf {
    let mut result = PathBuf::new();

    for component in Path::new(pattern).components() {
        let s = component.as_os_str().to_string_lossy();
        // Stop at the first component containing glob metacharacters
        if s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{') {
            break;
        }
        result.push(component);
    }

    // If no prefix was found (pattern starts with glob), watch current directory
    if result.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        result
    }
}

/// Extract all watch directories from a config.
///
/// This collects directory prefixes from:
/// - Spec include patterns (markdown files)
/// - Impl include patterns (source files)
/// - Impl test_include patterns (test files)
///
/// The result is deduplicated and sorted.
pub fn extract_watch_dirs_from_config(config: &Config, project_root: &Path) -> HashSet<PathBuf> {
    let mut dirs = HashSet::new();

    for spec in &config.specs {
        // Spec include patterns (e.g., "docs/spec/**/*.md")
        for include in &spec.include {
            let dir = glob_to_watch_dir(&include.pattern);
            let full_path = project_root.join(&dir);
            if full_path.exists() {
                dirs.insert(full_path);
            } else {
                debug!(
                    "Watch directory does not exist (yet): {}",
                    full_path.display()
                );
            }
        }

        // Impl include and test_include patterns
        for impl_ in &spec.impls {
            for include in &impl_.include {
                let dir = glob_to_watch_dir(&include.pattern);
                let full_path = project_root.join(&dir);
                if full_path.exists() {
                    dirs.insert(full_path);
                }
            }

            for test_include in &impl_.test_include {
                let dir = glob_to_watch_dir(&test_include.pattern);
                let full_path = project_root.join(&dir);
                if full_path.exists() {
                    dirs.insert(full_path);
                }
            }
        }
    }

    dirs
}

// ============================================================================
// Watcher Manager
// ============================================================================

/// Manages file watching with dynamic reconfiguration.
///
/// r[impl daemon.watcher.smart-watch]
/// r[impl daemon.watcher.reconfigure]
pub struct WatcherManager {
    /// The underlying debounced watcher.
    debouncer: Debouncer<RecommendedWatcher>,

    /// Currently watched directories.
    watched_dirs: HashSet<PathBuf>,

    /// Project root for resolving relative paths.
    project_root: PathBuf,

    /// Config file path (always watched).
    config_path: PathBuf,

    /// Gitignore path (always watched if exists).
    gitignore_path: PathBuf,
}

impl WatcherManager {
    /// Create a new watcher manager.
    ///
    /// The watcher starts with no directories watched. Call `reconfigure()`
    /// after creation to set up watches based on config.
    pub fn new<F>(
        project_root: PathBuf,
        config_path: PathBuf,
        debounce_duration: Duration,
        event_handler: F,
    ) -> Result<Self>
    where
        F: Fn(DebounceEventResult) + Send + 'static,
    {
        let debouncer = new_debouncer(debounce_duration, event_handler)
            .wrap_err("Failed to create file watcher")?;

        let gitignore_path = project_root.join(".gitignore");

        let mut manager = Self {
            debouncer,
            watched_dirs: HashSet::new(),
            project_root,
            config_path,
            gitignore_path,
        };

        // Always watch config file
        manager.watch_static_paths()?;

        Ok(manager)
    }

    /// Watch static paths that should always be monitored.
    fn watch_static_paths(&mut self) -> Result<()> {
        // Watch config file
        if self.config_path.exists() {
            self.debouncer
                .watcher()
                .watch(&self.config_path, RecursiveMode::NonRecursive)
                .wrap_err_with(|| {
                    format!(
                        "Failed to watch config file: {}",
                        self.config_path.display()
                    )
                })?;
            info!("Watching config file: {}", self.config_path.display());
        }

        // Watch .gitignore if it exists
        if self.gitignore_path.exists() {
            self.debouncer
                .watcher()
                .watch(&self.gitignore_path, RecursiveMode::NonRecursive)
                .wrap_err_with(|| {
                    format!(
                        "Failed to watch gitignore file: {}",
                        self.gitignore_path.display()
                    )
                })?;
            info!("Watching gitignore: {}", self.gitignore_path.display());
        }

        Ok(())
    }

    /// Reconfigure watches based on config patterns.
    ///
    /// r[impl daemon.watcher.reconfigure]
    ///
    /// This is called on startup and whenever config changes.
    /// It computes the new set of watch directories, removes watches
    /// for directories no longer needed, and adds watches for new ones.
    pub fn reconfigure(&mut self, config: &Config) -> Result<()> {
        let new_dirs = extract_watch_dirs_from_config(config, &self.project_root);

        // Find directories to remove and add
        let to_remove: Vec<_> = self.watched_dirs.difference(&new_dirs).cloned().collect();
        let to_add: Vec<_> = new_dirs.difference(&self.watched_dirs).cloned().collect();

        // Remove old watches
        for dir in &to_remove {
            match self.debouncer.watcher().unwatch(dir) {
                Ok(()) => {
                    debug!("Stopped watching: {}", dir.display());
                }
                Err(e) => {
                    // Not fatal - directory might have been deleted
                    debug!(
                        "Failed to unwatch {} (may be deleted): {}",
                        dir.display(),
                        e
                    );
                }
            }
        }

        // Add new watches
        for dir in &to_add {
            match self
                .debouncer
                .watcher()
                .watch(dir, RecursiveMode::Recursive)
            {
                Ok(()) => {
                    info!("Watching directory: {}", dir.display());
                }
                Err(e) => {
                    warn!("Failed to watch {}: {}", dir.display(), e);
                    // Continue - don't fail the whole reconfigure for one directory
                }
            }
        }

        self.watched_dirs = new_dirs;

        // Log summary
        if !to_remove.is_empty() || !to_add.is_empty() {
            info!(
                "Reconfigured watcher: {} directories ({} added, {} removed)",
                self.watched_dirs.len(),
                to_add.len(),
                to_remove.len()
            );
        }

        Ok(())
    }

    /// Get the currently watched directories (for health reporting).
    pub fn watched_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<_> = self.watched_dirs.iter().cloned().collect();
        dirs.sort();
        dirs
    }

    /// Check if a path is the config file.
    pub fn is_config_path(&self, path: &Path) -> bool {
        path == self.config_path
    }

    /// Check if a path is the gitignore file.
    pub fn is_gitignore_path(&self, path: &Path) -> bool {
        path == self.gitignore_path
    }

    /// Check if a path should trigger reconfiguration (config or gitignore).
    pub fn is_reconfigure_trigger(&self, path: &Path) -> bool {
        self.is_config_path(path) || self.is_gitignore_path(path)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_to_watch_dir_with_double_star() {
        assert_eq!(
            glob_to_watch_dir("foo/bar/**/*.rs"),
            PathBuf::from("foo/bar")
        );
    }

    #[test]
    fn test_glob_to_watch_dir_with_single_star() {
        assert_eq!(glob_to_watch_dir("src/*.rs"), PathBuf::from("src"));
    }

    #[test]
    fn test_glob_to_watch_dir_root_pattern() {
        assert_eq!(glob_to_watch_dir("*.rs"), PathBuf::from("."));
    }

    #[test]
    fn test_glob_to_watch_dir_deep_path() {
        assert_eq!(
            glob_to_watch_dir("docs/spec/**/*.md"),
            PathBuf::from("docs/spec")
        );
    }

    #[test]
    fn test_glob_to_watch_dir_literal_path() {
        // A literal path without globs returns the full path
        assert_eq!(glob_to_watch_dir("src/lib.rs"), PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn test_glob_to_watch_dir_question_mark() {
        assert_eq!(glob_to_watch_dir("src/?.rs"), PathBuf::from("src"));
    }

    #[test]
    fn test_glob_to_watch_dir_brackets() {
        assert_eq!(glob_to_watch_dir("src/[abc].rs"), PathBuf::from("src"));
    }

    #[test]
    fn test_glob_to_watch_dir_braces() {
        assert_eq!(glob_to_watch_dir("src/{foo,bar}.rs"), PathBuf::from("src"));
    }

    #[test]
    fn test_watcher_state_lifecycle() {
        let state = WatcherState::new();

        // Initial state
        assert!(!state.is_active());
        assert!(state.error().is_none());
        assert_eq!(state.event_count(), 0);
        assert!(state.last_event_ms().is_none());

        // Mark active
        state.mark_active();
        assert!(state.is_active());
        assert!(state.error().is_none());

        // Record event
        state.record_event();
        assert_eq!(state.event_count(), 1);
        assert!(state.last_event_ms().is_some());

        // Mark failed
        state.mark_failed("test error".to_string());
        assert!(!state.is_active());
        assert_eq!(state.error(), Some("test error".to_string()));

        // Mark active clears error
        state.mark_active();
        assert!(state.is_active());
        assert!(state.error().is_none());
    }

    #[test]
    fn test_watcher_state_watched_dirs() {
        let state = WatcherState::new();

        assert!(state.watched_dirs().is_empty());

        state.set_watched_dirs(vec![PathBuf::from("/foo"), PathBuf::from("/bar")]);

        let dirs = state.watched_dirs();
        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(&PathBuf::from("/foo")));
        assert!(dirs.contains(&PathBuf::from("/bar")));
    }
}
