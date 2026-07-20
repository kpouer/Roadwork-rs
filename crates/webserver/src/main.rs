use axum::{
    Router,
    body::Body,
    http::{Response, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use std::net::SocketAddr;

include!(concat!(env!("OUT_DIR"), "/static_files.rs"));

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/health", get(|| async { "ok" }))
        .fallback(static_handler);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("Listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn find_file(name: &str) -> Option<&'static [u8]> {
    STATIC_FILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, data)| *data)
}

async fn index_handler() -> impl IntoResponse {
    match find_file("index.html") {
        Some(data) => Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(data))
            .unwrap(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match find_file(path) {
        Some(data) => {
            let mime = mime_from_path(path);
            Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .body(Body::from(data))
                .unwrap()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn mime_from_path(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}
