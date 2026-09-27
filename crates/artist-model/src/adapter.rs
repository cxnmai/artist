//! Provider-specific schema translation. Adapters own any streaming frame buffer.

use crate::event::ModelEvent;
use crate::http::{HeaderMap, Request, Response};
use serde_json::Value;

pub trait Adapter {
    type Error;

    /// Build the entire provider request from caller-supplied data.
    fn request(&self, input: &Value) -> Result<Request, Self::Error>;

    /// Interpret status, headers, and JSON (or other) response bytes.
    fn response(&mut self, response: Response) -> Result<Vec<ModelEvent>, Self::Error>;

    /// Stream chunks may split a JSON token, SSE frame, or UTF-8 character.
    fn stream_start(
        &mut self,
        _status: u16,
        _headers: &HeaderMap,
    ) -> Result<Vec<ModelEvent>, Self::Error> {
        Ok(Vec::new())
    }
    fn stream_chunk(&mut self, chunk: &[u8]) -> Result<Vec<ModelEvent>, Self::Error>;
    fn stream_end(&mut self) -> Result<Vec<ModelEvent>, Self::Error> {
        Ok(Vec::new())
    }
}
