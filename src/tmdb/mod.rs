pub mod images;
pub mod model;
pub mod search;

use std::num::NonZeroU32;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use reqwest::Client;
use serde::de::DeserializeOwned;

#[allow(dead_code)]
pub const IMG_BASE_W500: &str = "https://image.tmdb.org/t/p/w500";
pub const IMG_BASE_ORIGINAL: &str = "https://image.tmdb.org/t/p/original";
pub const API_BASE: &str = "https://api.themoviedb.org/3";

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

pub struct TmdbClient {
    http: Client,
    api_key: String,
    limiter: Limiter,
}

impl TmdbClient {
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        let http = Client::builder()
            .user_agent(concat!("archivist/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .context("build reqwest client")?;
        let quota = Quota::per_second(NonZeroU32::new(40).unwrap());
        let limiter = RateLimiter::direct(quota);
        Ok(Self {
            http,
            api_key: api_key.into(),
            limiter,
        })
    }

    #[allow(dead_code)]
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub async fn ping(&self) -> Result<()> {
        let url = format!("{API_BASE}/configuration");
        let resp = self
            .http
            .get(&url)
            .query(&[("api_key", &self.api_key)])
            .send()
            .await
            .context("TMDB ping request failed")?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else if status == reqwest::StatusCode::UNAUTHORIZED {
            Err(anyhow!("TMDB rejected the API key (401 Unauthorized)"))
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(anyhow!("TMDB returned {}: {}", status, body))
        }
    }

    pub(crate) async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        extra_params: &[(&str, &str)],
    ) -> Result<T> {
        self.limiter.until_ready().await;
        let url = format!("{API_BASE}{path}");
        let mut req = self.http.get(&url).query(&[("api_key", &self.api_key)]);
        if !extra_params.is_empty() {
            req = req.query(extra_params);
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("GET {url} returned an error"))?;
        let json = resp
            .json::<T>()
            .await
            .with_context(|| format!("decode JSON from GET {url}"))?;
        Ok(json)
    }

    /// Download a poster JPEG. Caller chooses the size base (w500/original).
    pub async fn fetch_poster_bytes(&self, base: &str, file_path: &str) -> Result<bytes::Bytes> {
        self.limiter.until_ready().await;
        let url = format!("{base}{file_path}");
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("GET {url} returned an error"))?;
        Ok(resp.bytes().await?)
    }
}
