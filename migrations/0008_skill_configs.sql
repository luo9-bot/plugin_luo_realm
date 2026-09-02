CREATE TABLE combat_skill_configs (
    skill_id TEXT PRIMARY KEY,
    definition_json TEXT NOT NULL,
    visual_json TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    updated_at INTEGER NOT NULL
);
