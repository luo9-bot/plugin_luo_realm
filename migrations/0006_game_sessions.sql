CREATE TABLE game_session_issuance (
    player_id INTEGER PRIMARY KEY REFERENCES players(player_id) ON DELETE CASCADE,
    last_issued_at INTEGER NOT NULL
);
