//! Update manifest, channels, key file and minisign verification (M7).

use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::net::{NetConfig, UreqHttp};
use coverly_core::settings::UpdateChannel;
use coverly_core::update::testing::{manifest_json, TestKey};
use coverly_core::update::verify::{verify_signed, VerifyError};
use coverly_core::update::*;

fn http() -> UreqHttp {
    UreqHttp::new(&NetConfig::default())
}

#[test]
fn channels_have_their_own_manifest_on_the_project_releases() {
    assert_eq!(
        endpoint_for(UpdateChannel::Stable),
        "https://github.com/MAECLY/coverly-roms/releases/latest/download/latest.json"
    );
    assert_eq!(
        endpoint_for(UpdateChannel::Beta),
        format!("https://github.com/MAECLY/coverly-roms/releases/download/{BETA_TAG}/latest.json")
    );
    assert_eq!(
        resolve_endpoint(None, UpdateChannel::Beta).unwrap(),
        BETA_ENDPOINT
    );
    assert_eq!(
        resolve_endpoint(Some("  "), UpdateChannel::Stable).unwrap(),
        STABLE_ENDPOINT
    );
    assert_eq!(
        resolve_endpoint(
            Some(" http://127.0.0.1:9/latest.json "),
            UpdateChannel::Beta
        )
        .unwrap(),
        "http://127.0.0.1:9/latest.json"
    );
    assert!(resolve_endpoint(Some("file:///etc/passwd"), UpdateChannel::Stable).is_err());
    assert!(resolve_endpoint(Some("not a url"), UpdateChannel::Stable).is_err());
}

#[test]
fn the_key_file_is_a_placeholder_or_one_base64_key() {
    assert_eq!(parse_pubkey_file("# c\nUNSET\n"), None);
    assert_eq!(
        parse_pubkey_file("# c\n\n  dW50cnVzdGVk  \n"),
        Some("dW50cnVzdGVk")
    );
    assert_eq!(parse_pubkey_file("# only comments\n"), None);
    let shipped = include_str!("../updater.pub");
    match parse_pubkey_file(shipped) {
        None => assert!(shipped.contains(PUBKEY_PLACEHOLDER)),
        Some(key) => {
            use base64::Engine as _;
            let text = base64::engine::general_purpose::STANDARD
                .decode(key)
                .expect("base64");
            assert!(String::from_utf8(text)
                .unwrap()
                .contains("minisign public key"));
        }
    }
    assert_eq!(release_pubkey(), parse_pubkey_file(shipped));
}

#[test]
fn semver_precedence_decides_what_is_newer() {
    assert!(is_newer("0.1.0", "0.2.0"));
    assert!(
        is_newer("0.2.0-beta.1", "0.2.0"),
        "a release beats its betas"
    );
    assert!(is_newer("0.2.0-beta.1", "v0.2.0-beta.2"));
    assert!(!is_newer("0.2.0", "0.2.0"));
    assert!(
        !is_newer("0.2.0", "0.2.0+build.5"),
        "build metadata is not newer"
    );
    assert!(!is_newer("0.2.0", "0.1.9"));
    assert!(!is_newer("0.2.0", "latest"), "unparseable is never newer");
}

#[test]
fn cli_platform_keys_follow_the_tauri_names() {
    assert_eq!(
        cli_platform_key_for("macos", "aarch64").as_deref(),
        Some("cli-darwin-aarch64")
    );
    assert_eq!(
        cli_platform_key_for("windows", "x86_64").as_deref(),
        Some("cli-windows-x86_64")
    );
    assert_eq!(
        cli_platform_key_for("linux", "aarch64").as_deref(),
        Some("cli-linux-aarch64")
    );
    assert_eq!(cli_platform_key_for("freebsd", "x86_64"), None);
    assert_eq!(cli_platform_key_for("linux", "riscv64"), None);
}

#[test]
fn a_tauri_style_signature_verifies_and_one_changed_byte_does_not() {
    let key = TestKey::generate();
    let data = b"coverly 9.9.9 binary";
    let sig = key.sign(data, "coverly-9.9.9-darwin-aarch64", Some("9.9.9"));
    assert_eq!(
        verify_signed(data, &sig, &key.public_b64(), "9.9.9"),
        Ok(())
    );
    assert_eq!(
        verify_signed(data, &sig, &key.public_b64(), "v9.9.9"),
        Ok(())
    );
    let mut tampered = data.to_vec();
    tampered[0] ^= 1;
    assert_eq!(
        verify_signed(&tampered, &sig, &key.public_b64(), "9.9.9"),
        Err(VerifyError::BadSignature)
    );
    let other = TestKey::generate();
    assert_eq!(
        verify_signed(data, &sig, &other.public_b64(), "9.9.9"),
        Err(VerifyError::BadSignature)
    );
    assert_eq!(
        verify_signed(data, "%%%", &key.public_b64(), "9.9.9"),
        Err(VerifyError::BadSignature)
    );
    assert_eq!(
        verify_signed(data, &sig, "UNSET", "9.9.9"),
        Err(VerifyError::BadKey)
    );
}

#[test]
fn refuses_a_signature_bound_to_another_version() {
    let key = TestKey::generate();
    let data = b"old but genuine";
    let old = key.sign(data, "coverly", Some("1.0.0"));
    assert_eq!(
        verify_signed(data, &old, &key.public_b64(), "9.9.9"),
        Err(VerifyError::VersionMismatch {
            signed: "1.0.0".into(),
            announced: "9.9.9".into()
        })
    );
    let unbound = key.sign(data, "coverly", None);
    assert_eq!(
        verify_signed(data, &unbound, &key.public_b64(), "9.9.9"),
        Err(VerifyError::MissingVersion)
    );
}

#[test]
fn check_reads_the_manifest_and_compares_versions() {
    let server = MockServer::start();
    server.serve_json(
        "/latest.json",
        &manifest_json("9.9.9", &[("cli-linux-x86_64", "{{BASE}}/bin", "sig")]),
    );
    let url = server.url("/latest.json");
    match check(&http(), &url, "0.0.1").unwrap() {
        Check::Available(m) => {
            assert_eq!(m.version, "9.9.9");
            assert!(m.platforms["cli-linux-x86_64"]
                .url
                .starts_with("http://127.0.0.1"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        check(&http(), &url, "9.9.9").unwrap(),
        Check::UpToDate {
            latest: "9.9.9".into()
        }
    );
    assert_eq!(server.hits("/latest.json"), 2);
    assert!(
        server.requests().iter().all(|r| r.path == "/latest.json"),
        "a check never downloads"
    );
}

#[test]
fn hostile_manifests_are_refused_cleanly() {
    let server = MockServer::start();
    server.mock("/garbage", MockResponse::ok(b"<html>".to_vec()));
    server.mock(
        "/huge",
        MockResponse::ok(vec![b' '; (MAX_MANIFEST_BYTES + 1) as usize]),
    );
    server.serve_json("/nover", r#"{"version":"latest","platforms":{}}"#);
    server.mock("/missing", MockResponse::status(404));
    let h = http();
    assert!(matches!(
        check(&h, &server.url("/garbage"), "0.0.1"),
        Err(UpdateError::BadManifest(_))
    ));
    assert!(matches!(
        check(&h, &server.url("/huge"), "0.0.1"),
        Err(UpdateError::BadManifest(_))
    ));
    assert!(matches!(
        check(&h, &server.url("/nover"), "0.0.1"),
        Err(UpdateError::BadManifest(_))
    ));
    assert!(matches!(
        check(&h, &server.url("/missing"), "0.0.1"),
        Err(UpdateError::NotPublished(_))
    ));
    assert!(matches!(
        check(&h, "http://127.0.0.1:1/latest.json", "0.0.1"),
        Err(UpdateError::Offline(_))
    ));
    let m = parse_manifest(manifest_json("9.9.9", &[]).as_bytes()).unwrap();
    assert!(matches!(
        download_verified(
            &h,
            &m,
            "cli-linux-x86_64",
            &TestKey::generate().public_b64()
        ),
        Err(UpdateError::NoPlatform(_))
    ));
}

#[test]
fn download_verified_returns_only_verified_bytes() {
    let key = TestKey::generate();
    let server = MockServer::start();
    let good = b"#!/bin/sh\necho coverly 9.9.9\n".to_vec();
    let sig = key.sign(&good, "coverly", Some("9.9.9"));
    server.mock("/bin", MockResponse::ok(good.clone()));
    server.mock(
        "/tampered",
        MockResponse::ok(b"#!/bin/sh\necho evil\n".to_vec()),
    );
    let m = parse_manifest(
        manifest_json(
            "9.9.9",
            &[
                ("cli-linux-x86_64", &server.url("/bin"), &sig),
                ("cli-linux-aarch64", &server.url("/tampered"), &sig),
            ],
        )
        .as_bytes(),
    )
    .unwrap();
    let h = http();
    assert_eq!(
        download_verified(&h, &m, "cli-linux-x86_64", &key.public_b64()).unwrap(),
        good
    );
    assert!(matches!(
        download_verified(&h, &m, "cli-linux-aarch64", &key.public_b64()),
        Err(UpdateError::Signature(_))
    ));
}

/// `COVERLY_SIGCHECK_DIR` = a folder with `key.pub`, `file`, `file.sig` from the real Tauri CLI.
#[test]
#[ignore]
fn real_tauri_cli_signatures_verify() {
    let Ok(dir) = std::env::var("COVERLY_SIGCHECK_DIR") else {
        return;
    };
    let dir = std::path::Path::new(&dir);
    let pk = std::fs::read_to_string(dir.join("key.pub")).unwrap();
    let sig = std::fs::read_to_string(dir.join("file.sig")).unwrap();
    let data = std::fs::read(dir.join("file")).unwrap();
    assert_eq!(verify_signed(&data, &sig, pk.trim(), "1.2.3"), Ok(()));
}
