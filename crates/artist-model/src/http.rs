//! Provider-neutral HTTP transport. A non-2xx status is still a response.

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
            .request(request.method, request.url)
            .headers(request.headers)
            .json(&request.body);
        if let Some(timeout) = request.timeout {
            builder = builder.timeout(timeout);
        }
        Ok(StreamResponse {
            response: builder.send().await?,
        })
    }
}
