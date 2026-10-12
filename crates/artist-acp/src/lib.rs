//! Client-independent ACP v1 frontend over JSON-RPC stdio.
mod agent;
mod events;
mod session;

use agent::ArtistAgent;
use agent_client_protocol::AgentSideConnection;
use std::path::PathBuf;
use std::rc::Rc;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[derive(Clone, Default)]
pub struct Options {
    pub config: Option<PathBuf>,
    pub model: Option<String>,
    pub reasoning: Option<String>,
}

/// Owns stdout until EOF or Ctrl-C. Invoke inside a Tokio runtime.
pub async fn serve(mut options: Options) -> Result<(), String> {
    if let Some(config) = &options.config {
        options.config = Some(std::fs::canonicalize(config).map_err(|e| e.to_string())?);
    }
    tokio::task::LocalSet::new()
        .run_until(async move {
            let agent = Rc::new(ArtistAgent::new(options));
            let (connection, io) = AgentSideConnection::new(
                agent.clone(),
                tokio::io::stdout().compat_write(),
                tokio::io::stdin().compat(),
                |task| {
                    tokio::task::spawn_local(task);
                },
            );
            agent.set_connection(Rc::new(connection));
            let result = tokio::select! {
                result = io => result.map_err(|e| e.to_string()),
                result = tokio::signal::ctrl_c() => result.map_err(|e| e.to_string()),
            };
            agent.disconnect();
            result
        })
        .await
}
