//! `warden-call`: make one signed call through the Mandate Warden.
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
//! warden-call --ollama POST api/chat '{"model":"gpt-oss:120b","messages":[...]}'
//! ```
//! Env: `MANDATE_AGENT_KEY` (hex), `MANDATE_WARDEN_URL` (e.g. https://mandate-warden-xxxx.sprites.app).

#[path = "../auth.rs"]
#[allow(dead_code)]
mod auth;
#[path = "../client.rs"]
mod client;

use std::process::ExitCode;

use mandate_crypto::{address_of, generate_signing_key, signing_key_to_hex};

use crate::client::WardenClient;

#[tokio::main]
async fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let provider = if args.first().map(String::as_str) == Some("--ollama") {
        args.remove(0);
        "ollama"
    } else {
        "github"
    };
    match args.first().map(String::as_str) {
        Some("keygen") => {
            let key = generate_signing_key();
            println!("MANDATE_AGENT_KEY={}", signing_key_to_hex(&key));
            println!("address={}", address_of(&key));
            ExitCode::SUCCESS
        }
        Some("address") => match WardenClient::from_env() {
            Ok(c) => {
                println!("{}", c.address());
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        Some(method) if args.len() >= 2 => {
            let client = match WardenClient::from_env() {
                Ok(c) => c,
                Err(e) => return fail(&e),
            };
            let body = args.get(2).map(String::as_bytes).unwrap_or_default();
            match client.call(provider, method, &args[1], body).await {
                Ok(r) => {
                    eprintln!("HTTP {} · mandate: {}", r.status, r.decision);
                    println!("{}", r.body);
                    if (200..300).contains(&r.status) {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(2)
                    }
                }
                Err(e) => fail(&e),
            }
        }
        _ => fail("usage: warden-call keygen | address | [--ollama] <METHOD> <path> [json-body]"),
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("warden-call: {msg}");
    ExitCode::FAILURE
}
