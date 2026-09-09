//! The job wrapper: run/complete/fail telemetry around a closure.

use std::fmt::{Debug, Display};
use std::panic::{self, AssertUnwindSafe};

use serde_json::Value;

use crate::monitor::unix_time;
use crate::{Client, PingOptions, PutOptions, Result, State};

/// Options for [`Client::job_with`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobOptions {
    key: String,
    env: Option<String>,
    log_output: bool,
    attributes: Option<Value>,
}

impl JobOptions {
    /// Options for the monitor `key` with output logging enabled.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            env: None,
            log_output: true,
            attributes: None,
        }
    }

    /// Send pings for this job with the given environment.
    pub fn env(mut self, env: impl Into<String>) -> Self {
        self.env = Some(env.into());
        self
    }

    /// Whether the closure's return value is sent as the `complete` message
    /// (default `true`).
    pub fn log_output(mut self, log_output: bool) -> Self {
        self.log_output = log_output;
        self
    }

    /// Monitor attributes (e.g. `schedule`, `notify`) to queue for
    /// [`Client::sync_monitors`]. The monitor key is added automatically.
    pub fn attributes(mut self, attributes: Value) -> Self {
        self.attributes = Some(attributes);
        self
    }
}

impl Client {
    /// Run `f` as a monitored job.
    ///
    /// A `run` ping is sent before the closure is called and a `complete`
    /// ping (with the `Debug` form of the return value as the message) or a
    /// `fail` ping (with the error's `Display` form) after it returns, each
    /// with a `duration` metric. The closure's result is returned unchanged.
    /// A panic inside the closure sends a `fail` ping and then resumes.
    ///
    /// Ping failures are logged and never affect the returned result.
    pub fn job<T, E, F>(&self, key: impl Into<String>, f: F) -> std::result::Result<T, E>
    where
        F: FnOnce() -> std::result::Result<T, E>,
        T: Debug,
        E: Display,
    {
        self.job_with(JobOptions::new(key), f)
    }

    /// [`Client::job`] with explicit [`JobOptions`].
    pub fn job_with<T, E, F>(&self, options: JobOptions, f: F) -> std::result::Result<T, E>
    where
        F: FnOnce() -> std::result::Result<T, E>,
        T: Debug,
        E: Display,
    {
        if let Some(attributes) = options.attributes {
            self.queue_monitor_attributes(&options.key, attributes);
        }
        let mut monitor = self.monitor(options.key);
        if let Some(env) = options.env {
            monitor = monitor.with_env(env);
        }

        let start = unix_time();
        log_ping_error(monitor.ping(PingOptions::new().state(State::Run).series(start)));

        let outcome = panic::catch_unwind(AssertUnwindSafe(f));
        let duration = unix_time() - start;
        let finish = |state: State, message: Option<String>| {
            let mut opts = PingOptions::new()
                .state(state)
                .series(start)
                .metric("duration", duration);
            if let Some(message) = message {
                opts = opts.message(message);
            }
            log_ping_error(monitor.ping(opts));
        };

        match outcome {
            Ok(Ok(out)) => {
                let message = options.log_output.then(|| format!("{out:?}"));
                finish(State::Complete, message);
                Ok(out)
            }
            Ok(Err(err)) => {
                finish(State::Fail, Some(err.to_string()));
                Err(err)
            }
            Err(payload) => {
                let message = if let Some(s) = payload.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "job panicked".to_string()
                };
                finish(State::Fail, Some(message));
                panic::resume_unwind(payload)
            }
        }
    }

    /// Queue monitor `attributes` for `key` to be sent by
    /// [`Client::sync_monitors`]. Attributes queued earlier for the same key
    /// are replaced.
    pub fn queue_monitor_attributes(&self, key: &str, attributes: Value) {
        let mut attributes = match attributes {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        attributes.insert("key".into(), Value::String(key.to_string()));
        let mut pending = self
            .pending_attributes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        pending.retain(|m| m.get("key").and_then(Value::as_str) != Some(key));
        pending.push(Value::Object(attributes));
    }

    /// Monitor attributes queued by [`Client::job_with`] and
    /// [`Client::queue_monitor_attributes`] that have not been synced yet.
    pub fn pending_attributes(&self) -> Vec<Value> {
        self.pending_attributes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Send queued monitor attributes to Cronitor with [`Client::put`] and
    /// clear the queue. Does nothing when the queue is empty.
    ///
    /// Call this once at startup (after your jobs are registered) or from
    /// a deploy step; requires an API key with `monitor:write` privileges.
    pub fn sync_monitors(&self) -> Result<()> {
        let pending = std::mem::take(
            &mut *self
                .pending_attributes
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        if pending.is_empty() {
            return Ok(());
        }
        match self.put(&pending, PutOptions::new()) {
            Ok(_) => Ok(()),
            Err(err) => {
                // Put the attributes back so a later sync can retry.
                let mut queue = self
                    .pending_attributes
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let mut restored = pending;
                restored.append(&mut queue);
                *queue = restored;
                Err(err)
            }
        }
    }
}

fn log_ping_error(result: Result<()>) {
    if let Err(err) = result {
        log::error!("cronitor ping failed: {err}");
    }
}
