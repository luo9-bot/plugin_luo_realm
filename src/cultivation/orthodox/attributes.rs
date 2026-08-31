use super::super::AttributeModifier;
pub fn attribute_modifier(realm_index: usize) -> AttributeModifier {
    let x = 1.0 + realm_index as f64 * 0.06;
    AttributeModifier {
        hp: x,
        attack: x,
        defense: x,
        speed: x,
        critical: 1.0,
        damage: x,
    }
}
