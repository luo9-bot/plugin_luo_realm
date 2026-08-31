mod attributes;
mod balance;
mod mechanics;
mod realms;
mod skills;

#[cfg(test)]
mod tests;

crate::define_cultivation_system!("blood_demon", "血魔邪修", "速成爆发，但承受心魔反噬");
