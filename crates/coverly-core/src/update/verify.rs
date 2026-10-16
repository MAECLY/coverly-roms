//! minisign verification in Tauri's format: the public key and the signature
//! are base64 of minisign's own text; the trusted comment carries
//! `version:<v>` (written by `tauri signer sign --app-version`), which must
//! equal the version the manifest announces (`requireSignedVersion`).

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerifyError {
    #[error("the update key is not a minisign public key")]
    BadKey,
    #[error("the signature does not match the file")]
    BadSignature,
    #[error("the signature is not bound to a version")]
    MissingVersion,
    #[error("the file was signed for {signed}, but the manifest announces {announced}")]
    VersionMismatch { signed: String, announced: String },
}

fn decode_text(b64: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .ok()?;
    String::from_utf8(bytes).ok()
}

/// The `version:` field of a trusted comment (`timestamp:…\tfile:…\tversion:1.2.3`).
pub fn signed_version(trusted_comment: &str) -> Option<&str> {
    trusted_comment
        .split('\t')
        .find_map(|f| f.strip_prefix("version:"))
}

fn bare(v: &str) -> &str {
    v.trim().trim_start_matches(['v', 'V'])
}

/// Verifies `data` against `signature_b64` with `pubkey_b64`, then (only
/// after the signature checked out, so the comment is trusted) that it was
/// signed for `announced_version`.
pub fn verify_signed(
    data: &[u8],
    signature_b64: &str,
    pubkey_b64: &str,
    announced_version: &str,
) -> Result<(), VerifyError> {
    let pk_text = decode_text(pubkey_b64).ok_or(VerifyError::BadKey)?;
    let pk = PublicKey::decode(&pk_text).map_err(|_| VerifyError::BadKey)?;
    let sig_text = decode_text(signature_b64).ok_or(VerifyError::BadSignature)?;
    let sig = Signature::decode(&sig_text).map_err(|_| VerifyError::BadSignature)?;
    pk.verify(data, &sig, true)
        .map_err(|_| VerifyError::BadSignature)?;
    match signed_version(sig.trusted_comment()) {
        None => Err(VerifyError::MissingVersion),
        Some(v) if bare(v) == bare(announced_version) => Ok(()),
        Some(v) => Err(VerifyError::VersionMismatch {
            signed: bare(v).to_string(),
            announced: bare(announced_version).to_string(),
        }),
    }
}
