#[test]
fn user_agent_carries_the_crate_version() {
    assert_eq!(
        cronitor::USER_AGENT,
        format!("cronitor-rust/{}", env!("CARGO_PKG_VERSION"))
    );
}
