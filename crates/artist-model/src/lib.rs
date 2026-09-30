//! Arbitrary model APIs over one reusable HTTP client.

pub mod adapter;
pub mod adapters;
pub mod http;
pub mod json;
pub mod retry;

use adapter::Adapter;
use artist_core::context::{ContextEntry, Conversation, SelectedContext};
use artist_core::event::ModelEvent;
use artist_core::tools::ToolDefinition;
use http::{HttpClient, Response};

/// The portion of the conversation selected for one model request.
pub struct ModelInput<'a> {
    pub system_prompt: &'a str,
    pub entries: &'a [ContextEntry],
    pub tools: &'a [ToolDefinition],
}

impl<'a> From<&'a Conversation> for ModelInput<'a> {
    fn from(conversation: &'a Conversation) -> Self {
        Self {
            system_prompt: &conversation.system_prompt,
            entries: &conversation.entries,
            tools: &[],
        }
    }
}

impl<'a> From<&'a SelectedContext> for ModelInput<'a> {
    fn from(context: &'a SelectedContext) -> Self {
        Self {
            system_prompt: &context.system_prompt,
            entries: &context.entries,
            tools: &[],
        }
    }
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

    /// Configure retries before model response processing. Zero retries disables them.
    pub fn with_retry_policy(policy: retry::RetryPolicy) -> Self {
        Self {
            http: HttpClient::with_retry_policy(policy),
        }
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
        mut on_events: impl FnMut(Vec<ModelEvent>),
    ) -> Result<(), ModelError<A::Error>> {
        let request = adapter.request(input).map_err(ModelError::Adapter)?;
        let mut response = self
            .http
            .stream(request)
            .await
            .map_err(ModelError::Transport)?;
        if !(200..300).contains(&response.status()) {
            let status = response.status();
            let headers = response.headers().clone();
            let mut body = Vec::new();
            while let Some(chunk) = response.next_chunk().await.map_err(ModelError::Transport)? {
                body.extend_from_slice(&chunk);
                if body.len() > 1024 * 1024 {
                    break;
                }
            }
            let events = adapter
                .response(Response {
                    status,
                    headers,
                    body,
                })
                .map_err(ModelError::Adapter)?;
            if !events.is_empty() {
                on_events(events);
            }
            return Ok(());
        }
        let events = adapter
            .stream_start(response.status(), response.headers())
            .map_err(ModelError::Adapter)?;
        if !events.is_empty() {
            on_events(events);
        }
        while let Some(chunk) = response.next_chunk().await.map_err(ModelError::Transport)? {
            let events = adapter.stream_chunk(&chunk).map_err(ModelError::Adapter)?;
            if !events.is_empty() {
                on_events(events);
            }
        }
        let events = adapter.stream_end().map_err(ModelError::Adapter)?;
        if !events.is_empty() {
            on_events(events);
        }
        Ok(())
    }
}
