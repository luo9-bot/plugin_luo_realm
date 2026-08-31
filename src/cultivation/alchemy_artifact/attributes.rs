use super::super::AttributeModifier;
pub fn attribute_modifier(realm_index: usize) -> AttributeModifier {
    let x = 1.0 + realm_index as f64 * 0.05;
    AttributeModifier {
        hp: x,
        attack: 0.8,
        defense: 1.1,
        speed: 0.9,
        critical: 0.9,
        damage: 1.0 * x,
    }
}
