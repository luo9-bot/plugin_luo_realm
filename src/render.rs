pub(crate) mod assets;
pub(crate) mod battle;
mod profile;

use std::{io, path::Path};

pub struct ProfileRenderData<'a> {
    pub player: &'a crate::core::Player,
    pub system_id: &'a str,
    pub system_name: &'a str,
    pub realm_name: &'a str,
    pub realm_index: u32,
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
