//! Pins the webview's file access (part D D12): the asset protocol is on
//! but its static scope allows nothing (each Extras file is granted one by
//! one after canonical checks), key names and dotfiles are denied as a
//! second fence, the CSP is pinned word for word, no fs plugin and no
//! opener permissions for JS.

use serde_json::Value;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::utils::config::AssetProtocolConfig;
use tauri::Manager;

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn conf() -> Value {
    json(include_str!("../tauri.conf.json"))
}

/// The CSP of D12: media only through the asset protocol, workers from
/// the app, no eval, no plugins, no frames, no `<base>`.
const CSP: &str = "default-src 'self'; \
img-src 'self' data: blob: asset: http://asset.localhost; \
media-src 'self' asset: http://asset.localhost; \
style-src 'self' 'unsafe-inline'; \
font-src 'self' data:; \
connect-src 'self' ipc: http://ipc.localhost asset: http://asset.localhost; \
worker-src 'self'; \
object-src 'none'; \
frame-src 'none'; \
base-uri 'none'";

/// Key and licence names (D12) and dotfiles.
const DENY: [&str; 13] = [
    "**/*.rap",
    "**/*.rif",
    "**/*.edat",
    "**/act.dat",
    "**/*.tik",
    "**/ticket.db",
    "**/seeddb.bin",
    "**/*.keys",
    "**/keys.txt",
    "**/otp.bin",
    "**/kv.bin",
    "**/work.bin",
    "**/.*",
];

#[test]
fn asset_protocol_allows_nothing_statically_and_denies_keys_and_dotfiles() {
    let conf = conf();
    let asset = &conf["app"]["security"]["assetProtocol"];
    assert_eq!(asset["enable"], Value::Bool(true), "{asset}");
    assert_eq!(asset["scope"]["allow"], serde_json::json!([]), "{asset}");
    assert_eq!(asset["scope"]["deny"], serde_json::json!(DENY), "{asset}");
    // Nothing else (no `requireLiteralLeadingDot` override, no other key).
    assert_eq!(asset.as_object().unwrap().len(), 2, "{asset}");
    assert_eq!(asset["scope"].as_object().unwrap().len(), 2, "{asset}");
}

#[test]
fn csp_is_pinned() {
    let conf = conf();
    let csp = conf["app"]["security"]["csp"].as_str().unwrap();
    assert_eq!(csp, CSP);
    assert!(!csp.contains("unsafe-eval"), "{csp}");
    assert!(
        !csp.split_whitespace()
            .any(|s| s == "*" || s == "http:" || s == "https:"),
        "{csp}"
    );
    assert!(conf["app"]["security"]
        .get("dangerousDisableAssetCspModification")
        .is_none());
    assert!(conf["app"]["security"].get("freezePrototype") != Some(&Value::Bool(false)));
}

#[test]
fn capabilities_grant_only_core_and_the_folder_dialog() {
    let cap = json(include_str!("../capabilities/default.json"));
    assert_eq!(
        cap["permissions"],
        serde_json::json!(["core:default", "dialog:allow-open"])
    );
}

/// The scope Tauri builds from the shipped config, in a mock app.
fn shipped_scope() -> (
    tauri::App<tauri::test::MockRuntime>,
    tauri::scope::fs::Scope,
) {
    let mut ctx = mock_context(noop_assets());
    let asset: AssetProtocolConfig =
        serde_json::from_value(conf()["app"]["security"]["assetProtocol"].clone()).unwrap();
    ctx.config_mut().app.security.asset_protocol = asset;
    let app = mock_builder().build(ctx).expect("mock app");
    let scope = app.asset_protocol_scope();
    (app, scope)
}

#[test]
fn the_shipped_scope_serves_nothing_until_a_file_is_granted() {
    // No dot in any component: unix glob `**` does not cross dot folders.
    let dir = tempfile::Builder::new()
        .prefix("coverly-scope-")
        .tempdir()
        .unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let song = base.join("01 Opening.flac");
    std::fs::write(&song, b"fLaC").unwrap();
    let (_app, scope) = shipped_scope();
    assert!(!scope.is_allowed(&song), "nothing is allowed statically");
    scope.allow_file(&song).unwrap();
    assert!(scope.is_allowed(&song));
    assert!(!scope.is_allowed(base.join("Game.cia")), "one file only");
}

#[test]
fn the_deny_globs_win_over_a_grant() {
    let dir = tempfile::Builder::new()
        .prefix("coverly-scope-")
        .tempdir()
        .unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let (_app, scope) = shipped_scope();
    for name in [
        "game.rap",
        "license.rif",
        "data.edat",
        "act.dat",
        "title.tik",
        "ticket.db",
        "seeddb.bin",
        "prod.keys",
        "keys.txt",
        "otp.bin",
        "kv.bin",
        "work.bin",
        ".hidden.png",
        "._front.png",
    ] {
        let p = base.join(name);
        std::fs::write(&p, b"x").unwrap();
        scope.allow_file(&p).unwrap();
        assert!(!scope.is_allowed(&p), "{name} must stay denied");
    }
}

#[test]
fn a_grant_is_literal_even_with_glob_characters_in_the_name() {
    let dir = tempfile::Builder::new()
        .prefix("coverly-scope-")
        .tempdir()
        .unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let (_app, scope) = shipped_scope();
    // Dump names carry brackets: `[Remix]` must not act as a glob class.
    let granted = base.join("Boss [Remix] (v1.1).flac");
    let sibling = base.join("Boss R (v1.1).flac");
    std::fs::write(&granted, b"fLaC").unwrap();
    std::fs::write(&sibling, b"fLaC").unwrap();
    scope.allow_file(&granted).unwrap();
    assert!(scope.is_allowed(&granted));
    assert!(!scope.is_allowed(&sibling), "a bracket is not a glob class");
}
