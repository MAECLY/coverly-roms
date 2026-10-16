//! Throwaway minisign keys for tests, in Tauri's format. Never used by a
//! release build (the `minisign` crate is only in the `test-util` feature).

use base64::Engine as _;

pub struct TestKey {
    pk: minisign::PublicKey,
    sk: minisign::SecretKey,
}

fn b64(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
}

impl TestKey {
    pub fn generate() -> TestKey {
        let minisign::KeyPair { pk, sk } =
            minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
        TestKey { pk, sk }
    }

    /// What `tauri signer generate` prints as the public key.
    pub fn public_b64(&self) -> String {
        b64(&self.pk.to_box().expect("pk box").to_string())
    }

    /// What `tauri signer sign [--app-version V]` writes to the `.sig` file.
    pub fn sign(&self, data: &[u8], file_name: &str, version: Option<&str>) -> String {
        let mut trusted = format!("timestamp:1700000000\tfile:{file_name}");
        if let Some(v) = version {
            trusted.push_str(&format!("\tversion:{v}"));
        }
        let sig = minisign::sign(
            Some(&self.pk),
            &self.sk,
            std::io::Cursor::new(data),
            Some(&trusted),
            Some("signature from tauri secret key"),
        )
        .expect("sign");
        b64(&sig.to_string())
    }
}

/// A `latest.json` with `entries` = `(platform key, url, signature)`.
pub fn manifest_json(version: &str, entries: &[(&str, &str, &str)]) -> String {
    let platforms: serde_json::Map<String, serde_json::Value> = entries
        .iter()
        .map(|(k, url, sig)| {
            (
                (*k).to_string(),
                serde_json::json!({ "url": url, "signature": sig }),
            )
        })
        .collect();
    serde_json::json!({
        "version": version,
        "notes": "Test notes",
        "pub_date": "2026-10-16T00:00:00Z",
        "platforms": platforms,
    })
    .to_string()
}
