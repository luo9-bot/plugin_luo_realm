use super::super::AttributeModifier;
pub fn attribute_modifier(realm_index: usize) -> AttributeModifier {
    AttributeModifier {
        hp: 1.0,
        attack: 1.15 + realm_index as f64 * 0.06,
        defense: 0.9,
        speed: 1.05,
        critical: 1.1,
        damage: 1.2,
    }
}
