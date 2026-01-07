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
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use eyre::Result;
use facet::Facet;
use facet_axum::Json;
use serde::Deserialize;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

use crate::daemon::DaemonClient;
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

    // Connect to daemon (will error if not running)
    let client = DaemonClient::connect(&project_root).await?;

    let state = Arc::new(AppState {
        client: Mutex::new(client),
        project_root: project_root.clone(),
    });

    // Build router
    // r[impl dashboard.api.config]
    // r[impl dashboard.api.forward]
    // r[impl dashboard.api.reverse]
    // r[impl dashboard.api.spec]
    // r[impl dashboard.api.file]
    let app = Router::new()
        // API routes
        .route("/api/config", get(api_config))
        .route("/api/forward", get(api_forward))
        .route("/api/reverse", get(api_reverse))
        .route("/api/version", get(api_version))
        .route("/api/spec", get(api_spec))
        .route("/api/file", get(api_file))
        .route("/api/search", get(api_search))
        .route("/api/status", get(api_status))
        .route("/api/validate", get(api_validate))
        .route("/api/uncovered", get(api_uncovered))
        .route("/api/untested", get(api_untested))
        .route("/api/unmapped", get(api_unmapped))
        .route("/api/rule", get(api_rule))
        .route("/api/reload", get(api_reload))
        // Static assets
        .route("/assets/{*path}", get(serve_asset))
        // SPA fallback - all other routes serve index.html
        .fallback(spa_fallback)
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

// Embedded dashboard assets (colocated in src/bridge/http/dashboard/)
static INDEX_HTML: &str = include_str!("dashboard/dist/index.html");
static INDEX_CSS: &str = include_str!("dashboard/dist/assets/index.css");
static INDEX_JS: &str = include_str!("dashboard/dist/assets/index.js");

/// SPA fallback - serve index.html for all non-API routes.
async fn spa_fallback() -> Html<&'static str> {
    Html(INDEX_HTML)
}

/// Serve static assets from embedded files.
async fn serve_asset(Path(path): Path<String>) -> Response {
    match path.as_str() {
        "index.css" => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/css")],
            INDEX_CSS,
        )
            .into_response(),
        "index.js" => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/javascript")],
            INDEX_JS,
        )
            .into_response(),
        _ => (StatusCode::NOT_FOUND, "Asset not found").into_response(),
    }
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

/// Query parameters for file endpoint.
#[derive(Debug, Clone, Deserialize)]
struct FileQuery {
    path: String,
    spec: Option<String>,
    #[serde(rename = "impl")]
    impl_name: Option<String>,
}

/// Query parameters for uncovered/untested endpoints.
#[derive(Debug, Clone, Deserialize)]
struct CoverageQuery {
    spec: Option<String>,
    #[serde(rename = "impl")]
    impl_name: Option<String>,
    prefix: Option<String>,
}

/// Query parameters for unmapped endpoint.
#[derive(Debug, Clone, Deserialize)]
struct UnmappedQuery {
    spec: Option<String>,
    #[serde(rename = "impl")]
    impl_name: Option<String>,
    path: Option<String>,
}

/// Query parameters for rule endpoint.
#[derive(Debug, Clone, Deserialize)]
struct RuleQuery {
    id: String,
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

/// GET /api/file - Get file content with syntax highlighting.
async fn api_file(State(state): State<Arc<AppState>>, Query(query): Query<FileQuery>) -> Response {
    let mut client = state.client.lock().await;

    let config = match client.config().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    let req = tracey_proto::FileRequest {
        spec,
        impl_name,
        path: query.path,
    };

    match client.file(req).await {
        Ok(Some(data)) => Json(data).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "File not found").into_response(),
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

/// GET /api/validate - Validate spec/impl for errors.
async fn api_validate(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImplQuery>,
) -> Response {
    let mut client = state.client.lock().await;

    let config = match client.config().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    let req = tracey_proto::ValidateRequest {
        spec: Some(spec),
        impl_name: Some(impl_name),
    };

    match client.validate(req).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/uncovered - Get uncovered rules.
async fn api_uncovered(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CoverageQuery>,
) -> Response {
    let mut client = state.client.lock().await;

    let config = match client.config().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    let req = tracey_proto::UncoveredRequest {
        spec: Some(spec),
        impl_name: Some(impl_name),
        prefix: query.prefix,
    };

    match client.uncovered(req).await {
        Ok(data) => Json(data).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/untested - Get untested rules.
async fn api_untested(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CoverageQuery>,
) -> Response {
    let mut client = state.client.lock().await;

    let config = match client.config().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    let req = tracey_proto::UntestedRequest {
        spec: Some(spec),
        impl_name: Some(impl_name),
        prefix: query.prefix,
    };

    match client.untested(req).await {
        Ok(data) => Json(data).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/unmapped - Get unmapped code.
async fn api_unmapped(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnmappedQuery>,
) -> Response {
    let mut client = state.client.lock().await;

    let config = match client.config().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    let req = tracey_proto::UnmappedRequest {
        spec: Some(spec),
        impl_name: Some(impl_name),
        path: query.path,
    };

    match client.unmapped(req).await {
        Ok(data) => Json(data).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/rule - Get details for a specific rule.
async fn api_rule(State(state): State<Arc<AppState>>, Query(query): Query<RuleQuery>) -> Response {
    let mut client = state.client.lock().await;

    match client.rule(query.id).await {
        Ok(Some(info)) => Json(info).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Rule not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/reload - Force a rebuild.
async fn api_reload(State(state): State<Arc<AppState>>) -> Response {
    let mut client = state.client.lock().await;
    match client.reload().await {
        Ok(response) => Json(response).into_response(),
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
