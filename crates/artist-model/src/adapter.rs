//! Provider-specific schema translation. Adapters own any streaming frame buffer.

use crate::ModelInput;
use crate::event::ModelEvent;
use crate::http::{HeaderMap, Request, Response};

pub trait Adapter {
    type Error;

    /// Translate selected history into the provider's request schema.
    fn request(&self, input: &ModelInput<'_>) -> Result<Request, Self::Error>;

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
