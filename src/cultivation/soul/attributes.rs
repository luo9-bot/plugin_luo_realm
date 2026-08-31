use super::super::AttributeModifier;
pub fn attribute_modifier(realm_index: usize) -> AttributeModifier {
    AttributeModifier {
        hp: 0.8,
        attack: 1.25 + realm_index as f64 * 0.08,
        defense: 0.75,
        speed: 1.1,
        critical: 1.3,
        damage: 1.3,
    }
}
