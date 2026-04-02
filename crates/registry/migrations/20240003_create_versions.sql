CREATE TABLE package_versions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    package_id   UUID NOT NULL REFERENCES packages(id),
    version      TEXT NOT NULL,
    bundle       BYTEA NOT NULL,
    manifest     JSONB NOT NULL,
    signature    TEXT NOT NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(package_id, version)
);
