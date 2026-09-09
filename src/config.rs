use std::path::{Path, PathBuf};
use std::time::Duration;

/// Default timeout for Monitor API calls, in seconds.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// SDK settings.
///
/// Every setting has an environment-variable default read by [`Config::from_env`]:
///
/// | setting       | environment variable   |
/// |---------------|------------------------|
/// | `api_key`     | `CRONITOR_API_KEY`     |
/// | `api_version` | `CRONITOR_API_VERSION` |
/// | `environment` | `CRONITOR_ENVIRONMENT` |
/// | `timeout`     | `CRONITOR_TIMEOUT` (seconds, default 10) |
/// | `config_path` | `CRONITOR_CONFIG`      |
///
/// Settings can also be set in code with the builder-style methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    api_key: Option<String>,
    api_version: Option<String>,
    environment: Option<String>,
    timeout: Duration,
    config_path: Option<PathBuf>,
}

impl Default for Config {
    /// Equivalent to [`Config::from_env`].
    fn default() -> Self {
        Self::from_env()
    }
}

impl Config {
    /// An empty configuration that ignores environment variables.
    pub fn new() -> Self {
        Self {
            api_key: None,
            api_version: None,
            environment: None,
            timeout: DEFAULT_TIMEOUT,
            config_path: None,
        }
    }

    /// Build a configuration from the `CRONITOR_*` environment variables.
    ///
    /// Empty variables are treated as unset and an unparsable
    /// `CRONITOR_TIMEOUT` falls back to the 10 second default.
    pub fn from_env() -> Self {
        let timeout = env_var("CRONITOR_TIMEOUT")
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_TIMEOUT);
        Self {
            api_key: env_var("CRONITOR_API_KEY"),
            api_version: env_var("CRONITOR_API_VERSION"),
            environment: env_var("CRONITOR_ENVIRONMENT"),
            timeout,
            config_path: env_var("CRONITOR_CONFIG").map(PathBuf::from),
        }
    }

    /// Set the API key used to authenticate pings and Monitor API calls.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set the API version sent as the `Cronitor-Version` header.
    pub fn with_api_version(mut self, api_version: impl Into<String>) -> Self {
        self.api_version = Some(api_version.into());
        self
    }

    /// Set the environment sent as the `env` ping parameter.
    pub fn with_environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }

    /// Set the timeout for Monitor API calls.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the path to the `cronitor.yaml` config file.
    pub fn with_config_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config_path = Some(path.into());
        self
    }

    /// The configured API key, if any.
    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// The configured API version, if any.
    pub fn api_version(&self) -> Option<&str> {
        self.api_version.as_deref()
    }

    /// The configured environment, if any.
    pub fn environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }

    /// The timeout for Monitor API calls.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The configured path to `cronitor.yaml`, if any.
    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}
