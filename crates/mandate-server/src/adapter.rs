//! `Backend` backed by Arkiv, through the TypeScript adapter's HTTP API
//! (`apps/arkiv-adapter`).

use mandate_core::{Mandate, MandateId, PrincipalId, ResourceId, Revocation};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::backend::{ArkivRef, Backend, BackendError, Published, Result, Stored};

/// The adapter's limit on IDs per batched lookup.
const MAX_IDS_PER_QUERY: usize = 50;

pub struct AdapterBackend {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct Documents {
    documents: Vec<StoredDocument>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredDocument {
    entity_key: Option<String>,
    creator: Option<String>,
    document: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishResponse {
    entity_key: Option<String>,
    tx_hash: Option<String>,
    created: bool,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
    message: String,
}

fn unavailable(what: impl std::fmt::Display) -> BackendError {
    BackendError::Unavailable(what.to_string())
}

/// Turn an unsuccessful adapter response into an error. A 4xx with an error
/// code is a rejection; anything else means the state couldn't be read.
async fn error_from(res: reqwest::Response) -> BackendError {
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    match serde_json::from_str::<ErrorBody>(&body) {
        Ok(e) if status.is_client_error() => BackendError::Rejected {
            code: e.error,
            message: e.message,
        },
        _ => unavailable(format!("adapter {status}: {body}")),
    }
}

impl AdapterBackend {
    pub fn new(base: impl Into<String>, token: Option<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| unavailable(format!("http client: {e}")))?;
        Ok(Self {
            base: base.into().trim_end_matches('/').to_string(),
            token,
            http,
        })
    }

    /// GET a document list. Entries that aren't a valid `T` are skipped:
    /// anyone can write into the namespace.
    async fn get_docs<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Vec<Stored<T>>> {
        let res = self
            .http
            .get(format!("{}{path}", self.base))
            .query(query)
            .send()
            .await
            .map_err(|e| unavailable(format!("adapter unreachable: {e}")))?;
        if !res.status().is_success() {
            return Err(error_from(res).await);
        }
        let docs: Documents = res
            .json()
            .await
            .map_err(|e| unavailable(format!("adapter returned invalid JSON: {e}")))?;
        Ok(docs
            .documents
            .into_iter()
            .filter_map(|d| {
                let doc = serde_json::from_value(d.document).ok()?;
                Some(Stored {
                    doc,
                    arkiv: ArkivRef {
                        entity_key: d.entity_key,
                        creator: d
                            .creator
                            .and_then(|c| mandate_crypto::parse_address(&c).ok()),
                    },
                })
            })
            .collect())
    }

    async fn post(&self, path: &str, query: &[(&str, String)], body: Vec<u8>) -> Result<Published> {
        let mut req = self
            .http
            .post(format!("{}{path}", self.base))
            .query(query)
            .header("content-type", "application/json")
            .body(body);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let res = req
            .send()
            .await
            .map_err(|e| unavailable(format!("adapter unreachable: {e}")))?;
        if !res.status().is_success() {
            return Err(error_from(res).await);
        }
        let p: PublishResponse = res
            .json()
            .await
            .map_err(|e| unavailable(format!("adapter returned invalid JSON: {e}")))?;
        Ok(Published {
            entity_key: p.entity_key,
            tx_hash: p.tx_hash,
            created: p.created,
        })
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|e| unavailable(format!("encode: {e}")))
}

impl Backend for AdapterBackend {
    fn name(&self) -> &'static str {
        "arkiv"
    }

    async fn mandates_by_ids(&self, ids: &[MandateId]) -> Result<Vec<Stored<Mandate>>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(MAX_IDS_PER_QUERY) {
            let query: Vec<(&str, &str)> = chunk.iter().map(|id| ("id", id.0.as_str())).collect();
            out.extend(self.get_docs::<Mandate>("/v1/mandates", &query).await?);
        }
        Ok(out)
    }

    async fn mandates_for_subject(
        &self,
        subject: &PrincipalId,
        action: Option<&str>,
        resource: Option<&ResourceId>,
        active_at: Option<u64>,
    ) -> Result<Vec<Stored<Mandate>>> {
        let resource = resource.map(ToString::to_string);
        let mut query = vec![("subject", subject.0.as_str())];
        if let Some(a) = action {
            query.push(("action", a));
        }
        if let Some(r) = &resource {
            query.push(("resource", r.as_str()));
        }
        // The adapter evaluates "active" against its own clock.
        if active_at.is_some() {
            query.push(("active", "1"));
        }
        self.get_docs("/v1/mandates", &query).await
    }

    async fn revocations_for(
        &self,
        targets: &[(MandateId, PrincipalId)],
    ) -> Result<Vec<Stored<Revocation>>> {
        let mut out = Vec::new();
        for chunk in targets.chunks(MAX_IDS_PER_QUERY) {
            let pairs: Vec<String> = chunk
                .iter()
                .map(|(id, issuer)| format!("{}:{}", id.0, issuer.0))
                .collect();
            let query: Vec<(&str, &str)> = pairs.iter().map(|p| ("of", p.as_str())).collect();
            out.extend(
                self.get_docs::<Revocation>("/v1/revocations", &query)
                    .await?,
            );
        }
        Ok(out)
    }

    async fn publish_mandate(&self, mandate: &Mandate) -> Result<Published> {
        self.post("/v1/mandates", &[], to_json(mandate)?).await
    }

    async fn publish_revocation(
        &self,
        revocation: &Revocation,
        retain_until: u64,
    ) -> Result<Published> {
        self.post(
            "/v1/revocations",
            &[("retain_until", retain_until.to_string())],
            to_json(revocation)?,
        )
        .await
    }
}
