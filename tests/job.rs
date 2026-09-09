use std::collections::HashMap;
use std::sync::Mutex;

use cronitor::{Client, Config, JobOptions};
use httpmock::prelude::*;
use serde_json::json;

const FAKE_API_KEY: &str = "ping-api-key";

type Params = HashMap<String, String>;

/// Pings captured by monitor key; each test uses its own key so the tests can
/// run in parallel on separate mock servers.
static PINGS: Mutex<Option<HashMap<String, Vec<Params>>>> = Mutex::new(None);

fn record(req: &HttpMockRequest) -> bool {
    let key = req.path.rsplit('/').next().unwrap_or_default().to_string();
    let params: Params = req
        .query_params
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let mut pings = PINGS.lock().unwrap();
    pings
        .get_or_insert_with(HashMap::new)
        .entry(key)
        .or_default()
        .push(params);
    true
}

fn pings_for(key: &str) -> Vec<Params> {
    PINGS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(key).cloned())
        .unwrap_or_default()
}

fn client_for(server: &MockServer) -> Client {
    Client::new(Config::new().with_api_key(FAKE_API_KEY))
        .with_ping_base_url(server.url("/p"))
        .with_monitor_api_url(server.url("/api/monitors"))
}

/// Record every ping's query parameters so the sequence can be asserted.
fn capture_pings(server: &MockServer) {
    server.mock(|when, then| {
        when.method(GET)
            .path_contains(format!("/p/{FAKE_API_KEY}/"))
            .matches(record);
        then.status(200);
    });
}

#[test]
fn job_pings_run_then_complete_with_duration_and_output() {
    let server = MockServer::start();
    capture_pings(&server);
    let client = client_for(&server);

    let result: Result<u32, std::io::Error> = client.job("job-complete", || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        Ok(42)
    });
    assert_eq!(result.unwrap(), 42);

    let pings = pings_for("job-complete");
    assert_eq!(pings.len(), 2, "{pings:?}");
    let run = &pings[0];
    let complete = &pings[1];
    assert_eq!(run["state"], "run");
    assert!(!run.contains_key("metric"));
    assert!(!run.contains_key("message"));
    let series: f64 = run["series"]
        .parse()
        .expect("series is the start timestamp");
    assert!(series > 1_600_000_000.0);
    assert_eq!(complete["state"], "complete");
    assert_eq!(complete["series"], run["series"]);
    assert_eq!(complete["message"], "42");
    let duration: f64 = complete["metric"]
        .strip_prefix("duration:")
        .expect("duration metric")
        .parse()
        .unwrap();
    assert!((0.02..5.0).contains(&duration), "duration {duration}");
}

#[test]
fn job_pings_run_then_fail_with_error_message_and_returns_the_error() {
    let server = MockServer::start();
    capture_pings(&server);
    let client = client_for(&server);

    let result: Result<(), String> = client.job("job-fail", || Err("disk full".to_string()));
    assert_eq!(result.unwrap_err(), "disk full");

    let pings = pings_for("job-fail");
    assert_eq!(pings.len(), 2, "{pings:?}");
    assert_eq!(pings[0]["state"], "run");
    assert_eq!(pings[1]["state"], "fail");
    assert_eq!(pings[1]["message"], "disk full");
    assert_eq!(pings[1]["series"], pings[0]["series"]);
    assert!(pings[1]["metric"].starts_with("duration:"));
}

#[test]
fn job_pings_fail_when_the_closure_panics_and_resumes_the_panic() {
    let server = MockServer::start();
    capture_pings(&server);
    let client = client_for(&server);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.job::<(), String, _>("job-panic", || panic!("kaboom"))
    }));
    assert!(outcome.is_err(), "panic must propagate");

    let pings = pings_for("job-panic");
    assert_eq!(pings.len(), 2, "{pings:?}");
    assert_eq!(pings[1]["state"], "fail");
    assert!(pings[1]["message"].contains("kaboom"), "{:?}", pings[1]);
}

#[test]
fn job_with_log_output_false_omits_the_message() {
    let server = MockServer::start();
    capture_pings(&server);
    let client = client_for(&server);

    let result: Result<&str, String> = client
        .job_with(JobOptions::new("job-no-output").log_output(false), || {
            Ok("secret")
        });
    assert_eq!(result.unwrap(), "secret");

    let pings = pings_for("job-no-output");
    assert_eq!(pings[1]["state"], "complete");
    assert!(!pings[1].contains_key("message"), "{:?}", pings[1]);
}

#[test]
fn job_with_env_sends_the_env_param() {
    let server = MockServer::start();
    capture_pings(&server);
    let client = client_for(&server);

    let _: Result<(), String> =
        client.job_with(JobOptions::new("job-env").env("staging"), || Ok(()));

    let pings = pings_for("job-env");
    assert_eq!(pings.len(), 2);
    assert_eq!(pings[0]["env"], "staging");
    assert_eq!(pings[1]["env"], "staging");
}

#[test]
fn job_attributes_are_queued_and_synced_with_put() {
    let server = MockServer::start();
    capture_pings(&server);
    let put = server.mock(|when, then| {
        when.method(PUT).path("/api/monitors").json_body(json!({
            "monitors": [{"key": "job-attrs", "name": "Ping Decorator Test"}]
        }));
        then.status(200)
            .json_body(json!({"monitors": [{"key": "job-attrs", "name": "Ping Decorator Test"}]}));
    });
    let client = client_for(&server);

    let opts = JobOptions::new("job-attrs").attributes(json!({"name": "Ping Decorator Test"}));
    let _: Result<(), String> = client.job_with(opts.clone(), || Ok(()));
    // Running the same job twice must not queue the attributes twice.
    let _: Result<(), String> = client.job_with(opts, || Ok(()));
    assert_eq!(client.pending_attributes().len(), 1);

    client.sync_monitors().unwrap();
    put.assert();
    assert!(client.pending_attributes().is_empty());

    // Nothing left to sync: no further request is made.
    client.sync_monitors().unwrap();
    assert_eq!(put.hits(), 1);
}

#[test]
fn job_without_api_key_still_runs_the_closure() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });
    let client = Client::new(Config::new())
        .with_ping_base_url(server.url("/p"))
        .with_monitor_api_url(server.url("/api/monitors"));
    let result: Result<u8, String> = client.job("job-no-key", || Ok(7));
    assert_eq!(result.unwrap(), 7);
    assert_eq!(mock.hits(), 0);
}
