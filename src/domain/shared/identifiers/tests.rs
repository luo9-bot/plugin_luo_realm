//! 标识类型的单元测试。

use super::{
    AvatarId, CombatantId, GroupId, IdentityId, PlatformUserId, PlayerId, SkillId, SystemId,
    WorldId,
};

#[test]
fn string_identifiers_display_and_deref_to_str() {
    let system = SystemId::new("sword");
    assert_eq!(system.as_str(), "sword");
    assert_eq!(system.to_string(), "sword");
    assert_eq!(system.len(), 5);

    let skill = SkillId::new("sword.basic");
    assert!(skill.starts_with("sword"));
    assert_eq!(skill.as_str(), "sword.basic");

    let combatant = CombatantId::new("10001");
    assert_eq!(combatant.as_str(), "10001");
}

#[test]
fn identifiers_serialize_transparently() {
    // 透明序列化保证既有 JSON 持久化格式不变。
    assert_eq!(
        serde_json::to_string(&SystemId::new("sword")).expect("system id json"),
        r#""sword""#
    );
    assert_eq!(
        serde_json::to_string(&PlatformUserId::new(42)).expect("platform user id json"),
        "42"
    );
    let restored: CombatantId = serde_json::from_str(r#""10001""#).expect("combatant id");
    assert_eq!(restored.as_str(), "10001");
}

#[test]
fn platform_and_group_identifiers_roundtrip() {
    let user = PlatformUserId::new(12_345);
    assert_eq!(user.value(), 12_345);
    assert_eq!(PlatformUserId::try_from(12_345_i64), Ok(user));

    let group = GroupId::new(987_654);
    assert_eq!(group.value(), 987_654);
    assert_eq!(GroupId::try_from(group.value() as i64), Ok(group));
}

#[test]
fn row_identifiers_reject_negative_values() {
    assert_eq!(PlayerId::new(1), Some(PlayerId::new(1).expect("positive")));
    assert_eq!(PlayerId::new(-1), None);
    assert_eq!(IdentityId::new(0).map(IdentityId::value), Some(0));
    assert_eq!(WorldId::new(-5), None);
    assert_eq!(AvatarId::new(-5), None);
}

#[test]
fn platform_identifiers_reject_out_of_i64_range() {
    assert!(PlatformUserId::try_from(-1_i64).is_err());
    assert!(GroupId::try_from(-1_i64).is_err());
    assert!(PlayerId::try_from(u64::MAX).is_err());
}
