CREATE TABLE game_voucher_redemptions (
    voucher_nonce TEXT PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(player_id),
    game_id TEXT NOT NULL,
    score INTEGER NOT NULL CHECK (score >= 0),
    reward_amount INTEGER NOT NULL CHECK (reward_amount BETWEEN 1 AND 1000),
    redemption_date TEXT NOT NULL,
    reward_transaction_id INTEGER NOT NULL UNIQUE REFERENCES wallet_transactions(transaction_id),
    issued_at INTEGER NOT NULL,
    redeemed_at INTEGER NOT NULL
);

CREATE INDEX game_voucher_redemptions_player_date
ON game_voucher_redemptions(player_id, game_id, redemption_date);
