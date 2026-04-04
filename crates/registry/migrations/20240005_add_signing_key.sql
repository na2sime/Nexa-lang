-- Add Ed25519 public key storage for package signing.
-- Populated on the user's first nexa publish; immutable after that.
ALTER TABLE users ADD COLUMN signing_key TEXT;
