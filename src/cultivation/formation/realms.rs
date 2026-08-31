use super::super::RealmDefinition;
pub static REALMS: &[RealmDefinition] = &[
    RealmDefinition {
        name: "阵徒",
        threshold: 0,
        pressure: 1.0,
    },
    RealmDefinition {
        name: "阵士",
        threshold: 120,
        pressure: 1.1,
    },
    RealmDefinition {
        name: "阵师",
        threshold: 300,
        pressure: 1.2,
    },
    RealmDefinition {
        name: "阵灵",
        threshold: 650,
        pressure: 1.3,
    },
    RealmDefinition {
        name: "阵王",
        threshold: 1200,
        pressure: 1.4,
    },
    RealmDefinition {
        name: "阵皇",
        threshold: 2100,
        pressure: 1.5,
    },
    RealmDefinition {
        name: "阵圣",
        threshold: 3500,
        pressure: 1.6,
    },
    RealmDefinition {
        name: "阵神",
        threshold: 5500,
        pressure: 1.75,
    },
];
