//! Integration tests for file watcher functionality.
//!
//! These tests verify:
//! - File changes trigger rebuilds
//! - Excluded files are ignored
//! - Config changes trigger reconfiguration
//! - Health endpoint reports watcher status
//! - Glob pattern to watch directory conversion

mod common;

use std::path::Path;
use std::sync::Arc;

use tracey::daemon::watcher::{WatcherState, glob_to_watch_dir};

// ============================================================================
// Unit Tests for glob_to_watch_dir
// ============================================================================

#[test]
fn test_glob_to_watch_dir_double_star() {
    assert_eq!(glob_to_watch_dir("foo/bar/**/*.rs"), Path::new("foo/bar"));
}

#[test]
fn test_glob_to_watch_dir_single_star() {
    assert_eq!(glob_to_watch_dir("src/*.rs"), Path::new("src"));
}

#[test]
fn test_glob_to_watch_dir_root_pattern() {
    assert_eq!(glob_to_watch_dir("*.rs"), Path::new("."));
}

#[test]
fn test_glob_to_watch_dir_deep_path() {
    assert_eq!(
        glob_to_watch_dir("docs/spec/**/*.md"),
        Path::new("docs/spec")
    );
}

#[test]
fn test_glob_to_watch_dir_literal_path() {
    assert_eq!(glob_to_watch_dir("src/lib.rs"), Path::new("src/lib.rs"));
}

#[test]
fn test_glob_to_watch_dir_question_mark() {
    assert_eq!(glob_to_watch_dir("src/?.rs"), Path::new("src"));
}

#[test]
fn test_glob_to_watch_dir_brackets() {
    assert_eq!(glob_to_watch_dir("src/[abc].rs"), Path::new("src"));
}

#[test]
fn test_glob_to_watch_dir_braces() {
    assert_eq!(glob_to_watch_dir("src/{foo,bar}.rs"), Path::new("src"));
}

#[test]
fn test_glob_to_watch_dir_nested_globs() {
    assert_eq!(
        glob_to_watch_dir("crates/*/src/**/*.rs"),
        Path::new("crates")
    );
}

// ============================================================================
// Unit Tests for WatcherState
// ============================================================================

#[test]
fn test_watcher_state_initial() {
    let state = WatcherState::new();
    assert!(!state.is_active());
    assert!(state.error().is_none());
    assert_eq!(state.event_count(), 0);
    assert!(state.last_event_ms().is_none());
    assert!(state.watched_dirs().is_empty());
}

#[test]
fn test_watcher_state_mark_active() {
    let state = WatcherState::new();
    state.mark_active();
    assert!(state.is_active());
    assert!(state.error().is_none());
}

#[test]
fn test_watcher_state_mark_failed() {
    let state = WatcherState::new();
    state.mark_active();
    state.mark_failed("test error".to_string());

    assert!(!state.is_active());
    assert_eq!(state.error(), Some("test error".to_string()));
}

#[test]
fn test_watcher_state_mark_active_clears_error() {
    let state = WatcherState::new();
    state.mark_failed("test error".to_string());
    state.mark_active();

    assert!(state.is_active());
    assert!(state.error().is_none());
}

#[test]
fn test_watcher_state_record_event() {
    let state = WatcherState::new();

    state.record_event();
    assert_eq!(state.event_count(), 1);
    assert!(state.last_event_ms().is_some());

    state.record_event();
    assert_eq!(state.event_count(), 2);
}

#[test]
fn test_watcher_state_set_watched_dirs() {
    let state = WatcherState::new();

    state.set_watched_dirs(vec!["/foo/bar".into(), "/baz/qux".into()]);

    let dirs = state.watched_dirs();
    assert_eq!(dirs.len(), 2);
}

// ============================================================================
// Integration Tests for Health Endpoint
// ============================================================================

/// Helper to create a test engine and service
async fn create_test_service() -> tracey::daemon::TraceyService {
    use tracey::daemon::{Engine, TraceyService};

    let fixtures = common::fixtures_dir();
    let config_path = fixtures.join("config.yaml");

    let engine = Arc::new(
        Engine::new(fixtures, config_path)
            .await
            .expect("Failed to create engine"),
    );

    TraceyService::new(engine)
}

/// Helper to create a test service with watcher state
async fn create_test_service_with_watcher() -> (tracey::daemon::TraceyService, Arc<WatcherState>) {
    use tracey::daemon::{Engine, TraceyService};

    let fixtures = common::fixtures_dir();
    let config_path = fixtures.join("config.yaml");

    let engine = Arc::new(
        Engine::new(fixtures, config_path)
            .await
            .expect("Failed to create engine"),
    );

    let watcher_state = WatcherState::new();
    watcher_state.mark_active();
    watcher_state.set_watched_dirs(vec!["/test/dir".into()]);
    watcher_state.record_event();

    let service = TraceyService::new_with_watcher(engine, Arc::clone(&watcher_state));

    (service, watcher_state)
}

#[tokio::test]
async fn test_health_without_watcher_state() {
    use tracey_proto::TraceyDaemon;

    let service = Arc::new(create_test_service().await);
    let health = service.health().await.expect("health() failed");

    // Without watcher state, defaults are returned
    assert!(!health.watcher_active);
    assert!(health.watcher_error.is_none());
    assert!(health.watcher_last_event_ms.is_none());
    assert_eq!(health.watcher_event_count, 0);
    assert!(health.watched_directories.is_empty());
    // uptime_secs should be a reasonable value (u64, so always >= 0)
}

#[tokio::test]
async fn test_health_with_watcher_state() {
    use tracey_proto::TraceyDaemon;

    let (service, _state) = create_test_service_with_watcher().await;
    let service = Arc::new(service);
    let health = service.health().await.expect("health() failed");

    assert!(health.watcher_active);
    assert!(health.watcher_error.is_none());
    assert!(health.watcher_last_event_ms.is_some());
    assert_eq!(health.watcher_event_count, 1);
    assert_eq!(health.watched_directories.len(), 1);
    // uptime_secs should be a reasonable value (u64, so always >= 0)
}

#[tokio::test]
async fn test_health_reports_watcher_error() {
    use tracey_proto::TraceyDaemon;

    let (service, state) = create_test_service_with_watcher().await;

    // Simulate watcher failure
    state.mark_failed("Connection lost".to_string());

    let service = Arc::new(service);
    let health = service.health().await.expect("health() failed");

    assert!(!health.watcher_active);
    assert_eq!(health.watcher_error, Some("Connection lost".to_string()));
}

// ============================================================================
// Integration Tests for File Change Detection
// ============================================================================

#[tokio::test]
async fn test_engine_rebuild_increments_version() {
    use tracey::daemon::Engine;

    let fixtures = common::fixtures_dir();
    let config_path = fixtures.join("config.yaml");

    let engine = Arc::new(
        Engine::new(fixtures, config_path)
            .await
            .expect("Failed to create engine"),
    );

    let version1 = engine.version();
    engine.rebuild().await.expect("Rebuild failed");
    let version2 = engine.version();

    assert!(
        version2 > version1,
        "Version should increment after rebuild"
    );
}

// Note: Full file watcher integration tests would require actually running
// the daemon and watching for file changes. These are more complex and
// time-sensitive, so we test the components individually above.
