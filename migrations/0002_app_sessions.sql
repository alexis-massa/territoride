-- Login sessions of the Rust app (pi-common auth). The Python version used Django's signed sessions
-- (django_session), which cannot be read here: players log in with Strava once more after the cutover.
-- Rollback: DROP TABLE app_sessions;

CREATE TABLE app_sessions (
    token_hash varchar(64) PRIMARY KEY,
    user_id bigint NOT NULL REFERENCES accounts_user (id) ON DELETE CASCADE,
    csrf_token varchar(64) NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);
CREATE INDEX app_sessions_user_id_idx ON app_sessions (user_id);
