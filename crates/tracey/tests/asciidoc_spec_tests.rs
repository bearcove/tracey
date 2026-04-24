//! Integration tests for AsciiDoc spec file support.

use std::path::PathBuf;
use std::sync::Arc;

use tracey_core::parse_rule_id;

mod common;

fn rpc<T, E: std::fmt::Debug>(res: Result<T, roam::RoamError<E>>) -> T {
    res.expect("RPC call failed")
}

fn rid(id: &str) -> tracey_core::RuleId {
    parse_rule_id(id).expect("valid rule id")
}

fn fixtures_asciidoc() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures-asciidoc")
}

async fn create_adoc_engine() -> Arc<tracey::daemon::Engine> {
    let project_root = fixtures_asciidoc();
    let config_path = project_root.join("config.styx");
    Arc::new(
        tracey::daemon::Engine::new(project_root, config_path)
            .await
            .expect("Failed to create AsciiDoc test engine"),
    )
}

async fn create_adoc_service() -> common::RpcTestService {
    let engine = create_adoc_engine().await;
    let service = tracey::daemon::TraceyService::new(engine);
    common::create_test_rpc_service(service).await
}

// ============================================================================
// Parsing: spec file discovery and rule extraction
// ============================================================================

#[tokio::test]
async fn test_adoc_status_has_rules() {
    let service = create_adoc_service().await;
    let status = rpc(service.client.status().await);

    let test_impl = status
        .impls
        .iter()
        .find(|i| i.spec == "test" && i.impl_name == "rust")
        .expect("test/rust impl should exist");

    assert!(
        test_impl.total_rules >= 7,
        "Expected at least 7 rules from spec.adoc, got {}",
        test_impl.total_rules
    );
}

#[tokio::test]
async fn test_adoc_listing_block_masks_markers() {
    let service = create_adoc_service().await;
    let status = rpc(service.client.status().await);

    let test_impl = status
        .impls
        .iter()
        .find(|i| i.spec == "test" && i.impl_name == "rust")
        .expect("test/rust impl should exist");

    // The masked markers inside listing/comment blocks must not appear as rules
    assert!(
        test_impl.total_rules < 15,
        "Listing/comment block masking failed — too many rules: {}",
        test_impl.total_rules
    );
}

#[tokio::test]
async fn test_adoc_rule_lookup() {
    let service = create_adoc_service().await;
    let rule = rpc(service.client.rule(rid("auth.login")).await);

    assert!(rule.is_some(), "auth.login rule should exist");
    let info = rule.unwrap();
    assert_eq!(info.id, rid("auth.login"));
    assert!(
        info.raw.contains("valid credentials"),
        "Rule body should contain spec text, got: {:?}",
        info.raw
    );
}

#[tokio::test]
async fn test_adoc_anchor_ids_use_r_prefix() {
    // Parse the spec directly to check anchor IDs
    use tracey_core::spec::{SpecFormat, parse_spec};

    let spec_path = fixtures_asciidoc().join("spec.adoc");
    let content = std::fs::read_to_string(&spec_path).expect("read spec.adoc");
    let doc = parse_spec(SpecFormat::AsciiDoc, &content)
        .await
        .expect("parse AsciiDoc spec");

    for req in &doc.reqs {
        assert!(
            req.anchor_id.starts_with("r--"),
            "anchor_id {:?} does not start with 'r--'",
            req.anchor_id
        );
        assert_eq!(
            req.anchor_id,
            format!("r--{}", req.id),
            "anchor_id mismatch for {}",
            req.id
        );
    }
}

#[tokio::test]
async fn test_adoc_parse_extracts_headings() {
    use tracey_core::spec::{SpecFormat, parse_spec};

    let spec_path = fixtures_asciidoc().join("spec.adoc");
    let content = std::fs::read_to_string(&spec_path).expect("read spec.adoc");
    let doc = parse_spec(SpecFormat::AsciiDoc, &content)
        .await
        .expect("parse AsciiDoc spec");

    assert!(
        !doc.headings.is_empty(),
        "Expected headings to be extracted"
    );
    let titles: Vec<_> = doc.headings.iter().map(|h| h.title.as_str()).collect();
    assert!(
        titles.contains(&"Authentication"),
        "Expected 'Authentication' heading, got: {:?}",
        titles
    );
}

#[tokio::test]
async fn test_adoc_parse_reqs_have_correct_line_numbers() {
    use tracey_core::spec::{SpecFormat, parse_spec};

    let spec_path = fixtures_asciidoc().join("spec.adoc");
    let content = std::fs::read_to_string(&spec_path).expect("read spec.adoc");
    let doc = parse_spec(SpecFormat::AsciiDoc, &content)
        .await
        .expect("parse AsciiDoc spec");

    let auth_login = doc
        .reqs
        .iter()
        .find(|r| r.id.to_string() == "auth.login")
        .expect("auth.login should be present");

    assert!(
        auth_login.line > 0,
        "line number should be positive, got {}",
        auth_login.line
    );
}

// ============================================================================
// Coverage: uncovered / untested rules
// ============================================================================

#[tokio::test]
async fn test_adoc_uncovered_rules() {
    let service = create_adoc_service().await;
    let status = rpc(service.client.status().await);

    let test_impl = status
        .impls
        .iter()
        .find(|i| i.spec == "test" && i.impl_name == "rust")
        .expect("test/rust impl should exist");

    // Covered rules should not exceed total
    assert!(
        test_impl.covered_rules <= test_impl.total_rules,
        "covered ({}) > total ({})",
        test_impl.covered_rules,
        test_impl.total_rules
    );
}

// ============================================================================
// Bump: id_range_in_marker and rewrite_marker work for .adoc files
// ============================================================================

#[test]
fn test_adoc_id_range_in_marker() {
    use tracey_core::spec::{SpecFormat, id_range_in_marker};

    let marker = "r[auth.login]";
    let range = id_range_in_marker(SpecFormat::AsciiDoc, marker).expect("id_range");
    assert_eq!(&marker[range], "auth.login");
}

#[test]
fn test_adoc_rewrite_marker() {
    use tracey_core::spec::{SpecFormat, id_range_in_marker, rewrite_marker};

    let marker = "r[auth.login]";
    let range = id_range_in_marker(SpecFormat::AsciiDoc, marker).expect("id_range");
    let rewritten = rewrite_marker(marker, range, "auth.login", 2).expect("rewrite");
    assert_eq!(rewritten, "r[auth.login+2]");
}

#[test]
fn test_adoc_extract_marker_prefix() {
    use tracey_core::spec::{SpecFormat, SourceSpan, extract_marker_prefix};

    let content = "r[auth.login] Some text";
    let span = SourceSpan { offset: 0, length: 13 }; // covers "r[auth.login]"
    let prefix =
        extract_marker_prefix(SpecFormat::AsciiDoc, content, span).expect("prefix");
    assert_eq!(prefix, "r");
}

// ============================================================================
// parse_weight for AsciiDoc attribute syntax
// ============================================================================

#[test]
fn test_adoc_parse_weight_attribute() {
    use tracey_core::spec::{SpecFormat, parse_weight};

    assert_eq!(parse_weight(SpecFormat::AsciiDoc, ":weight: 10\n\n= Title"), 10);
    assert_eq!(parse_weight(SpecFormat::AsciiDoc, "= Title\n\nNo weight"), 0);
}

#[test]
fn test_adoc_parse_weight_frontmatter() {
    use tracey_core::spec::{SpecFormat, parse_weight};

    assert_eq!(
        parse_weight(SpecFormat::AsciiDoc, "---\nweight: 5\n---\n\n= Title"),
        5
    );
}

// ============================================================================
// New capabilities: tables, admonitions, inline formatting
// ============================================================================

#[tokio::test]
async fn test_adoc_table_renders_in_html() {
    use tracey_core::spec::{SpecFormat, parse_spec};

    let src = "= Doc\n\n|===\n|A |B\n\n|1 |2\n|===\n";
    let doc = parse_spec(SpecFormat::AsciiDoc, src).await.expect("parse");
    assert!(
        doc.html.contains("<table"),
        "table should render to HTML, got: {}",
        &doc.html[..doc.html.len().min(400)]
    );
    assert!(doc.reqs.is_empty(), "table block should not produce requirements");
}

#[tokio::test]
async fn test_adoc_fixture_has_table_reqs() {
    use tracey_core::spec::{SpecFormat, parse_spec};

    let spec_path = fixtures_asciidoc().join("spec.adoc");
    let content = std::fs::read_to_string(&spec_path).expect("read spec.adoc");
    let doc = parse_spec(SpecFormat::AsciiDoc, &content).await.expect("parse");

    let ids: Vec<_> = doc.reqs.iter().map(|r| r.id.to_string()).collect();
    assert!(ids.contains(&"table.structure".to_string()), "table.structure should be present, got: {:?}", ids);
    assert!(ids.contains(&"table.inline-code".to_string()), "table.inline-code should be present");
    assert!(ids.contains(&"format.bold".to_string()), "format.bold should be present");
    assert!(ids.contains(&"format.multiline".to_string()), "format.multiline should be present");
}

#[tokio::test]
async fn test_adoc_table_html_in_fixture() {
    use tracey_core::spec::{SpecFormat, parse_spec};

    let spec_path = fixtures_asciidoc().join("spec.adoc");
    let content = std::fs::read_to_string(&spec_path).expect("read spec.adoc");
    let doc = parse_spec(SpecFormat::AsciiDoc, &content).await.expect("parse");

    assert!(
        doc.html.contains("<table"),
        "fixture HTML should contain a rendered table"
    );
}

#[tokio::test]
async fn test_adoc_multiline_req_raw() {
    use tracey_core::spec::{SpecFormat, parse_spec};

    let src = "r[multi.line]\nFirst sentence.\nSecond sentence.\n";
    let doc = parse_spec(SpecFormat::AsciiDoc, src).await.expect("parse");

    let req = doc.reqs.iter().find(|r| r.id.to_string() == "multi.line").expect("multi.line");
    assert!(
        req.raw.contains("First sentence."),
        "raw should contain first line, got: {:?}",
        req.raw
    );
    assert!(
        req.raw.contains("Second sentence."),
        "raw should contain second line, got: {:?}",
        req.raw
    );
}

#[tokio::test]
async fn test_adoc_req_html_injected_in_spec_html() {
    use tracey_core::spec::{SpecFormat, parse_spec};

    let src = "r[inject.test]\nThis requirement should be wrapped.\n";
    let doc = parse_spec(SpecFormat::AsciiDoc, src).await.expect("parse");

    assert!(
        doc.html.contains("r--inject.test"),
        "spec HTML should contain req anchor id, got: {}",
        &doc.html[..doc.html.len().min(500)]
    );
    assert!(
        doc.html.contains("req-container"),
        "spec HTML should contain req-container div"
    );
}

#[tokio::test]
async fn test_adoc_inline_ignored_not_a_req() {
    use tracey_core::spec::{SpecFormat, parse_spec};

    let src = "See r[auth.login] for details.\n";
    let doc = parse_spec(SpecFormat::AsciiDoc, src).await.expect("parse");
    assert!(doc.reqs.is_empty(), "inline marker should not be a requirement");
}

#[tokio::test]
async fn test_adoc_duplicate_req_errors() {
    use tracey_core::spec::{SpecFormat, parse_spec};

    let src = "r[dup.id]\nFirst.\n\nr[dup.id]\nSecond.\n";
    let result = parse_spec(SpecFormat::AsciiDoc, src).await;
    assert!(result.is_err(), "duplicate requirement should return an error");
}

#[tokio::test]
async fn test_adoc_cross_file_xref_does_not_error() {
    use tracey_core::spec::{SpecFormat, parse_spec};

    // Cross-file xrefs (<<anchor-in-another-file>>) are unresolvable during
    // per-file parsing but valid at display time since all spec files share
    // one HTML page. The parser must not return an error for these.
    let src = "== Section A\n\nSee <<other-section>> for details.\n";
    let doc = parse_spec(SpecFormat::AsciiDoc, src).await
        .expect("cross-file xref should not cause a parse error");
    assert!(
        doc.html.contains("other-section"),
        "HTML should contain the xref target as a link, got: {}",
        &doc.html[..doc.html.len().min(500)]
    );
}

#[tokio::test]
async fn test_adoc_heading_ids_in_html_match_headings() {
    use tracey_core::spec::{SpecFormat, parse_spec};

    let src = "== My Section\n\nSome text.\n";
    let doc = parse_spec(SpecFormat::AsciiDoc, src).await.expect("parse");

    assert!(!doc.headings.is_empty(), "should have at least one heading");
    let heading = &doc.headings[0];
    assert!(
        doc.html.contains(&format!("id=\"{}\"", heading.id)),
        "HTML should contain heading id {:?}, got html: {}",
        heading.id,
        &doc.html[..doc.html.len().min(500)]
    );
}
