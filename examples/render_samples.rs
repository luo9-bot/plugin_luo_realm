//! 渲染一套本地样例卡片到 `tests/img/`，供人工查看界面效果。
//!
//! ```powershell
//! cargo run --example render_samples -- [输出目录]
//! ```
//!
//! 默认输出到 `tests/img`（该目录不入 Git）。素材来自
//! `data/luo_realm/assets`，缺失时按运行时逻辑降级。

use std::path::PathBuf;

use luo_realm::{
    core::Player,
    engine,
    render::{
        self, DestinyCardData, EquipmentCardData, SkillCardData, SystemCardEntry,
        WorldEventCardData, card::system_positioning,
    },
};

fn main() -> std::io::Result<()> {
    let root = std::env::current_dir()?;
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tests/img"));
    std::fs::create_dir_all(&output)?;

    let player = demo_player();

    render::menu(&root, &output.join("菜单.png"))?;
    println!("菜单.png");

    let entries = [
        ("修真", "orthodox"),
        ("剑修", "sword"),
        ("体修", "body"),
        ("法修", "mage"),
        ("灵修", "soul"),
        ("气修", "qi"),
        ("血魔邪修", "blood_demon"),
        ("阵修", "formation"),
        ("丹器修", "alchemy_artifact"),
        ("召唤流", "summoner"),
        ("音修", "music"),
    ]
    .into_iter()
    .map(|(name, id)| SystemCardEntry {
        name: name.into(),
        id: id.into(),
        positioning: system_positioning(id).to_owned(),
    })
    .collect::<Vec<_>>();
    render::systems(&root, &entries, &output.join("体系.png"))?;
    println!("体系.png");

    let skills = [
        ("晨露斩", 2_u8),
        ("燕回三叠", 3),
        ("剑气纵横", 1),
        ("心剑合一", 0),
        ("流风回雪", 2),
        ("万剑归宗", 3),
    ]
    .into_iter()
    .map(|(name, mastery)| (name.to_owned(), mastery))
    .collect::<Vec<_>>();
    render::skills(
        &root,
        &SkillCardData {
            display_name: &player.display_name,
            system_name: "剑修",
            system_id: "sword",
            tactic_name: "强攻",
            skills: &skills,
        },
        &output.join("技能.png"),
    )?;
    println!("技能.png");

    let equipped = [
        ("main_hand", "精铁长剑"),
        ("head", "青岚束发冠"),
        ("body", "游侠劲装"),
        ("accessory_1", "凝血玉佩"),
    ]
    .into_iter()
    .map(|(slot, item)| (slot.to_owned(), item.to_owned()))
    .collect::<Vec<_>>();
    let bag = [
        ("凝血丹", 3_i64),
        ("护身符", 1),
        ("玄铁", 7),
        ("剑意残卷", 2),
    ]
    .into_iter()
    .map(|(name, quantity)| (name.to_owned(), quantity))
    .collect::<Vec<_>>();
    render::equipment(
        &root,
        &EquipmentCardData {
            display_name: &player.display_name,
            system_name: "剑修",
            system_id: "sword",
            equipped: &equipped,
            bag: &bag,
        },
        &output.join("装备.png"),
    )?;
    println!("装备.png");

    render::destiny(
        &root,
        &DestinyCardData {
            destiny_name: "资源潮汐",
            description: "灵材涌动，坊市兴盛，交易采集皆有裨益。",
            world_event_line: Some("群内签到、机缘与决斗会推进今日世界事件。"),
        },
        &output.join("机缘.png"),
    )?;
    println!("机缘.png");

    let objectives = [
        ("签到人数".to_owned(), 4_i64, 6_i64),
        ("取得机缘".to_owned(), 5, 5),
        ("完成决斗".to_owned(), 7, 9),
    ];
    render::world_event(
        &root,
        &WorldEventCardData {
            event_name: "魔物入侵",
            description: "妖兽出没界缘，历练除魔可得厚赏。",
            status: "进行中",
            completed: false,
            coin_reward: 800,
            mark_reward: 5,
            objectives: &objectives,
        },
        &output.join("世界事件.png"),
    )?;
    println!("世界事件.png");

    let cultivation_progress = 320_u64;
    let combat = engine::build_combat_profile(&player, "sword", 3, "2026-09-03");
    render::profile(
        &root,
        &render::ProfileRenderData {
            player: &player,
            system_id: "sword",
            system_name: "剑修",
            realm_name: "剑侠",
            realm_index: 3,
            progress: cultivation_progress,
            power: combat.power,
        },
        &output.join("角色卡.png"),
    )?;
    println!("角色卡.png");

    println!("全部样例已输出到 {}", output.display());
    Ok(())
}

fn demo_player() -> Player {
    let mut player = Player::new(10001);
    player.display_name = "洛玖".into();
    player.character_id = "0".into();
    player.level = 12;
    player.coins = 2_800;
    player.marks = 6;
    player.wins = 9;
    player.losses = 0;
    player
}
