use super::super::RealmDefinition;
pub static REALMS: &[RealmDefinition] = &[
    RealmDefinition {
        name: "初级魔法师",
        threshold: 0,
        pressure: 1.0,
    },
    RealmDefinition {
        name: "中级魔法师",
        threshold: 150,
        pressure: 1.1,
    },
    RealmDefinition {
        name: "高级魔法师",
        threshold: 400,
        pressure: 1.2,
    },
    RealmDefinition {
        name: "大魔法师",
        threshold: 900,
        pressure: 1.3,
    },
    RealmDefinition {
        name: "法神",
        threshold: 1800,
        pressure: 1.45,
    },
];
