//! Stable corruption detection for a derived resume boundary, not authentication.
use crate::storage::error;
use sha2::{Digest, Sha256};

pub(crate) fn checksum(
    id: &str,
    sequence: i64,
    settings: &str,
    pending: &str,
) -> Result<String, String> {
    // Length-delimited JSON encoding binds both raw cached payloads, the session
    // identity and boundary. A copied row cannot seed a different session.
    let bytes = serde_json::to_vec(&("artist-resume-v1", id, sequence, settings, pending))
        .map_err(error)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
