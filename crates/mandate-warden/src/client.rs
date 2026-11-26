//! Agent side of the Warden protocol: sign one request with the agent's key
//! and send it. Shared by `warden-call` and `mandate-agent`.

use std::time::{SystemTime, UNIX_EPOCH};

use mandate_crypto::{address_of, sign_message, signing_key_from_hex, SigningKey};

use crate::auth;

/// What came back from the Warden.
pub struct Reply {
    pub status: u16,
    /// `allow`, `deny`, or `-` when the Warden made no decision (bad signature, etc.).
    pub decision: String,
    pub body: String,
}

pub struct WardenClient {
    http: reqwest::Client,
    base: String,
    key: SigningKey,
}

impl WardenClient {
    /// Build from `MANDATE_AGENT_KEY` and `MANDATE_WARDEN_URL`.
    pub fn from_env() -> Result<Self, String> {
        let hex = std::env::var("MANDATE_AGENT_KEY")
            .map_err(|_| "MANDATE_AGENT_KEY is not set".to_string())?;
        let key = signing_key_from_hex(hex.trim())
            .map_err(|e| format!("bad MANDATE_AGENT_KEY: {e:?}"))?;
        let base = std::env::var("MANDATE_WARDEN_URL")
            .map_err(|_| "MANDATE_WARDEN_URL is not set".to_string())?;
        Ok(Self {
            http: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_string(),
            key,
        })
    }

    pub fn address(&self) -> String {
        address_of(&self.key).0
    }

    /// Send `method` to `/v1/gateway/<provider>/<path>` with an optional JSON body.
    pub async fn call(
        &self,
        provider: &str,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Result<Reply, String> {
        let method = method.to_ascii_uppercase();
        let path_and_query = format!("/v1/gateway/{provider}/{}", path.trim_start_matches('/'));
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let digest = auth::request_digest(&method, &path_and_query, ts, body);
        let signature = sign_message(&digest, &self.key).map_err(|e| format!("sign: {e:?}"))?;
        let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| "bad HTTP method")?;
        let mut req = self
            .http
            .request(m, format!("{}{path_and_query}", self.base))
            .header("x-mandate-agent", self.address())
            .header("x-mandate-timestamp", ts.to_string())
            .header(
                "x-mandate-signature",
                format!("0x{}", hex::encode(signature)),
            );
        if !body.is_empty() {
            req = req
                .header("content-type", "application/json")
                .body(body.to_vec());
        }
        let res = req
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        let status = res.status().as_u16();
        let decision = res
            .headers()
            .get("x-mandate-decision")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-")
            .to_string();
        let body = res.text().await.unwrap_or_default();
        Ok(Reply {
            status,
            decision,
            body,
        })
    }
}
