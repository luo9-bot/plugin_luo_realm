//! 领域实体标识的新类型。
//!
//! 字符串标识区分体系、技能和战斗单位；数值标识区分平台标识与数据库行
//! 标识。所有标识对 JSON 透明序列化，保持既有持久化格式不变。`P1` 词汇表
//! 中的 `IdentityId`、`WorldId`、`AvatarId` 随对应聚合在 P1-01/02/03 落地，
//! 在此先行固化，避免迁移时临时发明同义类型。

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Deref;

macro_rules! string_identifier {
    ($($(#[$doc:meta])+ $name:ident,)*) => {
        $(
            $(#[$doc])+
            #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
            #[serde(transparent)]
            pub struct $name(String);

            impl $name {
                /// 由内部已验证的字符串构造标识。
                ///
                /// 标识的合法性由产生它的注册表或数据库约束保证；来自外部
                /// 输入的字符串必须先经对应信任边界校验（P2-01、P6-02）。
                pub fn new(value: impl Into<String>) -> Self {
                    Self(value.into())
                }

                /// 返回标识字符串。
                pub fn as_str(&self) -> &str {
                    &self.0
                }
            }

            impl fmt::Display for $name {
                fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    formatter.write_str(&self.0)
                }
            }

            impl Deref for $name {
                type Target = str;

                fn deref(&self) -> &str {
                    &self.0
                }
            }
        )*
    };
}

string_identifier! {
    /// 修行体系标识：小写蛇形命名，由体系注册表保证唯一（如 `sword`）。
    SystemId,
    /// 技能稳定标识：`<体系>.<技能名>`，由技能规则包保证唯一。
    SkillId,
    /// 战斗单位标识：一场战斗内唯一，关联快照、事件流和结算记录。
    CombatantId,
}

/// 平台用户号（QQ 号）。
///
/// 平台身份标识，只用于与平台交互和查找 LR 全局身份；不得当作内部行标识
/// 传播到领域逻辑中。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlatformUserId(u64);

impl PlatformUserId {
    /// 由平台原始数值构造。
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 返回平台原始数值。
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl TryFrom<i64> for PlatformUserId {
    type Error = std::num::TryFromIntError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        u64::try_from(value).map(Self)
    }
}

/// 平台群号（QQ 群号）：一个启用 LR 的群对应一界（`World`）。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupId(u64);

impl GroupId {
    /// 由平台原始数值构造。
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 返回平台原始数值。
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl TryFrom<i64> for GroupId {
    type Error = std::num::TryFromIntError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        u64::try_from(value).map(Self)
    }
}

/// 当前 `players` 表的行标识。
///
/// 历史原因，该列的值恰为平台 QQ 号的 `i64` 表示；这一等价关系是待迁移的
/// 遗留事实，不是设计约定。P1 会引入 [`IdentityId`] 与 [`AvatarId`]，届时
/// 本类型仅作为迁移期行标识使用。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerId(i64);

impl PlayerId {
    /// 构造非负行标识；负值返回 `None`。
    pub const fn new(value: i64) -> Option<Self> {
        if value < 0 { None } else { Some(Self(value)) }
    }

    /// 返回行标识数值。
    pub const fn value(self) -> i64 {
        self.0
    }
}

impl TryFrom<u64> for PlayerId {
    type Error = std::num::TryFromIntError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        i64::try_from(value).map(Self)
    }
}

macro_rules! reserved_row_identifier {
    ($(#[doc = $doc:expr] $name:ident, $work_package:literal;)*) => {
        $(
            #[doc = $doc]
            #[doc = concat!("计划随 ", $work_package, " 引入；当前为保留词汇，不得持久化。")]
            #[derive(
                Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
            )]
            #[serde(transparent)]
            pub struct $name(i64);

            impl $name {
                /// 构造非负标识；负值返回 `None`。
                pub const fn new(value: i64) -> Option<Self> {
                    if value < 0 {
                        None
                    } else {
                        Some(Self(value))
                    }
                }

                /// 返回标识数值。
                pub const fn value(self) -> i64 {
                    self.0
                }
            }
        )*
    };
}

reserved_row_identifier! {
    /// 全局身份标识：与平台用户唯一绑定的 LR 账号。
    IdentityId, "P1-01";
    /// 界标识：一个启用 LR 的群对应的独立世界。
    WorldId, "P1-02";
    /// 本地化身标识：某身份在某一界内成长的角色，唯一键为 `(identity_id, world_id)`。
    AvatarId, "P1-03";
}

#[cfg(test)]
mod tests;
