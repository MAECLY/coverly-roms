//! The real `PluginDriver` against loopback servers (mock runtime, no real
//! network): a server that never answers must not hang a check.

use std::io::Read;
use std::net::TcpListener;
use std::time::{Duration, Instant};

use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::update::*;
use tauri::test::{mock_builder, mock_context, noop_assets};

fn mock_handle() -> tauri::AppHandle<tauri::test::MockRuntime> {
    let mut ctx = mock_context(noop_assets());
    ctx.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({ "pubkey": "unused", "endpoints": [] }),
    );
    let app = mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(ctx)
        .expect("mock app");
    app.handle().clone()
}

#[test]
fn a_server_that_never_answers_reads_as_offline_within_the_bound() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // Accept and read the request, then never answer (captive portal, stall).
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            held.push(stream);
        }
    });
    let driver = PluginDriver::new(mock_handle())
        .with_timeouts(Duration::from_secs(1), Duration::from_secs(1));
    let started = Instant::now();
    let got = driver.check(&format!("http://127.0.0.1:{port}/latest.json"), "unused");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "check hung for {:?}",
        started.elapsed()
    );
    assert!(matches!(got, Err(IpcError::UpdateOffline(_))), "{got:?}");
}

#[test]
fn the_app_path_always_has_bounded_requests() {
    assert!(CHECK_TIMEOUT > Duration::ZERO && CHECK_TIMEOUT <= Duration::from_secs(60));
    assert!(DOWNLOAD_TIMEOUT >= CHECK_TIMEOUT);
    assert!(DOWNLOAD_TIMEOUT <= Duration::from_secs(60 * 60));
}

#[test]
fn huge_release_notes_are_cut_before_they_reach_the_webview() {
    let notes = "é".repeat(10 * 1024 * 1024); // 20 MiB
    let view = offer_view(
        "9.9.9",
        Some(&notes),
        &serde_json::json!({"pub_date": "2026-10-16T00:00:00Z"}),
    );
    let cut = view.notes.unwrap();
    assert!(cut.len() <= MAX_NOTES_BYTES, "{}", cut.len());
    assert!(cut.starts_with("éé"));
    assert_eq!(view.date.as_deref(), Some("2026-10-16T00:00:00Z"));
    let small = offer_view("9.9.9", Some("New"), &serde_json::json!({}));
    assert_eq!(small.notes.as_deref(), Some("New"));
    assert_eq!(small.date, None);
}
