//! Native Turso's default IO and fsync are synchronous. Keep entire operations
//! off the caller's executor, not just advisory-lock acquisition.
use std::future::Future;

pub(crate) async fn run<T, F, Fut>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, String>>,
{
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?
            .block_on(operation())
    })
    .await
    .map_err(|error| format!("session storage worker failed: {error}"))?
}
