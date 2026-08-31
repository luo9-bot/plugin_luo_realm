mod definitions;

use serde::{Deserialize, Serialize};

use crate::core::stable_seed;

use self::definitions::DEFINITIONS;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DailyModifiers {
    pub hp: f64,
    pub attack: f64,
    pub defense: f64,
    pub speed: f64,
    pub critical: f64,
    pub destiny: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DailyState {
    pub id: String,
    pub name: String,
    pub description: String,
    pub modifiers: DailyModifiers,
    pub seed: u64,
}

#[derive(Debug, Serialize)]
pub struct DailyStateInput {
    pub user_id: u64,
    pub system_id: String,
    pub realm_index: u32,
    pub progress: u64,
    pub foundation: u64,
    pub comprehension: u64,
    pub deviation: u64,
    pub checkin_streak: u64,
    pub recent_wins: u32,
    pub recent_losses: u32,
    pub recent_destinies: u32,
    pub previous_states: Vec<String>,
}

pub fn generate(date: &str, input: &DailyStateInput) -> DailyState {
    let identity = format!(
        "{}:{}:{}:{}:{}:{}",
        input.user_id,
        input.system_id,
        input.realm_index,
        input.progress,
        input.foundation,
        input.comprehension,
    );
    let seed = stable_seed(
        date,
        "daily-state",
        &identity,
        crate::identity::VERSION_SALT,
    );
    let weighted = DEFINITIONS
        .iter()
        .map(|definition| (definition, state_weight(definition.id, input)))
        .collect::<Vec<_>>();
    let total = weighted
        .iter()
        .map(|(_, weight)| weight)
        .sum::<u64>()
        .max(1);
    let mut cursor = seed % total;
    let definition = weighted
        .into_iter()
        .find_map(|(definition, weight)| {
            if cursor < weight {
                Some(definition)
            } else {
                cursor = cursor.saturating_sub(weight);
                None
            }
        })
        .unwrap_or(&DEFINITIONS[0]);

    DailyState {
        id: definition.id.into(),
        name: definition.name.into(),
        description: definition.description.into(),
        modifiers: definition.modifiers.clone(),
        seed,
    }
}

fn state_weight(id: &str, input: &DailyStateInput) -> u64 {
    let base = match id {
        "calm" => 8,
        "clear_mind" => 4 + input.checkin_streak.min(8) + input.comprehension / 20,
        "blood_heat" => 4 + u64::from(input.recent_wins),
        "wounded" => 1 + u64::from(input.recent_losses * 4) + input.deviation / 25,
        "dao_dust" => 1 + input.deviation / 15,
        "fated" => 2 + u64::from(input.recent_destinies * 3) + input.foundation / 25,
        "tempered" => 2 + u64::from(input.recent_wins * 4) + input.realm_index as u64,
        _ => 1,
    };
    match input
        .previous_states
        .iter()
        .position(|previous| previous == id)
    {
        Some(0) => 0,
        Some(_) => base.div_ceil(2),
        None => base,
    }
}
