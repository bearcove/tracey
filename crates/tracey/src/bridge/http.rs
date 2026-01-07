//! HTTP bridge for the tracey daemon.
//!
//! This module provides an HTTP server that translates REST API requests
//! to daemon RPC calls. It serves the dashboard SPA and proxies API calls
//! to the daemon.
//!
//! r[impl daemon.bridge.http]

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use eyre::Result;
use facet::Facet;
use facet_axum::Json;
use serde::Deserialize;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

use crate::daemon::{DaemonClient, ensure_daemon_running};
use tracey_api::*;

/// State shared across HTTP handlers.
struct AppState {
    /// Client connection to daemon (protected by mutex for single-threaded access)
    client: Mutex<DaemonClient>,
    /// Project root for resolving paths
    #[allow(dead_code)]
    project_root: PathBuf,
}

/// Run the HTTP bridge server.
///
/// This function starts an HTTP server that connects to the daemon and
/// translates REST API requests to RPC calls.
pub async fn run(
    root: Option<PathBuf>,
    _config_path: Option<PathBuf>,
    port: u16,
    open: bool,
) -> Result<()> {
    use tracing::info;

    // Determine project root
    let project_root = match root {
        Some(r) => r,
        None => crate::find_project_root()?,
    };

    // Ensure daemon is running
    ensure_daemon_running(&project_root).await?;

    // Connect to daemon
    let client = DaemonClient::connect(&project_root).await?;

    let state = Arc::new(AppState {
        client: Mutex::new(client),
        project_root: project_root.clone(),
    });

    // Build router
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/config", get(api_config))
        .route("/api/forward", get(api_forward))
        .route("/api/reverse", get(api_reverse))
        .route("/api/version", get(api_version))
        .route("/api/spec", get(api_spec))
        .route("/api/search", get(api_search))
        .route("/api/status", get(api_status))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // Start server
    let addr = format!("127.0.0.1:{}", port);
    info!("HTTP bridge listening on http://{}", addr);

    if open {
        let url = format!("http://{}", addr);
        eprintln!("Open browser at: {}", url);
        // Note: webbrowser crate not available, user should open manually
    }

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Index page - serve embedded dashboard HTML.
async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../../dashboard/dist/index.html"))
}

/// Query parameters for forward/reverse endpoints.
#[derive(Debug, Clone, Deserialize)]
struct ImplQuery {
    spec: Option<String>,
    #[serde(rename = "impl")]
    impl_name: Option<String>,
}

/// Query parameters for search endpoint.
#[derive(Debug, Clone, Deserialize)]
struct SearchQuery {
    q: Option<String>,
    limit: Option<usize>,
}

/// Query parameters for spec endpoint.
#[derive(Debug, Clone, Deserialize)]
struct SpecQuery {
    spec: Option<String>,
    #[serde(rename = "impl")]
    impl_name: Option<String>,
}

/// Version response.
#[derive(Debug, Clone, Facet)]
struct VersionResponse {
    version: u64,
}

/// Search response.
#[derive(Debug, Clone, Facet)]
struct SearchResponse {
    query: String,
    results: Vec<tracey_proto::SearchResult>,
    available: bool,
}

/// GET /api/config - Get configuration.
async fn api_config(State(state): State<Arc<AppState>>) -> Response {
    let mut client = state.client.lock().await;
    match client.config().await {
        Ok(config) => Json(config).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/forward - Get forward traceability data.
async fn api_forward(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImplQuery>,
) -> Response {
    let mut client = state.client.lock().await;

    // Get config to resolve spec/impl if not provided
    let config = match client.config().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    match client.forward(spec, impl_name).await {
        Ok(Some(data)) => Json(data).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Spec/impl not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/reverse - Get reverse traceability data.
async fn api_reverse(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImplQuery>,
) -> Response {
    let mut client = state.client.lock().await;

    let config = match client.config().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    match client.reverse(spec, impl_name).await {
        Ok(Some(data)) => Json(data).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Spec/impl not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/version - Get current data version.
async fn api_version(State(state): State<Arc<AppState>>) -> Response {
    let mut client = state.client.lock().await;
    match client.version().await {
        Ok(version) => Json(VersionResponse { version }).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/spec - Get rendered spec content.
async fn api_spec(State(state): State<Arc<AppState>>, Query(query): Query<SpecQuery>) -> Response {
    let mut client = state.client.lock().await;

    let config = match client.config().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    match client.spec_content(spec, impl_name).await {
        Ok(Some(data)) => Json(data).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Spec not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/search - Search rules and files.
async fn api_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Response {
    let q = query.q.unwrap_or_default();
    let limit = query.limit.unwrap_or(50);

    let mut client = state.client.lock().await;
    match client.search(q.clone(), limit).await {
        Ok(results) => Json(SearchResponse {
            query: q,
            results,
            available: true,
        })
        .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/status - Get coverage status.
async fn api_status(State(state): State<Arc<AppState>>) -> Response {
    let mut client = state.client.lock().await;
    match client.status().await {
        Ok(status) => Json(status).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Resolve spec/impl from query params or use defaults from config.
fn resolve_spec_impl(
    spec: Option<String>,
    impl_name: Option<String>,
    config: &ApiConfig,
) -> (String, String) {
    let spec_name = spec.unwrap_or_else(|| {
        config
            .specs
            .first()
            .map(|s| s.name.clone())
            .unwrap_or_default()
    });

    let impl_name = impl_name.unwrap_or_else(|| {
        config
            .specs
            .iter()
            .find(|s| s.name == spec_name)
            .and_then(|s| s.implementations.first().cloned())
            .unwrap_or_default()
    });

    (spec_name, impl_name)
}
