# Cronitor Rust Library
![Test](https://github.com/cronitorio/cronitor-rust/workflows/Test/badge.svg)

[Cronitor](https://cronitor.io/) provides end-to-end monitoring for background jobs, websites, APIs, and anything else that can send or receive an HTTP request. This library provides convenient access to the Cronitor API from applications written in Rust. See our [API docs](https://cronitor.io/docs/api) for detailed references on configuring monitors and sending telemetry pings.

In this guide:

- [Installation](#installation)
- [Monitoring Background Jobs](#monitoring-background-jobs)
- [Sending Telemetry Events](#sending-telemetry-events)
- [Configuring Monitors](#configuring-monitors)
- [Package Configuration & Env Vars](#package-configuration)
- [Roadmap](#roadmap)
- [Contributing](#contributing)

## Installation

```
cargo add cronitor-sdk
```

or add it to your `Cargo.toml`:

```toml
[dependencies]
cronitor-sdk = "1"
```

The package is published as `cronitor-sdk` but the library is named `cronitor`, so import it as `use cronitor::...`.

The crate requires Rust 1.75 or later. The API is blocking; from an async runtime call it through `tokio::task::spawn_blocking` (or your runtime's equivalent):

```rust
let client = cronitor::Client::from_env();
let result = tokio::task::spawn_blocking(move || {
    client.job("send-invoices", || send_invoices())
}).await?;
```

## Monitoring Background Jobs

#### Manual Integration

`Client::job` is a lightweight way to monitor any background task regardless of how it is executed. It sends telemetry events before calling your closure and after it returns. If your closure returns an `Err` (or panics) a `fail` event is sent, and the result (or panic) is passed back to you unchanged.

```rust
use cronitor::{Client, Config};

// your api keys can found here - https://cronitor.io/settings/api
let client = Client::new(Config::from_env().with_api_key("apiKey123"));

// Wrap any closure to monitor it.
// If no monitor matches the provided key, one will be created automatically.
let sent: Result<usize, std::io::Error> = client.job("send-invoices", || {
    // ... send the invoices ...
    Ok(42)
});
```

The closure's `Ok` value is sent as the `complete` message (via its `Debug` form) and the `Err` value's `Display` form as the `fail` message. Use `JobOptions::log_output(false)` to keep the return value out of the message.

#### You can provide monitor attributes that will be synced when your app starts

To sync attributes, provide an API key with monitor:write privileges. Attributes are queued when the job runs and sent with a single `put` request when you call `sync_monitors`; the library never spawns a background thread for this, so call it once at startup or from a deploy step.

```rust
use cronitor::{Client, Config, JobOptions};
use serde_json::json;

// Copy your SDK Integration key from https://cronitor.io/app/settings/api
let client = Client::new(Config::new().with_api_key("apiKey123"));

let options = JobOptions::new("send-invoices")
    .attributes(json!({"schedule": "0 8 * * *", "notify": ["devops-alerts"]}));

let result: Result<(), String> = client.job_with(options, || {
    // ...
    Ok(())
});

// Send the queued attributes to Cronitor.
client.sync_monitors()?;
```

## Sending Telemetry Events

If you want to send heartbeat events, or want finer control over when/how [telemetry events](https://cronitor.io/docs/telemetry-api) are sent for your jobs, you can create a monitor instance and call the `ping` method.

```rust
use cronitor::{Client, Config, PingOptions, State};

// your api keys can found here - https://cronitor.io/settings/api
let config = Config::new()
    .with_api_key("apiKey123")
    // optionally, set an environment
    .with_environment("staging");
let client = Client::new(config);

let monitor = client.monitor("heartbeat-monitor");
monitor.ping(PingOptions::new())?; // send a heartbeat event

// optional params are set with builder methods.
// for a complete list see https://cronitor.io/docs/telemetry-api#parameters
monitor.ping(
    PingOptions::new()
        .state(State::Run) // Run|Complete|Fail used to measure lifecycle of a job, Ok used for manual reset only.
        .message("") // message that will be displayed in alerts as well as monitor activity panel on your dashboard.
        .metric("duration", 100) // how long the job ran (complete|fail only). cronitor will calculate this when not provided
        .metric("count", 4500) // if your job is processing a number of items you can report a count
        .metric("error_count", 10), // the number of errors that occurred while this job was running
)?;
```

If no API key is configured, `ping` logs an error (through the [`log`](https://docs.rs/log) facade) and returns `Ok(())` without sending anything, so a missing key never breaks your job.

## Configuring Monitors

### YAML Configuration File

You can configure all of your monitors using a single YAML file. This can be version controlled and synced to Cronitor as part of
a deployment or build process. For details on all of the attributes that can be set, see the [Monitor API](https://cronitor.io/docs/monitor-api) documentation.

```rust
use cronitor::{Client, Config};

// your api keys can found here - https://cronitor.io/settings/api
let client = Client::new(Config::new().with_api_key("apiKey123"));

client.read_config("./cronitor.yaml")?; // parse the yaml file of monitors

client.validate_config()?; // send monitors to Cronitor for configuration validation

client.apply_config()?; // sync the monitors from the config file to Cronitor

client.generate_config()?; // generate a new config file from the Cronitor API
```

The timeout value for validate_config, apply_config and generate_config is 10 seconds by default. The value can be rewritten by setting the environment variable `CRONITOR_TIMEOUT`. It can also be rewritten with `Config::with_timeout`.

```rust
use std::time::Duration;
use cronitor::{Client, Config};

let client = Client::new(Config::from_env().with_timeout(Duration::from_secs(30)));
client.apply_config()?;
```

The `cronitor.yaml` file includes three top level keys `jobs`, `checks`, `heartbeats`. You can configure monitors under each key by defining [monitors](https://cronitor.io/docs/monitor-api#attributes).

```yaml
jobs:
    nightly-database-backup:
        schedule: 0 0 * * *
        notify:
            - devops-alert-pagerduty
        assertions:
            - metric.duration < 5 minutes

    send-welcome-email:
        schedule: every 10 minutes
        assertions:
            - metric.count > 0
            - metric.duration < 30 seconds

checks:
    cronitor-homepage:
        request:
            url: https://cronitor.io
            regions:
                - us-east-1
                - eu-central-1
                - ap-northeast-1
        assertions:
            - response.code = 200
            - response.time < 2s

    cronitor-ping-api:
        request:
            url: https://cronitor.link/ping
        assertions:
            - response.body contains ok
            - response.time < .25s

heartbeats:
    production-deploy:
        notify:
            alerts: ['deploys-slack']
            events: true # send alert when the event occurs

```
#### Async Uploads
If you are working with large YAML files (300+ monitors), you may hit timeouts when trying to sync monitors in a single http request. This workload to be processed asynchronously by adding the key `async: true` to the config file. The request will immediately return a `batch_key`. If a `webhook_url` parameter is included, Cronitor will POST to that URL with the results of the background processing and will include the `batch_key` matching the one returned in the initial response.

### Client::put

You can also create and update monitors by calling `Client::put`. Monitors can be `serde_json::Value`s built with `json!` or any of your own `serde::Serialize` types. For details on all of the attributes that can be set see the Monitor API [documentation](https://cronitor.io/docs/monitor-api#attributes).

```rust
use cronitor::{Client, Config, PutOptions};
use serde_json::json;

let client = Client::new(Config::new().with_api_key("apiKey123"));

let monitors = client.put(&[
  json!({
    "type": "job",
    "key": "send-customer-invoices",
    "schedule": "0 0 * * *",
    "assertions": [
        "metric.duration < 5 min"
    ],
    "notify": ["devops-alerts-slack"]
  }),
  json!({
    "type": "check",
    "key": "Cronitor Homepage",
    "schedule": "every 45 seconds",
    "request": {
        "url": "https://cronitor.io"
    },
    "assertions": [
        "response.code = 200",
        "response.time < 600ms",
    ]
  })
], PutOptions::new())?;
```

Pass `PutOptions::new().rollback(true)` to validate the monitors without saving them. A `400` response is returned as `Error::ApiValidation` with the API's message.

### Listing and Inspecting Monitors

You can fetch multiple monitors using `Client::list` with optional filtering and pagination:

```rust
use cronitor::{Client, ListOptions};

let client = Client::from_env();

// Fetch specific monitors by key
let monitors = client.list(ListOptions::new().keys(["backup-job", "health-check", "send-invoices"]))?;

// Fetch all monitors (first page, 100 results by default)
let monitors = client.list(ListOptions::new())?;

// Fetch monitors with filters
let monitors = client.list(ListOptions::new().monitor_type("check").state("failing"))?;

// Fetch a specific page
let monitors = client.list(ListOptions::new().monitor_type("job").page(2).page_size(50))?;

// Fetch all pages automatically
let monitors = client.list(ListOptions::new().monitor_type("job").auto_paginate(true))?;

// Search monitors
let monitors = client.list(ListOptions::new().search("backup"))?;
```

Monitors returned by `list` and `put` carry their data; use `fetch` to load a single monitor. Typed accessors cover the common fields and `get` walks nested values by dotted path:

```rust
use cronitor::Client;

let client = Client::from_env();

// Fetch an existing monitor
let data = client.monitor("send-invoices").fetch()?;

// Access monitor attributes
println!("{:?}", data.name());
println!("{:?}", data.monitor_type());
println!("{:?}", data.schedule());

// Access nested data
println!("{:?}", data.get_str("attributes.code"));
if let Some(event) = data.get("latest_event") {
    println!("{:?}", event["stamp"]);
    println!("{:?}", event["event"]);
}

// Pretty print the entire monitor as JSON
println!("{data}");

// Or take the raw serde_json::Value
let raw = data.into_value();
```

### Pausing, Reseting, and Deleting

```rust
use cronitor::Client;

let client = Client::from_env();
let monitor = client.monitor("heartbeat-monitor");

monitor.pause(24)?; // pause alerting for 24 hours
monitor.unpause()?; // alias for .pause(0)
monitor.ok()?; // manually reset to a passing state alias for monitor.ping(PingOptions::new().state(State::Ok))
monitor.delete()?; // destroy the monitor
```

## Package Configuration

The client needs to be configured with your account's `API key`, which is available on the [account settings](https://cronitor.io/settings) page. You can also optionally specify an `api_version` and an `environment`. If not provided, your account default is used. These can also be supplied using the environment variables `CRONITOR_API_KEY`, `CRONITOR_API_VERSION`, `CRONITOR_ENVIRONMENT`.

```rust
use cronitor::{Client, Config};

// your api keys can found here - https://cronitor.io/settings
let config = Config::from_env()
    .with_api_key("apiKey123")
    .with_api_version("2020-10-01")
    .with_environment("cluster_1_prod");
let client = Client::new(config);
```

| Setting        | Environment variable   | Default                     |
|----------------|------------------------|-----------------------------|
| `api_key`      | `CRONITOR_API_KEY`     | none                        |
| `api_version`  | `CRONITOR_API_VERSION` | account default             |
| `environment`  | `CRONITOR_ENVIRONMENT` | none                        |
| `timeout`      | `CRONITOR_TIMEOUT`     | 10 seconds (Monitor API)    |
| `config_path`  | `CRONITOR_CONFIG`      | `./cronitor.yaml`           |

`Client::from_env()` (or `Client::default()`) reads all of them; `Config::new()` starts empty and ignores the environment. Every setting can be read back with the getters on `Config` (`client.config().api_key()` etc.).

The API base URLs can be overridden, for example to point unit tests at a local HTTP server:

```rust
use cronitor::{Client, Config};

let client = Client::new(Config::new().with_api_key("test"))
    .with_ping_base_url("http://127.0.0.1:8080/p")
    .with_monitor_api_url("http://127.0.0.1:8080/api/monitors");
```

Requests are retried up to three times with exponential backoff on connection errors and `5xx` responses. Per-monitor overrides for the API key, API version and environment are available through `Monitor::with_api_key`, `Monitor::with_api_version` and `Monitor::with_env`.

### Errors

All fallible calls return `cronitor::Result<T>` with the `cronitor::Error` enum:

- `MonitorNotFound` - the monitor does not exist (HTTP 404)
- `ConfigValidation` - no config path set, or the config file cannot be read
- `ApiValidation` - the Monitor API rejected the monitors (HTTP 400)
- `Authentication` - an API key is required but not configured
- `Api` - any other unexpected API response
- `Http`, `Io`, `Json`, `Yaml` - wrapped transport, filesystem and (de)serialisation errors

## Roadmap

- Integration with [tokio-cron-scheduler](https://crates.io/crates/tokio-cron-scheduler) so scheduled jobs are monitored automatically.

## Contributing

Pull requests and features are happily considered! By participating in this project you agree to abide by the [Code of Conduct](http://contributor-covenant.org/version/2/0).

### To contribute

Fork, then clone the repo:

    git clone git@github.com:your-username/cronitor-rust.git

Set up your machine (Rust 1.75 or later, see [rustup](https://rustup.rs)):

    rustup component add rustfmt clippy

Make sure the tests pass:

    cargo test

**Optional:** Run integration tests against the real Cronitor API:

    export CRONITOR_TEST_API_KEY=your_api_key_here
    cargo test --test integration -- --nocapture

Make your change. Add tests for your change. Make the tests, formatting and lints pass:

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test

Push to your fork and [submit a pull request](https://github.com/cronitorio/cronitor-rust/compare/)
