ALTER TABLE groups ADD COLUMN battle_report_mode TEXT NOT NULL DEFAULT 'inherit'
    CHECK (battle_report_mode IN ('inherit', 'enabled', 'disabled'));

CREATE TABLE player_daily_states (
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    state_date TEXT NOT NULL,
    state_id TEXT NOT NULL,
    state_name TEXT NOT NULL,
    description TEXT NOT NULL,
    hp_modifier REAL NOT NULL, attack_modifier REAL NOT NULL,
    defense_modifier REAL NOT NULL, speed_modifier REAL NOT NULL,
    critical_modifier REAL NOT NULL, destiny_modifier REAL NOT NULL,
    source_json TEXT NOT NULL, seed TEXT NOT NULL, created_at INTEGER NOT NULL,
    PRIMARY KEY (player_id, state_date)
);

CREATE TABLE group_daily_events (
    group_id INTEGER NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE,
    event_date TEXT NOT NULL, definition_id TEXT NOT NULL, event_name TEXT NOT NULL,
    description TEXT NOT NULL, coin_reward INTEGER NOT NULL, mark_reward INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','completed')),
    seed TEXT NOT NULL, completed_at INTEGER, created_at INTEGER NOT NULL,
    PRIMARY KEY (group_id, event_date)
);

CREATE TABLE group_event_objectives (
    group_id INTEGER NOT NULL, event_date TEXT NOT NULL, objective_id TEXT NOT NULL,
    objective_type TEXT NOT NULL, objective_label TEXT NOT NULL,
    target_value INTEGER NOT NULL, current_value INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (group_id, event_date, objective_id),
    FOREIGN KEY (group_id, event_date) REFERENCES group_daily_events(group_id, event_date) ON DELETE CASCADE
);

CREATE TABLE group_event_contributions (
    group_id INTEGER NOT NULL, event_date TEXT NOT NULL, player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    contribution_type TEXT NOT NULL, contribution_value INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL,
    PRIMARY KEY (group_id, event_date, player_id, contribution_type),
    FOREIGN KEY (group_id, event_date) REFERENCES group_daily_events(group_id, event_date) ON DELETE CASCADE
);

CREATE TABLE group_event_rewards (
    group_id INTEGER NOT NULL, event_date TEXT NOT NULL, player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    reward_code TEXT NOT NULL, transaction_id INTEGER, created_at INTEGER NOT NULL,
    PRIMARY KEY (group_id, event_date, player_id, reward_code),
    FOREIGN KEY (group_id, event_date) REFERENCES group_daily_events(group_id, event_date) ON DELETE CASCADE
);
