CREATE TABLE player_page_sessions (
    token TEXT PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    scope TEXT NOT NULL DEFAULT 'profile:read',
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL CHECK (expires_at > created_at)
);

CREATE INDEX player_page_sessions_expiry ON player_page_sessions(expires_at);
