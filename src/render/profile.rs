//! 角色档案卡：圆框头像 + 称号行 + 三列圆圈图标数值。
//!
//! 与命令卡片（`card.rs`）共用「洛界典籍」视觉系统：暖炭底、山水
//! 剪影、细金线。数值列借鉴名录式排印——圆圈线性图标、暗金标签、
//! 亮金数值，列间以细竖线分隔。

use std::io;
use std::path::Path;

use image::{DynamicImage, ImageBuffer, Rgba, imageops};

use super::{
    ProfileRenderData, assets,
    card::{self, StatGlyph},
};

const PORTRAIT_CENTER: (i32, i32) = (206, 274);
const PORTRAIT_RADIUS: i32 = 140;
/// 右侧信息区起点。
const INFO_X: i32 = 396;
const INFO_RIGHT: i32 = 896;

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

/// 圆形头像：双细金圈夹一层山水底，立绘圆形裁剪贴入；缺失时画剪影人形。
fn draw_portrait(
    assets: &assets::RealmAssets,
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    character_id: &str,
    user_id: &str,
    cx: i32,
    cy: i32,
) {
    imageproc::drawing::draw_hollow_circle_mut(
        image,
        (cx, cy),
        PORTRAIT_RADIUS + 18,
        card::GOLD_DIM,
    );
    imageproc::drawing::draw_hollow_circle_mut(image, (cx, cy), PORTRAIT_RADIUS + 10, card::GOLD);
    let portrait = assets
        .portrait_by_id(character_id)
        .or_else(|| assets.portrait(user_id));
    match portrait {
        Some(portrait) => stamp_circular(image, &portrait, cx, cy, PORTRAIT_RADIUS),
        None => placeholder_figure(image, cx, cy, PORTRAIT_RADIUS),
    }
}

/// 将立绘圆形裁剪后贴入画布：保留角色本身的图像与颜色，
/// 圆形外的像素保持原样，由双细金圈收束边缘。
fn stamp_circular(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    source: &DynamicImage,
    cx: i32,
    cy: i32,
    radius: i32,
) {
    let size = (radius * 2) as u32;
    let resized = source
        .resize_exact(size, size, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let radius_sq = (radius * radius) as f32;
    for dy in 0..size as i32 {
        for dx in 0..size as i32 {
            let offset_x = dx as f32 - radius as f32 + 0.5;
            let offset_y = dy as f32 - radius as f32 + 0.5;
            if offset_x * offset_x + offset_y * offset_y > radius_sq {
                continue;
            }
            let pixel = resized.get_pixel(dx as u32, dy as u32);
            if pixel.0[3] < 250 {
                continue;
            }
            let px = (cx - radius + dx) as u32;
            let py = (cy - radius + dy) as u32;
            image.put_pixel(px, py, *pixel);
        }
    }
}

/// 无立绘时的占位：暗色圆面上一具极简的人形剪影。
fn placeholder_figure(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, cx: i32, cy: i32, radius: i32) {
    let backdrop = Rgba([24, 20, 14, 255]);
    let radius_sq = (radius * radius) as f32;
    for dy in -radius..radius {
        for dx in -radius..radius {
            let distance_sq = (dx * dx + dy * dy) as f32;
            if distance_sq > radius_sq {
                continue;
            }
            image.put_pixel((cx + dx) as u32, (cy + dy) as u32, backdrop);
        }
    }
    let head_center = (0_i32, -44_i32);
    let head_radius_sq = (34.0_f32).powi(2);
    let shoulder_center = (0_i32, 116_i32);
    let shoulder_radius_sq = (88.0_f32).powi(2);
    for dy in -radius..radius {
        for dx in -radius..radius {
            let distance_sq = (dx * dx + dy * dy) as f32;
            if distance_sq > radius_sq {
                continue;
            }
            let head_dx = (dx - head_center.0) as f32;
            let head_dy = (dy - head_center.1) as f32;
            let shoulder_dx = (dx - shoulder_center.0) as f32;
            let shoulder_dy = (dy - shoulder_center.1) as f32;
            let in_figure = head_dx * head_dx + head_dy * head_dy <= head_radius_sq
                || shoulder_dx * shoulder_dx + shoulder_dy * shoulder_dy <= shoulder_radius_sq;
            if in_figure {
                image.put_pixel((cx + dx) as u32, (cy + dy) as u32, card::FIGURE);
            }
        }
    }
}
