use super::super::AttributeModifier;
pub fn attribute_modifier(realm_index: usize) -> AttributeModifier {
    let x = 1.0 + realm_index as f64 * 0.06;
    AttributeModifier {
        hp: 1.2,
        attack: 0.9,
        defense: 1.45 + realm_index as f64 * 0.08,
        speed: 0.75,
        critical: 1.0,
        damage: 1.15 * x,
    }
}
