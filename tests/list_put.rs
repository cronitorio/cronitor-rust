use cronitor::{Client, Config, Error, ListOptions, PutOptions};
use httpmock::prelude::*;
use serde_json::json;

const FAKE_API_KEY: &str = "cb54ac4fd16142469f2d84fc1bbebd84XXXDEADXXX";
const EXPECTED_AUTH: &str = "Basic Y2I1NGFjNGZkMTYxNDI0NjlmMmQ4NGZjMWJiZWJkODRYWFhERUFEWFhYOg==";

fn authed_client(server: &MockServer) -> Client {
    Client::new(Config::new().with_api_key(FAKE_API_KEY))
        .with_ping_base_url(server.url("/p"))
        .with_monitor_api_url(server.url("/api/monitors"))
        .with_retry_backoff(std::time::Duration::from_millis(1))
}

fn monitor(key: &str) -> serde_json::Value {
    json!({"type": "job", "key": key, "schedule": "* * * * *", "assertions": ["metric.duration < 10 seconds"]})
}

// ---- list -----------------------------------------------------------------

#[test]
fn list_with_filters_sends_default_pagination() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/monitors")
            .header("Authorization", EXPECTED_AUTH)
            .header("User-Agent", "cronitor-rust")
            .query_param("type", "job")
            .query_param("group", "production")
            .query_param("page", "1")
            .query_param("pageSize", "100")
            .matches(|req| {
                let params = req.query_params.clone().unwrap_or_default();
                params.len() == 4
            });
        then.status(200)
            .json_body(json!({"monitors": [monitor("job1"), monitor("job2")]}));
    });
    let monitors = authed_client(&server)
        .list(ListOptions::new().monitor_type("job").group("production"))
        .unwrap();
    mock.assert();
    assert_eq!(monitors.len(), 2);
    assert_eq!(monitors[0].key(), "job1");
    assert_eq!(monitors[0].data().and_then(|d| d.key()), Some("job1"));
    assert_eq!(
        monitors[1].data().and_then(|d| d.schedule()),
        Some("* * * * *")
    );
}

#[test]
fn list_sends_every_filter_and_explicit_pagination() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/monitors")
            .query_param("type", "check")
            .query_param("group", "g")
            .query_param("tag", "t")
            .query_param("state", "failing")
            .query_param("env", "staging")
            .query_param("search", "backup")
            .query_param("sort", "name")
            .query_param("page", "3")
            .query_param("pageSize", "25");
        then.status(200)
            .json_body(json!({"monitors": [monitor("job1")]}));
    });
    let opts = ListOptions::new()
        .monitor_type("check")
        .group("g")
        .tag("t")
        .state("failing")
        .env("staging")
        .search("backup")
        .sort("name")
        .page(3)
        .page_size(25);
    let monitors = authed_client(&server).list(opts).unwrap();
    mock.assert();
    assert_eq!(monitors.len(), 1);
}

#[test]
fn list_auto_paginate_fetches_until_a_short_page() {
    let server = MockServer::start();
    let page1 = server.mock(|when, then| {
        when.method(GET)
            .path("/api/monitors")
            .query_param("page", "1")
            .query_param("pageSize", "2");
        then.status(200)
            .json_body(json!({"monitors": [monitor("job1"), monitor("job2")]}));
    });
    let page2 = server.mock(|when, then| {
        when.method(GET)
            .path("/api/monitors")
            .query_param("page", "2")
            .query_param("pageSize", "2");
        then.status(200)
            .json_body(json!({"monitors": [monitor("job3")]}));
    });
    let monitors = authed_client(&server)
        .list(ListOptions::new().page_size(2).auto_paginate(true))
        .unwrap();
    page1.assert();
    page2.assert();
    let keys: Vec<&str> = monitors.iter().map(|m| m.key()).collect();
    assert_eq!(keys, ["job1", "job2", "job3"]);
}

#[test]
fn list_without_auto_paginate_fetches_a_single_page() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/monitors");
        then.status(200)
            .json_body(json!({"monitors": [monitor("job1"), monitor("job2")]}));
    });
    let monitors = authed_client(&server)
        .list(ListOptions::new().page_size(2))
        .unwrap();
    assert_eq!(mock.hits(), 1);
    assert_eq!(monitors.len(), 2);
}

#[test]
fn list_with_keys_fetches_each_monitor_individually() {
    let server = MockServer::start();
    let key1 = server.mock(|when, then| {
        when.method(GET).path("/api/monitors/key1");
        then.status(200)
            .json_body(json!({"key": "key1", "name": "Monitor 1", "type": "job"}));
    });
    let key2 = server.mock(|when, then| {
        when.method(GET).path("/api/monitors/key2");
        then.status(200)
            .json_body(json!({"key": "key2", "name": "Monitor 2", "type": "check"}));
    });
    let monitors = authed_client(&server)
        .list(ListOptions::new().keys(["key1", "key2"]))
        .unwrap();
    key1.assert();
    key2.assert();
    assert_eq!(monitors.len(), 2);
    assert_eq!(monitors[0].data().and_then(|d| d.name()), Some("Monitor 1"));
    assert_eq!(
        monitors[1].data().and_then(|d| d.monitor_type()),
        Some("check")
    );
}

#[test]
fn list_with_missing_key_is_monitor_not_found() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/api/monitors/nope");
        then.status(404);
    });
    let err = authed_client(&server)
        .list(ListOptions::new().keys(["nope"]))
        .unwrap_err();
    assert!(matches!(err, Error::MonitorNotFound(_)), "{err:?}");
}

#[test]
fn list_empty_results() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/api/monitors");
        then.status(200).json_body(json!({"monitors": []}));
    });
    let monitors = authed_client(&server)
        .list(ListOptions::new().monitor_type("nonexistent"))
        .unwrap();
    assert!(monitors.is_empty());
}

#[test]
fn list_api_error() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/monitors");
        then.status(500).body("Internal Server Error");
    });
    let err = authed_client(&server).list(ListOptions::new()).unwrap_err();
    assert!(
        matches!(err, Error::Api(ref m) if m.contains("Internal Server Error")),
        "{err:?}"
    );
    assert_eq!(mock.hits(), 4, "5xx responses are retried three times");
}

#[test]
fn list_without_api_key_is_an_authentication_error() {
    let client = Client::new(Config::new());
    let err = client.list(ListOptions::new()).unwrap_err();
    assert!(matches!(err, Error::Authentication(_)), "{err:?}");
}

// ---- put ------------------------------------------------------------------

#[test]
fn put_sends_monitors_payload_without_rollback_by_default() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PUT)
            .path("/api/monitors")
            .header("Authorization", EXPECTED_AUTH)
            .header("User-Agent", "cronitor-rust")
            .header("Content-Type", "application/json")
            .json_body(json!({"monitors": [monitor("a-test_key"), monitor("another-test-key")]}));
        then.status(200)
            .json_body(json!({"monitors": [monitor("a-test_key"), monitor("another-test-key")]}));
    });
    let monitors = authed_client(&server)
        .put(
            &[monitor("a-test_key"), monitor("another-test-key")],
            PutOptions::new(),
        )
        .unwrap();
    mock.assert();
    let keys: Vec<&str> = monitors.iter().map(|m| m.key()).collect();
    assert_eq!(keys, ["a-test_key", "another-test-key"]);
    assert_eq!(
        monitors[0].data().and_then(|d| d.get("assertions.0")),
        Some(&json!("metric.duration < 10 seconds"))
    );
}

#[test]
fn put_with_rollback_and_version_header() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PUT)
            .path("/api/monitors")
            .header("Cronitor-Version", "2020-10-01")
            .json_body(
                json!({"monitors": [{"key": "k", "name": "Updated Name"}], "rollback": true}),
            );
        then.status(200)
            .json_body(json!({"monitors": [{"key": "k", "name": "Updated Name"}]}));
    });
    let monitors = authed_client(&server)
        .put(
            &[json!({"key": "k", "name": "Updated Name"})],
            PutOptions::new().rollback(true).api_version("2020-10-01"),
        )
        .unwrap();
    mock.assert();
    assert_eq!(
        monitors[0].data().and_then(|d| d.name()),
        Some("Updated Name")
    );
}

#[test]
fn put_accepts_any_serializable_monitor() {
    #[derive(serde::Serialize)]
    struct Job<'a> {
        key: &'a str,
        schedule: &'a str,
    }
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PUT)
            .path("/api/monitors")
            .json_body(json!({"monitors": [{"key": "typed", "schedule": "0 0 * * *"}]}));
        then.status(200)
            .json_body(json!({"monitors": [{"key": "typed"}]}));
    });
    let monitors = authed_client(&server)
        .put(
            &[Job {
                key: "typed",
                schedule: "0 0 * * *",
            }],
            PutOptions::new(),
        )
        .unwrap();
    mock.assert();
    assert_eq!(monitors[0].key(), "typed");
}

#[test]
fn put_maps_400_to_api_validation_error() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(PUT).path("/api/monitors");
        then.status(400).body("{\"schedule\": [\"invalid\"]}");
    });
    let err = authed_client(&server)
        .put(&[json!({"schedule": "* * * * *"})], PutOptions::new())
        .unwrap_err();
    assert!(
        matches!(err, Error::ApiValidation(ref m) if m.contains("invalid")),
        "{err:?}"
    );
}

#[test]
fn put_maps_other_errors_to_api_error() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(PUT).path("/api/monitors");
        then.status(403).body("forbidden");
    });
    let err = authed_client(&server)
        .put(&[monitor("k")], PutOptions::new())
        .unwrap_err();
    assert!(matches!(err, Error::Api(_)), "{err:?}");
}

#[test]
fn put_uses_per_call_api_key_override() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PUT)
            .path("/api/monitors")
            .header("Authorization", "Basic b3RoZXI6"); // base64("other:")
        then.status(200).json_body(json!({"monitors": []}));
    });
    authed_client(&server)
        .put(&[monitor("k")], PutOptions::new().api_key("other"))
        .unwrap();
    mock.assert();
}

// ---- yaml -----------------------------------------------------------------

#[test]
fn put_yaml_sends_yaml_body_with_rollback_and_returns_parsed_yaml() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PUT)
            .path("/api/monitors.yaml")
            .header("Authorization", EXPECTED_AUTH)
            .header("Content-Type", "application/yaml")
            .matches(|req| {
                let body = req.body.as_ref().map(|b| String::from_utf8_lossy(b).into_owned());
                let doc: serde_yaml::Value = serde_yaml::from_str(&body.unwrap_or_default()).unwrap();
                doc["jobs"]["a-test_key"]["schedule"] == "* * * * *"
                    && doc["rollback"] == true
            });
        then.status(200)
            .header("Content-Type", "application/yaml")
            .body("jobs:\n  a-test_key:\n    schedule: '* * * * *'\n  another-test-key:\n    schedule: '* * * * *'\n");
    });
    let doc: serde_yaml::Value = serde_yaml::from_str(
        "jobs:\n  a-test_key:\n    schedule: '* * * * *'\n  another-test-key:\n    schedule: '* * * * *'\n",
    )
    .unwrap();
    let result = authed_client(&server)
        .put_yaml(&doc, PutOptions::new().rollback(true))
        .unwrap();
    mock.assert();
    assert!(result["jobs"]["a-test_key"].is_mapping());
    assert!(result["jobs"]["another-test-key"].is_mapping());
}

#[test]
fn put_yaml_without_rollback_omits_the_key() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PUT).path("/api/monitors.yaml").matches(|req| {
            let body =
                String::from_utf8_lossy(req.body.as_deref().unwrap_or_default()).into_owned();
            !body.contains("rollback")
        });
        then.status(200).body("jobs: {}\n");
    });
    let doc: serde_yaml::Value = serde_yaml::from_str("jobs: {}\n").unwrap();
    authed_client(&server)
        .put_yaml(&doc, PutOptions::new())
        .unwrap();
    mock.assert();
}

#[test]
fn put_yaml_maps_400_to_api_validation_error() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(PUT).path("/api/monitors.yaml");
        then.status(400).body("bad");
    });
    let doc: serde_yaml::Value = serde_yaml::from_str("jobs: {}\n").unwrap();
    let err = authed_client(&server)
        .put_yaml(&doc, PutOptions::new())
        .unwrap_err();
    assert!(matches!(err, Error::ApiValidation(_)), "{err:?}");
}

#[test]
fn as_yaml_returns_the_document_text() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/monitors.yaml")
            .header("Authorization", EXPECTED_AUTH)
            .header("User-Agent", "cronitor-rust");
        then.status(200)
            .body("jobs:\n  a:\n    schedule: '* * * * *'\n");
    });
    let text = authed_client(&server).as_yaml().unwrap();
    mock.assert();
    assert_eq!(text, "jobs:\n  a:\n    schedule: '* * * * *'\n");
}

#[test]
fn as_yaml_failure_is_an_api_error() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/api/monitors.yaml");
        then.status(401).body("nope");
    });
    let err = authed_client(&server).as_yaml().unwrap_err();
    assert!(matches!(err, Error::Api(_)), "{err:?}");
}
