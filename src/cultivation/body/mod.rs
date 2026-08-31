mod attributes;
mod balance;
mod mechanics;
mod realms;
mod skills;

#[cfg(test)]
mod tests;

crate::define_cultivation_system!("body", "体修", "高生命高防御，越战越勇");
