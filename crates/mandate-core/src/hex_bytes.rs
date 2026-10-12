//! Serde helpers that encode byte vectors as `0x`-prefixed hex strings,
//! matching the format used in Arkiv payloads.

use serde::{de, Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("0x{}", hex::encode(bytes)))
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let s = String::deserialize(deserializer)?;
    let digits = s
        .strip_prefix("0x")
        .ok_or_else(|| de::Error::custom("hex string must start with 0x"))?;
    hex::decode(digits).map_err(de::Error::custom)
}
