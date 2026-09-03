mod definitions;

use serde::{Deserialize, Serialize};

use crate::core::stable_seed;
use crate::domain::rule_versions;
use crate::domain::shared::RuleVersion;

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
    pub rule_version: RuleVersion,
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
        rule_version: rule_versions::DAILY_STATE,
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

#[cfg(test)]
mod tests {
    use super::{DailyStateInput, generate};
    use crate::domain::rule_versions;

    fn sample_input() -> DailyStateInput {
        DailyStateInput {
            user_id: 10001,
            system_id: "sword".into(),
            realm_index: 2,
            progress: 300,
            foundation: 40,
            comprehension: 55,
            deviation: 5,
            checkin_streak: 3,
            recent_wins: 2,
            recent_losses: 1,
            recent_destinies: 1,
            previous_states: vec![],
        }
    }

    #[test]
    fn generation_is_deterministic_and_versioned() {
        let input = sample_input();
        let first = generate("2026-09-03", &input);
        let second = generate("2026-09-03", &input);

        assert_eq!(first.id, second.id);
        assert_eq!(first.name, second.name);
        assert_eq!(first.seed, second.seed);
        assert_eq!(first.rule_version, rule_versions::DAILY_STATE);
    }

    #[test]
    fn different_date_yields_independent_seed() {
        let input = sample_input();
        let earlier = generate("2026-09-02", &input);
        let later = generate("2026-09-03", &input);

        assert_ne!(earlier.seed, later.seed);
        assert_eq!(earlier.rule_version, later.rule_version);
    }
}
