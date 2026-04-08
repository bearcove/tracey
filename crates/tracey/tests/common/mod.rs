//! Common test utilities.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Once;

use tracey_proto::{TraceyDaemonClient, TraceyDaemonDispatcher};
use tracing::debug;
use tracing_subscriber::EnvFilter;
use vox_core::memory_link_pair as memory_transport_pair;

static TEST_TRACING: Once = Once::new();

pub fn init_test_tracing() {
    TEST_TRACING.call_once(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new("info,tracey::daemon=debug,tracey::daemon::client=debug")
        });
        if let Err(e) = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .try_init()
        {
            eprintln!("tracey test tracing init skipped: {e}");
        } else {
            eprintln!("tracey test tracing initialized");
        }
    });
}

pub struct RpcTestService {
    pub client: TraceyDaemonClient,
    _server_client: TraceyDaemonClient,
}

pub async fn create_test_rpc_service(service: tracey::daemon::TraceyService) -> RpcTestService {
    init_test_tracing();
    debug!("create_test_rpc_service: start");

    let (client_transport, server_transport) = memory_transport_pair(256);
    let dispatcher = TraceyDaemonDispatcher::new(service);

    let server_fut = vox::acceptor_on(server_transport).establish::<TraceyDaemonClient>(dispatcher);
    let client_fut = vox::initiator_on(client_transport, vox::TransportMode::Bare)
        .establish::<TraceyDaemonClient>(());
    let (server_result, client_result) = tokio::try_join!(server_fut, client_fut)
        .expect("failed to establish in-memory vox transport");
    let (server_client, _server_session_handle) = server_result;
    let (client, _client_session_handle) = client_result;
    debug!("create_test_rpc_service: server+client established");

    RpcTestService {
        client,
        _server_client: server_client,
    }
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
