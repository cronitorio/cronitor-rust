use cronitor::{Client, Config, PingOptions, State};
use httpmock::prelude::*;

const FAKE_KEY: &str = "d3x0c1";
const FAKE_API_KEY: &str = "ping-api-key";

fn client_for(server: &MockServer, config: Config) -> Client {
    Client::new(config)
        .with_ping_base_url(server.url("/p"))
        .with_monitor_api_url(server.url("/api/monitors"))
}

#[test]
fn ping_with_all_params_hits_exact_url_and_query() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path(format!("/p/{FAKE_API_KEY}/{FAKE_KEY}"))
            .header("User-Agent", "cronitor-rust")
            .query_param("state", "run")
            .query_param("host", "foo")
            .query_param("message", "test message")
            .query_param("series", "abc")
            .query_param("env", "staging")
            .query_param_exists("stamp")
            .matches(|req| {
                let params = req.query_params.clone().unwrap_or_default();
                let mut metrics: Vec<&str> = params
                    .iter()
                    .filter(|(k, _)| k == "metric")
                    .map(|(_, v)| v.as_str())
                    .collect();
                metrics.sort_unstable();
                let stamp_ok = params
                    .iter()
                    .find(|(k, _)| k == "stamp")
                    .and_then(|(_, v)| v.parse::<f64>().ok())
                    .map(|s| s > 1_600_000_000.0)
                    .unwrap_or(false);
                let no_auth = req
                    .headers
                    .as_ref()
                    .map(|h| {
                        !h.iter()
                            .any(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                    })
                    .unwrap_or(true);
                metrics == ["count:5", "duration:100", "error_count:2"] && stamp_ok && no_auth
            });
        then.status(200).body("ok");
    });

    let client = client_for(
        &server,
        Config::new()
            .with_api_key(FAKE_API_KEY)
            .with_environment("staging"),
    );
    let monitor = client.monitor(FAKE_KEY);
    let opts = PingOptions::new()
        .state(State::Run)
        .host("foo")
        .message("test message")
        .series("abc")
        .metric("duration", 100)
        .metric("count", 5)
        .metric("error_count", 2);
    monitor.ping(opts).expect("ping succeeds");

    mock.assert();
}

#[test]
fn ping_omits_unset_params_and_defaults_host_to_hostname() {
    let server = MockServer::start();
    let expected_host = hostname::get().unwrap().to_string_lossy().into_owned();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path(format!("/p/{FAKE_API_KEY}/{FAKE_KEY}"))
            .query_param("host", expected_host.as_str())
            .query_param_exists("stamp")
            .matches(|req| {
                let params = req.query_params.clone().unwrap_or_default();
                let mut keys: Vec<&str> = params.iter().map(|(k, _)| k.as_str()).collect();
                keys.sort_unstable();
                keys == ["host", "stamp"]
            });
        then.status(200);
    });

    let client = client_for(&server, Config::new().with_api_key(FAKE_API_KEY));
    client
        .monitor(FAKE_KEY)
        .ping(PingOptions::default())
        .unwrap();
    mock.assert();
}

#[test]
fn ping_sends_each_state() {
    let server = MockServer::start();
    let client = client_for(&server, Config::new().with_api_key(FAKE_API_KEY));
    let monitor = client.monitor(FAKE_KEY);
    for (state, name) in [
        (State::Run, "run"),
        (State::Complete, "complete"),
        (State::Fail, "fail"),
        (State::Ok, "ok"),
    ] {
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/p/{FAKE_API_KEY}/{FAKE_KEY}"))
                .query_param("state", name);
            then.status(200);
        });
        monitor.ping(PingOptions::new().state(state)).unwrap();
        mock.assert();
        assert_eq!(state.as_str(), name);
    }
}

#[test]
fn ok_pings_with_state_ok() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path(format!("/p/{FAKE_API_KEY}/{FAKE_KEY}"))
            .query_param("state", "ok");
        then.status(200);
    });
    let client = client_for(&server, Config::new().with_api_key(FAKE_API_KEY));
    client.monitor(FAKE_KEY).ok().unwrap();
    mock.assert();
}

#[test]
fn ping_uses_per_monitor_overrides() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path(format!("/p/other-key/{FAKE_KEY}"))
            .query_param("env", "qa");
        then.status(200);
    });
    let client = client_for(
        &server,
        Config::new()
            .with_api_key(FAKE_API_KEY)
            .with_environment("staging"),
    );
    client
        .monitor(FAKE_KEY)
        .with_api_key("other-key")
        .with_env("qa")
        .ping(PingOptions::new())
        .unwrap();
    mock.assert();
}

#[test]
fn ping_without_api_key_is_a_logged_noop() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });
    let client = client_for(&server, Config::new());
    let result = client
        .monitor(FAKE_KEY)
        .ping(PingOptions::new().state(State::Run));
    assert!(result.is_ok(), "missing api key must not be an error");
    assert_eq!(mock.hits(), 0, "no request may be sent without an api key");
}

#[test]
fn ping_reports_http_failures_as_api_errors() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.any_request();
        then.status(404).body("not found");
    });
    let client = client_for(&server, Config::new().with_api_key(FAKE_API_KEY));
    let err = client
        .monitor(FAKE_KEY)
        .ping(PingOptions::new())
        .unwrap_err();
    assert!(matches!(err, cronitor::Error::Api(_)), "{err:?}");
}

#[test]
fn ping_retries_5xx_responses_three_times_with_backoff() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path(format!("/p/{FAKE_API_KEY}/{FAKE_KEY}"));
        then.status(503).body("unavailable");
    });
    let client = client_for(&server, Config::new().with_api_key(FAKE_API_KEY))
        .with_retry_backoff(std::time::Duration::from_millis(5));
    let started = std::time::Instant::now();
    let err = client
        .monitor(FAKE_KEY)
        .ping(PingOptions::new())
        .unwrap_err();
    assert!(matches!(err, cronitor::Error::Api(_)), "{err:?}");
    // 1 initial attempt + 3 retries
    assert_eq!(mock.hits(), 4);
    // backoff 5ms + 10ms + 20ms
    assert!(started.elapsed() >= std::time::Duration::from_millis(35));
}

#[test]
fn ping_retries_connection_errors_then_returns_http_error() {
    // Nothing listens on this port; the connection is refused every time.
    let client = Client::new(Config::new().with_api_key(FAKE_API_KEY))
        .with_ping_base_url("http://127.0.0.1:9/p")
        .with_retry_backoff(std::time::Duration::from_millis(1));
    let err = client
        .monitor(FAKE_KEY)
        .ping(PingOptions::new())
        .unwrap_err();
    assert!(matches!(err, cronitor::Error::Http(_)), "{err:?}");
}

#[test]
fn ping_does_not_retry_4xx_responses() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path(format!("/p/{FAKE_API_KEY}/{FAKE_KEY}"));
        then.status(400);
    });
    let client = client_for(&server, Config::new().with_api_key(FAKE_API_KEY));
    client
        .monitor(FAKE_KEY)
        .ping(PingOptions::new())
        .unwrap_err();
    assert_eq!(mock.hits(), 1);
}
