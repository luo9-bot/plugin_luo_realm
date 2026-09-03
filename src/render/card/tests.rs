//! 卡片渲染的本地验证：输出 PNG 到临时目录并检查基本属性。

use std::path::Path;

use super::{
    DestinyCardData, EquipmentCardData, SkillCardData, SystemCardEntry, WorldEventCardData,
    destiny, equipment, menu, skills, systems, world_event,
};

fn output_path(name: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join("lr_card_test");
    std::fs::create_dir_all(&directory).expect("create card test dir");
    directory.join(name)
}

fn assert_card(path: &Path) {
    let metadata = std::fs::metadata(path).expect("card file exists");
    assert!(
        metadata.len() > 1_500,
        "卡片 {} 过小，可能未绘制内容",
        path.display()
    );
    let image = image::open(path).expect("decode card");
    assert_eq!((image.width(), image.height()), (960, 540));
}

#[test]
fn menu_card_renders() {
    let path = output_path("menu.png");
    menu(Path::new("."), &path).expect("render menu");
    assert_card(&path);
}

#[test]
fn systems_card_renders_all_eleven() {
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
        positioning: "定位示例".into(),
    })
    .collect::<Vec<_>>();
    let path = output_path("systems.png");
    systems(Path::new("."), &entries, &path).expect("render systems");
    assert_card(&path);
}

#[test]
fn skill_and_equipment_cards_render() {
    let skill_data = SkillCardData {
        display_name: "演示剑修",
        system_name: "剑修",
        system_id: "sword",
        tactic_name: "强攻",
        skills: &[
            ("晨露斩".into(), 1),
            ("燕回三叠".into(), 3),
            ("剑气纵横".into(), 0),
        ],
    };
    let skill_path = output_path("skills.png");
    skills(Path::new("."), &skill_data, &skill_path).expect("render skills");
    assert_card(&skill_path);

    let equipment_data = EquipmentCardData {
        display_name: "演示剑修",
        system_name: "剑修",
        system_id: "sword",
        equipped: &[("main_hand".into(), "精铁长剑".into())],
        bag: &[
            ("凝血丹".into(), 3),
            ("护身符".into(), 1),
            ("玄铁".into(), 7),
        ],
    };
    let equipment_path = output_path("equipment.png");
    equipment(Path::new("."), &equipment_data, &equipment_path).expect("render equipment");
    assert_card(&equipment_path);
}

#[test]
fn destiny_and_world_event_cards_render() {
    let destiny_data = DestinyCardData {
        destiny_name: "资源潮汐",
        description: "灵材涌动，坊市兴盛，交易采集皆有裨益。",
        world_event_line: Some("群内签到、机缘与决斗会推进今日世界事件。"),
    };
    let destiny_path = output_path("destiny.png");
    destiny(Path::new("."), &destiny_data, &destiny_path).expect("render destiny");
    assert_card(&destiny_path);

    let event_data = WorldEventCardData {
        event_name: "魔物入侵",
        description: "妖兽出没界缘，历练除魔可得厚赏。",
        status: "进行中",
        completed: false,
        coin_reward: 800,
        mark_reward: 5,
        objectives: &[("签到人数".into(), 3, 5), ("完成决斗".into(), 7, 9)],
    };
    let event_path = output_path("world_event.png");
    world_event(Path::new("."), &event_data, &event_path).expect("render world event");
    assert_card(&event_path);
}
