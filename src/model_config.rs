// src/model_config.rs
//
// Fetches and caches the AI model config from the store's public endpoint.
// Falls back to env var COHERE_MODEL or the hardcoded default when the store
// is unreachable.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const DEFAULT_MODEL: &str = "command-r7b-12-2024";
const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct Cached {
    model: String,
    fetched_at: Instant,
}

#[derive(Clone)]
pub struct ModelConfigCache {
    store_url: Option<String>,
    inner: Arc<Mutex<Option<Cached>>>,
}

impl ModelConfigCache {
    pub fn new(store_url: Option<String>) -> Self {
        Self {
            store_url,
            inner: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn get_model(&self) -> String {
        let mut guard = self.inner.lock().await;

        if let Some(ref cached) = *guard {
            if cached.fetched_at.elapsed() < CACHE_TTL {
                return cached.model.clone();
            }
        }

        let model = self.fetch_from_store().await;
        *guard = Some(Cached {
            model: model.clone(),
            fetched_at: Instant::now(),
        });
        model
    }

    async fn fetch_from_store(&self) -> String {
        let store_url = match &self.store_url {
            Some(u) => u.clone(),
            None => return self.default_model(),
        };

        let url = format!("{}/api/system/ai-config", store_url);
        match reqwest::get(&url).await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<std::collections::HashMap<String, String>>().await {
                    Ok(map) => map
                        .get("model")
                        .cloned()
                        .unwrap_or_else(|| self.default_model()),
                    Err(_) => self.default_model(),
                }
            }
            _ => self.default_model(),
        }
    }

    fn default_model(&self) -> String {
        std::env::var("COHERE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string())
    }
}
