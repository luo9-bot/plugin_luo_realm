use super::super::RealmDefinition;
pub static REALMS: &[RealmDefinition] = &[
    RealmDefinition {
        name: "练气",
        threshold: 0,
        pressure: 1.0,
    },
    RealmDefinition {
        name: "筑基",
        threshold: 100,
        pressure: 1.08,
    },
    RealmDefinition {
        name: "金丹",
        threshold: 300,
        pressure: 1.16,
    },
    RealmDefinition {
        name: "元婴",
        threshold: 700,
        pressure: 1.24,
    },
    RealmDefinition {
        name: "出窍",
        threshold: 1400,
        pressure: 1.32,
    },
    RealmDefinition {
        name: "分神",
        threshold: 2500,
        pressure: 1.4,
    },
    RealmDefinition {
        name: "合体",
        threshold: 4200,
        pressure: 1.48,
    },
    RealmDefinition {
        name: "渡劫",
        threshold: 6800,
        pressure: 1.56,
    },
    RealmDefinition {
        name: "大乘",
        threshold: 10000,
        pressure: 1.64,
    },
];
