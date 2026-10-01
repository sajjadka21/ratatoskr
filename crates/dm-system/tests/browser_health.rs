use dm_system::browser_health::connected_browsers;

fn records(values: &[(&str, &str)]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|(key, value)| ((*key).into(), (*value).into()))
        .collect()
}

#[test]
fn registration_without_an_extension_heartbeat_is_not_a_connection() {
    assert!(connected_browsers(&[], 1_000).is_empty());
    assert!(
        connected_browsers(&records(&[("browser_registered_chrome", "true")]), 1_000).is_empty()
    );
}

#[test]
fn a_recent_ping_identifies_the_browser_that_actually_contacted_the_host() {
    assert_eq!(
        connected_browsers(&records(&[("browser_ping_chrome", "999")]), 1_000),
        vec!["chrome"]
    );
}

#[test]
fn the_connection_window_includes_150_seconds_but_not_151() {
    assert_eq!(
        connected_browsers(&records(&[("browser_ping_edge", "850")]), 1_000),
        vec!["edge"]
    );
    assert!(connected_browsers(&records(&[("browser_ping_edge", "849")]), 1_000).is_empty());
}

#[test]
fn malformed_overflowing_negative_and_future_timestamps_are_ignored() {
    for timestamp in [
        "",
        "not-a-time",
        "-1",
        "1001",
        "18446744073709551616",
        "900.5",
    ] {
        assert!(
            connected_browsers(&records(&[("browser_ping_chrome", timestamp)]), 1_000).is_empty(),
            "timestamp {timestamp:?}"
        );
    }
}

#[test]
fn unknown_browser_names_and_unrelated_settings_do_not_become_connections() {
    let values = records(&[
        ("browser_ping_unknown", "999"),
        ("browser_ping_chrome_extra", "999"),
        ("browser_ping_Chrome", "999"),
        ("not_browser_ping_chrome", "999"),
        ("theme", "999"),
    ]);
    assert!(connected_browsers(&values, 1_000).is_empty());
}

#[test]
fn browser_names_are_unique_and_have_a_stable_order_independent_of_record_order() {
    let values = records(&[
        ("browser_ping_firefox", "990"),
        ("browser_ping_edge", "999"),
        ("browser_ping_chrome", "999"),
        ("browser_ping_brave", "950"),
        ("browser_ping_chromium", "990"),
        ("browser_ping_chrome", "900"),
    ]);
    let mut reversed = values.clone();
    reversed.reverse();
    let expected = vec!["brave", "chrome", "chromium", "edge", "firefox"];
    assert_eq!(connected_browsers(&values, 1_000), expected);
    assert_eq!(connected_browsers(&reversed, 1_000), expected);
}

#[test]
fn a_valid_recent_duplicate_is_not_hidden_by_an_expired_or_invalid_duplicate() {
    let values = records(&[
        ("browser_ping_chrome", "999"),
        ("browser_ping_chrome", "849"),
        ("browser_ping_chrome", "invalid"),
    ]);
    assert_eq!(connected_browsers(&values, 1_000), vec!["chrome"]);
}

#[test]
fn timestamp_arithmetic_does_not_wrap_at_u64_boundaries() {
    assert!(
        connected_browsers(
            &records(&[("browser_ping_chrome", "18446744073709551615")]),
            0
        )
        .is_empty()
    );
    assert_eq!(
        connected_browsers(
            &records(&[("browser_ping_firefox", "18446744073709551615")]),
            u64::MAX
        ),
        vec!["firefox"]
    );
}
