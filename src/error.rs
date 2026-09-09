/// Errors returned by the Cronitor SDK.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The requested monitor does not exist (HTTP 404 from the Monitor API).
    #[error("monitor not found: {0}")]
    MonitorNotFound(String),

    /// A local configuration problem, e.g. no config path set or an invalid file.
    #[error("config validation error: {0}")]
    ConfigValidation(String),

    /// The Monitor API rejected the submitted monitors (HTTP 400).
    #[error("API validation error: {0}")]
    ApiValidation(String),

    /// No API key is configured for an operation that requires one.
    #[error("authentication error: {0}")]
    Authentication(String),

    /// The Monitor API returned an unexpected response.
    #[error("API error: {0}")]
    Api(String),

    /// A transport-level HTTP failure (connection refused, timeout, TLS, ...).
    #[error("HTTP error: {0}")]
    Http(#[from] Box<ureq::Error>),

    /// A filesystem error while reading or writing a config file.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// A JSON (de)serialisation error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// A YAML (de)serialisation error.
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
}

impl From<ureq::Error> for Error {
    fn from(err: ureq::Error) -> Self {
        Error::Http(Box::new(err))
    }
}

/// Convenience alias for results produced by this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;
