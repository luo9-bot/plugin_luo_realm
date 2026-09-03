pub(crate) mod assets;
pub(crate) mod battle;
pub mod card;
mod profile;

use std::{io, path::Path};

pub struct ProfileRenderData<'a> {
    pub player: &'a crate::core::Player,
    pub system_id: &'a str,
    pub system_name: &'a str,
    pub realm_name: &'a str,
    pub progress: u64,
    pub power: f64,
}

pub fn profile(root: &Path, data: &ProfileRenderData<'_>, path: &Path) -> io::Result<()> {
    profile::render(root, data, path)
}

pub fn battle(
    root: &Path,
    snapshot: &crate::combat::CombatSnapshot,
    outcome: &crate::combat::CombatOutcome,
    path: &Path,
) -> io::Result<()> {
    battle::render(root, snapshot, outcome, path)
}

/// 读取角色卡头像样式；配置缺失或损坏时回落默认（圆形 + 等比裁剪）。
pub(crate) fn portrait_style(
    root: &Path,
) -> (crate::config::PortraitShape, crate::config::PortraitFill) {
    crate::config::RuntimeConfig::load(root)
        .map(|config| {
            let card = config.profile_card;
            (card.portrait_shape, card.portrait_fill)
        })
        .unwrap_or_default()
}

pub use card::{
    BagItemView, EquipmentCardData, EquippedSlotView, ItemDetailData, SkillCardData,
    SystemCardEntry,
};

pub fn menu(root: &Path, path: &Path) -> io::Result<()> {
    card::menu(root, path)
}

pub fn systems(root: &Path, entries: &[SystemCardEntry], path: &Path) -> io::Result<()> {
    card::systems(root, entries, path)
}

pub fn skills(root: &Path, data: &SkillCardData<'_>, path: &Path) -> io::Result<()> {
    card::skills(root, data, path)
}

pub fn equipment(root: &Path, data: &EquipmentCardData<'_>, path: &Path) -> io::Result<()> {
    card::equipment(root, data, path)
}

pub fn item_detail(root: &Path, data: &ItemDetailData<'_>, path: &Path) -> io::Result<()> {
    card::item_detail(root, data, path)
}

pub struct DestinyCardData<'a> {
    pub destiny_name: &'a str,
    pub description: &'a str,
    pub world_event_line: Option<&'a str>,
}

pub fn destiny(root: &Path, data: &DestinyCardData<'_>, path: &Path) -> io::Result<()> {
    card::destiny(
        root,
        &card::DestinyCardData {
            destiny_name: data.destiny_name,
            description: data.description,
            world_event_line: data.world_event_line,
        },
        path,
    )
}

pub struct WorldEventCardData<'a> {
    pub event_name: &'a str,
    pub description: &'a str,
    pub status: &'a str,
    pub completed: bool,
    pub coin_reward: i64,
    pub mark_reward: i64,
    pub objectives: &'a [(String, i64, i64)],
}

pub fn world_event(root: &Path, data: &WorldEventCardData<'_>, path: &Path) -> io::Result<()> {
    card::world_event(
        root,
        &card::WorldEventCardData {
            event_name: data.event_name,
            description: data.description,
            status: data.status,
            completed: data.completed,
            coin_reward: data.coin_reward,
            mark_reward: data.mark_reward,
            objectives: data.objectives,
        },
        path,
    )
}
