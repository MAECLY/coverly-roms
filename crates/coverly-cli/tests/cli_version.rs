mod common;

#[test]
fn prints_version() {
    common::coverly()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains(format!(
            "coverly {}",
            env!("CARGO_PKG_VERSION")
        )));
}

/// Every CLI test starts `coverly` through `common::coverly()` (no
/// settings file, keychain off): a test that built its own command
/// would read the developer's real keychain.
#[test]
fn every_test_starts_coverly_through_the_helper() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    // Split so this file does not match itself.
    let needles = [concat!("cargo", "_bin("), concat!("Command", "::new(")];
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for needle in needles {
            if text.contains(needle) {
                offenders.push(format!("{} uses {needle}", path.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "start coverly with common::coverly(): {offenders:?}"
    );
}
