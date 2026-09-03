//! 群内静态命令卡片：暖炭底、鎏金细线的「洛界典籍」视觉系统。
//!
//! 菜单、体系、技能、装备、物品详情、机缘与世界事件卡片共用同一套
//! 绘图原语：近单色暖炭底、极淡的山水剪影、细金线外框与角饰、
//! 菱形分隔、大字距标题。装饰只保留细线、菱形与角饰三种，不做
//! 色块按钮与发光效果，让文案本身成为画面主角。
//!
//! 角色卡（`profile.rs`）复用这些原语；战斗 GIF（`battle.rs`）保留
//! 独立的动画视觉体系。所有文案由本视图层生成（设计方案书 23.3），
//! 渲染失败时由命令层回退为文字，不影响权威结果。

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::{self, Cursor},
    path::Path,
};

use ab_glyph::{FontArc, PxScale, PxScaleFont, ScaleFont as _};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba, imageops};
use imageproc::{
    drawing::{
        draw_filled_circle_mut, draw_filled_rect_mut, draw_hollow_circle_mut,
        draw_line_segment_mut, draw_polygon_mut, draw_text_mut,
    },
    point::Point,
    rect::Rect,
};

use super::assets;

// ---- 画布 ----

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;
/// 页面外框内缩。
const FRAME_INSET: i32 = 16;
/// 角饰臂长。
const CORNER_ARM: i32 = 24;
/// 内容左右边距。
const LEFT: i32 = 48;
const RIGHT: i32 = 912;
/// 脚注基线。
const FOOTNOTE_Y: i32 = 502;

// ---- 调色板：暖炭与旧金 ----

pub(crate) const BG: Rgba<u8> = Rgba([19, 17, 12, 255]);
const HAZE: Rgba<u8> = Rgba([22, 19, 13, 255]);
const HILLS_FAR: Rgba<u8> = Rgba([27, 23, 16, 255]);
const HILLS_NEAR: Rgba<u8> = Rgba([14, 12, 8, 255]);
const MOON_OUTER: Rgba<u8> = Rgba([28, 24, 15, 255]);
const MOON_INNER: Rgba<u8> = Rgba([34, 29, 18, 255]);
pub(crate) const GOLD: Rgba<u8> = Rgba([182, 154, 98, 255]);
pub(crate) const GOLD_BRIGHT: Rgba<u8> = Rgba([217, 190, 140, 255]);
pub(crate) const GOLD_DIM: Rgba<u8> = Rgba([110, 92, 59, 255]);
pub(crate) const LINE_FAINT: Rgba<u8> = Rgba([52, 44, 28, 255]);
pub(crate) const TEXT_MAIN: Rgba<u8> = Rgba([200, 178, 130, 255]);
pub(crate) const TEXT_SUB: Rgba<u8> = Rgba([150, 131, 94, 255]);
pub(crate) const TEXT_MUTE: Rgba<u8> = Rgba([104, 90, 64, 255]);
const GAIN_TEXT: Rgba<u8> = Rgba([140, 176, 148, 255]);
const TILE_BG: Rgba<u8> = Rgba([25, 21, 15, 255]);
pub(crate) const FIGURE: Rgba<u8> = Rgba([64, 55, 36, 255]);

// ---- 基础原语 ----

pub(crate) fn fill(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: Rgba<u8>,
) {
    if width == 0 || height == 0 {
        return;
    }
    draw_filled_rect_mut(
        image,
        Rect::at(x as i32, y as i32).of_size(width, height),
        color,
    );
}

/// 细横线（单像素），端点任意方向。
fn hline(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, x0: i32, x1: i32, y: i32, color: Rgba<u8>) {
    if x0 == x1 {
        return;
    }
    let (start, end) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
    draw_line_segment_mut(
        image,
        (start as f32, y as f32),
        (end as f32, y as f32),
        color,
    );
}

/// 细竖线（单像素），端点任意方向。
pub(crate) fn vline(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: i32,
    y0: i32,
    y1: i32,
    color: Rgba<u8>,
) {
    if y0 == y1 {
        return;
    }
    let (start, end) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
    draw_line_segment_mut(
        image,
        (x as f32, start as f32),
        (x as f32, end as f32),
        color,
    );
}

/// 实心菱形：本系统唯一的基础装饰点。
pub(crate) fn diamond_filled(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    cx: i32,
    cy: i32,
    radius: i32,
    color: Rgba<u8>,
) {
    if radius <= 0 {
        return;
    }
    let points = [
        Point::new(cx, cy - radius),
        Point::new(cx + radius, cy),
        Point::new(cx, cy + radius),
        Point::new(cx - radius, cy),
    ];
    draw_polygon_mut(image, &points, color);
}

/// 空心菱形：用于「未点亮」的刻度与空槽提示。
fn diamond_hollow(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    cx: i32,
    cy: i32,
    radius: i32,
    color: Rgba<u8>,
) {
    if radius <= 0 {
        return;
    }
    let corners = [
        (cx, cy - radius),
        (cx + radius, cy),
        (cx, cy + radius),
        (cx - radius, cy),
    ];
    for (index, &(x, y)) in corners.iter().enumerate() {
        let (next_x, next_y) = corners[(index + 1) % corners.len()];
        draw_line_segment_mut(
            image,
            (x as f32, y as f32),
            (next_x as f32, next_y as f32),
            color,
        );
    }
}

/// 细分隔线，中点缀一枚菱形。
pub(crate) fn divider(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x0: i32,
    x1: i32,
    y: i32,
    color: Rgba<u8>,
) {
    hline(image, x0, x1, y, LINE_FAINT);
    diamond_filled(image, (x0 + x1) / 2, y, 3, color);
}

/// 细进度轨道：单像素轨道 + 同线填充 + 末端菱形。
fn thin_track(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: i32,
    y: i32,
    width: i32,
    ratio: f64,
    color: Rgba<u8>,
) {
    hline(image, x, x + width, y, LINE_FAINT);
    let filled = (width as f64 * ratio.clamp(0.02, 1.0)).round() as i32;
    if filled > 1 {
        hline(image, x, x + filled, y, color);
        diamond_filled(image, x + filled, y, 4, color);
    }
}

/// 页面外框：细金线矩形 + 四角加粗角饰。
fn frame(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>) {
    let inset = FRAME_INSET;
    let right = WIDTH as i32 - inset;
    let bottom = HEIGHT as i32 - inset;
    hline(image, inset, right, inset, GOLD_DIM);
    hline(image, inset, right, bottom, GOLD_DIM);
    vline(image, inset, inset, bottom, GOLD_DIM);
    vline(image, right, inset, bottom, GOLD_DIM);
    let corners = [
        (inset, inset, 1, 1),
        (right, inset, -1, 1),
        (inset, bottom, 1, -1),
        (right, bottom, -1, -1),
    ];
    for &(x, y, dx, dy) in &corners {
        hline(image, x, x + dx * CORNER_ARM, y, GOLD);
        hline(image, x, x + dx * CORNER_ARM, y + dy, GOLD);
        vline(image, x + dx, y, y + dy * CORNER_ARM, GOLD);
        vline(image, x, y, y + dy * CORNER_ARM, GOLD);
    }
}

/// 程序化山水剪影：淡月、远山、一座五重塔与近山。
///
/// 剪影与底色只差几个灰阶，需在安静的屏幕上才能察觉——
/// 它提供质感，而不是内容。
fn scenery(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, seed: u64) {
    fill(image, 0, 296, WIDTH, HEIGHT - 296, HAZE);
    let moon_x = 756 + (seed % 80) as i32;
    draw_filled_circle_mut(image, (moon_x, 92), 44, MOON_OUTER);
    draw_filled_circle_mut(image, (moon_x, 92), 34, MOON_INNER);

    let phase = |k: u32| ((seed >> (k * 8)) & 0xFF) as f32 * 0.0246;
    for x in 0..WIDTH as i32 {
        let t = x as f32;
        let ridge =
            312.0 + 28.0 * (t * 0.0085 + phase(0)).sin() + 13.0 * (t * 0.021 + phase(1)).sin();
        for y in (ridge as i32).max(0)..HEIGHT as i32 {
            image.put_pixel(x as u32, y as u32, HILLS_FAR);
        }
    }
    pagoda(image, 166, 372);
    for x in 0..WIDTH as i32 {
        let t = x as f32;
        let ridge =
            398.0 + 24.0 * (t * 0.0063 + phase(2)).sin() + 11.0 * (t * 0.017 + phase(3)).sin();
        for y in (ridge as i32).max(0)..HEIGHT as i32 {
            image.put_pixel(x as u32, y as u32, HILLS_NEAR);
        }
    }
}

/// 五重塔剪影：逐层收窄的塔身与出檐，底部没入近山。
fn pagoda(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, cx: i32, base: i32) {
    for level in 0..5_i32 {
        let level_base = base - level * 24;
        let body_width = 44 - level * 5;
        let eave_width = 84 - level * 11;
        fill(
            image,
            (cx - body_width / 2) as u32,
            (level_base - 18) as u32,
            body_width as u32,
            18,
            HILLS_FAR,
        );
        fill(
            image,
            (cx - eave_width / 2) as u32,
            (level_base - 24) as u32,
            eave_width as u32,
            6,
            HILLS_FAR,
        );
    }
    fill(
        image,
        (cx - 1) as u32,
        (base - 134) as u32,
        2,
        14,
        HILLS_FAR,
    );
}

/// 以卡片名派生相位的全新底图：山水 + 外框。
pub(crate) fn blank(seed_key: &str) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let mut image = ImageBuffer::from_pixel(WIDTH, HEIGHT, BG);
    let mut hasher = DefaultHasher::new();
    seed_key.hash(&mut hasher);
    scenery(&mut image, hasher.finish());
    frame(&mut image);
    image
}

/// 编码 PNG 并原子写盘。
pub(crate) fn encode(path: &Path, image: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> io::Result<()> {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut bytes, ImageFormat::Png)
        .map_err(io::Error::other)?;
    assets::atomic_write(path, bytes.get_ref())
}

// ---- 文本 ----

pub(crate) fn label(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    font: &FontArc,
    size: f32,
    x: i32,
    y: i32,
    color: Rgba<u8>,
    text: &str,
) {
    draw_text_mut(image, color, x, y, PxScale::from(size), font, text);
}

fn scaled_font(font: &FontArc, size: f32) -> PxScaleFont<FontArc> {
    PxScaleFont {
        font: font.clone(),
        scale: PxScale::from(size),
    }
}

/// 文本宽度（含额外字距）。
pub(crate) fn text_width(font: &FontArc, size: f32, text: &str, spacing: f32) -> f32 {
    let scaled = scaled_font(font, size);
    let advance: f32 = text
        .chars()
        .map(|ch| scaled.h_advance(scaled.glyph_id(ch)))
        .sum();
    advance + spacing * text.chars().count().saturating_sub(1) as f32
}

/// 带额外字距的逐字绘制（空格只推进笔位）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn label_spaced(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    font: &FontArc,
    size: f32,
    x: i32,
    y: i32,
    color: Rgba<u8>,
    text: &str,
    spacing: f32,
) {
    let scaled = scaled_font(font, size);
    let mut pen = x as f32;
    for ch in text.chars() {
        if ch != ' ' {
            draw_text_mut(
                image,
                color,
                pen.round() as i32,
                y,
                PxScale::from(size),
                font,
                &ch.to_string(),
            );
        }
        pen += scaled.h_advance(scaled.glyph_id(ch)) + spacing;
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn label_centered(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    font: &FontArc,
    size: f32,
    cx: i32,
    y: i32,
    color: Rgba<u8>,
    text: &str,
    spacing: f32,
) {
    let width = text_width(font, size, text, spacing);
    label_spaced(
        image,
        font,
        size,
        (cx as f32 - width / 2.0).round() as i32,
        y,
        color,
        text,
        spacing,
    );
}

#[allow(clippy::too_many_arguments)]
fn label_right(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    font: &FontArc,
    size: f32,
    right: i32,
    y: i32,
    color: Rgba<u8>,
    text: &str,
    spacing: f32,
) {
    let width = text_width(font, size, text, spacing);
    label_spaced(
        image,
        font,
        size,
        (right as f32 - width).round() as i32,
        y,
        color,
        text,
        spacing,
    );
}

/// 页首标题带：居中大字距标题，两侧细线收进边框，线端各缀一枚菱形。
fn title_band(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    font: Option<&FontArc>,
    title: &str,
    cy: i32,
) {
    let Some(font) = font else {
        hline(image, 52, 384, cy - 6, GOLD_DIM);
        hline(image, 576, 908, cy - 6, GOLD_DIM);
        return;
    };
    let center = WIDTH as i32 / 2;
    let width = text_width(font, 28.0, title, 8.0);
    label_spaced(
        image,
        font,
        28.0,
        (center as f32 - width / 2.0).round() as i32,
        cy - 20,
        GOLD_BRIGHT,
        title,
        8.0,
    );
    let gap = (width / 2.0 + 34.0).round() as i32;
    hline(image, 52, center - gap, cy - 6, GOLD_DIM);
    hline(image, center + gap, 908, cy - 6, GOLD_DIM);
    diamond_filled(image, center - gap, cy - 6, 3, GOLD);
    diamond_filled(image, center + gap, cy - 6, 3, GOLD);
}

/// 页脚提示：一行暗金小字。
fn footnote(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, font: Option<&FontArc>, text: &str) {
    if let Some(font) = font {
        label_centered(
            image,
            font,
            14.0,
            WIDTH as i32 / 2,
            FOOTNOTE_Y,
            TEXT_MUTE,
            text,
            3.0,
        );
    }
}

// ---- 线性图标 ----

/// 角色数值列的圆圈简笔图标。
#[derive(Clone, Copy)]
pub(crate) enum StatGlyph {
    /// 打坐人形：修为。
    Meditate,
    /// 交叉双剑：战力。
    Swords,
    /// 铜钱：金币。
    Coin,
}

/// 细线圆圈 + 圈内简笔图形。
pub(crate) fn ring_icon(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    cx: i32,
    cy: i32,
    radius: i32,
    color: Rgba<u8>,
    glyph: StatGlyph,
) {
    draw_hollow_circle_mut(image, (cx, cy), radius, color);
    match glyph {
        StatGlyph::Meditate => {
            draw_filled_circle_mut(image, (cx, cy - 8), 4, color);
            let body = [
                Point::new(cx - 12, cy + 12),
                Point::new(cx + 12, cy + 12),
                Point::new(cx, cy - 3),
            ];
            draw_polygon_mut(image, &body, color);
            hline(image, cx - 17, cx + 17, cy + 15, color);
        }
        StatGlyph::Swords => {
            draw_line_segment_mut(
                image,
                ((cx - 11) as f32, (cy - 11) as f32),
                ((cx + 11) as f32, (cy + 11) as f32),
                color,
            );
            draw_line_segment_mut(
                image,
                ((cx + 11) as f32, (cy - 11) as f32),
                ((cx - 11) as f32, (cy + 11) as f32),
                color,
            );
            draw_line_segment_mut(
                image,
                ((cx - 13) as f32, (cy + 5) as f32),
                ((cx - 5) as f32, (cy + 13) as f32),
                color,
            );
            draw_line_segment_mut(
                image,
                ((cx + 5) as f32, (cy + 13) as f32),
                ((cx + 13) as f32, (cy + 5) as f32),
                color,
            );
        }
        StatGlyph::Coin => {
            draw_hollow_circle_mut(image, (cx, cy), 13, color);
            let half = 5;
            hline(image, cx - half, cx + half, cy - half, color);
            hline(image, cx - half, cx + half, cy + half, color);
            vline(image, cx - half, cy - half, cy + half, color);
            vline(image, cx + half, cy - half, cy + half, color);
        }
    }
}

// ---- 玩法菜单 ----

/// 渲染玩法菜单：四个分区行，分区名与指令以表格细线分隔。
pub fn menu(root: &Path, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let mut image = blank("menu");
    title_band(&mut image, assets.font(), "洛界 · 玩法", 60);

    const SECTION_SPLIT: i32 = 186;
    const ROW_BOUNDS: [i32; 5] = [100, 197, 293, 390, 486];
    let sections: [(&str, &[&str]); 4] = [
        (
            "初入此界",
            &["注册 <名称>", "体系", "选择体系 <体系>", "改名 <名称>"],
        ),
        (
            "日常修行",
            &["签到", "修行行动", "状态", "技能", "战术", "装备"],
        ),
        (
            "行走世间",
            &["今日状态", "每日事件", "世界事件", "排行", "主页"],
        ),
        (
            "争锋机缘",
            &["战力", "决斗 <QQ>", "御空试炼", "兑换 <兑换码>"],
        ),
    ];

    hline(&mut image, LEFT, RIGHT, ROW_BOUNDS[0], GOLD_DIM);
    hline(&mut image, LEFT, RIGHT, ROW_BOUNDS[4], GOLD_DIM);
    vline(&mut image, LEFT, ROW_BOUNDS[0], ROW_BOUNDS[4], GOLD_DIM);
    vline(&mut image, RIGHT, ROW_BOUNDS[0], ROW_BOUNDS[4], GOLD_DIM);
    vline(
        &mut image,
        SECTION_SPLIT,
        ROW_BOUNDS[0],
        ROW_BOUNDS[4],
        GOLD_DIM,
    );
    for &row_y in &ROW_BOUNDS[1..4] {
        hline(&mut image, LEFT, RIGHT, row_y, LINE_FAINT);
        diamond_filled(&mut image, SECTION_SPLIT, row_y, 3, GOLD);
    }

    if let Some(font) = assets.font() {
        for (index, (title, items)) in sections.into_iter().enumerate() {
            let row_top = ROW_BOUNDS[index];
            let row_bottom = ROW_BOUNDS[index + 1];
            label_centered(
                &mut image,
                font,
                21.0,
                (LEFT + SECTION_SPLIT) / 2,
                row_top + 37,
                GOLD_BRIGHT,
                title,
                3.0,
            );
            let columns = items.len();
            let column_width = (RIGHT - SECTION_SPLIT) / columns as i32;
            for (column, item) in items.iter().enumerate() {
                let column_x = SECTION_SPLIT + column as i32 * column_width;
                if column > 0 {
                    vline(
                        &mut image,
                        column_x,
                        row_top + 18,
                        row_bottom - 18,
                        LINE_FAINT,
                    );
                    for &row_y in &ROW_BOUNDS[1..4] {
                        diamond_filled(&mut image, column_x, row_y, 3, GOLD);
                    }
                }
                label_centered(
                    &mut image,
                    font,
                    18.0,
                    column_x + column_width / 2,
                    row_top + 39,
                    TEXT_MAIN,
                    item,
                    2.0,
                );
            }
        }
    }
    footnote(&mut image, assets.font(), "以上指令在群内直接发送即可");
    encode(path, &image)
}

// ---- 修行体系 ----

/// 修行体系卡片数据：由命令层从体系注册表组装。
pub struct SystemCardEntry {
    pub name: String,
    pub id: String,
    pub positioning: String,
}

/// 体系的一句定位文案。
pub fn system_positioning(id: &str) -> &'static str {
    match id {
        "orthodox" => "多工具 · 慢成长 · 高上限",
        "sword" => "爆发追击 · 以攻代守",
        "body" => "贴身压制 · 金刚不坏",
        "mage" => "远程范围 · 元素塑场",
        "soul" => "韧性压制 · 领域控制",
        "qi" => "近中距均衡 · 形态百变",
        "blood_demon" => "以命换势 · 速成爆发",
        "formation" => "布阵控场 · 越阶困敌",
        "alchemy_artifact" => "丹器济世 · 战前筹备",
        "summoner" => "多单位协同 · 契约指挥",
        "music" => "群体增减益 · 曲势连绵",
        _ => "自成一道",
    }
}

/// 渲染修行体系总览：双列六行的细线表格，体系色只落在名称前的菱形上。
pub fn systems(root: &Path, entries: &[SystemCardEntry], path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let mut image = blank("systems");
    title_band(&mut image, assets.font(), "修行体系", 60);

    const MIDDLE: i32 = 480;
    const ROW_BOUNDS: [i32; 7] = [100, 164, 228, 292, 356, 420, 486];
    hline(&mut image, LEFT, RIGHT, ROW_BOUNDS[0], GOLD_DIM);
    hline(&mut image, LEFT, RIGHT, ROW_BOUNDS[6], GOLD_DIM);
    vline(&mut image, LEFT, ROW_BOUNDS[0], ROW_BOUNDS[6], GOLD_DIM);
    vline(&mut image, RIGHT, ROW_BOUNDS[0], ROW_BOUNDS[6], GOLD_DIM);
    vline(&mut image, MIDDLE, ROW_BOUNDS[0], ROW_BOUNDS[6], GOLD_DIM);
    for &row_y in &ROW_BOUNDS[1..6] {
        hline(&mut image, LEFT, MIDDLE, row_y, LINE_FAINT);
        hline(&mut image, MIDDLE, RIGHT, row_y, LINE_FAINT);
    }

    if let Some(font) = assets.font() {
        for (index, entry) in entries.iter().enumerate().take(12) {
            let column = index % 2;
            let row = index / 2;
            let x0 = if column == 0 { LEFT } else { MIDDLE };
            let row_top = ROW_BOUNDS[row];
            diamond_filled(
                &mut image,
                x0 + 34,
                row_top + 24,
                4,
                system_color(&entry.id),
            );
            label(
                &mut image,
                font,
                20.0,
                x0 + 56,
                row_top + 13,
                GOLD_BRIGHT,
                &entry.name,
            );
            label(
                &mut image,
                font,
                13.0,
                x0 + 56,
                row_top + 40,
                TEXT_SUB,
                &format!("{} · {}", entry.id, entry.positioning),
            );
        }
    }
    footnote(
        &mut image,
        assets.font(),
        "选择体系 <名称或标识>，体系一经确定不可更改",
    );
    encode(path, &image)
}

// ---- 技能 ----

/// 技能卡片数据。
pub struct SkillCardData<'a> {
    pub display_name: &'a str,
    pub system_name: &'a str,
    pub system_id: &'a str,
    pub tactic_name: &'a str,
    pub skills: &'a [(String, u8)],
}

/// 渲染技能卡片：技能名 + 菱形熟练度刻度。
pub fn skills(root: &Path, data: &SkillCardData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let accent = system_color(data.system_id);
    let mut image = blank("skills");
    title_band(&mut image, assets.font(), "技艺", 60);

    if let Some(font) = assets.font() {
        label(
            &mut image,
            font,
            26.0,
            64,
            104,
            GOLD_BRIGHT,
            data.display_name,
        );
        label_right(
            &mut image,
            font,
            16.0,
            896,
            112,
            accent,
            &format!("{} · 战术 {}", data.system_name, data.tactic_name),
            2.0,
        );
        divider(&mut image, 64, 896, 148, GOLD);
        for (index, (name, mastery)) in data.skills.iter().enumerate().take(8) {
            let row_top = 166 + index as i32 * 40;
            label(&mut image, font, 19.0, 64, row_top + 9, TEXT_MAIN, name);
            for pip in 0..3_u8 {
                let pip_x = 690 + pip as i32 * 38;
                if *mastery > pip {
                    diamond_filled(&mut image, pip_x, row_top + 18, 7, accent);
                } else {
                    diamond_hollow(&mut image, pip_x, row_top + 18, 7, LINE_FAINT);
                }
            }
            label(
                &mut image,
                font,
                14.0,
                800,
                row_top + 12,
                TEXT_MUTE,
                &format!("{mastery}/3"),
            );
            if index < 7 {
                hline(&mut image, 64, 896, row_top + 40, LINE_FAINT);
            }
        }
        if data.skills.len() > 8 {
            footnote(
                &mut image,
                Some(font),
                &format!("其余 {} 项技能已略，详见网页档案", data.skills.len() - 8),
            );
        }
    }
    encode(path, &image)
}

// ---- 装备 ----

/// 装备卡片数据。
pub struct EquipmentCardData<'a> {
    pub display_name: &'a str,
    pub system_name: &'a str,
    pub system_id: &'a str,
    /// 已装备的槽位。
    pub equipped: &'a [EquippedSlotView],
    /// 未装备物品。
    pub bag: &'a [BagItemView],
}

#[derive(Clone, Debug)]
pub struct EquippedSlotView {
    pub slot_code: String,
    pub item_name: String,
    pub quality: String,
}

#[derive(Clone, Debug)]
pub struct BagItemView {
    pub name: String,
    pub quality: String,
    pub quantity: i64,
}

const SLOT_ORDER: [(&str, &str); 8] = [
    ("main_hand", "主手"),
    ("off_hand", "副手"),
    ("head", "头部"),
    ("body", "身体"),
    ("hands", "手部"),
    ("feet", "足部"),
    ("accessory_1", "饰品一"),
    ("accessory_2", "饰品二"),
];

/// 物品格细框。
fn tile_frame(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: i32,
    y: i32,
    size: i32,
    color: Rgba<u8>,
) {
    hline(image, x, x + size, y, color);
    hline(image, x, x + size, y + size, color);
    vline(image, x, y, y + size, color);
    vline(image, x + size, y, y + size, color);
}

/// 深底面板上的物品格：细金框 + 底部稀有度色条 + 居中图标（缺失时画首字）。
#[allow(clippy::too_many_arguments)]
fn item_tile(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    font: &FontArc,
    x: i32,
    y: i32,
    size: i32,
    ring: Rgba<u8>,
    icon: Option<DynamicImage>,
    glyph: &str,
) {
    fill(image, x as u32, y as u32, size as u32, size as u32, TILE_BG);
    tile_frame(image, x, y, size, GOLD_DIM);
    fill(
        image,
        (x + 4) as u32,
        (y + size - 8) as u32,
        (size - 8) as u32,
        3,
        ring,
    );
    let icon_size = (size as u32 * 3 / 4).max(8);
    match icon.map(|icon| icon.resize(icon_size, icon_size, imageops::FilterType::Lanczos3)) {
        Some(resized) => {
            let offset = (size - resized.width() as i32) / 2;
            imageops::overlay(
                image,
                &resized,
                (x + offset) as i64,
                (y + offset - 2) as i64,
            );
        }
        None => {
            let offset = (glyph.chars().count() as i32) * size / 4;
            label(
                image,
                font,
                size as f32 * 0.5,
                x + size / 2 - offset,
                y + size / 3,
                TEXT_SUB,
                glyph,
            );
        }
    }
}

/// 渲染装备卡片：八个槽位徽章与背包，稀有度只落在底条上。
pub fn equipment(root: &Path, data: &EquipmentCardData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let accent = system_color(data.system_id);
    let mut image = blank("equipment");
    title_band(&mut image, assets.font(), "行装", 60);

    if let Some(font) = assets.font() {
        label(
            &mut image,
            font,
            26.0,
            64,
            104,
            GOLD_BRIGHT,
            data.display_name,
        );
        label_right(
            &mut image,
            font,
            16.0,
            896,
            112,
            accent,
            &format!("{} · 装备栏", data.system_name),
            2.0,
        );
        divider(&mut image, 64, 896, 148, GOLD);

        for (index, (slot_code, slot_name)) in SLOT_ORDER.into_iter().enumerate() {
            let x = 64 + index as i32 * 100;
            let slot = data
                .equipped
                .iter()
                .find(|slot| slot.slot_code == slot_code);
            match slot {
                Some(slot) => {
                    let (ring, _, _) = tier_of(root, &slot.quality);
                    item_tile(
                        &mut image,
                        font,
                        x,
                        166,
                        78,
                        ring,
                        assets.equipment_icon(&slot.item_name),
                        &slot.item_name.chars().next().unwrap_or('器').to_string(),
                    );
                    label(
                        &mut image,
                        font,
                        12.0,
                        x,
                        252,
                        TEXT_MAIN,
                        &truncate_name(&slot.item_name, 7),
                    );
                }
                None => {
                    tile_frame(&mut image, x, 166, 78, LINE_FAINT);
                    diamond_hollow(&mut image, x + 39, 205, 10, LINE_FAINT);
                }
            }
            label(
                &mut image,
                font,
                12.0,
                x,
                slot_name_y(slot.is_some()),
                TEXT_MUTE,
                slot_name,
            );
        }

        divider(&mut image, 64, 896, 306, GOLD);
        label(&mut image, font, 17.0, 64, 322, GOLD_BRIGHT, "背包");
        if data.bag.is_empty() {
            label(&mut image, font, 14.0, 150, 330, TEXT_MUTE, "暂无其他物品");
        }
        for (index, item) in data.bag.iter().enumerate().take(7) {
            let x = 150 + index as i32 * 92;
            let (ring, _, _) = tier_of(root, &item.quality);
            item_tile(
                &mut image,
                font,
                x,
                354,
                56,
                ring,
                assets.equipment_icon(&item.name),
                &item.name.chars().next().unwrap_or('物').to_string(),
            );
            label(
                &mut image,
                font,
                12.0,
                x,
                418,
                TEXT_SUB,
                &format!("×{}", item.quantity),
            );
        }
        if data.bag.len() > 7 {
            label(
                &mut image,
                font,
                14.0,
                150 + 7 * 92,
                376,
                TEXT_SUB,
                &format!("+{}", data.bag.len() - 7),
            );
        }
    }
    footnote(&mut image, assets.font(), "装备 查看 <编号> 查看物品详情");
    encode(path, &image)
}

/// 槽位名的纵坐标：已装备格下方让位给物品名，空格则紧贴格底。
fn slot_name_y(occupied: bool) -> i32 {
    if occupied { 270 } else { 252 }
}

fn truncate_name(name: &str, limit: usize) -> String {
    if name.chars().count() <= limit {
        name.to_owned()
    } else {
        let mut truncated = name.chars().take(limit).collect::<String>();
        truncated.push('…');
        truncated
    }
}

// ---- 物品详情 ----

/// 物品详情卡片数据（`装备 查看 <编号>`）。
pub struct ItemDetailData<'a> {
    pub item_id: i64,
    pub definition_id: &'a str,
    pub quality: &'a str,
    pub level: u32,
    pub quantity: i64,
    pub equipped_slot: Option<&'a str>,
    pub modifiers: &'a [(String, i64)],
}

fn slot_display_name(slot_code: &str) -> &str {
    SLOT_ORDER
        .into_iter()
        .find(|(code, _)| *code == slot_code)
        .map(|(_, name)| name)
        .unwrap_or(slot_code)
}

/// 渲染物品详情：左侧器物徽记，右侧词条清单。
pub fn item_detail(root: &Path, data: &ItemDetailData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let (ring, stars, rarity_name) = tier_of(root, data.quality);
    let mut image = blank("item_detail");
    title_band(&mut image, assets.font(), "器物", 60);

    if let Some(font) = assets.font() {
        item_tile(
            &mut image,
            font,
            122,
            108,
            148,
            ring,
            assets.equipment_icon(data.definition_id),
            &data
                .definition_id
                .chars()
                .next()
                .unwrap_or('器')
                .to_string(),
        );
        label_centered(
            &mut image,
            font,
            21.0,
            196,
            278,
            GOLD_BRIGHT,
            &truncate_name(data.definition_id, 9),
            2.0,
        );
        let star_start = 196 - (stars.max(1) as i32 - 1) * 11;
        for star in 0..stars.max(1) {
            diamond_filled(&mut image, star_start + star as i32 * 22, 318, 5, ring);
        }
        label_centered(&mut image, font, 14.0, 196, 336, ring, &rarity_name, 2.0);
        let state = match data.equipped_slot {
            Some(slot) => format!("已装备 · {}", slot_display_name(slot)),
            None => "未装备".to_owned(),
        };
        label_centered(&mut image, font, 14.0, 196, 372, TEXT_SUB, &state, 2.0);
        label_centered(
            &mut image,
            font,
            13.0,
            196,
            396,
            TEXT_MUTE,
            &format!("编号 #{}", data.item_id),
            2.0,
        );

        vline(&mut image, 368, 108, 430, LINE_FAINT);
        label(
            &mut image,
            font,
            16.0,
            400,
            112,
            TEXT_SUB,
            &format!("强化 +{} · 持有 {}", data.level, data.quantity),
        );
        divider(&mut image, 400, 896, 148, GOLD);
        if data.modifiers.is_empty() {
            label(
                &mut image,
                font,
                16.0,
                400,
                190,
                TEXT_MUTE,
                "暂无词条加成。",
            );
        }
        for (index, (code, value)) in data.modifiers.iter().enumerate().take(7) {
            let y = 176 + index as i32 * 42;
            label(
                &mut image,
                font,
                18.0,
                400,
                y,
                TEXT_MAIN,
                modifier_name(code),
            );
            let display = if *value >= 0 {
                format!("+{value}")
            } else {
                value.to_string()
            };
            label_right(&mut image, font, 18.0, 896, y, GAIN_TEXT, &display, 1.0);
            if index < data.modifiers.len().min(7) - 1 {
                hline(&mut image, 400, 896, y + 32, LINE_FAINT);
            }
        }
    }
    footnote(
        &mut image,
        assets.font(),
        "装备 穿戴 <编号> <槽位> · 装备 卸下 <槽位>",
    );
    encode(path, &image)
}

// ---- 机缘 ----

/// 机缘卡片数据。
pub struct DestinyCardData<'a> {
    pub destiny_name: &'a str,
    pub description: &'a str,
    pub world_event_line: Option<&'a str>,
}

/// 渲染每日机缘：中央大字机缘名 + 一线一菱 + 描述。
pub fn destiny(root: &Path, data: &DestinyCardData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let mut image = blank("destiny");
    title_band(&mut image, assets.font(), "今日机缘", 64);

    if let Some(font) = assets.font() {
        label_centered(
            &mut image,
            font,
            38.0,
            WIDTH as i32 / 2,
            180,
            GOLD_BRIGHT,
            data.destiny_name,
            10.0,
        );
        divider(&mut image, 330, 630, 256, GOLD);
        label_centered(
            &mut image,
            font,
            19.0,
            WIDTH as i32 / 2,
            290,
            TEXT_MAIN,
            data.description,
            3.0,
        );
        if let Some(line) = data.world_event_line {
            label_centered(
                &mut image,
                font,
                14.0,
                WIDTH as i32 / 2,
                448,
                TEXT_MUTE,
                line,
                2.0,
            );
        }
    }
    encode(path, &image)
}

// ---- 世界事件 ----

/// 世界事件卡片数据。
pub struct WorldEventCardData<'a> {
    pub event_name: &'a str,
    pub description: &'a str,
    pub status: &'a str,
    pub completed: bool,
    pub coin_reward: i64,
    pub mark_reward: i64,
    pub objectives: &'a [(String, i64, i64)],
}

/// 渲染世界事件：事件名 + 状态徽记 + 目标进度轨道。
pub fn world_event(root: &Path, data: &WorldEventCardData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let accent = if data.completed { GOLD_BRIGHT } else { GOLD };
    let mut image = blank("world_event");
    title_band(&mut image, assets.font(), "世界事件", 60);

    if let Some(font) = assets.font() {
        label(
            &mut image,
            font,
            25.0,
            64,
            106,
            GOLD_BRIGHT,
            data.event_name,
        );
        let badge_width = text_width(font, 14.0, data.status, 2.0) as i32 + 36;
        let badge_x1 = 896;
        let badge_x0 = badge_x1 - badge_width;
        let badge_line = if data.completed { GOLD } else { GOLD_DIM };
        hline(&mut image, badge_x0, badge_x1, 104, badge_line);
        hline(&mut image, badge_x0, badge_x1, 136, badge_line);
        vline(&mut image, badge_x0, 104, 136, badge_line);
        vline(&mut image, badge_x1, 104, 136, badge_line);
        label_centered(
            &mut image,
            font,
            14.0,
            (badge_x0 + badge_x1) / 2,
            112,
            accent,
            data.status,
            2.0,
        );
        label(&mut image, font, 16.0, 64, 156, TEXT_SUB, data.description);
        divider(&mut image, 64, 896, 200, GOLD);
        for (index, (objective, current, target)) in data.objectives.iter().enumerate().take(3) {
            let y = 222 + index as i32 * 82;
            label(&mut image, font, 18.0, 64, y, TEXT_MAIN, objective);
            let ratio = if *target > 0 {
                *current as f64 / *target as f64
            } else {
                0.0
            };
            thin_track(&mut image, 64, y + 36, 756, ratio, accent);
            label_right(
                &mut image,
                font,
                14.0,
                896,
                y + 2,
                TEXT_MUTE,
                &format!("{current}/{target}"),
                1.0,
            );
        }
        footnote(
            &mut image,
            Some(font),
            &format!(
                "完成奖励：金币 {} · 刻印 {}（签到、机缘与决斗自动推进）",
                data.coin_reward, data.mark_reward
            ),
        );
    }
    encode(path, &image)
}

// ---- 共享辅助 ----

/// 品阶解析：从规则注册表读取色、星数与显示名。
///
/// 品阶表来自 `data/luo_realm/rules/rarities.toml`（可整体覆盖）或内置
/// 默认；本模块不关心品阶会套在什么物品上，只按品质代码取外观。
fn tier_of(root: &Path, quality: &str) -> (Rgba<u8>, usize, String) {
    let tiers = crate::domain::rules::rarity_tiers(root);
    match crate::domain::rules::rarity_by_code(&tiers, quality) {
        Some(tier) => {
            let (red, green, blue) = tier.rgb();
            (
                Rgba([red, green, blue, 255]),
                tier.stars as usize,
                tier.display.clone(),
            )
        }
        None => (Rgba([112, 118, 124, 255]), 1, "普通".into()),
    }
}

/// 词条代码的中文显示名。
pub(crate) fn modifier_name(code: &str) -> &str {
    match code {
        "max_health" => "生命",
        "attack" => "攻击",
        "physical_defense" => "物防",
        "arcane_defense" => "法防",
        "soul_defense" => "魂防",
        "speed" => "速度",
        "critical_rate" => "暴击",
        _ => code,
    }
}

/// 大数字的紧凑显示：过万折算为「万」。
pub(crate) fn format_number(value: f64) -> String {
    if value.abs() >= 10_000.0 {
        format!("{:.2}万", value / 10_000.0)
    } else {
        format!("{}", value.round() as i64)
    }
}

pub(crate) fn system_color(system_id: &str) -> Rgba<u8> {
    match system_id {
        "sword" => Rgba([74, 128, 176, 255]),
        "body" => Rgba([176, 92, 78, 255]),
        "mage" => Rgba([104, 112, 176, 255]),
        "soul" => Rgba([128, 94, 156, 255]),
        "qi" => Rgba([72, 146, 132, 255]),
        "blood_demon" => Rgba([160, 72, 78, 255]),
        "formation" => Rgba([86, 132, 100, 255]),
        "alchemy_artifact" => Rgba([168, 130, 70, 255]),
        "summoner" => Rgba([108, 134, 94, 255]),
        "music" => Rgba([162, 100, 128, 255]),
        _ => Rgba([98, 128, 106, 255]),
    }
}

#[cfg(test)]
mod tests;
