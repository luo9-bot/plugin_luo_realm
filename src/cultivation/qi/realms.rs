use super::super::RealmDefinition;
pub static REALMS: &[RealmDefinition] = &[
    RealmDefinition {
        name: "斗者",
        threshold: 0,
        pressure: 1.0,
    },
    RealmDefinition {
        name: "斗师",
        threshold: 120,
        pressure: 1.08,
    },
    RealmDefinition {
        name: "大斗师",
        threshold: 300,
        pressure: 1.16,
    },
    RealmDefinition {
        name: "斗灵",
        threshold: 600,
        pressure: 1.24,
    },
    RealmDefinition {
        name: "斗王",
        threshold: 1100,
        pressure: 1.32,
    },
    RealmDefinition {
        name: "斗皇",
        threshold: 1900,
        pressure: 1.4,
    },
    RealmDefinition {
        name: "斗宗",
        threshold: 3000,
        pressure: 1.48,
    },
    RealmDefinition {
        name: "斗尊",
        threshold: 4500,
        pressure: 1.56,
    },
    RealmDefinition {
        name: "斗圣",
        threshold: 6500,
        pressure: 1.64,
    },
    RealmDefinition {
        name: "斗帝",
        threshold: 9000,
        pressure: 1.72,
    },
];
