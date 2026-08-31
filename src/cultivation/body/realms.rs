use super::super::RealmDefinition;
pub static REALMS: &[RealmDefinition] = &[
    RealmDefinition {
        name: "明劲",
        threshold: 0,
        pressure: 1.0,
    },
    RealmDefinition {
        name: "暗劲",
        threshold: 120,
        pressure: 1.1,
    },
    RealmDefinition {
        name: "化劲",
        threshold: 350,
        pressure: 1.2,
    },
    RealmDefinition {
        name: "丹劲",
        threshold: 800,
        pressure: 1.3,
    },
    RealmDefinition {
        name: "罡劲",
        threshold: 1600,
        pressure: 1.45,
    },
];
