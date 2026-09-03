ALTER TABLE player_daily_states ADD COLUMN rule_version INTEGER NOT NULL DEFAULT 1
    CHECK (rule_version > 0);
ALTER TABLE group_daily_events ADD COLUMN rule_version INTEGER NOT NULL DEFAULT 1
    CHECK (rule_version > 0);
ALTER TABLE destiny_events ADD COLUMN rule_version INTEGER NOT NULL DEFAULT 1
    CHECK (rule_version > 0);
