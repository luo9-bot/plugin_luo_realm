#![allow(clippy::drop_non_drop, clippy::collapsible_if)]

use std::collections::{HashMap, HashSet};

use bevy_ecs::prelude::*;

use crate::domain::shared::{CombatantId, PlatformUserId, SkillId, SystemId};

use super::{
    ACTION_THRESHOLD, BASIS_POINTS, BattleEndReason, CombatError, CombatEvent, CombatEventKind,
    CombatOutcome, CombatSnapshot, CombatantOutcome, DamageType, EquipmentTrigger, ResourceKind,
    SkillDefinition, SkillEffect, SkillTag, StatusKind, Tactic, TargetRule, TriggerCondition,
};

#[allow(dead_code)]
#[derive(Component)]
struct Identity {
    id: CombatantId,
    name: String,
    character_id: String,
    system_id: SystemId,
    platform_user_id: Option<PlatformUserId>,
    team: u8,
}

#[derive(Component)]
struct Unit {
    max_health: i64,
    health: i64,
    attack: i64,
    physical_defense: i64,
    arcane_defense: i64,
    soul_defense: i64,
    speed: i64,
    critical_rate: i64,
    critical_damage: i64,
    recovery_power: i64,
    control_power: i64,
    max_tenacity: i64,
    tenacity: i64,
    domain_power: i64,
    damage_dealt: i64,
    healing_done: i64,
}

#[derive(Component)]
struct ResourcePool {
    kind: ResourceKind,
    current: i64,
    maximum: i64,
    regeneration: i64,
}

#[derive(Component, Default)]
struct Gauge(i64);

#[derive(Component)]
struct Location(i32);

#[derive(Component)]
struct Loadout {
    active: Vec<SkillDefinition>,
    domain: Option<SkillDefinition>,
}

#[derive(Component, Default)]
struct Cooldowns(HashMap<SkillId, u32>);

#[derive(Component)]
struct Tactics(Tactic);

#[derive(Component, Default)]
struct Defenses {
    shield: i64,
    shield_expires: u32,
    block_reduction: i64,
    block_charges: u8,
    block_expires: u32,
    dodge_charges: u8,
    dodge_expires: u32,
    healing_suppression: i64,
    stunned_until: u32,
    control_resistance_stacks: u8,
}

#[derive(Component)]
struct Equipment {
    triggers: Vec<RuntimeTrigger>,
}

struct RuntimeTrigger {
    definition: EquipmentTrigger,
    used: bool,
}

#[derive(Component)]
struct Casting {
    skill: SkillDefinition,
    target: Entity,
    completes_at: u32,
    trigger_chain: u64,
}

#[derive(Component)]
struct Summoned {
    expires_at: u32,
}

#[derive(Component)]
struct OngoingEffect {
    source: Entity,
    target: Entity,
    kind: StatusKind,
    magnitude: i64,
    next_tick: u32,
    expires_at: u32,
    trigger_chain: u64,
}

#[allow(dead_code)]
#[derive(Component)]
struct DomainEffect {
    owner: Entity,
    team: u8,
    skill_id: SkillId,
    strength: i64,
    expires_at: u32,
}

#[derive(Resource)]
struct Clock {
    tick: u32,
    max_ticks: u32,
}

#[derive(Resource)]
struct RandomState(u64);

impl RandomState {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn basis_points(&mut self, minimum: i64, maximum: i64) -> i64 {
        let width = (maximum - minimum + 1).max(1) as u64;
        minimum + (self.next_u64() % width) as i64
    }
}

#[derive(Resource, Default)]
struct PendingActions(Vec<PendingAction>);

struct PendingAction {
    actor: Entity,
    target: Entity,
    skill: SkillDefinition,
    trigger_chain: u64,
}

#[derive(Resource, Default)]
struct EventLog {
    next_sequence: u64,
    next_chain: u64,
    events: Vec<CombatEvent>,
}

#[derive(Resource)]
struct RuntimeConfig {
    max_trigger_depth: u8,
}

#[derive(Resource, Default)]
struct BattleState {
    finished: bool,
    winner_team: u8,
    reason: Option<BattleEndReason>,
}

pub fn run_battle(snapshot: &CombatSnapshot) -> Result<CombatOutcome, CombatError> {
    validate_snapshot(snapshot)?;
    let mut world = build_world(snapshot);
    emit(&mut world, None, None, 0, CombatEventKind::BattleStarted);
    trigger_battle_started_equipment(&mut world);

    let mut schedule = Schedule::default();
    schedule.add_systems(
        (
            advance_phase,
            action_selection_phase,
            action_resolution_phase,
            cleanup_phase,
        )
            .chain(),
    );
    while !world.resource::<BattleState>().finished {
        schedule.run(&mut world);
    }
    build_outcome(&mut world, snapshot.seed)
}

fn validate_snapshot(snapshot: &CombatSnapshot) -> Result<(), CombatError> {
    if snapshot.rules.max_ticks == 0 || snapshot.rules.max_trigger_depth == 0 {
        return Err(CombatError::InvalidRules(
            "时间片和触发深度必须大于零".into(),
        ));
    }
    let mut ids = HashSet::new();
    let teams = snapshot
        .combatants
        .iter()
        .map(|combatant| combatant.team)
        .collect::<HashSet<_>>();
    if teams.len() < 2 {
        return Err(CombatError::MissingTeams);
    }
    snapshot.combatants.iter().try_for_each(|combatant| {
        if !ids.insert(combatant.combatant_id.as_str()) {
            return Err(CombatError::DuplicateCombatant(
                combatant.combatant_id.clone(),
            ));
        }
        if combatant.active_skills.is_empty() {
            return Err(CombatError::EmptyLoadout(combatant.combatant_id.clone()));
        }
        Ok(())
    })
}

fn build_world(snapshot: &CombatSnapshot) -> World {
    let mut world = World::new();
    world.insert_resource(Clock {
        tick: 0,
        max_ticks: snapshot.rules.max_ticks,
    });
    world.insert_resource(RandomState(snapshot.seed.max(1)));
    world.insert_resource(PendingActions::default());
    world.insert_resource(EventLog::default());
    world.insert_resource(RuntimeConfig {
        max_trigger_depth: snapshot.rules.max_trigger_depth,
    });
    world.insert_resource(BattleState::default());

    snapshot.combatants.iter().for_each(|combatant| {
        let attributes = &combatant.attributes;
        let entity = world
            .spawn((
                Identity {
                    id: combatant.combatant_id.clone(),
                    name: combatant.display_name.clone(),
                    character_id: combatant.character_id.clone(),
                    system_id: combatant.system_id.clone(),
                    platform_user_id: combatant.platform_user_id,
                    team: combatant.team,
                },
                Unit {
                    max_health: attributes.max_health,
                    health: attributes.max_health,
                    attack: attributes.attack,
                    physical_defense: attributes.physical_defense,
                    arcane_defense: attributes.arcane_defense,
                    soul_defense: attributes.soul_defense,
                    speed: attributes.speed,
                    critical_rate: attributes.critical_rate_basis_points,
                    critical_damage: attributes.critical_damage_basis_points,
                    recovery_power: attributes.recovery_power,
                    control_power: attributes.control_power,
                    max_tenacity: attributes.tenacity,
                    tenacity: attributes.tenacity,
                    domain_power: attributes.domain_power,
                    damage_dealt: 0,
                    healing_done: 0,
                },
                ResourcePool {
                    kind: combatant.resource.kind,
                    current: combatant.resource.current,
                    maximum: combatant.resource.maximum,
                    regeneration: combatant.resource.regeneration,
                },
                Gauge::default(),
                Location(combatant.position),
                Loadout {
                    active: combatant.active_skills.clone(),
                    domain: combatant.domain_skill.clone(),
                },
                Cooldowns::default(),
                Tactics(combatant.tactic),
                Defenses::default(),
                Equipment {
                    triggers: combatant
                        .equipment_triggers
                        .iter()
                        .cloned()
                        .map(|definition| RuntimeTrigger {
                            definition,
                            used: false,
                        })
                        .collect(),
                },
            ))
            .id();
        apply_passives(&mut world, entity, &combatant.passive_skills);
    });
    world
}

fn apply_passives(world: &mut World, entity: Entity, passives: &[SkillDefinition]) {
    passives.iter().for_each(|skill| {
        skill.effects.iter().for_each(|effect| {
            apply_passive_effect(world, entity, effect);
        });
        emit(
            world,
            Some(entity),
            Some(entity),
            0,
            CombatEventKind::PassiveTriggered {
                definition_id: skill.id.clone(),
                name: skill.name.clone(),
            },
        );
    });
}

fn apply_passive_effect(world: &mut World, entity: Entity, effect: &SkillEffect) {
    let SkillEffect::Status {
        status,
        magnitude_basis_points,
        ..
    } = effect
    else {
        return;
    };
    if let Some(mut unit) = world.get_mut::<Unit>(entity) {
        match status {
            StatusKind::AttackUp => {
                unit.attack += unit.attack * magnitude_basis_points / BASIS_POINTS;
            }
            StatusKind::DefenseUp => {
                unit.physical_defense +=
                    unit.physical_defense * magnitude_basis_points / BASIS_POINTS;
                unit.arcane_defense += unit.arcane_defense * magnitude_basis_points / BASIS_POINTS;
                unit.soul_defense += unit.soul_defense * magnitude_basis_points / BASIS_POINTS;
            }
            StatusKind::SpeedUp => {
                unit.speed += unit.speed * magnitude_basis_points / BASIS_POINTS;
            }
            _ => {}
        }
    }
}

fn advance_phase(world: &mut World) {
    let tick = {
        let mut clock = world.resource_mut::<Clock>();
        clock.tick += 1;
        clock.tick
    };
    advance_units(world, tick);
    advance_ongoing_effects(world, tick);
    complete_casts(world, tick);
    expire_domains(world, tick);
}

fn advance_units(world: &mut World, tick: u32) {
    let mut query = world.query::<(
        &mut Unit,
        &mut Gauge,
        &mut ResourcePool,
        &mut Cooldowns,
        &mut Defenses,
        Option<&Casting>,
    )>();
    query.iter_mut(world).for_each(
        |(mut unit, mut gauge, mut resource, mut cooldowns, mut defenses, casting)| {
            cooldowns.0.values_mut().for_each(|remaining| {
                *remaining = remaining.saturating_sub(1);
            });
            if tick.is_multiple_of(10) {
                resource.current = (resource.current + resource.regeneration).min(resource.maximum);
                if defenses.control_resistance_stacks > 0 {
                    defenses.control_resistance_stacks -= 1;
                }
                defenses.tenacity_recovery(&mut unit);
            }
            if defenses.shield_expires <= tick {
                defenses.shield = 0;
            }
            if defenses.block_expires <= tick {
                defenses.block_charges = 0;
            }
            if defenses.dodge_expires <= tick {
                defenses.dodge_charges = 0;
            }
            if unit.health > 0 && defenses.stunned_until <= tick && casting.is_none() {
                gauge.0 = gauge.0.saturating_add(speed_progress(unit.speed));
            }
        },
    );
}

impl Defenses {
    fn tenacity_recovery(&mut self, unit: &mut Unit) {
        self.healing_suppression = self.healing_suppression.saturating_sub(250);
        let recovered = (unit.max_tenacity / 20).max(1);
        unit.tenacity = (unit.tenacity + recovered).min(unit.max_tenacity);
    }
}

fn speed_progress(speed: i64) -> i64 {
    let speed = speed.max(1);
    80 + (speed * 40_000) / (speed + 80)
}

fn advance_ongoing_effects(world: &mut World, tick: u32) {
    let pending = {
        let mut query = world.query::<(Entity, &OngoingEffect)>();
        query
            .iter(world)
            .filter(|(_, effect)| effect.next_tick <= tick)
            .map(|(entity, effect)| {
                (
                    entity,
                    effect.source,
                    effect.target,
                    effect.kind,
                    effect.magnitude,
                    effect.expires_at,
                    effect.trigger_chain,
                )
            })
            .collect::<Vec<_>>()
    };
    pending.into_iter().for_each(
        |(effect_entity, source, target, kind, magnitude, expires_at, chain)| {
            match kind {
                StatusKind::DamageOverTime => {
                    apply_direct_damage(
                        world,
                        source,
                        target,
                        magnitude,
                        DamageType::True,
                        chain,
                        0,
                    );
                }
                StatusKind::HealingOverTime => {
                    apply_heal(world, source, target, magnitude, 0, chain);
                }
                _ => {}
            }
            if tick >= expires_at {
                let _ = world.despawn(effect_entity);
                emit(
                    world,
                    Some(source),
                    Some(target),
                    chain,
                    CombatEventKind::StatusRemoved { status: kind },
                );
            } else if let Some(mut effect) = world.get_mut::<OngoingEffect>(effect_entity) {
                effect.next_tick = tick + 10;
            }
        },
    );
}

fn complete_casts(world: &mut World, tick: u32) {
    let completed = {
        let mut query = world.query::<(Entity, &Casting)>();
        query
            .iter(world)
            .filter(|(_, casting)| casting.completes_at <= tick)
            .map(|(entity, casting)| PendingAction {
                actor: entity,
                target: casting.target,
                skill: casting.skill.clone(),
                trigger_chain: casting.trigger_chain,
            })
            .collect::<Vec<_>>()
    };
    completed.iter().for_each(|action| {
        world.entity_mut(action.actor).remove::<Casting>();
    });
    world.resource_mut::<PendingActions>().0.extend(completed);
}

fn expire_domains(world: &mut World, tick: u32) {
    let expired = {
        let mut query = world.query::<(Entity, &DomainEffect)>();
        query
            .iter(world)
            .filter(|(_, domain)| domain.expires_at <= tick)
            .map(|(entity, domain)| (entity, domain.owner))
            .collect::<Vec<_>>()
    };
    expired.into_iter().for_each(|(entity, owner)| {
        let _ = world.despawn(entity);
        emit(
            world,
            Some(owner),
            None,
            0,
            CombatEventKind::DomainCollapsed,
        );
    });
}

fn action_selection_phase(world: &mut World) {
    let actors = sorted_living_units(world)
        .into_iter()
        .filter(|entity| {
            world
                .get::<Gauge>(*entity)
                .is_some_and(|gauge| gauge.0 >= ACTION_THRESHOLD)
                && world.get::<Casting>(*entity).is_none()
        })
        .collect::<Vec<_>>();
    actors.into_iter().for_each(|actor| {
        if let Some((skill, target)) = choose_action(world, actor) {
            begin_action(world, actor, target, skill);
        }
    });
}

fn sorted_living_units(world: &mut World) -> Vec<Entity> {
    let mut query = world.query::<(Entity, &Identity, &Unit)>();
    let mut units = query
        .iter(world)
        .filter(|(_, _, unit)| unit.health > 0)
        .map(|(entity, identity, _)| (identity.id.clone(), entity))
        .collect::<Vec<_>>();
    units.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    units.into_iter().map(|(_, entity)| entity).collect()
}

fn choose_action(world: &mut World, actor: Entity) -> Option<(SkillDefinition, Entity)> {
    let team = world.get::<Identity>(actor)?.team;
    let unit = world.get::<Unit>(actor)?;
    let health_ratio = unit.health * BASIS_POINTS / unit.max_health.max(1);
    let resource_current = world.get::<ResourcePool>(actor)?.current;
    let loadout = world.get::<Loadout>(actor)?;
    let mut skills = loadout.active.clone();
    skills.extend(loadout.domain.iter().cloned());
    let cooldowns = world.get::<Cooldowns>(actor)?.0.clone();
    let tactic = world.get::<Tactics>(actor)?.0;
    let shield = world.get::<Defenses>(actor)?.shield;
    let actor_position = world.get::<Location>(actor)?.0;
    let selected = skills
        .into_iter()
        .filter_map(|skill| {
            if resource_current < skill.resource_cost
                || cooldowns.get(&skill.id).copied().unwrap_or(0) > 0
            {
                return None;
            }
            let target = target_for_skill(world, actor, team, actor_position, &skill)?;
            let score = skill_score(&skill, tactic, health_ratio, shield);
            Some((score, skill.id.clone(), skill, target))
        })
        .max_by(|left, right| left.0.cmp(&right.0).then_with(|| right.1.cmp(&left.1)))
        .map(|(_, _, skill, target)| (skill, target));
    selected.or_else(|| {
        let target = target_for_basic_attack(world, actor, team)?;
        Some((
            SkillDefinition {
                id: SkillId::new("basic_attack"),
                name: "普通攻击".into(),
                system_id: world.get::<Identity>(actor)?.system_id.clone(),
                category: super::SkillCategory::Active,
                unlock_tier: 0,
                action_cost: ACTION_THRESHOLD,
                resource_cost: 0,
                cooldown: 0,
                cast_time: 0,
                min_range: 0,
                max_range: i32::MAX,
                target: TargetRule::SingleEnemy,
                tags: vec![SkillTag::Attack],
                effects: vec![SkillEffect::Damage {
                    damage_type: DamageType::Physical,
                    power_basis_points: 7_000,
                    flat: 0,
                    can_critical: true,
                    can_dodge: true,
                    blockable: true,
                }],
                mastery: 0,
            },
            target,
        ))
    })
}

fn target_for_basic_attack(world: &mut World, actor: Entity, team: u8) -> Option<Entity> {
    let mut query = world.query::<(Entity, &Identity, &Unit)>();
    query
        .iter(world)
        .filter(|(entity, identity, unit)| {
            *entity != actor && identity.team != team && unit.health > 0
        })
        .map(|(entity, _, unit)| (unit.health, entity))
        .min_by_key(|(health, entity)| (*health, entity.to_bits()))
        .map(|(_, entity)| entity)
}

fn target_for_skill(
    world: &mut World,
    actor: Entity,
    team: u8,
    actor_position: i32,
    skill: &SkillDefinition,
) -> Option<Entity> {
    if skill.target == TargetRule::SelfTarget {
        return Some(actor);
    }
    let mut query = world.query::<(Entity, &Identity, &Unit, &Location)>();
    let mut candidates = query
        .iter(world)
        .filter(|(_, identity, unit, _)| identity.team != team && unit.health > 0)
        .filter(|(_, _, _, location)| {
            let distance = (location.0 - actor_position).abs();
            distance >= skill.min_range && distance <= skill.max_range
        })
        .map(|(entity, identity, unit, location)| {
            (
                unit.health,
                (location.0 - actor_position).abs(),
                identity.id.clone(),
                entity,
            )
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates.first().map(|candidate| candidate.3)
}

fn skill_score(skill: &SkillDefinition, tactic: Tactic, health_ratio: i64, shield: i64) -> i64 {
    let mut score = 100 - skill.resource_cost - i64::from(skill.cooldown);
    if skill.tags.contains(&SkillTag::Healing) {
        score += (BASIS_POINTS - health_ratio) / 50;
        if health_ratio > 8_000 {
            score -= 250;
        }
    }
    if skill.tags.contains(&SkillTag::Defense) {
        score += (BASIS_POINTS - health_ratio) / 80;
        if shield > 0 {
            score -= 80;
        }
    }
    score
        + match tactic {
            Tactic::Balanced => 0,
            Tactic::Aggressive if skill.tags.contains(&SkillTag::Attack) => 80,
            Tactic::Defensive if skill.tags.contains(&SkillTag::Defense) => 100,
            Tactic::Sustain if skill.tags.contains(&SkillTag::Healing) => 120,
            Tactic::Control if skill.tags.contains(&SkillTag::Control) => 120,
            _ => -20,
        }
}

fn begin_action(world: &mut World, actor: Entity, target: Entity, skill: SkillDefinition) {
    let chain = next_chain(world);
    if let Some(mut gauge) = world.get_mut::<Gauge>(actor) {
        gauge.0 = gauge.0.saturating_sub(skill.action_cost);
    }
    if let Some(mut resource) = world.get_mut::<ResourcePool>(actor) {
        resource.current = resource.current.saturating_sub(skill.resource_cost);
        let remaining = resource.current;
        let kind = resource.kind;
        emit(
            world,
            Some(actor),
            Some(actor),
            chain,
            CombatEventKind::ResourceChanged {
                kind,
                delta: -skill.resource_cost,
                remaining,
            },
        );
    }
    if let Some(mut cooldowns) = world.get_mut::<Cooldowns>(actor) {
        cooldowns.0.insert(skill.id.clone(), skill.cooldown);
    }
    if skill.cast_time > 0 {
        let tick = world.resource::<Clock>().tick;
        let cast_time = skill.cast_time;
        emit(
            world,
            Some(actor),
            Some(target),
            chain,
            CombatEventKind::ActionPrepared {
                skill_id: skill.id.clone(),
                skill_name: skill.name.clone(),
            },
        );
        world.entity_mut(actor).insert(Casting {
            skill,
            target,
            completes_at: tick + cast_time,
            trigger_chain: chain,
        });
    } else {
        world
            .resource_mut::<PendingActions>()
            .0
            .push(PendingAction {
                actor,
                target,
                skill,
                trigger_chain: chain,
            });
    }
}

fn action_resolution_phase(world: &mut World) {
    let actions = std::mem::take(&mut world.resource_mut::<PendingActions>().0);
    actions.into_iter().for_each(|action| {
        if !is_alive(world, action.actor) {
            return;
        }
        let targets = action_targets(world, &action);
        if targets.is_empty() {
            return;
        }
        emit(
            world,
            Some(action.actor),
            targets.first().copied(),
            action.trigger_chain,
            CombatEventKind::SkillCast {
                skill_id: action.skill.id.clone(),
                skill_name: action.skill.name.clone(),
                tags: action.skill.tags.clone(),
            },
        );
        targets.into_iter().for_each(|target| {
            action.skill.effects.iter().for_each(|effect| {
                let scaled = scale_effect(effect, action.skill.mastery);
                apply_effect(
                    world,
                    action.actor,
                    target,
                    &scaled,
                    action.trigger_chain,
                    0,
                );
            });
        });
    });
}

fn action_targets(world: &mut World, action: &PendingAction) -> Vec<Entity> {
    let Some(identity) = world.get::<Identity>(action.actor) else {
        return Vec::new();
    };
    let team = identity.team;
    match action.skill.target {
        TargetRule::SelfTarget => vec![action.actor],
        TargetRule::LowestHealthAlly => {
            let mut query = world.query::<(Entity, &Identity, &Unit)>();
            query
                .iter(world)
                .filter(|(_, target_identity, unit)| {
                    target_identity.team == team && unit.health > 0
                })
                .min_by_key(|(entity, _, unit)| (unit.health, entity.to_bits()))
                .map(|(entity, _, _)| vec![entity])
                .unwrap_or_default()
        }
        TargetRule::AllEnemies => {
            let mut query = world.query::<(Entity, &Identity, &Unit)>();
            query
                .iter(world)
                .filter(|(_, target_identity, unit)| {
                    target_identity.team != team && unit.health > 0
                })
                .map(|(entity, _, _)| entity)
                .collect()
        }
        TargetRule::SingleEnemy => is_alive(world, action.target)
            .then_some(action.target)
            .into_iter()
            .collect(),
    }
}

fn scale_effect(effect: &SkillEffect, mastery: u8) -> SkillEffect {
    let multiplier = BASIS_POINTS + i64::from(mastery.min(3)) * 500;
    match effect {
        SkillEffect::Damage {
            damage_type,
            power_basis_points,
            flat,
            can_critical,
            can_dodge,
            blockable,
        } => SkillEffect::Damage {
            damage_type: *damage_type,
            power_basis_points: power_basis_points * multiplier / BASIS_POINTS,
            flat: *flat,
            can_critical: *can_critical,
            can_dodge: *can_dodge,
            blockable: *blockable,
        },
        SkillEffect::Heal {
            power_basis_points,
            flat,
        } => SkillEffect::Heal {
            power_basis_points: power_basis_points * multiplier / BASIS_POINTS,
            flat: *flat,
        },
        SkillEffect::Shield {
            power_basis_points,
            duration,
        } => SkillEffect::Shield {
            power_basis_points: power_basis_points * multiplier / BASIS_POINTS,
            duration: *duration,
        },
        SkillEffect::Control { strength, duration } => SkillEffect::Control {
            strength: strength * multiplier / BASIS_POINTS,
            duration: *duration,
        },
        _ => effect.clone(),
    }
}

fn apply_effect(
    world: &mut World,
    source: Entity,
    target: Entity,
    effect: &SkillEffect,
    chain: u64,
    depth: u8,
) {
    if depth > world.resource::<RuntimeConfig>().max_trigger_depth {
        return;
    }
    match effect {
        SkillEffect::Damage {
            damage_type,
            power_basis_points,
            flat,
            can_critical,
            can_dodge,
            blockable,
        } => apply_damage(
            world,
            source,
            target,
            DamageRequest {
                damage_type: *damage_type,
                power_basis_points: *power_basis_points,
                flat: *flat,
                can_critical: *can_critical,
                can_dodge: *can_dodge,
                blockable: *blockable,
            },
            chain,
            depth,
        ),
        SkillEffect::Heal {
            power_basis_points,
            flat,
        } => apply_heal(world, source, target, *power_basis_points, *flat, chain),
        SkillEffect::RestoreResource { amount } => {
            restore_resource(world, source, *amount, chain);
        }
        SkillEffect::Shield {
            power_basis_points,
            duration,
        } => apply_shield(world, source, target, *power_basis_points, *duration, chain),
        SkillEffect::Block {
            reduction_basis_points,
            charges,
            duration,
        } => apply_block(world, source, *reduction_basis_points, *charges, *duration),
        SkillEffect::Dodge { charges, duration } => {
            apply_dodge(world, source, *charges, *duration);
        }
        SkillEffect::Move { distance_delta } => {
            move_unit(world, source, target, *distance_delta, chain);
        }
        SkillEffect::Control { strength, duration } => {
            apply_control(world, source, target, *strength, *duration, chain);
        }
        SkillEffect::Cleanse { count } => cleanse(world, source, source, *count, chain),
        SkillEffect::Status {
            status,
            magnitude_basis_points,
            duration,
        } => apply_status(
            world,
            source,
            target,
            *status,
            *magnitude_basis_points,
            *duration,
            chain,
        ),
        SkillEffect::Summon {
            definition_id,
            health_basis_points,
            attack_basis_points,
            duration,
        } => summon(
            world,
            source,
            definition_id,
            *health_basis_points,
            *attack_basis_points,
            *duration,
            chain,
        ),
        SkillEffect::Domain { strength, duration } => {
            establish_domain(world, source, *strength, *duration, chain);
        }
    }
}

struct DamageRequest {
    damage_type: DamageType,
    power_basis_points: i64,
    flat: i64,
    can_critical: bool,
    can_dodge: bool,
    blockable: bool,
}

fn apply_damage(
    world: &mut World,
    source: Entity,
    target: Entity,
    request: DamageRequest,
    chain: u64,
    depth: u8,
) {
    if request.can_dodge && consume_dodge(world, target) {
        emit(
            world,
            Some(source),
            Some(target),
            chain,
            CombatEventKind::Dodged,
        );
        run_equipment_triggers(
            world,
            target,
            TriggerCondition::DodgeSucceeded,
            chain,
            depth + 1,
        );
        return;
    }
    let Some(source_unit) = world.get::<Unit>(source) else {
        return;
    };
    let attack = source_unit.attack;
    let critical_rate = source_unit.critical_rate;
    let critical_damage = source_unit.critical_damage;
    let domain_bonus = domain_bonus(world, source);
    let defense = world
        .get::<Unit>(target)
        .map(|unit| match request.damage_type {
            DamageType::Physical => unit.physical_defense,
            DamageType::Arcane => unit.arcane_defense,
            DamageType::Soul => unit.soul_defense,
            DamageType::True => 0,
        })
        .unwrap_or(0);
    let mut damage = attack * request.power_basis_points / BASIS_POINTS + request.flat;
    damage = damage * domain_bonus / BASIS_POINTS;
    let reduction = if request.damage_type == DamageType::True {
        0
    } else {
        (defense * BASIS_POINTS / (defense + 1_000).max(1)).min(7_500)
    };
    damage = damage * (BASIS_POINTS - reduction) / BASIS_POINTS;
    let variance = world
        .resource_mut::<RandomState>()
        .basis_points(9_500, 10_500);
    damage = damage * variance / BASIS_POINTS;
    let critical = request.can_critical
        && world
            .resource_mut::<RandomState>()
            .basis_points(0, BASIS_POINTS - 1)
            < critical_rate.clamp(0, 7_500);
    if critical {
        damage = damage * critical_damage.max(BASIS_POINTS) / BASIS_POINTS;
    }
    if request.blockable {
        damage = apply_block_reduction(world, source, target, damage, chain);
    }
    apply_direct_damage(
        world,
        source,
        target,
        damage.max(1),
        request.damage_type,
        chain,
        depth,
    );
    if critical {
        if let Some(event) = world
            .resource_mut::<EventLog>()
            .events
            .iter_mut()
            .rev()
            .find(|event| matches!(event.kind, CombatEventKind::DamageApplied { .. }))
            && let CombatEventKind::DamageApplied {
                critical: event_critical,
                ..
            } = &mut event.kind
        {
            *event_critical = true;
        }
    }
}

fn apply_direct_damage(
    world: &mut World,
    source: Entity,
    target: Entity,
    amount: i64,
    damage_type: DamageType,
    chain: u64,
    depth: u8,
) {
    let (absorbed, remaining_shield, shield_broken) = absorb_shield(world, target, amount);
    if absorbed > 0 {
        emit(
            world,
            Some(source),
            Some(target),
            chain,
            CombatEventKind::ShieldChanged {
                delta: -absorbed,
                remaining: remaining_shield,
            },
        );
    }
    let health_damage = amount.saturating_sub(absorbed);
    if health_damage > 0 {
        if let Some(mut target_unit) = world.get_mut::<Unit>(target) {
            let applied = health_damage.min(target_unit.health).max(0);
            target_unit.health -= applied;
            drop(target_unit);
            if let Some(mut source_unit) = world.get_mut::<Unit>(source) {
                source_unit.damage_dealt += applied;
            }
            emit(
                world,
                Some(source),
                Some(target),
                chain,
                CombatEventKind::DamageApplied {
                    amount: applied,
                    critical: false,
                    damage_type,
                },
            );
            run_equipment_triggers(
                world,
                target,
                TriggerCondition::DamageTaken,
                chain,
                depth + 1,
            );
            if health_below_half(world, target) {
                run_equipment_triggers(
                    world,
                    target,
                    TriggerCondition::HealthBelowHalf,
                    chain,
                    depth + 1,
                );
            }
        }
    }
    if shield_broken {
        run_equipment_triggers(
            world,
            target,
            TriggerCondition::ShieldBroken,
            chain,
            depth + 1,
        );
    }
}

fn consume_dodge(world: &mut World, target: Entity) -> bool {
    let tick = world.resource::<Clock>().tick;
    let Some(mut defenses) = world.get_mut::<Defenses>(target) else {
        return false;
    };
    if defenses.dodge_charges == 0 || defenses.dodge_expires <= tick {
        return false;
    }
    defenses.dodge_charges -= 1;
    true
}

fn apply_block_reduction(
    world: &mut World,
    source: Entity,
    target: Entity,
    damage: i64,
    chain: u64,
) -> i64 {
    let tick = world.resource::<Clock>().tick;
    let Some(mut defenses) = world.get_mut::<Defenses>(target) else {
        return damage;
    };
    if defenses.block_charges == 0 || defenses.block_expires <= tick {
        return damage;
    }
    let prevented = damage * defenses.block_reduction.clamp(0, 8_000) / BASIS_POINTS;
    defenses.block_charges -= 1;
    drop(defenses);
    emit(
        world,
        Some(source),
        Some(target),
        chain,
        CombatEventKind::Blocked { prevented },
    );
    damage.saturating_sub(prevented)
}

fn absorb_shield(world: &mut World, target: Entity, amount: i64) -> (i64, i64, bool) {
    let Some(mut defenses) = world.get_mut::<Defenses>(target) else {
        return (0, 0, false);
    };
    let previous = defenses.shield;
    let absorbed = amount.min(previous).max(0);
    defenses.shield -= absorbed;
    (
        absorbed,
        defenses.shield,
        previous > 0 && defenses.shield == 0,
    )
}

fn apply_heal(
    world: &mut World,
    source: Entity,
    target: Entity,
    power_basis_points: i64,
    flat: i64,
    chain: u64,
) {
    let recovery = world
        .get::<Unit>(source)
        .map(|unit| unit.recovery_power)
        .unwrap_or(0);
    let suppression = world
        .get::<Defenses>(target)
        .map(|defenses| defenses.healing_suppression)
        .unwrap_or(0)
        .clamp(0, 9_000);
    let requested = (recovery * power_basis_points / BASIS_POINTS + flat)
        * (BASIS_POINTS - suppression)
        / BASIS_POINTS;
    let Some(mut target_unit) = world.get_mut::<Unit>(target) else {
        return;
    };
    let amount = requested
        .max(0)
        .min(target_unit.max_health - target_unit.health);
    target_unit.health += amount;
    drop(target_unit);
    if let Some(mut source_unit) = world.get_mut::<Unit>(source) {
        source_unit.healing_done += amount;
    }
    emit(
        world,
        Some(source),
        Some(target),
        chain,
        CombatEventKind::HealingApplied { amount },
    );
}

fn restore_resource(world: &mut World, target: Entity, amount: i64, chain: u64) {
    let Some(mut resource) = world.get_mut::<ResourcePool>(target) else {
        return;
    };
    let previous = resource.current;
    resource.current = (resource.current + amount).min(resource.maximum);
    let delta = resource.current - previous;
    let kind = resource.kind;
    let remaining = resource.current;
    drop(resource);
    emit(
        world,
        Some(target),
        Some(target),
        chain,
        CombatEventKind::ResourceChanged {
            kind,
            delta,
            remaining,
        },
    );
}

fn apply_shield(
    world: &mut World,
    source: Entity,
    target: Entity,
    power_basis_points: i64,
    duration: u32,
    chain: u64,
) {
    let attack = world
        .get::<Unit>(source)
        .map(|unit| unit.attack)
        .unwrap_or(0);
    let amount = (attack * power_basis_points / BASIS_POINTS).max(1);
    let tick = world.resource::<Clock>().tick;
    let Some(mut defenses) = world.get_mut::<Defenses>(target) else {
        return;
    };
    defenses.shield = defenses.shield.max(amount);
    defenses.shield_expires = tick + duration;
    let remaining = defenses.shield;
    drop(defenses);
    emit(
        world,
        Some(source),
        Some(target),
        chain,
        CombatEventKind::ShieldChanged {
            delta: amount,
            remaining,
        },
    );
}

fn apply_block(
    world: &mut World,
    target: Entity,
    reduction_basis_points: i64,
    charges: u8,
    duration: u32,
) {
    let tick = world.resource::<Clock>().tick;
    if let Some(mut defenses) = world.get_mut::<Defenses>(target) {
        defenses.block_reduction = reduction_basis_points.clamp(0, 8_000);
        defenses.block_charges = defenses.block_charges.max(charges);
        defenses.block_expires = tick + duration;
    }
}

fn apply_dodge(world: &mut World, target: Entity, charges: u8, duration: u32) {
    let tick = world.resource::<Clock>().tick;
    if let Some(mut defenses) = world.get_mut::<Defenses>(target) {
        defenses.dodge_charges = defenses.dodge_charges.max(charges);
        defenses.dodge_expires = tick + duration;
    }
}

fn move_unit(world: &mut World, source: Entity, target: Entity, distance_delta: i32, chain: u64) {
    let target_position = world.get::<Location>(target).map(|location| location.0);
    let Some(mut source_position) = world.get_mut::<Location>(source) else {
        return;
    };
    let from = source_position.0;
    source_position.0 = match target_position {
        Some(target_position) if distance_delta < 0 => {
            if from <= target_position {
                (from - distance_delta).min(target_position)
            } else {
                (from + distance_delta).max(target_position)
            }
        }
        Some(target_position) => {
            if from <= target_position {
                from - distance_delta
            } else {
                from + distance_delta
            }
        }
        None => from,
    };
    let to = source_position.0;
    drop(source_position);
    emit(
        world,
        Some(source),
        Some(target),
        chain,
        CombatEventKind::Moved { from, to },
    );
}

fn apply_control(
    world: &mut World,
    source: Entity,
    target: Entity,
    strength: i64,
    duration: u32,
    chain: u64,
) {
    let control_power = world
        .get::<Unit>(source)
        .map(|unit| unit.control_power)
        .unwrap_or(0);
    let tick = world.resource::<Clock>().tick;
    let stacks = world
        .get::<Defenses>(target)
        .map(|defenses| defenses.control_resistance_stacks)
        .unwrap_or(0);
    let Some(mut target_unit) = world.get_mut::<Unit>(target) else {
        return;
    };
    let diminishing = BASIS_POINTS - i64::from(stacks).min(4) * 1_800;
    let tenacity_damage = (strength + control_power) * diminishing / BASIS_POINTS;
    target_unit.tenacity = target_unit.tenacity.saturating_sub(tenacity_damage.max(1));
    let remaining = target_unit.tenacity;
    let broken = remaining == 0;
    if broken {
        target_unit.tenacity = target_unit.max_tenacity;
    }
    drop(target_unit);
    if broken {
        if let Some(mut defenses) = world.get_mut::<Defenses>(target) {
            defenses.stunned_until = tick + duration;
            defenses.control_resistance_stacks =
                defenses.control_resistance_stacks.saturating_add(1).min(4);
        }
        if let Some(casting) = world.get::<Casting>(target) {
            let skill_name = casting.skill.name.clone();
            world.entity_mut(target).remove::<Casting>();
            emit(
                world,
                Some(source),
                Some(target),
                chain,
                CombatEventKind::ActionInterrupted { skill_name },
            );
        }
        emit(
            world,
            Some(source),
            Some(target),
            chain,
            CombatEventKind::ControlBroken,
        );
        emit(
            world,
            Some(source),
            Some(target),
            chain,
            CombatEventKind::StatusApplied {
                status: StatusKind::Stunned,
                duration,
            },
        );
    } else {
        emit(
            world,
            Some(source),
            Some(target),
            chain,
            CombatEventKind::ControlResisted {
                tenacity_remaining: remaining,
            },
        );
    }
}

fn cleanse(world: &mut World, source: Entity, target: Entity, count: u8, chain: u64) {
    if let Some(mut defenses) = world.get_mut::<Defenses>(target) {
        defenses.healing_suppression = 0;
        defenses.stunned_until = 0;
    }
    let removable = {
        let mut query = world.query::<(Entity, &OngoingEffect)>();
        query
            .iter(world)
            .filter(|(_, effect)| {
                effect.target == target && effect.kind == StatusKind::DamageOverTime
            })
            .take(count as usize)
            .map(|(entity, effect)| (entity, effect.kind))
            .collect::<Vec<_>>()
    };
    removable.into_iter().for_each(|(entity, status)| {
        let _ = world.despawn(entity);
        emit(
            world,
            Some(source),
            Some(target),
            chain,
            CombatEventKind::StatusRemoved { status },
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn apply_status(
    world: &mut World,
    source: Entity,
    target: Entity,
    status: StatusKind,
    magnitude: i64,
    duration: u32,
    chain: u64,
) {
    let tick = world.resource::<Clock>().tick;
    match status {
        StatusKind::DamageOverTime | StatusKind::HealingOverTime => {
            world.spawn(OngoingEffect {
                source,
                target,
                kind: status,
                magnitude,
                next_tick: tick + 10,
                expires_at: tick.saturating_add(duration),
                trigger_chain: chain,
            });
        }
        StatusKind::HealingSuppression => {
            if let Some(mut defenses) = world.get_mut::<Defenses>(target) {
                defenses.healing_suppression = magnitude.clamp(0, 9_000);
            }
        }
        _ => apply_passive_effect(
            world,
            target,
            &SkillEffect::Status {
                status,
                magnitude_basis_points: magnitude,
                duration,
            },
        ),
    }
    emit(
        world,
        Some(source),
        Some(target),
        chain,
        CombatEventKind::StatusApplied { status, duration },
    );
}

fn summon(
    world: &mut World,
    source: Entity,
    definition_id: &str,
    health_basis_points: i64,
    attack_basis_points: i64,
    duration: u32,
    chain: u64,
) {
    let Some(source_identity) = world.get::<Identity>(source) else {
        return;
    };
    let source_id = source_identity.id.clone();
    let team = source_identity.team;
    let system_id = source_identity.system_id.clone();
    let source_name = source_identity.name.clone();
    let Some(source_unit) = world.get::<Unit>(source) else {
        return;
    };
    let max_health = (source_unit.max_health * health_basis_points / BASIS_POINTS).max(1);
    let attack = (source_unit.attack * attack_basis_points / BASIS_POINTS).max(1);
    let position = world
        .get::<Location>(source)
        .map(|value| value.0)
        .unwrap_or(0);
    let tick = world.resource::<Clock>().tick;
    let summon_index = world.entities().len();
    let summon_id = CombatantId::new(format!("{source_id}:{definition_id}:{summon_index}"));
    let summon_name = format!("{source_name}的契灵");
    let basic_skill = SkillDefinition {
        id: SkillId::new(format!("{definition_id}.attack")),
        name: "契灵扑击".into(),
        system_id: system_id.clone(),
        category: super::SkillCategory::Active,
        unlock_tier: 0,
        action_cost: 10_000,
        resource_cost: 0,
        cooldown: 0,
        cast_time: 0,
        min_range: 0,
        max_range: 5,
        target: TargetRule::SingleEnemy,
        tags: vec![SkillTag::Attack, SkillTag::Summon],
        effects: vec![SkillEffect::Damage {
            damage_type: DamageType::Physical,
            power_basis_points: 8_000,
            flat: 0,
            can_critical: false,
            can_dodge: true,
            blockable: true,
        }],
        mastery: 0,
    };
    let entity = world
        .spawn((
            Identity {
                id: summon_id,
                name: summon_name.clone(),
                character_id: definition_id.into(),
                system_id,
                platform_user_id: None,
                team,
            },
            Unit {
                max_health,
                health: max_health,
                attack,
                physical_defense: source_unit.physical_defense / 3,
                arcane_defense: source_unit.arcane_defense / 3,
                soul_defense: source_unit.soul_defense / 3,
                speed: source_unit.speed,
                critical_rate: 0,
                critical_damage: BASIS_POINTS,
                recovery_power: 0,
                control_power: 0,
                max_tenacity: source_unit.max_tenacity / 2,
                tenacity: source_unit.max_tenacity / 2,
                domain_power: 0,
                damage_dealt: 0,
                healing_done: 0,
            },
            ResourcePool {
                kind: ResourceKind::ContractPower,
                current: 0,
                maximum: 0,
                regeneration: 0,
            },
            Gauge::default(),
            Location(position),
            Loadout {
                active: vec![basic_skill],
                domain: None,
            },
            Cooldowns::default(),
            Tactics(Tactic::Aggressive),
            Defenses::default(),
            Equipment { triggers: vec![] },
            Summoned {
                expires_at: tick + duration,
            },
        ))
        .id();
    emit(
        world,
        Some(source),
        Some(entity),
        chain,
        CombatEventKind::EntitySummoned {
            definition_id: definition_id.into(),
            display_name: summon_name,
        },
    );
}

fn establish_domain(world: &mut World, source: Entity, strength: i64, duration: u32, chain: u64) {
    let Some(identity) = world.get::<Identity>(source) else {
        return;
    };
    let team = identity.team;
    let power = world
        .get::<Unit>(source)
        .map(|unit| unit.domain_power)
        .unwrap_or(0);
    let final_strength = strength + power;
    let tick = world.resource::<Clock>().tick;
    let skill_id = world
        .get::<Loadout>(source)
        .and_then(|loadout| loadout.domain.as_ref())
        .map(|skill| skill.id.clone())
        .unwrap_or_else(|| SkillId::new("domain"));
    let domains = {
        let mut query = world.query::<(Entity, &DomainEffect)>();
        query
            .iter(world)
            .map(|(entity, domain)| (entity, domain.team, domain.strength, domain.owner))
            .collect::<Vec<_>>()
    };
    let mut survives = true;
    domains
        .into_iter()
        .filter(|(_, domain_team, _, _)| *domain_team != team)
        .for_each(|(entity, _, domain_strength, owner)| {
            if final_strength > domain_strength {
                let _ = world.despawn(entity);
                emit(
                    world,
                    Some(source),
                    Some(owner),
                    chain,
                    CombatEventKind::DomainContested {
                        winner_id: identity_id(world, source),
                    },
                );
            } else {
                survives = false;
                emit(
                    world,
                    Some(source),
                    Some(owner),
                    chain,
                    CombatEventKind::DomainContested {
                        winner_id: identity_id(world, owner),
                    },
                );
            }
        });
    if survives {
        world.spawn(DomainEffect {
            owner: source,
            team,
            skill_id: skill_id.clone(),
            strength: final_strength,
            expires_at: tick + duration,
        });
        emit(
            world,
            Some(source),
            None,
            chain,
            CombatEventKind::DomainEstablished {
                skill_id,
                strength: final_strength,
            },
        );
    }
}

fn domain_bonus(world: &mut World, source: Entity) -> i64 {
    let Some(team) = world.get::<Identity>(source).map(|identity| identity.team) else {
        return BASIS_POINTS;
    };
    let mut query = world.query::<&DomainEffect>();
    if query.iter(world).any(|domain| domain.team == team) {
        11_000
    } else {
        BASIS_POINTS
    }
}

fn trigger_battle_started_equipment(world: &mut World) {
    let entities = sorted_living_units(world);
    entities.into_iter().for_each(|entity| {
        let chain = next_chain(world);
        run_equipment_triggers(world, entity, TriggerCondition::BattleStarted, chain, 0);
    });
}

fn run_equipment_triggers(
    world: &mut World,
    owner: Entity,
    condition: TriggerCondition,
    chain: u64,
    depth: u8,
) {
    if depth > world.resource::<RuntimeConfig>().max_trigger_depth {
        return;
    }
    let triggered = {
        let Some(mut equipment) = world.get_mut::<Equipment>(owner) else {
            return;
        };
        equipment
            .triggers
            .iter_mut()
            .filter(|trigger| {
                trigger.definition.condition == condition
                    && !(trigger.definition.once_per_battle && trigger.used)
            })
            .map(|trigger| {
                trigger.used = true;
                trigger.definition.clone()
            })
            .collect::<Vec<_>>()
    };
    triggered.into_iter().for_each(|trigger| {
        emit(
            world,
            Some(owner),
            Some(owner),
            chain,
            CombatEventKind::EquipmentTriggered {
                item_id: trigger.source_item_id,
                item_name: trigger.source_name,
            },
        );
        apply_effect(world, owner, owner, &trigger.effect, chain, depth + 1);
    });
}

fn cleanup_phase(world: &mut World) {
    let tick = world.resource::<Clock>().tick;
    let defeated = {
        let mut query = world.query::<(Entity, &Unit)>();
        query
            .iter(world)
            .filter(|(_, unit)| unit.health <= 0)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>()
    };
    defeated.into_iter().for_each(|entity| {
        if world.get::<Casting>(entity).is_some() {
            world.entity_mut(entity).remove::<Casting>();
        }
        if world.get::<Summoned>(entity).is_some() {
            world.entity_mut(entity).remove::<Summoned>();
        }
        if !already_defeated(world, entity) {
            emit(
                world,
                Some(entity),
                Some(entity),
                0,
                CombatEventKind::EntityDefeated,
            );
        }
    });
    let expired_summons = {
        let mut query = world.query::<(Entity, &Summoned)>();
        query
            .iter(world)
            .filter(|(_, summoned)| summoned.expires_at <= tick)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>()
    };
    expired_summons.into_iter().for_each(|entity| {
        if let Some(mut unit) = world.get_mut::<Unit>(entity) {
            unit.health = 0;
        }
        emit(
            world,
            Some(entity),
            Some(entity),
            0,
            CombatEventKind::EntityDefeated,
        );
    });
    check_battle_end(world);
}

fn already_defeated(world: &World, entity: Entity) -> bool {
    let id = identity_id(world, entity);
    world.resource::<EventLog>().events.iter().any(|event| {
        event.source_id.as_deref() == Some(id.as_str())
            && matches!(event.kind, CombatEventKind::EntityDefeated)
    })
}

fn check_battle_end(world: &mut World) {
    let alive_teams = {
        let mut query = world.query::<(&Identity, &Unit)>();
        query
            .iter(world)
            .filter(|(_, unit)| unit.health > 0)
            .map(|(identity, _)| identity.team)
            .collect::<HashSet<_>>()
    };
    let tick = world.resource::<Clock>().tick;
    let max_ticks = world.resource::<Clock>().max_ticks;
    let result = if alive_teams.len() == 1 {
        alive_teams
            .iter()
            .next()
            .copied()
            .map(|team| (team, BattleEndReason::Defeated))
    } else if tick >= max_ticks {
        Some((timeout_winner(world), BattleEndReason::Timeout))
    } else {
        None
    };
    if let Some((winner_team, reason)) = result {
        let mut state = world.resource_mut::<BattleState>();
        state.finished = true;
        state.winner_team = winner_team;
        state.reason = Some(reason);
        drop(state);
        emit(
            world,
            None,
            None,
            0,
            CombatEventKind::BattleEnded {
                winner_team,
                reason,
            },
        );
    }
}

fn timeout_winner(world: &mut World) -> u8 {
    let mut query = world.query::<(&Identity, &Unit)>();
    let mut scores = HashMap::<u8, i128>::new();
    query.iter(world).for_each(|(identity, unit)| {
        let health_score =
            i128::from(unit.health.max(0)) * 100_000 / i128::from(unit.max_health.max(1));
        *scores.entry(identity.team).or_default() +=
            health_score + i128::from(unit.damage_dealt) * 10 + i128::from(unit.healing_done) * 4;
    });
    scores
        .into_iter()
        .max_by_key(|(team, score)| (*score, std::cmp::Reverse(*team)))
        .map(|(team, _)| team)
        .unwrap_or(0)
}

fn build_outcome(world: &mut World, seed: u64) -> Result<CombatOutcome, CombatError> {
    let state = world.resource::<BattleState>();
    let reason = state
        .reason
        .ok_or_else(|| CombatError::InvalidState("战斗结束但没有结束原因".into()))?;
    let winner_team = state.winner_team;
    let elapsed_ticks = world.resource::<Clock>().tick;
    let mut query = world.query::<(&Identity, &Unit)>();
    let mut combatants = query
        .iter(world)
        .map(|(identity, unit)| CombatantOutcome {
            combatant_id: identity.id.clone(),
            team: identity.team,
            health: unit.health.max(0),
            max_health: unit.max_health,
            damage_dealt: unit.damage_dealt,
            healing_done: unit.healing_done,
            defeated: unit.health <= 0,
        })
        .collect::<Vec<_>>();
    combatants.sort_unstable_by(|left, right| left.combatant_id.cmp(&right.combatant_id));
    let events = std::mem::take(&mut world.resource_mut::<EventLog>().events);
    Ok(CombatOutcome {
        seed,
        winner_team,
        end_reason: reason,
        elapsed_ticks,
        events,
        combatants,
    })
}

fn health_below_half(world: &World, entity: Entity) -> bool {
    world
        .get::<Unit>(entity)
        .is_some_and(|unit| unit.health * 2 <= unit.max_health)
}

fn is_alive(world: &World, entity: Entity) -> bool {
    world
        .get::<Unit>(entity)
        .is_some_and(|unit| unit.health > 0)
}

fn next_chain(world: &mut World) -> u64 {
    let mut log = world.resource_mut::<EventLog>();
    log.next_chain += 1;
    log.next_chain
}

fn emit(
    world: &mut World,
    source: Option<Entity>,
    target: Option<Entity>,
    trigger_chain: u64,
    kind: CombatEventKind,
) {
    let tick = world.resource::<Clock>().tick;
    let source_id = source.map(|entity| identity_id(world, entity));
    let target_id = target.map(|entity| identity_id(world, entity));
    let mut log = world.resource_mut::<EventLog>();
    let sequence = log.next_sequence;
    log.next_sequence += 1;
    log.events.push(CombatEvent {
        sequence,
        tick,
        source_id,
        target_id,
        trigger_chain,
        kind,
    });
}

fn identity_id(world: &World, entity: Entity) -> CombatantId {
    world
        .get::<Identity>(entity)
        .map(|identity| identity.id.clone())
        .unwrap_or_else(|| CombatantId::new(format!("entity:{}", entity.to_bits())))
}
