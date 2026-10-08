//! End-to-end test for StrictDoc (`.sdoc`) spec loading + `@relation(...)`
//! source markers.

use std::path::PathBuf;
use std::sync::Arc;

mod common;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures-strictdoc")
}

async fn create_engine() -> Arc<tracey::daemon::Engine> {
    let project_root = fixture_root();
    let config_path = project_root.join("config.styx");

    Arc::new(
        tracey::daemon::Engine::new(project_root, config_path)
            .await
            .expect("Failed to create engine"),
    )
}

#[tokio::test]
async fn sdoc_spec_yields_uppercase_uids_and_html() {
    let rules = tracey::load_rules_from_globs(&fixture_root(), &["spec.sdoc"], true)
        .await
        .expect("load_rules_from_globs must succeed for .sdoc fixture");

    let ids: Vec<String> = rules.iter().map(|r| r.def.id.to_string()).collect();
    assert!(
        ids.contains(&"BR-001".to_string()),
        "expected BR-001, got {ids:?}"
    );
    assert!(
        ids.contains(&"BR-002".to_string()),
        "expected BR-002, got {ids:?}"
    );
    assert!(
        ids.contains(&"BR-003".to_string()),
        "expected BR-003, got {ids:?}"
    );
    for uid in ["BR-004", "BR-005", "BR-006"] {
        assert!(
            ids.contains(&uid.to_string()),
            "expected {uid} (composite / nested / custom element), got {ids:?}"
        );
    }
    assert_eq!(ids.len(), 6, "[TEXT] nodes must not become rules: {ids:?}");

    let br001 = rules
        .iter()
        .find(|r| r.def.id.to_string() == "BR-001")
        .expect("BR-001 not found");
    assert!(
        br001.def.html.contains("<strong>"),
        "BR-001 declares MARKUP:Markdown; expected rendered <strong>, got: {}",
        br001.def.html
    );

    let br002 = rules
        .iter()
        .find(|r| r.def.id.to_string() == "BR-002")
        .expect("BR-002 not found");
    assert!(
        br002.def.html.contains("<p"),
        "BR-002 should be wrapped in a paragraph element, got: {}",
        br002.def.html
    );
    assert!(
        !br002.def.html.contains("<strong>"),
        "BR-002 STATEMENT has no markdown emphasis"
    );

    for rule in &rules {
        assert_eq!(
            rule.prefix, "r",
            "sdoc rules should expose synthetic prefix 'r'"
        );
    }
}

#[tokio::test]
async fn engine_status_covers_sdoc_rules() {
    let engine = create_engine().await;
    let service = tracey::daemon::TraceyService::new(engine);
    let rpc_service = common::create_test_rpc_service(service).await;

    let status = rpc_service
        .client
        .status()
        .await
        .expect("status RPC must succeed");

    let br = status
        .impls
        .iter()
        .find(|i| i.spec == "br" && i.impl_name == "rust")
        .unwrap_or_else(|| {
            panic!(
                "expected br/rust impl in status; got: {:?}",
                status
                    .impls
                    .iter()
                    .map(|i| (
                        &i.spec,
                        &i.impl_name,
                        i.total_rules,
                        i.covered_rules,
                        i.verified_rules
                    ))
                    .collect::<Vec<_>>()
            )
        });

    // Six requirements parsed from spec.sdoc: BR-001..003 [REQUIREMENT]s,
    // the composite BR-004 and its nested BR-005, and the custom [FEATURE]
    // BR-006; the [TEXT] node is not a requirement.
    //
    // Implemented: BR-001 (@relation, no role), BR-003 (legacy r[impl]),
    // BR-004 (role=Implementation), BR-005 (scope=range_start).
    // Verified: BR-001 and BR-002 (role=Verifies), BR-006 (role=Test).
    let counts = (br.total_rules, br.covered_rules, br.verified_rules);
    assert_eq!(counts, (6, 4, 3), "(total, covered, verified)");
}
