//! Serves the compiled MyLib web frontend (`web/dist`) that is embedded into the
//! binary at compile time. In production the same Axum process answers both
//! `/api/*` (JSON API) and every other path (the React SPA), so MyLib ships and
//! runs as a single product on one host and port.

use axum::{
    body::Body,
    http::{HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/dist"]
struct WebDist;

const INDEX: &str = "index.html";

/// Router fallback: static asset when it exists, otherwise the SPA entrypoint.
/// Unknown `/api` paths are never rewritten to `index.html`.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    if path.starts_with("api/") || path == "api" || path == "health" {
        return json_not_found();
    }

    if !path.is_empty()
        && let Some(response) = asset_response(path)
    {
        return response;
    }

    match asset_response(INDEX) {
        Some(response) => response,
        // The frontend has not been built into this binary.
        None => (
            StatusCode::NOT_FOUND,
            "MyLib web frontend is not embedded in this build. Run `npm --prefix web ci && npm --prefix web run build` before `cargo build`.",
        )
            .into_response(),
    }
}

fn asset_response(path: &str) -> Option<Response> {
    let file = WebDist::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path == INDEX {
        "no-cache"
    } else if path.starts_with("assets/") {
        // Vite emits content-hashed filenames under /assets.
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    };

    let mut response = Response::builder()
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(mime.as_ref()).ok()?,
        )
        .header(header::CACHE_CONTROL, HeaderValue::from_static(cache))
        .body(Body::from(file.data.into_owned()))
        .ok()?;
    response
        .headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    Some(response.into_response())
}

fn json_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"error":{"code":"NOT_FOUND","message":"Resource not found.","requestId":"-"}}"#,
    )
        .into_response()
}
