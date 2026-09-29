//! Per-turn cancellation shared between a frontend and the running engine.

use tokio::sync::watch;

#[derive(Clone, Debug)]
pub struct CancellationToken {
    sender: watch::Sender<bool>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        let (sender, _) = watch::channel(false);
        Self { sender }
    }
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.sender.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.sender.borrow()
    }

    pub async fn cancelled(&self) {
        let mut receiver = self.sender.subscribe();
        if *receiver.borrow() {
            return;
        }
        let _ = receiver.changed().await;
    }
}
