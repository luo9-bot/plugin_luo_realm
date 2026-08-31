use super::super::AttributeModifier;
pub fn attribute_modifier(realm_index: usize) -> AttributeModifier {
    AttributeModifier {
        hp: 1.0,
        attack: 1.4 + realm_index as f64 * 0.12,
        defense: 0.8,
        speed: 1.15,
        critical: 1.4,
        damage: 1.5,
    }
}
