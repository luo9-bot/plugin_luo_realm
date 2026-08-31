use super::super::AttributeModifier;
pub fn attribute_modifier(realm_index: usize) -> AttributeModifier {
    AttributeModifier {
        hp: 1.5 + realm_index as f64 * 0.12,
        attack: 1.1 + realm_index as f64 * 0.05,
        defense: 1.5 + realm_index as f64 * 0.1,
        speed: 0.8,
        critical: 0.9,
        damage: 1.05,
    }
}
