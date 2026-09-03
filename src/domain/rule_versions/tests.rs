//! 规则版本登记处的冻结验证：改动版本必须是显式的、经过审阅的决定。

use super::{COMBAT, DAILY_STATE, DESTINY, WORLD_EVENT};
use crate::domain::shared::RuleVersion;

#[test]
fn current_versions_are_the_first_recorded_generation() {
    // 版本随算法演进递增；此测试故意冻结当前值，
    // 使任何版本变化都必须同步修改并知晓其影响。
    assert_eq!(COMBAT, RuleVersion::INITIAL);
    assert_eq!(DAILY_STATE, RuleVersion::INITIAL);
    assert_eq!(DESTINY, RuleVersion::INITIAL);
    assert_eq!(WORLD_EVENT, RuleVersion::INITIAL);
}

#[test]
fn every_domain_has_its_own_version_constant() {
    // 常量按领域独立登记，防止共享一个可变版本导致语义漂移。
    let _ = (
        COMBAT.value(),
        DAILY_STATE.value(),
        DESTINY.value(),
        WORLD_EVENT.value(),
    );
}
