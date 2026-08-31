use super::super::RealmDefinition;
pub static REALMS: &[RealmDefinition] = &[
    RealmDefinition {
        name: "剑士",
        threshold: 0,
        pressure: 1.0,
    },
    RealmDefinition {
        name: "剑客",
        threshold: 150,
        pressure: 1.1,
    },
    RealmDefinition {
        name: "剑手",
        threshold: 400,
        pressure: 1.2,
    },
    RealmDefinition {
        name: "剑侠",
        threshold: 900,
        pressure: 1.3,
    },
    RealmDefinition {
        name: "剑仙",
        threshold: 1800,
        pressure: 1.45,
    },
];
