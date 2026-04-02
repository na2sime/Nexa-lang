use anyhow::{anyhow, Result};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::application::ports::storage::{PackageStore, UserStore};
use crate::domain::{
    package::{Package, PackageVersion},
    user::User,
};

// ── UserStore ────────────────────────────────────────────────────────────────

pub struct PgUserStore {
    pool: PgPool,
}

impl PgUserStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserStore for PgUserStore {
    async fn create(&self, email: &str, password_hash: &str) -> Result<User> {
        let row = sqlx::query!(
            r#"INSERT INTO users (email, password_hash)
               VALUES ($1, $2)
               RETURNING id, email, password_hash, created_at"#,
            email,
            password_hash,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| anyhow!("create user: {e}"))?;

        Ok(User {
            id: row.id,
            email: row.email,
            password_hash: row.password_hash,
            created_at: row.created_at,
        })
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        let row = sqlx::query!(
            r#"SELECT id, email, password_hash, created_at
               FROM users WHERE email = $1"#,
            email,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow!("find user: {e}"))?;

        Ok(row.map(|r| User {
            id: r.id,
            email: r.email,
            password_hash: r.password_hash,
            created_at: r.created_at,
        }))
    }
}

// ── PackageStore ─────────────────────────────────────────────────────────────

pub struct PgPackageStore {
    pool: PgPool,
}

impl PgPackageStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PackageStore for PgPackageStore {
    async fn find_or_create_package(&self, name: &str, owner_id: Uuid) -> Result<Package> {
        let row = sqlx::query!(
            r#"INSERT INTO packages (name, owner_id)
               VALUES ($1, $2)
               ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
               RETURNING id, name, owner_id, created_at"#,
            name,
            owner_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| anyhow!("find_or_create package: {e}"))?;

        Ok(Package {
            id: row.id,
            name: row.name,
            owner_id: row.owner_id,
            created_at: row.created_at,
        })
    }

    async fn find_package(&self, name: &str) -> Result<Option<Package>> {
        let row = sqlx::query!(
            r#"SELECT id, name, owner_id, created_at FROM packages WHERE name = $1"#,
            name,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow!("find package: {e}"))?;

        Ok(row.map(|r| Package {
            id: r.id,
            name: r.name,
            owner_id: r.owner_id,
            created_at: r.created_at,
        }))
    }

    async fn publish_version(
        &self,
        pkg_id: Uuid,
        version: &str,
        bundle: &[u8],
        manifest: &str,
        signature: &str,
    ) -> Result<PackageVersion> {
        let manifest_json: serde_json::Value = serde_json::from_str(manifest)
            .map_err(|e| anyhow!("invalid manifest: {e}"))?;

        let row = sqlx::query!(
            r#"INSERT INTO package_versions (package_id, version, bundle, manifest, signature)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING id, package_id, version, bundle, manifest, signature, published_at"#,
            pkg_id,
            version,
            bundle,
            manifest_json,
            signature,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| anyhow!("publish version: {e}"))?;

        Ok(PackageVersion {
            id: row.id,
            package_id: row.package_id,
            version: row.version,
            bundle: row.bundle,
            manifest: row.manifest.to_string(),
            signature: row.signature,
            published_at: row.published_at,
        })
    }

    async fn get_version(&self, name: &str, version: &str) -> Result<Option<PackageVersion>> {
        let row = sqlx::query!(
            r#"SELECT pv.id, pv.package_id, pv.version, pv.bundle, pv.manifest,
                      pv.signature, pv.published_at
               FROM package_versions pv
               JOIN packages p ON p.id = pv.package_id
               WHERE p.name = $1 AND pv.version = $2"#,
            name,
            version,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow!("get version: {e}"))?;

        Ok(row.map(|r| PackageVersion {
            id: r.id,
            package_id: r.package_id,
            version: r.version,
            bundle: r.bundle,
            manifest: r.manifest.to_string(),
            signature: r.signature,
            published_at: r.published_at,
        }))
    }

    async fn get_latest_version(&self, name: &str) -> Result<Option<PackageVersion>> {
        let row = sqlx::query!(
            r#"SELECT pv.id, pv.package_id, pv.version, pv.bundle, pv.manifest,
                      pv.signature, pv.published_at
               FROM package_versions pv
               JOIN packages p ON p.id = pv.package_id
               WHERE p.name = $1
               ORDER BY pv.published_at DESC
               LIMIT 1"#,
            name,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow!("get latest: {e}"))?;

        Ok(row.map(|r| PackageVersion {
            id: r.id,
            package_id: r.package_id,
            version: r.version,
            bundle: r.bundle,
            manifest: r.manifest.to_string(),
            signature: r.signature,
            published_at: r.published_at,
        }))
    }

    async fn list_versions(&self, name: &str) -> Result<Vec<PackageVersion>> {
        let rows = sqlx::query!(
            r#"SELECT pv.id, pv.package_id, pv.version, pv.bundle, pv.manifest,
                      pv.signature, pv.published_at
               FROM package_versions pv
               JOIN packages p ON p.id = pv.package_id
               WHERE p.name = $1
               ORDER BY pv.published_at DESC"#,
            name,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| anyhow!("list versions: {e}"))?;

        Ok(rows
            .into_iter()
            .map(|r| PackageVersion {
                id: r.id,
                package_id: r.package_id,
                version: r.version,
                bundle: r.bundle,
                manifest: r.manifest.to_string(),
                signature: r.signature,
                published_at: r.published_at,
            })
            .collect())
    }

    async fn search(&self, q: &str, page: i64, per_page: i64) -> Result<Vec<Package>> {
        let pattern = format!("%{q}%");
        let offset = (page - 1).max(0) * per_page;
        let rows = sqlx::query!(
            r#"SELECT id, name, owner_id, created_at FROM packages
               WHERE name ILIKE $1
               ORDER BY name
               LIMIT $2 OFFSET $3"#,
            pattern,
            per_page,
            offset,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| anyhow!("search: {e}"))?;

        Ok(rows
            .into_iter()
            .map(|r| Package {
                id: r.id,
                name: r.name,
                owner_id: r.owner_id,
                created_at: r.created_at,
            })
            .collect())
    }
}
