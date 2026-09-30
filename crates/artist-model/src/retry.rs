//! Bounded retries before response processing begins. Never restart an SSE stream.

use crate::http::HeaderMap;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct RetryPolicy {
    /// Additional attempts after the initial request; zero disables retries.
    pub max_retries: usize,
    pub initial_delay: Duration,
    /// If Retry-After exceeds this limit, return the response instead of retrying early.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            initial_delay: Duration::from_millis(250),
            max_delay: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    pub(crate) fn delay(&self, retry: usize, headers: Option<&HeaderMap>) -> Option<Duration> {
        if retry >= self.max_retries {
            return None;
        }
        let backoff = self
            .initial_delay
            .saturating_mul(1_u32.checked_shl(retry.min(31) as u32).unwrap_or(u32::MAX))
            .min(self.max_delay);
        if let Some(value) = headers.and_then(|headers| headers.get("retry-after")) {
            let value = value.to_str().ok()?;
            let delay = if let Ok(seconds) = value.parse::<u64>() {
                Duration::from_secs(seconds)
            } else {
                httpdate::parse_http_date(value)
                    .ok()?
                    .duration_since(std::time::SystemTime::now())
                    .unwrap_or_default()
            };
            if delay > self.max_delay {
                return None;
            }
            return Some(delay.max(backoff));
        }
        Some(backoff)
    }
}

pub(crate) fn retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504 | 529)
}
