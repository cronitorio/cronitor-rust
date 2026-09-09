use std::path::{Path, PathBuf};

use cronitor::{Client, Config, Error};
use httpmock::prelude::*;

const FAKE_API_KEY: &str = "cb54ac4fd16142469f2d84fc1bbebd84XXXDEADXXX";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cronitor.yaml")
}

fn fixture_doc() -> serde_yaml::Value {
    serde_yaml::from_str(&std::fs::read_to_string(fixture()).unwrap()).unwrap()
}

fn client_for(server: &MockServer, config: Config) -> Client {
    Client::new(config.with_api_key(FAKE_API_KEY))
        .with_ping_base_url(server.url("/p"))
        .with_monitor_api_url(server.url("/api/monitors"))
}

/// The fixture document as the server echoes it, with `rollback` stripped.
fn body_without_rollback(req: &HttpMockRequest) -> serde_yaml::Value {
    let text = String::from_utf8_lossy(req.body.as_deref().unwrap_or_default()).into_owned();
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
    if let Some(map) = doc.as_mapping_mut() {
        map.remove("rollback");
    }
    doc
}

fn is_fixture_with_rollback(req: &HttpMockRequest) -> bool {
    let text = String::from_utf8_lossy(req.body.as_deref().unwrap_or_default()).into_owned();
    let doc: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
    doc["rollback"] == true && body_without_rollback(req) == fixture_doc()
}

fn is_fixture_without_rollback(req: &HttpMockRequest) -> bool {
    let text = String::from_utf8_lossy(req.body.as_deref().unwrap_or_default()).into_owned();
    !text.contains("rollback") && body_without_rollback(req) == fixture_doc()
}

#[test]
fn read_config_parses_the_file_and_remembers_the_path() {
    let server = MockServer::start();
    let client = client_for(&server, Config::new());
    assert_eq!(client.config_path(), None);

    let doc = client.read_config(fixture()).unwrap();
    assert!(doc.get("jobs").is_some());
    assert!(doc.get("checks").is_some());
    assert!(doc.get("heartbeats").is_some());
    assert_eq!(client.config_path(), Some(fixture()));
}

#[test]
fn read_config_missing_file_is_a_config_validation_error() {
    let server = MockServer::start();
    let client = client_for(&server, Config::new());
    let err = client.read_config("/definitely/not/here.yaml").unwrap_err();
    assert!(
        matches!(err, Error::ConfigValidation(ref m) if m.contains("not/here.yaml")),
        "{err:?}"
    );
}

#[test]
fn validate_config_puts_yaml_with_rollback() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PUT)
            .path("/api/monitors.yaml")
            .header("Content-Type", "application/yaml")
            .matches(is_fixture_with_rollback);
        then.status(200)
            .body(std::fs::read_to_string(fixture()).unwrap());
    });
    let client = client_for(&server, Config::new().with_config_path(fixture()));
    let summary = client.validate_config().unwrap();
    mock.assert();
    assert_eq!(summary.jobs, 3);
    assert_eq!(summary.checks, 1);
    assert_eq!(summary.heartbeats, 1);
    assert_eq!(summary.total(), 5);
}

#[test]
fn apply_config_puts_yaml_without_rollback() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PUT)
            .path("/api/monitors.yaml")
            .matches(is_fixture_without_rollback);
        then.status(200)
            .body("jobs:\n  only-one:\n    schedule: '* * * * *'\n");
    });
    let client = client_for(&server, Config::new());
    client.read_config(fixture()).unwrap();
    let summary = client.apply_config().unwrap();
    mock.assert();
    assert_eq!(
        (summary.jobs, summary.checks, summary.heartbeats),
        (1, 0, 0)
    );
}

#[test]
fn apply_config_without_a_path_is_a_config_validation_error() {
    let server = MockServer::start();
    let client = client_for(&server, Config::new());
    assert!(matches!(
        client.apply_config().unwrap_err(),
        Error::ConfigValidation(_)
    ));
    assert!(matches!(
        client.validate_config().unwrap_err(),
        Error::ConfigValidation(_)
    ));
}

#[test]
fn apply_config_surfaces_api_validation_errors() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(PUT).path("/api/monitors.yaml");
        then.status(400).body("schedule invalid");
    });
    let client = client_for(&server, Config::new().with_config_path(fixture()));
    let err = client.apply_config().unwrap_err();
    assert!(
        matches!(err, Error::ApiValidation(ref m) if m.contains("schedule invalid")),
        "{err:?}"
    );
}

#[test]
fn generate_config_writes_as_yaml_to_the_config_path() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/monitors.yaml");
        then.status(200)
            .body("jobs:\n  exported:\n    schedule: '0 0 * * *'\n");
    });
    let dir = std::env::temp_dir().join(format!("cronitor-rust-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("generated.yaml");

    let client = client_for(&server, Config::new().with_config_path(&path));
    let written = client.generate_config().unwrap();
    mock.assert();
    assert_eq!(written, path);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "jobs:\n  exported:\n    schedule: '0 0 * * *'\n"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn set_config_path_overrides_the_configured_path() {
    let server = MockServer::start();
    let client = client_for(&server, Config::new().with_config_path("/a.yaml"));
    assert_eq!(client.config_path(), Some(PathBuf::from("/a.yaml")));
    client.set_config_path("/b.yaml");
    assert_eq!(client.config_path(), Some(PathBuf::from("/b.yaml")));
}
