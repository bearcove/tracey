//! Integration tests for tracey daemon service.
//!
//! These tests verify the daemon service functionality by setting up
//! a test project and exercising the various APIs.

use std::path::PathBuf;
use std::sync::Arc;

use tracey_core::parse_rule_id;
use tracey_proto::*;

// Re-export test modules
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

fn rid(id: &str) -> tracey_core::RuleId {
    parse_rule_id(id).expect("valid rule id")
}

// ============================================================================
// Status API Tests
// ============================================================================

#[tokio::test]
async fn test_status_returns_coverage() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;
    let status = service.status().await;

    // We should have at least one impl
    assert!(!status.impls.is_empty(), "Expected at least one impl");

    // Check that our test spec is present
    let test_impl = status
        .impls
        .iter()
        .find(|i| i.spec == "test" && i.impl_name == "rust");
    assert!(test_impl.is_some(), "Expected test/rust impl");

    let impl_status = test_impl.unwrap();
    assert!(impl_status.total_rules > 0, "Expected some rules");
}

#[tokio::test]
async fn test_status_coverage_percentages() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;
    let status = service.status().await;

    for impl_status in &status.impls {
        // Covered rules should not exceed total
        assert!(
            impl_status.covered_rules <= impl_status.total_rules,
            "Covered rules ({}) exceeds total ({})",
            impl_status.covered_rules,
            impl_status.total_rules
        );

        // Verified rules should not exceed total
        assert!(
            impl_status.verified_rules <= impl_status.total_rules,
            "Verified rules ({}) exceeds total ({})",
            impl_status.verified_rules,
            impl_status.total_rules
        );
    }
}

// ============================================================================
// Uncovered/Untested API Tests
// ============================================================================

#[tokio::test]
async fn test_uncovered_returns_rules() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;
    let req = UncoveredRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: None,
    };

    let response = service.uncovered(req).await;

    assert_eq!(response.spec, "test");
    assert_eq!(response.impl_name, "rust");
    // We have some uncovered rules (data.format, error.logging)
    assert!(
        response.uncovered_count > 0,
        "Expected some uncovered rules"
    );
}

#[tokio::test]
async fn test_uncovered_with_prefix_filter() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;
    let req = UncoveredRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: Some("auth".to_string()),
    };

    let response = service.uncovered(req).await;

    // All returned rules should start with "auth."
    for section in &response.by_section {
        for rule in &section.rules {
            assert!(
                rule.id.base.starts_with("auth."),
                "Rule {} doesn't match prefix filter",
                rule.id
            );
        }
    }
}

#[tokio::test]
async fn test_untested_returns_rules() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;
    let req = UntestedRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: None,
    };

    let response = service.untested(req).await;

    assert_eq!(response.spec, "test");
    assert_eq!(response.impl_name, "rust");
    // auth.session and auth.logout are implemented but not verified
    assert!(response.untested_count > 0, "Expected some untested rules");
}

// ============================================================================
// Rule Details API Tests
// ============================================================================

#[tokio::test]
async fn test_rule_returns_details() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;
    let rule = service.rule(rid("auth.login")).await;

    assert!(rule.is_some(), "Expected auth.login rule to exist");

    let info = rule.unwrap();
    assert_eq!(info.id, rid("auth.login"));
    assert!(!info.raw.is_empty(), "Expected rule raw markdown");
    assert!(
        !info.coverage.is_empty(),
        "Expected coverage info for auth.login"
    );
}

#[tokio::test]
async fn test_rule_not_found() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;
    let rule = service.rule(rid("nonexistent.rule")).await;

    assert!(rule.is_none(), "Expected nonexistent rule to return None");
}

// ============================================================================
// Config API Tests
// ============================================================================

#[tokio::test]
async fn test_config_returns_project_info() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;
    let config = service.config().await;

    assert!(!config.specs.is_empty(), "Expected at least one spec");

    let test_spec = config.specs.iter().find(|s| s.name == "test");
    assert!(test_spec.is_some(), "Expected test spec");

    let spec = test_spec.unwrap();
    assert_eq!(spec.prefix, "r");
    assert!(
        spec.implementations.contains(&"rust".to_string()),
        "Expected rust implementation"
    );
}

// ============================================================================
// LSP API Tests
// ============================================================================

#[tokio::test]
async fn test_lsp_hover_on_reference() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // Content with a reference to auth.login
    let content = r#"/// r[impl auth.login]
fn test_func() {}"#;

    let req = LspPositionRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
        line: 0,
        character: 12, // Position within "auth.login"
    };

    let hover = service.lsp_hover(req).await;

    assert!(hover.is_some(), "Expected hover info for auth.login");

    let info = hover.unwrap();
    assert_eq!(info.rule_id, "auth.login");
    assert!(!info.raw.is_empty(), "Expected rule raw markdown in hover");
}

#[tokio::test]
async fn test_lsp_hover_outside_reference() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    let content = r#"// Just a comment
fn test_func() {}"#;

    let req = LspPositionRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
        line: 1,
        character: 5, // Position in "fn"
    };

    let hover = service.lsp_hover(req).await;

    assert!(hover.is_none(), "Expected no hover info outside reference");
}

#[tokio::test]
async fn test_lsp_definition() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    let content = r#"/// r[impl auth.login]
fn test_func() {}"#;

    let req = LspPositionRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
        line: 0,
        character: 12,
    };

    let locations = service.lsp_definition(req).await;

    assert!(
        !locations.is_empty(),
        "Expected definition location for auth.login"
    );

    // Definition should point to the spec file
    assert!(
        locations[0].path.contains("spec.md"),
        "Expected definition in spec.md"
    );
}

#[tokio::test]
async fn test_lsp_completions() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // Typing "r[impl auth" should suggest auth.* rules
    let content = "/// r[impl auth";

    let req = LspPositionRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
        line: 0,
        character: 15, // After "auth"
    };

    let completions = service.lsp_completions(req).await;

    // Should have some auth.* completions
    let auth_completions: Vec<_> = completions
        .iter()
        .filter(|c| c.label.starts_with("auth."))
        .collect();

    assert!(
        !auth_completions.is_empty(),
        "Expected auth.* completions, got: {:?}",
        completions.iter().map(|c| &c.label).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_lsp_diagnostics_orphaned_reference() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // Reference to non-existent rule
    let content = r#"/// r[impl nonexistent.rule]
fn test_func() {}"#;

    let req = LspDocumentRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
    };

    let diagnostics = service.lsp_diagnostics(req).await;

    // Should have a diagnostic for the orphaned reference
    let orphaned = diagnostics.iter().find(|d| d.code == "orphaned");
    assert!(orphaned.is_some(), "Expected orphaned diagnostic");
}

#[tokio::test]
async fn test_lsp_document_symbols() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    let content = r#"/// r[impl auth.login]
fn login() {}

/// r[impl auth.session]
struct Session {}

/// r[verify auth.login]
#[test]
fn test_login() {}"#;

    let req = LspDocumentRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
    };

    let symbols = service.lsp_document_symbols(req).await;

    // Should have symbols for each reference
    assert!(symbols.len() >= 3, "Expected at least 3 symbols");

    // Check that we have auth.login symbol
    let login_symbol = symbols.iter().find(|s| s.name == "auth.login");
    assert!(
        login_symbol.is_some(),
        "Expected auth.login symbol, got: {:?}",
        symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_lsp_workspace_symbols() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    let symbols = service.lsp_workspace_symbols("auth".to_string()).await;

    // Should have auth.* symbols
    assert!(!symbols.is_empty(), "Expected auth.* symbols");

    for symbol in &symbols {
        assert!(
            symbol.name.to_lowercase().contains("auth"),
            "Symbol {} doesn't match query 'auth'",
            symbol.name
        );
    }
}

#[tokio::test]
async fn test_lsp_references() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    let content = r#"/// r[impl auth.login]
fn login() {}"#;

    let req = LspReferencesRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
        line: 0,
        character: 12,
        include_declaration: true,
    };

    let references = service.lsp_references(req).await;

    // Should have at least the definition and one impl reference
    assert!(!references.is_empty(), "Expected references for auth.login");
}

// ============================================================================
// Validation API Tests
// ============================================================================

#[tokio::test]
async fn test_validate_returns_results() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;
    let req = ValidateRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
    };

    let result = service.validate(req).await;

    assert_eq!(result.spec, "test");
    assert_eq!(result.impl_name, "rust");
    // The fixture has valid data, so should have no errors (or minimal)
}

// ============================================================================
// Semantic Tokens Tests
// ============================================================================

#[tokio::test]
async fn test_lsp_semantic_tokens() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    let content = r#"/// r[impl auth.login]
fn login() {}

/// r[verify auth.login]
#[test]
fn test_login() {}"#;

    let req = LspDocumentRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
    };

    let tokens = service.lsp_semantic_tokens(req).await;

    // Should have tokens for each reference
    assert!(!tokens.is_empty(), "Expected semantic tokens");
}

// ============================================================================
// Code Lens Tests
// ============================================================================

#[tokio::test]
async fn test_lsp_code_lens() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // Code lenses show for r[define ...] references (explicit definition verb)
    // This is useful for defining requirements in Rust code that aren't in markdown
    let content = r#"//! Module documentation
//!
//! r[define auth.login]
//! This defines the login requirement in code.

pub fn login() {}"#;

    let req = LspDocumentRequest {
        path: fixtures_dir().join("src/example.rs").display().to_string(),
        content: content.to_string(),
    };

    let lenses = service.lsp_code_lens(req).await;

    // Should have a code lens for the auth.login definition
    assert!(
        !lenses.is_empty(),
        "Expected code lenses for r[define auth.login]"
    );
    assert_eq!(lenses[0].command, "tracey.showReferences");
}

// ============================================================================
// Multi-Spec Prefix Filtering Tests
// r[verify ref.prefix.filter]
// ============================================================================

#[tokio::test]
async fn test_validate_ignores_other_spec_prefixes() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // @tracey:ignore-start
    // Validate test/rust - should NOT report errors for o[...] references
    // The fixtures/src/lib.rs has both r[impl auth.login] and o[impl api.fetch]
    // @tracey:ignore-end
    let req = ValidateRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
    };

    let result = service.validate(req).await;

    // @tracey:ignore-start
    // Should not have any UnknownRequirement errors for o[impl api.fetch]
    // because that reference belongs to the "other" spec, not "test"
    // @tracey:ignore-end
    let unknown_api_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| {
            e.code == ValidationErrorCode::UnknownRequirement && e.message.contains("api.fetch")
        })
        .collect();

    assert!(
        unknown_api_errors.is_empty(),
        // @tracey:ignore-next-line
        "Validation of test/rust should NOT report errors for o[...] references. \
         Found errors: {:?}",
        unknown_api_errors
    );
}

#[tokio::test]
async fn test_validate_other_spec_validates_its_own_prefix() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // @tracey:ignore-next-line
    // Validate other/rust - should properly validate o[...] references
    let req = ValidateRequest {
        spec: Some("other".to_string()),
        impl_name: Some("rust".to_string()),
    };

    let result = service.validate(req).await;

    // @tracey:ignore-start
    // Should not have UnknownRequirement errors for o[impl api.fetch]
    // because api.fetch exists in the other spec
    // @tracey:ignore-end
    let unknown_api_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| {
            e.code == ValidationErrorCode::UnknownRequirement && e.message.contains("api.fetch")
        })
        .collect();

    assert!(
        unknown_api_errors.is_empty(),
        // @tracey:ignore-next-line
        "Validation of other/rust should NOT report errors for valid o[...] references. \
         Found errors: {:?}",
        unknown_api_errors
    );
}

#[tokio::test]
async fn test_validate_other_spec_ignores_r_prefix() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // @tracey:ignore-start
    // Validate other/rust - should NOT report errors for r[...] references
    // because those belong to the "test" spec
    // @tracey:ignore-end
    let req = ValidateRequest {
        spec: Some("other".to_string()),
        impl_name: Some("rust".to_string()),
    };

    let result = service.validate(req).await;

    // @tracey:ignore-start
    // Should not have UnknownRequirement errors for r[impl auth.login]
    // because that reference belongs to the "test" spec, not "other"
    // @tracey:ignore-end
    let unknown_auth_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| {
            e.code == ValidationErrorCode::UnknownRequirement && e.message.contains("auth.")
        })
        .collect();

    assert!(
        unknown_auth_errors.is_empty(),
        "Validation of other/rust should NOT report errors for r[...] references. \
         Found errors: {:?}",
        unknown_auth_errors
    );
}

#[tokio::test]
async fn test_validate_detects_unknown_rule_in_matching_prefix() {
    use tracey_proto::TraceyDaemon;

    let service = create_test_service().await;

    // Create a test case where a rule ID is wrong for the matching prefix
    // We'll use the existing lsp_diagnostics to check for orphaned references
    // in a synthetic content

    // For the r[...] prefix (test spec), a reference to a non-existent rule should error
    let content = r#"/// r[impl nonexistent.rule]
fn test_func() {}"#;

    let req = LspDocumentRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
    };

    let diagnostics = service.lsp_diagnostics(req).await;

    // Should have a diagnostic for the orphaned reference
    let orphaned = diagnostics.iter().find(|d| d.code == "orphaned");
    assert!(
        orphaned.is_some(),
        "Expected orphaned diagnostic for r[impl nonexistent.rule]"
    );
}
