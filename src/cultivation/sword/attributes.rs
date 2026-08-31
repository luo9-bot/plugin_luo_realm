use super::super::AttributeModifier;
pub fn attribute_modifier(realm_index: usize) -> AttributeModifier {
    AttributeModifier {
        hp: 0.88 + realm_index as f64 * 0.03,
        attack: 1.35 + realm_index as f64 * 0.08,
        defense: 0.82,
        speed: 1.3 + realm_index as f64 * 0.05,
        critical: 1.25,
        damage: 1.2,
    }
}
