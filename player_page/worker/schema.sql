-- Cloudflare D1：玩家页面快照与会话。由 `wrangler d1 execute` 应用。
CREATE TABLE IF NOT EXISTS player_state (
    token       TEXT PRIMARY KEY,
    player_id   INTEGER NOT NULL,
    state_json  TEXT NOT NULL,
    expires_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS player_state_expiry ON player_state(expires_at);
