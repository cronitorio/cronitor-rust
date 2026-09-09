//! Official Rust SDK for [Cronitor](https://cronitor.io).
//!
//! Cronitor provides end-to-end monitoring for background jobs, websites, APIs and
//! anything else that can send or receive an HTTP request. This crate wraps the
//! [telemetry API](https://cronitor.io/docs/telemetry-api) and the
//! [Monitor API](https://cronitor.io/docs/monitor-api).
//!
//! The API is blocking. From an async runtime, call it through
//! `tokio::task::spawn_blocking` (or your runtime's equivalent).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod config;
mod error;

pub use config::{Config, DEFAULT_TIMEOUT};
pub use error::{Error, Result};
