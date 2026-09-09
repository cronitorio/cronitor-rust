//! Thin HTTP layer over `ureq` adding retries and Cronitor's standard headers.

use std::thread;
use std::time::Duration;

use crate::{Client, Result};

/// `User-Agent` sent with every request.
pub const USER_AGENT: &str = "cronitor-rust";

/// Number of retries after the initial attempt.
pub(crate) const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff between retries (matches the Python SDK).
pub(crate) const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_millis(300);

/// A fully read HTTP response.
#[derive(Debug)]
pub(crate) struct Response {
    pub status: u16,
    pub body: String,
}

impl Client {
    /// Send `request`, retrying connection errors and 5xx responses with
    /// exponential backoff. Any final response, including 4xx/5xx, is returned
    /// as `Ok` so callers can map status codes to SDK errors.
    pub(crate) fn send(&self, request: ureq::Request, body: Option<&str>) -> Result<Response> {
        let request = request.set("User-Agent", USER_AGENT);
        let mut attempt = 0;
        loop {
            let result = match body {
                Some(body) => request.clone().send_string(body),
                None => request.clone().call(),
            };
            let retryable = match result {
                Ok(resp) => return read(resp),
                Err(ureq::Error::Status(code, resp)) => {
                    if code < 500 || attempt >= MAX_RETRIES {
                        return read(resp);
                    }
                    format!("HTTP {code}")
                }
                Err(err @ ureq::Error::Transport(_)) => {
                    if attempt >= MAX_RETRIES {
                        return Err(err.into());
                    }
                    err.to_string()
                }
            };
            let delay = self.retry_backoff * 2u32.pow(attempt);
            log::debug!(
                "cronitor request failed ({retryable}); retry {} of {MAX_RETRIES} in {delay:?}",
                attempt + 1
            );
            thread::sleep(delay);
            attempt += 1;
        }
    }
}

fn read(resp: ureq::Response) -> Result<Response> {
    let status = resp.status();
    let body = resp.into_string()?;
    Ok(Response { status, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_constants_match_the_spec() {
        assert_eq!(MAX_RETRIES, 3);
        assert_eq!(DEFAULT_RETRY_BACKOFF, Duration::from_millis(300));
    }
}
