//! `mandate-server`: the Mandate HTTP API.
//!
//! Environment:
//!   ADAPTER_URL     Arkiv adapter (default http://127.0.0.1:8787)
//!   ADAPTER_TOKEN   bearer token for adapter writes, if it requires one
//!   TRUST_ANCHORS   who may issue root mandates: kind:id=0xaddr[,0xaddr];…
//!   MANDATE_BACKEND `arkiv` (default) or `memory` (no Arkiv, for local trials)
//!   HOST, PORT      listen address (default 127.0.0.1:8080)
//!   RUST_LOG        log filter (default info)

use std::process::ExitCode;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use mandate_server::adapter::AdapterBackend;
use mandate_server::backend::{Backend, MemoryBackend};
use mandate_server::config::parse_trust_anchors;
use mandate_server::{router, AppState, Clock};
use tracing::{error, info};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn system_clock() -> Clock {
    Arc::new(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    })
}

async fn serve<B: Backend>(backend: B, addr: String) -> Result<(), String> {
    let anchors = parse_trust_anchors(&env("TRUST_ANCHORS").unwrap_or_default())
        .map_err(|e| format!("TRUST_ANCHORS: {e}"))?;
    let app = router(AppState {
        backend,
        anchors,
        clock: system_clock(),
    });
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;
    info!("mandate-server listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| format!("server: {e}"))
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let addr = format!(
        "{}:{}",
        env("HOST").unwrap_or_else(|| "127.0.0.1".into()),
        env("PORT").unwrap_or_else(|| "8080".into())
    );
    let result = match env("MANDATE_BACKEND").as_deref().unwrap_or("arkiv") {
        "memory" => {
            info!("backend: in-memory (state is lost on restart, nothing goes to Arkiv)");
            serve(MemoryBackend::new(), addr).await
        }
        "arkiv" => {
            let url = env("ADAPTER_URL").unwrap_or_else(|| "http://127.0.0.1:8787".into());
            info!("backend: Arkiv via adapter at {url}");
            match AdapterBackend::new(url, env("ADAPTER_TOKEN")) {
                Ok(backend) => serve(backend, addr).await,
                Err(e) => Err(e.to_string()),
            }
        }
        other => Err(format!(
            "MANDATE_BACKEND must be `arkiv` or `memory`, not `{other}`"
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    }
}
