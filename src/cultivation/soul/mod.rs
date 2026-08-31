mod attributes;
mod balance;
mod mechanics;
mod realms;
mod skills;

#[cfg(test)]
mod tests;

crate::define_cultivation_system!("soul", "灵修", "神魂攻击与领域压制");
