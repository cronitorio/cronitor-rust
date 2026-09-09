use std::time::Duration;

use cronitor::{Config, Error};

#[test]
fn config_new_has_spec_defaults() {
    let config = Config::new();
    assert_eq!(config.api_key(), None);
    assert_eq!(config.api_version(), None);
    assert_eq!(config.environment(), None);
    assert_eq!(config.timeout(), Duration::from_secs(10));
    assert_eq!(config.config_path(), None);
}

#[test]
fn config_builders_set_every_setting() {
    let config = Config::new()
        .with_api_key("key123")
        .with_api_version("2020-10-01")
        .with_environment("staging")
        .with_timeout(Duration::from_secs(30))
        .with_config_path("./cronitor.yaml");
    assert_eq!(config.api_key(), Some("key123"));
    assert_eq!(config.api_version(), Some("2020-10-01"));
    assert_eq!(config.environment(), Some("staging"));
    assert_eq!(config.timeout(), Duration::from_secs(30));
    assert_eq!(
        config
            .config_path()
            .map(|p| p.to_string_lossy().into_owned()),
        Some("./cronitor.yaml".to_string())
    );
}

#[test]
fn config_from_env_reads_cronitor_variables() {
    // Environment variables are process-global, so this is the only test that
    // touches them; the assertions run under a single lock-free sequence.
    let vars = [
        ("CRONITOR_API_KEY", "env-key"),
        ("CRONITOR_API_VERSION", "2020-10-01"),
        ("CRONITOR_ENVIRONMENT", "prod"),
        ("CRONITOR_TIMEOUT", "25"),
        ("CRONITOR_CONFIG", "/etc/cronitor.yaml"),
    ];
    for (k, v) in vars {
        std::env::set_var(k, v);
    }
    let config = Config::from_env();
    for (k, _) in vars {
        std::env::remove_var(k);
    }

    assert_eq!(config.api_key(), Some("env-key"));
    assert_eq!(config.api_version(), Some("2020-10-01"));
    assert_eq!(config.environment(), Some("prod"));
    assert_eq!(config.timeout(), Duration::from_secs(25));
    assert_eq!(
        config
            .config_path()
            .map(|p| p.to_string_lossy().into_owned()),
        Some("/etc/cronitor.yaml".to_string())
    );

    // Empty / unparsable values fall back to the defaults.
    std::env::set_var("CRONITOR_API_KEY", "");
    std::env::set_var("CRONITOR_TIMEOUT", "not-a-number");
    let config = Config::from_env();
    std::env::remove_var("CRONITOR_API_KEY");
    std::env::remove_var("CRONITOR_TIMEOUT");
    assert_eq!(config.api_key(), None);
    assert_eq!(config.timeout(), Duration::from_secs(10));
}

#[test]
fn error_variants_display_their_context() {
    let cases: Vec<(Error, &str)> = vec![
        (Error::MonitorNotFound("abc".into()), "abc"),
        (Error::ConfigValidation("no path".into()), "no path"),
        (Error::ApiValidation("bad schedule".into()), "bad schedule"),
        (Error::Authentication("no api key".into()), "no api key"),
        (Error::Api("boom".into()), "boom"),
    ];
    for (err, needle) in cases {
        let text = err.to_string();
        assert!(text.contains(needle), "{text:?} should contain {needle:?}");
    }
    let io: Error = std::io::Error::new(std::io::ErrorKind::NotFound, "gone").into();
    assert!(matches!(io, Error::Io(_)));
    let json: Error = serde_json::from_str::<serde_json::Value>("{")
        .unwrap_err()
        .into();
    assert!(matches!(json, Error::Json(_)));
    let yaml: Error = serde_yaml::from_str::<serde_yaml::Value>(": :")
        .unwrap_err()
        .into();
    assert!(matches!(yaml, Error::Yaml(_)));
}
