use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Package {
    pub id: Uuid,
    pub name: String,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PackageVersion {
    pub id: Uuid,
    pub package_id: Uuid,
    pub version: String,
    /// Raw .nexa ZIP bytes
    pub bundle: Vec<u8>,
    /// manifest.json content
    pub manifest: String,
    pub signature: String,
    pub published_at: DateTime<Utc>,
}
