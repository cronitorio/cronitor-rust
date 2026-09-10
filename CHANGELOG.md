# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-09-09

### Added

- Initial release of the official Cronitor Rust SDK.
- The `User-Agent` header includes the crate version, for example `cronitor-rust/1.0.0`.
- `Config` read from `CRONITOR_API_KEY`, `CRONITOR_API_VERSION`, `CRONITOR_ENVIRONMENT`,
  `CRONITOR_TIMEOUT` and `CRONITOR_CONFIG`, with builder-style overrides.
- `Client` with overridable ping and Monitor API base URLs, retries with exponential
  backoff on connection errors and 5xx responses.
- `Monitor` with `ping`, `ok`, `pause`, `unpause`, `delete` and `fetch`.
- `Client::list`, `Client::put`, `Client::put_yaml` and `Client::as_yaml`.
- `Client::job` wrapper sending run/complete/fail telemetry with duration metrics, and
  `Client::sync_monitors` for queued monitor attributes.
- YAML configuration helpers: `read_config`, `validate_config`, `apply_config` and
  `generate_config`.
