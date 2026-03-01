use anyhow::Context;
use arc_swap::ArcSwap;
use axum::{
    body::Body,
    extract::{Request, State, WebSocketUpgrade},
    http::{header, Method, StatusCode},
    response::IntoResponse,
    routing::any,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use include_dir::{include_dir, Dir};
use reqwest::Client;
use std::sync::Arc;

use crate::config::AppConfig;

static UI_DIR: Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/../incus-ui-canonical/build/ui");

static DOCS_DIR: Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/../incus-docs-build");

// ── State ────────────────────────────────────────────────────────────────────

pub struct ProxyStateInner {
    pub config: AppConfig,
    pub client: Option<Client>,
}

/// ArcSwap gives lock-free reads on every request.
pub type ProxyState = Arc<ArcSwap<ProxyStateInner>>;

pub fn build_client(config: &AppConfig) -> anyhow::Result<Client> {
    let mut builder = Client::builder()
        .danger_accept_invalid_certs(config.accept_invalid_certs)
        .use_rustls_tls()
        .pool_max_idle_per_host(8)
        .tcp_keepalive(std::time::Duration::from_secs(20));

    if let Some(ca_path) = &config.ca_cert_path {
        let pem = std::fs::read(ca_path)
            .with_context(|| format!("reading CA cert from {ca_path}"))?;
        let cert = reqwest::Certificate::from_pem(&pem).context("parsing CA cert PEM")?;
        builder = builder.add_root_certificate(cert);
    }

    if let (Some(cert_path), Some(key_path)) =
        (&config.client_cert_path, &config.client_key_path)
    {
        let mut pem_data = std::fs::read(cert_path)
            .with_context(|| format!("reading client cert from {cert_path}"))?;
        pem_data.extend(
            std::fs::read(key_path)
                .with_context(|| format!("reading client key from {key_path}"))?,
        );
        let identity =
            reqwest::Identity::from_pem(&pem_data).context("parsing client cert+key identity")?;
        builder = builder.identity(identity);
    }

    builder.build().context("building reqwest client")
}

// ── Router ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct RouterState {
    proxy: ProxyState,
    port: u16,
}

pub fn build_router(proxy: ProxyState, port: u16) -> Router {
    let state = RouterState { proxy, port };
    Router::new()
        .route("/1.0", any(api_handler))
        .route("/1.0/*path", any(api_handler))
        .route("/oidc/*path", any(api_handler))
        .route("/docs", any(docs_handler))
        .route("/docs/", any(docs_handler))
        .route("/docs/*path", any(docs_handler))
        .fallback(static_handler)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            host_guard,
        ))
        .with_state(state)
}

/// Reject requests whose Host header is not 127.0.0.1:<port> or localhost:<port>.
/// This prevents DNS rebinding attacks where a malicious page remaps a domain
/// to 127.0.0.1 and tries to reach the proxy from a browser tab.
async fn host_guard(
    State(state): State<RouterState>,
    req: Request,
    next: axum::middleware::Next,
) -> impl IntoResponse {
    if let Some(host) = req.headers().get(header::HOST) {
        let host_str = host.to_str().unwrap_or("");
        let allowed = host_str == format!("127.0.0.1:{}", state.port)
            || host_str == format!("localhost:{}", state.port);
        if !allowed {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    next.run(req).await
}

// ── Static file handler ──────────────────────────────────────────────────────

async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let raw = uri.path().trim_start_matches('/');
    let path = raw.strip_prefix("ui/").unwrap_or(raw);
    let path = if path.is_empty() { "index.html" } else { path };

    let (file, resolved_path) = match UI_DIR.get_file(path) {
        Some(f) => (f, path),
        None => {
            let f = UI_DIR
                .get_file("index.html")
                .expect("index.html must exist in built UI");
            (f, "index.html")
        }
    };

    let mime = mime_guess::from_path(resolved_path)
        .first_or_octet_stream()
        .to_string();

    let cache_control = if resolved_path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache, must-revalidate"
    };

    // Inject link interceptor into index.html so the React shell can handle
    // documentation links (open in in-app docs window) and external links
    // (Discussion, Report a bug → open in system browser).
    if resolved_path == "index.html" {
        let mut html = String::from_utf8_lossy(file.contents()).into_owned();
        html.push_str(LINK_INTERCEPTOR_SCRIPT);
        return (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache, must-revalidate"),
            ],
            html.into_bytes(),
        )
            .into_response();
    }

    (
        [
            (header::CONTENT_TYPE, mime.as_str()),
            (header::CACHE_CONTROL, cache_control),
        ],
        file.contents(),
    )
        .into_response()
}

// ── Docs file handler ────────────────────────────────────────────────────────

async fn docs_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let raw = uri.path().trim_start_matches('/');
    // Strip the /docs/ prefix
    let path = raw.strip_prefix("docs/").unwrap_or_else(|| {
        if raw == "docs" { "" } else { raw }
    });
    let path = if path.is_empty() { "index.html" } else { path };

    // Try exact path, then with .html, then as directory index
    let candidates = [
        path.to_string(),
        format!("{}.html", path),
        format!("{}/index.html", path),
    ];

    for candidate in &candidates {
        if let Some(file) = DOCS_DIR.get_file(candidate.as_str()) {
            let mime = mime_guess::from_path(candidate.as_str())
                .first_or_octet_stream()
                .to_string();
            let cache_control = if candidate.contains("/_static/") || candidate.contains("/_images/") {
                "public, max-age=86400"
            } else {
                "no-cache, must-revalidate"
            };
            return (
                [
                    (header::CONTENT_TYPE, mime.as_str()),
                    (header::CACHE_CONTROL, cache_control),
                ],
                file.contents(),
            )
                .into_response();
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

/// Injected into the SPA's index.html.
/// Intercepts link clicks before the browser/webview acts on them:
///   - /documentation/* → map to local /docs/ and open in in-app docs window
///   - external https:// links → open in system browser
const LINK_INTERCEPTOR_SCRIPT: &str = r#"
<script>
(function () {
  document.addEventListener('click', function (e) {
    var a = e.target.closest('a[href]');
    if (!a) return;
    var href;
    try { href = new URL(a.href, window.location.href); } catch (_) { return; }

    // Documentation links: map /documentation/<path> → local /docs/<path>
    if (href.origin === window.location.origin && href.pathname.startsWith('/documentation')) {
      e.preventDefault();
      e.stopPropagation();
      var rel = href.pathname.replace(/^\/documentation\/?/, '');
      var target = href.origin + '/docs/' + (rel ? rel : '') + href.search + href.hash;
      window.parent.postMessage({ __incus: 'open-docs', url: target }, '*');
      return;
    }

    // External links (Discussion, Report a bug, etc.) → system browser
    if (href.origin !== window.location.origin) {
      e.preventDefault();
      e.stopPropagation();
      window.parent.postMessage({ __incus: 'open-external', url: href.href }, '*');
    }
  }, true);
})();
</script>
"#;

// ── API proxy handler ────────────────────────────────────────────────────────

async fn api_handler(
    State(state): State<RouterState>,
    ws_upgrade: Option<WebSocketUpgrade>,
    req: Request,
) -> impl IntoResponse {
    if req.method() == Method::OPTIONS {
        return cors_preflight();
    }

    let inner = state.proxy.load_full();

    if let Some(upgrade) = ws_upgrade {
        let uri = req.uri().clone();
        if inner.config.socket_path.is_some() {
            return upgrade
                .on_upgrade(move |ws| ws_relay_unix(ws, uri, inner))
                .into_response();
        }
        return upgrade
            .on_upgrade(move |ws| ws_relay(ws, uri, inner))
            .into_response();
    }

    if inner.config.socket_path.is_some() {
        return http_proxy_unix(state.proxy, req).await.into_response();
    }

    http_proxy(state.proxy, req).await.into_response()
}

fn cors_preflight() -> axum::response::Response {
    axum::response::Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("access-control-allow-origin", "*")
        .header("access-control-allow-methods", "GET, POST, PUT, PATCH, DELETE, OPTIONS")
        .header("access-control-allow-headers", "Content-Type, Authorization, X-Incus-Type, X-Incus-Fingerprint")
        .header("access-control-max-age", "86400")
        .body(Body::empty())
        .unwrap()
}

fn cors_headers(mut resp: axum::http::response::Builder) -> axum::http::response::Builder {
    resp = resp
        .header("access-control-allow-origin", "*")
        .header("access-control-allow-methods", "GET, POST, PUT, PATCH, DELETE, OPTIONS")
        .header("access-control-allow-headers", "Content-Type, Authorization, X-Incus-Type, X-Incus-Fingerprint");
    resp
}

// ── HTTP proxy (streaming, HTTPS) ────────────────────────────────────────────

async fn http_proxy(state: ProxyState, req: Request) -> impl IntoResponse {
    let inner = state.load();

    let client = match &inner.client {
        Some(c) => c,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "no HTTPS client configured").into_response(),
    };

    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("");
    let target_url = format!(
        "https://{}:{}{}",
        inner.config.host, inner.config.port, path_and_query
    );

    let method = match reqwest::Method::from_bytes(req.method().as_str().as_bytes()) {
        Ok(m) => m,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid method").into_response(),
    };

    let headers = req.headers().clone();
    let req_body = reqwest::Body::wrap_stream(req.into_body().into_data_stream());

    let mut req_builder = client.request(method, &target_url).body(req_body);
    for (k, v) in &headers {
        if is_hop_by_hop(k.as_str()) {
            continue;
        }
        req_builder = req_builder.header(k, v);
    }

    let resp = match req_builder.send().await {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("proxy error: {e}")).into_response(),
    };

    let status = resp.status().as_u16();
    let resp_headers = resp.headers().clone();
    let stream_body = Body::from_stream(resp.bytes_stream());

    let mut response = axum::response::Response::builder().status(status);
    for (k, v) in &resp_headers {
        if is_hop_by_hop(k.as_str()) {
            continue;
        }
        response = response.header(k, v);
    }
    response = cors_headers(response);

    response
        .body(stream_body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// ── HTTP proxy (streaming, Unix socket) ──────────────────────────────────────

async fn http_proxy_unix(state: ProxyState, req: Request) -> impl IntoResponse {
    use hyper::client::conn::http1;
    use hyper_util::rt::TokioIo;

    let inner = state.load();
    let socket_path = match &inner.config.socket_path {
        Some(p) => p.clone(),
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "no socket path").into_response(),
    };

    let stream = match tokio::net::UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("unix connect: {e}")).into_response(),
    };
    let io = TokioIo::new(stream);

    let (mut sender, conn) = match http1::handshake(io).await {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("handshake: {e}")).into_response(),
    };
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::error!("unix http1 connection error: {e}");
        }
    });

    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");

    let method = req.method().clone();
    let headers = req.headers().clone();

    let mut hyper_req = hyper::Request::builder()
        .method(method)
        .uri(path_and_query)
        .header("Host", "localhost");

    for (k, v) in &headers {
        if is_hop_by_hop(k.as_str()) {
            continue;
        }
        hyper_req = hyper_req.header(k, v);
    }

    let body_stream = req.into_body().into_data_stream();
    let body = http_body_util::StreamBody::new(
        body_stream.map(|r| r.map(hyper::body::Frame::data)),
    );

    let hyper_req = match hyper_req.body(body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("build req: {e}")).into_response(),
    };

    let resp = match sender.send_request(hyper_req).await {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("unix proxy: {e}")).into_response(),
    };

    let status = resp.status().as_u16();
    let resp_headers = resp.headers().clone();

    let stream_body = Body::from_stream(
        http_body_util::BodyStream::new(resp.into_body()).filter_map(|frame| async {
            match frame {
                Ok(f) => f.into_data().ok().map(Ok),
                Err(e) => Some(Err(std::io::Error::new(std::io::ErrorKind::Other, e))),
            }
        }),
    );

    let mut response = axum::response::Response::builder().status(status);
    for (k, v) in &resp_headers {
        if is_hop_by_hop(k.as_str()) {
            continue;
        }
        response = response.header(k, v);
    }
    response = cors_headers(response);

    response
        .body(stream_body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// ── WebSocket relay (HTTPS) ──────────────────────────────────────────────────

async fn ws_relay(
    client_ws: axum::extract::ws::WebSocket,
    uri: axum::http::Uri,
    inner: Arc<ProxyStateInner>,
) {
    let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("");
    let ws_url = format!(
        "wss://{}:{}{}",
        inner.config.host, inner.config.port, path_and_query
    );

    let connector = match build_tls_connector(&inner.config) {
        Ok(c) => c,
        Err(e) => { tracing::error!("WS TLS build failed: {e}"); return; }
    };

    let (incus_ws, _) = match tokio_tungstenite::connect_async_tls_with_config(
        &ws_url,
        None,
        false,
        Some(tokio_tungstenite::Connector::Rustls(Arc::new(connector))),
    ).await {
        Ok(r) => r,
        Err(e) => { tracing::error!("WS connect failed: {e}"); return; }
    };

    bidirectional_relay(client_ws, incus_ws).await;
}

// ── WebSocket relay (Unix socket) ────────────────────────────────────────────

async fn ws_relay_unix(
    client_ws: axum::extract::ws::WebSocket,
    uri: axum::http::Uri,
    inner: Arc<ProxyStateInner>,
) {
    let socket_path = match &inner.config.socket_path {
        Some(p) => p.clone(),
        None => { tracing::error!("WS unix: no socket path"); return; }
    };

    let stream = match tokio::net::UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => { tracing::error!("WS unix connect: {e}"); return; }
    };

    let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    let ws_uri = format!("ws://localhost{}", path_and_query);

    let ws_req = match tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(&ws_uri)
        .header("Host", "localhost")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
        .body(())
    {
        Ok(r) => r,
        Err(e) => { tracing::error!("WS unix build request: {e}"); return; }
    };

    let (incus_ws, _) = match tokio_tungstenite::client_async(ws_req, stream).await {
        Ok(r) => r,
        Err(e) => { tracing::error!("WS unix handshake: {e}"); return; }
    };

    bidirectional_relay(client_ws, incus_ws).await;
}

// ── Shared bidirectional WS relay ────────────────────────────────────────────

async fn bidirectional_relay<S>(
    client_ws: axum::extract::ws::WebSocket,
    incus_ws: tokio_tungstenite::WebSocketStream<S>,
)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use axum::extract::ws::Message as AMsg;
    use tokio_tungstenite::tungstenite::Message as TMsg;

    let (mut incus_sink, mut incus_stream) = incus_ws.split();
    let (mut client_sink, mut client_stream) = client_ws.split();

    let c2i = async {
        while let Some(Ok(msg)) = client_stream.next().await {
            let tmsg = match msg {
                AMsg::Text(t)   => TMsg::Text(t.into()),
                AMsg::Binary(b) => TMsg::Binary(b.into()),
                AMsg::Ping(p)   => TMsg::Ping(p.into()),
                AMsg::Pong(p)   => TMsg::Pong(p.into()),
                AMsg::Close(_)  => break,
            };
            if incus_sink.send(tmsg).await.is_err() { break; }
        }
    };

    let i2c = async {
        while let Some(Ok(msg)) = incus_stream.next().await {
            let amsg = match msg {
                TMsg::Text(t)        => AMsg::Text(t.to_string()),
                TMsg::Binary(b)      => AMsg::Binary(b.to_vec()),
                TMsg::Ping(p)        => AMsg::Ping(p.to_vec()),
                TMsg::Pong(p)        => AMsg::Pong(p.to_vec()),
                TMsg::Close(_) | TMsg::Frame(_) => break,
            };
            if client_sink.send(amsg).await.is_err() { break; }
        }
    };

    tokio::select! { _ = c2i => {}, _ = i2c => {} }
}

// ── TLS connector for WebSockets ─────────────────────────────────────────────

fn build_tls_connector(config: &AppConfig) -> anyhow::Result<rustls::ClientConfig> {
    use rustls::RootCertStore;

    if config.accept_invalid_certs {
        return Ok(rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth());
    }

    let mut root_store = RootCertStore::empty();
    if let Some(ca_path) = &config.ca_cert_path {
        let pem = std::fs::read(ca_path)?;
        for cert in rustls_pemfile::certs(&mut std::io::Cursor::new(&pem)) {
            root_store.add(cert?)?;
        }
    } else {
        for cert in rustls_native_certs::load_native_certs().certs {
            let _ = root_store.add(cert);
        }
    }

    let builder = rustls::ClientConfig::builder().with_root_certificates(root_store);

    if let (Some(cert_path), Some(key_path)) =
        (&config.client_cert_path, &config.client_key_path)
    {
        let cert_pem = std::fs::read(cert_path)?;
        let key_pem  = std::fs::read(key_path)?;
        let certs: Vec<_> = rustls_pemfile::certs(&mut std::io::Cursor::new(&cert_pem))
            .collect::<Result<_, _>>()?;
        let key = rustls_pemfile::private_key(&mut std::io::Cursor::new(&key_pem))?
            .context("no private key found")?;
        Ok(builder.with_client_auth_cert(certs, key)?)
    } else {
        Ok(builder.with_no_client_auth())
    }
}

// ── Proxy startup ────────────────────────────────────────────────────────────

pub async fn start_proxy(config: AppConfig) -> anyhow::Result<(u16, ProxyState)> {
    let client = if config.socket_path.is_some() {
        None
    } else {
        Some(build_client(&config)?)
    };

    let state: ProxyState =
        Arc::new(ArcSwap::from_pointee(ProxyStateInner { config, client }));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("binding proxy listener")?;
    let port = listener.local_addr()?.port();
    let router = build_router(state.clone(), port);

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!("proxy server error: {e}");
        }
    });

    Ok((port, state))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name,
        "connection" | "keep-alive" | "proxy-authenticate" | "proxy-authorization"
            | "te" | "trailers" | "transfer-encoding" | "upgrade" | "host"
    )
}

#[derive(Debug)]
struct NoVerifier;

impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(&self, _: &rustls::pki_types::CertificateDer<'_>, _: &[rustls::pki_types::CertificateDer<'_>], _: &rustls::pki_types::ServerName<'_>, _: &[u8], _: rustls::pki_types::UnixTime) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, _: &[u8], _: &rustls::pki_types::CertificateDer<'_>, _: &rustls::DigitallySignedStruct) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(&self, _: &[u8], _: &rustls::pki_types::CertificateDer<'_>, _: &rustls::DigitallySignedStruct) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider().signature_verification_algorithms.supported_schemes()
    }
}
