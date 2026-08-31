mod definitions;

use crate::core::stable_seed;

pub use definitions::{ObjectiveDefinition, WorldEventDefinition};

pub fn select(date: &str, group_id: u64) -> &'static WorldEventDefinition {
    let seed = stable_seed(
        date,
        "group-world-event",
        &group_id.to_string(),
        crate::identity::VERSION_SALT,
    );
    &definitions::DEFINITIONS[(seed as usize) % definitions::DEFINITIONS.len()]
}

pub fn find(id: &str) -> Option<&'static WorldEventDefinition> {
    definitions::DEFINITIONS
        .iter()
        .find(|definition| definition.id == id)
}
