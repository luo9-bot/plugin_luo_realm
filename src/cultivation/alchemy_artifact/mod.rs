mod attributes;
mod balance;
mod mechanics;
mod realms;
mod skills;

#[cfg(test)]
mod tests;

crate::define_cultivation_system!("alchemy_artifact", "丹器修", "炼丹炼器，资源与强化收益");
