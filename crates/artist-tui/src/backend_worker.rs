//! A single local task owns the non-Send Lua runtime and the conversation.

use artist_core::cancellation::CancellationToken;
use artist_core::engine::AgentEvent;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::task::JoinHandle;

use crate::backend::Backend;

struct Prompt {
    text: String,
    cancellation: CancellationToken,
}

pub struct Connection {
    prompts: UnboundedSender<Prompt>,
    worker: JoinHandle<()>,
}

impl Connection {
    /// Must be called inside a Tokio LocalSet; no JSON crosses this boundary.
    pub fn start(mut backend: Backend, events: UnboundedSender<Vec<AgentEvent>>) -> Self {
        let (prompts, mut requests) = mpsc::unbounded_channel::<Prompt>();
        let worker = tokio::task::spawn_local(async move {
            while let Some(prompt) = requests.recv().await {
                backend
                    .run_prompt(prompt.text, &prompt.cancellation, |batch| {
                        let _ = events.send(batch);
                    })
                    .await;
            }
        });
        Self { prompts, worker }
    }

    pub fn submit(&self, text: String, cancellation: CancellationToken) -> Result<(), String> {
        self.prompts
            .send(Prompt { text, cancellation })
            .map_err(|_| "backend worker stopped".into())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // Dropping the worker future also drops active HTTP/tool futures.
        self.worker.abort();
    }
}
