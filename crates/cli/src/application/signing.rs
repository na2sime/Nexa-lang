//! Ed25519 signing keypair management for `nexa publish`.
//!
//! The 32-byte seed is stored at `~/.nexa/signing_key` (raw bytes, mode 0600).
//! The public key is derived from it on every call — no separate public key file.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

fn signing_key_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".nexa").join("signing_key")
}

/// Load the signing key from disk, generating and persisting a new one if absent.
pub fn load_or_generate() -> SigningKey {
    let path = signing_key_path();
    if let Ok(bytes) = fs::read(&path) {
        if bytes.len() == 32 {
            let arr: [u8; 32] = bytes.try_into().expect("32 bytes");
            return SigningKey::from_bytes(&arr);
        }
    }
    // Generate a new keypair
    let mut rng = rand::thread_rng();
    use ed25519_dalek::SigningKey as SK;
    use rand::RngCore;
    let mut seed = [0u8; 32];
    rng.fill_bytes(&mut seed);
    let key = SK::from_bytes(&seed);
    // Persist
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::write(&path, key.to_bytes()) {
        eprintln!(
            "warning: could not save signing key to {}: {e}",
            path.display()
        );
    }
    // Set restrictive permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    key
}

/// Returns the base64-encoded Ed25519 public key.
pub fn public_key_b64(key: &SigningKey) -> String {
    let vk: VerifyingKey = key.verifying_key();
    B64.encode(vk.as_bytes())
}

/// Sign `SHA-256(data)` with the given key. Returns base64-encoded signature.
pub fn sign_bundle(key: &SigningKey, data: &[u8]) -> String {
    use ed25519_dalek::Signer;
    let hash = {
        let mut h = Sha256::new();
        h.update(data);
        h.finalize()
    };
    let sig = key.sign(&hash);
    B64.encode(sig.to_bytes())
}
