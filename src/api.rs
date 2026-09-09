//! Account-level Monitor API operations: list, put and YAML export.

use serde::Serialize;
use serde_json::Value;

use crate::{Client, Error, Monitor, MonitorData, Result};

/// Options for [`Client::list`].
///
/// Either fetch specific monitors by [`ListOptions::keys`] (each is fetched
/// individually) or filter the account's monitors with the remaining options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListOptions {
    keys: Vec<String>,
    filters: Vec<(&'static str, String)>,
    page: u32,
    page_size: u32,
    auto_paginate: bool,
}

impl ListOptions {
    /// Default options: page 1 with 100 results per page and no filters.
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            filters: Vec::new(),
            page: 1,
            page_size: 100,
            auto_paginate: false,
        }
    }

    /// Fetch these monitors individually instead of listing with filters.
    pub fn keys<I, S>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.keys = keys.into_iter().map(Into::into).collect();
        self
    }

    /// Filter by monitor type (`job`, `check` or `heartbeat`).
    pub fn monitor_type(self, monitor_type: impl Into<String>) -> Self {
        self.filter("type", monitor_type)
    }

    /// Filter by group.
    pub fn group(self, group: impl Into<String>) -> Self {
        self.filter("group", group)
    }

    /// Filter by tag.
    pub fn tag(self, tag: impl Into<String>) -> Self {
        self.filter("tag", tag)
    }

    /// Filter by state (e.g. `passing`, `failing`, `paused`).
    pub fn state(self, state: impl Into<String>) -> Self {
        self.filter("state", state)
    }

    /// Filter by environment.
    pub fn env(self, env: impl Into<String>) -> Self {
        self.filter("env", env)
    }

    /// Full-text search.
    pub fn search(self, search: impl Into<String>) -> Self {
        self.filter("search", search)
    }

    /// Sort order.
    pub fn sort(self, sort: impl Into<String>) -> Self {
        self.filter("sort", sort)
    }

    /// Page number (default 1).
    pub fn page(mut self, page: u32) -> Self {
        self.page = page;
        self
    }

    /// Results per page (default 100).
    pub fn page_size(mut self, page_size: u32) -> Self {
        self.page_size = page_size;
        self
    }

    /// Keep fetching pages until one returns fewer than `page_size` results.
    pub fn auto_paginate(mut self, auto_paginate: bool) -> Self {
        self.auto_paginate = auto_paginate;
        self
    }

    fn filter(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.filters.retain(|(k, _)| *k != name);
        self.filters.push((name, value.into()));
        self
    }
}

/// Options for [`Client::put`] and [`Client::put_yaml`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PutOptions {
    rollback: bool,
    api_key: Option<String>,
    api_version: Option<String>,
}

impl PutOptions {
    /// Default options: no rollback, the client's API key and version.
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate the monitors without persisting them.
    pub fn rollback(mut self, rollback: bool) -> Self {
        self.rollback = rollback;
        self
    }

    /// Override the API key for this call.
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Override the API version for this call.
    pub fn api_version(mut self, api_version: impl Into<String>) -> Self {
        self.api_version = Some(api_version.into());
        self
    }
}

impl Client {
    /// List monitors.
    ///
    /// With [`ListOptions::keys`] each monitor is fetched individually and a
    /// missing key yields [`Error::MonitorNotFound`]. Otherwise the account's
    /// monitors are listed with the given filters and pagination.
    pub fn list(&self, options: ListOptions) -> Result<Vec<Monitor>> {
        self.require_api_key()?;
        if !options.keys.is_empty() {
            return options
                .keys
                .iter()
                .map(|key| {
                    let data = self.monitor(key.clone()).fetch()?;
                    Ok(Monitor::with_data(self.clone(), data))
                })
                .collect();
        }

        let mut monitors = Vec::new();
        let mut page = options.page;
        loop {
            let batch = self.fetch_page(&options, page)?;
            let count = batch.len();
            monitors.extend(batch);
            if !options.auto_paginate || count < options.page_size as usize {
                return Ok(monitors);
            }
            page += 1;
        }
    }

    fn fetch_page(&self, options: &ListOptions, page: u32) -> Result<Vec<Monitor>> {
        let api_key = self.require_api_key()?;
        let mut req = self.api_request(
            "GET",
            &self.monitor_api_url,
            api_key,
            self.config.api_version(),
            self.config.timeout(),
        );
        for (name, value) in &options.filters {
            req = req.query(name, value);
        }
        req = req
            .query("page", &page.to_string())
            .query("pageSize", &options.page_size.to_string());
        let resp = self.send(req, None)?;
        if resp.status != 200 {
            return Err(Error::Api(format!(
                "Unexpected error {} listing monitors: {}",
                resp.status, resp.body
            )));
        }
        let body: Value = serde_json::from_str(&resp.body)?;
        Ok(self.monitors_from(body))
    }

    /// Create or update monitors.
    ///
    /// `monitors` may be any serialisable values, e.g. `serde_json::Value`s
    /// built with `json!` or your own structs. See the
    /// [Monitor API docs](https://cronitor.io/docs/monitor-api#attributes)
    /// for the available attributes. A 400 response yields
    /// [`Error::ApiValidation`].
    pub fn put<T: Serialize>(&self, monitors: &[T], options: PutOptions) -> Result<Vec<Monitor>> {
        let mut payload = serde_json::Map::new();
        payload.insert("monitors".into(), serde_json::to_value(monitors)?);
        if options.rollback {
            payload.insert("rollback".into(), Value::Bool(true));
        }
        let body = serde_json::to_string(&payload)?;
        let resp = self.put_raw(&self.monitor_api_url, "application/json", &body, &options)?;
        let data: Value = serde_json::from_str(&resp)?;
        Ok(self.monitors_from(data))
    }

    /// Create or update monitors from a YAML document with the top-level keys
    /// `jobs`, `checks` and/or `heartbeats`, returning the parsed response.
    ///
    /// With [`PutOptions::rollback`] the document is validated only.
    pub fn put_yaml(
        &self,
        document: &serde_yaml::Value,
        options: PutOptions,
    ) -> Result<serde_yaml::Value> {
        let mut payload = document.clone();
        if options.rollback {
            match &mut payload {
                serde_yaml::Value::Mapping(map) => {
                    map.insert("rollback".into(), serde_yaml::Value::Bool(true));
                }
                _ => {
                    return Err(Error::ConfigValidation(
                        "YAML config must be a mapping with jobs/checks/heartbeats keys".into(),
                    ))
                }
            }
        }
        let body = serde_yaml::to_string(&payload)?;
        let url = format!("{}.yaml", self.monitor_api_url);
        let resp = self.put_raw(&url, "application/yaml", &body, &options)?;
        Ok(serde_yaml::from_str(&resp)?)
    }

    /// Export the account's monitors as a YAML document.
    pub fn as_yaml(&self) -> Result<String> {
        let api_key = self.require_api_key()?;
        let url = format!("{}.yaml", self.monitor_api_url);
        let req = self
            .api_request(
                "GET",
                &url,
                api_key,
                self.config.api_version(),
                self.config.timeout(),
            )
            .set("Content-Type", "application/yaml");
        let resp = self.send(req, None)?;
        if resp.status == 200 {
            Ok(resp.body)
        } else {
            Err(Error::Api(format!(
                "Unexpected error {}: {}",
                resp.status, resp.body
            )))
        }
    }

    fn put_raw(
        &self,
        url: &str,
        content_type: &str,
        body: &str,
        options: &PutOptions,
    ) -> Result<String> {
        let api_key = options
            .api_key
            .as_deref()
            .or_else(|| self.config.api_key())
            .ok_or_else(missing_api_key)?;
        let api_version = options
            .api_version
            .as_deref()
            .or_else(|| self.config.api_version());
        let req = self
            .api_request("PUT", url, api_key, api_version, self.config.timeout())
            .set("Content-Type", content_type);
        let resp = self.send(req, Some(body))?;
        match resp.status {
            200 => Ok(resp.body),
            400 => Err(Error::ApiValidation(resp.body)),
            status => Err(Error::Api(format!(
                "Unexpected error {}: {}",
                status, resp.body
            ))),
        }
    }

    /// Monitor handles for the `monitors` array of a Monitor API response.
    fn monitors_from(&self, body: Value) -> Vec<Monitor> {
        match body {
            Value::Object(mut map) => match map.remove("monitors") {
                Some(Value::Array(items)) => items
                    .into_iter()
                    .map(|item| Monitor::with_data(self.clone(), MonitorData::new(item)))
                    .collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    pub(crate) fn require_api_key(&self) -> Result<&str> {
        self.config.api_key().ok_or_else(missing_api_key)
    }
}

pub(crate) fn missing_api_key() -> Error {
    Error::Authentication(
        "No api_key detected. Set CRONITOR_API_KEY or Config::with_api_key.".to_string(),
    )
}
