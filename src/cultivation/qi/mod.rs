mod attributes;
mod balance;
mod mechanics;
mod realms;
mod skills;

#[cfg(test)]
mod tests;

crate::define_cultivation_system!("qi", "气修", "攻防一体，稳定成长");
