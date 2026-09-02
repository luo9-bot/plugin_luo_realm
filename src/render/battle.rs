//! 动态分屏对战 GIF 渲染。
//!
//! 复刻“动态分屏对战”网页原型的视觉语言：斜切分屏、中央光刃、技能弹道、
//! 受击粒子、伤害数字、连击计数、震屏与胜负幕。所有节拍由确定性战斗事件
//! （CombatOutcome）驱动，而不是旧版插件的随机剧本；渲染采用预烘焙静态层、
//! 全局调色板与差量帧编码，保证在纯 CPU 上 1 秒内完成出图。

use std::borrow::Cow;
use std::io::{self, Cursor};
use std::path::Path;

use ab_glyph::{FontArc, PxScale, PxScaleFont, ScaleFont as _};
use gif::{DisposalMethod, Encoder, Frame, Repeat};
use image::{DynamicImage, ImageBuffer, Rgba, imageops};
use imageproc::drawing::draw_text_mut;

use super::assets;
use crate::combat::{CombatEventKind, CombatOutcome, CombatSnapshot, CombatantSnapshot};

#[cfg(test)]
mod tests;

type Canvas = ImageBuffer<Rgba<u8>, Vec<u8>>;

// ---- 画布与节拍 ----

const W: u32 = 640;
const H: u32 = 360;
const FRAME_MS: f32 = 60.0;
const FRAME_DELAY_CS: u16 = 6;
const INTRO_MS: f32 = 620.0;
const BEAT_MS: f32 = 560.0;
const IMPACT_MS: f32 = 220.0;
const PROJ_MS: f32 = 260.0;
const RETURN_MS: f32 = 360.0;
const SETTLE_MS: f32 = 240.0;
const VICTORY_MS: f32 = 1560.0;
const FLASH_MS: f32 = 450.0;
const SHAKE_MS: f32 = 400.0;
const DMG_MS: f32 = 1100.0;
const EMBLEM_MS: f32 = 460.0;
const PANEL_MS: f32 = 950.0;
const ZOOM_MS: f32 = 380.0;
const COMBO_HOLD_MS: f32 = 520.0;
const FADE_MS: f32 = 260.0;
const MAX_BEATS: usize = 7;

const SPLIT_MIN: f32 = 12.0;
const SPLIT_MAX: f32 = 88.0;
const SPLIT_CENTER: f32 = 50.0;
const DIAG_OFFSET: f32 = 8.0;
const SPLIT_APPROACH: f32 = 0.26;
const HP_APPROACH: f32 = 0.30;

const HUD_H: i32 = 30;
const HP_MARGIN: i32 = 18;
const HP_TRACK_W: i32 = 226;
const HP_TRACK_Y: i32 = 38;
const HP_NAME_Y: i32 = 46;
const HP_SUB_Y: i32 = 64;
const GRID_PX: u32 = 38;
const ART_BOX_W: u32 = 296;
const ART_BOX_H: u32 = 330;

// ---- 基础色 ----

#[derive(Clone, Copy)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

impl Rgb {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    fn lerp(a: Self, b: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self::new(
            (f32::from(a.r) + (f32::from(b.r) - f32::from(a.r)) * t).round() as u8,
            (f32::from(a.g) + (f32::from(b.g) - f32::from(a.g)) * t).round() as u8,
            (f32::from(a.b) + (f32::from(b.b) - f32::from(a.b)) * t).round() as u8,
        )
    }

    fn gap(a: Self, b: Self) -> i32 {
        (i32::from(a.r) - i32::from(b.r)).abs()
            + (i32::from(a.g) - i32::from(b.g)).abs()
            + (i32::from(a.b) - i32::from(b.b)).abs()
    }
}

const CRIMSON: Rgb = Rgb::new(230, 57, 70);
const CYAN: Rgb = Rgb::new(0, 180, 216);
const INK: Rgb = Rgb::new(240, 236, 228);
const PAPER: Rgb = Rgb::new(248, 250, 252);
const HEAL: Rgb = Rgb::new(110, 231, 160);
const GOLD: Rgb = Rgb::new(240, 197, 100);

#[derive(Clone, Copy)]
struct Theme {
    primary: Rgb,
    bright: Rgb,
    bg: Rgb,
}

fn system_primary(system_id: &str) -> Option<Rgb> {
    match system_id {
        "sword" => Some(Rgb::new(79, 157, 247)),
        "qi" => Some(Rgb::new(46, 230, 168)),
        "mage" => Some(Rgb::new(138, 125, 255)),
        "soul" => Some(Rgb::new(176, 108, 255)),
        "body" => Some(Rgb::new(255, 122, 69)),
        "blood_demon" => Some(Rgb::new(230, 57, 70)),
        "formation" => Some(Rgb::new(56, 209, 124)),
        "alchemy_artifact" => Some(Rgb::new(255, 197, 61)),
        "summoner" => Some(Rgb::new(154, 205, 90)),
        "music" => Some(Rgb::new(255, 111, 165)),
        _ => None,
    }
}

fn theme_of(primary: Rgb) -> Theme {
    let bright = Rgb::lerp(primary, Rgb::new(255, 255, 255), 0.38);
    let tint = |c: u8| (4.0 + f32::from(c) * 0.022).round() as u8;
    Theme {
        primary,
        bright,
        bg: Rgb::new(tint(primary.r), tint(primary.g), tint(primary.b)),
    }
}

fn build_theme(system_id: &str, fallback: Rgb) -> Theme {
    theme_of(system_primary(system_id).unwrap_or(fallback))
}

// ---- 视图模型 ----

#[derive(Clone)]
struct SideView {
    team: u8,
    name: String,
    character_id: String,
    theme: Theme,
    max_health: i64,
}

#[derive(Clone, Copy, Debug)]
enum Strike {
    Damage {
        amount: i64,
        critical: bool,
        dodged: bool,
        blocked: bool,
        prevented: i64,
    },
    Heal {
        amount: i64,
    },
    Support,
}

#[derive(Clone)]
struct Beat {
    actor: usize,
    skill: String,
    strike: Strike,
    hp: [i64; 2],
    combo: u32,
}

struct BattleView {
    sides: [SideView; 2],
    beats: Vec<Beat>,
    winner: Option<usize>,
    seed: u64,
}

fn clamp_text(text: &str) -> String {
    const LIMIT: usize = 8;
    let mut out: String = text.chars().take(LIMIT).collect();
    if text.chars().count() > LIMIT {
        out.push('…');
    }
    out
}

fn invalid_input(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn side_view(combatant: &CombatantSnapshot, theme: Theme) -> SideView {
    SideView {
        team: combatant.team,
        name: clamp_text(&combatant.display_name),
        character_id: combatant.character_id.clone(),
        theme,
        max_health: combatant.attributes.max_health.max(1),
    }
}

fn locate_side(snapshot: &CombatSnapshot, left_team: u8, id: &str) -> Option<usize> {
    snapshot.combatants.iter().find_map(|combatant| {
        if combatant.combatant_id == id {
            Some(if combatant.team == left_team { 0 } else { 1 })
        } else {
            None
        }
    })
}

fn close_beat(current: &mut Option<(usize, String, Strike)>, beats: &mut Vec<Beat>, hp: [i64; 2]) {
    if let Some((actor, skill, strike)) = current.take() {
        let meaningful = !matches!(strike, Strike::Support) || !skill.is_empty();
        if meaningful {
            beats.push(Beat {
                actor,
                skill,
                strike,
                hp,
                combo: 0,
            });
        }
    }
}

fn apply_combos(beats: &mut [Beat]) {
    let mut counters = [0_u32; 2];
    let mut last_actor: Option<usize> = None;
    for beat in beats.iter_mut() {
        if matches!(beat.strike, Strike::Support | Strike::Heal { .. }) {
            counters[beat.actor] = 0;
            last_actor = None;
            continue;
        }
        counters[beat.actor] = if last_actor == Some(beat.actor) {
            counters[beat.actor] + 1
        } else {
            1
        };
        beat.combo = counters[beat.actor];
        last_actor = Some(beat.actor);
    }
}

fn extract_view(snapshot: &CombatSnapshot, outcome: &CombatOutcome) -> io::Result<BattleView> {
    let left = snapshot
        .combatants
        .first()
        .ok_or_else(|| invalid_input("战斗快照没有参战角色"))?;
    let right = snapshot
        .combatants
        .iter()
        .find(|combatant| combatant.team != left.team)
        .ok_or_else(|| invalid_input("战斗快照只有一个阵营"))?;

    let left_theme = build_theme(&left.system_id, CRIMSON);
    let right_theme_initial = build_theme(&right.system_id, CYAN);
    let right_theme = if Rgb::gap(left_theme.primary, right_theme_initial.primary) < 120 {
        let alt = if Rgb::gap(left_theme.primary, CYAN) < 120 {
            CRIMSON
        } else {
            CYAN
        };
        theme_of(alt)
    } else {
        right_theme_initial
    };

    let sides = [side_view(left, left_theme), side_view(right, right_theme)];
    let left_team = left.team;
    let mut hp = [sides[0].max_health, sides[1].max_health];
    let mut beats: Vec<Beat> = Vec::new();
    let mut current: Option<(usize, String, Strike)> = None;
    let mut winner = None;

    for event in &outcome.events {
        let actor = event
            .source_id
            .as_deref()
            .and_then(|id| locate_side(snapshot, left_team, id));
        let target = event
            .target_id
            .as_deref()
            .and_then(|id| locate_side(snapshot, left_team, id));
        match &event.kind {
            CombatEventKind::SkillCast { skill_name, .. } => {
                close_beat(&mut current, &mut beats, hp);
                if let Some(actor) = actor {
                    current = Some((actor, clamp_text(skill_name), Strike::Support));
                }
            }
            CombatEventKind::DamageApplied {
                amount, critical, ..
            } => {
                if let Some(target) = target {
                    hp[target] = (hp[target] - amount).max(0);
                }
                match (&mut current, actor) {
                    (Some((owner, _, strike)), Some(source)) if *owner == source => {
                        if matches!(strike, Strike::Support) {
                            *strike = Strike::Damage {
                                amount: *amount,
                                critical: *critical,
                                dodged: false,
                                blocked: false,
                                prevented: 0,
                            };
                        }
                    }
                    (None, Some(source)) => {
                        current = Some((
                            source,
                            "追击".into(),
                            Strike::Damage {
                                amount: *amount,
                                critical: *critical,
                                dodged: false,
                                blocked: false,
                                prevented: 0,
                            },
                        ));
                    }
                    _ => {}
                }
            }
            CombatEventKind::Dodged => {
                if let (Some((owner, _, strike)), Some(source)) = (&mut current, actor)
                    && *owner == source
                    && matches!(strike, Strike::Support)
                {
                    *strike = Strike::Damage {
                        amount: 0,
                        critical: false,
                        dodged: true,
                        blocked: false,
                        prevented: 0,
                    };
                }
            }
            CombatEventKind::Blocked { prevented } => {
                if let (Some((owner, _, strike)), Some(source)) = (&mut current, actor)
                    && let Strike::Damage {
                        blocked,
                        prevented: saved,
                        ..
                    } = strike
                    && *owner == source
                {
                    *blocked = true;
                    *saved = *prevented;
                }
            }
            CombatEventKind::HealingApplied { amount } => {
                if let Some(target) = target {
                    hp[target] = (hp[target] + amount).min(sides[target].max_health);
                }
                if let Some((owner, _, strike)) = current.as_mut() {
                    if Some(*owner) == actor && matches!(strike, Strike::Support) {
                        *strike = Strike::Heal { amount: *amount };
                    }
                } else if let Some(source) = actor {
                    current = Some((source, "回复".into(), Strike::Heal { amount: *amount }));
                }
            }
            CombatEventKind::PassiveTriggered { name, .. } => {
                open_trigger_beat(&mut current, actor, name);
            }
            CombatEventKind::EquipmentTriggered { item_name, .. } => {
                open_trigger_beat(&mut current, actor, item_name);
            }
            CombatEventKind::BattleEnded { winner_team, .. } => {
                winner = if sides[0].team == *winner_team {
                    Some(0)
                } else if sides[1].team == *winner_team {
                    Some(1)
                } else {
                    None
                };
            }
            _ => {}
        }
    }
    close_beat(&mut current, &mut beats, hp);
    apply_combos(&mut beats);
    if beats.len() > MAX_BEATS {
        let head = MAX_BEATS * 2 / 5;
        let tail_start = beats.len() - (MAX_BEATS - head);
        beats = beats[..head]
            .iter()
            .chain(beats[tail_start..].iter())
            .cloned()
            .collect();
    }

    Ok(BattleView {
        sides,
        beats,
        winner,
        seed: outcome.seed,
    })
}

fn open_trigger_beat(
    current: &mut Option<(usize, String, Strike)>,
    actor: Option<usize>,
    name: &str,
) {
    if current.is_none()
        && let Some(source) = actor
    {
        *current = Some((source, clamp_text(name), Strike::Support));
    }
}

// ---- 确定性随机 ----

struct SplitMix(u64);

impl SplitMix {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        let unit = (self.next_u64() >> 11) as f32 / (1_u64 << 53) as f32;
        lo + unit * (hi - lo)
    }
}

// ---- 时间轴与逐帧状态 ----

struct Timeline {
    frames: usize,
    beat_starts: Vec<f32>,
    impacts: Vec<f32>,
    victory_at: f32,
}

fn build_timeline(beat_count: usize) -> Timeline {
    let beat_starts: Vec<f32> = (0..beat_count)
        .map(|index| INTRO_MS + BEAT_MS * index as f32)
        .collect();
    let impacts = beat_starts.iter().map(|start| start + IMPACT_MS).collect();
    let victory_at = INTRO_MS + BEAT_MS * beat_count as f32 + SETTLE_MS;
    let frames = ((victory_at + VICTORY_MS) / FRAME_MS).ceil() as usize;
    Timeline {
        frames,
        beat_starts,
        impacts,
        victory_at,
    }
}

#[derive(Clone, Copy)]
struct FrameState {
    split: f32,
    hp: [f32; 2],
    flash_alpha: f32,
    flash_color: Rgb,
    shake: (i32, i32),
    fade: f32,
    overlay: f32,
}

const SHAKE_KEYS: [(f32, f32, f32); 8] = [
    (0.00, 0.0, 0.0),
    (0.10, -5.2, 3.0),
    (0.20, 5.2, -2.2),
    (0.30, -3.7, -1.5),
    (0.40, 3.7, 1.5),
    (0.50, -2.2, 2.2),
    (0.60, 2.2, -0.7),
    (1.00, 0.0, 0.0),
];

fn shake_offset(progress: f32, kill: bool) -> (i32, i32) {
    let scale = if kill { 1.35 } else { 1.0 };
    let mut index = 0;
    while index + 2 < SHAKE_KEYS.len() && SHAKE_KEYS[index + 1].0 <= progress {
        index += 1;
    }
    let (start, x0, y0) = SHAKE_KEYS[index];
    let (end, x1, y1) = SHAKE_KEYS[index + 1];
    let k = ((progress - start) / (end - start)).clamp(0.0, 1.0);
    (
        ((x0 + (x1 - x0) * k) * scale).round() as i32,
        ((y0 + (y1 - y0) * k) * scale).round() as i32,
    )
}

fn beat_split_target(beat: &Beat, kill: bool) -> f32 {
    let dir = if beat.actor == 0 { 1.0 } else { -1.0 };
    let magnitude = match beat.strike {
        Strike::Support => 0.0,
        Strike::Heal { .. } => 1.0,
        Strike::Damage {
            dodged, critical, ..
        } => {
            let base = if dodged { 1.5 } else { 4.0 };
            base + f32::from(critical) * 3.0 + f32::from(kill) * 2.0
        }
    };
    (SPLIT_CENTER + dir * magnitude).clamp(SPLIT_MIN, SPLIT_MAX)
}

fn precompute_states(view: &BattleView, timeline: &Timeline) -> Vec<FrameState> {
    let targets: Vec<(f32, bool)> = view
        .beats
        .iter()
        .map(|beat| {
            let defender = 1 - beat.actor;
            (
                beat_split_target(beat, beat.hp[defender] == 0),
                beat.hp[defender] == 0,
            )
        })
        .collect();
    let full = [
        view.sides[0].max_health as f32,
        view.sides[1].max_health as f32,
    ];
    let mut split = SPLIT_CENTER;
    let mut hp = full;
    let mut states = Vec::with_capacity(timeline.frames);
    for frame in 0..timeline.frames {
        let t = frame as f32 * FRAME_MS;
        let mut target = SPLIT_CENTER;
        let mut flash_alpha = 0.0_f32;
        let mut flash_color = INK;
        let mut shake = (0_i32, 0_i32);
        for (index, beat) in view.beats.iter().enumerate() {
            let start = timeline.beat_starts[index];
            let (beat_target, kill) = targets[index];
            if (start..start + RETURN_MS).contains(&t) {
                target = beat_target;
            }
            let into = t - start;
            if (0.0..FLASH_MS).contains(&into) {
                let alpha = (0.45 * (1.0 - into / FLASH_MS) + f32::from(kill) * 0.15).min(0.6);
                if alpha > flash_alpha {
                    flash_alpha = alpha;
                    flash_color = view.sides[beat.actor].theme.primary;
                }
            }
            let since = t - (start + IMPACT_MS);
            if (0.0..SHAKE_MS).contains(&since) {
                shake = shake_offset(since / SHAKE_MS, kill);
            }
        }
        split += (target - split) * SPLIT_APPROACH;
        let mut hp_target = full;
        for (index, beat) in view.beats.iter().enumerate() {
            if t >= timeline.impacts[index] {
                hp_target = [beat.hp[0] as f32, beat.hp[1] as f32];
            }
        }
        hp[0] += (hp_target[0] - hp[0]) * HP_APPROACH;
        hp[1] += (hp_target[1] - hp[1]) * HP_APPROACH;
        states.push(FrameState {
            split,
            hp,
            flash_alpha,
            flash_color,
            shake,
            fade: (t / FADE_MS).min(1.0),
            overlay: ((t - timeline.victory_at) / 500.0).clamp(0.0, 1.0) * 0.75,
        });
    }
    states
}

// ---- 节拍特效参数 ----

struct Particle {
    angle: f32,
    speed: f32,
    size: f32,
    life: f32,
}

struct BeatFx {
    particles: Vec<Particle>,
    proj_dy: f32,
    dmg_off: (f32, f32),
    icon: Option<Canvas>,
    effect: Option<Canvas>,
}

/// 把素材图等比缩到边长上限内，返回 RGBA 画布。
fn sprite_canvas(image: &DynamicImage, max_side: f32) -> Canvas {
    let scale = (max_side / image.width() as f32).min(max_side / image.height() as f32);
    let width = ((image.width() as f32 * scale).round() as u32).max(1);
    let height = ((image.height() as f32 * scale).round() as u32).max(1);
    image
        .resize_exact(width, height, imageops::FilterType::Lanczos3)
        .to_rgba8()
}

/// 以 (cx, cy) 为中心、指定缩放与整体透明度叠加精灵图。
fn stamp_sprite(world: &mut Canvas, sprite: &Canvas, cx: f32, cy: f32, scale: f32, alpha: f32) {
    if alpha <= 0.01 || scale <= 0.01 {
        return;
    }
    let (sw, sh) = (sprite.width() as f32, sprite.height() as f32);
    let (half_w, half_h) = (sw * scale / 2.0, sh * scale / 2.0);
    let (origin_x, origin_y) = (cx - half_w, cy - half_h);
    let x0 = (cx - half_w).floor() as i32;
    let x1 = (cx + half_w).ceil() as i32;
    let y0 = (cy - half_h).floor() as i32;
    let y1 = (cy + half_h).ceil() as i32;
    for py in y0..y1 {
        if py < 0 || py >= H as i32 {
            continue;
        }
        let v = ((py as f32 + 0.5 - origin_y) / scale).floor();
        if !(0.0..sh).contains(&v) {
            continue;
        }
        for px in x0..x1 {
            if px < 0 || px >= W as i32 {
                continue;
            }
            let u = ((px as f32 + 0.5 - origin_x) / scale).floor();
            if !(0.0..sw).contains(&u) {
                continue;
            }
            let pixel = sprite.get_pixel(u as u32, v as u32).0;
            if pixel[3] == 0 {
                continue;
            }
            let coverage = f32::from(pixel[3]) / 255.0 * alpha;
            blend_px(
                world,
                px,
                py,
                [
                    pixel[0],
                    pixel[1],
                    pixel[2],
                    (coverage * 255.0).round() as u8,
                ],
            );
        }
    }
}

fn precompute_fx(view: &BattleView, realm: &assets::RealmAssets) -> Vec<BeatFx> {
    view.beats
        .iter()
        .enumerate()
        .map(|(index, beat)| {
            let mut rng =
                SplitMix(view.seed ^ (index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let count = match beat.strike {
                Strike::Damage { critical: true, .. } => 22,
                Strike::Damage { dodged: true, .. } => 6,
                Strike::Damage { .. } => 12,
                _ => 8,
            };
            let icon = realm
                .skill_icon(&beat.skill)
                .map(|image| sprite_canvas(&image, 46.0));
            let effect = {
                let named = realm.skill_effect(&beat.skill);
                let crit_fallback = || {
                    realm
                        .skill_effect("致命一击")
                        .filter(|_| matches!(beat.strike, Strike::Damage { critical: true, .. }))
                };
                named
                    .or_else(crit_fallback)
                    .map(|image| sprite_canvas(&image, 150.0))
            };
            BeatFx {
                particles: (0..count)
                    .map(|p| {
                        let angle =
                            std::f32::consts::TAU * p as f32 / count as f32 + rng.range(-0.4, 0.4);
                        Particle {
                            angle,
                            speed: rng.range(55.0, 118.0),
                            size: rng.range(2.0, 4.5),
                            life: rng.range(380.0, 650.0),
                        }
                    })
                    .collect(),
                proj_dy: rng.range(-24.0, 24.0),
                dmg_off: (rng.range(-46.0, 46.0), rng.range(-36.0, 36.0)),
                icon,
                effect,
            }
        })
        .collect()
}

// ---- 光栅基元 ----

fn blend_px(image: &mut Canvas, x: i32, y: i32, rgba: [u8; 4]) {
    if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
        return;
    }
    let pixel = image.get_pixel_mut(x as u32, y as u32);
    let source_alpha = u32::from(rgba[3]);
    if source_alpha == 0 {
        return;
    }
    if source_alpha == 255 {
        *pixel = Rgba(rgba);
        return;
    }
    let destination_alpha = u32::from(pixel.0[3]);
    let out_alpha = source_alpha + destination_alpha * (255 - source_alpha) / 255;
    if out_alpha == 0 {
        *pixel = Rgba([0, 0, 0, 0]);
        return;
    }
    for (target, source) in pixel.0[..3].iter_mut().zip(rgba[..3].iter()) {
        let numerator = u32::from(*source) * source_alpha
            + u32::from(*target) * destination_alpha * (255 - source_alpha) / 255;
        *target = (numerator / out_alpha).min(255) as u8;
    }
    pixel.0[3] = out_alpha.min(255) as u8;
}

fn add_px(image: &mut Canvas, x: i32, y: i32, color: Rgb, strength: f32) {
    if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
        return;
    }
    let weight = (strength * 255.0).round().min(255.0) as u32;
    if weight == 0 {
        return;
    }
    let pixel = image.get_pixel_mut(x as u32, y as u32);
    for (target, source) in pixel.0[..3]
        .iter_mut()
        .zip([color.r, color.g, color.b].iter())
    {
        let addition = (u32::from(*source) * weight / 255).min(255) as u16;
        *target = (u16::from(*target) + addition).min(255) as u8;
    }
}

fn fill_rect(image: &mut Canvas, x: i32, y: i32, width: i32, height: i32, rgba: [u8; 4]) {
    if rgba[3] == 0 || width <= 0 || height <= 0 {
        return;
    }
    for dy in 0..height {
        for dx in 0..width {
            blend_px(image, x + dx, y + dy, rgba);
        }
    }
}

/// 水平线性渐变填充；`gradient` 为 (左端颜色, 右端颜色)。
fn fill_h_gradient(
    image: &mut Canvas,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    gradient: (Rgb, Rgb),
    alpha: u8,
) {
    if alpha == 0 || width <= 0 || height <= 0 {
        return;
    }
    for column in 0..width {
        let t = if width <= 1 {
            0.0
        } else {
            column as f32 / (width - 1) as f32
        };
        let color = Rgb::lerp(gradient.0, gradient.1, t);
        for row in 0..height {
            blend_px(
                image,
                x + column,
                y + row,
                [color.r, color.g, color.b, alpha],
            );
        }
    }
}

fn fill_circle(image: &mut Canvas, cx: f32, cy: f32, radius: f32, rgba: [u8; 4]) {
    if rgba[3] == 0 || radius <= 0.0 {
        return;
    }
    let span = radius.ceil() as i32;
    let center_x = cx.round() as i32;
    let center_y = cy.round() as i32;
    for dy in -span..=span {
        for dx in -span..=span {
            if (dx * dx + dy * dy) as f32 <= radius * radius {
                blend_px(image, center_x + dx, center_y + dy, rgba);
            }
        }
    }
}

fn stroke_ring(image: &mut Canvas, cx: f32, cy: f32, radius: f32, half_width: f32, rgba: [u8; 4]) {
    if rgba[3] == 0 || radius <= 0.0 {
        return;
    }
    let span = (radius + half_width).ceil() as i32;
    let center_x = cx.round() as i32;
    let center_y = cy.round() as i32;
    for dy in -span..=span {
        for dx in -span..=span {
            let distance = ((dx * dx + dy * dy) as f32).sqrt();
            if (distance - radius).abs() <= half_width {
                blend_px(image, center_x + dx, center_y + dy, rgba);
            }
        }
    }
}

/// 从 (cx, cy) 沿 angle 方向画一条带渐隐的射线（用于命中纹章）。
fn draw_ray(image: &mut Canvas, cx: f32, cy: f32, angle: f32, from: f32, to: f32, rgba: [u8; 4]) {
    let (sin, cos) = angle.sin_cos();
    let steps = ((to - from).ceil() as i32).max(1);
    for step in 0..=steps {
        let distance = from + (to - from) * step as f32 / steps as f32;
        let fade = 1.0 - step as f32 / steps as f32;
        let mut rgba = rgba;
        rgba[3] = (f32::from(rgba[3]) * fade).round() as u8;
        if rgba[3] == 0 {
            continue;
        }
        blend_px(
            image,
            (cx + cos * distance).round() as i32,
            (cy + sin * distance).round() as i32,
            rgba,
        );
    }
}

struct GlowTable([f32; 64]);

impl GlowTable {
    fn new() -> Self {
        let mut table = [0.0_f32; 64];
        for (index, value) in table.iter_mut().enumerate() {
            let d = index as f32 / 64.0;
            *value = (1.0 - d * d).powi(2);
        }
        Self(table)
    }
}

/// 以 (cx, cy) 为中心叠加柔光斑点；`additive` 为真时使用加色混合。
fn stamp_glow(
    image: &mut Canvas,
    table: &GlowTable,
    (cx, cy): (f32, f32),
    radius: f32,
    color: Rgb,
    strength: f32,
    additive: bool,
) {
    if radius <= 0.0 || strength <= 0.01 {
        return;
    }
    let span = radius.ceil() as i32;
    let center_x = cx.round() as i32;
    let center_y = cy.round() as i32;
    for dy in -span..=span {
        for dx in -span..=span {
            let distance = ((dx * dx + dy * dy) as f32).sqrt() / radius;
            if distance >= 1.0 {
                continue;
            }
            let falloff = (table.0[(distance * 64.0) as usize] * strength).min(1.0);
            if falloff < 0.012 {
                continue;
            }
            let px = center_x + dx;
            let py = center_y + dy;
            if additive {
                add_px(image, px, py, color, falloff);
            } else {
                let alpha = (falloff * 255.0).round() as u8;
                blend_px(image, px, py, [color.r, color.g, color.b, alpha]);
            }
        }
    }
}

// ---- 文本 ----

#[derive(Clone, Copy)]
enum Align {
    Center,
    Left,
    Right,
}

fn text_span(scaled: &PxScaleFont<FontArc>, text: &str, spacing: f32) -> f32 {
    let advance: f32 = text
        .chars()
        .map(|ch| scaled.h_advance(scaled.glyph_id(ch)))
        .sum();
    advance + spacing * text.chars().count().saturating_sub(1) as f32
}

/// 在文字后方绘制柔发光晕（等效 HTML text-shadow 的模糊光），避免
/// 多次位移描摹在笔画密集的中文上产生的重影。
#[allow(clippy::too_many_arguments)]
fn stamp_text_halo(
    image: &mut Canvas,
    cx: f32,
    cy: f32,
    half_width: f32,
    half_height: f32,
    margin: f32,
    color: Rgb,
    strength: f32,
) {
    if strength <= 0.01 || margin <= 0.0 {
        return;
    }
    let x0 = (cx - half_width - margin).floor() as i32;
    let x1 = (cx + half_width + margin).ceil() as i32;
    let y0 = (cy - half_height - margin).floor() as i32;
    let y1 = (cy + half_height + margin).ceil() as i32;
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = ((px as f32 + 0.5 - cx).abs() - half_width).max(0.0);
            let dy = ((py as f32 + 0.5 - cy).abs() - half_height).max(0.0);
            let distance = ((dx * dx + dy * dy).sqrt() / margin).min(1.0);
            let falloff = (1.0 - distance * distance).powi(2) * strength;
            if falloff < 0.01 {
                continue;
            }
            blend_px(
                image,
                px,
                py,
                [color.r, color.g, color.b, (falloff * 255.0).round() as u8],
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    image: &mut Canvas,
    font: &FontArc,
    size: f32,
    x: f32,
    y: f32,
    rgba: [u8; 4],
    text: &str,
    spacing: f32,
    align: Align,
    glow: f32,
    bold: bool,
) {
    if text.is_empty() || rgba[3] == 0 {
        return;
    }
    let scale = PxScale::from(size);
    let scaled = PxScaleFont {
        font: font.clone(),
        scale,
    };
    let width = text_span(&scaled, text, spacing);
    let mut pen = match align {
        Align::Center => x - width / 2.0,
        Align::Left => x,
        Align::Right => x - width,
    };
    if glow > 0.0 {
        let center_x = match align {
            Align::Center => x,
            Align::Left => x + width / 2.0,
            Align::Right => x - width / 2.0,
        };
        let strength = f32::from(rgba[3]) / 255.0 * 0.6;
        stamp_text_halo(
            image,
            center_x,
            y + size * 0.55,
            width / 2.0 + 2.0,
            size * 0.55,
            glow.clamp(2.0, 12.0),
            Rgb::new(rgba[0], rgba[1], rgba[2]),
            strength,
        );
    }
    let mut passes: Vec<(f32, f32, u8)> = Vec::new();
    if bold {
        passes.extend([(1.0, 0.0, 255), (0.0, 1.0, 255), (1.0, 1.0, 255)]);
    }
    passes.push((0.0, 0.0, 255));
    for ch in text.chars() {
        if ch != ' ' {
            let glyph_text = ch.to_string();
            for (offset_x, offset_y, pass_alpha) in &passes {
                let alpha = (u32::from(*pass_alpha) * u32::from(rgba[3]) / 255) as u8;
                if alpha == 0 {
                    continue;
                }
                draw_text_mut(
                    image,
                    Rgba([rgba[0], rgba[1], rgba[2], alpha]),
                    pen as i32 + *offset_x as i32,
                    y as i32 + *offset_y as i32,
                    scale,
                    font,
                    &glyph_text,
                );
            }
        }
        pen += scaled.h_advance(scaled.glyph_id(ch)) + spacing;
    }
}

// ---- 静态层预渲染 ----

/// 单侧静态层：底图（主题色板 + 光晕 + 网格）与完整显示的立绘。
/// 立绘单独存放，便于攻击时按帧做镜头缩放。
struct SideLayer {
    bg: Canvas,
    art: Option<Canvas>,
    art_center: (f32, f32),
}

fn ellipse_falloff(x: u32, y: u32, cx: f32, cy: f32, rx: f32, ry: f32) -> f32 {
    let dx = (x as f32 - cx) / rx;
    let dy = (y as f32 - cy) / ry;
    1.0 - dx * dx - dy * dy
}

/// 立绘等比缩放到完整显示（不裁切、不拉伸），透明边缘保留。
fn contain_portrait(portrait: &DynamicImage, center: (f32, f32)) -> Canvas {
    let scale = (ART_BOX_W as f32 / portrait.width() as f32)
        .min(ART_BOX_H as f32 / portrait.height() as f32);
    let width = ((portrait.width() as f32 * scale).round() as u32).max(1);
    let height = ((portrait.height() as f32 * scale).round() as u32).max(1);
    let resized = portrait.resize_exact(width, height, imageops::FilterType::Lanczos3);
    let mut canvas = Canvas::new(W, H);
    let x = (center.0 - width as f32 / 2.0).round() as i64;
    let y = (center.1 - height as f32 / 2.0).round() as i64;
    imageops::overlay(&mut canvas, &resized, x, y);
    canvas
}

fn render_side_layer(side: &SideView, left: bool, portrait: Option<&DynamicImage>) -> SideLayer {
    let theme = side.theme;
    let mut bg = Canvas::new(W, H);
    let (glow1, glow2) = if left {
        (
            (0.25 * W as f32, 0.45 * H as f32),
            (0.80 * W as f32, 0.80 * H as f32),
        )
    } else {
        (
            (0.75 * W as f32, 0.45 * H as f32),
            (0.20 * W as f32, 0.80 * H as f32),
        )
    };
    for y in 0..H {
        for x in 0..W {
            let mut r = f32::from(theme.bg.r);
            let mut g = f32::from(theme.bg.g);
            let mut b = f32::from(theme.bg.b);
            for (center, radius_x, radius_y, peak) in [
                (glow1, 0.62 * W as f32, 0.62 * H as f32, 0.18),
                (glow2, 0.50 * W as f32, 0.50 * H as f32, 0.06),
            ] {
                let falloff =
                    (ellipse_falloff(x, y, center.0, center.1, radius_x, radius_y).max(0.0)) * peak;
                r += f32::from(theme.primary.r) * falloff;
                g += f32::from(theme.primary.g) * falloff;
                b += f32::from(theme.primary.b) * falloff;
            }
            if x % GRID_PX == 0 || y % GRID_PX == 0 {
                r += 3.0;
                g += 3.0;
                b += 3.0;
            }
            bg.put_pixel(
                x,
                y,
                Rgba([
                    r.min(255.0) as u8,
                    g.min(255.0) as u8,
                    b.min(255.0) as u8,
                    255,
                ]),
            );
        }
    }

    let art_center = if left {
        (0.272 * W as f32, 0.54 * H as f32)
    } else {
        (0.728 * W as f32, 0.54 * H as f32)
    };
    let art = portrait.map(|portrait| contain_portrait(portrait, art_center));
    SideLayer {
        bg,
        art,
        art_center,
    }
}

// ---- 分屏合成与光刃 ----

fn split_edges(split: f32) -> (f32, f32) {
    let centered = split.clamp(SPLIT_MIN, SPLIT_MAX);
    (
        (centered + DIAG_OFFSET) / 100.0 * W as f32,
        (centered - DIAG_OFFSET) / 100.0 * W as f32,
    )
}

fn overlay_span(dst: &mut [u8], src: &[u8], x0: usize) {
    for (index, chunk) in src.chunks_exact(4).enumerate() {
        let alpha = chunk[3];
        if alpha == 0 {
            continue;
        }
        let base = (x0 + index) * 4;
        if alpha == 255 {
            dst[base..base + 4].copy_from_slice(chunk);
            continue;
        }
        let sa = u32::from(alpha);
        let da = u32::from(dst[base + 3]);
        let out = sa + da * (255 - sa) / 255;
        for channel in 0..3 {
            let numerator = u32::from(chunk[channel]) * sa
                + u32::from(dst[base + channel]) * da * (255 - sa) / 255;
            dst[base + channel] = (numerator / out.max(1)).min(255) as u8;
        }
        dst[base + 3] = out.min(255) as u8;
    }
}

fn composite_base(world: &mut Canvas, left: &SideLayer, right: &SideLayer, xt: f32, xb: f32) {
    let stride = W as usize * 4;
    let left_bg: &[u8] = &left.bg;
    let right_bg: &[u8] = &right.bg;
    let raw: &mut [u8] = &mut *world;
    for y in 0..H as usize {
        let row_start = y * stride;
        let row = &mut raw[row_start..row_start + stride];
        let progress = y as f32 / (H as f32 - 1.0);
        let cut = ((xt + (xb - xt) * progress).round() as usize * 4).clamp(0, stride);
        row[..cut].copy_from_slice(&left_bg[row_start..row_start + cut]);
        row[cut..].copy_from_slice(&right_bg[row_start + cut..row_start + stride]);
    }
}

/// 把立绘以指定缩放合成到世界画布，并按斜切线裁剪到所属侧。
/// 缩放为 1 时走逐行快速路径；攻击缩放时走双线性采样路径。
fn blit_side_art(world: &mut Canvas, layer: &SideLayer, left: bool, xt: f32, xb: f32, scale: f32) {
    let Some(art) = layer.art.as_ref() else {
        return;
    };
    let (cx, cy) = layer.art_center;
    let stride = W as usize * 4;
    if (scale - 1.0).abs() < 0.004 {
        let art_raw: &[u8] = art;
        let raw: &mut [u8] = &mut *world;
        for y in 0..H as usize {
            let progress = y as f32 / (H as f32 - 1.0);
            let cut = ((xt + (xb - xt) * progress).round() as usize * 4).clamp(0, stride);
            let row = &mut raw[y * stride..(y + 1) * stride];
            let art_row = &art_raw[y * stride..(y + 1) * stride];
            if left {
                overlay_span(row, &art_row[..cut], 0);
            } else {
                overlay_span(row, &art_row[cut..], cut / 4);
            }
        }
        return;
    }

    let (aw, ah) = (art.width() as f32, art.height() as f32);
    let (half_w, half_h) = (aw * scale / 2.0, ah * scale / 2.0);
    let (origin_x, origin_y) = (cx - half_w, cy - half_h);
    let x0 = (cx - half_w).floor() as i32;
    let x1 = (cx + half_w).ceil() as i32;
    let y0 = (cy - half_h).floor() as i32;
    let y1 = (cy + half_h).ceil() as i32;
    for py in y0..y1 {
        if py < 0 || py >= H as i32 {
            continue;
        }
        let progress = py as f32 / (H as f32 - 1.0);
        let cut = (xt + (xb - xt) * progress).round() as i32;
        let v = (py as f32 + 0.5 - origin_y) / scale - 0.5;
        let v0 = v.floor().max(0.0) as u32;
        let v1 = (v0 + 1).min(ah as u32 - 1);
        let fv = v - v.floor();
        for px in x0..x1 {
            if px < 0 || px >= W as i32 {
                continue;
            }
            let inside = if left { px < cut } else { px >= cut };
            if !inside {
                continue;
            }
            let u = (px as f32 + 0.5 - origin_x) / scale - 0.5;
            let u0 = u.floor().max(0.0) as u32;
            let u1 = (u0 + 1).min(aw as u32 - 1);
            let fu = u - u.floor();
            let p00 = art.get_pixel(u0, v0).0;
            let p10 = art.get_pixel(u1, v0).0;
            let p01 = art.get_pixel(u0, v1).0;
            let p11 = art.get_pixel(u1, v1).0;
            let mut rgba = [0_u8; 4];
            for (channel, target) in rgba.iter_mut().enumerate() {
                let top = f32::from(p00[channel]) * (1.0 - fu) + f32::from(p10[channel]) * fu;
                let bottom = f32::from(p01[channel]) * (1.0 - fu) + f32::from(p11[channel]) * fu;
                *target = (top * (1.0 - fv) + bottom * fv).round() as u8;
            }
            blend_px(world, px, py, rgba);
        }
    }
}

fn divider_profile() -> [f32; 33] {
    let mut profile = [0.0_f32; 33];
    for (index, value) in profile.iter_mut().enumerate() {
        let dx = index as f32 - 16.0;
        let glow = if dx.abs() <= 12.0 { 0.06 } else { 0.0 };
        let blur = 0.13 * (-dx * dx / (2.0 * 4.5 * 4.5)).exp();
        let core = if dx.abs() <= 1.0 { 0.45 } else { 0.0 };
        *value = (glow + blur + core).min(0.8);
    }
    profile
}

fn stamp_divider(world: &mut Canvas, xt: f32, xb: f32, profile: &[f32; 33]) {
    for y in 0..H as i32 {
        let progress = y as f32 / (H as f32 - 1.0);
        let center = (xt + (xb - xt) * progress).round() as i32;
        for (index, &alpha) in profile.iter().enumerate() {
            if alpha <= 0.002 {
                continue;
            }
            blend_px(
                world,
                center + index as i32 - 16,
                y,
                [PAPER.r, PAPER.g, PAPER.b, (alpha * 255.0).round() as u8],
            );
        }
    }
}

// ---- 渲染器 ----

struct Renderer {
    view: BattleView,
    timeline: Timeline,
    states: Vec<FrameState>,
    fx: Vec<BeatFx>,
    layers: [SideLayer; 2],
    glow: GlowTable,
    divider: [f32; 33],
    font: Option<FontArc>,
    world: Canvas,
    stage: Canvas,
}

fn fx_center(beat: &Beat, fx: &BeatFx) -> (f32, f32) {
    let own_side = beat.actor == 0;
    let xf = match beat.strike {
        Strike::Heal { .. } => {
            if own_side {
                0.38
            } else {
                0.62
            }
        }
        _ => {
            if own_side {
                0.62
            } else {
                0.38
            }
        }
    };
    (W as f32 * xf + fx.dmg_off.0, H as f32 * 0.38 + fx.dmg_off.1)
}

impl Renderer {
    /// 受击方的镜头缩放：仅在暴击与致命一击时 punch-in（关键技能触发）。
    fn side_zoom(&self, side: usize, t: f32) -> f32 {
        let mut zoom = 1.0_f32;
        for (index, beat) in self.view.beats.iter().enumerate() {
            if 1 - beat.actor != side {
                continue;
            }
            let Strike::Damage {
                critical, dodged, ..
            } = beat.strike
            else {
                continue;
            };
            if dodged || !critical {
                continue;
            }
            let since = t - self.timeline.impacts[index];
            if !(0.0..ZOOM_MS).contains(&since) {
                continue;
            }
            let kill = beat.hp[side] == 0;
            let peak = if kill { 0.11 } else { 0.09 };
            let q = since / ZOOM_MS;
            zoom = zoom.max(1.0 + peak * (1.0 - q).powi(2));
        }
        zoom
    }

    fn render_frame(&mut self, frame: usize) {
        let state = self.states[frame];
        let t = frame as f32 * FRAME_MS;
        let (xt, xb) = split_edges(state.split);
        composite_base(&mut self.world, &self.layers[0], &self.layers[1], xt, xb);
        let zooms = [self.side_zoom(0, t), self.side_zoom(1, t)];
        for (side_index, scale) in zooms.iter().enumerate() {
            blit_side_art(
                &mut self.world,
                &self.layers[side_index],
                side_index == 0,
                xt,
                xb,
                *scale,
            );
        }
        if state.flash_alpha > 0.0 {
            let color = state.flash_color;
            fill_rect(
                &mut self.world,
                0,
                0,
                W as i32,
                H as i32,
                [
                    color.r,
                    color.g,
                    color.b,
                    (state.flash_alpha * 255.0).round() as u8,
                ],
            );
        }
        stamp_divider(&mut self.world, xt, xb, &self.divider);
        self.render_projectiles_particles(t);
        self.render_hud(t, state.hp);
        self.render_texts(t);
        self.render_overlay(t, state.overlay);
    }

    fn render_projectiles_particles(&mut self, t: f32) {
        for index in 0..self.view.beats.len() {
            let beat = &self.view.beats[index];
            let theme = self.view.sides[beat.actor].theme;
            let fx = &self.fx[index];
            let start = self.timeline.beat_starts[index];
            let progress = (t - start) / PROJ_MS;
            if (0.0..1.0).contains(&progress) {
                let ease = 1.0 - (1.0 - progress).powi(4);
                let (sx_ratio, ex_ratio) = if beat.actor == 0 {
                    (0.22, 0.62)
                } else {
                    (0.78, 0.38)
                };
                let p0 = (W as f32 * sx_ratio, H as f32 * 0.60);
                let p2 = (W as f32 * ex_ratio, H as f32 * 0.44 + fx.proj_dy);
                let p1 = (
                    (p0.0 + p2.0) / 2.0,
                    p0.1.min(p2.1) - 96.0 - fx.proj_dy.abs(),
                );
                let bezier = |k: f32| {
                    let inverted = 1.0 - k;
                    (
                        inverted * inverted * p0.0 + 2.0 * inverted * k * p1.0 + k * k * p2.0,
                        inverted * inverted * p0.1 + 2.0 * inverted * k * p1.1 + k * k * p2.1,
                    )
                };
                // 彗星：实心锥形弧身 + 白色核心 + 外发光，任何立绘上都清晰可读
                for step in (0..16).rev() {
                    let trail = (progress - step as f32 * 0.030).max(0.0);
                    let fade = 1.0 - step as f32 / 16.0;
                    let (tx, ty) = bezier(trail * ease);
                    let radius = 2.0 + 8.0 * fade;
                    fill_circle(
                        &mut self.world,
                        tx,
                        ty,
                        radius,
                        [
                            theme.bright.r,
                            theme.bright.g,
                            theme.bright.b,
                            (120.0 + 120.0 * fade) as u8,
                        ],
                    );
                    if step < 7 {
                        fill_circle(
                            &mut self.world,
                            tx,
                            ty,
                            radius * 0.5,
                            [255, 255, 255, (150.0 + 100.0 * fade) as u8],
                        );
                    }
                    stamp_glow(
                        &mut self.world,
                        &self.glow,
                        (tx, ty),
                        radius * 2.8,
                        theme.primary,
                        fade * 0.7,
                        true,
                    );
                }
                let (head_x, head_y) = bezier(ease);
                stamp_glow(
                    &mut self.world,
                    &self.glow,
                    (head_x, head_y),
                    20.0,
                    theme.bright,
                    0.9,
                    true,
                );
                fill_circle(&mut self.world, head_x, head_y, 4.5, [255, 255, 255, 245]);
            }
            let age = t - (start + IMPACT_MS);
            if age >= 0.0 {
                let (ix, iy) = fx_center(beat, fx);
                for particle in &fx.particles {
                    if !(0.0..particle.life).contains(&age) {
                        continue;
                    }
                    let q = age / particle.life;
                    let ease = 1.0 - (1.0 - q).powf(1.9);
                    let x = ix + particle.angle.cos() * particle.speed * ease;
                    let y = iy + particle.angle.sin() * particle.speed * ease;
                    let radius = particle.size * 2.3 * (1.0 - 0.55 * q);
                    stamp_glow(
                        &mut self.world,
                        &self.glow,
                        (x, y),
                        radius,
                        theme.primary,
                        (1.0 - q) * 0.85,
                        true,
                    );
                }
            }
        }
    }

    /// 顶部 HUD：标题 / 回合数 / 当前技能名 + 双侧血条与角色名。
    fn render_hud(&mut self, t: f32, hp_display: [f32; 2]) {
        fill_rect(&mut self.world, 0, 0, W as i32, HUD_H, [8, 8, 11, 178]);
        if let Some(font) = self.font.as_ref() {
            draw_text(
                &mut self.world,
                font,
                11.0,
                12.0,
                8.0,
                [INK.r, INK.g, INK.b, 205],
                "LUO REALM · 天命对决",
                1.0,
                Align::Left,
                0.0,
                true,
            );
            let (round_label, skill_label) = self.hud_labels(t);
            draw_text(
                &mut self.world,
                font,
                13.0,
                W as f32 / 2.0,
                7.0,
                [GOLD.r, GOLD.g, GOLD.b, 235],
                &round_label,
                2.0,
                Align::Center,
                0.0,
                true,
            );
            draw_text(
                &mut self.world,
                font,
                12.0,
                (W as i32 - 12) as f32,
                8.0,
                [PAPER.r, PAPER.g, PAPER.b, 225],
                &skill_label,
                2.0,
                Align::Right,
                0.0,
                true,
            );
            if let Some(index) = self.hud_current(t)
                && let Some(icon) = self.fx[index].icon.as_ref()
            {
                let scaled = PxScaleFont {
                    font: font.clone(),
                    scale: PxScale::from(12.0),
                };
                let name_width = text_span(&scaled, &skill_label, 2.0);
                stamp_sprite(
                    &mut self.world,
                    icon,
                    W as f32 - 12.0 - name_width - 14.0,
                    15.0,
                    0.55,
                    0.95,
                );
            }
        }
        for (side_index, &display) in hp_display.iter().enumerate() {
            let side = &self.view.sides[side_index];
            let theme = side.theme;
            let max_health = side.max_health;
            let name = side.name.clone();
            let left = side_index == 0;
            let track_x = if left {
                HP_MARGIN
            } else {
                W as i32 - HP_MARGIN - HP_TRACK_W
            };
            let align = if left { Align::Left } else { Align::Right };
            let label_x = if left {
                track_x as f32
            } else {
                (track_x + HP_TRACK_W) as f32
            };
            if let Some(font) = self.font.as_ref() {
                draw_text(
                    &mut self.world,
                    font,
                    13.0,
                    label_x,
                    HP_NAME_Y as f32,
                    [INK.r, INK.g, INK.b, 240],
                    &name,
                    2.0,
                    align,
                    3.0,
                    true,
                );
            }
            fill_rect(
                &mut self.world,
                track_x,
                HP_TRACK_Y,
                HP_TRACK_W,
                5,
                [255, 255, 255, 18],
            );
            let ratio = (display / max_health as f32).clamp(0.0, 1.0);
            let fill_w = (HP_TRACK_W as f32 * ratio).round() as i32;
            if fill_w > 0 {
                let (fill_x, edge_x, gradient_left, gradient_right) = if left {
                    let x = track_x + HP_TRACK_W - fill_w;
                    (x, x, theme.bright, theme.primary)
                } else {
                    let x = track_x;
                    (x, x + fill_w, theme.primary, theme.bright)
                };
                fill_h_gradient(
                    &mut self.world,
                    fill_x,
                    HP_TRACK_Y,
                    fill_w,
                    5,
                    (gradient_left, gradient_right),
                    255,
                );
                for step in 0..3 {
                    let glow_x = if left {
                        edge_x as f32 - 8.0 * step as f32
                    } else {
                        edge_x as f32 + 8.0 * step as f32
                    };
                    stamp_glow(
                        &mut self.world,
                        &self.glow,
                        (glow_x, HP_TRACK_Y as f32 + 2.5),
                        8.0,
                        theme.primary,
                        0.22,
                        true,
                    );
                }
            }
            let mut sub = format!("{} / {}", display.round() as i64, max_health);
            let combo = self
                .view
                .beats
                .iter()
                .zip(self.timeline.impacts.iter())
                .rev()
                .find(|(beat, impact)| {
                    beat.combo >= 2
                        && beat.actor == side_index
                        && (0.0..COMBO_HOLD_MS).contains(&(t - *impact))
                })
                .map(|(beat, _)| beat.combo);
            if let Some(combo) = combo {
                sub.push_str(&format!("    COMBO ×{combo}"));
            }
            if let Some(font) = self.font.as_ref() {
                draw_text(
                    &mut self.world,
                    font,
                    10.0,
                    label_x,
                    HP_SUB_Y as f32,
                    [INK.r, INK.g, INK.b, 125],
                    &sub,
                    1.0,
                    align,
                    0.0,
                    false,
                );
            }
        }
    }

    /// HUD 中央的回合数与右上角的当前技能名。
    fn hud_current(&self, t: f32) -> Option<usize> {
        let mut current: Option<usize> = None;
        for (index, start) in self.timeline.beat_starts.iter().enumerate() {
            if t >= *start {
                current = Some(index);
            } else {
                break;
            }
        }
        current
    }

    fn hud_labels(&self, t: f32) -> (String, String) {
        match self.hud_current(t) {
            Some(index) => (
                format!("第 {} 回合", index + 1),
                self.view.beats[index].skill.clone(),
            ),
            None => ("天命对决".to_string(), String::new()),
        }
    }

    fn render_texts(&mut self, t: f32) {
        let Some(font) = self.font.clone() else {
            return;
        };
        for index in 0..self.view.beats.len() {
            let beat = &self.view.beats[index];
            let theme = self.view.sides[beat.actor].theme;
            let (fx_dx, fx_dy) = {
                let fx = &self.fx[index];
                (fx.dmg_off.0, fx.dmg_off.1)
            };
            let since_impact = t - self.timeline.impacts[index];
            if !(0.0..DMG_MS).contains(&since_impact) {
                continue;
            }
            let (emblem_x, emblem_y) = fx_center(beat, &self.fx[index]);

            // 命中纹章：特效素材（缺失时退化为程序化双环 + 放射线）
            if (0.0..EMBLEM_MS).contains(&since_impact) {
                let p = since_impact / EMBLEM_MS;
                let ease = 1.0 - (1.0 - p).powi(3);
                let fade = 1.0 - p;
                stamp_glow(
                    &mut self.world,
                    &self.glow,
                    (emblem_x, emblem_y),
                    26.0 + 22.0 * ease,
                    theme.primary,
                    fade * 0.5,
                    true,
                );
                match self.fx[index].effect.as_ref() {
                    Some(effect) => {
                        stamp_sprite(
                            &mut self.world,
                            effect,
                            emblem_x,
                            emblem_y,
                            0.72 + 0.5 * ease,
                            fade,
                        );
                    }
                    None => {
                        stroke_ring(
                            &mut self.world,
                            emblem_x,
                            emblem_y,
                            6.0 + 42.0 * ease,
                            2.0,
                            [PAPER.r, PAPER.g, PAPER.b, (fade * 150.0) as u8],
                        );
                        stroke_ring(
                            &mut self.world,
                            emblem_x,
                            emblem_y,
                            3.0 + 28.0 * ease,
                            1.4,
                            [
                                theme.bright.r,
                                theme.bright.g,
                                theme.bright.b,
                                (fade * 200.0) as u8,
                            ],
                        );
                        for ray in 0..10 {
                            let angle = std::f32::consts::TAU * ray as f32 / 10.0 + 0.31;
                            draw_ray(
                                &mut self.world,
                                emblem_x,
                                emblem_y,
                                angle,
                                12.0 + 30.0 * ease,
                                22.0 + 58.0 * ease,
                                [
                                    theme.bright.r,
                                    theme.bright.g,
                                    theme.bright.b,
                                    (fade * 220.0) as u8,
                                ],
                            );
                        }
                    }
                }
            }

            let strike_label = match beat.strike {
                Strike::Damage {
                    critical, dodged, ..
                } => {
                    if dodged {
                        Some(("闪避", INK))
                    } else if critical {
                        Some(("致命一击", GOLD))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some((label, color)) = strike_label
                && (0.0..EMBLEM_MS).contains(&since_impact)
            {
                let fade = 1.0 - since_impact / EMBLEM_MS;
                stamp_text_halo(
                    &mut self.world,
                    emblem_x,
                    emblem_y + 44.0,
                    46.0,
                    12.0,
                    8.0,
                    Rgb::new(7, 7, 10),
                    0.55 * fade,
                );
                draw_text(
                    &mut self.world,
                    &font,
                    15.0,
                    emblem_x,
                    emblem_y + 36.0,
                    [color.r, color.g, color.b, (fade * 255.0) as u8],
                    label,
                    2.0,
                    Align::Center,
                    0.0,
                    true,
                );
            }

            match beat.strike {
                Strike::Damage {
                    amount,
                    critical,
                    dodged,
                    blocked,
                    prevented,
                } => {
                    if !dodged && (0.0..DMG_MS).contains(&since_impact) {
                        let q = since_impact / DMG_MS;
                        let rise = -46.0 * (1.0 - (1.0 - q).powf(1.6));
                        let pop = if q < 0.18 {
                            0.4 + 4.72 * q
                        } else {
                            1.25 - 0.55 * (q - 0.18) / 0.82
                        };
                        let alpha = if q < 0.55 {
                            1.0
                        } else {
                            1.0 - (q - 0.55) / 0.45
                        };
                        let (text, color, base) = if critical {
                            (format!("{amount}!"), GOLD, 40.0)
                        } else {
                            (format!("-{amount}"), PAPER, 30.0)
                        };
                        let size = (base * pop.max(0.35)).max(8.0);
                        let ix = W as f32 * if beat.actor == 0 { 0.655 } else { 0.345 } + fx_dx;
                        let iy = H as f32 * 0.20 + fx_dy + rise;
                        let scaled = PxScaleFont {
                            font: font.clone(),
                            scale: PxScale::from(size),
                        };
                        let text_width = text_span(&scaled, &text, 0.0);
                        stamp_text_halo(
                            &mut self.world,
                            ix,
                            iy + size * 0.45,
                            text_width / 2.0 + 5.0,
                            size * 0.45,
                            9.0,
                            Rgb::new(6, 6, 9),
                            0.62 * alpha,
                        );
                        draw_text(
                            &mut self.world,
                            &font,
                            size,
                            ix,
                            iy,
                            [color.r, color.g, color.b, (alpha * 255.0) as u8],
                            &text,
                            0.0,
                            Align::Center,
                            0.0,
                            true,
                        );
                    }
                    if blocked && (0.0..PANEL_MS).contains(&since_impact) {
                        let p = since_impact / PANEL_MS;
                        let alpha =
                            ((p / 0.12).min(1.0) * ((1.0 - p) / 0.2).min(1.0)).clamp(0.0, 1.0);
                        let panel_x = W as i32 - 210;
                        let panel_y = H as i32 - 66;
                        fill_rect(
                            &mut self.world,
                            panel_x,
                            panel_y,
                            194,
                            46,
                            [6, 6, 9, (alpha * 165.0) as u8],
                        );
                        fill_rect(
                            &mut self.world,
                            panel_x,
                            panel_y,
                            194,
                            1,
                            [
                                theme.bright.r,
                                theme.bright.g,
                                theme.bright.b,
                                (alpha * 110.0) as u8,
                            ],
                        );
                        draw_text(
                            &mut self.world,
                            &font,
                            11.0,
                            (panel_x + 12) as f32,
                            (panel_y + 7) as f32,
                            [GOLD.r, GOLD.g, GOLD.b, (alpha * 235.0) as u8],
                            "格挡",
                            2.0,
                            Align::Left,
                            0.0,
                            true,
                        );
                        draw_text(
                            &mut self.world,
                            &font,
                            12.0,
                            (panel_x + 12) as f32,
                            (panel_y + 24) as f32,
                            [INK.r, INK.g, INK.b, (alpha * 230.0) as u8],
                            &format!("抵消了 {prevented} 点伤害"),
                            1.0,
                            Align::Left,
                            0.0,
                            false,
                        );
                    }
                }
                Strike::Heal { amount } => {
                    if (0.0..DMG_MS).contains(&since_impact) {
                        let q = since_impact / DMG_MS;
                        let pop = if q < 0.18 {
                            0.4 + 4.72 * q
                        } else {
                            1.25 - 0.55 * (q - 0.18) / 0.82
                        };
                        let alpha = if q < 0.55 {
                            1.0
                        } else {
                            1.0 - (q - 0.55) / 0.45
                        };
                        let size = (28.0 * pop.max(0.35)).max(8.0);
                        let (ix, iy) = fx_center(beat, &self.fx[index]);
                        stamp_text_halo(
                            &mut self.world,
                            ix,
                            iy + size * 0.45,
                            30.0,
                            size * 0.45,
                            8.0,
                            Rgb::new(6, 10, 8),
                            0.5 * alpha,
                        );
                        draw_text(
                            &mut self.world,
                            &font,
                            size,
                            ix,
                            iy,
                            [HEAL.r, HEAL.g, HEAL.b, (alpha * 255.0) as u8],
                            &format!("+{amount}"),
                            0.0,
                            Align::Center,
                            0.0,
                            true,
                        );
                    }
                }
                Strike::Support => {}
            }
        }
    }

    fn render_overlay(&mut self, t: f32, overlay: f32) {
        if overlay <= 0.0 {
            return;
        }
        fill_rect(
            &mut self.world,
            0,
            0,
            W as i32,
            H as i32,
            [0, 0, 0, (overlay * 255.0).round() as u8],
        );
        let Some(font) = self.font.clone() else {
            return;
        };
        let (color, text) = match self.view.winner {
            Some(0) => (GOLD, format!("{} 胜利", self.view.sides[0].name)),
            Some(_) => (GOLD, format!("{} 胜利", self.view.sides[1].name)),
            None => (INK, "平局".into()),
        };
        let reveal = ((t - self.timeline.victory_at - 120.0) / 400.0).clamp(0.0, 1.0);
        if reveal > 0.0 {
            let ease = 1.0 - (1.0 - reveal).powi(3);
            let size = 44.0 * (0.5 + 0.5 * ease);
            let scaled = PxScaleFont {
                font: font.clone(),
                scale: PxScale::from(size),
            };
            let text_width = text_span(&scaled, &text, 3.0);
            stamp_text_halo(
                &mut self.world,
                W as f32 / 2.0,
                H as f32 * 0.40 + size * 0.45,
                text_width / 2.0 + 10.0,
                size * 0.45,
                12.0,
                Rgb::new(5, 5, 8),
                0.65 * ease,
            );
            draw_text(
                &mut self.world,
                &font,
                size,
                W as f32 / 2.0,
                H as f32 * 0.40,
                [color.r, color.g, color.b, (ease * 255.0) as u8],
                &text,
                3.0,
                Align::Center,
                0.0,
                true,
            );
        }
        let sub_reveal = ((t - self.timeline.victory_at - 450.0) / 400.0).clamp(0.0, 1.0);
        if sub_reveal > 0.0 {
            draw_text(
                &mut self.world,
                &font,
                13.0,
                W as f32 / 2.0,
                H as f32 * 0.40 + 48.0,
                [INK.r, INK.g, INK.b, (sub_reveal * 150.0) as u8],
                "战斗结束",
                6.0,
                Align::Center,
                0.0,
                false,
            );
        }
    }

    fn present(&mut self, frame: usize) -> &Canvas {
        let state = self.states[frame];
        if state.shake == (0, 0) && state.fade >= 1.0 {
            return &self.world;
        }
        {
            let (dx, dy) = state.shake;
            let stride = W as usize * 4;
            let source: &[u8] = &self.world;
            let target: &mut [u8] = &mut self.stage;
            target.fill(0);
            let shift = dx.clamp(-(W as i32), W as i32);
            for y in 0..H as usize {
                let source_y = y as i32 - dy;
                if !(0..H as i32).contains(&source_y) {
                    continue;
                }
                let source_row =
                    &source[source_y as usize * stride..(source_y as usize + 1) * stride];
                let target_row = &mut target[y * stride..(y + 1) * stride];
                let (source_x, target_x) = if shift >= 0 {
                    (0_usize, shift as usize * 4)
                } else {
                    ((-shift) as usize * 4, 0_usize)
                };
                let length = (stride - source_x).min(stride - target_x);
                if length > 0 {
                    target_row[target_x..target_x + length]
                        .copy_from_slice(&source_row[source_x..source_x + length]);
                }
            }
            if state.fade < 1.0 {
                let k = state.fade;
                for pixel in target.chunks_exact_mut(4) {
                    pixel[0] = (f32::from(pixel[0]) * k) as u8;
                    pixel[1] = (f32::from(pixel[1]) * k) as u8;
                    pixel[2] = (f32::from(pixel[2]) * k) as u8;
                    pixel[3] = 255;
                }
            }
        }
        &self.stage
    }
}

// ---- 调色板与编码 ----

const BIN_COUNT: usize = 32 * 32 * 32;

#[inline]
fn bin_of(r: u8, g: u8, b: u8) -> usize {
    ((r >> 3) as usize) << 10 | ((g >> 3) as usize) << 5 | ((b >> 3) as usize)
}

fn bin_center(bin: usize) -> (u8, u8, u8) {
    (
        ((bin >> 10) * 8 + 3) as u8,
        (((bin >> 5) & 31) * 8 + 3) as u8,
        ((bin & 31) * 8 + 3) as u8,
    )
}

fn forced_colors(view: &BattleView) -> Vec<Rgb> {
    vec![
        view.sides[0].theme.bg,
        view.sides[1].theme.bg,
        view.sides[0].theme.primary,
        view.sides[0].theme.bright,
        view.sides[1].theme.primary,
        view.sides[1].theme.bright,
        INK,
        PAPER,
        Rgb::new(0, 0, 0),
        HEAL,
    ]
}

fn build_palette(probes: &[Canvas], forced: &[Rgb]) -> (Vec<u8>, Box<[u8; BIN_COUNT]>) {
    let mut histogram = vec![0_u32; BIN_COUNT];
    for probe in probes {
        for pixel in probe.pixels() {
            histogram[bin_of(pixel.0[0], pixel.0[1], pixel.0[2])] += 1;
        }
    }
    let mut order: Vec<usize> = (0..BIN_COUNT).filter(|&bin| histogram[bin] > 0).collect();
    order.sort_unstable_by(|&a, &b| histogram[b].cmp(&histogram[a]));
    let mut used = vec![false; BIN_COUNT];
    let mut palette_bins: Vec<usize> = Vec::with_capacity(256);
    for color in forced {
        let bin = bin_of(color.r, color.g, color.b);
        if !used[bin] {
            used[bin] = true;
            palette_bins.push(bin);
        }
    }
    for &bin in &order {
        if palette_bins.len() >= 256 {
            break;
        }
        if !used[bin] {
            used[bin] = true;
            palette_bins.push(bin);
        }
    }
    let mut palette = Vec::with_capacity(palette_bins.len() * 3);
    for &bin in &palette_bins {
        let (r, g, b) = bin_center(bin);
        palette.extend([r, g, b]);
    }
    let centers: Vec<(u8, u8, u8)> = palette_bins.iter().copied().map(bin_center).collect();
    let mut lut = Box::new([0_u8; BIN_COUNT]);
    for (bin, slot) in lut.iter_mut().enumerate() {
        let (r, g, b) = bin_center(bin);
        let mut best = 0_u8;
        let mut best_distance = i32::MAX;
        for (index, &(pr, pg, pb)) in centers.iter().enumerate() {
            let distance = (i32::from(r) - i32::from(pr)).pow(2)
                + (i32::from(g) - i32::from(pg)).pow(2)
                + (i32::from(b) - i32::from(pb)).pow(2);
            if distance < best_distance {
                best_distance = distance;
                best = index as u8;
            }
        }
        *slot = best;
    }
    (palette, lut)
}

fn map_indices(source: &Canvas, lut: &[u8; BIN_COUNT], target: &mut [u8]) {
    for (index, pixel) in source.pixels().enumerate() {
        target[index] = lut[bin_of(pixel.0[0], pixel.0[1], pixel.0[2])];
    }
}

fn write_diff_frame<W: io::Write>(
    encoder: &mut Encoder<W>,
    indices: &[u8],
    delay: u16,
    previous: Option<&[u8]>,
) -> io::Result<()> {
    let stride = W as usize;
    let (left, top, width, height, buffer) = match previous {
        None => (0_u16, 0_u16, W as u16, H as u16, indices.to_vec()),
        Some(previous) => {
            let mut x0 = stride;
            let mut y0 = H as usize;
            let mut x1 = 0;
            let mut y1 = 0;
            for (index, (current, past)) in indices.iter().zip(previous.iter()).enumerate() {
                if current != past {
                    let x = index % stride;
                    let y = index / stride;
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
            if x0 >= x1 || y0 >= y1 {
                (0, 0, 1, 1, vec![indices[0]])
            } else if (x1 - x0) * (y1 - y0) * 5 > stride * (H as usize) * 3 {
                (0, 0, W as u16, H as u16, indices.to_vec())
            } else {
                let crop_w = x1 - x0;
                let mut crop = Vec::with_capacity(crop_w * (y1 - y0));
                for y in y0..y1 {
                    let row_start = y * stride + x0;
                    crop.extend_from_slice(&indices[row_start..row_start + crop_w]);
                }
                (x0 as u16, y0 as u16, crop_w as u16, (y1 - y0) as u16, crop)
            }
        }
    };
    let frame = Frame {
        delay,
        dispose: DisposalMethod::Keep,
        transparent: None,
        needs_user_input: false,
        top,
        left,
        width,
        height,
        interlaced: false,
        palette: None,
        buffer: Cow::Owned(buffer),
    };
    encoder
        .write_frame(&frame)
        .map_err(|error| io::Error::other(error.to_string()))
}

fn probe_frame_ids(renderer: &Renderer) -> Vec<usize> {
    let total = renderer.timeline.frames;
    let last = total - 1;
    let mut ids = vec![total.min(5).min(last)];
    if let Some(&start) = renderer.timeline.beat_starts.first() {
        ids.push((((start + IMPACT_MS) / FRAME_MS) as usize + 1).min(last));
    }
    let pick = renderer
        .view
        .beats
        .iter()
        .position(|beat| matches!(beat.strike, Strike::Damage { critical: true, .. }))
        .unwrap_or(renderer.view.beats.len() / 2);
    if let Some(&start) = renderer.timeline.beat_starts.get(pick) {
        ids.push((((start + IMPACT_MS) / FRAME_MS) as usize + 2).min(last));
    }
    ids.push((((renderer.timeline.victory_at + 540.0) / FRAME_MS) as usize).min(last));
    ids.push(last);
    ids.retain(|&id| id <= last);
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub(crate) fn render(
    root: &Path,
    snapshot: &CombatSnapshot,
    outcome: &CombatOutcome,
    path: &Path,
) -> io::Result<()> {
    let view = extract_view(snapshot, outcome)?;
    let realm_assets = assets::RealmAssets::discover(root);
    let font = realm_assets.font().cloned();
    let timeline = build_timeline(view.beats.len());
    let states = precompute_states(&view, &timeline);
    let fx = precompute_fx(&view, &realm_assets);
    let portraits = [
        realm_assets.portrait_by_id(&view.sides[0].character_id),
        realm_assets.portrait_by_id(&view.sides[1].character_id),
    ];
    let layers = [
        render_side_layer(&view.sides[0], true, portraits[0].as_ref()),
        render_side_layer(&view.sides[1], false, portraits[1].as_ref()),
    ];
    let mut renderer = Renderer {
        view,
        timeline,
        states,
        fx,
        layers,
        glow: GlowTable::new(),
        divider: divider_profile(),
        font,
        world: Canvas::new(W, H),
        stage: Canvas::new(W, H),
    };

    let probes: Vec<Canvas> = probe_frame_ids(&renderer)
        .into_iter()
        .map(|frame| {
            renderer.render_frame(frame);
            renderer.present(frame).clone()
        })
        .collect();
    let forced = forced_colors(&renderer.view);
    let (palette, lut) = build_palette(&probes, &forced);
    drop(probes);

    let mut output = Cursor::new(Vec::<u8>::new());
    {
        let mut encoder = Encoder::new(&mut output, W as u16, H as u16, &palette)
            .map_err(|error| io::Error::other(error.to_string()))?;
        encoder
            .set_repeat(Repeat::Infinite)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let mut previous: Option<Vec<u8>> = None;
        let mut pending: Option<(Vec<u8>, u16)> = None;
        let mut indices = vec![0_u8; (W * H) as usize];
        for frame in 0..renderer.timeline.frames {
            renderer.render_frame(frame);
            map_indices(renderer.present(frame), &lut, &mut indices);
            if let Some((buffer, delay)) = pending.as_mut() {
                if *buffer == indices {
                    *delay += FRAME_DELAY_CS;
                    continue;
                }
                write_diff_frame(&mut encoder, buffer, *delay, previous.as_deref())?;
                previous = Some(buffer.clone());
                buffer.copy_from_slice(&indices);
                *delay = FRAME_DELAY_CS;
            } else {
                pending = Some((indices.clone(), FRAME_DELAY_CS));
            }
        }
        if let Some((buffer, delay)) = pending.as_ref() {
            write_diff_frame(&mut encoder, buffer, *delay, previous.as_deref())?;
        }
    }
    assets::atomic_write(path, output.get_ref())
}
