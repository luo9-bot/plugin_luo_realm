pub(crate) mod assets;
mod battle;
mod profile;

use std::{io, path::Path};

use crate::{core::CombatResult, engine::CombatProfile};

pub struct ProfileRenderData<'a> {
    pub player: &'a crate::core::Player,
    pub system_id: &'a str,
    pub system_name: &'a str,
    pub realm_name: &'a str,
    pub realm_index: u32,
    pub progress: u64,
    pub power: f64,
}

pub struct BattleRenderData<'a> {
    pub left: &'a CombatProfile,
    pub right: &'a CombatProfile,
    pub left_system: &'a str,
    pub right_system: &'a str,
    pub result: &'a CombatResult,
}

pub fn profile(root: &Path, data: &ProfileRenderData<'_>, path: &Path) -> io::Result<()> {
    profile::render(root, data, path)
}

pub fn battle(root: &Path, data: &BattleRenderData<'_>, path: &Path) -> io::Result<()> {
    battle::render(root, data, path)
}
