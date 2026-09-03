//! 群内图片卡片：共享绘图原语与菜单、体系、技能、装备、机缘、世界事件卡片。
//!
//! 与角色卡（`profile.rs`）共用同一套视觉语言：960×540 画布、深色题头、
//! 纸色底、体系强调色。所有文案由本视图层生成（设计方案书 23.3），渲染
//! 失败时由命令层回退为文字，不影响权威结果。

use std::{io, io::Cursor, path::Path};

use ab_glyph::PxScale;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use imageproc::{drawing::draw_text_mut, rect::Rect};

use super::assets;

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;
const HEADER_HEIGHT: u32 = 58;
const PAPER: Rgba<u8> = Rgba([244, 242, 236, 255]);
const HEADER: Rgba<u8> = Rgba([36, 45, 40, 255]);
const BODY_TEXT: Rgba<u8> = Rgba([45, 48, 45, 255]);
const MUTED_TEXT: Rgba<u8> = Rgba([83, 82, 76, 255]);
const DIVIDER: Rgba<u8> = Rgba([219, 216, 207, 255]);
const CARD_BG: Rgba<u8> = Rgba([225, 224, 216, 255]);
const ACCENT: Rgba<u8> = Rgba([198, 151, 42, 255]);
const LIGHT_TEXT: Rgba<u8> = Rgba([247, 246, 241, 255]);

/// 渲染玩法菜单卡片。
pub fn menu(root: &Path, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let mut image = blank();
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
    let positions: [(i32, i32); 4] = [(36, 92), (492, 92), (36, 306), (492, 306)];
    for ((title, items), (x, y)) in sections.into_iter().zip(positions) {
        fill(&mut image, x as u32, y as u32, 432, 198, CARD_BG);
        fill(&mut image, x as u32, y as u32, 6, 198, ACCENT);
        if let Some(font) = assets.font() {
            label(
                &mut image,
                font,
                24.0,
                x + 26,
                y + 18,
                HEADER,
                &format!("【{title}】"),
            );
            for (index, item) in items.iter().enumerate() {
                let column = (index % 2) as i32;
                let row = (index / 2) as i32;
                label(
                    &mut image,
                    font,
                    19.0,
                    x + 26 + column * 204,
                    y + 74 + row * 42,
                    BODY_TEXT,
                    item,
                );
            }
        }
    }
    finish(&assets, &mut image, "LUO REALM / 玩法菜单", None, path)
}

/// 修行体系卡片数据：由命令层从体系注册表组装。
pub struct SystemCardEntry {
    pub name: String,
    pub id: String,
    pub positioning: String,
}

/// 渲染修行体系总览卡片。
pub fn systems(root: &Path, entries: &[SystemCardEntry], path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let mut image = blank();
    for (index, entry) in entries.iter().enumerate() {
        let column = (index % 2) as u32;
        let row = (index / 2) as u32;
        let x = 36 + column * 456;
        let y = 90 + row * 74;
        fill(&mut image, x, y, 432, 62, CARD_BG);
        fill(&mut image, x, y, 6, 62, system_color(&entry.id));
        if let Some(font) = assets.font() {
            label(
                &mut image,
                font,
                23.0,
                x as i32 + 22,
                y as i32 + 6,
                HEADER,
                &entry.name,
            );
            label(
                &mut image,
                font,
                15.0,
                x as i32 + 22,
                y as i32 + 36,
                MUTED_TEXT,
                &format!("{} · {}", entry.id, entry.positioning),
            );
        }
    }
    finish(
        &assets,
        &mut image,
        "LUO REALM / 修行体系",
        Some("选择体系 <名称或标识>，体系一经确定不可更改"),
        path,
    )
}

/// 技能卡片数据。
pub struct SkillCardData<'a> {
    pub display_name: &'a str,
    pub system_name: &'a str,
    pub system_id: &'a str,
    pub tactic_name: &'a str,
    pub skills: &'a [(String, u8)],
}

/// 渲染技能卡片：名称行 + 熟练度刻度。
pub fn skills(root: &Path, data: &SkillCardData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let accent = system_color(data.system_id);
    let mut image = blank();
    fill(
        &mut image,
        0,
        HEADER_HEIGHT,
        16,
        HEIGHT - HEADER_HEIGHT,
        accent,
    );
    fill(&mut image, 36, 92, WIDTH - 72, HEIGHT - 128, CARD_BG);
    if let Some(font) = assets.font() {
        label(&mut image, font, 30.0, 64, 112, HEADER, data.display_name);
        label(
            &mut image,
            font,
            20.0,
            64,
            154,
            accent,
            &format!("{} · 当前战术：{}", data.system_name, data.tactic_name),
        );
        for (index, (name, mastery)) in data.skills.iter().enumerate().take(8) {
            let y = 204 + index as i32 * 36;
            label(&mut image, font, 20.0, 64, y, BODY_TEXT, name);
            for pip in 0..3_u8 {
                let pip_color = if *mastery > pip { accent } else { DIVIDER };
                fill(
                    &mut image,
                    700 + pip as u32 * 26,
                    y as u32 + 8,
                    16,
                    16,
                    pip_color,
                );
            }
            label(
                &mut image,
                font,
                15.0,
                794,
                y + 1,
                MUTED_TEXT,
                &format!("{mastery}/3"),
            );
        }
        if data.skills.len() > 8 {
            label(
                &mut image,
                font,
                15.0,
                64,
                HEIGHT as i32 - 66,
                MUTED_TEXT,
                &format!("其余 {} 项技能已略，详情见网页档案", data.skills.len() - 8),
            );
        }
    }
    finish(&assets, &mut image, "LUO REALM / 技能", None, path)
}

/// 装备卡片数据。
pub struct EquipmentCardData<'a> {
    pub display_name: &'a str,
    pub system_name: &'a str,
    pub system_id: &'a str,
    /// 已装备的 `(槽位代码, 物品名)`。
    pub equipped: &'a [(String, String)],
    /// 未装备物品 `(物品名, 数量)`。
    pub bag: &'a [(String, i64)],
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

/// 渲染装备卡片：八个槽位 + 背包摘要。
pub fn equipment(root: &Path, data: &EquipmentCardData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let accent = system_color(data.system_id);
    let mut image = blank();
    fill(
        &mut image,
        0,
        HEADER_HEIGHT,
        16,
        HEIGHT - HEADER_HEIGHT,
        accent,
    );
    fill(&mut image, 36, 92, WIDTH - 72, 252, CARD_BG);
    fill(&mut image, 36, 360, WIDTH - 72, 144, CARD_BG);
    if let Some(font) = assets.font() {
        label(&mut image, font, 30.0, 64, 112, HEADER, data.display_name);
        label(
            &mut image,
            font,
            20.0,
            64,
            154,
            accent,
            &format!("{} · 装备栏", data.system_name),
        );
        for (index, (slot_code, slot_name)) in SLOT_ORDER.into_iter().enumerate() {
            let column = (index % 2) as i32;
            let row = (index / 2) as i32;
            let x = 64 + column * 432;
            let y = 202 + row * 34;
            let equipped = data
                .equipped
                .iter()
                .find(|(code, _)| code == slot_code)
                .map(|(_, item)| item.as_str())
                .unwrap_or("空");
            label(&mut image, font, 17.0, x, y, MUTED_TEXT, slot_name);
            label(&mut image, font, 17.0, x + 92, y, BODY_TEXT, equipped);
        }
        label(&mut image, font, 20.0, 64, 378, HEADER, "背包");
        if data.bag.is_empty() {
            label(&mut image, font, 17.0, 64, 420, MUTED_TEXT, "暂无其他物品");
        } else {
            let summary = data
                .bag
                .iter()
                .take(3)
                .map(|(name, quantity)| format!("{name} ×{quantity}"))
                .collect::<Vec<_>>()
                .join("    ");
            label(&mut image, font, 17.0, 64, 420, BODY_TEXT, &summary);
            if data.bag.len() > 3 {
                label(
                    &mut image,
                    font,
                    15.0,
                    64,
                    456,
                    MUTED_TEXT,
                    &format!("其余 {} 种物品已略，详情见网页档案", data.bag.len() - 3),
                );
            }
        }
    }
    finish(&assets, &mut image, "LUO REALM / 装备", None, path)
}

/// 机缘卡片数据。
pub struct DestinyCardData<'a> {
    pub destiny_name: &'a str,
    pub description: &'a str,
    pub world_event_line: Option<&'a str>,
}

/// 渲染每日机缘卡片。
pub fn destiny(root: &Path, data: &DestinyCardData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let mut image = blank();
    fill(
        &mut image,
        0,
        HEADER_HEIGHT,
        16,
        HEIGHT - HEADER_HEIGHT,
        ACCENT,
    );
    fill(&mut image, 36, 120, WIDTH - 72, 300, CARD_BG);
    if let Some(font) = assets.font() {
        let offset = (data.destiny_name.chars().count() as i32) * 19;
        label(
            &mut image,
            font,
            38.0,
            WIDTH as i32 / 2 - offset,
            190,
            HEADER,
            data.destiny_name,
        );
        fill(&mut image, WIDTH / 2 - 90, 256, 180, 3, ACCENT);
        let text_offset = (data.description.chars().count() as i32) * 11;
        label(
            &mut image,
            font,
            22.0,
            WIDTH as i32 / 2 - text_offset,
            296,
            BODY_TEXT,
            data.description,
        );
        if let Some(line) = data.world_event_line {
            label(
                &mut image,
                font,
                17.0,
                64,
                HEIGHT as i32 - 96,
                MUTED_TEXT,
                line,
            );
        }
    }
    finish(&assets, &mut image, "LUO REALM / 今日机缘", None, path)
}

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

/// 渲染世界事件卡片：事件说明 + 目标进度条。
pub fn world_event(root: &Path, data: &WorldEventCardData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let status_color = if data.completed {
        Rgba([36, 139, 91, 255])
    } else {
        ACCENT
    };
    let mut image = blank();
    fill(
        &mut image,
        0,
        HEADER_HEIGHT,
        16,
        HEIGHT - HEADER_HEIGHT,
        status_color,
    );
    fill(&mut image, 36, 92, WIDTH - 72, HEIGHT - 128, CARD_BG);
    if let Some(font) = assets.font() {
        label(&mut image, font, 30.0, 64, 112, HEADER, data.event_name);
        fill(&mut image, 700, 120, 190, 34, status_color);
        label(&mut image, font, 18.0, 742, 126, LIGHT_TEXT, data.status);
        label(
            &mut image,
            font,
            19.0,
            64,
            166,
            MUTED_TEXT,
            data.description,
        );
        for (index, (objective, current, target)) in data.objectives.iter().enumerate().take(3) {
            let y = 222 + index as i32 * 76;
            label(&mut image, font, 19.0, 64, y, BODY_TEXT, objective);
            fill(&mut image, 64, y as u32 + 34, 832, 12, DIVIDER);
            let ratio = if *target > 0 {
                (*current as f64 / *target as f64).clamp(0.0, 1.0)
            } else {
                0.0
            };
            fill(
                &mut image,
                64,
                y as u32 + 34,
                (832.0 * ratio) as u32,
                12,
                status_color,
            );
            label(
                &mut image,
                font,
                16.0,
                830,
                y,
                MUTED_TEXT,
                &format!("{current}/{target}"),
            );
        }
        label(
            &mut image,
            font,
            17.0,
            64,
            HEIGHT as i32 - 66,
            MUTED_TEXT,
            &format!(
                "完成奖励：金币 {} · 刻印 {}（签到、机缘与决斗自动推进）",
                data.coin_reward, data.mark_reward
            ),
        );
    }
    finish(&assets, &mut image, "LUO REALM / 世界事件", None, path)
}

fn blank() -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    ImageBuffer::from_pixel(WIDTH, HEIGHT, PAPER)
}

pub(crate) fn finish(
    assets: &assets::RealmAssets,
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    title: &str,
    footnote: Option<&str>,
    path: &Path,
) -> io::Result<()> {
    if let Some(font) = assets.font() {
        label(image, font, 30.0, 34, 10, LIGHT_TEXT, title);
        if let Some(footnote) = footnote {
            label(
                image,
                font,
                16.0,
                36,
                HEIGHT as i32 - 34,
                MUTED_TEXT,
                footnote,
            );
        }
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut bytes, ImageFormat::Png)
        .map_err(io::Error::other)?;
    assets::atomic_write(path, bytes.get_ref())
}

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
    imageproc::drawing::draw_filled_rect_mut(
        image,
        Rect::at(x as i32, y as i32).of_size(width, height),
        color,
    );
}

pub(crate) fn stat_bar(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: u32,
    y: u32,
    ratio: f64,
    color: Rgba<u8>,
) {
    fill(image, x, y, 468, 14, DIVIDER);
    fill(
        image,
        x,
        y,
        (468.0 * ratio.clamp(0.02, 1.0)) as u32,
        14,
        color,
    );
}

pub(crate) fn label(
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    font: &ab_glyph::FontArc,
    size: f32,
    x: i32,
    y: i32,
    color: Rgba<u8>,
    text: &str,
) {
    draw_text_mut(image, color, x, y, PxScale::from(size), font, text);
}

pub(crate) fn system_color(system_id: &str) -> Rgba<u8> {
    match system_id {
        "sword" => Rgba([43, 105, 158, 255]),
        "body" => Rgba([169, 64, 47, 255]),
        "mage" => Rgba([77, 86, 154, 255]),
        "soul" => Rgba([103, 69, 132, 255]),
        "qi" => Rgba([35, 129, 114, 255]),
        "blood_demon" => Rgba([137, 35, 42, 255]),
        "formation" => Rgba([51, 111, 72, 255]),
        "alchemy_artifact" => Rgba([151, 108, 27, 255]),
        "summoner" => Rgba([78, 112, 67, 255]),
        "music" => Rgba([152, 70, 106, 255]),
        _ => Rgba([65, 104, 75, 255]),
    }
}

#[cfg(test)]
mod tests;
