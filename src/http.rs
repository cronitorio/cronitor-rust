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

/// `Authorization` header value for HTTP Basic auth with the API key as the
/// username and an empty password.
pub(crate) fn basic_auth(api_key: &str) -> String {
    format!("Basic {}", base64(format!("{api_key}:").as_bytes()))
}

fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_reference_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(basic_auth("apiKey123"), "Basic YXBpS2V5MTIzOg==");
    }

    #[test]
    fn retry_constants_match_the_spec() {
        assert_eq!(MAX_RETRIES, 3);
        assert_eq!(DEFAULT_RETRY_BACKOFF, Duration::from_millis(300));
    }
}
