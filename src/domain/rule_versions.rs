//! 权威规则版本登记处。
//!
//! 每个产生权威随机结果或规则化结果的领域拥有独立的版本序列，写入对应表
//! 的 `rule_version` 列（设计方案书 3.2“随机必须可复现”、34“版本与发布
//! 策略”）。约定：
//!
//! - `v1` 表示“引入版本记录时的现行行为”；
//! - 迁移为历史行补默认值 `1`——这些行正是现行算法产生的，因此按 `v1`
//!   解释与事实一致，旧记录仍然可解释；
//! - 任何算法、定义或权重变化必须递增对应常量；新版本只需保证能按其版本
//!   解释旧版本行，不要求兼容旧算法。
//!
//! 战斗的版本同时冻结在 `CombatSnapshot.rule_version` 中，与
//! `combat_records.rule_version` 一致。

use super::shared::RuleVersion;

/// 战斗运行时与快照公式版本（`combat_records.rule_version`）。
pub const COMBAT: RuleVersion = RuleVersion::INITIAL;

/// 每日状态生成算法版本（`player_daily_states.rule_version`）。
pub const DAILY_STATE: RuleVersion = RuleVersion::INITIAL;

/// 每日机缘选择与记录版本（`destiny_events.rule_version`）。
pub const DESTINY: RuleVersion = RuleVersion::INITIAL;

/// 群世界事件定义与目标算法版本（`group_daily_events.rule_version`）。
pub const WORLD_EVENT: RuleVersion = RuleVersion::INITIAL;

#[cfg(test)]
mod tests;
