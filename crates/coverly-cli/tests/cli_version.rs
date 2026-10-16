use assert_cmd::Command;

#[test]
fn prints_version() {
    Command::cargo_bin("coverly")
        .unwrap()
        .env("COVERLY_SETTINGS", "")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("coverly 0.0.1"));
}
