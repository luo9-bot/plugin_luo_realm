//! 权威数值的整数新类型。
//!
//! 百分比使用基点、版本使用非零整数、战力使用非负整数。这些类型替代裸
//! `i64`/`f64` 进入领域接口（设计方案书 8.1），对 JSON 透明序列化以保持
//! 既有持久化格式。

use serde::{Deserialize, Serialize};

/// 基点：`10_000` 表示 100%。
///
/// [`scale`](Self::saturating_scale) 使用饱和运算并保持与战斗运行时一致的
/// “先乘后除、向零取整”次序，防止极端输入改变结果符号或溢出回绕。
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct BasisPoints(i64);

impl BasisPoints {
    /// 100% 对应的基点数。
    pub const SCALE: i64 = 10_000;

    /// 零基点。
    pub const ZERO: Self = Self(0);

    /// 100%。
    pub const HUNDRED_PERCENT: Self = Self(Self::SCALE);

    /// 构造非负基点；负值返回 `None`。
    pub const fn new(value: i64) -> Option<Self> {
        if value < 0 { None } else { Some(Self(value)) }
    }

    /// 构造允许符号的基点，用于表示增减量。
    pub const fn signed(value: i64) -> Self {
        Self(value)
    }

    /// 返回原始基点值。
    pub const fn value(self) -> i64 {
        self.0
    }

    /// 按 `self / 100%` 的比例缩放 `amount`，结果向零取整。
    pub fn saturating_scale(self, amount: i64) -> i64 {
        let scaled = (self.0 as i128 * amount as i128) / Self::SCALE as i128;
        scaled.clamp(i64::MIN as i128, i64::MAX as i128) as i64
    }
}

/// 规则版本：权威规则与随机过程的不可变版本号。
///
/// 写入数据库或快照后，同一版本号的含义不得改变；规则变化必须递增版本。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuleVersion(u32);

impl RuleVersion {
    /// 首个规则版本。
    pub const INITIAL: Self = Self(1);

    /// 构造正数版本号；`0` 无效并返回 `None`。
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// 返回原始版本号。
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// 战力评分：只用于比较与匹配，不得进入伤害公式（设计方案书 11.3）。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PowerScore(i64);

impl PowerScore {
    /// 构造非负战力；负输入饱和为 `0`。
    ///
    /// 饱和只防御不可能出现的内部计算错误，不改变正常路径的数值。
    pub const fn saturating_new(value: i64) -> Self {
        Self(if value < 0 { 0 } else { value })
    }

    /// 返回原始战力值。
    pub const fn value(self) -> i64 {
        self.0
    }
}

#[cfg(test)]
mod tests;
