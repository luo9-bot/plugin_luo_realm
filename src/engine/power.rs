use crate::cultivation::AttributeModifier;
pub fn calculate_total_power(
    health: f64,
    attack: f64,
    defense: f64,
    speed: f64,
    modifier: &AttributeModifier,
    realm_multiplier: f64,
    destiny_multiplier: f64,
) -> f64 {
    let attribute_power = health * 0.08 * modifier.hp
        + attack * 2.0 * modifier.attack
        + defense * 1.5 * modifier.defense
        + speed * 3.0 * modifier.speed;
    let damage_power = modifier.damage * attack;

    attribute_power * realm_multiplier * destiny_multiplier + damage_power
}
