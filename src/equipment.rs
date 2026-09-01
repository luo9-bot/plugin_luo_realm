use crate::combat::{
    BASIS_POINTS, CombatAttributes, EquipmentSlot, EquipmentTrigger, SkillEffect, TriggerCondition,
};

#[derive(Clone, Debug)]
pub struct EquipmentItem {
    pub item_id: i64,
    pub definition_id: String,
    pub quality: String,
    pub level: u32,
    pub slot: EquipmentSlot,
    pub modifiers: Vec<(String, i64)>,
}

#[derive(Clone, Debug, Default)]
pub struct EquipmentBonuses {
    pub max_health: i64,
    pub attack: i64,
    pub physical_defense: i64,
    pub arcane_defense: i64,
    pub soul_defense: i64,
    pub speed: i64,
    pub critical_rate_basis_points: i64,
    pub recovery_power: i64,
    pub control_power: i64,
    pub tenacity: i64,
    pub domain_power: i64,
    pub triggers: Vec<EquipmentTrigger>,
}

pub fn slot_for_definition(definition_id: &str) -> Option<EquipmentSlot> {
    let normalized = definition_id.to_ascii_lowercase();
    if definition_id.contains("灵器")
        || definition_id.contains("武器")
        || normalized.contains("weapon")
        || normalized.contains("sword")
    {
        Some(EquipmentSlot::MainHand)
    } else if definition_id.contains("帽子") || normalized.contains("head") {
        Some(EquipmentSlot::Head)
    } else if definition_id.contains("衣服")
        || definition_id.contains("护甲")
        || normalized.contains("body")
        || normalized.contains("armor")
    {
        Some(EquipmentSlot::Body)
    } else if definition_id.contains("手套") || normalized.contains("hands") {
        Some(EquipmentSlot::Hands)
    } else if definition_id.contains("鞋") || normalized.contains("feet") {
        Some(EquipmentSlot::Feet)
    } else if definition_id.contains("副手") || normalized.contains("offhand") {
        Some(EquipmentSlot::OffHand)
    } else if definition_id.contains("饰品") || normalized.contains("accessory") {
        Some(EquipmentSlot::AccessoryOne)
    } else {
        None
    }
}

pub fn compile(items: &[EquipmentItem]) -> EquipmentBonuses {
    let mut bonuses = EquipmentBonuses::default();
    items.iter().for_each(|item| {
        apply_base_bonus(&mut bonuses, item);
        item.modifiers.iter().for_each(|(code, value)| {
            apply_modifier(&mut bonuses, item, code, *value);
        });
    });
    bonuses.critical_rate_basis_points = bonuses.critical_rate_basis_points.min(4_000);
    bonuses.speed = bonuses.speed.min(60);
    bonuses
}

pub fn apply_to_attributes(attributes: &mut CombatAttributes, bonuses: &EquipmentBonuses) {
    attributes.max_health = (attributes.max_health + bonuses.max_health).max(1);
    attributes.attack = (attributes.attack + bonuses.attack).max(1);
    attributes.physical_defense = (attributes.physical_defense + bonuses.physical_defense).max(0);
    attributes.arcane_defense = (attributes.arcane_defense + bonuses.arcane_defense).max(0);
    attributes.soul_defense = (attributes.soul_defense + bonuses.soul_defense).max(0);
    attributes.speed = (attributes.speed + bonuses.speed).max(1);
    attributes.critical_rate_basis_points =
        (attributes.critical_rate_basis_points + bonuses.critical_rate_basis_points).min(7_500);
    attributes.recovery_power = (attributes.recovery_power + bonuses.recovery_power).max(1);
    attributes.control_power = (attributes.control_power + bonuses.control_power).max(0);
    attributes.tenacity = (attributes.tenacity + bonuses.tenacity).max(1);
    attributes.domain_power = (attributes.domain_power + bonuses.domain_power).max(0);
}

fn apply_base_bonus(bonuses: &mut EquipmentBonuses, item: &EquipmentItem) {
    let quality = quality_basis_points(&item.quality);
    let level = i64::from(item.level) + 1;
    match item.slot {
        EquipmentSlot::MainHand => bonuses.attack += 12 * level * quality / BASIS_POINTS,
        EquipmentSlot::OffHand => {
            bonuses.physical_defense += 8 * level * quality / BASIS_POINTS;
            bonuses.arcane_defense += 8 * level * quality / BASIS_POINTS;
        }
        EquipmentSlot::Head => {
            bonuses.soul_defense += 10 * level * quality / BASIS_POINTS;
            bonuses.tenacity += 8 * level * quality / BASIS_POINTS;
        }
        EquipmentSlot::Body => {
            bonuses.max_health += 70 * level * quality / BASIS_POINTS;
            bonuses.physical_defense += 12 * level * quality / BASIS_POINTS;
        }
        EquipmentSlot::Hands => bonuses.control_power += 8 * level * quality / BASIS_POINTS,
        EquipmentSlot::Feet => bonuses.speed += 2 * level * quality / BASIS_POINTS,
        EquipmentSlot::AccessoryOne | EquipmentSlot::AccessoryTwo => {
            bonuses.recovery_power += 8 * level * quality / BASIS_POINTS;
            bonuses.domain_power += 6 * level * quality / BASIS_POINTS;
        }
    }
}

fn quality_basis_points(quality: &str) -> i64 {
    match quality {
        "common" | "legacy" => 10_000,
        "fine" => 12_000,
        "rare" => 14_000,
        "epic" => 17_000,
        "legendary" => 20_000,
        _ => 10_000,
    }
}

fn apply_modifier(bonuses: &mut EquipmentBonuses, item: &EquipmentItem, code: &str, value: i64) {
    match code {
        "max_health" => bonuses.max_health += value.clamp(0, 5_000),
        "attack" => bonuses.attack += value.clamp(0, 1_000),
        "physical_defense" => bonuses.physical_defense += value.clamp(0, 1_000),
        "arcane_defense" => bonuses.arcane_defense += value.clamp(0, 1_000),
        "soul_defense" => bonuses.soul_defense += value.clamp(0, 1_000),
        "speed" => bonuses.speed += value.clamp(0, 30),
        "critical_rate" => bonuses.critical_rate_basis_points += value.clamp(0, 1_500),
        "recovery" => bonuses.recovery_power += value.clamp(0, 1_000),
        "control" => bonuses.control_power += value.clamp(0, 1_000),
        "tenacity" => bonuses.tenacity += value.clamp(0, 2_000),
        "domain" => bonuses.domain_power += value.clamp(0, 1_000),
        "battle_shield" => bonuses.triggers.push(trigger(
            item,
            TriggerCondition::BattleStarted,
            SkillEffect::Shield {
                power_basis_points: value.clamp(500, 5_000),
                duration: 30,
            },
        )),
        "emergency_heal" => bonuses.triggers.push(trigger(
            item,
            TriggerCondition::HealthBelowHalf,
            SkillEffect::Heal {
                power_basis_points: value.clamp(500, 4_000),
                flat: 0,
            },
        )),
        "guard_trigger" => bonuses.triggers.push(trigger(
            item,
            TriggerCondition::DamageTaken,
            SkillEffect::Block {
                reduction_basis_points: value.clamp(500, 5_000),
                charges: 1,
                duration: 10,
            },
        )),
        "dodge_trigger" => bonuses.triggers.push(trigger(
            item,
            TriggerCondition::ShieldBroken,
            SkillEffect::Dodge {
                charges: 1,
                duration: 10,
            },
        )),
        _ => {}
    }
}

fn trigger(
    item: &EquipmentItem,
    condition: TriggerCondition,
    effect: SkillEffect,
) -> EquipmentTrigger {
    EquipmentTrigger {
        source_item_id: item.item_id,
        source_name: item.definition_id.clone(),
        condition,
        once_per_battle: true,
        effect,
    }
}
