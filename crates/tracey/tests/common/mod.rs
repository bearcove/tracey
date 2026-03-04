//! Common test utilities.

#![allow(dead_code)]

use std::path::PathBuf;

use roam_core::memory_link_pair as memory_transport_pair;
use tracey_proto::{TraceyDaemonClient, TraceyDaemonDispatcher};

pub struct RpcTestService {
    pub client: TraceyDaemonClient,
}

pub async fn create_test_rpc_service(service: tracey::daemon::TraceyService) -> RpcTestService {
    let (client_transport, server_transport) = memory_transport_pair(256);
    let dispatcher = TraceyDaemonDispatcher::new(service);

    let _ = roam::acceptor(server_transport)
        .establish::<()>(dispatcher)
        .await
        .expect("failed to establish in-memory roam transport");

    let (client, _session_handle) = roam::initiator(client_transport)
        .establish::<TraceyDaemonClient>(())
        .await
        .expect("failed to establish in-memory roam transport");

    RpcTestService { client }
}

/// Get the path to the test fixtures directory.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Create a temporary directory for test isolation.
pub fn create_temp_project() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("Failed to create temp dir");

    // Copy fixtures to temp dir
    let fixtures = fixtures_dir();

    // Copy spec.md
    std::fs::copy(fixtures.join("spec.md"), temp.path().join("spec.md"))
        .expect("Failed to copy spec.md");
    std::fs::copy(
        fixtures.join("other-spec.md"),
        temp.path().join("other-spec.md"),
    )
    .expect("Failed to copy other-spec.md");

    // Copy config.styx
    std::fs::copy(
        fixtures.join("config.styx"),
        temp.path().join("config.styx"),
    )
    .expect("Failed to copy config.styx");

    // Create src directory and copy source files
    std::fs::create_dir_all(temp.path().join("src")).expect("Failed to create src dir");
    std::fs::copy(fixtures.join("src/lib.rs"), temp.path().join("src/lib.rs"))
        .expect("Failed to copy lib.rs");
    std::fs::copy(
        fixtures.join("src/tests.rs"),
        temp.path().join("src/tests.rs"),
    )
    .expect("Failed to copy tests.rs");

    temp
}
