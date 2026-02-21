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
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, error, info, warn};

use crate::daemon::{DaemonClient, new_client};
use tracey_api::*;
use tracey_core::parse_rule_id;

/// Message sent to WebSocket clients when data changes.
#[derive(Debug, Clone, Facet)]
struct WsMessage {
    #[facet(rename = "type")]
    msg_type: String,
    version: u64,
}

/// State shared across HTTP handlers.
struct AppState {
    client: DaemonClient,
    /// Broadcast channel for notifying WebSocket clients of version changes
    version_tx: broadcast::Sender<u64>,
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
    _config_path: PathBuf,
    port: Option<u16>,
    open: bool,
    dev: bool,
) -> Result<()> {
    // Determine project root
    let project_root = match root {
        Some(r) => r,
        None => crate::find_project_root()?,
    };

    // Create client (connects lazily, auto-reconnects)
    let client = new_client(project_root.clone());

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

    // Create broadcast channel for WebSocket clients (capacity 16 is plenty)
    let (version_tx, _) = broadcast::channel(16);

    let state = Arc::new(AppState {
        client,
        version_tx: version_tx.clone(),
        project_root: project_root.clone(),
        vite_port,
        _vite_server: vite_server,
    });

    // Start background task to poll daemon version and broadcast changes
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            version_poller(state).await;
        });
    }

    // Build router
    // r[impl dashboard.api.config]
    // r[impl dashboard.api.forward]
    // r[impl dashboard.api.reverse]
    // r[impl dashboard.api.spec]
    // r[impl dashboard.api.file]
    let app = Router::new()
        // WebSocket for live updates
        .route("/ws", get(ws_handler))
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

    // Start server — find a free port if none was explicitly requested
    let listener = match port {
        Some(p) => {
            let addr = format!("127.0.0.1:{p}");
            tokio::net::TcpListener::bind(&addr).await?
        }
        None => {
            const DEFAULT_PORT: u16 = 3000;
            const MAX_ATTEMPTS: u16 = 20;
            let mut listener = None;
            for p in DEFAULT_PORT..DEFAULT_PORT + MAX_ATTEMPTS {
                match tokio::net::TcpListener::bind(format!("127.0.0.1:{p}")).await {
                    Ok(l) => {
                        listener = Some(l);
                        break;
                    }
                    Err(_) => continue,
                }
            }
            listener.ok_or_else(|| {
                eyre::eyre!(
                    "Could not find a free port in range {DEFAULT_PORT}..{}",
                    DEFAULT_PORT + MAX_ATTEMPTS
                )
            })?
        }
    };

    let addr = listener.local_addr()?;
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

/// API error response (always JSON).
#[derive(Debug, Clone, Facet)]
struct ApiError {
    error: String,
    code: String,
}

impl ApiError {
    fn bad_request(msg: impl Into<String>) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: msg.into(),
                code: "bad_request".to_string(),
            }),
        )
            .into_response()
    }

    fn not_found(msg: impl Into<String>) -> Response {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: msg.into(),
                code: "not_found".to_string(),
            }),
        )
            .into_response()
    }

    #[allow(dead_code)]
    fn internal(msg: impl Into<String>) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: msg.into(),
                code: "internal_error".to_string(),
            }),
        )
            .into_response()
    }

    fn rpc_error(e: impl std::fmt::Display) -> Response {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: format!("RPC error: {}", e),
                code: "rpc_error".to_string(),
            }),
        )
            .into_response()
    }
}

/// Convert roam RPC result to Result<T, Response>
#[allow(clippy::result_large_err)]
fn rpc<T, E: std::fmt::Debug>(res: Result<T, roam_stream::CallError<E>>) -> Result<T, Response> {
    res.map_err(|e| ApiError::rpc_error(format!("{:?}", e)))
}

/// GET /api/config - Get configuration.
async fn api_config(State(state): State<Arc<AppState>>) -> Response {
    let client = state.client.clone();
    match rpc(client.config().await) {
        Ok(config) => Json(config).into_response(),
        Err(e) => e,
    }
}

/// GET /api/forward - Get forward traceability data.
async fn api_forward(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImplQuery>,
) -> Response {
    let client = state.client.clone();

    // Get config to resolve spec/impl if not provided
    let config = match rpc(client.config().await) {
        Ok(c) => c,
        Err(e) => return e,
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    match rpc(client.forward(spec, impl_name).await) {
        Ok(Some(data)) => Json(ApiForwardData { specs: vec![data] }).into_response(),
        Ok(None) => ApiError::not_found("Spec/impl not found"),
        Err(e) => e,
    }
}

/// GET /api/reverse - Get reverse traceability data.
async fn api_reverse(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImplQuery>,
) -> Response {
    let client = state.client.clone();

    let config = match rpc(client.config().await) {
        Ok(c) => c,
        Err(e) => return e,
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    match rpc(client.reverse(spec, impl_name).await) {
        Ok(Some(data)) => Json(data).into_response(),
        Ok(None) => ApiError::not_found("Spec/impl not found"),
        Err(e) => e,
    }
}

/// GET /api/version - Get current data version.
async fn api_version(State(state): State<Arc<AppState>>) -> Response {
    let client = state.client.clone();
    match rpc(client.version().await) {
        Ok(version) => Json(VersionResponse { version }).into_response(),
        Err(e) => e,
    }
}

/// GET /api/health - Get daemon health status.
async fn api_health(State(state): State<Arc<AppState>>) -> Response {
    let client = state.client.clone();
    match rpc(client.health().await) {
        Ok(health) => Json(health).into_response(),
        Err(e) => e,
    }
}

/// GET /api/spec - Get rendered spec content.
async fn api_spec(State(state): State<Arc<AppState>>, Query(query): Query<SpecQuery>) -> Response {
    let client = state.client.clone();

    let config = match rpc(client.config().await) {
        Ok(c) => c,
        Err(e) => return e,
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    match rpc(client.spec_content(spec, impl_name).await) {
        Ok(Some(data)) => Json(data).into_response(),
        Ok(None) => ApiError::not_found("Spec not found"),
        Err(e) => e,
    }
}

/// GET /api/file - Get file content with syntax highlighting.
async fn api_file(State(state): State<Arc<AppState>>, Query(query): Query<FileQuery>) -> Response {
    let client = state.client.clone();

    let config = match rpc(client.config().await) {
        Ok(c) => c,
        Err(e) => return e,
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    let req = tracey_proto::FileRequest {
        spec,
        impl_name,
        path: query.path,
    };

    match rpc(client.file(req).await) {
        Ok(Some(data)) => Json(data).into_response(),
        Ok(None) => ApiError::not_found("File not found"),
        Err(e) => e,
    }
}

/// GET /api/search - Search rules and files.
async fn api_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Response {
    let q = query.q.unwrap_or_default();
    let limit = query.limit.unwrap_or(50);

    let client = state.client.clone();
    match rpc(client.search(q.clone(), limit as u32).await) {
        Ok(results) => Json(SearchResponse {
            query: q,
            results,
            available: true,
        })
        .into_response(),
        Err(e) => e,
    }
}

/// GET /api/status - Get coverage status.
async fn api_status(State(state): State<Arc<AppState>>) -> Response {
    let client = state.client.clone();
    match rpc(client.status().await) {
        Ok(status) => Json(status).into_response(),
        Err(e) => e,
    }
}

/// GET /api/validate - Validate spec/impl for errors.
async fn api_validate(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImplQuery>,
) -> Response {
    let client = state.client.clone();

    let config = match rpc(client.config().await) {
        Ok(c) => c,
        Err(e) => return e,
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    let req = tracey_proto::ValidateRequest {
        spec: Some(spec),
        impl_name: Some(impl_name),
    };

    match rpc(client.validate(req).await) {
        Ok(result) => Json(result).into_response(),
        Err(e) => e,
    }
}

/// GET /api/uncovered - Get uncovered rules.
async fn api_uncovered(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CoverageQuery>,
) -> Response {
    let client = state.client.clone();

    let config = match rpc(client.config().await) {
        Ok(c) => c,
        Err(e) => return e,
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    let req = tracey_proto::UncoveredRequest {
        spec: Some(spec),
        impl_name: Some(impl_name),
        prefix: query.prefix,
    };

    match rpc(client.uncovered(req).await) {
        Ok(data) => Json(data).into_response(),
        Err(e) => e,
    }
}

/// GET /api/untested - Get untested rules.
async fn api_untested(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CoverageQuery>,
) -> Response {
    let client = state.client.clone();

    let config = match rpc(client.config().await) {
        Ok(c) => c,
        Err(e) => return e,
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    let req = tracey_proto::UntestedRequest {
        spec: Some(spec),
        impl_name: Some(impl_name),
        prefix: query.prefix,
    };

    match rpc(client.untested(req).await) {
        Ok(data) => Json(data).into_response(),
        Err(e) => e,
    }
}

/// GET /api/unmapped - Get unmapped code.
async fn api_unmapped(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnmappedQuery>,
) -> Response {
    let client = state.client.clone();

    let config = match rpc(client.config().await) {
        Ok(c) => c,
        Err(e) => return e,
    };

    let (spec, impl_name) = resolve_spec_impl(query.spec, query.impl_name, &config);

    let req = tracey_proto::UnmappedRequest {
        spec: Some(spec),
        impl_name: Some(impl_name),
        path: query.path,
    };

    match rpc(client.unmapped(req).await) {
        Ok(data) => Json(data).into_response(),
        Err(e) => e,
    }
}

/// GET /api/rule - Get details for a specific rule.
async fn api_rule(State(state): State<Arc<AppState>>, Query(query): Query<RuleQuery>) -> Response {
    let client = state.client.clone();
    let Some(rule_id) = parse_rule_id(&query.id) else {
        return ApiError::bad_request("Invalid rule ID");
    };

    match rpc(client.rule(rule_id).await) {
        Ok(Some(info)) => Json(info).into_response(),
        Ok(None) => ApiError::not_found("Rule not found"),
        Err(e) => e,
    }
}

/// GET /api/reload - Force a rebuild.
async fn api_reload(State(state): State<Arc<AppState>>) -> Response {
    let client = state.client.clone();
    match rpc(client.reload().await) {
        Ok(response) => Json(response).into_response(),
        Err(e) => e,
    }
}

// ============================================================================
// WebSocket for live updates
// ============================================================================

/// Handle WebSocket upgrade for live version updates.
async fn ws_handler(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_ws_client(socket, state))
}

/// Handle a single WebSocket client connection.
async fn handle_ws_client(socket: ws::WebSocket, state: Arc<AppState>) {
    let (mut tx, mut rx) = socket.split();

    // Subscribe to version updates
    let mut version_rx = state.version_tx.subscribe();

    // Send initial version
    {
        let client = state.client.clone();
        if let Ok(version) = client.version().await {
            let msg = WsMessage {
                msg_type: "version".to_string(),
                version,
            };
            if let Ok(json) = facet_json::to_string(&msg) {
                let _ = tx.send(ws::Message::Text(json.into())).await;
            }
        }
    }

    // Spawn task to forward version updates to client
    let send_task = tokio::spawn(async move {
        while let Ok(version) = version_rx.recv().await {
            let msg = WsMessage {
                msg_type: "version".to_string(),
                version,
            };
            if let Ok(json) = facet_json::to_string(&msg)
                && tx.send(ws::Message::Text(json.into())).await.is_err()
            {
                break; // Client disconnected
            }
        }
    });

    // Handle incoming messages (just drain them, we don't expect any)
    while let Some(msg) = rx.next().await {
        match msg {
            Ok(ws::Message::Close(_)) => break,
            Err(_) => break,
            _ => {} // Ignore other messages
        }
    }

    // Clean up
    send_task.abort();
    debug!("WebSocket client disconnected");
}

/// Background task that polls daemon version and broadcasts changes.
async fn version_poller(state: Arc<AppState>) {
    let mut last_version: Option<u64> = None;

    loop {
        // Poll every second (much better than every client polling every 500ms)
        tokio::time::sleep(Duration::from_secs(1)).await;

        let version = {
            let client = state.client.clone();
            client.version().await.ok()
        };

        if let Some(v) = version {
            if last_version.is_some() && last_version != Some(v) {
                info!(
                    "Version changed: {:?} -> {}, broadcasting to clients",
                    last_version, v
                );
                // Broadcast to all connected WebSocket clients
                let _ = state.version_tx.send(v);
            }
            last_version = Some(v);
        }
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
