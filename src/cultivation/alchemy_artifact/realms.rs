use super::super::RealmDefinition;
pub static REALMS: &[RealmDefinition] = &[
    RealmDefinition {
        name: "丹器徒",
        threshold: 0,
        pressure: 1.0,
    },
    RealmDefinition {
        name: "丹器士",
        threshold: 150,
        pressure: 1.1,
    },
    RealmDefinition {
        name: "丹器师",
        threshold: 400,
        pressure: 1.2,
    },
    RealmDefinition {
        name: "丹器宗",
        threshold: 800,
        pressure: 1.3,
    },
    RealmDefinition {
        name: "丹器王",
        threshold: 1400,
        pressure: 1.4,
    },
    RealmDefinition {
        name: "丹器皇",
        threshold: 2300,
        pressure: 1.5,
    },
    RealmDefinition {
        name: "丹器圣",
        threshold: 3600,
        pressure: 1.6,
    },
    RealmDefinition {
        name: "丹器神",
        threshold: 5400,
        pressure: 1.75,
    },
];
