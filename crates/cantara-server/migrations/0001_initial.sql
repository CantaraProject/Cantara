-- The whole schema of a Cantara server, as of its first version.
--
-- Every row has a `uuid` as its primary key. The server generates them as
-- UUID v7, which sort by time and therefore keep the indexes compact; nothing
-- reads that order as meaning anything. Everything an organisation owns
-- carries `tenant_uuid`, and every query the server makes names the tenant —
-- see `src/auth.rs` for where that is enforced.

-- ── People ───────────────────────────────────────────────────────────────────

CREATE TABLE users (
    uuid            uuid PRIMARY KEY,
    email           text NOT NULL,
    display_name    text NOT NULL,
    -- argon2id, in PHC string form. NULL for an account that only logs in
    -- through OpenID Connect.
    password_hash   text,
    is_server_admin boolean NOT NULL DEFAULT false,
    disabled        boolean NOT NULL DEFAULT false,
    created_at      timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_email ON users (lower(email));

-- How an account is reached through an OpenID Connect provider.
CREATE TABLE oidc_identities (
    uuid        uuid PRIMARY KEY,
    user_uuid   uuid NOT NULL REFERENCES users ON DELETE CASCADE,
    provider    text NOT NULL,
    subject     text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (provider, subject)
);

-- Browser sessions and the tokens programs are given. Only a hash of the
-- token is kept: a copy of this table logs nobody in.
CREATE TABLE auth_tokens (
    uuid         uuid PRIMARY KEY,
    user_uuid    uuid NOT NULL REFERENCES users ON DELETE CASCADE,
    token_hash   text NOT NULL UNIQUE,
    kind         text NOT NULL CHECK (kind IN ('session', 'api')),
    name         text NOT NULL DEFAULT '',
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz,
    expires_at   timestamptz
);
CREATE INDEX auth_tokens_user ON auth_tokens (user_uuid);

-- Codes of the authorisation flow a desktop program goes through (PKCE).
-- Lived for a minute, used once.
CREATE TABLE auth_codes (
    code_hash      text PRIMARY KEY,
    user_uuid      uuid NOT NULL REFERENCES users ON DELETE CASCADE,
    redirect_uri   text NOT NULL,
    code_challenge text NOT NULL,
    expires_at     timestamptz NOT NULL
);

-- What is remembered between sending somebody to an OpenID Connect provider
-- and their coming back.
CREATE TABLE oidc_states (
    state         text PRIMARY KEY,
    provider      text NOT NULL,
    nonce         text NOT NULL,
    pkce_verifier text NOT NULL,
    next          text NOT NULL,
    expires_at    timestamptz NOT NULL
);

-- ── Organisations ────────────────────────────────────────────────────────────

CREATE TABLE tenants (
    uuid       uuid PRIMARY KEY,
    name       text NOT NULL,
    slug       text NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE memberships (
    tenant_uuid uuid NOT NULL REFERENCES tenants ON DELETE CASCADE,
    user_uuid   uuid NOT NULL REFERENCES users ON DELETE CASCADE,
    joined_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_uuid, user_uuid)
);
CREATE INDEX memberships_user ON memberships (user_uuid);

-- `system` marks the `admins` group every tenant has: whoever is in it may
-- do everything within the tenant.
CREATE TABLE groups (
    uuid        uuid PRIMARY KEY,
    tenant_uuid uuid NOT NULL REFERENCES tenants ON DELETE CASCADE,
    name        text NOT NULL,
    system      boolean NOT NULL DEFAULT false,
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_uuid, name)
);

CREATE TABLE group_members (
    group_uuid uuid NOT NULL REFERENCES groups ON DELETE CASCADE,
    user_uuid  uuid NOT NULL REFERENCES users ON DELETE CASCADE,
    role       text NOT NULL CHECK (role IN ('member', 'manager')),
    PRIMARY KEY (group_uuid, user_uuid)
);
CREATE INDEX group_members_user ON group_members (user_uuid);

CREATE TABLE invitations (
    uuid        uuid PRIMARY KEY,
    tenant_uuid uuid NOT NULL REFERENCES tenants ON DELETE CASCADE,
    email       text NOT NULL,
    group_uuid  uuid REFERENCES groups ON DELETE CASCADE,
    role        text NOT NULL CHECK (role IN ('member', 'manager')),
    token_hash  text NOT NULL UNIQUE,
    invited_by  uuid REFERENCES users ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    expires_at  timestamptz NOT NULL,
    accepted_at timestamptz
);
CREATE INDEX invitations_tenant ON invitations (tenant_uuid);

-- Who may do what with a resource beyond what their membership gives them.
CREATE TABLE grants (
    uuid          uuid PRIMARY KEY,
    tenant_uuid   uuid NOT NULL REFERENCES tenants ON DELETE CASCADE,
    resource_kind text NOT NULL CHECK (resource_kind IN ('library', 'selection', 'settings', 'session')),
    resource_uuid uuid NOT NULL,
    user_uuid     uuid REFERENCES users ON DELETE CASCADE,
    group_uuid    uuid REFERENCES groups ON DELETE CASCADE,
    role          text NOT NULL CHECK (role IN ('read', 'write', 'manage')),
    created_at    timestamptz NOT NULL DEFAULT now(),
    CHECK ((user_uuid IS NULL) <> (group_uuid IS NULL))
);
CREATE INDEX grants_resource ON grants (resource_kind, resource_uuid);
CREATE UNIQUE INDEX grants_user ON grants (resource_kind, resource_uuid, user_uuid) WHERE user_uuid IS NOT NULL;
CREATE UNIQUE INDEX grants_group ON grants (resource_kind, resource_uuid, group_uuid) WHERE group_uuid IS NOT NULL;

-- ── Libraries ────────────────────────────────────────────────────────────────

CREATE TABLE libraries (
    uuid        uuid PRIMARY KEY,
    tenant_uuid uuid NOT NULL REFERENCES tenants ON DELETE CASCADE,
    name        text NOT NULL,
    description text NOT NULL DEFAULT '',
    -- Counts up with every change to the library; an item remembers the
    -- revision it last changed in. That is all a client needs to ask "what
    -- changed since I last looked".
    revision    bigint NOT NULL DEFAULT 0,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX libraries_tenant ON libraries (tenant_uuid);

-- File content, once per content, however many versions share it. The bytes
-- are in the blob store under the hash.
CREATE TABLE blobs (
    sha256     text PRIMARY KEY,
    size       bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- One element of a library across all its versions.
CREATE TABLE items (
    uuid            uuid PRIMARY KEY,
    tenant_uuid     uuid NOT NULL REFERENCES tenants ON DELETE CASCADE,
    library_uuid    uuid NOT NULL REFERENCES libraries ON DELETE CASCADE,
    -- A song's `uuid` tag, or one assigned when the item was first stored.
    lineage_uuid    uuid NOT NULL,
    -- The current version's path, kept here so that two live items cannot
    -- claim the same one.
    relative_path   text NOT NULL,
    current_version uuid,
    revision        bigint NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    deleted_at      timestamptz,
    UNIQUE (library_uuid, lineage_uuid)
);
CREATE UNIQUE INDEX items_live_path ON items (library_uuid, relative_path) WHERE deleted_at IS NULL;
CREATE INDEX items_revision ON items (library_uuid, revision);

-- One version of an item. Never updated: what was stored under a version's
-- uuid stays what is stored under it, which is what makes a client's cache
-- keyed by that uuid correct without ever asking again.
CREATE TABLE item_versions (
    uuid          uuid PRIMARY KEY,
    item_uuid     uuid NOT NULL REFERENCES items ON DELETE CASCADE,
    relative_path text NOT NULL,
    file_name     text NOT NULL,
    kind          text NOT NULL,
    sha256        text NOT NULL REFERENCES blobs,
    md5           text NOT NULL,
    size          bigint NOT NULL,
    title         text NOT NULL,
    tags          jsonb NOT NULL DEFAULT '{}',
    created_by    uuid REFERENCES users ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX item_versions_item ON item_versions (item_uuid);
CREATE INDEX item_versions_sha ON item_versions (sha256);

ALTER TABLE items ADD FOREIGN KEY (current_version) REFERENCES item_versions DEFERRABLE INITIALLY DEFERRED;

-- ── Designs and slide settings ───────────────────────────────────────────────

CREATE TABLE settings_entries (
    uuid            uuid PRIMARY KEY,
    tenant_uuid     uuid NOT NULL REFERENCES tenants ON DELETE CASCADE,
    kind            text NOT NULL CHECK (kind IN ('design', 'slide_settings')),
    lineage_uuid    uuid NOT NULL,
    current_version uuid,
    created_at      timestamptz NOT NULL DEFAULT now(),
    deleted_at      timestamptz,
    UNIQUE (tenant_uuid, lineage_uuid)
);

CREATE TABLE settings_versions (
    uuid        uuid PRIMARY KEY,
    entry_uuid  uuid NOT NULL REFERENCES settings_entries ON DELETE CASCADE,
    name        text NOT NULL,
    body        jsonb NOT NULL,
    created_by  uuid REFERENCES users ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE settings_entries ADD FOREIGN KEY (current_version) REFERENCES settings_versions DEFERRABLE INITIALLY DEFERRED;

-- ── Running orders ───────────────────────────────────────────────────────────

CREATE TABLE selections (
    uuid        uuid PRIMARY KEY,
    tenant_uuid uuid NOT NULL REFERENCES tenants ON DELETE CASCADE,
    owner_uuid  uuid REFERENCES users ON DELETE SET NULL,
    name        text NOT NULL,
    date        date,
    note        text NOT NULL DEFAULT '',
    manifest    jsonb NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX selections_tenant ON selections (tenant_uuid);

-- ── Presentations running on the server ──────────────────────────────────────

CREATE TABLE sessions (
    uuid              uuid PRIMARY KEY,
    tenant_uuid       uuid NOT NULL REFERENCES tenants ON DELETE CASCADE,
    owner_uuid        uuid REFERENCES users ON DELETE SET NULL,
    name              text NOT NULL,
    anonymous_control boolean NOT NULL DEFAULT false,
    -- Capability links: whoever holds one may watch, or control. Kept in the
    -- clear so that the owner can be shown them again; replacing one is how
    -- it is revoked.
    watch_token       text NOT NULL UNIQUE,
    control_token     text NOT NULL UNIQUE,
    state             jsonb,
    revision          bigint NOT NULL DEFAULT 0,
    created_at        timestamptz NOT NULL DEFAULT now(),
    ended_at          timestamptz
);
CREATE INDEX sessions_tenant ON sessions (tenant_uuid);
