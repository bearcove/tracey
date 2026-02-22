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

fn rpc<T, E: std::fmt::Debug>(res: Result<T, roam_stream::CallError<E>>) -> T {
    res.expect("RPC call failed")
}

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
async fn create_test_service() -> common::RpcTestService {
    let engine = create_test_engine().await;
    let service = tracey::daemon::TraceyService::new(engine);
    common::create_test_rpc_service(service).await
}

/// Helper to create an isolated test project with its own engine.
async fn create_isolated_test_service() -> (tempfile::TempDir, common::RpcTestService) {
    let temp = common::create_temp_project();
    let project_root = temp.path().to_path_buf();
    let config_path = project_root.join("config.styx");

    let engine = Arc::new(
        tracey::daemon::Engine::new(project_root, config_path)
            .await
            .expect("Failed to create engine"),
    );
    let service = tracey::daemon::TraceyService::new(engine);
    let rpc = common::create_test_rpc_service(service).await;
    (temp, rpc)
}

fn rid(id: &str) -> tracey_core::RuleId {
    parse_rule_id(id).expect("valid rule id")
}

// ============================================================================
// Status API Tests
// ============================================================================

#[tokio::test]
async fn test_status_returns_coverage() {
    let service = create_test_service().await;
    let status = rpc(service.client.status().await);

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
    let service = create_test_service().await;
    let status = rpc(service.client.status().await);

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
    let service = create_test_service().await;
    let req = UncoveredRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: None,
    };

    let response = rpc(service.client.uncovered(req).await);

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
    let service = create_test_service().await;
    let req = UncoveredRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: Some("auth".to_string()),
    };

    let response = rpc(service.client.uncovered(req).await);

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
    let service = create_test_service().await;
    let req = UntestedRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: None,
    };

    let response = rpc(service.client.untested(req).await);

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
    let service = create_test_service().await;
    let rule = rpc(service.client.rule(rid("auth.login")).await);

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
    let service = create_test_service().await;
    let rule = rpc(service.client.rule(rid("nonexistent.rule")).await);

    assert!(rule.is_none(), "Expected nonexistent rule to return None");
}

// ============================================================================
// Config API Tests
// ============================================================================

#[tokio::test]
async fn test_config_returns_project_info() {
    let service = create_test_service().await;
    let config = rpc(service.client.config().await);

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

#[tokio::test]
async fn test_daemon_starts_with_semantically_invalid_config() {
    let temp = common::create_temp_project();
    std::fs::write(
        temp.path().join("config.styx"),
        r#"
specs (
  {
    name test
    prefix r
    include (spec.md)
    impls (
      {
        name rust
        include (src/**/*.rs)
      }
    )
  }
)
"#,
    )
    .expect("Failed to write config");

    let engine = Arc::new(
        tracey::daemon::Engine::new(temp.path().to_path_buf(), temp.path().join("config.styx"))
            .await
            .expect("Engine should initialize even with invalid semantic config"),
    );
    let service = tracey::daemon::TraceyService::new(engine);
    let rpc_service = common::create_test_rpc_service(service).await;

    let health = rpc(rpc_service.client.health().await);
    let config_error = health.config_error.unwrap_or_default();
    assert!(
        config_error.contains("deprecated `prefix r`"),
        "Expected config error about deprecated prefix, got: {config_error}"
    );

    // Service remains responsive even when config is invalid.
    let _status = rpc(rpc_service.client.status().await);
}

#[tokio::test]
async fn test_reload_with_semantically_invalid_config_keeps_previous_data() {
    let (temp, service) = create_isolated_test_service().await;
    let before = rpc(service.client.status().await);
    assert!(
        !before.impls.is_empty(),
        "Expected fixture project to have initial coverage data"
    );

    std::fs::write(
        temp.path().join("config.styx"),
        r#"
specs (
  {
    name test
    prefix r
    include (spec.md)
    impls (
      {
        name rust
        include (src/**/*.rs)
      }
    )
  }
)
"#,
    )
    .expect("Failed to write invalid config");

    let _reload = rpc(service.client.reload().await);

    let health = rpc(service.client.health().await);
    let config_error = health.config_error.unwrap_or_default();
    assert!(
        config_error.contains("deprecated `prefix r`"),
        "Expected config error about deprecated prefix, got: {config_error}"
    );

    // Last known good data remains available after failed semantic rebuild.
    let after = rpc(service.client.status().await);
    assert!(
        !after.impls.is_empty(),
        "Expected previous data to remain available after failed rebuild"
    );
}

// ============================================================================
// LSP API Tests
// ============================================================================

#[tokio::test]
async fn test_lsp_hover_on_reference() {
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

    let hover = rpc(service.client.lsp_hover(req).await);

    assert!(hover.is_some(), "Expected hover info for auth.login");

    let info = hover.unwrap();
    assert_eq!(info.rule_id, "auth.login");
    assert!(!info.raw.is_empty(), "Expected rule raw markdown in hover");
}

#[tokio::test]
async fn test_lsp_hover_outside_reference() {
    let service = create_test_service().await;

    let content = r#"// Just a comment
fn test_func() {}"#;

    let req = LspPositionRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
        line: 1,
        character: 5, // Position in "fn"
    };

    let hover = rpc(service.client.lsp_hover(req).await);

    assert!(hover.is_none(), "Expected no hover info outside reference");
}

#[tokio::test]
async fn test_lsp_definition() {
    let service = create_test_service().await;

    let content = r#"/// r[impl auth.login]
fn test_func() {}"#;

    let req = LspPositionRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
        line: 0,
        character: 12,
    };

    let locations = rpc(service.client.lsp_definition(req).await);

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
    let service = create_test_service().await;

    // Typing "r[impl auth" should suggest auth.* rules
    let content = "/// r[impl auth";

    let req = LspPositionRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
        line: 0,
        character: 15, // After "auth"
    };

    let completions = rpc(service.client.lsp_completions(req).await);

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
    let service = create_test_service().await;

    // Reference to non-existent rule
    let content = r#"/// r[impl nonexistent.rule]
fn test_func() {}"#;

    let req = LspDocumentRequest {
        path: fixtures_dir().join("src/test.rs").display().to_string(),
        content: content.to_string(),
    };

    let diagnostics = rpc(service.client.lsp_diagnostics(req).await);

    // Should have a diagnostic for the orphaned reference
    let orphaned = diagnostics.iter().find(|d| d.code == "orphaned");
    assert!(orphaned.is_some(), "Expected orphaned diagnostic");
}

#[tokio::test]
async fn test_lsp_document_symbols() {
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

    let symbols = rpc(service.client.lsp_document_symbols(req).await);

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
    let service = create_test_service().await;

    let symbols = rpc(service
        .client
        .lsp_workspace_symbols("auth".to_string())
        .await);

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

    let references = rpc(service.client.lsp_references(req).await);

    // Should have at least the definition and one impl reference
    assert!(!references.is_empty(), "Expected references for auth.login");
}

// ============================================================================
// Validation API Tests
// ============================================================================

#[tokio::test]
async fn test_validate_returns_results() {
    let service = create_test_service().await;
    let req = ValidateRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
    };

    let result = rpc(service.client.validate(req).await);

    assert_eq!(result.spec, "test");
    assert_eq!(result.impl_name, "rust");
    // The fixture has valid data, so should have no errors (or minimal)
}

// ============================================================================
// Semantic Tokens Tests
// ============================================================================

#[tokio::test]
async fn test_lsp_semantic_tokens() {
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

    let tokens = rpc(service.client.lsp_semantic_tokens(req).await);

    // Should have tokens for each reference
    assert!(!tokens.is_empty(), "Expected semantic tokens");
}

// ============================================================================
// Code Lens Tests
// ============================================================================

#[tokio::test]
async fn test_lsp_code_lens() {
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

    let lenses = rpc(service.client.lsp_code_lens(req).await);

    // Should have a code lens for the auth.login definition
    assert!(
        !lenses.is_empty(),
        "Expected code lenses for r[define auth.login]"
    );
    assert_eq!(lenses[0].command, "tracey.showReferences");
}

// ============================================================================
// Multi-Spec Same-Prefix Filtering Tests
// r[verify ref.prefix.filter+2]
// ============================================================================

#[tokio::test]
async fn test_validate_ignores_other_spec_prefixes() {
    let service = create_test_service().await;

    // @tracey:ignore-start
    // Validate test/rust - should NOT report errors for references owned by other spec
    // The fixtures/src/lib.rs has both r[impl auth.login] and r[impl api.fetch]
    // @tracey:ignore-end
    let req = ValidateRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
    };

    let result = rpc(service.client.validate(req).await);

    // @tracey:ignore-start
    // Should not have any UnknownRequirement errors for r[impl api.fetch]
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
        "Validation of test/rust should NOT report errors for other-spec references. \
         Found errors: {:?}",
        unknown_api_errors
    );
}

#[tokio::test]
async fn test_validate_other_spec_validates_its_own_prefix() {
    let service = create_test_service().await;

    // @tracey:ignore-next-line
    // Validate other/rust - should properly validate its own references
    let req = ValidateRequest {
        spec: Some("other".to_string()),
        impl_name: Some("rust".to_string()),
    };

    let result = rpc(service.client.validate(req).await);

    // @tracey:ignore-start
    // Should not have UnknownRequirement errors for r[impl api.fetch]
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
        "Validation of other/rust should NOT report errors for valid api.fetch references. \
         Found errors: {:?}",
        unknown_api_errors
    );
}

#[tokio::test]
async fn test_validate_other_spec_ignores_r_prefix() {
    let service = create_test_service().await;

    // @tracey:ignore-start
    // Validate other/rust - should NOT report errors for r[...] references
    // because those belong to the "test" spec
    // @tracey:ignore-end
    let req = ValidateRequest {
        spec: Some("other".to_string()),
        impl_name: Some("rust".to_string()),
    };

    let result = rpc(service.client.validate(req).await);

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

    let diagnostics = rpc(service.client.lsp_diagnostics(req).await);

    // Should have a diagnostic for the orphaned reference
    let orphaned = diagnostics.iter().find(|d| d.code == "orphaned");
    assert!(
        orphaned.is_some(),
        "Expected orphaned diagnostic for r[impl nonexistent.rule]"
    );
}

#[tokio::test]
async fn test_validate_handles_multiple_specs_with_distinct_prefixes() {
    let temp = tempfile::tempdir().expect("Failed to create temp dir");
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).expect("Failed to create src dir");
    std::fs::write(
        root.join("config.styx"),
        r#"
specs (
  {
    name alpha
    include (alpha-spec.md)
    impls (
      {
        name rust
        include (src/**/*.rs)
      }
    )
  }
  {
    name beta
    include (beta-spec.md)
    impls (
      {
        name rust
        include (src/**/*.rs)
      }
    )
  }
)
"#,
    )
    .expect("Failed to write config");
    std::fs::write(
        root.join("alpha-spec.md"),
        r#"
> r[alpha.rule]
> Alpha rule definition.
"#,
    )
    .expect("Failed to write alpha spec");
    std::fs::write(
        root.join("beta-spec.md"),
        r#"
> shm[beta.rule]
> Beta rule definition.
"#,
    )
    .expect("Failed to write beta spec");
    std::fs::write(
        root.join("src/lib.rs"),
        r#"
// r[impl alpha.rule]
pub fn alpha_impl() {}

// shm[impl beta.rule]
pub fn beta_impl() {}
"#,
    )
    .expect("Failed to write source file");

    let engine = Arc::new(
        tracey::daemon::Engine::new(root.to_path_buf(), root.join("config.styx"))
            .await
            .expect("Failed to create engine"),
    );
    let service = tracey::daemon::TraceyService::new(engine);
    let rpc_service = common::create_test_rpc_service(service).await;

    let config = rpc(rpc_service.client.config().await);
    let alpha = config
        .specs
        .iter()
        .find(|s| s.name == "alpha")
        .expect("Missing alpha spec");
    let beta = config
        .specs
        .iter()
        .find(|s| s.name == "beta")
        .expect("Missing beta spec");
    assert_eq!(alpha.prefix, "r", "alpha should infer r prefix");
    assert_eq!(beta.prefix, "shm", "beta should infer shm prefix");

    let alpha_result = rpc(rpc_service
        .client
        .validate(ValidateRequest {
            spec: Some("alpha".to_string()),
            impl_name: Some("rust".to_string()),
        })
        .await);
    assert!(
        alpha_result.errors.is_empty(),
        "alpha/rust should validate cleanly, got: {:?}",
        alpha_result.errors
    );

    let beta_result = rpc(rpc_service
        .client
        .validate(ValidateRequest {
            spec: Some("beta".to_string()),
            impl_name: Some("rust".to_string()),
        })
        .await);
    assert!(
        beta_result.errors.is_empty(),
        "beta/rust should validate cleanly, got: {:?}",
        beta_result.errors
    );

    let content = std::fs::read_to_string(root.join("src/lib.rs")).expect("Failed to read source");
    let diagnostics = rpc(rpc_service
        .client
        .lsp_diagnostics(LspDocumentRequest {
            path: root.join("src/lib.rs").display().to_string(),
            content,
        })
        .await);

    let unknown_prefix_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == "unknown-prefix")
        .collect();
    assert!(
        unknown_prefix_errors.is_empty(),
        "Known prefixes r/shm should not produce unknown-prefix diagnostics: {:?}",
        unknown_prefix_errors
    );
}

#[tokio::test]
async fn test_config_infers_distinct_prefixes_from_nested_globs() {
    let temp = tempfile::tempdir().expect("Failed to create temp dir");
    let root = temp.path();

    std::fs::create_dir_all(root.join("docs/content/alpha-spec"))
        .expect("Failed to create alpha spec dir");
    std::fs::create_dir_all(root.join("docs/content/shm-spec"))
        .expect("Failed to create shm spec dir");
    std::fs::create_dir_all(root.join("rust")).expect("Failed to create rust dir");

    std::fs::write(
        root.join("config.styx"),
        r#"
specs (
  {
    name alpha
    include (docs/content/alpha-spec/**/*.md)
    impls (
      {
        name rust
        include (rust/**/*.rs)
      }
    )
  }
  {
    name shm
    include (docs/content/shm-spec/**/*.md)
    impls (
      {
        name rust
        include (rust/**/*.rs)
      }
    )
  }
)
"#,
    )
    .expect("Failed to write config");
    std::fs::write(
        root.join("docs/content/alpha-spec/_index.md"),
        r#"
> r[alpha.rule]
> Alpha rule definition.
"#,
    )
    .expect("Failed to write alpha spec");
    std::fs::write(
        root.join("docs/content/shm-spec/_index.md"),
        r#"
> shm[shm.rule]
> SHM rule definition.
"#,
    )
    .expect("Failed to write shm spec");
    std::fs::write(
        root.join("rust/lib.rs"),
        r#"
// r[impl alpha.rule]
pub fn alpha_impl() {}

// shm[impl shm.rule]
pub fn shm_impl() {}
"#,
    )
    .expect("Failed to write source file");

    let engine = Arc::new(
        tracey::daemon::Engine::new(root.to_path_buf(), root.join("config.styx"))
            .await
            .expect("Failed to create engine"),
    );
    let service = tracey::daemon::TraceyService::new(engine);
    let rpc_service = common::create_test_rpc_service(service).await;

    let config = rpc(rpc_service.client.config().await);
    let alpha = config
        .specs
        .iter()
        .find(|s| s.name == "alpha")
        .expect("Missing alpha spec");
    let shm = config
        .specs
        .iter()
        .find(|s| s.name == "shm")
        .expect("Missing shm spec");

    assert_eq!(alpha.prefix, "r", "alpha should infer r prefix");
    assert_eq!(shm.prefix, "shm", "shm should infer shm prefix");
}
