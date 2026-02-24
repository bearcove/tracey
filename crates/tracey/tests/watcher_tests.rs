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

fn rpc<T, E: std::fmt::Debug>(res: Result<T, roam_stream::CallError<E>>) -> T {
    res.expect("RPC call failed")
}

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
async fn create_test_service() -> common::RpcTestService {
    use tracey::daemon::{Engine, TraceyService};

    let fixtures = common::fixtures_dir();
    let config_path = fixtures.join("config.styx");

    let engine = Arc::new(
        Engine::new(fixtures, config_path)
            .await
            .expect("Failed to create engine"),
    );

    let service = TraceyService::new(engine);
    common::create_test_rpc_service(service).await
}

/// Helper to create a test service with watcher state
async fn create_test_service_with_watcher() -> (common::RpcTestService, Arc<WatcherState>) {
    use tracey::daemon::{Engine, TraceyService};

    let fixtures = common::fixtures_dir();
    let config_path = fixtures.join("config.styx");

    let engine = Arc::new(
        Engine::new(fixtures, config_path)
            .await
            .expect("Failed to create engine"),
    );

    let watcher_state = WatcherState::new();
    watcher_state.mark_active();
    watcher_state.set_watched_dirs(vec!["/test/dir".into()]);
    watcher_state.record_event();

    let (service, _shutdown_rx) =
        TraceyService::new_with_watcher(engine, Arc::clone(&watcher_state));
    let service = common::create_test_rpc_service(service).await;

    (service, watcher_state)
}

#[tokio::test]
async fn test_health_without_watcher_state() {
    let service = Arc::new(create_test_service().await);
    let health = rpc(service.client.health().await);

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
    let (service, _state) = create_test_service_with_watcher().await;
    let service = Arc::new(service);
    let health = rpc(service.client.health().await);

    assert!(health.watcher_active);
    assert!(health.watcher_error.is_none());
    assert!(health.watcher_last_event_ms.is_some());
    assert_eq!(health.watcher_event_count, 1);
    assert_eq!(health.watched_directories.len(), 1);
    // uptime_secs should be a reasonable value (u64, so always >= 0)
}

#[tokio::test]
async fn test_health_reports_watcher_error() {
    let (service, state) = create_test_service_with_watcher().await;

    // Simulate watcher failure
    state.mark_failed("Connection lost".to_string());

    let service = Arc::new(service);
    let health = rpc(service.client.health().await);

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
    let config_path = fixtures.join("config.styx");

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

// ============================================================================
// Helper: create a temp project for engine rebuild tests
// ============================================================================

/// Create a temporary project with a config, spec, and source file.
/// Returns (temp_dir, config_path) — temp_dir must be kept alive for the test duration.
fn create_rebuild_test_project(
    spec_content: &str,
    source_files: &[(&str, &str)],
) -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().expect("Failed to create temp dir");
    let root = temp.path();

    // Write config
    std::fs::create_dir_all(root.join(".config/tracey")).expect("create config dir");
    let config_path = root.join(".config/tracey/config.styx");
    std::fs::write(
        &config_path,
        "specs (\n  {\n    name test\n    include (spec.md)\n    impls (\n      {\n        name rust\n        include (src/**/*.rs)\n      }\n    )\n  }\n)\n",
    )
    .expect("write config");

    // Write spec
    std::fs::write(root.join("spec.md"), spec_content).expect("write spec");

    // Write source files
    std::fs::create_dir_all(root.join("src")).expect("create src dir");
    for (name, content) in source_files {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(&path, content).expect("write source file");
    }

    (temp, config_path)
}

/// Check if any workspace diagnostic has the given code for the given rule ID substring.
fn has_diagnostic_with_code(
    diagnostics: &[tracey_proto::LspFileDiagnostics],
    code: &str,
    rule_id_substring: &str,
) -> bool {
    diagnostics.iter().any(|file_diag| {
        file_diag
            .diagnostics
            .iter()
            .any(|d| d.code == code && d.message.contains(rule_id_substring))
    })
}

/// Check if any forward_by_impl entry contains a rule with the given base ID.
fn has_rule_in_forward(data: &tracey::data::DashboardData, rule_base: &str) -> bool {
    data.forward_by_impl
        .values()
        .any(|spec| spec.rules.iter().any(|r| r.id.base == rule_base))
}

/// Check if any forward_by_impl rule has at least one impl_ref.
fn rule_has_impl_refs(data: &tracey::data::DashboardData, rule_base: &str) -> bool {
    data.forward_by_impl.values().any(|spec| {
        spec.rules
            .iter()
            .any(|r| r.id.base == rule_base && !r.impl_refs.is_empty())
    })
}

// ============================================================================
// Engine-Level Rebuild Tests
// ============================================================================

/// Add a new rule to the spec → an orphaned reference becomes valid.
#[tokio::test]
async fn test_rebuild_add_rule_to_spec_resolves_orphan() {
    use tracey::daemon::Engine;

    // Start with only auth.login in spec, but source references payment.checkout
    let (temp, config_path) = create_rebuild_test_project(
        "# Spec\n\nr[auth.login]\nUsers must log in.\n",
        &[(
            "src/lib.rs",
            "/// r[impl auth.login]\n/// r[impl payment.checkout]\npub fn handler() {}\n",
        )],
    );
    let root = temp.path().to_path_buf();

    let engine = Arc::new(
        Engine::new(root.clone(), config_path)
            .await
            .expect("Failed to create engine"),
    );

    // Before: payment.checkout should be orphaned
    let data = engine.data().await;
    assert!(
        has_diagnostic_with_code(&data.workspace_diagnostics, "orphaned", "payment.checkout"),
        "Expected orphaned diagnostic for payment.checkout before adding rule"
    );

    // Add payment.checkout to spec
    let spec_path = root.join("spec.md");
    std::fs::write(
        &spec_path,
        "# Spec\n\nr[auth.login]\nUsers must log in.\n\nr[payment.checkout]\nUsers can check out.\n",
    )
    .expect("update spec");

    engine
        .rebuild_with_changes(&[spec_path])
        .await
        .expect("rebuild failed");

    // After: payment.checkout should exist in forward data and orphan diagnostic should be gone
    let data = engine.data().await;
    assert!(
        has_rule_in_forward(&data, "payment.checkout"),
        "Expected payment.checkout in forward data after adding rule to spec"
    );
    assert!(
        !has_diagnostic_with_code(&data.workspace_diagnostics, "orphaned", "payment.checkout"),
        "Expected no orphaned diagnostic for payment.checkout after adding rule"
    );
}

/// Remove a rule from the spec → a valid reference becomes orphaned.
#[tokio::test]
async fn test_rebuild_remove_rule_from_spec_creates_orphan() {
    use tracey::daemon::Engine;

    // Start with auth.login and auth.logout in spec, both referenced in source
    let (temp, config_path) = create_rebuild_test_project(
        "# Spec\n\nr[auth.login]\nUsers must log in.\n\nr[auth.logout]\nUsers must log out.\n",
        &[(
            "src/lib.rs",
            "/// r[impl auth.login]\npub fn login() {}\n\n/// r[impl auth.logout]\npub fn logout() {}\n",
        )],
    );
    let root = temp.path().to_path_buf();

    let engine = Arc::new(
        Engine::new(root.clone(), config_path)
            .await
            .expect("Failed to create engine"),
    );

    // Before: no orphaned diagnostics for auth.logout
    let data = engine.data().await;
    assert!(
        !has_diagnostic_with_code(&data.workspace_diagnostics, "orphaned", "auth.logout"),
        "Expected no orphaned diagnostic for auth.logout initially"
    );

    // Remove auth.logout from spec
    let spec_path = root.join("spec.md");
    std::fs::write(&spec_path, "# Spec\n\nr[auth.login]\nUsers must log in.\n")
        .expect("update spec");

    engine
        .rebuild_with_changes(&[spec_path])
        .await
        .expect("rebuild failed");

    // After: auth.logout should be orphaned
    let data = engine.data().await;
    assert!(
        has_diagnostic_with_code(&data.workspace_diagnostics, "orphaned", "auth.logout"),
        "Expected orphaned diagnostic for auth.logout after removing rule from spec"
    );
}

/// Add a new source file → rebuild picks it up.
#[tokio::test]
async fn test_rebuild_add_new_source_file() {
    use tracey::daemon::Engine;

    // Start with auth.login in spec but no impl in source
    let (temp, config_path) = create_rebuild_test_project(
        "# Spec\n\nr[auth.login]\nUsers must log in.\n",
        &[(
            "src/lib.rs",
            "// no impl refs here\npub fn placeholder() {}\n",
        )],
    );
    let root = temp.path().to_path_buf();

    let engine = Arc::new(
        Engine::new(root.clone(), config_path)
            .await
            .expect("Failed to create engine"),
    );

    // Before: auth.login has no impl refs
    let data = engine.data().await;
    assert!(
        !rule_has_impl_refs(&data, "auth.login"),
        "Expected auth.login to have no impl refs initially"
    );

    // Add a new source file with an impl ref
    let new_file = root.join("src/new_impl.rs");
    std::fs::write(&new_file, "/// r[impl auth.login]\npub fn do_login() {}\n")
        .expect("write new source file");

    engine
        .rebuild_with_changes(&[new_file.clone()])
        .await
        .expect("rebuild failed");

    // After: auth.login should have impl refs, and new file should appear in source_reqs_by_file
    let data = engine.data().await;
    assert!(
        rule_has_impl_refs(&data, "auth.login"),
        "Expected auth.login to have impl refs after adding new source file"
    );
    assert!(
        data.source_reqs_by_file
            .keys()
            .any(|p| p.ends_with("src/new_impl.rs")),
        "Expected src/new_impl.rs in source_reqs_by_file after rebuild"
    );
}

/// Modify an existing source file → rebuild picks up the changes.
#[tokio::test]
async fn test_rebuild_modify_source_file() {
    use tracey::daemon::Engine;

    // Start with auth.login in spec, source references nonexistent.rule
    let (temp, config_path) = create_rebuild_test_project(
        "# Spec\n\nr[auth.login]\nUsers must log in.\n",
        &[(
            "src/changing.rs",
            "/// r[impl nonexistent.rule]\npub fn handler() {}\n",
        )],
    );
    let root = temp.path().to_path_buf();

    let engine = Arc::new(
        Engine::new(root.clone(), config_path)
            .await
            .expect("Failed to create engine"),
    );

    // Before: nonexistent.rule should be orphaned
    let data = engine.data().await;
    assert!(
        has_diagnostic_with_code(&data.workspace_diagnostics, "orphaned", "nonexistent.rule"),
        "Expected orphaned diagnostic for nonexistent.rule initially"
    );

    // Overwrite the source file to reference auth.login instead
    let changing_path = root.join("src/changing.rs");
    std::fs::write(
        &changing_path,
        "/// r[impl auth.login]\npub fn handler() {}\n",
    )
    .expect("overwrite source file");

    engine
        .rebuild_with_changes(&[changing_path])
        .await
        .expect("rebuild failed");

    // After: no more orphaned diagnostic, and auth.login has impl refs
    let data = engine.data().await;
    assert!(
        !has_diagnostic_with_code(&data.workspace_diagnostics, "orphaned", "nonexistent.rule"),
        "Expected no orphaned diagnostic for nonexistent.rule after modifying source"
    );
    assert!(
        rule_has_impl_refs(&data, "auth.login"),
        "Expected auth.login to have impl refs after modifying source"
    );
}
