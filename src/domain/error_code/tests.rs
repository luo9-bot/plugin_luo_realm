//! 稳定错误码契约的验证：码值一经发布即冻结。

use super::StableErrorCode;
use crate::combat::CombatError;
use crate::database::DatabaseError;
use crate::domain::shared::CombatantId;

#[test]
fn combat_error_codes_are_stable() {
    let duplicate = CombatError::DuplicateCombatant(CombatantId::new("A"));
    let empty = CombatError::EmptyLoadout(CombatantId::new("A"));
    assert_eq!(
        CombatError::MissingTeams.error_code(),
        "combat.missing_teams"
    );
    assert_eq!(duplicate.error_code(), "combat.duplicate_combatant");
    assert_eq!(empty.error_code(), "combat.empty_loadout");
    assert_eq!(
        CombatError::InvalidRules("x".into()).error_code(),
        "combat.invalid_rules"
    );
    assert_eq!(
        CombatError::InvalidState("x".into()).error_code(),
        "combat.invalid_state"
    );
}

#[test]
fn database_error_codes_are_stable() {
    let unavailable = DatabaseError::Unavailable(rusqlite::Error::InvalidColumnName("x".into()));
    let constraint = DatabaseError::Constraint(rusqlite::Error::InvalidColumnName("x".into()));
    assert_eq!(unavailable.error_code(), "database.unavailable");
    assert_eq!(DatabaseError::Busy.error_code(), "database.busy");
    assert_eq!(constraint.error_code(), "database.constraint_violation");
    assert_eq!(DatabaseError::NotFound.error_code(), "database.not_found");
    assert_eq!(
        DatabaseError::InsufficientBalance.error_code(),
        "database.insufficient_balance"
    );
    assert_eq!(
        DatabaseError::Corrupt("x".into()).error_code(),
        "database.corrupt"
    );
    assert_eq!(
        DatabaseError::Migration("x".into()).error_code(),
        "database.migration_failed"
    );
    assert_eq!(
        DatabaseError::InvalidIdentifier.error_code(),
        "database.invalid_identifier"
    );
    assert_eq!(
        DatabaseError::InvalidData("x".into()).error_code(),
        "database.invalid_data"
    );
}
