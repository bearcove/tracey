//! Integration tests for LSP diagnostic lifecycle.
//!
//! These tests verify that diagnostics are properly published and cleared
//! during the document lifecycle (open, change, save, close).
//!
//! The key scenarios tested:
//! 1. Opening a file with errors produces diagnostics
//! 2. Fixing errors via VFS change clears diagnostics
//! 3. Workspace diagnostics tracks files that need clearing

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use tracey_proto::*;

mod common;

/// Get the path to the test fixtures directory.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Helper to create an engine for testing.
async fn create_test_engine() -> Arc<tracey::daemon::Engine> {
    let project_root = fixtures_dir();
    let config_path = project_root.join("config.styx");

    Arc::new(
        tracey::daemon::Engine::new(project_root, config_path)
            .await
            .expect("Failed to create engine"),
    )
}

/// Helper to create a service for testing.
async fn create_test_service() -> tracey::daemon::TraceyService {
    let engine = create_test_engine().await;
    tracey::daemon::TraceyService::new(engine)
}

/// Helper to create an isolated test project with its own engine.
async fn create_isolated_test_service() -> (tempfile::TempDir, tracey::daemon::TraceyService) {
    let temp = common::create_temp_project();
    let project_root = temp.path().to_path_buf();
    let config_path = project_root.join("config.styx");

    let engine = Arc::new(
        tracey::daemon::Engine::new(project_root, config_path)
            .await
            .expect("Failed to create engine"),
    );
    let service = tracey::daemon::TraceyService::new(engine);

    (temp, service)
}

// ============================================================================
// Basic Diagnostic Tests
// ============================================================================

/// Test that a file with an orphaned reference produces diagnostics.
#[tokio::test]
async fn test_orphaned_reference_produces_diagnostic() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // Content with a reference to a nonexistent rule
    let content = r#"/// r[impl nonexistent.rule]
fn test_func() {}"#;

    let req = LspDocumentRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
    };

    let diagnostics = service.lsp_diagnostics(req).await;

    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics for orphaned reference"
    );

    let orphaned = diagnostics.iter().find(|d| d.code == "orphaned");
    assert!(orphaned.is_some(), "Expected orphaned diagnostic");
    assert!(
        orphaned.unwrap().message.contains("nonexistent.rule"),
        "Diagnostic should mention the invalid rule ID"
    );
}

/// Test that a file with valid references produces no diagnostics.
#[tokio::test]
async fn test_valid_reference_no_diagnostic() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // Content with a reference to a valid rule (auth.login exists in spec.md)
    let content = r#"/// r[impl auth.login]
fn login_impl() {}"#;

    let req = LspDocumentRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
    };

    let diagnostics = service.lsp_diagnostics(req).await;

    // Should have no error diagnostics (might have hints for coverage)
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == "error" || d.severity == "warning")
        .filter(|d| d.code == "orphaned" || d.code == "unknown-prefix")
        .collect();

    assert!(
        errors.is_empty(),
        "Expected no error diagnostics, got: {:?}",
        errors
    );
}

/// Test that a reference to an older rule version is flagged as stale.
#[tokio::test]
async fn test_stale_reference_produces_stale_diagnostic() {
    use tracey_proto::TraceyDaemon;

    let (temp, service) = create_isolated_test_service().await;

    // Update spec to use a newer version.
    std::fs::write(
        temp.path().join("spec.md"),
        r#"# Versioned Spec

r[auth.login+2]
Users MUST provide valid credentials to log in.
"#,
    )
    .expect("failed to write spec");

    // Rebuild daemon data after changing spec content.
    service.reload().await;

    let content = r#"/// r[impl auth.login]
fn login_impl() {}"#;

    let req = LspDocumentRequest {
        path: temp.path().join("src/vfs_test.rs").display().to_string(),
        content: content.to_string(),
    };

    let diagnostics = service.lsp_diagnostics(req).await;
    let stale = diagnostics.iter().find(|d| d.code == "stale");
    assert!(stale.is_some(), "Expected stale diagnostic");

    let orphaned = diagnostics.iter().find(|d| d.code == "orphaned");
    assert!(
        orphaned.is_none(),
        "Stale references should not be reported as orphaned"
    );
}

// ============================================================================
// VFS + Diagnostic Lifecycle Tests
// ============================================================================

/// Test that VFS open with errors produces diagnostics.
#[tokio::test]
async fn test_vfs_open_with_error_produces_diagnostics() {
    use tracey_proto::TraceyDaemon;

    let (temp, service) = create_isolated_test_service().await;
    let test_file = temp.path().join("src/vfs_test.rs");

    // Content with an orphaned reference
    let content_with_error = r#"/// r[impl typo.nonexistent]
fn broken_func() {}"#;

    // Open file via VFS
    service
        .vfs_open(
            test_file.display().to_string(),
            content_with_error.to_string(),
        )
        .await;

    // Request diagnostics for this file
    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: content_with_error.to_string(),
    };

    let diagnostics = service.lsp_diagnostics(req).await;

    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics after VFS open with error"
    );

    let orphaned = diagnostics.iter().find(|d| d.code == "orphaned");
    assert!(
        orphaned.is_some(),
        "Expected orphaned diagnostic for typo.nonexistent"
    );
}

/// Test that VFS change to fix error clears diagnostics.
///
/// This is the key test for the diagnostic clearing issue:
/// 1. Open a file with an error → produces diagnostic
/// 2. Change the file to fix the error → diagnostic should clear
#[tokio::test]
async fn test_vfs_change_fixes_error_clears_diagnostics() {
    use tracey_proto::TraceyDaemon;

    let (temp, service) = create_isolated_test_service().await;
    let test_file = temp.path().join("src/vfs_test.rs");

    // Step 1: Open file with an error (typo in rule ID)
    let content_with_typo = r#"/// r[impl auth.logn]
fn login_impl() {}"#;

    service
        .vfs_open(
            test_file.display().to_string(),
            content_with_typo.to_string(),
        )
        .await;

    // Verify we have diagnostics
    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: content_with_typo.to_string(),
    };
    let diagnostics_before = service.lsp_diagnostics(req).await;

    assert!(
        !diagnostics_before.is_empty(),
        "Expected diagnostics for typo 'auth.logn'"
    );

    // Step 2: Fix the typo via VFS change
    let content_fixed = r#"/// r[impl auth.login]
fn login_impl() {}"#;

    service
        .vfs_change(test_file.display().to_string(), content_fixed.to_string())
        .await;

    // Step 3: Verify diagnostics are cleared
    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: content_fixed.to_string(),
    };
    let diagnostics_after = service.lsp_diagnostics(req).await;

    // Filter to only error/warning level diagnostics
    let error_diagnostics: Vec<_> = diagnostics_after
        .iter()
        .filter(|d| d.code == "orphaned" || d.code == "unknown-prefix")
        .collect();

    assert!(
        error_diagnostics.is_empty(),
        "Expected diagnostics to be cleared after fixing typo, but got: {:?}",
        error_diagnostics
    );
}

/// Test multiple fix-and-break cycles in the same session.
#[tokio::test]
async fn test_vfs_multiple_fix_break_cycles() {
    use tracey_proto::TraceyDaemon;

    let (temp, service) = create_isolated_test_service().await;
    let test_file = temp.path().join("src/vfs_test.rs");

    // Cycle 1: Start broken
    let broken_v1 = r#"/// r[impl nonexistent.rule1]
fn broken() {}"#;

    service
        .vfs_open(test_file.display().to_string(), broken_v1.to_string())
        .await;

    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: broken_v1.to_string(),
    };
    let diag = service.lsp_diagnostics(req).await;
    assert!(
        !diag.is_empty(),
        "Cycle 1: Expected diagnostics for broken state"
    );

    // Cycle 1: Fix it
    let fixed_v1 = r#"/// r[impl auth.login]
fn working() {}"#;

    service
        .vfs_change(test_file.display().to_string(), fixed_v1.to_string())
        .await;

    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: fixed_v1.to_string(),
    };
    let diag = service.lsp_diagnostics(req).await;
    let errors: Vec<_> = diag.iter().filter(|d| d.code == "orphaned").collect();
    assert!(
        errors.is_empty(),
        "Cycle 1: Expected diagnostics cleared after fix"
    );

    // Cycle 2: Break it again
    let broken_v2 = r#"/// r[impl another.broken.rule]
fn broken_again() {}"#;

    service
        .vfs_change(test_file.display().to_string(), broken_v2.to_string())
        .await;

    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: broken_v2.to_string(),
    };
    let diag = service.lsp_diagnostics(req).await;
    assert!(
        !diag.is_empty(),
        "Cycle 2: Expected diagnostics for broken state"
    );

    // Cycle 2: Fix it again
    let fixed_v2 = r#"/// r[impl auth.session]
fn working_again() {}"#;

    service
        .vfs_change(test_file.display().to_string(), fixed_v2.to_string())
        .await;

    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: fixed_v2.to_string(),
    };
    let diag = service.lsp_diagnostics(req).await;
    let errors: Vec<_> = diag.iter().filter(|d| d.code == "orphaned").collect();
    assert!(
        errors.is_empty(),
        "Cycle 2: Expected diagnostics cleared after fix"
    );
}

// ============================================================================
// Workspace Diagnostics Tests
// ============================================================================

/// Test that workspace diagnostics returns files with issues.
#[tokio::test]
async fn test_workspace_diagnostics_includes_files_with_issues() {
    use tracey_proto::TraceyDaemon;

    let (temp, service) = create_isolated_test_service().await;

    // Create a file with an error
    let test_file = temp.path().join("src/broken.rs");
    let content = r#"/// r[impl nonexistent.broken]
fn broken() {}"#;

    // Write to disk so it's picked up
    std::fs::write(&test_file, content).expect("Failed to write test file");

    // Force rebuild to pick up the new file
    // We'll use vfs_open and immediately close to trigger a rebuild
    service
        .vfs_open(test_file.display().to_string(), content.to_string())
        .await;

    // Get workspace diagnostics
    let workspace_diags = service.lsp_workspace_diagnostics().await;

    // Find diagnostics for our broken file
    let broken_file_diags = workspace_diags
        .iter()
        .find(|fd| fd.path.contains("broken.rs"));

    assert!(
        broken_file_diags.is_some(),
        "Expected workspace diagnostics to include broken.rs, got: {:?}",
        workspace_diags.iter().map(|d| &d.path).collect::<Vec<_>>()
    );
}

/// Test that workspace diagnostics excludes fixed files.
///
/// This tests the daemon service layer - the actual LSP bridge bug
/// is about not publishing empty diagnostics for previously-diagnosed files,
/// but the service layer should correctly return an empty list for fixed files.
#[tokio::test]
async fn test_workspace_diagnostics_excludes_fixed_files() {
    use tracey_proto::TraceyDaemon;

    let (temp, service) = create_isolated_test_service().await;
    let test_file = temp.path().join("src/fixable.rs");

    // Step 1: Create a broken file and verify it appears in workspace diagnostics
    let broken_content = r#"/// r[impl broken.reference]
fn broken() {}"#;

    std::fs::write(&test_file, broken_content).expect("Failed to write test file");
    service
        .vfs_open(test_file.display().to_string(), broken_content.to_string())
        .await;

    let workspace_diags_before = service.lsp_workspace_diagnostics().await;

    let has_broken_file_before = workspace_diags_before
        .iter()
        .any(|fd| fd.path.contains("fixable.rs"));

    assert!(
        has_broken_file_before,
        "Expected fixable.rs in workspace diagnostics before fix"
    );

    // Step 2: Fix the file
    let fixed_content = r#"/// r[impl auth.login]
fn working() {}"#;

    std::fs::write(&test_file, fixed_content).expect("Failed to write test file");
    service
        .vfs_change(test_file.display().to_string(), fixed_content.to_string())
        .await;

    // Step 3: Verify file is no longer in workspace diagnostics
    let workspace_diags_after = service.lsp_workspace_diagnostics().await;

    let has_broken_file_after = workspace_diags_after
        .iter()
        .any(|fd| fd.path.contains("fixable.rs"));

    assert!(
        !has_broken_file_after,
        "Expected fixable.rs to NOT be in workspace diagnostics after fix. Current files: {:?}",
        workspace_diags_after
            .iter()
            .map(|d| &d.path)
            .collect::<Vec<_>>()
    );
}

/// Test to verify the expected behavior for LSP diagnostic clearing.
///
/// This test documents the expected behavior: when a file is fixed,
/// the LSP client should receive an empty diagnostics array to clear
/// any previously shown diagnostics.
///
/// The test tracks which files have had diagnostics published and
/// ensures they would receive updates when fixed.
#[tokio::test]
async fn test_diagnostic_clearing_behavior_documented() {
    use tracey_proto::TraceyDaemon;

    let (temp, service) = create_isolated_test_service().await;
    let test_file = temp.path().join("src/clearing_test.rs");

    // Simulating LSP client state: track files that have received diagnostics
    let mut files_with_published_diagnostics: HashSet<String> = HashSet::new();

    // Step 1: Open file with error
    let broken_content = r#"/// r[impl typo.in.rule.name]
fn broken() {}"#;

    service
        .vfs_open(test_file.display().to_string(), broken_content.to_string())
        .await;

    // Simulate LSP publish_diagnostics call
    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: broken_content.to_string(),
    };
    let diagnostics = service.lsp_diagnostics(req).await;

    if !diagnostics.is_empty() {
        files_with_published_diagnostics.insert(test_file.display().to_string());
    }

    assert!(
        files_with_published_diagnostics.contains(&test_file.display().to_string()),
        "File should be tracked as having diagnostics"
    );

    // Step 2: Fix the file
    let fixed_content = r#"/// r[impl auth.login]
fn working() {}"#;

    service
        .vfs_change(test_file.display().to_string(), fixed_content.to_string())
        .await;

    // Simulate what SHOULD happen in LSP:
    // For each file in files_with_published_diagnostics, we should call lsp_diagnostics
    // and publish the result (even if empty) to clear old diagnostics
    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: fixed_content.to_string(),
    };
    let diagnostics = service.lsp_diagnostics(req).await;

    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == "orphaned")
        .collect();

    // The daemon correctly returns empty diagnostics
    assert!(
        errors.is_empty(),
        "Daemon should return empty diagnostics for fixed file"
    );

    // The LSP bridge should publish this empty list to clear client-side diagnostics
    // (This documents the expected behavior - the actual fix is in lsp.rs)
}

// ============================================================================
// VFS Close Tests
// ============================================================================

/// Test that VFS close doesn't affect diagnostic state - workspace diagnostics persist.
#[tokio::test]
async fn test_vfs_close_preserves_workspace_diagnostics() {
    use tracey_proto::TraceyDaemon;

    let (temp, service) = create_isolated_test_service().await;
    let test_file = temp.path().join("src/close_test.rs");

    // Write file to disk with error
    let content = r#"/// r[impl nonexistent.rule]
fn broken() {}"#;
    std::fs::write(&test_file, content).expect("Failed to write test file");

    // Open the file
    service
        .vfs_open(test_file.display().to_string(), content.to_string())
        .await;

    // Verify we have diagnostics while open
    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: content.to_string(),
    };
    let diagnostics = service.lsp_diagnostics(req).await;
    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics while file is open"
    );

    // Close the file - but the file still exists on disk with errors
    service.vfs_close(test_file.display().to_string()).await;

    // Workspace diagnostics should still include this file since it has errors on disk
    let workspace_diags = service.lsp_workspace_diagnostics().await;

    let has_close_test_file = workspace_diags
        .iter()
        .any(|fd| fd.path.contains("close_test.rs"));

    assert!(
        has_close_test_file,
        "Workspace diagnostics should still include closed file with errors. Got: {:?}",
        workspace_diags.iter().map(|d| &d.path).collect::<Vec<_>>()
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Test diagnostics for a file with multiple errors.
#[tokio::test]
async fn test_multiple_errors_in_file() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // Content with multiple orphaned references
    let content = r#"/// r[impl error.one]
fn first_error() {}

/// r[impl error.two]
fn second_error() {}

/// r[impl error.three]
fn third_error() {}"#;

    let req = LspDocumentRequest {
        path: fixtures_dir()
            .join("src/multi_error.rs")
            .display()
            .to_string(),
        content: content.to_string(),
    };

    let diagnostics = service.lsp_diagnostics(req).await;

    let orphaned_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == "orphaned")
        .collect();

    assert_eq!(
        orphaned_diagnostics.len(),
        3,
        "Expected 3 orphaned diagnostics, got: {:?}",
        orphaned_diagnostics
    );
}

/// Test that fixing one error but leaving others still produces diagnostics.
#[tokio::test]
async fn test_partial_fix_still_has_diagnostics() {
    use tracey_proto::TraceyDaemon;

    let (temp, service) = create_isolated_test_service().await;
    let test_file = temp.path().join("src/partial_fix.rs");

    // Start with two errors
    let content_with_two_errors = r#"/// r[impl broken.one]
fn first() {}

/// r[impl broken.two]
fn second() {}"#;

    service
        .vfs_open(
            test_file.display().to_string(),
            content_with_two_errors.to_string(),
        )
        .await;

    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: content_with_two_errors.to_string(),
    };
    let diagnostics_before = service.lsp_diagnostics(req).await;
    let errors_before: Vec<_> = diagnostics_before
        .iter()
        .filter(|d| d.code == "orphaned")
        .collect();
    assert_eq!(errors_before.len(), 2, "Expected 2 errors initially");

    // Fix only one error
    let content_with_one_error = r#"/// r[impl auth.login]
fn first() {}

/// r[impl broken.two]
fn second() {}"#;

    service
        .vfs_change(
            test_file.display().to_string(),
            content_with_one_error.to_string(),
        )
        .await;

    let req = LspDocumentRequest {
        path: test_file.display().to_string(),
        content: content_with_one_error.to_string(),
    };
    let diagnostics_after = service.lsp_diagnostics(req).await;
    let errors_after: Vec<_> = diagnostics_after
        .iter()
        .filter(|d| d.code == "orphaned")
        .collect();
    assert_eq!(
        errors_after.len(),
        1,
        "Expected 1 error after partial fix, got: {:?}",
        errors_after
    );
}

/// Test unknown prefix diagnostic.
#[tokio::test]
async fn test_unknown_prefix_diagnostic() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // Content with an unknown prefix (not 'r' or 'o')
    let content = r#"/// x[impl some.rule]
fn test_func() {}"#;

    let req = LspDocumentRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
    };

    let diagnostics = service.lsp_diagnostics(req).await;

    let unknown_prefix = diagnostics.iter().find(|d| d.code == "unknown-prefix");
    assert!(
        unknown_prefix.is_some(),
        "Expected unknown-prefix diagnostic for 'x' prefix"
    );
}

// ============================================================================
// LSP Bridge Behavior Simulation Tests
// ============================================================================

/// This test simulates the LSP bridge's publish_workspace_diagnostics behavior
/// to demonstrate and verify the diagnostic clearing mechanism.
///
/// The test shows what the LSP bridge MUST do to properly clear diagnostics:
/// 1. Track which files have been published with diagnostics
/// 2. When refreshing, publish empty diagnostics for files that no longer have issues
#[tokio::test]
async fn test_lsp_bridge_workspace_diagnostics_clearing_simulation() {
    use tracey_proto::TraceyDaemon;

    let (temp, service) = create_isolated_test_service().await;

    // Simulating LSP client state: files that have received non-empty diagnostics
    let mut files_with_diagnostics: HashSet<String> = HashSet::new();

    // Create two files - one with error, one without
    let broken_file = temp.path().join("src/broken.rs");
    let working_file = temp.path().join("src/working.rs");

    let broken_content = r#"/// r[impl nonexistent.rule]
fn broken() {}"#;

    let working_content = r#"/// r[impl auth.login]
fn working() {}"#;

    std::fs::write(&broken_file, broken_content).expect("Failed to write broken file");
    std::fs::write(&working_file, working_content).expect("Failed to write working file");

    // Open both files
    service
        .vfs_open(
            broken_file.display().to_string(),
            broken_content.to_string(),
        )
        .await;
    service
        .vfs_open(
            working_file.display().to_string(),
            working_content.to_string(),
        )
        .await;

    // Simulate initial publish_workspace_diagnostics
    let workspace_diags = service.lsp_workspace_diagnostics().await;

    // Track which files got diagnostics
    for file_diag in &workspace_diags {
        if !file_diag.diagnostics.is_empty() {
            files_with_diagnostics.insert(file_diag.path.clone());
        }
    }

    // Verify broken file is tracked
    assert!(
        files_with_diagnostics
            .iter()
            .any(|p| p.contains("broken.rs")),
        "broken.rs should be tracked as having diagnostics"
    );

    // Now fix the broken file
    let fixed_content = r#"/// r[impl auth.session]
fn now_working() {}"#;

    std::fs::write(&broken_file, fixed_content).expect("Failed to write fixed file");
    service
        .vfs_change(broken_file.display().to_string(), fixed_content.to_string())
        .await;

    // Simulate what SHOULD happen in publish_workspace_diagnostics:
    // Get fresh workspace diagnostics
    let workspace_diags_after = service.lsp_workspace_diagnostics().await;

    // The key insight: workspace_diagnostics only returns files WITH issues
    // It does NOT return the now-fixed file
    let fixed_file_in_results = workspace_diags_after
        .iter()
        .any(|fd| fd.path.contains("broken.rs"));
    assert!(
        !fixed_file_in_results,
        "Fixed file should NOT be in workspace diagnostics results"
    );

    // But we need to clear it! So we must track and publish empty diagnostics.
    // Collect files that are in our tracked set but NOT in the new results
    let workspace_paths: HashSet<_> = workspace_diags_after
        .iter()
        .map(|fd| fd.path.clone())
        .collect();

    let files_to_clear: Vec<_> = files_with_diagnostics
        .iter()
        .filter(|path| !workspace_paths.iter().any(|wp| wp.contains(path.as_str())))
        .cloned()
        .collect();

    // Verify that broken.rs (now fixed) is in the files_to_clear list
    assert!(
        files_to_clear.iter().any(|p| p.contains("broken.rs")),
        "Fixed file should be in files_to_clear list: {:?}",
        files_to_clear
    );

    // For each file to clear, we should call lsp_diagnostics and get an empty list
    for path in &files_to_clear {
        if path.contains("broken.rs") {
            let req = LspDocumentRequest {
                path: broken_file.display().to_string(),
                content: fixed_content.to_string(),
            };
            let diagnostics = service.lsp_diagnostics(req).await;
            let errors: Vec<_> = diagnostics
                .iter()
                .filter(|d| d.code == "orphaned")
                .collect();
            assert!(
                errors.is_empty(),
                "Fixed file should have empty diagnostics"
            );
        }
    }

    // Update tracked files
    files_with_diagnostics.clear();
    for file_diag in &workspace_diags_after {
        if !file_diag.diagnostics.is_empty() {
            files_with_diagnostics.insert(file_diag.path.clone());
        }
    }

    // Verify broken.rs is no longer tracked
    assert!(
        !files_with_diagnostics
            .iter()
            .any(|p| p.contains("broken.rs")),
        "Fixed file should no longer be tracked"
    );
}
