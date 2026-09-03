//! 群内图片卡片：共享绘图原语与菜单、体系、技能、装备、机缘、世界事件卡片。
//!
//! 与角色卡（`profile.rs`）共用同一套视觉语言：960×540 画布、深色题头、
//! 纸色底、体系强调色。所有文案由本视图层生成（设计方案书 23.3），渲染
//! 失败时由命令层回退为文字，不影响权威结果。

use std::{io, io::Cursor, path::Path};

use ab_glyph::PxScale;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba, imageops};
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

/// 渲染修行体系总览卡片。
pub fn systems(root: &Path, entries: &[SystemCardEntry], path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let mut image = blank();
    for (index, entry) in entries.iter().enumerate() {
        let column = (index % 2) as u32;
        let row = (index / 2) as u32;
        let x = 36 + column * 456;
        let y = 76 + row * 72;
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

const PANEL_DARK: Rgba<u8> = Rgba([26, 31, 38, 255]);
const TILE_DARK: Rgba<u8> = Rgba([16, 20, 26, 255]);
const TILE_EMPTY: Rgba<u8> = Rgba([38, 45, 54, 255]);

/// 品质对应的稀有度色环与星数。
pub(crate) fn rarity_tier(quality: &str) -> (Rgba<u8>, usize) {
    match quality {
        "legendary" => (Rgba([214, 158, 62, 255]), 5),
        "epic" => (Rgba([142, 92, 190, 255]), 4),
        "rare" => (Rgba([64, 118, 176, 255]), 3),
        "fine" => (Rgba([70, 138, 96, 255]), 2),
        _ => (Rgba([112, 118, 124, 255]), 1),
    }
}

/// 品质的中文显示名。
pub(crate) fn rarity_display(quality: &str) -> &'static str {
    match quality {
        "legendary" => "传奇",
        "epic" => "史诗",
        "rare" => "珍贵",
        "fine" => "精良",
        "common" => "普通",
        "legacy" => "遗留",
        _ => "普通",
    }
}

/// 深色面板上的次级文字色。
const PANEL_TEXT: Rgba<u8> = Rgba([196, 203, 212, 255]);

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

/// 画一个物品格：深色底 + 稀有度色环 + 居中图标（缺失时画首字）。
#[allow(clippy::too_many_arguments)]
fn item_tile(
    assets: &assets::RealmAssets,
    image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: u32,
    y: u32,
    size: u32,
    ring: Rgba<u8>,
    icon: Option<DynamicImage>,
    glyph: &str,
) {
    fill(image, x, y, size, size, TILE_DARK);
    let border = 3;
    fill(image, x, y, size, border, ring);
    fill(image, x, y + size - border, size, border, ring);
    fill(image, x, y, border, size, ring);
    fill(image, x + size - border, y, border, size, ring);
    let inner = size - 14;
    if let Some(resized) =
        icon.map(|icon| icon.resize_exact(inner, inner, image::imageops::FilterType::Lanczos3))
    {
        imageops::overlay(image, &resized, (x + 7) as i64, (y + 7) as i64);
    } else if let Some(font) = assets.font() {
        let offset = (glyph.chars().count() as i32) * (size as i32) / 4;
        label(
            image,
            font,
            size as f32 * 0.52,
            x as i32 + size as i32 / 2 - offset,
            y as i32 + size as i32 / 3,
            MUTED_TEXT,
            glyph,
        );
    }
}

/// 渲染装备卡片：八个槽位徽章 + 背包网格，图标来自素材库。
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
    fill(&mut image, 36, 92, WIDTH - 72, 268, PANEL_DARK);
    fill(&mut image, 36, 376, WIDTH - 72, 128, PANEL_DARK);
    if let Some(font) = assets.font() {
        label(
            &mut image,
            font,
            30.0,
            64,
            112,
            LIGHT_TEXT,
            data.display_name,
        );
        label(
            &mut image,
            font,
            20.0,
            64,
            154,
            accent,
            &format!("{} · 装备栏 · 装备 查看 <编号> 看详情", data.system_name),
        );
        for (index, (slot_code, slot_name)) in SLOT_ORDER.into_iter().enumerate() {
            let x = 72 + index as u32 * 106;
            let slot = data
                .equipped
                .iter()
                .find(|slot| slot.slot_code == slot_code);
            let (ring, icon, glyph) = match slot {
                Some(slot) => {
                    let (ring, _) = rarity_tier(&slot.quality);
                    (
                        ring,
                        assets.equipment_icon(&slot.item_name),
                        slot.item_name.chars().next().unwrap_or('器').to_string(),
                    )
                }
                None => (TILE_EMPTY, None, String::new()),
            };
            item_tile(&assets, &mut image, x, 200, 88, ring, icon, &glyph);
            label(
                &mut image,
                font,
                15.0,
                x as i32 + 4,
                294,
                MUTED_TEXT,
                slot_name,
            );
            if let Some(slot) = slot {
                label(
                    &mut image,
                    font,
                    14.0,
                    x as i32 + 4,
                    318,
                    LIGHT_TEXT,
                    &truncate_name(&slot.item_name, 6),
                );
            }
        }
        label(&mut image, font, 20.0, 64, 394, LIGHT_TEXT, "背包");
        if data.bag.is_empty() {
            label(&mut image, font, 16.0, 150, 394, MUTED_TEXT, "暂无其他物品");
        }
        for (index, item) in data.bag.iter().enumerate().take(7) {
            let x = 150 + index as u32 * 100;
            let (ring, _) = rarity_tier(&item.quality);
            item_tile(
                &assets,
                &mut image,
                x,
                376 + 20,
                64,
                ring,
                assets.equipment_icon(&item.name),
                &item.name.chars().next().unwrap_or('物').to_string(),
            );
            label(
                &mut image,
                font,
                13.0,
                x as i32,
                464,
                LIGHT_TEXT,
                &format!("×{}", item.quantity),
            );
        }
        if data.bag.len() > 7 {
            label(
                &mut image,
                font,
                14.0,
                150 + 7 * 100,
                404,
                MUTED_TEXT,
                &format!("+{}", data.bag.len() - 7),
            );
        }
    }
    finish(&assets, &mut image, "LUO REALM / 装备", None, path)
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

/// 渲染物品详情卡片：稀有度名条 + 图标 + 词条列表。
pub fn item_detail(root: &Path, data: &ItemDetailData<'_>, path: &Path) -> io::Result<()> {
    let assets = assets::RealmAssets::discover(root);
    let (ring, stars) = rarity_tier(data.quality);
    let mut image = blank();
    fill(
        &mut image,
        0,
        HEADER_HEIGHT,
        16,
        HEIGHT - HEADER_HEIGHT,
        ring,
    );
    fill(&mut image, 36, 92, 320, 412, PANEL_DARK);
    fill(&mut image, 372, 92, WIDTH - 408, 412, PANEL_DARK);
    fill(&mut image, 36, 92, 320, 8, ring);
    if let Some(font) = assets.font() {
        label(
            &mut image,
            font,
            24.0,
            56,
            116,
            LIGHT_TEXT,
            &truncate_name(data.definition_id, 9),
        );
        label(
            &mut image,
            font,
            16.0,
            56,
            152,
            ring,
            rarity_display(data.quality),
        );
        for star in 0..stars {
            label(
                &mut image,
                font,
                18.0,
                56 + star as i32 * 24,
                178,
                Rgba([232, 190, 92, 255]),
                "★",
            );
        }
        item_tile(
            &assets,
            &mut image,
            118,
            218,
            156,
            ring,
            assets.equipment_icon(data.definition_id),
            &data
                .definition_id
                .chars()
                .next()
                .unwrap_or('器')
                .to_string(),
        );
        if let Some(slot) = data.equipped_slot {
            label(
                &mut image,
                font,
                15.0,
                56,
                396,
                MUTED_TEXT,
                &format!("已装备 · {}", slot_display_name(slot)),
            );
        } else {
            label(&mut image, font, 15.0, 56, 396, MUTED_TEXT, "未装备");
        }
        label(
            &mut image,
            font,
            15.0,
            56,
            424,
            MUTED_TEXT,
            &format!("编号 #{}", data.item_id),
        );

        label(&mut image, font, 26.0, 404, 116, LIGHT_TEXT, "物品详情");
        label(
            &mut image,
            font,
            17.0,
            404,
            162,
            MUTED_TEXT,
            &format!("强化等级 +{} · 持有 {}", data.level, data.quantity),
        );
        if data.modifiers.is_empty() {
            label(
                &mut image,
                font,
                18.0,
                404,
                220,
                MUTED_TEXT,
                "暂无词条加成。",
            );
        }
        for (index, (code, value)) in data.modifiers.iter().enumerate().take(7) {
            let y = 214 + index as i32 * 40;
            label(
                &mut image,
                font,
                19.0,
                404,
                y,
                PANEL_TEXT,
                modifier_name(code),
            );
            let display = if *value >= 0 {
                format!("+{value}")
            } else {
                value.to_string()
            };
            label(
                &mut image,
                font,
                19.0,
                700,
                y,
                Rgba([126, 196, 158, 255]),
                &display,
            );
        }
        label(
            &mut image,
            font,
            15.0,
            404,
            HEIGHT as i32 - 66,
            MUTED_TEXT,
            "穿戴：装备 穿戴 <编号> <槽位> · 卸下：装备 卸下 <槽位>",
        );
    }
    finish(&assets, &mut image, "LUO REALM / 物品详情", None, path)
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
    fill(image, 0, 0, WIDTH, HEADER_HEIGHT, HEADER);
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
