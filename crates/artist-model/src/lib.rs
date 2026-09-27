//! Arbitrary model APIs over one reusable HTTP client.

pub mod adapter;
pub mod http;
pub mod json;

use adapter::Adapter;
use artist_core::context::ContextEntry;
use artist_core::event::ModelEvent;
use http::HttpClient;

/// The portion of the conversation selected for one model request.
pub struct ModelInput<'a> {
    pub system_prompt: &'a str,
    pub entries: &'a [ContextEntry],
}

#[derive(Debug)]
pub enum ModelError<E> {
    Transport(reqwest::Error),
    Adapter(E),
}

#[derive(Clone, Default)]
pub struct ModelClient {
    http: HttpClient,
}

impl ModelClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn complete<A: Adapter>(
        &self,
        adapter: &mut A,
        input: &ModelInput<'_>,
    ) -> Result<Vec<ModelEvent>, ModelError<A::Error>> {
        let request = adapter.request(input).map_err(ModelError::Adapter)?;
        let response = self
            .http
            .send(request)
            .await
            .map_err(ModelError::Transport)?;
        adapter.response(response).map_err(ModelError::Adapter)
    }

    /// Emits normalized events as they arrive. Dropping this future cancels the request.
    pub async fn stream<A: Adapter>(
        &self,
        adapter: &mut A,
        input: &ModelInput<'_>,
        mut on_event: impl FnMut(ModelEvent),
    ) -> Result<(), ModelError<A::Error>> {
        let request = adapter.request(input).map_err(ModelError::Adapter)?;
        let mut response = self
            .http
            .stream(request)
            .await
            .map_err(ModelError::Transport)?;
        for event in adapter
            .stream_start(response.status(), response.headers())
            .map_err(ModelError::Adapter)?
        {
            on_event(event);
        }
        while let Some(chunk) = response.next_chunk().await.map_err(ModelError::Transport)? {
            for event in adapter.stream_chunk(&chunk).map_err(ModelError::Adapter)? {
                on_event(event);
            }
        }
        for event in adapter.stream_end().map_err(ModelError::Adapter)? {
            on_event(event);
        }
        Ok(())
    }
}
