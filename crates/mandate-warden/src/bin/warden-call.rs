//! `warden-call`: make one signed GitHub call through the Mandate Warden.
//!
//! The agent's only network tool. It holds the agent's own signing key
//! (never a GitHub token) and signs every request so the Warden knows
//! exactly which agent is asking.
//!
//! ```text
//! warden-call keygen                       # print a new agent key + address
//! warden-call address                      # address of $MANDATE_AGENT_KEY
//! warden-call GET  repos/OWNER/REPO/issues
//! warden-call POST repos/OWNER/REPO/issues '{"title":"hi"}'
//! ```
//! Env: `MANDATE_AGENT_KEY` (hex), `MANDATE_WARDEN_URL` (e.g. https://mandate-warden-xxxx.sprites.app).

#[path = "../auth.rs"]
#[allow(dead_code)]
mod auth;

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use mandate_crypto::{
    address_of, generate_signing_key, sign_message, signing_key_from_hex, signing_key_to_hex,
};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("keygen") => {
            let key = generate_signing_key();
            println!("MANDATE_AGENT_KEY={}", signing_key_to_hex(&key));
            println!("address={}", address_of(&key));
            ExitCode::SUCCESS
        }
        Some("address") => match load_key() {
            Ok(key) => {
                println!("{}", address_of(&key));
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        Some(method) if args.len() >= 2 => {
            call(method, &args[1], args.get(2).map(String::as_str)).await
        }
        _ => fail("usage: warden-call keygen | address | <METHOD> <github-path> [json-body]"),
    }
}

fn load_key() -> Result<mandate_crypto::SigningKey, String> {
    let hex = std::env::var("MANDATE_AGENT_KEY")
        .map_err(|_| "MANDATE_AGENT_KEY is not set".to_string())?;
    signing_key_from_hex(hex.trim()).map_err(|e| format!("bad MANDATE_AGENT_KEY: {e:?}"))
}

async fn call(method: &str, path: &str, body: Option<&str>) -> ExitCode {
    let key = match load_key() {
        Ok(k) => k,
        Err(e) => return fail(&e),
    };
    let Ok(base) = std::env::var("MANDATE_WARDEN_URL") else {
        return fail("MANDATE_WARDEN_URL is not set");
    };
    let method = method.to_ascii_uppercase();
    let path_and_query = format!("/v1/gateway/github/{}", path.trim_start_matches('/'));
    let body = body.unwrap_or("").as_bytes().to_vec();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let digest = auth::request_digest(&method, &path_and_query, ts, &body);
    let signature = match sign_message(&digest, &key) {
        Ok(s) => s,
        Err(e) => return fail(&format!("sign: {e:?}")),
    };

    let client = reqwest::Client::new();
    let Ok(m) = reqwest::Method::from_bytes(method.as_bytes()) else {
        return fail("bad HTTP method");
    };
    let mut req = client
        .request(
            m,
            format!("{}{}", base.trim_end_matches('/'), path_and_query),
        )
        .header("x-mandate-agent", address_of(&key).0)
        .header("x-mandate-timestamp", ts.to_string())
        .header(
            "x-mandate-signature",
            format!("0x{}", hex::encode(signature)),
        );
    if !body.is_empty() {
        req = req.header("content-type", "application/json").body(body);
    }
    match req.send().await {
        Ok(res) => {
            let status = res.status();
            let decision = res
                .headers()
                .get("x-mandate-decision")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("-")
                .to_string();
            let text = res.text().await.unwrap_or_default();
            eprintln!("HTTP {} · mandate: {decision}", status.as_u16());
            println!("{text}");
            if status.is_success() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            }
        }
        Err(e) => fail(&format!("request failed: {e}")),
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("warden-call: {msg}");
    ExitCode::FAILURE
}
