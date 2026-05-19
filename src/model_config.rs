use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const DEFAULT_PROVIDER: &str = "cohere";
const DEFAULT_MODEL: &str = "command-r7b-12-2024";
const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct AiConfig {
    pub provider: String,
    pub model: String,
}

#[derive(Clone)]
struct Cached {
    config: AiConfig,
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

    pub async fn get_config(&self) -> AiConfig {
        let mut guard = self.inner.lock().await;

        if let Some(ref cached) = *guard {
            if cached.fetched_at.elapsed() < CACHE_TTL {
                return cached.config.clone();
            }
        }

        let config = self.fetch_from_store().await;
        *guard = Some(Cached {
            config: config.clone(),
            fetched_at: Instant::now(),
        });
        config
    }

    async fn fetch_from_store(&self) -> AiConfig {
        let store_url = match &self.store_url {
            Some(u) => u.clone(),
            None => return self.default_config(),
        };

        let url = format!("{}/api/system/ai-config", store_url);
        match reqwest::get(&url).await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<std::collections::HashMap<String, String>>().await {
                    Ok(map) => AiConfig {
                        provider: map
                            .get("provider")
                            .cloned()
                            .unwrap_or_else(|| DEFAULT_PROVIDER.to_string()),
                        model: map
                            .get("model")
                            .cloned()
                            .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
                    },
                    Err(_) => self.default_config(),
                }
            }
            _ => self.default_config(),
        }
    }

    fn default_config(&self) -> AiConfig {
        AiConfig {
            provider: std::env::var("AI_PROVIDER")
                .unwrap_or_else(|_| DEFAULT_PROVIDER.to_string()),
            model: std::env::var("COHERE_MODEL")
                .unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
        }
    }
}
