//! Common test utilities.

#![allow(dead_code)]

use std::path::PathBuf;
use tokio::task::JoinHandle;

use roam_memory::memory_transport_pair;
use roam_stream::{
    ConnectionHandle, HandshakeConfig, NoDispatcher, accept_framed, initiate_framed,
};
use tracey_proto::{TraceyDaemonClient, TraceyDaemonDispatcher};

pub struct RpcTestService {
    pub client: TraceyDaemonClient<ConnectionHandle>,
    driver_tasks: Vec<JoinHandle<()>>,
}

impl Drop for RpcTestService {
    fn drop(&mut self) {
        for task in &self.driver_tasks {
            task.abort();
        }
    }
}

pub async fn create_test_rpc_service(service: tracey::daemon::TraceyService) -> RpcTestService {
    let (client_transport, server_transport) = memory_transport_pair(256);
    let dispatcher = TraceyDaemonDispatcher::new(service);

    let client_fut = initiate_framed(client_transport, HandshakeConfig::default(), NoDispatcher);
    let server_fut = accept_framed(server_transport, HandshakeConfig::default(), dispatcher);
    let (client_setup, server_setup) = tokio::try_join!(client_fut, server_fut)
        .expect("failed to establish in-memory roam transport");

    let (client_handle, _incoming_client, client_driver) = client_setup;
    let (_server_handle, _incoming_server, server_driver) = server_setup;

    let client_task = tokio::spawn(async move {
        let _ = client_driver.run().await;
    });
    let server_task = tokio::spawn(async move {
        let _ = server_driver.run().await;
    });

    RpcTestService {
        client: TraceyDaemonClient::new(client_handle),
        driver_tasks: vec![client_task, server_task],
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
