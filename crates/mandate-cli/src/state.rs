//! Local CLI state, kept in a directory (default `.mandate/`).
//!
//! - `state.json`: public data only. Principals' names and addresses,
//!   resource owners, signed mandates and revocations, and `M1`-style
//!   aliases. Nothing secret, the same data that will go on Arkiv.
//! - `keys.json`: development private keys, by principal name. Written
//!   with owner-only permissions. Never commit or publish it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use mandate_core::{
    Mandate, MandateId, MemoryMandateStore, PrincipalId, ResourceId, Revocation, TrustAnchors,
};
use mandate_crypto::{signing_key_from_hex, signing_key_to_hex, SigningKey};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::error::CliError;

const STATE_FILE: &str = "state.json";
const KEYS_FILE: &str = "keys.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    /// Display name → address.
    #[serde(default)]
    pub principals: BTreeMap<String, PrincipalId>,
    /// Resource (`kind:id`) → principals trusted to issue root mandates.
    #[serde(default)]
    pub owners: BTreeMap<String, BTreeSet<PrincipalId>>,
    #[serde(default)]
    pub mandates: Vec<Mandate>,
    #[serde(default)]
    pub revocations: Vec<Revocation>,
    /// `aliases[i]` is mandate `M{i+1}`.
    #[serde(default)]
    pub aliases: Vec<MandateId>,
}

impl State {
    pub fn load(home: &Path) -> Result<Self, CliError> {
        read_json(&home.join(STATE_FILE))
    }

    pub fn save(&self, home: &Path) -> Result<(), CliError> {
        write_json(home, &home.join(STATE_FILE), self, false)
    }

    pub fn store(&self) -> MemoryMandateStore {
        let mut store = MemoryMandateStore::new();
        for m in &self.mandates {
            store.insert_mandate(m.clone());
        }
        for r in &self.revocations {
            store.insert_revocation(r.clone());
        }
        store
    }

    pub fn anchors(&self) -> TrustAnchors {
        let mut anchors = TrustAnchors::new();
        for (resource, owners) in &self.owners {
            if let Ok(resource) = resource.parse::<ResourceId>() {
                for owner in owners {
                    anchors = anchors.trust(resource.clone(), owner.clone());
                }
            }
        }
        anchors
    }

    pub fn address(&self, name: &str) -> Result<PrincipalId, CliError> {
        self.principals
            .get(name)
            .cloned()
            .ok_or_else(|| CliError::UnknownPrincipal(name.to_string()))
    }

    /// The display name for an address, or a shortened address.
    pub fn name_of(&self, p: &PrincipalId) -> String {
        self.principals
            .iter()
            .find(|(_, addr)| *addr == p)
            .map_or_else(|| short(&p.0), |(name, _)| name.clone())
    }

    /// Store a new mandate and give it the next `M<n>` alias.
    pub fn add_mandate(&mut self, m: Mandate) -> String {
        if let Some(existing) = self.alias_index(&m.id) {
            return format!("M{}", existing + 1);
        }
        self.aliases.push(m.id.clone());
        self.mandates.push(m);
        format!("M{}", self.aliases.len())
    }

    /// `M2`, or a shortened ID for mandates without an alias.
    pub fn alias(&self, id: &MandateId) -> String {
        self.alias_index(id)
            .map_or_else(|| short(&id.0), |i| format!("M{}", i + 1))
    }

    fn alias_index(&self, id: &MandateId) -> Option<usize> {
        self.aliases.iter().position(|a| a == id)
    }

    /// Resolve `M2`, a full mandate ID, or a unique ID prefix (≥ 6 chars).
    pub fn resolve(&self, reference: &str) -> Result<MandateId, CliError> {
        let unknown = || CliError::UnknownMandate(reference.to_string());
        if let Some(n) = reference
            .strip_prefix(['M', 'm'])
            .and_then(|n| n.parse::<usize>().ok())
        {
            return n
                .checked_sub(1)
                .and_then(|i| self.aliases.get(i))
                .cloned()
                .ok_or_else(unknown);
        }
        let needle = reference.to_ascii_lowercase();
        if !needle.starts_with("0x") || needle.len() < 6 {
            return Err(unknown());
        }
        let ids: BTreeSet<&MandateId> = self
            .mandates
            .iter()
            .map(|m| &m.id)
            .filter(|id| id.0.starts_with(&needle))
            .collect();
        match ids.len() {
            0 => Err(unknown()),
            1 => Ok(ids.into_iter().next().cloned().ok_or_else(unknown)?),
            _ => Err(CliError::AmbiguousMandate(reference.to_string())),
        }
    }
}

/// Development signing keys, by principal name.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Keys(BTreeMap<String, String>);

impl Keys {
    pub fn load(home: &Path) -> Result<Self, CliError> {
        read_json(&home.join(KEYS_FILE))
    }

    pub fn save(&self, home: &Path) -> Result<(), CliError> {
        write_json(home, &home.join(KEYS_FILE), self, true)
    }

    pub fn insert(&mut self, name: &str, key: &SigningKey) {
        self.0.insert(name.to_string(), signing_key_to_hex(key));
    }

    pub fn get(&self, name: &str) -> Result<SigningKey, CliError> {
        let hex = self
            .0
            .get(name)
            .ok_or_else(|| CliError::MissingKey(name.to_string()))?;
        Ok(signing_key_from_hex(hex)?)
    }
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_'))
}

/// `0x1234…cdef`
pub fn short(hex: &str) -> String {
    if hex.is_ascii() && hex.len() > 12 {
        format!("{}…{}", &hex[..6], &hex[hex.len() - 4..])
    } else {
        hex.to_string()
    }
}

fn read_json<T: DeserializeOwned + Default>(path: &Path) -> Result<T, CliError> {
    match fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| CliError::CorruptFile(path.into(), e))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(CliError::Io(path.into(), e)),
    }
}

/// Write via a temporary file and rename, so a crash never leaves a
/// half-written file. `secret` files are created owner-read/write only.
fn write_json<T: Serialize>(
    home: &Path,
    path: &Path,
    value: &T,
    secret: bool,
) -> Result<(), CliError> {
    create_home(home)?;
    let json =
        serde_json::to_vec_pretty(value).map_err(|e| CliError::CorruptFile(path.into(), e))?;
    let tmp: PathBuf = path.with_extension("json.tmp");
    let io = |e| CliError::Io(tmp.clone(), e);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if secret { 0o600 } else { 0o644 });
    }
    let mut file = options.open(&tmp).map_err(io)?;
    file.write_all(&json).map_err(io)?;
    file.write_all(b"\n").map_err(io)?;
    file.sync_all().map_err(io)?;
    fs::rename(&tmp, path).map_err(|e| CliError::Io(path.into(), e))
}

fn create_home(home: &Path) -> Result<(), CliError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(home)
        .map_err(|e| CliError::Io(home.into(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_restricted() {
        for ok in ["alice", "finance-agent", "agent_2"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in ["", "Alice", "a b", "a/b", "../x", "M1"] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn shortens_long_hex() {
        assert_eq!(
            short("0x7e5f4552091a69125d5dfcb7b8c2659029395bdf"),
            "0x7e5f…5bdf"
        );
        assert_eq!(short("0xabc"), "0xabc");
    }

    #[test]
    fn resolves_aliases_and_prefixes() {
        let mut state = State::default();
        let a = MandateId(format!("0xaaaa{}", "0".repeat(60)));
        let b = MandateId(format!("0xaaab{}", "0".repeat(60)));
        state.aliases = vec![a.clone(), b.clone()];
        state.mandates = Vec::new();
        assert_eq!(state.resolve("M1").unwrap(), a);
        assert_eq!(state.resolve("m2").unwrap(), b);
        assert!(state.resolve("M3").is_err());
        assert!(state.resolve("M0").is_err());
        assert!(state.resolve("nonsense").is_err());
    }
}
