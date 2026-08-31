CREATE TABLE group_features (
    group_id INTEGER NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE,
    feature_code TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (group_id, feature_code)
);

CREATE TABLE runtime_settings (
    setting_key TEXT PRIMARY KEY,
    setting_value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE admin_audit_log (
    audit_id INTEGER PRIMARY KEY,
    operator TEXT NOT NULL,
    action_code TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    before_json TEXT,
    after_json TEXT,
    result TEXT NOT NULL CHECK (result IN ('success', 'failure')),
    created_at INTEGER NOT NULL
);

CREATE INDEX admin_audit_created_at
ON admin_audit_log(created_at DESC);
