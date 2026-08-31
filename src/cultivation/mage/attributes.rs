use super::super::AttributeModifier;
pub fn attribute_modifier(realm_index: usize) -> AttributeModifier {
    AttributeModifier {
        hp: 0.75,
        attack: 1.5 + realm_index as f64 * 0.1,
        defense: 0.7,
        speed: 1.0,
        critical: 1.2,
        damage: 1.4,
    }
}
