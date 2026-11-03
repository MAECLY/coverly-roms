//! `coverly integrations` end to end (integrations spec D18–D20), always
//! with the keychain turned off (`common::coverly`) and a MockServer in
//! place of the services: no real keychain is read and no real service is
//! asked. Outputs are checked for the absence of the test key.

mod common;

use std::net::TcpListener;

use common::coverly;
use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::sources::steamgriddb::search_path;
use predicates::str::contains;

const KEY: &str = "sk-test";

fn output_of(a: &assert_cmd::assert::Assert) -> String {
    let out = a.get_output();
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn list_works_without_a_keychain_and_says_so() {
    coverly()
        .args(["integrations", "list"])
        .assert()
        .success()
        .stdout(contains("keychain: off"))
        .stdout(contains("steamgriddb"))
        .stdout(contains("screenscraper"))
        .stdout(contains("switch-catalog  off"))
        .stdout(contains("coming later: IGDB"));
}

#[test]
fn list_json_has_the_documented_shape() {
    let out = coverly()
        .args(["integrations", "list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["keychain"], "off");
    let ids: Vec<&str> = v["integrations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["steamgriddb", "screenscraper", "switch-catalog", "updates"]
    );
    assert!(v["planned"].as_array().unwrap().iter().any(|p| p == "igdb"));
}

#[test]
fn set_with_the_keychain_off_fails_and_never_echoes_the_value() {
    let a = coverly()
        .args(["integrations", "set", "steamgriddb"])
        .write_stdin("sk-123\n")
        .assert()
        .code(1)
        .stderr(contains("the keychain is off"));
    assert!(!output_of(&a).contains("sk-123"));
}

#[test]
fn a_secret_as_an_argument_is_bad_usage_and_never_echoed() {
    for args in [
        ["integrations", "set", "steamgriddb", "sk-123"].as_slice(),
        &[
            "integrations",
            "set",
            "screenscraper",
            "--field",
            "password",
            "sk-123",
        ],
        &[
            "integrations",
            "set",
            "steamgriddb",
            "--from-env",
            "VAR",
            "sk-123",
        ],
        &["integrations", "forget", "steamgriddb", "sk-123", "--yes"],
        &["integrations", "test", "steamgriddb", "sk-123"],
    ] {
        let a = coverly()
            .args(args)
            .assert()
            .code(2)
            .stderr(contains("never as an argument"));
        // A mistyped secret must not be repeated on the screen or in a log,
        // stored or confirmed.
        let out = output_of(&a);
        assert!(!out.contains("sk-123"), "{args:?}: {out}");
        assert!(!out.contains("saved"), "{args:?}: {out}");
    }
}

#[test]
fn a_planned_or_unknown_integration_is_bad_usage() {
    coverly()
        .args(["integrations", "test", "igdb"])
        .assert()
        .code(2)
        .stderr(contains("igdb"));
    coverly()
        .args(["integrations", "set", "nope"])
        .write_stdin("x\n")
        .assert()
        .code(2);
    coverly()
        .args(["integrations", "set", "switch-catalog"])
        .write_stdin("x\n")
        .assert()
        .code(2)
        .stderr(contains("has no credentials"));
}

#[test]
fn forget_without_a_terminal_needs_yes() {
    coverly()
        .args(["integrations", "forget", "steamgriddb"])
        .write_stdin("y\n")
        .assert()
        .code(2)
        .stderr(contains("--yes"));
    coverly()
        .args(["integrations", "forget", "steamgriddb", "--yes"])
        .assert()
        .code(1)
        .stderr(contains("the keychain is off"));
}

fn sgdb_test(base: &str) -> assert_cmd::Command {
    let mut cmd = coverly();
    cmd.args([
        "integrations",
        "test",
        "steamgriddb",
        "--steamgriddb-url",
        base,
    ])
    .env("COVERLY_SGDB_KEY", KEY);
    cmd
}

#[test]
fn test_says_ok_when_the_service_accepts_the_key() {
    let server = MockServer::start();
    server.serve_json(
        &format!("/sgdb{}", search_path("mario")),
        r#"{"success":true,"data":[]}"#,
    );
    let a = sgdb_test(&server.url("/sgdb"))
        .assert()
        .success()
        .stdout(contains("steamgriddb: ok"));
    assert!(!output_of(&a).contains(KEY));
    // The key went to the mock as a bearer token, once.
    let reqs = server.requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(
        reqs[0].header("authorization"),
        Some(format!("Bearer {KEY}").as_str())
    );
}

#[test]
fn test_says_invalid_when_the_service_refuses_the_key() {
    let server = MockServer::start();
    server.mock(
        &format!("/sgdb{}", search_path("mario")),
        MockResponse::status(401),
    );
    let a = sgdb_test(&server.url("/sgdb"))
        .assert()
        .code(1)
        .stdout(contains("steamgriddb: invalid"));
    assert!(!output_of(&a).contains(KEY));
}

#[test]
fn test_says_offline_when_nothing_answers() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let a = sgdb_test(&format!("http://127.0.0.1:{port}"))
        .assert()
        .code(5)
        .stdout(contains("steamgriddb: offline"));
    assert!(!output_of(&a).contains(KEY));
}

#[test]
fn test_json_carries_the_verdict_only() {
    let server = MockServer::start();
    server.serve_json(
        &format!("/sgdb{}", search_path("mario")),
        r#"{"success":true,"data":[]}"#,
    );
    let out = sgdb_test(&server.url("/sgdb"))
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["id"], "steamgriddb");
    assert_eq!(v["verdict"], "ok");
}

#[test]
fn test_without_any_key_is_missing() {
    let server = MockServer::start();
    coverly()
        .args([
            "integrations",
            "test",
            "steamgriddb",
            "--steamgriddb-url",
            &server.url("/sgdb"),
        ])
        .env_remove("COVERLY_SGDB_KEY")
        .assert()
        .code(1)
        .stdout(contains("steamgriddb: missing"));
    assert!(server.requests().is_empty());
}

#[test]
fn test_says_rate_limited_with_exit_5() {
    let server = MockServer::start();
    server.mock(
        &format!("/sgdb{}", search_path("mario")),
        MockResponse::status(429),
    );
    let a = sgdb_test(&server.url("/sgdb"))
        .assert()
        .code(5)
        .stdout(contains("steamgriddb: rate-limited"))
        .stderr(contains("try again later"));
    assert!(!output_of(&a).contains(KEY));
    // One try, as in the app: no retry loop on a 429.
    assert_eq!(server.requests().len(), 1);
}
