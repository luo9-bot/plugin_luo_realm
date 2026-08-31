use super::super::RealmDefinition;
pub static REALMS: &[RealmDefinition] = &[
    RealmDefinition {
        name: "魂士",
        threshold: 0,
        pressure: 1.0,
    },
    RealmDefinition {
        name: "魂师",
        threshold: 100,
        pressure: 1.08,
    },
    RealmDefinition {
        name: "大魂师",
        threshold: 250,
        pressure: 1.16,
    },
    RealmDefinition {
        name: "魂尊",
        threshold: 500,
        pressure: 1.24,
    },
    RealmDefinition {
        name: "魂宗",
        threshold: 900,
        pressure: 1.32,
    },
    RealmDefinition {
        name: "魂王",
        threshold: 1500,
        pressure: 1.4,
    },
    RealmDefinition {
        name: "魂帝",
        threshold: 2400,
        pressure: 1.48,
    },
    RealmDefinition {
        name: "魂圣",
        threshold: 3600,
        pressure: 1.56,
    },
    RealmDefinition {
        name: "魂斗罗",
        threshold: 5200,
        pressure: 1.64,
    },
    RealmDefinition {
        name: "封号斗罗",
        threshold: 7500,
        pressure: 1.72,
    },
];
