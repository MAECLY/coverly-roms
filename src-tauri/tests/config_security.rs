//! Pins the webview's file access: no asset protocol, no fs plugin, no
//! opener permissions for JS, art only as blob URLs.

use serde_json::Value;

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn asset_protocol_stays_disabled_and_csp_allows_only_blob_images() {
    let conf = json(include_str!("../tauri.conf.json"));
    let security = &conf["app"]["security"];
    assert!(
        security.get("assetProtocol").is_none()
            || security["assetProtocol"]["enable"] == Value::Bool(false),
        "asset protocol must stay disabled: {security}"
    );
    let csp = security["csp"].as_str().unwrap();
    let img_src = csp
        .split(';')
        .map(str::trim)
        .find(|d| d.starts_with("img-src"))
        .unwrap();
    assert_eq!(img_src, "img-src 'self' data: blob:");
}

#[test]
fn capabilities_grant_only_core_and_the_folder_dialog() {
    let cap = json(include_str!("../capabilities/default.json"));
    assert_eq!(
        cap["permissions"],
        serde_json::json!(["core:default", "dialog:allow-open"])
    );
}
