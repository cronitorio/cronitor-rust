use cronitor::{Client, Config, Error};
use httpmock::prelude::*;
use serde_json::json;

const FAKE_API_KEY: &str = "cb54ac4fd16142469f2d84fc1bbebd84XXXDEADXXX";
// base64("cb54ac4fd16142469f2d84fc1bbebd84XXXDEADXXX:")
const EXPECTED_AUTH: &str = "Basic Y2I1NGFjNGZkMTYxNDI0NjlmMmQ4NGZjMWJiZWJkODRYWFhERUFEWFhYOg==";

fn client_for(server: &MockServer, config: Config) -> Client {
    Client::new(config)
        .with_ping_base_url(server.url("/p"))
        .with_monitor_api_url(server.url("/api/monitors"))
}

fn authed_client(server: &MockServer) -> Client {
    client_for(server, Config::new().with_api_key(FAKE_API_KEY))
}

#[test]
fn fetch_returns_parsed_monitor_with_auth_and_version_headers() {
    let server = MockServer::start();
    let body = json!({
        "key": "send-invoices",
        "name": "Send Invoices",
        "type": "job",
        "schedule": "0 8 * * *",
        "passing": true,
        "attributes": {"code": "ABC123", "group_name": "production"},
        "latest_event": {"stamp": 1234567890.0, "event": "complete", "metrics": {"duration": 1.5}},
        "latest_events": [{"event": "run"}, {"event": "complete"}]
    });
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/monitors/send-invoices")
            .header("Authorization", EXPECTED_AUTH)
            .header("User-Agent", "cronitor-rust")
            .header("Cronitor-Version", "2020-10-01");
        then.status(200).json_body(body.clone());
    });

    let client = client_for(
        &server,
        Config::new()
            .with_api_key(FAKE_API_KEY)
            .with_api_version("2020-10-01"),
    );
    let data = client.monitor("send-invoices").fetch().unwrap();
    mock.assert();

    assert_eq!(data.key(), Some("send-invoices"));
    assert_eq!(data.name(), Some("Send Invoices"));
    assert_eq!(data.monitor_type(), Some("job"));
    assert_eq!(data.schedule(), Some("0 8 * * *"));
    assert_eq!(data.get("passing"), Some(&json!(true)));
    assert_eq!(data.get_str("attributes.code"), Some("ABC123"));
    assert_eq!(data.get("latest_event.metrics.duration"), Some(&json!(1.5)));
    assert_eq!(data.get("latest_events.1.event"), Some(&json!("complete")));
    assert_eq!(data.get("missing.path"), None);
    assert_eq!(data["attributes"]["group_name"], json!("production"));
    assert_eq!(data.as_value(), &body);

    // Display is pretty-printed JSON
    let text = data.to_string();
    assert!(text.contains('\n') && text.contains("  "));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).unwrap(),
        body
    );
}

#[test]
fn fetch_without_cronitor_version_omits_the_header() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/monitors/abc").matches(|req| {
            !req.headers
                .as_ref()
                .map(|h| {
                    h.iter()
                        .any(|(k, _)| k.eq_ignore_ascii_case("cronitor-version"))
                })
                .unwrap_or(false)
        });
        then.status(200).json_body(json!({"key": "abc"}));
    });
    authed_client(&server).monitor("abc").fetch().unwrap();
    mock.assert();
}

#[test]
fn fetch_maps_404_to_monitor_not_found() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/api/monitors/nope");
        then.status(404).body("not found");
    });
    let err = authed_client(&server).monitor("nope").fetch().unwrap_err();
    assert!(
        matches!(err, Error::MonitorNotFound(ref k) if k.contains("nope")),
        "{err:?}"
    );
}

#[test]
fn fetch_without_api_key_is_an_authentication_error() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });
    let err = client_for(&server, Config::new())
        .monitor("abc")
        .fetch()
        .unwrap_err();
    assert!(matches!(err, Error::Authentication(_)), "{err:?}");
    assert_eq!(mock.hits(), 0);
}

#[test]
fn fetch_maps_other_failures_to_api_error() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/api/monitors/abc");
        then.status(403).body("forbidden");
    });
    let err = authed_client(&server).monitor("abc").fetch().unwrap_err();
    assert!(
        matches!(err, Error::Api(ref m) if m.contains("forbidden")),
        "{err:?}"
    );
}

#[test]
fn delete_maps_status_codes() {
    let server = MockServer::start();
    let ok = server.mock(|when, then| {
        when.method(DELETE)
            .path("/api/monitors/gone")
            .header("Authorization", EXPECTED_AUTH)
            .header("User-Agent", "cronitor-rust");
        then.status(204);
    });
    server.mock(|when, then| {
        when.method(DELETE).path("/api/monitors/missing");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(DELETE).path("/api/monitors/broken");
        then.status(403);
    });
    let client = authed_client(&server);
    client.monitor("gone").delete().unwrap();
    ok.assert();
    assert!(matches!(
        client.monitor("missing").delete().unwrap_err(),
        Error::MonitorNotFound(_)
    ));
    assert!(matches!(
        client.monitor("broken").delete().unwrap_err(),
        Error::Api(_)
    ));
}

#[test]
fn pause_and_unpause_hit_the_pause_endpoint() {
    let server = MockServer::start();
    let pause = server.mock(|when, then| {
        when.method(GET)
            .path("/api/monitors/abc/pause/24")
            .header("Authorization", EXPECTED_AUTH)
            .header("User-Agent", "cronitor-rust");
        then.status(200);
    });
    let unpause = server.mock(|when, then| {
        when.method(GET).path("/api/monitors/abc/pause/0");
        then.status(200);
    });
    let client = authed_client(&server);
    client.monitor("abc").pause(24).unwrap();
    client.monitor("abc").unpause().unwrap();
    pause.assert();
    unpause.assert();
}

#[test]
fn pause_without_api_key_is_a_logged_noop() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });
    client_for(&server, Config::new())
        .monitor("abc")
        .pause(1)
        .unwrap();
    assert_eq!(mock.hits(), 0);
}

#[test]
fn pause_failure_is_an_api_error() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/api/monitors/abc/pause/1");
        then.status(401);
    });
    let err = authed_client(&server).monitor("abc").pause(1).unwrap_err();
    assert!(matches!(err, Error::Api(_)), "{err:?}");
}
