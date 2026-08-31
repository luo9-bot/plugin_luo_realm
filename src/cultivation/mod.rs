use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CultivationContext {
    pub realm: usize,
    pub level: u32,
    pub destiny_power: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AttributeModifier {
    pub hp: f64,
    pub attack: f64,
    pub defense: f64,
    pub speed: f64,
    pub critical: f64,
    pub damage: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PowerContribution {
    pub base: f64,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RealmDefinition {
    pub name: &'static str,
    pub threshold: u64,
    pub pressure: f64,
}

pub trait CultivationSystem: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn realms(&self) -> &'static [RealmDefinition];
    fn attribute_modifier(&self, context: &CultivationContext) -> AttributeModifier;
    fn power_contribution(&self, context: &CultivationContext) -> PowerContribution;
    fn skills(&self) -> &'static [&'static str];
    fn tags(&self) -> &'static [&'static str];
}

#[macro_export]
macro_rules! define_cultivation_system {
    ($id:literal, $name:literal, $description:literal) => {
        pub struct System;

        impl $crate::cultivation::CultivationSystem for System {
            fn id(&self) -> &'static str {
                $id
            }

            fn name(&self) -> &'static str {
                $name
            }

            fn realms(&self) -> &'static [$crate::cultivation::RealmDefinition] {
                realms::REALMS
            }

            fn attribute_modifier(
                &self,
                context: &$crate::cultivation::CultivationContext,
            ) -> $crate::cultivation::AttributeModifier {
                attributes::attribute_modifier(context.realm)
            }

            fn power_contribution(
                &self,
                context: &$crate::cultivation::CultivationContext,
            ) -> $crate::cultivation::PowerContribution {
                $crate::cultivation::PowerContribution {
                    base: balance::BASE_POWER * (context.realm as f64 + 1.0),
                    description: $description.into(),
                }
            }

            fn skills(&self) -> &'static [&'static str] {
                skills::SKILLS
            }

            fn tags(&self) -> &'static [&'static str] {
                mechanics::TAGS
            }
        }
    };
}

macro_rules! declare_systems {
    ($($name:ident),+ $(,)?) => {
        $(pub mod $name;)+
    };
}
declare_systems!(
    orthodox,
    sword,
    body,
    mage,
    soul,
    qi,
    blood_demon,
    formation,
    alchemy_artifact,
    summoner,
    music
);

pub fn registered_systems() -> Vec<Box<dyn CultivationSystem>> {
    vec![
        Box::new(orthodox::System),
        Box::new(sword::System),
        Box::new(body::System),
        Box::new(mage::System),
        Box::new(soul::System),
        Box::new(qi::System),
        Box::new(blood_demon::System),
        Box::new(formation::System),
        Box::new(alchemy_artifact::System),
        Box::new(summoner::System),
        Box::new(music::System),
    ]
}
