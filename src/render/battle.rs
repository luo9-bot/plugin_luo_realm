use std::{io, path::Path};

use ab_glyph::PxScale;
use gif::{Encoder, Frame, Repeat};
use image::{ImageBuffer, Rgba, RgbaImage, imageops};
use imageproc::{drawing::draw_text_mut, rect::Rect};

use crate::combat::{CombatEventKind, CombatOutcome, CombatSnapshot};

use super::assets;

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;
const PORTRAIT_W: u32 = 230;
const PORTRAIT_H: u32 = 290;

pub fn render(
    root: &Path,
    snapshot: &CombatSnapshot,
    outcome: &CombatOutcome,
    path: &Path,
) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let mut bytes = Vec::new();
    {
        let mut encoder =
            Encoder::new(&mut bytes, WIDTH as u16, HEIGHT as u16, &[]).map_err(io::Error::other)?;
        encoder
            .set_repeat(Repeat::Infinite)
            .map_err(io::Error::other)?;
        let mut previous = None;
        let first = scene(&assets, snapshot, outcome, previous, None, None);
        write_frame(&mut encoder, first, 80)?;
        for (index, event) in outcome.events.iter().enumerate() {
            let image = scene(
                &assets,
                snapshot,
                outcome,
                Some(event),
                previous,
                Some(index),
            );
            write_frame(&mut encoder, image, event_delay(&event.kind))?;
            previous = Some(event);
        }
        let victory = scene(
            &assets,
            snapshot,
            outcome,
            None,
            previous,
            Some(outcome.events.len()),
        );
        write_frame(&mut encoder, victory, 120)?;
    }
    assets::atomic_write(path, &bytes)
}

fn write_frame(
    encoder: &mut Encoder<&mut Vec<u8>>,
    image: RgbaImage,
    delay: u16,
) -> io::Result<()> {
    let mut pixels = image.into_raw();
    let mut frame = Frame::from_rgba_speed(WIDTH as u16, HEIGHT as u16, &mut pixels, 20);
    frame.delay = delay;
    encoder.write_frame(&frame).map_err(io::Error::other)
}

fn scene(
    assets: &assets::RealmAssets,
    snapshot: &CombatSnapshot,
    outcome: &CombatOutcome,
    event: Option<&crate::combat::CombatEvent>,
    previous: Option<&crate::combat::CombatEvent>,
    event_index: Option<usize>,
) -> RgbaImage {
    let mut image = ImageBuffer::from_pixel(WIDTH, HEIGHT, Rgba([18, 24, 29, 255]));
    fill(&mut image, 0, 0, WIDTH, 86, Rgba([28, 38, 44, 255]));
    fill(&mut image, 0, 86, WIDTH, 8, Rgba([196, 145, 50, 255]));
    fill(&mut image, 0, 500, WIDTH, 40, Rgba([11, 15, 18, 255]));
    let left = &snapshot.combatants[0];
    let right = &snapshot.combatants[1];
    let action_source = event.and_then(|event| event.source_id.as_deref());
    let left_active = action_source == Some(left.combatant_id.as_str());
    let right_active = action_source == Some(right.combatant_id.as_str());
    let left_x = if left_active {
        72
    } else if right_active {
        42
    } else {
        52
    };
    let right_x = if right_active {
        638
    } else if left_active {
        668
    } else {
        658
    };
    draw_portrait(
        &mut image,
        assets,
        &left.avatar_id,
        left_x,
        162,
        Rgba([54, 151, 190, 255]),
    );
    draw_portrait(
        &mut image,
        assets,
        &right.avatar_id,
        right_x,
        162,
        Rgba([194, 82, 76, 255]),
    );
    let left_health = health_at(snapshot, outcome, &left.combatant_id, event_index);
    let right_health = health_at(snapshot, outcome, &right.combatant_id, event_index);
    bar(
        &mut image,
        52,
        122,
        left_health.0,
        left_health.1,
        Rgba([52, 185, 132, 255]),
    );
    bar(
        &mut image,
        658,
        122,
        right_health.0,
        right_health.1,
        Rgba([205, 82, 76, 255]),
    );
    if let Some(event) = event {
        draw_event_effect(&mut image, event, left, right);
    }
    if let Some(font) = assets.font() {
        text(
            &mut image,
            font,
            28.0,
            28,
            24,
            Rgba([244, 239, 222, 255]),
            "LUO REALM / 事件战斗",
        );
        text(
            &mut image,
            font,
            18.0,
            52,
            94,
            Rgba([196, 224, 231, 255]),
            &left.display_name,
        );
        text(
            &mut image,
            font,
            18.0,
            658,
            94,
            Rgba([242, 213, 206, 255]),
            &right.display_name,
        );
        let label = event_label(event, previous, outcome);
        centered(
            &mut image,
            font,
            24.0,
            480,
            344,
            520,
            Rgba([245, 224, 171, 255]),
            &label,
        );
        text(
            &mut image,
            font,
            16.0,
            52,
            512,
            Rgba([191, 211, 207, 255]),
            &format!("生命 {}/{}", left_health.0, left_health.1),
        );
        text(
            &mut image,
            font,
            16.0,
            658,
            512,
            Rgba([231, 203, 199, 255]),
            &format!("生命 {}/{}", right_health.0, right_health.1),
        );
    }
    image
}

fn draw_portrait(
    image: &mut RgbaImage,
    assets: &assets::RealmAssets,
    avatar_id: &str,
    x: i64,
    y: i64,
    border: Rgba<u8>,
) {
    fill(
        image,
        (x - 8).max(0) as u32,
        (y - 8).max(0) as u32,
        PORTRAIT_W + 16,
        PORTRAIT_H + 16,
        border,
    );
    if let Some(portrait) = assets.portrait_by_id(avatar_id) {
        let portrait = portrait
            .resize_exact(PORTRAIT_W, PORTRAIT_H, imageops::FilterType::Lanczos3)
            .to_rgba8();
        imageops::overlay(image, &portrait, x, y);
    } else {
        fill(
            image,
            x as u32,
            y as u32,
            PORTRAIT_W,
            PORTRAIT_H,
            Rgba([64, 75, 81, 255]),
        );
    }
}

fn draw_event_effect(
    image: &mut RgbaImage,
    event: &crate::combat::CombatEvent,
    left: &crate::combat::CombatantSnapshot,
    _right: &crate::combat::CombatantSnapshot,
) {
    let target_left = event.target_id.as_deref() == Some(left.combatant_id.as_str());
    let x = if target_left { 52 } else { 658 };
    match event.kind {
        CombatEventKind::DamageApplied { .. } => fill(
            image,
            x,
            162,
            PORTRAIT_W,
            PORTRAIT_H,
            Rgba([216, 62, 54, 82]),
        ),
        CombatEventKind::HealingApplied { .. } => fill(
            image,
            x,
            162,
            PORTRAIT_W,
            PORTRAIT_H,
            Rgba([49, 194, 143, 70]),
        ),
        CombatEventKind::ShieldChanged { delta, .. } if delta >= 0 => fill(
            image,
            x,
            162,
            PORTRAIT_W,
            PORTRAIT_H,
            Rgba([49, 194, 143, 70]),
        ),
        CombatEventKind::Dodged | CombatEventKind::Moved { .. } => stroke(
            image,
            x.saturating_sub(12),
            150,
            PORTRAIT_W + 24,
            PORTRAIT_H + 24,
            5,
            Rgba([225, 215, 124, 220]),
        ),
        CombatEventKind::Blocked { .. } => stroke(
            image,
            x.saturating_sub(12),
            150,
            PORTRAIT_W + 24,
            PORTRAIT_H + 24,
            7,
            Rgba([124, 176, 227, 230]),
        ),
        CombatEventKind::ControlBroken | CombatEventKind::DomainEstablished { .. } => stroke(
            image,
            x.saturating_sub(16),
            144,
            PORTRAIT_W + 32,
            PORTRAIT_H + 32,
            4,
            Rgba([197, 112, 240, 230]),
        ),
        _ => {}
    }
}

fn event_label(
    event: Option<&crate::combat::CombatEvent>,
    previous: Option<&crate::combat::CombatEvent>,
    outcome: &CombatOutcome,
) -> String {
    let Some(event) = event else {
        return format!("胜者：队伍 {}", outcome.winner_team);
    };
    match &event.kind {
        CombatEventKind::SkillCast { skill_name, .. } => format!("释放 · {skill_name}"),
        CombatEventKind::DamageApplied {
            amount, critical, ..
        } => format!("{}伤害 {}", if *critical { "暴击 " } else { "" }, amount),
        CombatEventKind::HealingApplied { amount } => format!("恢复 {}", amount),
        CombatEventKind::Dodged => "闪避成功".into(),
        CombatEventKind::Blocked { prevented } => format!("格挡 {}", prevented),
        CombatEventKind::ShieldChanged { delta, remaining } => {
            format!("护盾 {} · {}", delta, remaining)
        }
        CombatEventKind::DomainEstablished { .. } => "领域展开".into(),
        CombatEventKind::DomainContested { .. } => "领域争夺".into(),
        CombatEventKind::EntitySummoned { display_name, .. } => format!("召唤 {display_name}"),
        CombatEventKind::BattleEnded { winner_team, .. } => format!("队伍 {} 获胜", winner_team),
        _ => previous
            .map(|_| "状态变化".into())
            .unwrap_or_else(|| "战斗开始".into()),
    }
}

fn health_at(
    snapshot: &CombatSnapshot,
    outcome: &CombatOutcome,
    id: &str,
    event_index: Option<usize>,
) -> (i64, i64) {
    let maximum = snapshot
        .combatants
        .iter()
        .find(|combatant| combatant.combatant_id == id)
        .map(|combatant| combatant.attributes.max_health)
        .unwrap_or(1);
    let mut health = maximum;
    let limit = event_index
        .map(|index| index.saturating_add(1))
        .unwrap_or(outcome.events.len())
        .min(outcome.events.len());
    for event in outcome.events.iter().take(limit) {
        if event.target_id.as_deref() != Some(id) {
            continue;
        }
        match event.kind {
            CombatEventKind::DamageApplied { amount, .. } => {
                health = health.saturating_sub(amount);
            }
            CombatEventKind::HealingApplied { amount } => {
                health = (health + amount).min(maximum);
            }
            _ => {}
        }
    }
    (health, maximum)
}

fn event_delay(kind: &CombatEventKind) -> u16 {
    match kind {
        CombatEventKind::SkillCast { .. } | CombatEventKind::DamageApplied { .. } => 18,
        CombatEventKind::BattleEnded { .. } => 90,
        _ => 10,
    }
}

fn fill(image: &mut RgbaImage, x: u32, y: u32, width: u32, height: u32, color: Rgba<u8>) {
    imageproc::drawing::draw_filled_rect_mut(
        image,
        Rect::at(x as i32, y as i32).of_size(width, height),
        color,
    );
}

fn stroke(
    image: &mut RgbaImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    thickness: u32,
    color: Rgba<u8>,
) {
    fill(image, x, y, width, thickness, color);
    fill(
        image,
        x,
        y + height.saturating_sub(thickness),
        width,
        thickness,
        color,
    );
    fill(image, x, y, thickness, height, color);
    fill(
        image,
        x + width.saturating_sub(thickness),
        y,
        thickness,
        height,
        color,
    );
}

fn bar(image: &mut RgbaImage, x: u32, y: u32, value: i64, maximum: i64, color: Rgba<u8>) {
    fill(image, x, y, 230, 12, Rgba([69, 78, 82, 255]));
    fill(
        image,
        x,
        y,
        (230.0 * (value.max(0) as f64 / maximum.max(1) as f64).clamp(0.0, 1.0)) as u32,
        12,
        color,
    );
}

fn text(
    image: &mut RgbaImage,
    font: &ab_glyph::FontArc,
    size: f32,
    x: i32,
    y: i32,
    color: Rgba<u8>,
    value: &str,
) {
    draw_text_mut(image, color, x, y, PxScale::from(size), font, value);
}

#[allow(clippy::too_many_arguments)]
fn centered(
    image: &mut RgbaImage,
    font: &ab_glyph::FontArc,
    size: f32,
    center_x: i32,
    y: i32,
    max_width: u32,
    color: Rgba<u8>,
    value: &str,
) {
    let (width, _) = imageproc::drawing::text_size(PxScale::from(size), font, value);
    let x = center_x - (width.min(max_width) as i32 / 2);
    text(image, font, size, x, y, color, value);
}
