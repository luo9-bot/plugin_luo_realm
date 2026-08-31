use super::super::RealmDefinition;
pub static REALMS: &[RealmDefinition] = &[
    RealmDefinition {
        name: "入门",
        threshold: 0,
        pressure: 1.0,
    },
    RealmDefinition {
        name: "血徒",
        threshold: 100,
        pressure: 1.1,
    },
    RealmDefinition {
        name: "血士",
        threshold: 250,
        pressure: 1.2,
    },
    RealmDefinition {
        name: "血师",
        threshold: 500,
        pressure: 1.3,
    },
    RealmDefinition {
        name: "血侯",
        threshold: 900,
        pressure: 1.4,
    },
    RealmDefinition {
        name: "血王",
        threshold: 1500,
        pressure: 1.5,
    },
    RealmDefinition {
        name: "血皇",
        threshold: 2400,
        pressure: 1.6,
    },
    RealmDefinition {
        name: "血帝",
        threshold: 4000,
        pressure: 1.75,
    },
];
