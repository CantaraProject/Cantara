-- Logging in a device that has no browser of its own (RFC 8628), and the
-- pictures and documents a shared presentation shows.

-- A device asks, shows the user code, and polls with the device code until
-- somebody logged in somewhere else has typed that user code in and said yes.
CREATE TABLE device_codes (
    device_code_hash text PRIMARY KEY,
    user_code        text NOT NULL UNIQUE,
    device_name      text NOT NULL DEFAULT '',
    -- Set when somebody approves; `denied` when somebody refuses.
    user_uuid        uuid REFERENCES users ON DELETE CASCADE,
    denied           boolean NOT NULL DEFAULT false,
    last_poll        timestamptz,
    expires_at       timestamptz NOT NULL
);

-- What a shared presentation's slides name by a path on the presenting
-- computer, uploaded so that a viewer elsewhere can see it. The bytes are
-- blobs like any other; the rows go when the session does.
CREATE TABLE session_media (
    session_uuid uuid NOT NULL REFERENCES sessions ON DELETE CASCADE,
    path         text NOT NULL,
    sha256       text NOT NULL REFERENCES blobs,
    file_name    text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (session_uuid, path)
);
