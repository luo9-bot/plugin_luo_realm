use std::{
    collections::{HashMap, HashSet},
    io,
    path::Path,
};

use ab_glyph::{FontArc, PxScale};
use bevy_ecs::prelude::{Component, World};
use gif::{Encoder, Frame, Repeat};
use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage, imageops};
use imageproc::{
    drawing::{draw_filled_circle_mut, draw_filled_rect_mut, draw_line_segment_mut, draw_text_mut},
    point::Point,
    rect::Rect,
};

use crate::combat::{
    CombatEvent, CombatEventKind, CombatOutcome, CombatSnapshot, DamageType, SkillVisualConfig,
};

use super::assets;

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;
const FRAME_DELAY: u16 = 6;
const FINAL_FRAME_DELAY: u16 = 40;
const HOLD_FRAMES: usize = 1;
const EVENT_FRAMES: usize = 2;
const MAX_ANIMATED_EVENTS: usize = 18;

#[derive(Component, Clone)]
struct RenderActor {
    team: u8,
    name: String,
    system: String,
    hp: i64,
    max_hp: i64,
    combo: u32,
}

#[derive(Component, Clone)]
struct RenderEffect {
    kind: EffectKind,
    x: f32,
    y: f32,
    color: Rgba<u8>,
    age: usize,
    duration: usize,
}

#[derive(Clone)]
enum EffectKind {
    Projectile {
        target_x: f32,
        target_y: f32,
    },
    Damage {
        value: i64,
        critical: bool,
    },
    Skill {
        name: String,
        visual: SkillVisualConfig,
    },
    Flash,
}

struct RenderState {
    world: World,
    actor_entities: HashMap<String, bevy_ecs::entity::Entity>,
    background: RgbaImage,
    skill_effects: HashMap<String, RgbaImage>,
    skill_visuals: HashMap<String, SkillVisualConfig>,
    split: f32,
    winner: Option<String>,
    end_reason: String,
}

pub fn render(
    root: &Path,
    snapshot: &CombatSnapshot,
    outcome: &CombatOutcome,
    path: &Path,
) -> io::Result<()> {
    validate_input(snapshot, outcome)?;
    let assets = assets::RealmAssets::discover(root);
    let mut state = build_state(root, &assets, snapshot, outcome);
    let mut frames = Vec::new();
    push_frame(&mut frames, &mut state, &assets, outcome, 0);

    let visual_event_count = outcome
        .events
        .iter()
        .filter(|event| is_visual_event(event))
        .count();
    let stride = visual_event_count.div_ceil(MAX_ANIMATED_EVENTS).max(1);
    let mut visual_index = 0;
    for event in &outcome.events {
        apply_event(&mut state, event, snapshot);
        if !is_visual_event(event) {
            continue;
        }
        let animate =
            visual_index % stride == 0 || matches!(event.kind, CombatEventKind::BattleEnded { .. });
        visual_index += 1;
        if !animate {
            continue;
        }
        for age in 0..EVENT_FRAMES {
            advance_effects(&mut state);
            push_frame(&mut frames, &mut state, &assets, outcome, age);
        }
    }
    state.winner = snapshot
        .combatants
        .iter()
        .find(|item| item.team == outcome.winner_team)
        .map(|item| item.display_name.clone());
    (0..HOLD_FRAMES).for_each(|age| {
        advance_effects(&mut state);
        push_frame(&mut frames, &mut state, &assets, outcome, age);
    });

    encode_gif(path, frames)
}

fn validate_input(snapshot: &CombatSnapshot, outcome: &CombatOutcome) -> io::Result<()> {
    if snapshot.combatants.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "战斗渲染至少需要两个角色",
        ));
    }
    if outcome.events.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "战斗没有可回放事件",
        ));
    }
    Ok(())
}

fn build_state(
    root: &Path,
    assets: &assets::RealmAssets,
    snapshot: &CombatSnapshot,
    outcome: &CombatOutcome,
) -> RenderState {
    let mut world = World::new();
    let mut actor_entities = HashMap::new();
    snapshot.combatants.iter().for_each(|combatant| {
        let final_hp = outcome
            .combatants
            .iter()
            .find(|item| item.combatant_id == combatant.combatant_id)
            .map(|item| item.health)
            .unwrap_or(combatant.attributes.max_health);
        let entity = world
            .spawn(RenderActor {
                team: combatant.team,
                name: combatant.display_name.clone(),
                system: combatant.system_id.clone(),
                hp: combatant.attributes.max_health,
                max_hp: combatant.attributes.max_health.max(1),
                combo: 0,
            })
            .id();
        actor_entities.insert(combatant.combatant_id.clone(), entity);
        let _ = final_hp;
    });
    let skill_visuals = load_skill_visuals(root);
    let skill_effects = load_skill_effects(root, outcome, &skill_visuals);
    RenderState {
        world,
        actor_entities,
        background: build_background(root, assets, snapshot),
        skill_effects,
        skill_visuals,
        split: 50.0,
        winner: None,
        end_reason: match outcome.end_reason {
            crate::combat::BattleEndReason::Defeated => "战斗结束".into(),
            crate::combat::BattleEndReason::Timeout => "时间耗尽".into(),
            crate::combat::BattleEndReason::Objective => "目标完成".into(),
        },
    }
}

fn is_visual_event(event: &CombatEvent) -> bool {
    matches!(
        event.kind,
        CombatEventKind::SkillCast { .. }
            | CombatEventKind::DamageApplied { .. }
            | CombatEventKind::HealingApplied { .. }
            | CombatEventKind::Dodged
            | CombatEventKind::Blocked { .. }
            | CombatEventKind::EntityDefeated
            | CombatEventKind::BattleEnded { .. }
    )
}

fn load_skill_visuals(root: &Path) -> HashMap<String, SkillVisualConfig> {
    let database_path = root
        .join(crate::identity::DATA_DIRECTORY)
        .join(crate::identity::DATABASE_FILE);
    crate::database::Database::open_request(database_path)
        .ok()
        .and_then(|database| crate::database::skills::list_configs(database.connection()).ok())
        .into_iter()
        .flatten()
        .map(|config| (config.definition.id, config.visual))
        .collect()
}

fn load_skill_effects(
    root: &Path,
    outcome: &CombatOutcome,
    visuals: &HashMap<String, SkillVisualConfig>,
) -> HashMap<String, RgbaImage> {
    let directory = root.join("assets").join("realm").join("skill_effects");
    outcome
        .events
        .iter()
        .filter_map(|event| match &event.kind {
            CombatEventKind::SkillCast {
                skill_id,
                skill_name,
                ..
            }
            | CombatEventKind::ActionPrepared {
                skill_id,
                skill_name,
            } => Some(
                visuals
                    .get(skill_id)
                    .and_then(|visual| visual.effect_asset.as_deref())
                    .and_then(|path| Path::new(path).file_stem())
                    .and_then(|name| name.to_str())
                    .unwrap_or(skill_name)
                    .to_owned(),
            ),
            _ => None,
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .filter_map(|name| {
            let path = directory.join(format!("{name}.png"));
            let image = image::open(path)
                .ok()?
                .resize(220, 220, imageops::FilterType::Triangle)
                .to_rgba8();
            Some((name, image))
        })
        .collect()
}

fn apply_event(state: &mut RenderState, event: &CombatEvent, snapshot: &CombatSnapshot) {
    let color_for = |team: u8| {
        if team == 0 {
            Rgba([230, 57, 70, 255])
        } else {
            Rgba([0, 180, 216, 255])
        }
    };
    let source = event
        .source_id
        .as_ref()
        .and_then(|id| state.actor_entities.get(id).copied());
    let target = event
        .target_id
        .as_ref()
        .and_then(|id| state.actor_entities.get(id).copied());
    match &event.kind {
        CombatEventKind::SkillCast {
            skill_id,
            skill_name,
            ..
        }
        | CombatEventKind::ActionPrepared {
            skill_id,
            skill_name,
        } => {
            if let Some(entity) = source {
                let team = state
                    .world
                    .get::<RenderActor>(entity)
                    .map(|actor| actor.team)
                    .unwrap_or(0);
                let target_team = 1_u8.saturating_sub(team.min(1));
                let visual = state
                    .skill_visuals
                    .get(skill_id)
                    .cloned()
                    .unwrap_or_default();
                state.world.spawn(RenderEffect {
                    kind: EffectKind::Skill {
                        name: skill_name.clone(),
                        visual: visual.clone(),
                    },
                    x: actor_x(team),
                    y: 285.0,
                    color: parse_color(&visual.primary_color).unwrap_or_else(|| color_for(team)),
                    age: 0,
                    duration: EVENT_FRAMES,
                });
                state.world.spawn(RenderEffect {
                    kind: EffectKind::Projectile {
                        target_x: actor_x(target_team),
                        target_y: 245.0,
                    },
                    x: actor_x(team),
                    y: 245.0,
                    color: color_for(team),
                    age: 0,
                    duration: EVENT_FRAMES,
                });
            }
        }
        CombatEventKind::DamageApplied {
            amount, critical, ..
        } => {
            if let Some(entity) = target {
                let target_team = state
                    .world
                    .get::<RenderActor>(entity)
                    .map(|actor| actor.team)
                    .unwrap_or(1);
                if let Some(mut actor) = state.world.get_mut::<RenderActor>(entity) {
                    actor.hp = actor.hp.saturating_sub(*amount).max(0);
                }
                if let Some(source_entity) = source
                    && let Some(mut actor) = state.world.get_mut::<RenderActor>(source_entity)
                {
                    actor.combo = actor.combo.saturating_add(1);
                }
                if let Some(source_entity) = source {
                    let source_team = state
                        .world
                        .get::<RenderActor>(source_entity)
                        .map(|actor| actor.team)
                        .unwrap_or(0);
                    let shift = if source_team == 0 { 4.0 } else { -4.0 };
                    state.split = (state.split + shift).clamp(12.0, 88.0);
                }
                let color = color_for(1_u8.saturating_sub(target_team.min(1)));
                state.world.spawn(RenderEffect {
                    kind: EffectKind::Damage {
                        value: *amount,
                        critical: *critical,
                    },
                    x: actor_x(target_team),
                    y: 240.0,
                    color,
                    age: 0,
                    duration: EVENT_FRAMES,
                });
                state.world.spawn(RenderEffect {
                    kind: EffectKind::Flash,
                    x: 0.0,
                    y: 0.0,
                    color,
                    age: 0,
                    duration: 3,
                });
            }
        }
        CombatEventKind::BattleEnded { winner_team, .. } => {
            state.winner = snapshot
                .combatants
                .iter()
                .find(|item| item.team == *winner_team)
                .map(|item| item.display_name.clone());
        }
        _ => {}
    }
}

fn advance_effects(state: &mut RenderState) {
    let entities = state
        .world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    entities.into_iter().for_each(|entity| {
        if let Some(mut effect) = state.world.get_mut::<RenderEffect>(entity) {
            effect.age = effect.age.saturating_add(1);
            if let EffectKind::Projectile { target_x, target_y } = effect.kind {
                let progress = (effect.age as f32 / effect.duration.max(1) as f32).min(1.0);
                effect.x += (target_x - effect.x) * progress;
                effect.y += (target_y - effect.y) * progress;
            }
            if effect.age >= effect.duration {
                let _ = state.world.despawn(entity);
            }
        }
    });
}

fn push_frame(
    frames: &mut Vec<Vec<u8>>,
    state: &mut RenderState,
    assets: &assets::RealmAssets,
    outcome: &CombatOutcome,
    age: usize,
) {
    let mut image = state.background.clone();
    draw_split_line(&mut image, state.split);
    draw_actors(&mut image, state);
    draw_effects(&mut image, state, age);
    if let Some(font) = assets.font() {
        draw_labels(&mut image, state, font, outcome);
    }
    frames.push(indexed_pixels(&image));
}

fn build_background(
    root: &Path,
    assets: &assets::RealmAssets,
    snapshot: &CombatSnapshot,
) -> RgbaImage {
    let mut image = ImageBuffer::from_pixel(WIDTH, HEIGHT, Rgba([7, 8, 12, 255]));
    draw_background(&mut image);
    draw_split_overlay(&mut image, 50.0);
    draw_side_portraits(&mut image, root, assets, snapshot);
    image
}

fn draw_background(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>) {
    (0..WIDTH).step_by(48).for_each(|x| {
        draw_line_segment_mut(
            image,
            (x as f32, 0.0),
            (x as f32, HEIGHT as f32),
            Rgba([255, 255, 255, 8]),
        )
    });
    (0..HEIGHT).step_by(48).for_each(|y| {
        draw_line_segment_mut(
            image,
            (0.0, y as f32),
            (WIDTH as f32, y as f32),
            Rgba([255, 255, 255, 8]),
        )
    });
}

fn draw_side_portraits(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    root: &Path,
    assets: &assets::RealmAssets,
    snapshot: &CombatSnapshot,
) {
    snapshot.combatants.iter().take(2).for_each(|combatant| {
        let portrait_name = if combatant.team == 0 {
            "portrait-left.png"
        } else {
            "portrait-right.png"
        };
        let portrait_path = root.join("data").join("luo_realm").join(portrait_name);
        let portrait = image::open(portrait_path)
            .ok()
            .or_else(|| assets.portrait_by_id(&combatant.avatar_id))
            .or_else(|| assets.portrait(&combatant.combatant_id));
        let Some(portrait) = portrait else {
            return;
        };
        let mut portrait = portrait.resize_to_fill(480, HEIGHT, imageops::FilterType::Lanczos3);
        if combatant.team != 0 {
            portrait = DynamicImage::ImageRgba8(imageops::flip_horizontal(&portrait.to_rgba8()));
        }
        let x = if combatant.team == 0 { 0 } else { 480 };
        imageops::overlay(image, &portrait.to_rgba8(), x, 0);
    });
}

fn draw_split_overlay(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, split: f32) {
    let top = ((split + 8.0).clamp(12.0, 88.0) / 100.0 * WIDTH as f32) as i32;
    let bottom = ((split - 8.0).clamp(12.0, 88.0) / 100.0 * WIDTH as f32) as i32;
    let left = vec![
        Point::new(0, 0),
        Point::new(top, 0),
        Point::new(bottom, HEIGHT as i32),
        Point::new(0, HEIGHT as i32),
    ];
    imageproc::drawing::draw_polygon_mut(image, &left, Rgba([65, 10, 20, 150]));
    let right = vec![
        Point::new(top, 0),
        Point::new(WIDTH as i32, 0),
        Point::new(WIDTH as i32, HEIGHT as i32),
        Point::new(bottom, HEIGHT as i32),
    ];
    imageproc::drawing::draw_polygon_mut(image, &right, Rgba([5, 52, 70, 150]));
}

fn draw_split_line(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, split: f32) {
    let top = (split + 8.0).clamp(12.0, 88.0) / 100.0 * WIDTH as f32;
    let bottom = (split - 8.0).clamp(12.0, 88.0) / 100.0 * WIDTH as f32;
    draw_line_segment_mut(
        image,
        (top, 0.0),
        (bottom, HEIGHT as f32),
        Rgba([240, 240, 240, 220]),
    );
}

fn draw_actors(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, state: &mut RenderState) {
    let mut query = state.world.query::<&RenderActor>();
    query.iter(&state.world).for_each(|actor| {
        let color = if actor.team == 0 {
            Rgba([230, 57, 70, 255])
        } else {
            Rgba([0, 180, 216, 255])
        };
        draw_filled_rect_mut(
            image,
            Rect::at(if actor.team == 0 { 22 } else { 738 }, 28).of_size(200, 8),
            Rgba([255, 255, 255, 55]),
        );
        let hp_width = (200.0 * actor.hp.max(0) as f32 / actor.max_hp as f32).round() as u32;
        let hp_x = if actor.team == 0 { 22 } else { 738 };
        draw_filled_rect_mut(image, Rect::at(hp_x, 28).of_size(hp_width, 8), color);
    });
}

fn draw_effects(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, state: &mut RenderState, _age: usize) {
    let mut query = state.world.query::<&RenderEffect>();
    query.iter(&state.world).for_each(|effect| {
        let alpha =
            (255_u16.saturating_sub((effect.age * 255 / effect.duration.max(1)) as u16)) as u8;
        let color = Rgba([effect.color[0], effect.color[1], effect.color[2], alpha]);
        match &effect.kind {
            EffectKind::Damage { .. } => {
                draw_filled_circle_mut(image, (effect.x as i32, effect.y as i32), 18, color)
            }
            EffectKind::Flash => draw_filled_rect_mut(
                image,
                Rect::at(0, 0).of_size(WIDTH, HEIGHT),
                Rgba([color[0], color[1], color[2], alpha / 4]),
            ),
            EffectKind::Projectile { .. } => {
                draw_filled_circle_mut(image, (effect.x as i32, effect.y as i32), 8, color)
            }
            EffectKind::Skill { name, visual } => {
                draw_filled_circle_mut(
                    image,
                    (effect.x as i32, effect.y as i32),
                    visual.arc_width.max(4) as i32,
                    Rgba([color[0], color[1], color[2], alpha / 4]),
                );
                let asset_name = visual
                    .effect_asset
                    .as_deref()
                    .and_then(|path| Path::new(path).file_stem())
                    .and_then(|stem| stem.to_str())
                    .unwrap_or(name);
                if let Some(asset) = state.skill_effects.get(asset_name) {
                    let mut effect_image = asset.clone();
                    effect_image.pixels_mut().for_each(|pixel| {
                        pixel.0[3] = pixel.0[3].saturating_mul(alpha) / 255;
                    });
                    imageops::overlay(
                        image,
                        &effect_image,
                        i64::from(effect.x as i32 - 110),
                        i64::from(effect.y as i32 - 110),
                    );
                }
            }
        }
    });
}

fn draw_labels(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    state: &mut RenderState,
    font: &FontArc,
    outcome: &CombatOutcome,
) {
    let white = Rgba([240, 236, 228, 255]);
    let mut query = state.world.query::<&RenderActor>();
    query.iter(&state.world).for_each(|actor| {
        let x = actor_x(actor.team) as i32;
        draw_text_mut(
            image,
            white,
            x - 100,
            72,
            PxScale::from(24.0),
            font,
            &actor.name,
        );
        draw_text_mut(
            image,
            Rgba([255, 255, 255, 150]),
            x - 100,
            102,
            PxScale::from(14.0),
            font,
            &format!("{}  连击 {}", actor.system, actor.combo),
        );
        draw_text_mut(
            image,
            Rgba([255, 255, 255, 150]),
            x - 100,
            350,
            PxScale::from(13.0),
            font,
            &format!("{} / {}", actor.hp.max(0), actor.max_hp),
        );
    });
    let mut effects = state.world.query::<&RenderEffect>();
    effects
        .iter(&state.world)
        .for_each(|effect| match &effect.kind {
            EffectKind::Damage { value, critical } => draw_text_mut(
                image,
                effect.color,
                effect.x as i32 - 20,
                effect.y as i32 - 25,
                PxScale::from(if *critical { 30.0 } else { 23.0 }),
                font,
                &format!("{}{}", if *critical { "暴击 " } else { "-" }, value),
            ),
            EffectKind::Skill { name, .. } => draw_text_mut(
                image,
                effect.color,
                effect.x as i32 - 75,
                effect.y as i32 - 58,
                PxScale::from(21.0),
                font,
                name,
            ),
            _ => {}
        });
    if let Some(winner) = &state.winner {
        draw_text_mut(
            image,
            white,
            355,
            442,
            PxScale::from(28.0),
            font,
            &format!("{} 胜利", winner),
        );
        draw_text_mut(
            image,
            Rgba([255, 255, 255, 150]),
            405,
            478,
            PxScale::from(16.0),
            font,
            &state.end_reason,
        );
    } else {
        draw_text_mut(
            image,
            Rgba([255, 255, 255, 120]),
            22,
            500,
            PxScale::from(13.0),
            font,
            &format!(
                "战斗事件 {} · {} 时间片",
                outcome.events.len(),
                outcome.elapsed_ticks
            ),
        );
    }
}

fn actor_x(team: u8) -> f32 {
    if team == 0 { 275.0 } else { 685.0 }
}

fn parse_color(value: &str) -> Option<Rgba<u8>> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Rgba([red, green, blue, 255]))
}

fn encode_gif(path: &Path, frames: Vec<Vec<u8>>) -> io::Result<()> {
    let palette = rgb332_palette();
    let mut bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut bytes, WIDTH as u16, HEIGHT as u16, &palette)
            .map_err(io::Error::other)?;
        encoder
            .set_repeat(Repeat::Infinite)
            .map_err(io::Error::other)?;
        let last_index = frames.len().saturating_sub(1);
        for (index, indexed) in frames.into_iter().enumerate() {
            let mut frame = Frame::from_indexed_pixels(WIDTH as u16, HEIGHT as u16, indexed, None);
            frame.delay = if index == last_index {
                FINAL_FRAME_DELAY
            } else {
                FRAME_DELAY
            };
            encoder.write_frame(&frame).map_err(io::Error::other)?;
        }
    }
    assets::atomic_write(path, &bytes)
}

fn indexed_pixels(image: &RgbaImage) -> Vec<u8> {
    image
        .pixels()
        .map(|pixel| (pixel[0] & 0xe0) | ((pixel[1] & 0xe0) >> 3) | (pixel[2] >> 6))
        .collect()
}

fn rgb332_palette() -> Vec<u8> {
    (0_u16..=255)
        .flat_map(|index| {
            let red = (((index >> 5) & 0x07) * 255 / 7) as u8;
            let green = (((index >> 2) & 0x07) * 255 / 7) as u8;
            let blue = ((index & 0x03) * 255 / 3) as u8;
            [red, green, blue]
        })
        .collect()
}

#[allow(dead_code)]
fn _damage_color(damage_type: DamageType) -> Rgba<u8> {
    match damage_type {
        DamageType::Physical => Rgba([230, 57, 70, 255]),
        DamageType::Arcane => Rgba([0, 180, 216, 255]),
        DamageType::Soul => Rgba([173, 102, 255, 255]),
        DamageType::True => Rgba([255, 214, 10, 255]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{
        BattleEndReason, CombatAttributes, CombatantOutcome, CombatantSnapshot, ResourceKind,
        ResourceSnapshot, SkillCategory, SkillDefinition, SkillEffect, SkillTag, Tactic,
        TargetRule,
    };

    fn snapshot() -> CombatSnapshot {
        let skill = SkillDefinition {
            id: "test.attack".into(),
            name: "冰霜结晶".into(),
            system_id: "sword".into(),
            category: SkillCategory::Active,
            unlock_tier: 0,
            action_cost: 0,
            resource_cost: 0,
            cooldown: 0,
            cast_time: 0,
            min_range: 0,
            max_range: 10,
            target: TargetRule::SingleEnemy,
            tags: vec![SkillTag::Attack],
            effects: vec![SkillEffect::Damage {
                damage_type: DamageType::Physical,
                power_basis_points: 10_000,
                flat: 10,
                can_critical: false,
                can_dodge: false,
                blockable: false,
            }],
            mastery: 0,
        };
        let make = |id: &str, name: &str, team: u8| CombatantSnapshot {
            combatant_id: id.into(),
            player_id: None,
            display_name: name.into(),
            avatar_id: "missing".into(),
            system_id: "sword".into(),
            universal_tier: 1,
            team,
            position: i32::from(team),
            attributes: CombatAttributes {
                max_health: 100,
                attack: 20,
                physical_defense: 10,
                arcane_defense: 10,
                soul_defense: 10,
                speed: 10,
                critical_rate_basis_points: 0,
                critical_damage_basis_points: 15_000,
                recovery_power: 0,
                control_power: 0,
                tenacity: 10,
                domain_power: 0,
            },
            resource: ResourceSnapshot {
                kind: ResourceKind::SwordIntent,
                current: 0,
                maximum: 100,
                regeneration: 0,
            },
            active_skills: vec![skill.clone()],
            passive_skills: Vec::new(),
            domain_skill: None,
            equipment_triggers: Vec::new(),
            tactic: Tactic::Balanced,
            power: 20,
        };
        CombatSnapshot {
            rule_version: 1,
            seed: 7,
            rules: crate::combat::BattleRules::default(),
            combatants: vec![make("left", "影刃", 0), make("right", "霜华", 1)],
        }
    }

    #[test]
    fn renders_deterministic_gif() {
        let snapshot = snapshot();
        let outcome = CombatOutcome {
            seed: 7,
            winner_team: 0,
            end_reason: BattleEndReason::Defeated,
            elapsed_ticks: 12,
            events: vec![
                CombatEvent {
                    sequence: 1,
                    tick: 1,
                    source_id: Some("left".into()),
                    target_id: Some("right".into()),
                    trigger_chain: 1,
                    kind: CombatEventKind::SkillCast {
                        skill_id: "test.attack".into(),
                        skill_name: "冰霜结晶".into(),
                        tags: vec![SkillTag::Attack],
                    },
                },
                CombatEvent {
                    sequence: 2,
                    tick: 2,
                    source_id: Some("left".into()),
                    target_id: Some("right".into()),
                    trigger_chain: 1,
                    kind: CombatEventKind::DamageApplied {
                        amount: 25,
                        critical: false,
                        damage_type: DamageType::Physical,
                    },
                },
                CombatEvent {
                    sequence: 3,
                    tick: 3,
                    source_id: None,
                    target_id: None,
                    trigger_chain: 0,
                    kind: CombatEventKind::BattleEnded {
                        winner_team: 0,
                        reason: BattleEndReason::Defeated,
                    },
                },
            ],
            combatants: vec![
                CombatantOutcome {
                    combatant_id: "left".into(),
                    team: 0,
                    health: 100,
                    max_health: 100,
                    damage_dealt: 25,
                    healing_done: 0,
                    defeated: false,
                },
                CombatantOutcome {
                    combatant_id: "right".into(),
                    team: 1,
                    health: 75,
                    max_health: 100,
                    damage_dealt: 0,
                    healing_done: 0,
                    defeated: false,
                },
            ],
        };
        let directory = std::path::PathBuf::from(r"C:\Users\drluo\AppData\Local\Temp\battle_test");
        let first = directory.join("battle.gif");
        let second = directory.join("battle-repeat.gif");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        render(root, &snapshot, &outcome, &first).unwrap();
        render(root, &snapshot, &outcome, &second).unwrap();
        let first_bytes = std::fs::read(first).unwrap();
        let second_bytes = std::fs::read(second).unwrap();
        assert_eq!(first_bytes, second_bytes);
        assert!(first_bytes.starts_with(b"GIF89a"));
    }
}
