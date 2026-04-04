use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use crate::application::ports::storage::{TokenStore, UserStore};
use crate::domain::token::ApiToken;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String, // user_id as string
    exp: i64,    // unix timestamp
}

pub struct AuthService {
    user_store: Arc<dyn UserStore>,
    token_store: Arc<dyn TokenStore>,
    jwt_secret: String,
}

impl AuthService {
    pub fn new(
        user_store: Arc<dyn UserStore>,
        token_store: Arc<dyn TokenStore>,
        jwt_secret: String,
    ) -> Self {
        Self {
            user_store,
            token_store,
            jwt_secret,
        }
    }

    pub async fn register(&self, email: &str, password: &str) -> Result<String> {
        if self.user_store.find_by_email(email).await?.is_some() {
            return Err(anyhow!("email already registered"));
        }
        let hash =
            bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(|e| anyhow!("hash error: {e}"))?;
        let user = self.user_store.create(email, &hash).await?;
        self.make_jwt(user.id)
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<String> {
        let user = self
            .user_store
            .find_by_email(email)
            .await?
            .ok_or_else(|| anyhow!("invalid email or password"))?;

        let valid = bcrypt::verify(password, &user.password_hash)
            .map_err(|e| anyhow!("verify error: {e}"))?;
        if !valid {
            return Err(anyhow!("invalid email or password"));
        }
        self.make_jwt(user.id)
    }

    /// Verify either a JWT session token or a permanent API token (`nxt_…`).
    /// Returns the authenticated user's UUID.
    pub async fn verify_token(&self, token: &str) -> Result<Uuid> {
        if token.starts_with("nxt_") {
            return self.verify_api_token(token).await;
        }
        self.verify_jwt(token)
    }

    // ── API token management ─────────────────────────────────────────────────

    /// Generate a new permanent API token for `user_id`.
    /// Returns `(raw_token, ApiToken)` — the raw value is shown only once.
    pub async fn create_api_token(&self, user_id: Uuid, name: &str) -> Result<(String, ApiToken)> {
        let raw = generate_token();
        let hash = hash_token(&raw);
        let record = self.token_store.create(user_id, name, &hash).await?;
        Ok((raw, record))
    }

    pub async fn list_api_tokens(&self, user_id: Uuid) -> Result<Vec<ApiToken>> {
        self.token_store.list_for_user(user_id).await
    }

    pub async fn revoke_api_token(&self, token_id: Uuid, user_id: Uuid) -> Result<bool> {
        self.token_store.delete(token_id, user_id).await
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    fn verify_jwt(&self, token: &str) -> Result<Uuid> {
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )
        .map_err(|e| anyhow!("invalid token: {e}"))?;
        Uuid::parse_str(&data.claims.sub).map_err(|e| anyhow!("invalid token subject: {e}"))
    }

    async fn verify_api_token(&self, raw: &str) -> Result<Uuid> {
        let hash = hash_token(raw);
        let record = self
            .token_store
            .find_by_hash(&hash)
            .await?
            .ok_or_else(|| anyhow!("invalid token"))?;
        Ok(record.user_id)
    }

    fn make_jwt(&self, user_id: Uuid) -> Result<String> {
        let exp = (Utc::now() + Duration::hours(24)).timestamp();
        let claims = Claims {
            sub: user_id.to_string(),
            exp,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )
        .map_err(|e| anyhow!("encode error: {e}"))
    }
}

// ── Token generation / hashing ────────────────────────────────────────────────

/// Generate a random `nxt_<64 hex chars>` token (32 random bytes).
fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("nxt_{}", hex::encode(bytes))
}

/// SHA-256 hash of the raw token, hex-encoded.
fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{token::ApiToken, user::User};
    use anyhow::Result;
    use async_trait::async_trait;
    use std::sync::Mutex;

    // ── In-memory UserStore ──────────────────────────────────────────────────

    #[derive(Default)]
    struct MemUserStore {
        users: Mutex<Vec<User>>,
    }

    #[async_trait]
    impl UserStore for MemUserStore {
        async fn create(&self, email: &str, password_hash: &str) -> Result<User> {
            let mut users = self.users.lock().unwrap();
            if users.iter().any(|u| u.email == email) {
                return Err(anyhow::anyhow!("email already registered"));
            }
            let user = User {
                id: Uuid::new_v4(),
                email: email.to_string(),
                password_hash: password_hash.to_string(),
                created_at: Utc::now(),
                signing_key: None,
            };
            users.push(user.clone());
            Ok(user)
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

    // ── In-memory TokenStore ─────────────────────────────────────────────────

    #[derive(Default)]
    struct MemTokenStore {
        tokens: Mutex<Vec<ApiToken>>,
    }

    #[async_trait]
    impl TokenStore for MemTokenStore {
        async fn create(&self, user_id: Uuid, name: &str, token_hash: &str) -> Result<ApiToken> {
            let t = ApiToken {
                id: Uuid::new_v4(),
                user_id,
                name: name.to_string(),
                token_hash: token_hash.to_string(),
                created_at: Utc::now(),
                last_used_at: None,
            };
            self.tokens.lock().unwrap().push(t.clone());
            Ok(t)
        }

        async fn find_by_hash(&self, token_hash: &str) -> Result<Option<ApiToken>> {
            let mut tokens = self.tokens.lock().unwrap();
            if let Some(t) = tokens.iter_mut().find(|t| t.token_hash == token_hash) {
                t.last_used_at = Some(Utc::now());
                return Ok(Some(t.clone()));
            }
            Ok(None)
        }

        async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<ApiToken>> {
            Ok(self
                .tokens
                .lock()
                .unwrap()
                .iter()
                .filter(|t| t.user_id == user_id)
                .cloned()
                .collect())
        }

        async fn delete(&self, id: Uuid, user_id: Uuid) -> Result<bool> {
            let mut tokens = self.tokens.lock().unwrap();
            let before = tokens.len();
            tokens.retain(|t| !(t.id == id && t.user_id == user_id));
            Ok(tokens.len() < before)
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn make_service() -> AuthService {
        AuthService::new(
            Arc::new(MemUserStore::default()),
            Arc::new(MemTokenStore::default()),
            "test-secret-32-chars-long-enough!".to_string(),
        )
    }

    // ── AuthService tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn register_returns_jwt() {
        let svc = make_service();
        let token = svc
            .register("alice@example.com", "password123")
            .await
            .unwrap();
        assert!(token.len() > 20, "token should be non-trivially long");
        // JWT starts with 'e' (base64url of {"alg":"HS256",...})
        assert!(
            !token.starts_with("nxt_"),
            "register should return a JWT, not an API token"
        );
    }

    #[tokio::test]
    async fn register_duplicate_email_fails() {
        let svc = make_service();
        svc.register("alice@example.com", "pass").await.unwrap();
        let err = svc
            .register("alice@example.com", "other")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("already registered"));
    }

    #[tokio::test]
    async fn login_valid_credentials_returns_jwt() {
        let svc = make_service();
        svc.register("bob@example.com", "correcthorsebattery")
            .await
            .unwrap();
        let token = svc
            .login("bob@example.com", "correcthorsebattery")
            .await
            .unwrap();
        assert!(!token.is_empty());
    }

    #[tokio::test]
    async fn login_wrong_password_fails() {
        let svc = make_service();
        svc.register("carol@example.com", "right").await.unwrap();
        let err = svc.login("carol@example.com", "wrong").await.unwrap_err();
        assert!(err.to_string().contains("invalid"));
    }

    #[tokio::test]
    async fn login_unknown_email_fails() {
        let svc = make_service();
        let err = svc.login("nobody@example.com", "pass").await.unwrap_err();
        assert!(err.to_string().contains("invalid"));
    }

    #[tokio::test]
    async fn verify_jwt_round_trip() {
        let svc = make_service();
        let token = svc.register("dave@example.com", "pass").await.unwrap();
        let user_id = svc.verify_token(&token).await.unwrap();
        // Registered user exists and JWT decodes to a valid UUID
        assert!(!user_id.is_nil());
    }

    #[tokio::test]
    async fn verify_invalid_jwt_fails() {
        let svc = make_service();
        let err = svc.verify_token("not.a.real.jwt").await.unwrap_err();
        assert!(err.to_string().contains("invalid token"));
    }

    #[tokio::test]
    async fn api_token_create_list_revoke() {
        let svc = make_service();
        let jwt = svc.register("eve@example.com", "pass").await.unwrap();
        let user_id = svc.verify_token(&jwt).await.unwrap();

        // Create
        let (raw, record) = svc.create_api_token(user_id, "ci-token").await.unwrap();
        assert!(raw.starts_with("nxt_"));
        assert_eq!(record.name, "ci-token");
        assert_eq!(record.user_id, user_id);

        // Verify via API token
        let verified = svc.verify_token(&raw).await.unwrap();
        assert_eq!(verified, user_id);

        // List
        let tokens = svc.list_api_tokens(user_id).await.unwrap();
        assert_eq!(tokens.len(), 1);

        // Revoke
        let deleted = svc.revoke_api_token(record.id, user_id).await.unwrap();
        assert!(deleted);
        let err = svc.verify_token(&raw).await.unwrap_err();
        assert!(err.to_string().contains("invalid token"));

        // List after revoke
        let tokens = svc.list_api_tokens(user_id).await.unwrap();
        assert!(tokens.is_empty());
    }

    #[tokio::test]
    async fn revoke_other_users_token_fails() {
        let svc = make_service();
        let jwt1 = svc.register("frank@example.com", "pass").await.unwrap();
        let jwt2 = svc.register("grace@example.com", "pass").await.unwrap();
        let uid1 = svc.verify_token(&jwt1).await.unwrap();
        let uid2 = svc.verify_token(&jwt2).await.unwrap();

        let (_, record) = svc.create_api_token(uid1, "my-token").await.unwrap();
        // uid2 tries to revoke uid1's token
        let deleted = svc.revoke_api_token(record.id, uid2).await.unwrap();
        assert!(
            !deleted,
            "should not be able to revoke another user's token"
        );
    }
}
