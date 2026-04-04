use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use crate::application::ports::storage::{PackageStore, UserStore};
use crate::domain::package::{Package, PackageVersion};

pub struct PackagesService {
    store: Arc<dyn PackageStore>,
    user_store: Arc<dyn UserStore>,
}

impl PackagesService {
    pub fn new(store: Arc<dyn PackageStore>, user_store: Arc<dyn UserStore>) -> Self {
        Self { store, user_store }
    }

    /// Publish a `.nexa` bundle.
    ///
    /// `ed25519_pubkey_b64`  — base64 Ed25519 public key of the publisher
    /// `ed25519_sig_b64`     — base64 Ed25519 signature of `SHA-256(bundle_bytes)`
    ///
    /// On first publish the public key is stored for the user.
    /// On subsequent publishes the stored key must match the provided one.
    pub async fn publish(
        &self,
        name: &str,
        owner_id: Uuid,
        bundle_bytes: Vec<u8>,
        ed25519_pubkey_b64: &str,
        ed25519_sig_b64: &str,
    ) -> Result<PackageVersion> {
        // ── 1. Verify Ed25519 signature ──────────────────────────────────────
        let pubkey_bytes = B64
            .decode(ed25519_pubkey_b64)
            .map_err(|_| anyhow!("invalid base64 public key"))?;
        let pubkey_arr: [u8; 32] = pubkey_bytes
            .try_into()
            .map_err(|_| anyhow!("public key must be 32 bytes"))?;
        let verifying_key = VerifyingKey::from_bytes(&pubkey_arr)
            .map_err(|e| anyhow!("invalid public key: {e}"))?;

        let sig_bytes = B64
            .decode(ed25519_sig_b64)
            .map_err(|_| anyhow!("invalid base64 signature"))?;
        let sig_arr: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| anyhow!("signature must be 64 bytes"))?;
        let signature = Signature::from_bytes(&sig_arr);

        // The signed message is SHA-256(bundle_bytes)
        let bundle_hash = {
            let mut h = Sha256::new();
            h.update(&bundle_bytes);
            h.finalize()
        };
        verifying_key
            .verify_strict(&bundle_hash, &signature)
            .map_err(|_| anyhow!("Ed25519 signature verification failed"))?;

        // ── 2. Store or verify the user's signing key ────────────────────────
        let user = self
            .user_store
            .find_by_id(owner_id)
            .await?
            .ok_or_else(|| anyhow!("user not found"))?;

        match &user.signing_key {
            None => {
                // First publish — persist the public key
                self.user_store
                    .set_signing_key(owner_id, ed25519_pubkey_b64)
                    .await?;
            }
            Some(stored) if stored != ed25519_pubkey_b64 => {
                return Err(anyhow!(
                    "signing key does not match the key registered for this account"
                ));
            }
            _ => {} // stored key matches
        }

        // ── 3. Validate bundle integrity (SHA-256 inside the ZIP) ────────────
        let (manifest, signature_sig) = extract_manifest_and_sig(&bundle_bytes)?;
        let nxb = extract_nxb(&bundle_bytes)?;
        let mut hasher = Sha256::new();
        hasher.update(&nxb);
        hasher.update(manifest.as_bytes());
        let computed = format!("{:x}", hasher.finalize());
        if computed != signature_sig.trim() {
            return Err(anyhow!("bundle integrity check failed"));
        }

        // ── 4. Store ─────────────────────────────────────────────────────────
        let version = parse_version(&manifest)?;
        let pkg = self.store.find_or_create_package(name, owner_id).await?;
        if self.store.get_version(name, &version).await?.is_some() {
            return Err(anyhow!("version {version} already published for {name}"));
        }
        self.store
            .publish_version(pkg.id, &version, &bundle_bytes, &manifest, &signature_sig)
            .await
    }

    pub async fn get_package(&self, name: &str) -> Result<Option<Package>> {
        self.store.find_package(name).await
    }

    pub async fn list_versions(&self, name: &str) -> Result<Vec<PackageVersion>> {
        self.store.list_versions(name).await
    }

    pub async fn download(&self, name: &str, version: &str) -> Result<Option<PackageVersion>> {
        if version == "latest" {
            self.store.get_latest_version(name).await
        } else {
            self.store.get_version(name, version).await
        }
    }

    pub async fn search(&self, q: &str, page: i64, per_page: i64) -> Result<Vec<Package>> {
        self.store.search(q, page, per_page).await
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn extract_nxb(bundle: &[u8]) -> Result<Vec<u8>> {
    extract_zip_entry(bundle, "app.nxb")
}

fn extract_manifest_and_sig(bundle: &[u8]) -> Result<(String, String)> {
    let manifest_bytes = extract_zip_entry(bundle, "manifest.json")?;
    let sig_bytes = extract_zip_entry(bundle, "signature.sig")?;
    let manifest = String::from_utf8(manifest_bytes)
        .map_err(|_| anyhow!("manifest.json is not valid UTF-8"))?;
    let signature =
        String::from_utf8(sig_bytes).map_err(|_| anyhow!("signature.sig is not valid UTF-8"))?;
    Ok((manifest, signature))
}

fn extract_zip_entry(bundle: &[u8], name: &str) -> Result<Vec<u8>> {
    use std::io::{Cursor, Read};
    let cursor = Cursor::new(bundle);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| anyhow!("invalid ZIP: {e}"))?;
    let mut entry = archive
        .by_name(name)
        .map_err(|_| anyhow!("bundle missing '{name}'"))?;
    let mut buf = Vec::new();
    entry
        .read_to_end(&mut buf)
        .map_err(|e| anyhow!("read '{name}': {e}"))?;
    Ok(buf)
}

fn parse_version(manifest: &str) -> Result<String> {
    let v: serde_json::Value =
        serde_json::from_str(manifest).map_err(|e| anyhow!("invalid manifest JSON: {e}"))?;
    v["version"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("manifest missing 'version' field"))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        package::{Package, PackageVersion},
        user::User,
    };
    use anyhow::Result;
    use async_trait::async_trait;
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use chrono::Utc;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::RngCore;
    use std::sync::Mutex;

    // ── In-memory UserStore ──────────────────────────────────────────────────

    #[derive(Default)]
    struct MemUserStore {
        users: Mutex<Vec<User>>,
    }

    #[async_trait]
    impl crate::application::ports::storage::UserStore for MemUserStore {
        async fn create(&self, email: &str, password_hash: &str) -> Result<User> {
            let u = User {
                id: Uuid::new_v4(),
                email: email.to_string(),
                password_hash: password_hash.to_string(),
                created_at: Utc::now(),
                signing_key: None,
            };
            self.users.lock().unwrap().push(u.clone());
            Ok(u)
        }
        async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.email == email)
                .cloned())
        }
        async fn find_by_id(&self, id: Uuid) -> Result<Option<User>> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.id == id)
                .cloned())
        }
        async fn set_signing_key(&self, id: Uuid, pubkey: &str) -> Result<()> {
            let mut users = self.users.lock().unwrap();
            if let Some(u) = users.iter_mut().find(|u| u.id == id) {
                u.signing_key = Some(pubkey.to_string());
            }
            Ok(())
        }
    }

    // ── In-memory PackageStore ───────────────────────────────────────────────

    #[derive(Default)]
    struct MemPackageStore {
        packages: Mutex<Vec<Package>>,
        versions: Mutex<Vec<PackageVersion>>,
    }

    #[async_trait]
    impl PackageStore for MemPackageStore {
        async fn find_or_create_package(&self, name: &str, owner_id: Uuid) -> Result<Package> {
            let mut pkgs = self.packages.lock().unwrap();
            if let Some(p) = pkgs.iter().find(|p| p.name == name) {
                return Ok(p.clone());
            }
            let p = Package {
                id: Uuid::new_v4(),
                name: name.to_string(),
                owner_id,
                created_at: Utc::now(),
            };
            pkgs.push(p.clone());
            Ok(p)
        }
        async fn find_package(&self, name: &str) -> Result<Option<Package>> {
            Ok(self
                .packages
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.name == name)
                .cloned())
        }
        async fn publish_version(
            &self,
            pkg_id: Uuid,
            version: &str,
            bundle: &[u8],
            manifest: &str,
            signature: &str,
        ) -> Result<PackageVersion> {
            let v = PackageVersion {
                id: Uuid::new_v4(),
                package_id: pkg_id,
                version: version.to_string(),
                bundle: bundle.to_vec(),
                manifest: manifest.to_string(),
                signature: signature.to_string(),
                published_at: Utc::now(),
            };
            self.versions.lock().unwrap().push(v.clone());
            Ok(v)
        }
        async fn get_version(&self, name: &str, version: &str) -> Result<Option<PackageVersion>> {
            let pkgs = self.packages.lock().unwrap();
            let pkg_id = match pkgs.iter().find(|p| p.name == name) {
                Some(p) => p.id,
                None => return Ok(None),
            };
            Ok(self
                .versions
                .lock()
                .unwrap()
                .iter()
                .find(|v| v.package_id == pkg_id && v.version == version)
                .cloned())
        }
        async fn get_latest_version(&self, name: &str) -> Result<Option<PackageVersion>> {
            let pkgs = self.packages.lock().unwrap();
            let pkg_id = match pkgs.iter().find(|p| p.name == name) {
                Some(p) => p.id,
                None => return Ok(None),
            };
            Ok(self
                .versions
                .lock()
                .unwrap()
                .iter()
                .filter(|v| v.package_id == pkg_id)
                .max_by_key(|v| v.published_at)
                .cloned())
        }
        async fn list_versions(&self, name: &str) -> Result<Vec<PackageVersion>> {
            let pkgs = self.packages.lock().unwrap();
            let pkg_id = match pkgs.iter().find(|p| p.name == name) {
                Some(p) => p.id,
                None => return Ok(vec![]),
            };
            Ok(self
                .versions
                .lock()
                .unwrap()
                .iter()
                .filter(|v| v.package_id == pkg_id)
                .cloned()
                .collect())
        }
        async fn search(&self, q: &str, _page: i64, _per_page: i64) -> Result<Vec<Package>> {
            Ok(self
                .packages
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p.name.contains(q))
                .cloned()
                .collect())
        }
    }

    // ── Bundle / keypair helpers ─────────────────────────────────────────────

    fn make_signing_key() -> SigningKey {
        let mut seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut seed);
        SigningKey::from_bytes(&seed)
    }

    fn pubkey_b64(key: &SigningKey) -> String {
        B64.encode(key.verifying_key().as_bytes())
    }

    fn sign_bundle(key: &SigningKey, bundle: &[u8]) -> String {
        let hash = {
            let mut h = Sha256::new();
            h.update(bundle);
            h.finalize()
        };
        B64.encode(key.sign(&hash).to_bytes())
    }

    /// Build a minimal valid .nexa ZIP bundle.
    fn make_bundle(name: &str, version: &str) -> Vec<u8> {
        use std::io::Write as _;
        let nxb = b"NXB\x01fakebinary";
        let manifest = format!(
            r#"{{"name":"{name}","version":"{version}","nexa_version":"0.1.0","nxb_version":1,"created_at":0}}"#
        );
        // Compute SHA-256(nxb || manifest)
        let sig = {
            let mut h = Sha256::new();
            h.update(nxb);
            h.update(manifest.as_bytes());
            format!("{:x}", h.finalize())
        };

        let buf = Vec::new();
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(buf));
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        zip.start_file("app.nxb", opts).unwrap();
        zip.write_all(nxb).unwrap();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.start_file("signature.sig", opts).unwrap();
        zip.write_all(sig.as_bytes()).unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn make_service_with_user(user_id: Uuid) -> (PackagesService, Arc<MemUserStore>) {
        let user_store = Arc::new(MemUserStore::default());
        user_store.users.lock().unwrap().push(User {
            id: user_id,
            email: "publisher@example.com".to_string(),
            password_hash: "hash".to_string(),
            created_at: Utc::now(),
            signing_key: None,
        });
        let svc = PackagesService::new(Arc::new(MemPackageStore::default()), user_store.clone());
        (svc, user_store)
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn publish_valid_bundle_succeeds() {
        let user_id = Uuid::new_v4();
        let (svc, _) = make_service_with_user(user_id);
        let key = make_signing_key();
        let bundle = make_bundle("my-pkg", "0.1.0");
        let pv = svc
            .publish(
                "my-pkg",
                user_id,
                bundle.clone(),
                &pubkey_b64(&key),
                &sign_bundle(&key, &bundle),
            )
            .await
            .unwrap();
        assert_eq!(pv.version, "0.1.0");
    }

    #[tokio::test]
    async fn publish_stores_signing_key_on_first_publish() {
        let user_id = Uuid::new_v4();
        let (svc, user_store) = make_service_with_user(user_id);
        let key = make_signing_key();
        let bundle = make_bundle("my-pkg", "0.1.0");
        svc.publish(
            "my-pkg",
            user_id,
            bundle.clone(),
            &pubkey_b64(&key),
            &sign_bundle(&key, &bundle),
        )
        .await
        .unwrap();
        let stored = user_store
            .users
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.id == user_id)
            .and_then(|u| u.signing_key.clone())
            .unwrap();
        assert_eq!(stored, pubkey_b64(&key));
    }

    #[tokio::test]
    async fn publish_wrong_ed25519_sig_fails() {
        let user_id = Uuid::new_v4();
        let (svc, _) = make_service_with_user(user_id);
        let key = make_signing_key();
        let wrong_key = make_signing_key();
        let bundle = make_bundle("my-pkg", "0.1.0");
        // Sign with wrong_key but provide correct pubkey
        let err = svc
            .publish(
                "my-pkg",
                user_id,
                bundle.clone(),
                &pubkey_b64(&key),
                &sign_bundle(&wrong_key, &bundle),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Ed25519"));
    }

    #[tokio::test]
    async fn publish_key_mismatch_fails() {
        let user_id = Uuid::new_v4();
        let (svc, _) = make_service_with_user(user_id);
        let key1 = make_signing_key();
        let key2 = make_signing_key();

        // First publish with key1
        let b1 = make_bundle("my-pkg", "0.1.0");
        svc.publish(
            "my-pkg",
            user_id,
            b1.clone(),
            &pubkey_b64(&key1),
            &sign_bundle(&key1, &b1),
        )
        .await
        .unwrap();

        // Second publish with different key2 → should fail
        let b2 = make_bundle("my-pkg", "0.2.0");
        let err = svc
            .publish(
                "my-pkg",
                user_id,
                b2.clone(),
                &pubkey_b64(&key2),
                &sign_bundle(&key2, &b2),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("signing key does not match"));
    }

    #[tokio::test]
    async fn publish_duplicate_version_fails() {
        let user_id = Uuid::new_v4();
        let (svc, _) = make_service_with_user(user_id);
        let key = make_signing_key();
        let bundle = make_bundle("my-pkg", "1.0.0");
        svc.publish(
            "my-pkg",
            user_id,
            bundle.clone(),
            &pubkey_b64(&key),
            &sign_bundle(&key, &bundle),
        )
        .await
        .unwrap();
        let err = svc
            .publish(
                "my-pkg",
                user_id,
                bundle.clone(),
                &pubkey_b64(&key),
                &sign_bundle(&key, &bundle),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("already published"));
    }

    #[tokio::test]
    async fn publish_tampered_bundle_fails() {
        let user_id = Uuid::new_v4();
        let (svc, _) = make_service_with_user(user_id);
        let key = make_signing_key();
        let mut bundle = make_bundle("my-pkg", "0.1.0");
        // Tamper: flip last byte — breaks the integrity SHA-256
        let last = bundle.last_mut().unwrap();
        *last ^= 0xff;
        // Re-sign the tampered bundle so Ed25519 passes, but SHA-256 inside ZIP fails
        let err = svc
            .publish(
                "my-pkg",
                user_id,
                bundle.clone(),
                &pubkey_b64(&key),
                &sign_bundle(&key, &bundle),
            )
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("integrity")
                || err.to_string().contains("ZIP")
                || err.to_string().contains("signature"),
            "unexpected error: {err}"
        );
    }
}
