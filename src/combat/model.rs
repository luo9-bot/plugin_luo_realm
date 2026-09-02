use serde::{Deserialize, Serialize};

pub const BASIS_POINTS: i64 = 10_000;
pub const ACTION_THRESHOLD: i64 = 10_000;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    Physical,
    Arcane,
    Soul,
    True,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillCategory {
    Active,
    Passive,
    Domain,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillTag {
    Attack,
    Charge,
    Defense,
    Healing,
    Shield,
    Block,
    Dodge,
    Control,
    Cleanse,
    Movement,
    Domain,
    Summon,
    Ultimate,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetRule {
    SelfTarget,
    SingleEnemy,
    AllEnemies,
    LowestHealthAlly,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    SpiritualEnergy,
    SwordIntent,
    BattleWill,
    Mana,
    SoulPower,
    FightingEnergy,
    BloodForce,
    FormationPoints,
    ArtifactEnergy,
    ContractPower,
    Melody,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tactic {
    Balanced,
    Aggressive,
    Defensive,
    Sustain,
    Control,
}

impl Tactic {
    pub fn code(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::Aggressive => "aggressive",
            Self::Defensive => "defensive",
            Self::Sustain => "sustain",
            Self::Control => "control",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "balanced" => Some(Self::Balanced),
            "aggressive" => Some(Self::Aggressive),
            "defensive" => Some(Self::Defensive),
            "sustain" => Some(Self::Sustain),
            "control" => Some(Self::Control),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Balanced => "均衡",
            Self::Aggressive => "强攻",
            Self::Defensive => "守御",
            Self::Sustain => "续航",
            Self::Control => "控制",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillEffect {
    Damage {
        damage_type: DamageType,
        power_basis_points: i64,
        flat: i64,
        can_critical: bool,
        can_dodge: bool,
        blockable: bool,
    },
    Heal {
        power_basis_points: i64,
        flat: i64,
    },
    RestoreResource {
        amount: i64,
    },
    Shield {
        power_basis_points: i64,
        duration: u32,
    },
    Block {
        reduction_basis_points: i64,
        charges: u8,
        duration: u32,
    },
    Dodge {
        charges: u8,
        duration: u32,
    },
    Move {
        distance_delta: i32,
    },
    Control {
        strength: i64,
        duration: u32,
    },
    Cleanse {
        count: u8,
    },
    Status {
        status: StatusKind,
        magnitude_basis_points: i64,
        duration: u32,
    },
    Summon {
        definition_id: String,
        health_basis_points: i64,
        attack_basis_points: i64,
        duration: u32,
    },
    Domain {
        strength: i64,
        duration: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusKind {
    AttackUp,
    DefenseUp,
    SpeedUp,
    DamageOverTime,
    HealingOverTime,
    HealingSuppression,
    Stunned,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub system_id: String,
    pub category: SkillCategory,
    pub unlock_tier: u8,
    pub action_cost: i64,
    pub resource_cost: i64,
    pub cooldown: u32,
    pub cast_time: u32,
    pub min_range: i32,
    pub max_range: i32,
    pub target: TargetRule,
    pub tags: Vec<SkillTag>,
    pub effects: Vec<SkillEffect>,
    #[serde(default)]
    pub mastery: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillVisualConfig {
    pub primary_color: String,
    pub secondary_color: String,
    pub icon_asset: Option<String>,
    pub effect_asset: Option<String>,
    pub arc_style: String,
    pub arc_width: u32,
    pub arc_duration: u32,
    pub flash_color: String,
}

impl Default for SkillVisualConfig {
    fn default() -> Self {
        Self {
            primary_color: "#00b4d8".into(),
            secondary_color: "#48cae4".into(),
            icon_asset: None,
            effect_asset: None,
            arc_style: "sweep".into(),
            arc_width: 12,
            arc_duration: 8,
            flash_color: "#00b4d8".into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CombatAttributes {
    pub max_health: i64,
    pub attack: i64,
    pub physical_defense: i64,
    pub arcane_defense: i64,
    pub soul_defense: i64,
    pub speed: i64,
    pub critical_rate_basis_points: i64,
    pub critical_damage_basis_points: i64,
    pub recovery_power: i64,
    pub control_power: i64,
    pub tenacity: i64,
    pub domain_power: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    pub kind: ResourceKind,
    pub current: i64,
    pub maximum: i64,
    pub regeneration: i64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentSlot {
    MainHand,
    OffHand,
    Head,
    Body,
    Hands,
    Feet,
    AccessoryOne,
    AccessoryTwo,
}

impl EquipmentSlot {
    pub fn code(self) -> &'static str {
        match self {
            Self::MainHand => "main_hand",
            Self::OffHand => "off_hand",
            Self::Head => "head",
            Self::Body => "body",
            Self::Hands => "hands",
            Self::Feet => "feet",
            Self::AccessoryOne => "accessory_1",
            Self::AccessoryTwo => "accessory_2",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "main_hand" => Some(Self::MainHand),
            "off_hand" => Some(Self::OffHand),
            "head" => Some(Self::Head),
            "body" => Some(Self::Body),
            "hands" => Some(Self::Hands),
            "feet" => Some(Self::Feet),
            "accessory_1" => Some(Self::AccessoryOne),
            "accessory_2" => Some(Self::AccessoryTwo),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerCondition {
    BattleStarted,
    DamageTaken,
    HealthBelowHalf,
    ShieldBroken,
    DodgeSucceeded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EquipmentTrigger {
    pub source_item_id: i64,
    pub source_name: String,
    pub condition: TriggerCondition,
    pub once_per_battle: bool,
    pub effect: SkillEffect,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CombatantSnapshot {
    pub combatant_id: String,
    pub player_id: Option<u64>,
    pub display_name: String,
    pub avatar_id: String,
    pub system_id: String,
    pub universal_tier: u8,
    pub team: u8,
    pub position: i32,
    pub attributes: CombatAttributes,
    pub resource: ResourceSnapshot,
    pub active_skills: Vec<SkillDefinition>,
    pub passive_skills: Vec<SkillDefinition>,
    pub domain_skill: Option<SkillDefinition>,
    pub equipment_triggers: Vec<EquipmentTrigger>,
    pub tactic: Tactic,
    pub power: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BattleRules {
    pub max_ticks: u32,
    pub max_trigger_depth: u8,
    pub initial_distance: i32,
}

impl Default for BattleRules {
    fn default() -> Self {
        Self {
            max_ticks: 2_000,
            max_trigger_depth: 16,
            initial_distance: 6,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CombatSnapshot {
    pub rule_version: u32,
    pub seed: u64,
    pub rules: BattleRules,
    pub combatants: Vec<CombatantSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CombatEvent {
    pub sequence: u64,
    pub tick: u32,
    pub source_id: Option<String>,
    pub target_id: Option<String>,
    pub trigger_chain: u64,
    pub kind: CombatEventKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CombatEventKind {
    BattleStarted,
    ActionPrepared {
        skill_id: String,
        skill_name: String,
    },
    ActionInterrupted {
        skill_name: String,
    },
    SkillCast {
        skill_id: String,
        skill_name: String,
        tags: Vec<SkillTag>,
    },
    Moved {
        from: i32,
        to: i32,
    },
    Dodged,
    Blocked {
        prevented: i64,
    },
    ShieldChanged {
        delta: i64,
        remaining: i64,
    },
    DamageApplied {
        amount: i64,
        critical: bool,
        damage_type: DamageType,
    },
    HealingApplied {
        amount: i64,
    },
    ResourceChanged {
        kind: ResourceKind,
        delta: i64,
        remaining: i64,
    },
    StatusApplied {
        status: StatusKind,
        duration: u32,
    },
    StatusRemoved {
        status: StatusKind,
    },
    ControlResisted {
        tenacity_remaining: i64,
    },
    ControlBroken,
    EntitySummoned {
        definition_id: String,
        display_name: String,
    },
    EntityDefeated,
    DomainEstablished {
        skill_id: String,
        strength: i64,
    },
    DomainContested {
        winner_id: String,
    },
    DomainCollapsed,
    PassiveTriggered {
        definition_id: String,
        name: String,
    },
    EquipmentTriggered {
        item_id: i64,
        item_name: String,
    },
    BattleEnded {
        winner_team: u8,
        reason: BattleEndReason,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BattleEndReason {
    Defeated,
    Timeout,
    Objective,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CombatantOutcome {
    pub combatant_id: String,
    pub team: u8,
    pub health: i64,
    pub max_health: i64,
    pub damage_dealt: i64,
    pub healing_done: i64,
    pub defeated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CombatOutcome {
    pub seed: u64,
    pub winner_team: u8,
    pub end_reason: BattleEndReason,
    pub elapsed_ticks: u32,
    pub events: Vec<CombatEvent>,
    pub combatants: Vec<CombatantOutcome>,
}

#[derive(Debug, thiserror::Error)]
pub enum CombatError {
    #[error("战斗至少需要两个阵营")]
    MissingTeams,
    #[error("战斗单位标识重复：{0}")]
    DuplicateCombatant(String),
    #[error("战斗单位没有可用主动技能：{0}")]
    EmptyLoadout(String),
    #[error("战斗规则无效：{0}")]
    InvalidRules(String),
    #[error("战斗运行时状态错误：{0}")]
    InvalidState(String),
}
