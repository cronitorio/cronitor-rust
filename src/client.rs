use std::time::Duration;

use crate::http::{basic_auth, DEFAULT_RETRY_BACKOFF};
use crate::{Config, Monitor};

/// Default base URL for telemetry pings; the API key and monitor key are appended.
pub const DEFAULT_PING_BASE_URL: &str = "https://cronitor.link/p";

/// Default base URL for the Monitor API.
pub const DEFAULT_MONITOR_API_URL: &str = "https://cronitor.io/api/monitors";

/// Entry point to the SDK: holds the [`Config`] and the API base URLs.
///
/// A `Client` is cheap to clone; clones share the same HTTP connection pool.
#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) config: Config,
    pub(crate) ping_base_url: String,
    pub(crate) monitor_api_url: String,
    pub(crate) retry_backoff: Duration,
    pub(crate) agent: ureq::Agent,
}

impl Default for Client {
    /// Equivalent to [`Client::from_env`].
    fn default() -> Self {
        Self::from_env()
    }
}

impl Client {
    /// Create a client from an explicit [`Config`].
    pub fn new(config: Config) -> Self {
        Self {
            config,
            ping_base_url: DEFAULT_PING_BASE_URL.to_string(),
            monitor_api_url: DEFAULT_MONITOR_API_URL.to_string(),
            retry_backoff: DEFAULT_RETRY_BACKOFF,
            agent: ureq::AgentBuilder::new().build(),
        }
    }

    /// Create a client configured from the `CRONITOR_*` environment variables.
    pub fn from_env() -> Self {
        Self::new(Config::from_env())
    }

    /// Override the base URL for telemetry pings (default `https://cronitor.link/p`).
    pub fn with_ping_base_url(mut self, url: impl Into<String>) -> Self {
        self.ping_base_url = trim_slash(url.into());
        self
    }

    /// Override the Monitor API base URL (default `https://cronitor.io/api/monitors`).
    pub fn with_monitor_api_url(mut self, url: impl Into<String>) -> Self {
        self.monitor_api_url = trim_slash(url.into());
        self
    }

    /// Override the base delay used for exponential backoff between retries
    /// (default 300ms; the delay doubles on each retry).
    pub fn with_retry_backoff(mut self, backoff: Duration) -> Self {
        self.retry_backoff = backoff;
        self
    }

    /// The client's configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The base URL used for telemetry pings.
    pub fn ping_base_url(&self) -> &str {
        &self.ping_base_url
    }

    /// The base URL used for Monitor API calls.
    pub fn monitor_api_url(&self) -> &str {
        &self.monitor_api_url
    }

    /// A [`Monitor`] handle for `key`, sharing this client's configuration.
    pub fn monitor(&self, key: impl Into<String>) -> Monitor {
        Monitor::new(self.clone(), key)
    }

    /// Build an authenticated Monitor API request with the standard headers.
    pub(crate) fn api_request(
        &self,
        method: &str,
        url: &str,
        api_key: &str,
        api_version: Option<&str>,
        timeout: Duration,
    ) -> ureq::Request {
        let mut req = self
            .agent
            .request(method, url)
            .timeout(timeout)
            .set("Authorization", &basic_auth(api_key));
        if let Some(version) = api_version {
            req = req.set("Cronitor-Version", version);
        }
        req
    }

    /// URL of a single monitor on the Monitor API.
    pub(crate) fn monitor_url(&self, key: &str) -> String {
        format!("{}/{}", self.monitor_api_url, key)
    }
}

fn trim_slash(mut url: String) -> String {
    while url.ends_with('/') {
        url.pop();
    }
    url
}
