use super::{
    DamageType, ResourceKind, SkillCategory, SkillDefinition, SkillEffect, SkillTag, StatusKind,
    TargetRule,
};

const SYSTEM_IDS: [&str; 11] = [
    "orthodox",
    "sword",
    "body",
    "mage",
    "soul",
    "qi",
    "blood_demon",
    "formation",
    "alchemy_artifact",
    "summoner",
    "music",
];

struct SystemSkills {
    id: &'static str,
    resource: ResourceKind,
    damage_type: DamageType,
    names: [&'static str; 11],
}

pub fn universal_tier(realm_index: u32, realm_count: usize) -> u8 {
    match realm_count {
        0 | 1 => 0,
        count => ((realm_index.min((count - 1) as u32) as usize * 8) / (count - 1)) as u8,
    }
}

pub fn active_slot_capacity(tier: u8) -> usize {
    match tier.min(8) {
        0 => 3,
        1 => 4,
        2 => 5,
        3 => 6,
        4 => 7,
        5 => 8,
        6 => 9,
        7 => 10,
        _ => 12,
    }
}

pub fn passive_slot_capacity(tier: u8) -> usize {
    match tier.min(8) {
        0 | 1 => 2,
        2 | 3 => 3,
        4 | 5 => 4,
        _ => 5,
    }
}

pub fn skills_for_system(system_id: &str) -> Vec<SkillDefinition> {
    system_skills(system_id)
        .map(build_skills)
        .unwrap_or_default()
}

pub fn resource_kind(system_id: &str) -> Option<ResourceKind> {
    system_skills(system_id).map(|profile| profile.resource)
}

pub fn skill_by_id(skill_id: &str) -> Option<SkillDefinition> {
    SYSTEM_IDS
        .into_iter()
        .flat_map(skills_for_system)
        .find(|skill| skill.id == skill_id)
}

pub fn default_loadout(
    system_id: &str,
    tier: u8,
) -> (
    Vec<SkillDefinition>,
    Vec<SkillDefinition>,
    Option<SkillDefinition>,
) {
    let skills = skills_for_system(system_id);
    let active = skills
        .iter()
        .filter(|skill| skill.category == SkillCategory::Active && skill.unlock_tier <= tier)
        .take(active_slot_capacity(tier))
        .cloned()
        .collect();
    let passive = skills
        .iter()
        .filter(|skill| skill.category == SkillCategory::Passive && skill.unlock_tier <= tier)
        .take(passive_slot_capacity(tier))
        .cloned()
        .collect();
    let domain = skills
        .into_iter()
        .find(|skill| skill.category == SkillCategory::Domain && skill.unlock_tier <= tier);
    (active, passive, domain)
}

fn build_skills(profile: SystemSkills) -> Vec<SkillDefinition> {
    let mut skills = vec![
        active_skill(
            &profile,
            "opening",
            profile.names[0],
            0,
            8_500,
            12,
            0,
            vec![damage_effect(
                profile.damage_type,
                9_000,
                10,
                true,
                true,
                true,
            )],
            vec![SkillTag::Attack],
        ),
        active_skill(
            &profile,
            "guard",
            profile.names[1],
            0,
            9_000,
            15,
            2,
            defense_effects(profile.id),
            defense_tags(profile.id),
        ),
        active_skill(
            &profile,
            "recovery",
            profile.names[2],
            1,
            10_000,
            20,
            3,
            recovery_effects(profile.id),
            vec![SkillTag::Healing],
        ),
        active_skill(
            &profile,
            "control",
            profile.names[3],
            2,
            11_000,
            28,
            4,
            control_effects(profile.id, profile.damage_type),
            vec![SkillTag::Control, SkillTag::Attack],
        ),
        active_skill(
            &profile,
            "mobility",
            profile.names[4],
            3,
            7_500,
            18,
            3,
            mobility_effects(profile.id),
            mobility_tags(profile.id),
        ),
        active_skill(
            &profile,
            "burst",
            profile.names[5],
            4,
            12_000,
            42,
            6,
            vec![damage_effect(
                profile.damage_type,
                17_000,
                35,
                true,
                true,
                true,
            )],
            vec![SkillTag::Attack, SkillTag::Charge],
        ),
        active_skill(
            &profile,
            "ultimate",
            profile.names[6],
            6,
            15_000,
            65,
            10,
            ultimate_effects(profile.id, profile.damage_type),
            vec![SkillTag::Attack, SkillTag::Charge, SkillTag::Ultimate],
        ),
        active_skill(
            &profile,
            "transcendent",
            profile.names[7],
            8,
            13_500,
            80,
            12,
            transcendent_effects(profile.id, profile.damage_type),
            vec![SkillTag::Attack, SkillTag::Ultimate],
        ),
        passive_skill(&profile, "instinct", profile.names[8], 1),
        passive_skill(&profile, "mastery", profile.names[9], 5),
        domain_skill(&profile, profile.names[10]),
    ];

    if profile.id == "summoner" {
        skills[0].effects.insert(
            0,
            SkillEffect::Summon {
                definition_id: "spirit_companion".into(),
                health_basis_points: 3_500,
                attack_basis_points: 3_000,
                duration: 80,
            },
        );
        skills[0].tags.push(SkillTag::Summon);
    }
    skills
}

#[allow(clippy::too_many_arguments)]
fn active_skill(
    profile: &SystemSkills,
    suffix: &str,
    name: &str,
    unlock_tier: u8,
    action_cost: i64,
    resource_cost: i64,
    cooldown: u32,
    effects: Vec<SkillEffect>,
    tags: Vec<SkillTag>,
) -> SkillDefinition {
    SkillDefinition {
        id: format!("{}.{}", profile.id, suffix),
        name: name.into(),
        system_id: profile.id.into(),
        category: SkillCategory::Active,
        unlock_tier,
        action_cost,
        resource_cost,
        cooldown,
        cast_time: u32::from(tags.contains(&SkillTag::Charge)) * 3,
        min_range: 0,
        max_range: skill_range(profile.id, &tags),
        target: if tags.contains(&SkillTag::Healing)
            || (tags.contains(&SkillTag::Defense) && !tags.contains(&SkillTag::Movement))
        {
            TargetRule::SelfTarget
        } else {
            TargetRule::SingleEnemy
        },
        tags,
        effects,
        mastery: 0,
    }
}

fn passive_skill(
    profile: &SystemSkills,
    suffix: &str,
    name: &str,
    unlock_tier: u8,
) -> SkillDefinition {
    SkillDefinition {
        id: format!("{}.{}", profile.id, suffix),
        name: name.into(),
        system_id: profile.id.into(),
        category: SkillCategory::Passive,
        unlock_tier,
        action_cost: 0,
        resource_cost: 0,
        cooldown: 0,
        cast_time: 0,
        min_range: 0,
        max_range: 0,
        target: TargetRule::SelfTarget,
        tags: vec![SkillTag::Defense],
        effects: vec![SkillEffect::Status {
            status: StatusKind::DefenseUp,
            magnitude_basis_points: if unlock_tier < 5 { 500 } else { 900 },
            duration: u32::MAX,
        }],
        mastery: 0,
    }
}

fn domain_skill(profile: &SystemSkills, name: &str) -> SkillDefinition {
    SkillDefinition {
        id: format!("{}.domain", profile.id),
        name: name.into(),
        system_id: profile.id.into(),
        category: SkillCategory::Domain,
        unlock_tier: 3,
        action_cost: 15_000,
        resource_cost: 70,
        cooldown: 18,
        cast_time: 5,
        min_range: 0,
        max_range: 12,
        target: TargetRule::SelfTarget,
        tags: vec![SkillTag::Domain, SkillTag::Charge],
        effects: vec![SkillEffect::Domain {
            strength: 120,
            duration: 36,
        }],
        mastery: 0,
    }
}

fn damage_effect(
    damage_type: DamageType,
    power_basis_points: i64,
    flat: i64,
    can_critical: bool,
    can_dodge: bool,
    blockable: bool,
) -> SkillEffect {
    SkillEffect::Damage {
        damage_type,
        power_basis_points,
        flat,
        can_critical,
        can_dodge,
        blockable,
    }
}

fn defense_effects(system_id: &str) -> Vec<SkillEffect> {
    match system_id {
        "sword" | "mage" | "music" => vec![SkillEffect::Dodge {
            charges: 1,
            duration: 16,
        }],
        "body" | "qi" => vec![SkillEffect::Block {
            reduction_basis_points: 6_000,
            charges: 2,
            duration: 20,
        }],
        _ => vec![SkillEffect::Shield {
            power_basis_points: 4_500,
            duration: 24,
        }],
    }
}

fn defense_tags(system_id: &str) -> Vec<SkillTag> {
    match system_id {
        "sword" | "mage" | "music" => vec![SkillTag::Defense, SkillTag::Dodge],
        "body" | "qi" => vec![SkillTag::Defense, SkillTag::Block],
        _ => vec![SkillTag::Defense, SkillTag::Shield],
    }
}

fn recovery_effects(system_id: &str) -> Vec<SkillEffect> {
    match system_id {
        "blood_demon" => vec![
            damage_effect(DamageType::Physical, 7_000, 0, false, true, true),
            SkillEffect::Heal {
                power_basis_points: 3_500,
                flat: 20,
            },
        ],
        "sword" => vec![
            SkillEffect::RestoreResource { amount: 38 },
            SkillEffect::Dodge {
                charges: 1,
                duration: 12,
            },
        ],
        "formation" => vec![
            SkillEffect::RestoreResource { amount: 32 },
            SkillEffect::Shield {
                power_basis_points: 3_000,
                duration: 18,
            },
        ],
        _ => vec![
            SkillEffect::Heal {
                power_basis_points: if matches!(system_id, "music" | "alchemy_artifact") {
                    6_000
                } else {
                    4_500
                },
                flat: 25,
            },
            SkillEffect::RestoreResource { amount: 25 },
        ],
    }
}

fn control_effects(system_id: &str, damage_type: DamageType) -> Vec<SkillEffect> {
    let strength = if matches!(system_id, "soul" | "formation" | "music") {
        145
    } else {
        105
    };
    vec![
        damage_effect(damage_type, 7_500, 5, false, true, true),
        SkillEffect::Control {
            strength,
            duration: 10,
        },
    ]
}

fn mobility_effects(system_id: &str) -> Vec<SkillEffect> {
    match system_id {
        "body" | "sword" | "qi" | "mage" => vec![
            SkillEffect::Move { distance_delta: -3 },
            SkillEffect::Dodge {
                charges: 1,
                duration: 10,
            },
        ],
        _ => vec![
            SkillEffect::Cleanse { count: 2 },
            SkillEffect::Move { distance_delta: 2 },
        ],
    }
}

fn mobility_tags(system_id: &str) -> Vec<SkillTag> {
    if matches!(system_id, "body" | "sword" | "qi" | "mage") {
        vec![SkillTag::Movement, SkillTag::Dodge]
    } else {
        vec![SkillTag::Movement, SkillTag::Cleanse]
    }
}

fn ultimate_effects(system_id: &str, damage_type: DamageType) -> Vec<SkillEffect> {
    let mut effects = vec![damage_effect(damage_type, 23_000, 60, true, false, true)];
    if matches!(system_id, "formation" | "soul" | "music") {
        effects.push(SkillEffect::Control {
            strength: 190,
            duration: 14,
        });
    }
    effects
}

fn transcendent_effects(system_id: &str, damage_type: DamageType) -> Vec<SkillEffect> {
    let mut effects = vec![damage_effect(damage_type, 28_000, 90, true, false, false)];
    if matches!(system_id, "orthodox" | "mage" | "alchemy_artifact") {
        effects.push(SkillEffect::Shield {
            power_basis_points: 7_000,
            duration: 30,
        });
    }
    effects
}

fn skill_range(system_id: &str, tags: &[SkillTag]) -> i32 {
    if tags.contains(&SkillTag::Defense) || tags.contains(&SkillTag::Healing) {
        0
    } else if matches!(
        system_id,
        "mage" | "soul" | "formation" | "music" | "summoner"
    ) {
        12
    } else if matches!(system_id, "orthodox" | "alchemy_artifact") {
        9
    } else {
        4
    }
}

fn system_skills(system_id: &str) -> Option<SystemSkills> {
    Some(match system_id {
        "orthodox" => SystemSkills {
            id: "orthodox",
            resource: ResourceKind::SpiritualEnergy,
            damage_type: DamageType::Arcane,
            names: [
                "御气诀",
                "法宝护体",
                "周天回灵",
                "定身符",
                "五行遁",
                "天雷引",
                "元神法相",
                "天地敕令",
                "清静道心",
                "万法归一",
                "五行道域",
            ],
        },
        "sword" => SystemSkills {
            id: "sword",
            resource: ResourceKind::SwordIntent,
            damage_type: DamageType::Physical,
            names: [
                "流光斩",
                "听风架势",
                "纳剑归意",
                "破甲剑痕",
                "瞬身",
                "蓄意绝锋",
                "万剑归宗",
                "一剑开天",
                "剑心通明",
                "人剑合一",
                "无极剑域",
            ],
        },
        "body" => SystemSkills {
            id: "body",
            resource: ResourceKind::BattleWill,
            damage_type: DamageType::Physical,
            names: [
                "崩山拳",
                "金刚架",
                "气血归元",
                "震地擒拿",
                "踏岳",
                "撼天重击",
                "不灭战躯",
                "力破万法",
                "铜皮铁骨",
                "战意不熄",
                "镇岳武域",
            ],
        },
        "mage" => SystemSkills {
            id: "mage",
            resource: ResourceKind::Mana,
            damage_type: DamageType::Arcane,
            names: [
                "元素飞矢",
                "相位屏障",
                "魔力潮汐",
                "冰霜禁锢",
                "闪现",
                "陨星咏唱",
                "元素风暴",
                "终焉星落",
                "元素亲和",
                "奥术洪流",
                "元素界域",
            ],
        },
        "soul" => SystemSkills {
            id: "soul",
            resource: ResourceKind::SoulPower,
            damage_type: DamageType::Soul,
            names: [
                "神魂刺",
                "魂障",
                "凝神养魂",
                "灵魂震荡",
                "移魂步",
                "噬念冲击",
                "武魂真身",
                "万魂寂灭",
                "灵觉",
                "魂契共鸣",
                "幽冥魂域",
            ],
        },
        "qi" => SystemSkills {
            id: "qi",
            resource: ResourceKind::FightingEnergy,
            damage_type: DamageType::Physical,
            names: [
                "斗气斩",
                "斗气铠甲",
                "纳气归旋",
                "爆炎冲",
                "斗气化翼",
                "战技连环",
                "天阶斗技",
                "斗破苍穹",
                "气旋护体",
                "斗心燃烧",
                "斗帝领域",
            ],
        },
        "blood_demon" => SystemSkills {
            id: "blood_demon",
            resource: ResourceKind::BloodForce,
            damage_type: DamageType::Physical,
            names: [
                "血刃",
                "血茧",
                "噬血归身",
                "血缚",
                "化血影",
                "血祭爆发",
                "不死秘法",
                "血海葬天",
                "血脉沸腾",
                "魔血再生",
                "无尽血域",
            ],
        },
        "formation" => SystemSkills {
            id: "formation",
            resource: ResourceKind::FormationPoints,
            damage_type: DamageType::Arcane,
            names: [
                "引灵阵纹",
                "阵眼守护",
                "聚灵回转",
                "困龙阵",
                "移阵换位",
                "杀伐阵",
                "周天大阵",
                "万象封界",
                "地势感应",
                "阵心不灭",
                "山河阵域",
            ],
        },
        "alchemy_artifact" => SystemSkills {
            id: "alchemy_artifact",
            resource: ResourceKind::ArtifactEnergy,
            damage_type: DamageType::Arcane,
            names: [
                "灵器轰击",
                "护身宝甲",
                "回元丹",
                "缚灵器",
                "傀儡换位",
                "神兵共振",
                "九转金丹",
                "万器朝宗",
                "炉火纯青",
                "器魂共鸣",
                "造化炉域",
            ],
        },
        "summoner" => SystemSkills {
            id: "summoner",
            resource: ResourceKind::ContractPower,
            damage_type: DamageType::Arcane,
            names: [
                "灵兽契召",
                "护主契约",
                "召回疗愈",
                "群兽牵制",
                "换位契印",
                "契约超载",
                "万灵奔袭",
                "神兽降世",
                "心灵联结",
                "主从同调",
                "万灵契域",
            ],
        },
        "music" => SystemSkills {
            id: "music",
            resource: ResourceKind::Melody,
            damage_type: DamageType::Soul,
            names: [
                "裂弦音",
                "流拍幻步",
                "回春小调",
                "乱心曲",
                "转调移宫",
                "惊涛变奏",
                "四章合奏",
                "大道希声",
                "余音绕梁",
                "曲心通明",
                "天籁乐域",
            ],
        },
        _ => return None,
    })
}
