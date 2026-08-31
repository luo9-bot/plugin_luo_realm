use super::DailyModifiers;

pub struct DailyStateDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub modifiers: DailyModifiers,
}

pub const DEFINITIONS: [DailyStateDefinition; 7] = [
    DailyStateDefinition {
        id: "calm",
        name: "心境平和",
        description: "气机平稳，今日修行与战斗没有明显偏向。",
        modifiers: DailyModifiers {
            hp: 1.0,
            attack: 1.0,
            defense: 1.0,
            speed: 1.0,
            critical: 1.0,
            destiny: 1.0,
        },
    },
    DailyStateDefinition {
        id: "clear_mind",
        name: "灵台澄明",
        description: "念头通达，攻防与出手判断均有提升。",
        modifiers: DailyModifiers {
            hp: 1.02,
            attack: 1.04,
            defense: 1.03,
            speed: 1.01,
            critical: 1.04,
            destiny: 1.02,
        },
    },
    DailyStateDefinition {
        id: "blood_heat",
        name: "气血沸腾",
        description: "攻势迅猛，但过度激进令防御出现空隙。",
        modifiers: DailyModifiers {
            hp: 1.0,
            attack: 1.08,
            defense: 0.94,
            speed: 1.08,
            critical: 1.03,
            destiny: 1.0,
        },
    },
    DailyStateDefinition {
        id: "wounded",
        name: "伤势未愈",
        description: "旧伤牵动气机，生存与攻势受到影响。",
        modifiers: DailyModifiers {
            hp: 0.94,
            attack: 0.96,
            defense: 0.95,
            speed: 0.97,
            critical: 0.98,
            destiny: 1.05,
        },
    },
    DailyStateDefinition {
        id: "dao_dust",
        name: "道心蒙尘",
        description: "神思不宁，战斗受限，却更容易撞见异数。",
        modifiers: DailyModifiers {
            hp: 0.98,
            attack: 0.95,
            defense: 1.0,
            speed: 0.96,
            critical: 0.96,
            destiny: 1.08,
        },
    },
    DailyStateDefinition {
        id: "fated",
        name: "天命眷顾",
        description: "诸事顺遂，六项战斗与机缘属性小幅提升。",
        modifiers: DailyModifiers {
            hp: 1.04,
            attack: 1.04,
            defense: 1.04,
            speed: 1.04,
            critical: 1.06,
            destiny: 1.10,
        },
    },
    DailyStateDefinition {
        id: "tempered",
        name: "百战余势",
        description: "往期争斗沉淀为经验，攻防更为老练。",
        modifiers: DailyModifiers {
            hp: 1.03,
            attack: 1.05,
            defense: 1.05,
            speed: 0.99,
            critical: 1.02,
            destiny: 0.99,
        },
    },
];
