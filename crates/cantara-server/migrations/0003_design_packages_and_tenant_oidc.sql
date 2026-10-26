-- A shared design with what it needs to look right elsewhere, and OpenID
-- Connect providers an organisation brings itself.

-- The design as a `.cantara-design.zip` — the design, its background picture
-- and its font files — beside the version it belongs to. What another
-- computer takes over, so that the fonts arrive with the design.
ALTER TABLE settings_versions ADD COLUMN package_sha256 text REFERENCES blobs;

-- A provider of one tenant. Its people log in through it, and — where
-- `auto_join` is set — become members of the tenant the first time they do,
-- which is what an organisation with its own directory of people wants.
CREATE TABLE tenant_oidc_providers (
    uuid          uuid PRIMARY KEY,
    tenant_uuid   uuid NOT NULL REFERENCES tenants ON DELETE CASCADE,
    -- Short, for the URL: `/auth/oidc/t/{tenant slug}/{slug}`.
    slug          text NOT NULL,
    name          text NOT NULL,
    issuer        text NOT NULL,
    client_id     text NOT NULL,
    client_secret text,
    auto_join     boolean NOT NULL DEFAULT false,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_uuid, slug)
);
