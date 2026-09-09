use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{Client, Error, Result};

/// Timeout for telemetry pings.
const PING_TIMEOUT: Duration = Duration::from_secs(5);

/// Telemetry event state.
///
/// `Run`, `Complete` and `Fail` measure the lifecycle of a job; `Ok` manually
/// resets a monitor to a passing state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    /// The job has started.
    Run,
    /// The job finished successfully.
    Complete,
    /// The job failed.
    Fail,
    /// Manually reset the monitor to a passing state.
    Ok,
}

impl State {
    /// The value sent as the `state` ping parameter.
    pub fn as_str(self) -> &'static str {
        match self {
            State::Run => "run",
            State::Complete => "complete",
            State::Fail => "fail",
            State::Ok => "ok",
        }
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Optional parameters for [`Monitor::ping`].
///
/// See <https://cronitor.io/docs/telemetry-api#parameters> for the full list.
/// Unset parameters are omitted from the request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PingOptions {
    state: Option<State>,
    message: Option<String>,
    series: Option<String>,
    host: Option<String>,
    metrics: Vec<(String, f64)>,
}

impl PingOptions {
    /// Options with no parameters set; sends a plain heartbeat.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the event state.
    pub fn state(mut self, state: State) -> Self {
        self.state = Some(state);
        self
    }

    /// Set a message shown in alerts and on the monitor activity panel.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Set a series identifier used to match `run` events with their
    /// `complete`/`fail` events.
    pub fn series(mut self, series: impl fmt::Display) -> Self {
        self.series = Some(series.to_string());
        self
    }

    /// Override the host name (defaults to the machine's hostname).
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Add a metric such as `duration`, `count` or `error_count`. Each metric
    /// is sent as a `metric=name:value` query parameter.
    pub fn metric(mut self, name: impl Into<String>, value: impl Into<f64>) -> Self {
        self.metrics.push((name.into(), value.into()));
        self
    }

    /// The `(name, value)` query parameters for these options.
    fn params(&self, env: Option<&str>) -> Vec<(&'static str, String)> {
        let mut params = Vec::new();
        if let Some(state) = self.state {
            params.push(("state", state.as_str().to_string()));
        }
        if let Some(message) = &self.message {
            params.push(("message", message.clone()));
        }
        if let Some(series) = &self.series {
            params.push(("series", series.clone()));
        }
        let host = self.host.clone().or_else(machine_hostname);
        if let Some(host) = host {
            params.push(("host", host));
        }
        for (name, value) in &self.metrics {
            params.push(("metric", format!("{name}:{value}")));
        }
        params.push(("stamp", unix_time().to_string()));
        if let Some(env) = env {
            params.push(("env", env.to_string()));
        }
        params
    }
}

/// A handle to a single Cronitor monitor.
///
/// Create one with [`Client::monitor`]. The client's API key, API version and
/// environment can be overridden per instance.
#[derive(Debug, Clone)]
pub struct Monitor {
    client: Client,
    key: String,
    api_key: Option<String>,
    api_version: Option<String>,
    env: Option<String>,
}

impl Monitor {
    pub(crate) fn new(client: Client, key: impl Into<String>) -> Self {
        Self {
            client,
            key: key.into(),
            api_key: None,
            api_version: None,
            env: None,
        }
    }

    /// The monitor key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Override the API key for this monitor only.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Override the API version for this monitor only.
    pub fn with_api_version(mut self, api_version: impl Into<String>) -> Self {
        self.api_version = Some(api_version.into());
        self
    }

    /// Override the environment for this monitor only.
    pub fn with_env(mut self, env: impl Into<String>) -> Self {
        self.env = Some(env.into());
        self
    }

    /// The API key in effect for this monitor.
    pub fn api_key(&self) -> Option<&str> {
        self.api_key
            .as_deref()
            .or_else(|| self.client.config.api_key())
    }

    /// The API version in effect for this monitor.
    pub fn api_version(&self) -> Option<&str> {
        self.api_version
            .as_deref()
            .or_else(|| self.client.config.api_version())
    }

    /// The environment in effect for this monitor.
    pub fn env(&self) -> Option<&str> {
        self.env
            .as_deref()
            .or_else(|| self.client.config.environment())
    }

    /// Send a telemetry event.
    ///
    /// If no API key is configured an error is logged and `Ok(())` is
    /// returned without sending anything.
    pub fn ping(&self, options: PingOptions) -> Result<()> {
        let Some(api_key) = self.api_key() else {
            log::error!(
                "No API key detected. Set CRONITOR_API_KEY, Config::with_api_key or Monitor::with_api_key."
            );
            return Ok(());
        };
        let url = format!("{}/{}/{}", self.client.ping_base_url, api_key, self.key);
        let mut req = self.client.agent.get(&url).timeout(PING_TIMEOUT);
        for (name, value) in options.params(self.env()) {
            req = req.query(name, &value);
        }
        let resp = self.client.send(req, None)?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(Error::Api(format!(
                "Unexpected response {} pinging '{}': {}",
                resp.status, self.key, resp.body
            )))
        }
    }

    /// Manually reset the monitor to a passing state; equivalent to
    /// `ping(PingOptions::new().state(State::Ok))`.
    pub fn ok(&self) -> Result<()> {
        self.ping(PingOptions::new().state(State::Ok))
    }
}

/// Seconds since the Unix epoch as a float.
pub(crate) fn unix_time() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn machine_hostname() -> Option<String> {
    hostname::get()
        .ok()
        .map(|h| h.to_string_lossy().into_owned())
        .filter(|h| !h.is_empty())
}
