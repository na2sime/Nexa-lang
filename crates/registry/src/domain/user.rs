use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
    /// Base64-encoded Ed25519 public key; set on first `nexa publish`.
    pub signing_key: Option<String>,
}
