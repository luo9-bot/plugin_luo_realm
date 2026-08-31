ALTER TABLE players ADD COLUMN registration_state TEXT NOT NULL DEFAULT 'active'
    CHECK (registration_state IN ('pending_system', 'active'));

CREATE INDEX players_registration_state
ON players(registration_state, status);
