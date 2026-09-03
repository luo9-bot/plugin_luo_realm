CREATE TABLE player_web_tickets (
    nonce TEXT PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK (scope IN ('profile:read')),
    issued_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL CHECK (expires_at > issued_at),
    used_at INTEGER,
    CHECK (used_at IS NULL OR used_at >= issued_at)
);

CREATE INDEX player_web_tickets_expiry ON player_web_tickets(expires_at);
