//! 规则注册表：品阶等“纯数据规则”的外置定义。
//!
//! 品阶只描述自己（代码、显示名、颜色、星数），不关心使用者是什么物品；
//! 卡片渲染、物品详情、网页主题都从这里取色。默认表内置，插件数据目录下
//! 的 `rules/rarities.toml` 可以整体覆盖——新增或调整品阶不需要改代码：
//!
//! ```toml
//! [[tier]]
//! code = "epic"
//! display = "史诗"
//! color = "#8e5cbe"
//! stars = 4
//! ```

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct RarityTier {
    pub code: String,
    pub display: String,
    /// `#RRGGBB` 形式的主题色。
    pub color: String,
    pub stars: u8,
}

impl RarityTier {
    /// 解析颜色为 `(r, g, b)`；非法颜色回退为中性灰。
    pub fn rgb(&self) -> (u8, u8, u8) {
        let hex = self.color.trim_start_matches('#');
        if hex.len() != 6 {
            return (112, 118, 124);
        }
        let red = u8::from_str_radix(&hex[0..2], 16).unwrap_or(112);
        let green = u8::from_str_radix(&hex[2..4], 16).unwrap_or(118);
        let blue = u8::from_str_radix(&hex[4..6], 16).unwrap_or(124);
        (red, green, blue)
    }
}

/// 内置默认品阶，顺序即星级顺序。
pub const BUILTIN_TIERS: &[(&str, &str, &str, u8)] = &[
    ("legacy", "遗留", "#70767c", 1),
    ("common", "普通", "#8a939c", 1),
    ("fine", "精良", "#468a60", 2),
    ("rare", "珍贵", "#4076b0", 3),
    ("epic", "史诗", "#8e5cbe", 4),
    ("legendary", "传奇", "#d69e3e", 5),
];

/// 读取品阶表：优先插件数据目录的 `rules/rarities.toml`，缺失或损坏时回退内置。
pub fn rarity_tiers(plugin_root: &std::path::Path) -> Vec<RarityTier> {
    let path = crate::paths::data_directory(plugin_root)
        .join("rules")
        .join("rarities.toml");
    let Ok(content) = std::fs::read_to_string(path) else {
        return builtin_tiers();
    };
    #[derive(Deserialize)]
    struct File {
        #[serde(rename = "tier")]
        tiers: Vec<RarityTier>,
    }
    match toml::from_str::<File>(&content) {
        Ok(file) if !file.tiers.is_empty() => file.tiers,
        _ => builtin_tiers(),
    }
}

/// 按代码查找品阶。
pub fn rarity_by_code<'a>(tiers: &'a [RarityTier], code: &str) -> Option<&'a RarityTier> {
    tiers.iter().find(|tier| tier.code == code)
}

pub fn builtin_tiers() -> Vec<RarityTier> {
    BUILTIN_TIERS
        .iter()
        .map(|(code, display, color, stars)| RarityTier {
            code: (*code).into(),
            display: (*display).into(),
            color: (*color).into(),
            stars: *stars,
        })
        .collect()
}

#[cfg(test)]
mod tests;
