pub struct ObjectiveDefinition {
    pub id: &'static str,
    pub kind: &'static str,
    pub label: &'static str,
    pub target: i64,
}

pub struct WorldEventDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub objectives: &'static [ObjectiveDefinition],
    pub coin_reward: i64,
    pub mark_reward: i64,
}

const RESOURCE_OBJECTIVES: [ObjectiveDefinition; 2] = [
    ObjectiveDefinition {
        id: "gather",
        kind: "checkin",
        label: "群员集结",
        target: 4,
    },
    ObjectiveDefinition {
        id: "explore",
        kind: "destiny",
        label: "探寻灵潮",
        target: 3,
    },
];
const INVASION_OBJECTIVES: [ObjectiveDefinition; 3] = [
    ObjectiveDefinition {
        id: "mobilize",
        kind: "checkin",
        label: "召集修士",
        target: 3,
    },
    ObjectiveDefinition {
        id: "scout",
        kind: "destiny",
        label: "侦察魔踪",
        target: 2,
    },
    ObjectiveDefinition {
        id: "repel",
        kind: "duel",
        label: "演武备战",
        target: 5,
    },
];
const TOURNAMENT_OBJECTIVES: [ObjectiveDefinition; 2] = [
    ObjectiveDefinition {
        id: "contest",
        kind: "duel",
        label: "完成对局",
        target: 6,
    },
    ObjectiveDefinition {
        id: "audience",
        kind: "checkin",
        label: "群英到场",
        target: 4,
    },
];
const STORM_OBJECTIVES: [ObjectiveDefinition; 2] = [
    ObjectiveDefinition {
        id: "observe",
        kind: "destiny",
        label: "观测异象",
        target: 4,
    },
    ObjectiveDefinition {
        id: "stabilize",
        kind: "checkin",
        label: "稳定阵眼",
        target: 4,
    },
];
const PATROL_OBJECTIVES: [ObjectiveDefinition; 3] = [
    ObjectiveDefinition {
        id: "assemble",
        kind: "checkin",
        label: "晨间点卯",
        target: 3,
    },
    ObjectiveDefinition {
        id: "patrol",
        kind: "destiny",
        label: "巡视四方",
        target: 3,
    },
    ObjectiveDefinition {
        id: "drill",
        kind: "duel",
        label: "日常操练",
        target: 3,
    },
];

pub const DEFINITIONS: [WorldEventDefinition; 5] = [
    WorldEventDefinition {
        id: "resource_tide",
        name: "资源潮汐",
        description: "灵脉翻涌，散落的灵材等待众人共同收集。",
        objectives: &RESOURCE_OBJECTIVES,
        coin_reward: 180,
        mark_reward: 2,
    },
    WorldEventDefinition {
        id: "demon_invasion",
        name: "魔物入侵",
        description: "魔物逼近驻地，群中修士必须完成侦察与备战。",
        objectives: &INVASION_OBJECTIVES,
        coin_reward: 260,
        mark_reward: 3,
    },
    WorldEventDefinition {
        id: "tournament",
        name: "竞技庆典",
        description: "今日开坛演武，以切磋磨砺诸道。",
        objectives: &TOURNAMENT_OBJECTIVES,
        coin_reward: 220,
        mark_reward: 2,
    },
    WorldEventDefinition {
        id: "arcane_storm",
        name: "奥术风暴",
        description: "紊乱灵潮席卷此地，需要共同观测并稳定阵眼。",
        objectives: &STORM_OBJECTIVES,
        coin_reward: 240,
        mark_reward: 3,
    },
    WorldEventDefinition {
        id: "peace_patrol",
        name: "安宁巡守",
        description: "世道暂安，正适合结伴巡守并巩固修为。",
        objectives: &PATROL_OBJECTIVES,
        coin_reward: 160,
        mark_reward: 2,
    },
];
