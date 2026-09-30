//! Provider-neutral HTTP transport. A non-2xx status is still a response.

use crate::retry::{RetryPolicy, retryable_status};
use std::time::Duration;

pub use reqwest::Method;
pub use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: HeaderMap,
    pub body: Value,
    pub timeout: Option<Duration>,
}

pub struct Response {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

pub struct StreamResponse {
    response: reqwest::Response,
}

impl StreamResponse {
    pub fn status(&self) -> u16 {
        self.response.status().as_u16()
    }

    pub fn headers(&self) -> &HeaderMap {
        self.response.headers()
    }

    /// Raw chunks are not guaranteed to align with JSON or SSE frame boundaries.
    pub async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, reqwest::Error> {
        self.response
            .chunk()
            .await
            .map(|chunk| chunk.map(|b| b.to_vec()))
    }
}

#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    retry: RetryPolicy,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            retry: RetryPolicy::default(),
        }
    }

    pub fn with_retry_policy(retry: RetryPolicy) -> Self {
        Self {
            client: reqwest::Client::new(),
            retry,
        }
    }

    pub async fn get(&self, url: &str, headers: HeaderMap) -> Result<Response, reqwest::Error> {
        let response = self
            .client
            .get(url)
            .headers(headers)
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response.bytes().await?.to_vec();
        Ok(Response {
            status,
            headers,
            body,
        })
    }

    pub async fn send(&self, request: Request) -> Result<Response, reqwest::Error> {
        let response = self.start(request).await?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.response.bytes().await?.to_vec();
        Ok(Response {
            status,
            headers,
            body,
        })
    }

    pub async fn stream(&self, request: Request) -> Result<StreamResponse, reqwest::Error> {
        self.start(request).await
    }

    async fn start(&self, request: Request) -> Result<StreamResponse, reqwest::Error> {
        let mut builder = self
            .client
            .request(request.method.clone(), &request.url)
            .headers(request.headers.clone())
            .json(&request.body);
        if let Some(timeout) = request.timeout {
            builder = builder.timeout(timeout);
        }
        let request = builder.build()?;
        let mut retry = 0;
        loop {
            // These requests have replayable JSON bodies. No adapter state is advanced here.
            let attempt = request.try_clone().expect("JSON HTTP request is cloneable");
            match self.client.execute(attempt).await {
                Ok(response) => {
                    if retryable_status(response.status().as_u16()) {
                        if let Some(delay) = self.retry.delay(retry, Some(response.headers())) {
                            drop(response);
                            retry += 1;
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                    }
                    return Ok(StreamResponse { response });
                }
                Err(error) => {
                    // Timeouts/read errors may follow acceptance by the provider; do not replay them.
                    if error.is_connect() {
                        if let Some(delay) = self.retry.delay(retry, None) {
                            retry += 1;
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                    }
                    return Err(error);
                }
            }
        }
    }
}
