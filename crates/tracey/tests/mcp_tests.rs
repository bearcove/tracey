//! MCP integration tests for tracey.
//!
//! These tests verify the MCP tool functionality by testing the underlying
//! daemon service methods that MCP tools call.

use std::path::PathBuf;
use std::sync::Arc;

use tracey_core::parse_rule_id;
use tracey_proto::*;

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

fn rid(id: &str) -> tracey_core::RuleId {
    parse_rule_id(id).expect("valid rule id")
}

// ============================================================================
// tracey_status Tool Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_status_tool() {
    let service = create_test_service().await;
    let status = rpc(service.client.status().await);

    // Verify we get coverage information
    assert!(!status.impls.is_empty(), "Expected at least one impl");

    // Check that test/rust is present
    let test_impl = status
        .impls
        .iter()
        .find(|i| i.spec == "test" && i.impl_name == "rust");
    assert!(test_impl.is_some(), "Expected test/rust impl");

    // Verify coverage metrics are reasonable
    let impl_status = test_impl.unwrap();
    assert!(impl_status.total_rules > 0);
    assert!(impl_status.impl_rules <= impl_status.total_rules);
}

// ============================================================================
// tracey_uncovered Tool Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_uncovered_tool_no_filter() {
    let service = create_test_service().await;
    let req = UncoveredRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: None,
    };

    let response = rpc(service.client.uncovered(req).await);

    // Should return list of uncovered rules
    assert_eq!(response.spec, "test");
    assert_eq!(response.impl_name, "rust");
    // We have some uncovered rules in the fixture
    assert!(response.missing_impl_count > 0);
}

#[tokio::test]
async fn test_mcp_uncovered_tool_with_prefix() {
    let service = create_test_service().await;
    let req = UncoveredRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: Some("data".to_string()),
    };

    let response = rpc(service.client.uncovered(req).await);

    // Filtered by prefix - all rules should start with "data."
    for section in &response.by_section {
        for rule in &section.rules {
            assert!(
                rule.id.base.starts_with("data."),
                "Rule {} doesn't match prefix 'data.'",
                rule.id
            );
        }
    }
}

#[tokio::test]
async fn test_mcp_uncovered_tool_auto_select() {
    let service = create_test_service().await;

    // When only one spec/impl exists, it should be auto-selected
    let req = UncoveredRequest {
        spec: None,
        impl_name: None,
        prefix: None,
    };

    let response = rpc(service.client.uncovered(req).await);

    // Should auto-select the only available spec/impl
    assert_eq!(response.spec, "test");
    assert_eq!(response.impl_name, "rust");
}

// ============================================================================
// tracey_untested Tool Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_untested_tool() {
    let service = create_test_service().await;
    let req = UntestedRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
        prefix: None,
    };

    let response = rpc(service.client.untested(req).await);

    // Should return rules that have impl but no verify
    assert_eq!(response.spec, "test");
    assert_eq!(response.impl_name, "rust");
    // Some rules are implemented but not tested
    assert!(response.missing_verify_count > 0);
}

// ============================================================================
// tracey_unmapped Tool Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_unmapped_tool() {
    let service = create_test_service().await;
    let req = UnmappedRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
        path: None,
    };

    let response = rpc(service.client.unmapped(req).await);

    // Should return file tree with coverage info
    assert_eq!(response.spec, "test");
    assert_eq!(response.impl_name, "rust");
    assert!(!response.entries.is_empty(), "Expected file entries");
}

#[tokio::test]
async fn test_mcp_unmapped_tool_with_path_filter() {
    let service = create_test_service().await;
    let req = UnmappedRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
        path: Some("src".to_string()),
    };

    let response = rpc(service.client.unmapped(req).await);

    // Should filter to only show src/ files
    for entry in &response.entries {
        assert!(
            entry.path.starts_with("src") || entry.path == "src",
            "Entry {} doesn't match path filter 'src'",
            entry.path
        );
    }
}

// ============================================================================
// tracey_rule Tool Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_rule_tool_found() {
    let service = create_test_service().await;
    let rule = rpc(service.client.rule(rid("auth.login")).await);

    assert!(rule.is_some(), "Expected auth.login rule to exist");

    let info = rule.unwrap();
    assert_eq!(info.id, rid("auth.login"));
    assert!(!info.raw.is_empty(), "Expected rule raw markdown");
    assert!(
        info.source_file.is_some(),
        "Expected source file for rule definition"
    );
}

#[tokio::test]
async fn test_mcp_rule_tool_not_found() {
    let service = create_test_service().await;
    let rule = rpc(service.client.rule(rid("nonexistent.rule.id")).await);

    assert!(rule.is_none(), "Expected nonexistent rule to return None");
}

#[tokio::test]
async fn test_mcp_rule_tool_coverage_info() {
    let service = create_test_service().await;
    let rule = rpc(service.client.rule(rid("auth.login")).await);

    let info = rule.expect("Expected rule to exist");

    // auth.login should have coverage info
    assert!(!info.coverage.is_empty(), "Expected coverage info");

    // Check coverage details
    for cov in &info.coverage {
        assert_eq!(cov.spec, "test");
        assert_eq!(cov.impl_name, "rust");
        // auth.login should have impl refs
        assert!(
            !cov.impl_refs.is_empty(),
            "Expected impl refs for auth.login"
        );
    }
}

// ============================================================================
// tracey_config Tool Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_config_tool() {
    let service = create_test_service().await;
    let config = rpc(service.client.config().await);

    // Should return project configuration
    assert!(!config.specs.is_empty(), "Expected at least one spec");

    let test_spec = config.specs.iter().find(|s| s.name == "test");
    assert!(test_spec.is_some(), "Expected test spec");

    let spec = test_spec.unwrap();
    assert_eq!(spec.prefix, "r");
    assert!(spec.implementations.contains(&"rust".to_string()));
}

// ============================================================================
// tracey_reload Tool Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_reload_tool() {
    let service = create_test_service().await;
    let response = rpc(service.client.reload().await);

    // Should return rebuild info
    assert!(response.version > 0, "Expected version > 0");
    // Rebuild time should be reasonable (< 10 seconds)
    assert!(
        response.rebuild_time_ms < 10000,
        "Rebuild took too long: {}ms",
        response.rebuild_time_ms
    );
}

// ============================================================================
// tracey_validate Tool Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_validate_tool() {
    let service = create_test_service().await;
    let req = ValidateRequest {
        spec: Some("test".to_string()),
        impl_name: Some("rust".to_string()),
    };

    let result = rpc(service.client.validate(req).await);

    // Should return validation results
    assert_eq!(result.spec, "test");
    assert_eq!(result.impl_name, "rust");
    // The fixture should be valid
    assert_eq!(result.error_count, 0, "Expected no validation errors");
}

#[tokio::test]
async fn test_mcp_validate_tool_auto_select() {
    let service = create_test_service().await;

    // When only one spec/impl exists, it should be auto-selected
    let req = ValidateRequest {
        spec: None,
        impl_name: None,
    };

    let result = rpc(service.client.validate(req).await);

    // Should auto-select the only available spec/impl
    assert_eq!(result.spec, "test");
    assert_eq!(result.impl_name, "rust");
}

// ============================================================================
// Search Tool Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_search() {
    let service = create_test_service().await;

    // Search for "auth"
    let results = rpc(service.client.search("auth".to_string(), 10).await);

    // Should find rules starting with "auth."
    assert!(!results.is_empty(), "Expected search results for 'auth'");

    // All results should be relevant to auth
    for result in &results {
        let matches_auth = result.id.to_lowercase().contains("auth")
            || result
                .content
                .as_ref()
                .is_some_and(|t: &String| t.to_lowercase().contains("auth"));
        assert!(
            matches_auth,
            "Search result {} doesn't seem relevant to 'auth'",
            result.id
        );
    }
}

#[tokio::test]
async fn test_mcp_search_limit() {
    let service = create_test_service().await;

    // Search with limit of 2
    let results = rpc(service.client.search("".to_string(), 2).await);

    // Should respect the limit
    assert!(results.len() <= 2, "Expected at most 2 results");
}

// ============================================================================
// Forward/Reverse Traceability Tests (used internally by MCP)
// ============================================================================

#[tokio::test]
async fn test_mcp_forward_data() {
    let service = create_test_service().await;
    let forward = rpc(service
        .client
        .forward("test".to_string(), "rust".to_string())
        .await);

    assert!(forward.is_some(), "Expected forward data for test/rust");

    let data = forward.unwrap();
    assert!(!data.rules.is_empty(), "Expected rules in forward data");

    // Check that rules have IDs
    for rule in &data.rules {
        assert!(!rule.id.base.is_empty(), "Rule ID should not be empty");
    }
}

#[tokio::test]
async fn test_mcp_reverse_data() {
    let service = create_test_service().await;
    let reverse = rpc(service
        .client
        .reverse("test".to_string(), "rust".to_string())
        .await);

    assert!(reverse.is_some(), "Expected reverse data for test/rust");

    let data = reverse.unwrap();
    assert!(!data.files.is_empty(), "Expected files in reverse data");
    assert!(data.total_units > 0, "Expected some code units");
}
