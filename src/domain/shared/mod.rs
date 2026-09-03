//! 跨领域共享的值类型：数量与标识。
//!
//! 数值类型（[`quantity`]）统一基点、规则版本和战力的整数表达；标识类型
//! （[`identifiers`]）区分平台标识、行标识和领域实体标识。所有新类型对
//! JSON 使用透明序列化，保证既有持久化格式不变。

pub mod identifiers;
pub mod quantity;

pub use identifiers::{
    AvatarId, CombatantId, GroupId, IdentityId, PlatformUserId, PlayerId, SkillId, SystemId,
    WorldId,
};
pub use quantity::{BasisPoints, PowerScore, RuleVersion};
