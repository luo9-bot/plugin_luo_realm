use std::{collections::HashMap, io, path::Path};

use ab_glyph::PxScale;
use image::{ImageBuffer, Rgba, RgbaImage, imageops};
use imageproc::{
    drawing::{draw_text_mut, text_size},
    rect::Rect,
};

use super::{BattleRenderData, assets};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;
const PORTRAIT_WIDTH: u32 = 250;
const PORTRAIT_HEIGHT: u32 = 300;
const LEFT_X: i64 = 52;
const RIGHT_X: i64 = 658;
const PORTRAIT_Y: i64 = 168;

#[derive(Clone, Copy, Eq, PartialEq)]
enum AnimationPhase {
    Intro,
    Windup,
    Impact,
    Victory,
}

struct SkillVisual {
    icon: Option<RgbaImage>,
    effect: Option<RgbaImage>,
}

struct BattleVisuals {
    left_portrait: Option<RgbaImage>,
    right_portrait: Option<RgbaImage>,
    skills: HashMap<String, SkillVisual>,
}

pub fn render(root: &Path, data: &BattleRenderData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let visuals = prepare_visuals(&assets, data);
    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut bytes, WIDTH as u16, HEIGHT as u16, &[])
            .map_err(io::Error::other)?;
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(io::Error::other)?;

        encode_scene(
            &mut encoder,
            &assets,
            &visuals,
            data,
            None,
            AnimationPhase::Intro,
            data.left.player.base_hp,
            data.right.player.base_hp,
            80,
        )?;

        let mut previous_hp = [data.left.player.base_hp, data.right.player.base_hp];
        data.result.frames.iter().try_for_each(|action| {
            encode_scene(
                &mut encoder,
                &assets,
                &visuals,
                data,
                Some(action),
                AnimationPhase::Windup,
                previous_hp[0],
                previous_hp[1],
                10,
            )?;
            encode_scene(
                &mut encoder,
                &assets,
                &visuals,
                data,
                Some(action),
                AnimationPhase::Impact,
                action.left_hp,
                action.right_hp,
                42,
            )?;
            previous_hp = [action.left_hp, action.right_hp];
            Ok::<(), io::Error>(())
        })?;

        [90, 130].into_iter().try_for_each(|delay| {
            encode_scene(
                &mut encoder,
                &assets,
                &visuals,
                data,
                data.result.frames.last(),
                AnimationPhase::Victory,
                data.result.left_hp,
                data.result.right_hp,
                delay,
            )
        })?;
    }
    assets::atomic_write(path, &bytes)
}

#[allow(clippy::too_many_arguments)]
fn encode_scene(
    encoder: &mut gif::Encoder<&mut Vec<u8>>,
    assets: &assets::RealmAssets,
    visuals: &BattleVisuals,
    data: &BattleRenderData<'_>,
    action: Option<&crate::core::CombatFrame>,
    phase: AnimationPhase,
    left_hp: i64,
    right_hp: i64,
    delay: u16,
) -> io::Result<()> {
    let image = battle_scene(assets, visuals, data, action, phase, left_hp, right_hp);
    let mut pixels = image.into_raw();
    let mut frame = gif::Frame::from_rgba_speed(WIDTH as u16, HEIGHT as u16, &mut pixels, 30);
    frame.delay = delay;
    encoder.write_frame(&frame).map_err(io::Error::other)
}

fn prepare_visuals(assets: &assets::RealmAssets, data: &BattleRenderData<'_>) -> BattleVisuals {
    let portrait = |player_id: &str| {
        assets.portrait(player_id).map(|image| {
            image
                .resize_exact(
                    PORTRAIT_WIDTH,
                    PORTRAIT_HEIGHT,
                    imageops::FilterType::Lanczos3,
                )
                .to_rgba8()
        })
    };
    let skills = data
        .result
        .frames
        .iter()
        .map(|frame| frame.skill.as_str())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .map(|skill| {
            let icon = assets.skill_icon(skill).map(|image| {
                image
                    .resize_exact(92, 92, imageops::FilterType::Lanczos3)
                    .to_rgba8()
            });
            let effect = assets.skill_effect(skill).map(|image| {
                image
                    .resize_exact(150, 150, imageops::FilterType::Lanczos3)
                    .to_rgba8()
            });
            (skill.to_owned(), SkillVisual { icon, effect })
        })
        .collect();

    BattleVisuals {
        left_portrait: portrait(&data.left.player.user_id),
        right_portrait: portrait(&data.right.player.user_id),
        skills,
    }
}

#[allow(clippy::too_many_arguments)]
fn battle_scene(
    assets: &assets::RealmAssets,
    visuals: &BattleVisuals,
    data: &BattleRenderData<'_>,
    action: Option<&crate::core::CombatFrame>,
    phase: AnimationPhase,
    left_hp: i64,
    right_hp: i64,
) -> RgbaImage {
    let mut image = ImageBuffer::from_pixel(WIDTH, HEIGHT, Rgba([237, 234, 225, 255]));
    draw_stage(&mut image);

    let attacker_is_left = action
        .map(|frame| frame.attacker_id == data.left.player.user_id)
        .unwrap_or(false);
    let (left_offset, right_offset) = motion_offsets(action, phase, attacker_is_left);
    draw_portrait(
        &mut image,
        visuals.left_portrait.as_ref(),
        LEFT_X + left_offset,
        PORTRAIT_Y,
        Rgba([41, 113, 133, 255]),
    );
    draw_portrait(
        &mut image,
        visuals.right_portrait.as_ref(),
        RIGHT_X + right_offset,
        PORTRAIT_Y,
        Rgba([151, 65, 61, 255]),
    );

    health_bar(
        &mut image,
        52,
        125,
        left_hp,
        data.left.player.base_hp,
        Rgba([34, 139, 104, 255]),
    );
    health_bar(
        &mut image,
        658,
        125,
        right_hp,
        data.right.player.base_hp,
        Rgba([187, 63, 57, 255]),
    );

    if let Some(action) = action {
        draw_action(
            &mut image,
            visuals.skills.get(&action.skill),
            action,
            phase,
            attacker_is_left,
        );
    }
    if phase == AnimationPhase::Victory {
        draw_victory_overlay(&mut image, data);
    }
    draw_labels(&mut image, assets, data, action, phase, left_hp, right_hp);
    image
}

fn draw_stage(image: &mut RgbaImage) {
    fill(image, 0, 0, WIDTH, 70, Rgba([31, 38, 36, 255]));
    fill(image, 0, 70, WIDTH, 92, Rgba([224, 222, 213, 255]));
    fill(image, 0, 500, WIDTH, 40, Rgba([45, 50, 47, 255]));
    fill(image, 365, 162, 230, 338, Rgba([231, 228, 217, 255]));
    fill(image, 478, 162, 4, 42, Rgba([193, 158, 70, 255]));
    fill(image, 478, 382, 4, 118, Rgba([193, 158, 70, 255]));
}

fn motion_offsets(
    action: Option<&crate::core::CombatFrame>,
    phase: AnimationPhase,
    attacker_is_left: bool,
) -> (i64, i64) {
    let Some(action) = action else {
        return (0, 0);
    };
    match phase {
        AnimationPhase::Windup if attacker_is_left => (-10, 0),
        AnimationPhase::Windup => (0, 10),
        AnimationPhase::Impact if attacker_is_left => (28, impact_shake(action.round)),
        AnimationPhase::Impact => (impact_shake(action.round), -28),
        _ => (0, 0),
    }
}

fn impact_shake(round: u32) -> i64 {
    if round.is_multiple_of(2) { 8 } else { -8 }
}

fn draw_portrait(
    image: &mut RgbaImage,
    portrait: Option<&RgbaImage>,
    x: i64,
    y: i64,
    border: Rgba<u8>,
) {
    fill(
        image,
        (x - 5).max(0) as u32,
        (y - 5).max(0) as u32,
        PORTRAIT_WIDTH + 10,
        PORTRAIT_HEIGHT + 10,
        border,
    );
    if let Some(portrait) = portrait {
        imageops::overlay(image, portrait, x, y);
    } else {
        fill(
            image,
            x.max(0) as u32,
            y.max(0) as u32,
            PORTRAIT_WIDTH,
            PORTRAIT_HEIGHT,
            Rgba([102, 108, 103, 255]),
        );
    }
}

fn draw_action(
    image: &mut RgbaImage,
    visual: Option<&SkillVisual>,
    action: &crate::core::CombatFrame,
    phase: AnimationPhase,
    attacker_is_left: bool,
) {
    let icon_size = if phase == AnimationPhase::Windup {
        76
    } else {
        92
    };
    if let Some(icon) = visual.and_then(|visual| visual.icon.as_ref()) {
        let icon = imageops::resize(icon, icon_size, icon_size, imageops::FilterType::Lanczos3);
        imageops::overlay(image, &icon, (WIDTH as i64 - icon_size as i64) / 2, 214);
    }
    if phase != AnimationPhase::Impact {
        return;
    }

    let defender_x = if attacker_is_left { RIGHT_X } else { LEFT_X };
    let flash = ImageBuffer::from_pixel(
        PORTRAIT_WIDTH,
        PORTRAIT_HEIGHT,
        Rgba([206, 52, 44, if action.critical { 105 } else { 62 }]),
    );
    imageops::overlay(image, &flash, defender_x, PORTRAIT_Y);
    if let Some(effect) = visual.and_then(|visual| visual.effect.as_ref()) {
        imageops::overlay(
            image,
            effect,
            defender_x + (PORTRAIT_WIDTH as i64 - effect.width() as i64) / 2,
            PORTRAIT_Y + 68,
        );
    }
}

fn draw_victory_overlay(image: &mut RgbaImage, data: &BattleRenderData<'_>) {
    let left_won = data.result.winner_id == data.left.player.user_id;
    let loser_x = if left_won { RIGHT_X } else { LEFT_X };
    let shade = ImageBuffer::from_pixel(PORTRAIT_WIDTH, PORTRAIT_HEIGHT, Rgba([20, 24, 23, 145]));
    imageops::overlay(image, &shade, loser_x, PORTRAIT_Y);
    let winner_x = if left_won { LEFT_X } else { RIGHT_X };
    stroke(
        image,
        winner_x as u32 - 8,
        PORTRAIT_Y as u32 - 8,
        PORTRAIT_WIDTH + 16,
        PORTRAIT_HEIGHT + 16,
        4,
        Rgba([214, 169, 53, 255]),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_labels(
    image: &mut RgbaImage,
    assets: &assets::RealmAssets,
    data: &BattleRenderData<'_>,
    action: Option<&crate::core::CombatFrame>,
    phase: AnimationPhase,
    left_hp: i64,
    right_hp: i64,
) {
    let Some(font) = assets.font() else {
        return;
    };
    text(
        image,
        font,
        27.0,
        24,
        16,
        Rgba([247, 246, 241, 255]),
        "LUO REALM / 天命对决",
    );
    let round = action.map(|frame| frame.round).unwrap_or(0);
    centered_text(
        image,
        font,
        20.0,
        480,
        22,
        180,
        Rgba([225, 191, 102, 255]),
        if phase == AnimationPhase::Victory {
            "胜负已定".into()
        } else if round == 0 {
            "对决开始".into()
        } else {
            format!("第 {round} 回合")
        }
        .as_str(),
    );
    combatant_labels(image, font, data, left_hp, right_hp);

    if phase == AnimationPhase::Intro {
        centered_text(
            image,
            font,
            46.0,
            480,
            255,
            150,
            Rgba([157, 112, 26, 255]),
            "VS",
        );
    }

    if let Some(action) = action {
        centered_text(
            image,
            font,
            fit_size(font, &action.skill, 180, 22.0, 15.0),
            480,
            320,
            180,
            Rgba([43, 46, 43, 255]),
            &action.skill,
        );
        if phase == AnimationPhase::Impact {
            let defender_center = if action.attacker_id == data.left.player.user_id {
                RIGHT_X as i32 + PORTRAIT_WIDTH as i32 / 2
            } else {
                LEFT_X as i32 + PORTRAIT_WIDTH as i32 / 2
            };
            let badge = ImageBuffer::from_pixel(176, 44, Rgba([30, 33, 31, 178]));
            imageops::overlay(image, &badge, i64::from(defender_center - 88), 176);
            centered_text(
                image,
                font,
                if action.critical { 31.0 } else { 27.0 },
                defender_center,
                181,
                190,
                if action.critical {
                    Rgba([231, 178, 45, 255])
                } else {
                    Rgba([255, 238, 224, 255])
                },
                &format!(
                    "{}-{}",
                    if action.critical { "暴击 " } else { "" },
                    action.damage
                ),
            );
        }
    }
    if phase == AnimationPhase::Victory {
        let winner = if data.result.winner_id == data.left.player.user_id {
            &data.left.player.display_name
        } else {
            &data.right.player.display_name
        };
        centered_text(
            image,
            font,
            26.0,
            480,
            382,
            220,
            Rgba([142, 96, 16, 255]),
            &format!("{winner} 胜"),
        );
    }
}

fn combatant_labels(
    image: &mut RgbaImage,
    font: &ab_glyph::FontArc,
    data: &BattleRenderData<'_>,
    left_hp: i64,
    right_hp: i64,
) {
    text(
        image,
        font,
        fit_size(font, &data.left.player.display_name, 250, 25.0, 17.0),
        52,
        76,
        Rgba([31, 63, 70, 255]),
        &data.left.player.display_name,
    );
    text(
        image,
        font,
        17.0,
        52,
        103,
        Rgba([56, 86, 91, 255]),
        data.left_system,
    );
    text(
        image,
        font,
        fit_size(font, &data.right.player.display_name, 250, 25.0, 17.0),
        658,
        76,
        Rgba([91, 43, 42, 255]),
        &data.right.player.display_name,
    );
    text(
        image,
        font,
        17.0,
        658,
        103,
        Rgba([102, 59, 56, 255]),
        data.right_system,
    );
    text(
        image,
        font,
        15.0,
        52,
        141,
        Rgba([42, 49, 47, 255]),
        &format!("生命 {}/{}", left_hp.max(0), data.left.player.base_hp),
    );
    text(
        image,
        font,
        15.0,
        658,
        141,
        Rgba([42, 49, 47, 255]),
        &format!("生命 {}/{}", right_hp.max(0), data.right.player.base_hp),
    );
    text(
        image,
        font,
        16.0,
        52,
        506,
        Rgba([220, 230, 228, 255]),
        &format!("战力 {:.0}", data.left.power),
    );
    text(
        image,
        font,
        16.0,
        658,
        506,
        Rgba([238, 221, 218, 255]),
        &format!("战力 {:.0}", data.right.power),
    );
}

fn health_bar(image: &mut RgbaImage, x: u32, y: u32, health: i64, maximum: i64, color: Rgba<u8>) {
    fill(image, x, y, 250, 12, Rgba([181, 180, 171, 255]));
    let ratio = health.max(0) as f64 / maximum.max(1) as f64;
    fill(
        image,
        x,
        y,
        (250.0 * ratio.clamp(0.0, 1.0)) as u32,
        12,
        color,
    );
}

fn fill(image: &mut RgbaImage, x: u32, y: u32, width: u32, height: u32, color: Rgba<u8>) {
    if width == 0 || height == 0 {
        return;
    }
    imageproc::drawing::draw_filled_rect_mut(
        image,
        Rect::at(x as i32, y as i32).of_size(width, height),
        color,
    );
}

#[allow(clippy::too_many_arguments)]
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
    fill(image, x, y + height - thickness, width, thickness, color);
    fill(image, x, y, thickness, height, color);
    fill(image, x + width - thickness, y, thickness, height, color);
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
fn centered_text(
    image: &mut RgbaImage,
    font: &ab_glyph::FontArc,
    size: f32,
    center_x: i32,
    y: i32,
    max_width: u32,
    color: Rgba<u8>,
    value: &str,
) {
    let size = fit_size(font, value, max_width, size, 13.0);
    let (width, _) = text_size(PxScale::from(size), font, value);
    text(
        image,
        font,
        size,
        center_x - width as i32 / 2,
        y,
        color,
        value,
    );
}

fn fit_size(
    font: &ab_glyph::FontArc,
    value: &str,
    max_width: u32,
    preferred: f32,
    minimum: f32,
) -> f32 {
    std::iter::successors(Some(preferred), |size| Some(size - 1.0))
        .take_while(|size| *size >= minimum)
        .find(|size| text_size(PxScale::from(*size), font, value).0 <= max_width)
        .unwrap_or(minimum)
}
