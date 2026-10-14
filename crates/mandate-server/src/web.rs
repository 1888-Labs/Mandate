//! The web UI, embedded so the server stays a single deployable binary.

use axum::http::header;
use axum::response::IntoResponse;

const INDEX: &str = include_str!("../../../apps/web/index.html");
const APP_JS: &str = include_str!("../../../apps/web/app.js");
const STYLE: &str = include_str!("../../../apps/web/style.css");

pub async fn index() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], INDEX)
}

pub async fn app_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        APP_JS,
    )
}

pub async fn style() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLE)
}
