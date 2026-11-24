//! Talking to Fly.io: the connector gateway (outbound calls) and the
//! Sprites management API (checkpointing and restoring the agent).

use reqwest::{Client, Method};

#[derive(Clone)]
pub struct Sprites {
    http: Client,
    gateway_base: String,
    api_base: String,
    token: Option<String>,
}

pub struct Upstream {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

impl Sprites {
    pub fn new(gateway_base: String, api_base: String, token: Option<String>) -> Self {
        Self {
            http: Client::new(),
            gateway_base,
            api_base,
            token,
        }
    }

    pub fn can_manage(&self) -> bool {
        self.token.is_some()
    }

    /// Forward a call through a connector. Runs inside the Warden Sprite,
    /// so the gateway attaches the provider credential; we never hold it.
    pub async fn forward(
        &self,
        provider: &str,
        connection: &str,
        method: &str,
        path_and_query: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> Result<Upstream, String> {
        let url = format!(
            "{}/{provider}/{connection}/{}",
            self.gateway_base,
            path_and_query.trim_start_matches('/')
        );
        let method = Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
        let mut req = self
            .http
            .request(method, url)
            .header("accept", "application/vnd.github+json");
        if let Some(ct) = content_type {
            req = req.header("content-type", ct);
        }
        if !body.is_empty() {
            req = req.body(body);
        }
        let res = req.send().await.map_err(|e| e.to_string())?;
        let status = res.status().as_u16();
        let content_type = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let body = res.bytes().await.map_err(|e| e.to_string())?.to_vec();
        Ok(Upstream {
            status,
            content_type,
            body,
        })
    }

    /// Checkpoint a Sprite's whole disk. Returns the raw NDJSON progress.
    pub async fn checkpoint(&self, sprite: &str, comment: &str) -> Result<String, String> {
        let url = format!("{}/v1/sprites/{sprite}/checkpoint", self.api_base);
        self.manage(
            Method::POST,
            &url,
            Some(serde_json::json!({ "comment": comment })),
        )
        .await
    }

    pub async fn list_checkpoints(&self, sprite: &str) -> Result<serde_json::Value, String> {
        let url = format!("{}/v1/sprites/{sprite}/checkpoints", self.api_base);
        let text = self.manage(Method::GET, &url, None).await?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }

    /// Roll a Sprite's disk back to `checkpoint`. Destructive: it restarts
    /// the Sprite and ends its sessions.
    pub async fn restore(&self, sprite: &str, checkpoint: &str) -> Result<String, String> {
        let url = format!(
            "{}/v1/sprites/{sprite}/checkpoints/{checkpoint}/restore",
            self.api_base
        );
        self.manage(Method::POST, &url, None).await
    }

    async fn manage(
        &self,
        method: Method,
        url: &str,
        json: Option<serde_json::Value>,
    ) -> Result<String, String> {
        let token = self.token.as_ref().ok_or("SPRITES_TOKEN not configured")?;
        let mut req = self.http.request(method, url).bearer_auth(token);
        if let Some(json) = json {
            req = req.json(&json);
        }
        let res = req.send().await.map_err(|e| e.to_string())?;
        let status = res.status();
        let text = res.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("{status}: {text}"));
        }
        Ok(text)
    }
}
