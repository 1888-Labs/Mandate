//! The web UI: the built React app in `apps/web/dist`, served from disk.

use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

/// Shown when the server can't find a built web UI.
pub async fn not_built() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        "<!doctype html><meta charset=utf-8><title>Mandate</title>\
         <body style=\"font-family:system-ui;max-width:640px;margin:60px auto;padding:0 16px\">\
         <h1>Mandate API is running</h1>\
         <p>The web UI hasn't been built. Run <code>npm install &amp;&amp; npm run build</code> \
         in <code>apps/web</code>, or set <code>WEB_DIR</code> to a built copy.</p>\
         <p>The API is under <code>/v1</code>: see \
         <a href=\"https://github.com/dicethedev/Mandate/blob/main/docs/api.md\">docs/api.md</a>.</p>",
    )
}
