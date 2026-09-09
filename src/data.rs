use std::fmt;
use std::ops::Index;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A monitor document as returned by the Monitor API.
///
/// The full parsed JSON is available through [`MonitorData::as_value`], with
/// typed accessors for common fields and [`MonitorData::get`] for nested
/// values addressed by a dotted path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MonitorData(Value);

impl MonitorData {
    /// Wrap a parsed monitor document.
    pub fn new(value: Value) -> Self {
        Self(value)
    }

    /// The monitor key.
    pub fn key(&self) -> Option<&str> {
        self.get_str("key")
    }

    /// The monitor name.
    pub fn name(&self) -> Option<&str> {
        self.get_str("name")
    }

    /// The monitor type (`job`, `check` or `heartbeat`).
    pub fn monitor_type(&self) -> Option<&str> {
        self.get_str("type")
    }

    /// The monitor schedule.
    pub fn schedule(&self) -> Option<&str> {
        self.get_str("schedule")
    }

    /// Look up a nested value by dotted path, e.g. `"attributes.code"` or
    /// `"latest_events.0.event"` (numeric segments index into arrays).
    pub fn get(&self, path: &str) -> Option<&Value> {
        path.split('.')
            .try_fold(&self.0, |current, segment| match current {
                Value::Object(map) => map.get(segment),
                Value::Array(items) => segment.parse::<usize>().ok().and_then(|i| items.get(i)),
                _ => None,
            })
    }

    /// [`MonitorData::get`] for string values.
    pub fn get_str(&self, path: &str) -> Option<&str> {
        self.get(path).and_then(Value::as_str)
    }

    /// The raw parsed document.
    pub fn as_value(&self) -> &Value {
        &self.0
    }

    /// Unwrap the raw parsed document.
    pub fn into_value(self) -> Value {
        self.0
    }
}

impl From<Value> for MonitorData {
    fn from(value: Value) -> Self {
        Self(value)
    }
}

impl From<MonitorData> for Value {
    fn from(data: MonitorData) -> Self {
        data.0
    }
}

impl Index<&str> for MonitorData {
    type Output = Value;

    /// Index a top-level field; missing fields yield `Value::Null`.
    fn index(&self, field: &str) -> &Value {
        &self.0[field]
    }
}

impl fmt::Display for MonitorData {
    /// Pretty-printed JSON.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match serde_json::to_string_pretty(&self.0) {
            Ok(text) => f.write_str(&text),
            Err(_) => write!(f, "{}", self.0),
        }
    }
}
