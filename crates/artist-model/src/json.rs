//! JSON conversion at the Rust boundary, independent of any provider schema.

use serde::Serialize;
use serde::de::DeserializeOwned;

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> serde_json::Result<T> {
    serde_json::from_slice(bytes)
}

pub fn encode<T: Serialize>(value: &T) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(value)
}
