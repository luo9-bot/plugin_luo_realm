use crate::{
    combat::{self, CombatAttributes, CombatSnapshot, CombatantSnapshot, ResourceSnapshot},
    core::Player,
    cultivation::{AttributeModifier, CultivationContext, registered_systems},
    equipment,
};

pub mod daily_state;
pub mod destiny;
pub mod event;
pub mod power;
pub mod world_event;

pub fn find_system(system_id: &str) -> Option<Box<dyn crate::cultivation::CultivationSystem>> {
    registered_systems()
        .into_iter()
        .find(|system| system.id() == system_id)
}
pub fn calculate_system_power(
    system_id: &str,
    context: &CultivationContext,
    base_power: f64,
) -> f64 {
    find_system(system_id)
        .map(|system| {
            let modifier = system.attribute_modifier(context);
            base_power * modifier.attack
                + system.power_contribution(context).base
                + context.destiny_power
        })
        .unwrap_or(base_power)
}
pub fn realm_pressure(attacker: usize, defender: usize) -> f64 {
    if attacker > defender {
        1.0 + ((attacker - defender) as f64 * 0.08).min(0.64)
    } else {
        1.0 - ((defender - attacker) as f64 * 0.05).min(0.35)
    }
}
pub fn combined_modifier(system_id: &str, context: &CultivationContext) -> AttributeModifier {
    find_system(system_id)
        .map(|system| system.attribute_modifier(context))
        .unwrap_or_default()
}

#[derive(Clone, Debug)]
pub struct CombatProfile {
    pub player: Player,
    pub power: f64,
    pub skills: &'static [&'static str],
}

pub fn build_combat_profile(
    player: &Player,
    system_id: &str,
    realm_index: u32,
    date: &str,
) -> CombatProfile {
    build_combat_profile_with_state(player, system_id, realm_index, date, None)
}

pub fn build_combat_profile_with_state(
    player: &Player,
    system_id: &str,
    realm_index: u32,
    date: &str,
    daily_state: Option<&daily_state::DailyState>,
) -> CombatProfile {
    let context = CultivationContext {
        realm: realm_index as usize,
        level: player.level,
        destiny_power: 0.0,
    };
    let system = find_system(system_id);
    let mut modifier = system
        .as_ref()
        .map(|system| system.attribute_modifier(&context))
        .unwrap_or_default();
    let realm_multiplier = 1.0 + realm_index as f64 * 0.08;
    let mut destiny_multiplier = destiny::destiny_multiplier(destiny::destiny_seed(
        date,
        &player.user_id,
        crate::identity::VERSION_SALT,
    ));
    if let Some(state) = daily_state {
        modifier.hp *= state.modifiers.hp;
        modifier.attack *= state.modifiers.attack;
        modifier.defense *= state.modifiers.defense;
        modifier.speed *= state.modifiers.speed;
        modifier.critical *= state.modifiers.critical;
        destiny_multiplier *= state.modifiers.destiny;
    }
    let power = power::calculate_total_power(
        player.base_hp as f64,
        player.base_attack as f64,
        player.base_defense as f64,
        player.speed as f64,
        &modifier,
        realm_multiplier,
        destiny_multiplier,
    );
    let mut combatant = player.clone();
    combatant.base_hp = scaled_attribute(player.base_hp, modifier.hp * realm_multiplier);
    combatant.base_attack = scaled_attribute(
        player.base_attack,
        modifier.attack * modifier.damage * realm_multiplier * destiny_multiplier,
    );
    combatant.base_defense =
        scaled_attribute(player.base_defense, modifier.defense * realm_multiplier);
    combatant.speed = scaled_attribute(player.speed, modifier.speed);
    combatant.critical_rate = (player.critical_rate * modifier.critical).clamp(0.0, 75.0);

    CombatProfile {
        player: combatant,
        power,
        skills: system.map(|system| system.skills()).unwrap_or_default(),
    }
}

pub fn build_combat_snapshot(
    left: (
        &Player,
        &crate::database::cultivation::CultivationState,
        Option<&daily_state::DailyState>,
        Vec<equipment::EquipmentItem>,
    ),
    right: (
        &Player,
        &crate::database::cultivation::CultivationState,
        Option<&daily_state::DailyState>,
        Vec<equipment::EquipmentItem>,
    ),
    date: &str,
    seed: u64,
) -> CombatSnapshot {
    let left_snapshot = build_combatant_snapshot(left, 0, 0, date);
    let right_snapshot = build_combatant_snapshot(right, 1, 1, date);
    CombatSnapshot {
        rule_version: 1,
        seed,
        rules: combat::BattleRules::default(),
        combatants: vec![left_snapshot, right_snapshot],
    }
}

fn build_combatant_snapshot(
    input: (
        &Player,
        &crate::database::cultivation::CultivationState,
        Option<&daily_state::DailyState>,
        Vec<equipment::EquipmentItem>,
    ),
    team: u8,
    position: i32,
    date: &str,
) -> CombatantSnapshot {
    let (player, cultivation, daily_state, equipped) = input;
    let tier = combat::universal_tier(
        cultivation.realm_index,
        find_system(&cultivation.system_id)
            .map(|system| system.realms().len())
            .unwrap_or(1),
    );
    let profile = build_combat_profile_with_state(
        player,
        &cultivation.system_id,
        cultivation.realm_index,
        date,
        daily_state,
    );
    let mut attributes = CombatAttributes {
        max_health: profile.player.base_hp,
        attack: profile.player.base_attack,
        physical_defense: profile.player.base_defense,
        arcane_defense: profile.player.base_defense / 2,
        soul_defense: profile.player.base_defense / 2,
        speed: profile.player.speed,
        critical_rate_basis_points: (profile.player.critical_rate * 100.0).round() as i64,
        critical_damage_basis_points: (profile.player.critical_multiplier * 10_000.0).round()
            as i64,
        recovery_power: profile.player.base_attack / 2,
        control_power: profile.player.base_attack / 3,
        tenacity: profile.player.base_defense * 10,
        domain_power: profile.player.base_attack / 4,
    };
    let bonuses = equipment::compile(&equipped);
    equipment::apply_to_attributes(&mut attributes, &bonuses);
    let (active, passive, domain) = combat::default_loadout(&cultivation.system_id, tier);
    CombatantSnapshot {
        combatant_id: player.user_id.clone(),
        player_id: player.user_id.parse().ok(),
        display_name: player.display_name.clone(),
        avatar_id: player.avatar_id.clone(),
        system_id: cultivation.system_id.clone(),
        universal_tier: tier,
        team,
        position,
        attributes,
        resource: ResourceSnapshot {
            kind: combat::resource_kind(&cultivation.system_id)
                .unwrap_or(combat::ResourceKind::SpiritualEnergy),
            current: 100,
            maximum: 100,
            regeneration: 4,
        },
        active_skills: active,
        passive_skills: passive,
        domain_skill: domain,
        equipment_triggers: bonuses.triggers,
        tactic: combat::Tactic::Balanced,
        power: profile.power.round() as i64,
    }
}

fn scaled_attribute(base: i64, multiplier: f64) -> i64 {
    ((base as f64 * multiplier).round() as i64).max(1)
}

#[cfg(test)]
mod tests {
    use crate::core::Player;

    use super::build_combat_profile;

    #[test]
    fn cultivation_system_changes_combat_attributes() {
        let player = Player::new(10001);

        let sword = build_combat_profile(&player, "sword", 0, "2026-08-31");
        let body = build_combat_profile(&player, "body", 0, "2026-08-31");

        assert!(sword.player.base_attack > body.player.base_attack);
        assert!(body.player.base_hp > sword.player.base_hp);
        assert_ne!(sword.power, body.power);
    }
}
