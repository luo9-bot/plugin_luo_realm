//! 角色档案卡：头像 + 称号行 + 三列圆圈图标数值。
//!
//! 与命令卡片（`card.rs`）共用「洛界典籍」视觉系统：暖炭底、山水
//! 剪影、细金线。头像的形状（圆形双圈 / 方形相框 / 无框直出）与
//! 立绘填充（等比裁剪 / 完整置入 / 拉伸）由 `config.toml` 的
//! `profile_card` 节配置；立绘默认等比裁剪，任何比例都不会被压扁。

use std::io;
use std::path::Path;

use image::{DynamicImage, ImageBuffer, Rgba, imageops};

use super::{
    ProfileRenderData, assets,
    card::{self, StatGlyph},
    portrait_style,
};
use crate::config::{PortraitFill, PortraitShape};

/// 头像中心与半宽（圆形模式下即半径）。
const PORTRAIT_CENTER: (i32, i32) = (206, 274);
const PORTRAIT_RADIUS: i32 = 140;
/// 右侧信息区起点。
const INFO_X: i32 = 396;
const INFO_RIGHT: i32 = 896;
/// 立绘未覆盖处与占位图的回填底色。
const PORTRAIT_BACKDROP: Rgba<u8> = Rgba([24, 20, 14, 255]);

pub fn render(root: &Path, data: &ProfileRenderData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let accent = card::system_color(data.system_id);
    let mut image = card::blank("profile");

    let (cx, cy) = PORTRAIT_CENTER;
    draw_portrait(
        &assets,
        &mut image,
        &data.player.character_id,
        &data.player.user_id,
        cx,
        cy,
        portrait_style(root),
    );

    let Some(font) = assets.font() else {
        return card::encode(path, &image);
    };

    card::label(
        &mut image,
        font,
        38.0,
        INFO_X,
        98,
        card::GOLD_BRIGHT,
        &truncate_display_name(&data.player.display_name),
    );
    card::diamond_filled(&mut image, INFO_X + 9, 172, 5, accent);
    card::label(
        &mut image,
        font,
        22.0,
        INFO_X + 30,
        158,
        card::TEXT_SUB,
        &format!(
            "{} · {} · 等级 {}",
            data.system_name, data.realm_name, data.player.level
        ),
    );
    card::divider(&mut image, INFO_X, INFO_RIGHT, 212, card::GOLD);

    let columns: [(card::StatGlyph, &str, String); 3] = [
        (
            StatGlyph::Meditate,
            "修为",
            card::format_number(data.progress as f64),
        ),
        (StatGlyph::Swords, "战力", card::format_number(data.power)),
        (
            StatGlyph::Coin,
            "金币",
            card::format_number(data.player.coins as f64),
        ),
    ];
    let centers = [472_i32, 646, 820];
    for (index, (glyph, caption, value)) in columns.into_iter().enumerate() {
        let cx = centers[index];
        card::ring_icon(&mut image, cx, 258, 26, card::GOLD, glyph);
        card::label_centered(
            &mut image,
            font,
            18.0,
            cx,
            298,
            card::TEXT_SUB,
            caption,
            2.0,
        );
        card::label_centered(
            &mut image,
            font,
            28.0,
            cx,
            320,
            card::GOLD_BRIGHT,
            &value,
            1.0,
        );
    }
    for &gap_x in &[559_i32, 733] {
        card::vline(&mut image, gap_x, 228, 352, card::LINE_FAINT);
    }

    card::divider(&mut image, INFO_X, INFO_RIGHT, 394, card::GOLD);
    let player = data.player;
    card::label_centered(
        &mut image,
        font,
        18.0,
        (INFO_X + INFO_RIGHT) / 2,
        416,
        card::TEXT_SUB,
        &format!(
            "生命 {} · 攻击 {} · 防御 {} · 刻印 {} · 胜 {} / 负 {}",
            player.base_hp,
            player.base_attack,
            player.base_defense,
            player.marks,
            player.wins,
            player.losses
        ),
        2.0,
    );

    card::encode(path, &image)
}

/// 名字过长时截断，避免大字号下压过右侧边界。
fn truncate_display_name(name: &str) -> String {
    const LIMIT: usize = 12;
    if name.chars().count() <= LIMIT {
        name.to_owned()
    } else {
        let mut truncated = name.chars().take(LIMIT).collect::<String>();
        truncated.push('…');
        truncated
    }
}

/// 头像绘制：先按形状画包边，再贴立绘，最后补画方形相框。
fn draw_portrait(
    assets: &assets::RealmAssets,
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    character_id: &str,
    user_id: &str,
    cx: i32,
    cy: i32,
    (shape, fill): (PortraitShape, PortraitFill),
) {
    if shape == PortraitShape::Circle {
        imageproc::drawing::draw_hollow_circle_mut(
            image,
            (cx, cy),
            PORTRAIT_RADIUS + 18,
            card::GOLD_DIM,
        );
        imageproc::drawing::draw_hollow_circle_mut(
            image,
            (cx, cy),
            PORTRAIT_RADIUS + 10,
            card::GOLD,
        );
    }
    let portrait = assets
        .portrait_by_id(character_id)
        .or_else(|| assets.portrait(user_id));
    match portrait {
        Some(portrait) => stamp_portrait(image, &portrait, cx, cy, PORTRAIT_RADIUS, shape, fill),
        None => placeholder_figure(image, cx, cy, PORTRAIT_RADIUS, shape),
    }
    if shape == PortraitShape::Square {
        let half = PORTRAIT_RADIUS;
        let mut frame = |inset: i32, color| {
            let (x0, y0) = (cx - half - inset, cy - half - inset);
            let span = half * 2 + inset * 2;
            card::hline(image, x0, x0 + span, y0, color);
            card::hline(image, x0, x0 + span, y0 + span, color);
            card::vline(image, x0, y0, y0 + span, color);
            card::vline(image, x0 + span, y0, y0 + span, color);
        };
        frame(8, card::GOLD_DIM);
        frame(0, card::GOLD);
    }
}

/// 目标像素是否落在形状裁剪区内。
fn in_mask(shape: PortraitShape, dx: i32, dy: i32, radius: i32) -> bool {
    match shape {
        PortraitShape::Circle => {
            let offset_x = dx as f32 - radius as f32 + 0.5;
            let offset_y = dy as f32 - radius as f32 + 0.5;
            offset_x * offset_x + offset_y * offset_y <= (radius * radius) as f32
        }
        PortraitShape::Square | PortraitShape::Plain => true,
    }
}

/// 将立绘按配置的填充方式贴入头像区域：
///
/// - `cover`：等比放大至铺满并居中裁剪——任何宽高比都保持原比例；
/// - `contain`：等比缩放完整置入，立绘未覆盖处回填暗色；
/// - `stretch`：强制拉伸铺满（旧行为，仅建议方形立绘使用）。
fn stamp_portrait(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    source: &DynamicImage,
    cx: i32,
    cy: i32,
    radius: i32,
    shape: PortraitShape,
    fill: PortraitFill,
) {
    let size = (radius * 2) as u32;
    let (source_width, source_height) = (source.width() as f32, source.height() as f32);
    let (draw_width, draw_height) = match fill {
        PortraitFill::Stretch => (size as f32, size as f32),
        PortraitFill::Cover => {
            let scale = (size as f32 / source_width).max(size as f32 / source_height);
            (source_width * scale, source_height * scale)
        }
        PortraitFill::Contain => {
            let scale = (size as f32 / source_width).min(size as f32 / source_height);
            (source_width * scale, source_height * scale)
        }
    };
    let offset_x = (size as f32 - draw_width) / 2.0;
    let offset_y = (size as f32 - draw_height) / 2.0;
    let resized = source
        .resize_exact(
            draw_width.round().max(1.0) as u32,
            draw_height.round().max(1.0) as u32,
            imageops::FilterType::Lanczos3,
        )
        .to_rgba8();

    for dy in 0..size as i32 {
        for dx in 0..size as i32 {
            if !in_mask(shape, dx, dy, radius) {
                continue;
            }
            let px = (cx - radius + dx) as u32;
            let py = (cy - radius + dy) as u32;
            let local_x = dx as f32 - offset_x;
            let local_y = dy as f32 - offset_y;
            let inside =
                local_x >= 0.0 && local_y >= 0.0 && local_x < draw_width && local_y < draw_height;
            if !inside {
                image.put_pixel(px, py, PORTRAIT_BACKDROP);
                continue;
            }
            let sample_x = ((local_x / draw_width) * resized.width() as f32)
                .floor()
                .clamp(0.0, resized.width() as f32 - 1.0) as u32;
            let sample_y = ((local_y / draw_height) * resized.height() as f32)
                .floor()
                .clamp(0.0, resized.height() as f32 - 1.0) as u32;
            let pixel = resized.get_pixel(sample_x, sample_y);
            if pixel.0[3] < 250 {
                image.put_pixel(px, py, PORTRAIT_BACKDROP);
                continue;
            }
            image.put_pixel(px, py, *pixel);
        }
    }
}

/// 无立绘时的占位：暗色底面上一具极简的人形剪影。
fn placeholder_figure(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    cx: i32,
    cy: i32,
    radius: i32,
    shape: PortraitShape,
) {
    let head_center = (0_i32, -44_i32);
    let head_radius_sq = (34.0_f32).powi(2);
    let shoulder_center = (0_i32, 116_i32);
    let shoulder_radius_sq = (88.0_f32).powi(2);
    for dy in -radius..radius {
        for dx in -radius..radius {
            if !in_mask(shape, dx, dy, radius) {
                continue;
            }
            let head_dx = (dx - head_center.0) as f32;
            let head_dy = (dy - head_center.1) as f32;
            let shoulder_dx = (dx - shoulder_center.0) as f32;
            let shoulder_dy = (dy - shoulder_center.1) as f32;
            let in_figure = head_dx * head_dx + head_dy * head_dy <= head_radius_sq
                || shoulder_dx * shoulder_dx + shoulder_dy * shoulder_dy <= shoulder_radius_sq;
            image.put_pixel(
                (cx + dx) as u32,
                (cy + dy) as u32,
                if in_figure {
                    card::FIGURE
                } else {
                    PORTRAIT_BACKDROP
                },
            );
        }
    }
}
