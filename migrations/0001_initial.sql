CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL
);

CREATE TABLE players (
    player_id INTEGER PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'disabled', 'deleted')),
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE player_profiles (
    player_id INTEGER PRIMARY KEY REFERENCES players(player_id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    avatar_id TEXT NOT NULL DEFAULT 'default',
    selected_title TEXT,
    biography TEXT NOT NULL DEFAULT ''
);

CREATE TABLE player_balances (
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    currency_code TEXT NOT NULL,
    amount INTEGER NOT NULL DEFAULT 0 CHECK (amount >= 0),
    PRIMARY KEY (player_id, currency_code)
);

CREATE TABLE wallet_transactions (
    transaction_id INTEGER PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(player_id),
    currency_code TEXT NOT NULL,
    delta INTEGER NOT NULL CHECK (delta != 0),
    balance_after INTEGER NOT NULL CHECK (balance_after >= 0),
    reason_code TEXT NOT NULL,
    reference_type TEXT,
    reference_id TEXT,
    idempotency_key TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL
);

CREATE INDEX wallet_transactions_player_time
ON wallet_transactions(player_id, created_at DESC);

CREATE TABLE player_cultivation (
    player_id INTEGER PRIMARY KEY REFERENCES players(player_id) ON DELETE CASCADE,
    system_id TEXT NOT NULL,
    realm_index INTEGER NOT NULL DEFAULT 0 CHECK (realm_index >= 0),
    realm_stage INTEGER NOT NULL DEFAULT 0 CHECK (realm_stage >= 0),
    progress INTEGER NOT NULL DEFAULT 0 CHECK (progress >= 0),
    foundation INTEGER NOT NULL DEFAULT 0 CHECK (foundation >= 0),
    comprehension INTEGER NOT NULL DEFAULT 0 CHECK (comprehension >= 0),
    deviation INTEGER NOT NULL DEFAULT 0 CHECK (deviation >= 0),
    updated_at INTEGER NOT NULL
);

CREATE TABLE breakthrough_history (
    breakthrough_id INTEGER PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(player_id),
    system_id TEXT NOT NULL,
    source_realm INTEGER NOT NULL,
    target_realm INTEGER NOT NULL,
    success INTEGER NOT NULL CHECK (success IN (0, 1)),
    probability REAL NOT NULL CHECK (probability >= 0 AND probability <= 1),
    seed TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE item_instances (
    item_instance_id INTEGER PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    definition_id TEXT NOT NULL,
    quantity INTEGER NOT NULL DEFAULT 1 CHECK (quantity > 0),
    quality TEXT NOT NULL,
    level INTEGER NOT NULL DEFAULT 0 CHECK (level >= 0),
    experience INTEGER NOT NULL DEFAULT 0 CHECK (experience >= 0),
    durability INTEGER CHECK (durability IS NULL OR durability >= 0),
    bound INTEGER NOT NULL DEFAULT 0 CHECK (bound IN (0, 1)),
    created_at INTEGER NOT NULL
);

CREATE TABLE item_modifiers (
    item_instance_id INTEGER NOT NULL
        REFERENCES item_instances(item_instance_id) ON DELETE CASCADE,
    modifier_code TEXT NOT NULL,
    modifier_value REAL NOT NULL,
    PRIMARY KEY (item_instance_id, modifier_code)
);

CREATE TABLE inventory_slots (
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    slot_index INTEGER NOT NULL CHECK (slot_index >= 0),
    item_instance_id INTEGER NOT NULL UNIQUE
        REFERENCES item_instances(item_instance_id) ON DELETE CASCADE,
    PRIMARY KEY (player_id, slot_index)
);

CREATE TABLE equipment_loadouts (
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    slot_code TEXT NOT NULL,
    item_instance_id INTEGER NOT NULL UNIQUE
        REFERENCES item_instances(item_instance_id) ON DELETE CASCADE,
    PRIMARY KEY (player_id, slot_code)
);

CREATE TABLE player_destinies (
    destiny_id INTEGER PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    definition_id TEXT NOT NULL,
    grade TEXT NOT NULL,
    state TEXT NOT NULL,
    acquired_at INTEGER NOT NULL,
    expires_at INTEGER,
    source_seed TEXT NOT NULL
);

CREATE TABLE destiny_events (
    event_id INTEGER PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    event_date TEXT NOT NULL,
    definition_id TEXT NOT NULL,
    choice_code TEXT,
    outcome_code TEXT,
    seed TEXT NOT NULL,
    UNIQUE (player_id, event_date, definition_id)
);

CREATE TABLE daily_checkins (
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    checkin_date TEXT NOT NULL,
    streak INTEGER NOT NULL CHECK (streak > 0),
    reward_transaction_id INTEGER REFERENCES wallet_transactions(transaction_id),
    PRIMARY KEY (player_id, checkin_date)
);

CREATE TABLE player_cooldowns (
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    cooldown_code TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY (player_id, cooldown_code)
);

CREATE TABLE player_statistics (
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    metric_code TEXT NOT NULL,
    metric_value INTEGER NOT NULL DEFAULT 0 CHECK (metric_value >= 0),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (player_id, metric_code)
);

CREATE TABLE combat_records (
    combat_id INTEGER PRIMARY KEY,
    combat_type TEXT NOT NULL,
    group_id INTEGER,
    seed TEXT NOT NULL,
    winner_player_id INTEGER REFERENCES players(player_id),
    rounds INTEGER NOT NULL CHECK (rounds > 0),
    started_at INTEGER NOT NULL,
    finished_at INTEGER NOT NULL
);

CREATE TABLE combat_participants (
    combat_id INTEGER NOT NULL REFERENCES combat_records(combat_id) ON DELETE CASCADE,
    player_id INTEGER NOT NULL REFERENCES players(player_id),
    side INTEGER NOT NULL,
    system_id TEXT NOT NULL,
    realm_index INTEGER NOT NULL,
    power_before INTEGER NOT NULL,
    hp_before INTEGER NOT NULL,
    hp_after INTEGER NOT NULL,
    reward_summary_json TEXT,
    PRIMARY KEY (combat_id, player_id)
);

CREATE TABLE combat_rounds (
    combat_id INTEGER NOT NULL REFERENCES combat_records(combat_id) ON DELETE CASCADE,
    round_index INTEGER NOT NULL CHECK (round_index > 0),
    frame_json TEXT NOT NULL,
    PRIMARY KEY (combat_id, round_index)
);

CREATE INDEX combat_records_group_time
ON combat_records(group_id, started_at DESC);

CREATE TABLE groups (
    group_id INTEGER PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE group_bosses (
    group_id INTEGER NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE,
    boss_instance_id TEXT NOT NULL,
    definition_id TEXT NOT NULL,
    current_hp INTEGER NOT NULL CHECK (current_hp >= 0),
    max_hp INTEGER NOT NULL CHECK (max_hp > 0),
    state TEXT NOT NULL,
    spawned_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY (group_id, boss_instance_id)
);

CREATE TABLE migration_runs (
    run_id INTEGER PRIMARY KEY,
    source_root TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    finished_at INTEGER,
    report_json TEXT
);

CREATE TABLE migration_source_files (
    run_id INTEGER NOT NULL REFERENCES migration_runs(run_id) ON DELETE CASCADE,
    source_file TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    PRIMARY KEY (run_id, source_file)
);

CREATE TABLE migration_source_values (
    source_value_id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES migration_runs(run_id) ON DELETE CASCADE,
    source_file TEXT NOT NULL,
    source_section TEXT NOT NULL,
    source_key TEXT NOT NULL,
    raw_value TEXT NOT NULL,
    normalized_value TEXT,
    mapping_status TEXT NOT NULL CHECK (
        mapping_status IN (
            'mapped', 'ignored_empty', 'static_definition', 'unmapped', 'invalid'
        )
    ),
    target_table TEXT,
    target_key TEXT,
    error_message TEXT
);
