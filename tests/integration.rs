//! Integration tests that make real API calls to Cronitor.
//!
//! These tests are skipped unless the `CRONITOR_TEST_API_KEY` environment
//! variable is set:
//!
//! ```sh
//! export CRONITOR_TEST_API_KEY=your_api_key_here
//! cargo test --test integration -- --nocapture
//! ```

use cronitor::{Client, Config, ListOptions, PingOptions, State};

const SKIP_REASON: &str = "Set CRONITOR_TEST_API_KEY environment variable to run integration tests";

/// A reserved monitor key for running ping tests against cronitor.link.
const PING_TEST_KEY: &str = "d3x0c1";

fn client() -> Option<Client> {
    match std::env::var("CRONITOR_TEST_API_KEY") {
        Ok(key) if !key.trim().is_empty() => Some(Client::new(Config::new().with_api_key(key))),
        _ => {
            eprintln!("skipped: {SKIP_REASON}");
            None
        }
    }
}

#[test]
fn ping_endpoints_accept_every_state() {
    let Some(client) = client() else { return };
    let monitor = client.monitor(PING_TEST_KEY);
    monitor.ping(PingOptions::new()).expect("heartbeat ping");
    for state in [State::Run, State::Complete, State::Fail, State::Ok] {
        monitor
            .ping(PingOptions::new().state(state))
            .unwrap_or_else(|e| panic!("ping state={state}: {e}"));
    }
}

#[test]
fn list_all_monitors() {
    let Some(client) = client() else { return };
    let monitors = client.list(ListOptions::new()).unwrap();
    if let Some(monitor) = monitors.first() {
        let data = monitor.data().expect("listed monitors carry data");
        assert!(data.key().is_some());
        assert!(data.name().is_some());
        println!(
            "\nFound {} monitors on first page (default pageSize=100)\n  First monitor: {} ({})",
            monitors.len(),
            data.name().unwrap_or_default(),
            data.key().unwrap_or_default()
        );
    }
}

#[test]
fn list_all_monitors_auto_paginate() {
    let Some(client) = client() else { return };
    let all = client.list(ListOptions::new().auto_paginate(true)).unwrap();
    let page1 = client.list(ListOptions::new().page_size(100)).unwrap();
    assert!(all.len() >= page1.len());
    println!(
        "\nAuto-paginate fetched {} monitors (first page had {})",
        all.len(),
        page1.len()
    );
}

#[test]
fn list_with_pagination() {
    let Some(client) = client() else { return };
    let monitors = client.list(ListOptions::new().page_size(5)).unwrap();
    assert!(monitors.len() <= 5);
    println!(
        "\nPagination works, got {} monitors (max 5)",
        monitors.len()
    );
}

#[test]
fn list_with_filter() {
    let Some(client) = client() else { return };
    let monitors = client.list(ListOptions::new().monitor_type("job")).unwrap();
    for monitor in &monitors {
        assert_eq!(monitor.data().and_then(|d| d.monitor_type()), Some("job"));
    }
    println!("\nFilter works, got {} job monitors", monitors.len());
}

#[test]
fn list_with_search() {
    let Some(client) = client() else { return };
    let monitors = client.list(ListOptions::new().search("test job")).unwrap();
    println!(
        "\nSearch works, found {} monitors matching 'test job'",
        monitors.len()
    );
}

#[test]
fn list_specific_keys() {
    let Some(client) = client() else { return };
    let sample = client.list(ListOptions::new().page_size(2)).unwrap();
    if sample.is_empty() {
        eprintln!("skipped: no monitors found in account");
        return;
    }
    let keys: Vec<String> = sample.iter().map(|m| m.key().to_string()).collect();
    let monitors = client.list(ListOptions::new().keys(keys.clone())).unwrap();
    assert_eq!(monitors.len(), keys.len());
    let mut returned: Vec<&str> = monitors.iter().map(|m| m.key()).collect();
    let mut expected: Vec<&str> = keys.iter().map(String::as_str).collect();
    returned.sort_unstable();
    expected.sort_unstable();
    assert_eq!(returned, expected);
    println!("\nFetched specific monitors: {}", keys.join(", "));
}

#[test]
fn monitor_data_structure() {
    let Some(client) = client() else { return };
    let monitors = client.list(ListOptions::new().page_size(1)).unwrap();
    let Some(monitor) = monitors.first() else {
        eprintln!("skipped: no monitors found in account");
        return;
    };
    let data = monitor.data().unwrap();
    assert!(data.key().is_some());
    assert!(data.name().is_some());
    assert!(data.monitor_type().is_some());
    assert!(data.get("attributes").is_some());
    assert!(data.get_str("attributes.code").is_some());

    let text = data.to_string();
    assert!(text.contains(data.key().unwrap()));
    assert!(text.contains('\n'));

    // fetch() returns the same document as the listing did
    let fetched = client.monitor(monitor.key()).fetch().unwrap();
    assert_eq!(fetched.key(), data.key());
    println!(
        "\nMonitor data structure correct\n  Key: {}\n  Name: {}\n  Type: {}",
        data.key().unwrap_or_default(),
        data.name().unwrap_or_default(),
        data.monitor_type().unwrap_or_default()
    );
}
