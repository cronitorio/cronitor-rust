//! Official Rust SDK for [Cronitor](https://cronitor.io).
//!
//! Cronitor provides end-to-end monitoring for background jobs, websites, APIs and
//! anything else that can send or receive an HTTP request. This crate wraps the
//! [telemetry API](https://cronitor.io/docs/telemetry-api) and the
//! [Monitor API](https://cronitor.io/docs/monitor-api).
//!
//! The API is blocking. From an async runtime, call it through
//! `tokio::task::spawn_blocking` (or your runtime's equivalent).
//!
//! ```no_run
//! use cronitor::{Client, PingOptions, State};
//!
//! let client = Client::from_env(); // reads CRONITOR_API_KEY etc.
//! let monitor = client.monitor("send-invoices");
//! monitor.ping(PingOptions::new().state(State::Run))?;
//! # Ok::<(), cronitor::Error>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod client;
mod config;
mod error;
mod http;
mod monitor;

pub use client::{Client, DEFAULT_MONITOR_API_URL, DEFAULT_PING_BASE_URL};
pub use config::{Config, DEFAULT_TIMEOUT};
pub use error::{Error, Result};
pub use http::USER_AGENT;
pub use monitor::{Monitor, PingOptions, State};
