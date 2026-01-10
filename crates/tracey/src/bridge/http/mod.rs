//! HTTP bridge for the tracey daemon.
//!
//! This module provides an HTTP server that translates REST API requests
//! to daemon RPC calls. It serves the dashboard SPA and proxies API calls
//! to the daemon.
//!
//! r[impl daemon.bridge.http]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    body::Body,
    extract::{FromRequestParts, Path, Query, State, WebSocketUpgrade, ws},
    http::{Request, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use eyre::Result;
use facet::Facet;
use facet_axum::Json;
use futures_util::{SinkExt, StreamExt};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::Deserialize;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, error, info, warn};

use crate::daemon::DaemonClient;
use tracey_api::*;

/// State shared across HTTP handlers.
struct AppState {
    /// Client connection to daemon (protected by mutex for single-threaded access)
    client: Mutex<DaemonClient>,
    /// Project root for resolving paths
    #[allow(dead_code)]
    project_root: PathBuf,
    /// Vite dev server port (Some in dev mode, None otherwise)
    vite_port: Option<u16>,
    /// Keep Vite server alive (kill_on_drop)
    #[allow(dead_code)]
    _vite_server: Option<crate::vite::ViteServer>,
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
    dev: bool,
) -> Result<()> {
    // Determine project root
    let project_root = match root {
        Some(r) => r,
        None => crate::find_project_root()?,
    };

    // Connect to daemon (will error if not running)
    let client = DaemonClient::connect(&project_root).await?;

    // In dev mode, start Vite dev server
    let vite_server = if dev {
        // Dashboard is colocated with this module
        let dashboard_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bridge/http/dashboard");
        let server = crate::vite::ViteServer::start(&dashboard_dir).await?;
        Some(server)
    } else {
        None
    };
    let vite_port = vite_server.as_ref().map(|s| s.port);

    let state = Arc::new(AppState {
        client: Mutex::new(client),
        project_root: project_root.clone(),
        vite_port,
        _vite_server: vite_server,
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
        .route("/api/health", get(api_health));

    // In dev mode, proxy to Vite; otherwise serve embedded assets
    let app = if dev {
        app.fallback(vite_proxy)
    } else {
        app.route("/assets/{*path}", get(serve_asset))
            .fallback(spa_fallback)
    };

    let app = app.with_state(state).layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    );

    // Start server
    let addr = format!("127.0.0.1:{}", port);
    if let Some(vp) = vite_port {
        info!(
            "HTTP bridge listening on http://{} (dev mode, proxying to Vite on port {})",
            addr, vp
        );
    } else {
        info!("HTTP bridge listening on http://{}", addr);
    }

    if open {
        let url = format!("http://{}", addr);
        if let Err(e) = ::open::that(&url) {
            eprintln!("Failed to open browser: {}. Open manually at: {}", e, url);
        }
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

/// GET /api/health - Get daemon health status.
///
/// r[impl daemon.health]
async fn api_health(State(state): State<Arc<AppState>>) -> Response {
    let mut client = state.client.lock().await;
    match client.health().await {
        Ok(health) => Json(health).into_response(),
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

// ============================================================================
// Vite Proxy (dev mode)
// ============================================================================

/// Check if request has a WebSocket upgrade
fn has_ws(req: &Request<Body>) -> bool {
    req.headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

/// Proxy requests to Vite dev server (handles both HTTP and WebSocket)
async fn vite_proxy(State(state): State<Arc<AppState>>, req: Request<Body>) -> Response<Body> {
    let vite_port = match state.vite_port {
        Some(p) => p,
        None => {
            warn!("Vite proxy request but vite server not running");
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from("Vite server not running"))
                .unwrap();
        }
    };

    let method = req.method().clone();
    let original_uri = req.uri().to_string();
    let path = req.uri().path().to_string();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();

    debug!(method = %method, uri = %original_uri, "=> proxying to vite");

    // Check if this is a WebSocket upgrade request (for HMR)
    if has_ws(&req) {
        info!(uri = %original_uri, "=> detected websocket upgrade request");

        // Split into parts so we can extract WebSocketUpgrade
        let (mut parts, _body) = req.into_parts();

        // Manually extract WebSocketUpgrade from request parts
        let ws = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
            Ok(ws) => ws,
            Err(e) => {
                error!(error = %e, "!! failed to extract websocket upgrade");
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(format!("WebSocket upgrade failed: {}", e)))
                    .unwrap();
            }
        };

        let target_uri = format!("ws://127.0.0.1:{}{}{}", vite_port, path, query);
        info!(target = %target_uri, "-> upgrading websocket to vite");

        return ws
            .protocols(["vite-hmr"])
            .on_upgrade(move |socket| async move {
                info!(path = %path, "websocket connection established, starting proxy");
                if let Err(e) = handle_vite_ws(socket, vite_port, &path, &query).await {
                    error!(error = %e, path = %path, "!! vite websocket proxy error");
                }
                info!(path = %path, "websocket connection closed");
            })
            .into_response();
    }

    // Regular HTTP proxy
    let target_uri = format!("http://127.0.0.1:{}{}{}", vite_port, path, query);

    let client = Client::builder(TokioExecutor::new()).build_http();

    let mut proxy_req_builder = Request::builder().method(req.method()).uri(&target_uri);

    // Copy headers (except Host)
    for (name, value) in req.headers() {
        if name != header::HOST {
            proxy_req_builder = proxy_req_builder.header(name, value);
        }
    }

    let proxy_req = proxy_req_builder.body(req.into_body()).unwrap();

    match client.request(proxy_req).await {
        Ok(res) => {
            let status = res.status();
            debug!(status = %status, path = %path, "<- vite response");

            let (parts, body) = res.into_parts();
            Response::from_parts(parts, Body::new(body))
        }
        Err(e) => {
            error!(error = %e, target = %target_uri, "!! vite proxy error");
            Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from(format!("Vite proxy error: {}", e)))
                .unwrap()
        }
    }
}

async fn handle_vite_ws(
    client_socket: ws::WebSocket,
    vite_port: u16,
    path: &str,
    query: &str,
) -> Result<()> {
    use axum::extract::ws::Message;
    use tokio_tungstenite::connect_async_with_config;
    use tokio_tungstenite::tungstenite::http::Request;

    let vite_url = format!("ws://127.0.0.1:{}{}{}", vite_port, path, query);

    info!(vite_url = %vite_url, "-> connecting to vite websocket");

    // Build request with vite-hmr subprotocol
    let request = Request::builder()
        .uri(&vite_url)
        .header("Sec-WebSocket-Protocol", "vite-hmr")
        .header("Host", format!("127.0.0.1:{}", vite_port))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .body(())
        .unwrap();

    let connect_timeout = Duration::from_secs(5);
    let connect_result = tokio::time::timeout(
        connect_timeout,
        connect_async_with_config(request, None, false),
    )
    .await;

    let (vite_ws, _response) = match connect_result {
        Ok(Ok((ws, resp))) => {
            info!(vite_url = %vite_url, "-> successfully connected to vite websocket");
            (ws, resp)
        }
        Ok(Err(e)) => {
            info!(vite_url = %vite_url, error = %e, "!! failed to connect to vite websocket");
            return Err(e.into());
        }
        Err(_) => {
            info!(vite_url = %vite_url, "!! timeout connecting to vite websocket");
            return Err(eyre::eyre!(
                "Timeout connecting to Vite WebSocket after {:?}",
                connect_timeout
            ));
        }
    };

    let (mut client_tx, mut client_rx) = client_socket.split();
    let (mut vite_tx, mut vite_rx) = vite_ws.split();

    // Bidirectional proxy
    let client_to_vite = async {
        while let Some(msg) = client_rx.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let text_str: String = text.to_string();
                    if vite_tx
                        .send(tokio_tungstenite::tungstenite::Message::Text(
                            text_str.into(),
                        ))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Message::Binary(data)) => {
                    let data_vec: Vec<u8> = data.to_vec();
                    if vite_tx
                        .send(tokio_tungstenite::tungstenite::Message::Binary(
                            data_vec.into(),
                        ))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    };

    let vite_to_client = async {
        while let Some(msg) = vite_rx.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    let text_str: String = text.to_string();
                    if client_tx
                        .send(Message::Text(text_str.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Binary(data)) => {
                    let data_vec: Vec<u8> = data.to_vec();
                    if client_tx
                        .send(Message::Binary(data_vec.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    };

    tokio::select! {
        _ = client_to_vite => {}
        _ = vite_to_client => {}
    }

    Ok(())
}
