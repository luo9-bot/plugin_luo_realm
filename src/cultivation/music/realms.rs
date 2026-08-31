use super::super::RealmDefinition;
pub static REALMS: &[RealmDefinition] = &[
    RealmDefinition {
        name: "初级",
        threshold: 0,
        pressure: 1.0,
    },
    RealmDefinition {
        name: "中级",
        threshold: 150,
        pressure: 1.1,
    },
    RealmDefinition {
        name: "高级",
        threshold: 400,
        pressure: 1.2,
    },
    RealmDefinition {
        name: "大师",
        threshold: 900,
        pressure: 1.35,
    },
    RealmDefinition {
        name: "宗师",
        threshold: 1800,
        pressure: 1.5,
    },
];
