DROP TABLE IF EXISTS combat_rounds;
DROP TABLE IF EXISTS combat_participants;
DROP TABLE IF EXISTS combat_records;

CREATE TABLE combat_records (
    combat_id INTEGER PRIMARY KEY,
    combat_type TEXT NOT NULL,
    group_id INTEGER,
    seed TEXT NOT NULL,
    rule_version INTEGER NOT NULL CHECK (rule_version > 0),
    winner_team INTEGER NOT NULL CHECK (winner_team >= 0),
    end_reason TEXT NOT NULL,
    elapsed_ticks INTEGER NOT NULL CHECK (elapsed_ticks > 0),
    snapshot_json TEXT NOT NULL,
    outcome_json TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    finished_at INTEGER NOT NULL
);

CREATE TABLE combat_participants (
    combat_id INTEGER NOT NULL REFERENCES combat_records(combat_id) ON DELETE CASCADE,
    player_id INTEGER NOT NULL REFERENCES players(player_id),
    team INTEGER NOT NULL CHECK (team >= 0),
    combatant_id TEXT NOT NULL,
    system_id TEXT NOT NULL,
    universal_tier INTEGER NOT NULL CHECK (universal_tier BETWEEN 0 AND 8),
    power_before INTEGER NOT NULL CHECK (power_before >= 0),
    hp_before INTEGER NOT NULL CHECK (hp_before > 0),
    hp_after INTEGER NOT NULL CHECK (hp_after >= 0),
    PRIMARY KEY (combat_id, player_id)
);

CREATE TABLE combat_events (
    combat_id INTEGER NOT NULL REFERENCES combat_records(combat_id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    event_json TEXT NOT NULL,
    PRIMARY KEY (combat_id, sequence)
);

CREATE INDEX combat_records_group_time
ON combat_records(group_id, started_at DESC);

CREATE TABLE player_skills (
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    skill_id TEXT NOT NULL,
    mastery INTEGER NOT NULL DEFAULT 0 CHECK (mastery BETWEEN 0 AND 3),
    branch_code TEXT,
    acquired_at INTEGER NOT NULL,
    PRIMARY KEY (player_id, skill_id)
);

CREATE TABLE player_skill_loadouts (
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    slot_type TEXT NOT NULL CHECK (slot_type IN ('active', 'passive', 'domain')),
    slot_index INTEGER NOT NULL CHECK (slot_index >= 0),
    skill_id TEXT NOT NULL,
    PRIMARY KEY (player_id, slot_type, slot_index),
    UNIQUE (player_id, skill_id)
);

CREATE TABLE player_battle_tactics (
    player_id INTEGER PRIMARY KEY REFERENCES players(player_id) ON DELETE CASCADE,
    tactic_code TEXT NOT NULL DEFAULT 'balanced'
        CHECK (tactic_code IN ('balanced', 'aggressive', 'defensive', 'sustain', 'control')),
    updated_at INTEGER NOT NULL
);

CREATE TABLE player_cultivation_actions (
    player_id INTEGER NOT NULL REFERENCES players(player_id) ON DELETE CASCADE,
    action_date TEXT NOT NULL,
    action_code TEXT NOT NULL,
    target_id TEXT,
    result_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (player_id, action_date)
);

ALTER TABLE player_cultivation ADD COLUMN mastery INTEGER NOT NULL DEFAULT 0
    CHECK (mastery >= 0);
ALTER TABLE player_cultivation ADD COLUMN mind_state INTEGER NOT NULL DEFAULT 10000
    CHECK (mind_state BETWEEN 0 AND 10000);
ALTER TABLE player_cultivation ADD COLUMN fatigue INTEGER NOT NULL DEFAULT 0
    CHECK (fatigue BETWEEN 0 AND 10000);
ALTER TABLE player_cultivation ADD COLUMN injury INTEGER NOT NULL DEFAULT 0
    CHECK (injury BETWEEN 0 AND 10000);

DROP TABLE IF EXISTS item_modifiers;
CREATE TABLE item_modifiers (
    item_instance_id INTEGER NOT NULL
        REFERENCES item_instances(item_instance_id) ON DELETE CASCADE,
    modifier_code TEXT NOT NULL,
    modifier_value INTEGER NOT NULL,
    PRIMARY KEY (item_instance_id, modifier_code)
);
