//! battle.rs 的本地验证测试：生成示例 GIF 到 %TEMP%\battle_test。
//! 本文件被 .gitignore 排除，禁止提交。

use std::path::PathBuf;
use std::time::Instant;

use crate::combat::{
    BattleEndReason, BattleRules, CombatAttributes, CombatEvent, CombatEventKind, CombatOutcome,
    CombatSnapshot, CombatantOutcome, CombatantSnapshot, DamageType, ResourceKind,
    ResourceSnapshot, SkillTag, default_loadout, run_battle,
};
use crate::render;
use image::{ImageBuffer, Rgba};

#[allow(clippy::too_many_arguments)]
fn combatant(
    id: &str,
    name: &str,
    team: u8,
    system: &str,
    hp: i64,
    attack: i64,
    crit: i64,
) -> CombatantSnapshot {
    CombatantSnapshot {
        combatant_id: id.into(),
        player_id: None,
        display_name: name.into(),
        character_id: "default".into(),
        system_id: system.into(),
        universal_tier: 3,
        team,
        position: team as i32,
        attributes: CombatAttributes {
            max_health: hp,
            attack,
            physical_defense: 120,
            arcane_defense: 60,
            soul_defense: 60,
            speed: 40,
            critical_rate_basis_points: crit,
            critical_damage_basis_points: 20_000,
            recovery_power: 30,
            control_power: 30,
            tenacity: 500,
            domain_power: 20,
        },
        resource: ResourceSnapshot {
            kind: ResourceKind::SwordIntent,
            current: 100,
            maximum: 100,
            regeneration: 4,
        },
        active_skills: Vec::new(),
        passive_skills: Vec::new(),
        domain_skill: None,
        equipment_triggers: Vec::new(),
        tactic: crate::combat::Tactic::Aggressive,
        power: 9_000,
    }
}

fn snapshot() -> CombatSnapshot {
    let mut left = combatant("A", "影刃", 0, "blood_demon", 1000, 150, 1800);
    let mut right = combatant("B", "霜华", 1, "sword", 1000, 140, 2200);
    let (active, passive, domain) = default_loadout("blood_demon", 3);
    left.active_skills = active;
    left.passive_skills = passive;
    left.domain_skill = domain;
    let (active, passive, domain) = default_loadout("sword", 3);
    right.active_skills = active;
    right.passive_skills = passive;
    right.domain_skill = domain;
    CombatSnapshot {
        rule_version: 1,
        seed: 20240601,
        rules: BattleRules::default(),
        combatants: vec![left, right],
    }
}

fn event(
    sequence: u64,
    tick: u32,
    source: Option<&str>,
    target: Option<&str>,
    kind: CombatEventKind,
) -> CombatEvent {
    CombatEvent {
        sequence,
        tick,
        source_id: source.map(str::to_string),
        target_id: target.map(str::to_string),
        trigger_chain: 0,
        kind,
    }
}

fn scripted_outcome() -> CombatOutcome {
    let events = vec![
        event(1, 1, Some("A"), Some("B"), CombatEventKind::BattleStarted),
        event(
            2,
            2,
            Some("A"),
            Some("B"),
            CombatEventKind::SkillCast {
                skill_id: "s1".into(),
                skill_name: "暗影突袭".into(),
                tags: vec![SkillTag::Attack],
            },
        ),
        event(
            3,
            2,
            Some("A"),
            Some("B"),
            CombatEventKind::DamageApplied {
                amount: 120,
                critical: false,
                damage_type: DamageType::Physical,
            },
        ),
        event(
            4,
            5,
            Some("B"),
            Some("A"),
            CombatEventKind::SkillCast {
                skill_id: "s2".into(),
                skill_name: "冰霜结晶".into(),
                tags: vec![SkillTag::Attack],
            },
        ),
        event(
            5,
            5,
            Some("B"),
            Some("A"),
            CombatEventKind::DamageApplied {
                amount: 95,
                critical: false,
                damage_type: DamageType::Arcane,
            },
        ),
        event(
            6,
            9,
            Some("A"),
            Some("B"),
            CombatEventKind::SkillCast {
                skill_id: "s3".into(),
                skill_name: "月影斩".into(),
                tags: vec![SkillTag::Attack],
            },
        ),
        event(
            7,
            9,
            Some("A"),
            Some("B"),
            CombatEventKind::DamageApplied {
                amount: 316,
                critical: true,
                damage_type: DamageType::Physical,
            },
        ),
        event(
            8,
            13,
            Some("B"),
            Some("A"),
            CombatEventKind::SkillCast {
                skill_id: "s4".into(),
                skill_name: "寒霜爆".into(),
                tags: vec![SkillTag::Attack],
            },
        ),
        event(9, 13, Some("B"), Some("A"), CombatEventKind::Dodged),
        event(
            10,
            16,
            Some("B"),
            None,
            CombatEventKind::PassiveTriggered {
                definition_id: "p1".into(),
                name: "寒霜共鸣".into(),
            },
        ),
        event(
            11,
            18,
            Some("B"),
            Some("A"),
            CombatEventKind::SkillCast {
                skill_id: "s5".into(),
                skill_name: "极冰裂".into(),
                tags: vec![SkillTag::Attack],
            },
        ),
        event(
            12,
            18,
            Some("B"),
            Some("A"),
            CombatEventKind::Blocked { prevented: 40 },
        ),
        event(
            13,
            18,
            Some("B"),
            Some("A"),
            CombatEventKind::DamageApplied {
                amount: 88,
                critical: false,
                damage_type: DamageType::Arcane,
            },
        ),
        event(
            14,
            21,
            Some("B"),
            Some("B"),
            CombatEventKind::HealingApplied { amount: 130 },
        ),
        event(
            15,
            24,
            Some("A"),
            Some("B"),
            CombatEventKind::SkillCast {
                skill_id: "s6".into(),
                skill_name: "夜刃风暴".into(),
                tags: vec![SkillTag::Attack],
            },
        ),
        event(
            16,
            24,
            Some("A"),
            Some("B"),
            CombatEventKind::DamageApplied {
                amount: 180,
                critical: true,
                damage_type: DamageType::Physical,
            },
        ),
        event(
            17,
            27,
            Some("A"),
            Some("B"),
            CombatEventKind::SkillCast {
                skill_id: "s7".into(),
                skill_name: "幽冥刺".into(),
                tags: vec![SkillTag::Attack],
            },
        ),
        event(
            18,
            27,
            Some("A"),
            Some("B"),
            CombatEventKind::DamageApplied {
                amount: 306,
                critical: false,
                damage_type: DamageType::Soul,
            },
        ),
        event(
            19,
            28,
            None,
            None,
            CombatEventKind::BattleEnded {
                winner_team: 0,
                reason: BattleEndReason::Defeated,
            },
        ),
    ];
    CombatOutcome {
        seed: 20240601,
        winner_team: 0,
        end_reason: BattleEndReason::Defeated,
        elapsed_ticks: 28,
        events,
        combatants: vec![
            CombatantOutcome {
                combatant_id: "A".into(),
                team: 0,
                health: 817,
                max_health: 1000,
                damage_dealt: 922,
                healing_done: 0,
                defeated: false,
            },
            CombatantOutcome {
                combatant_id: "B".into(),
                team: 1,
                health: 0,
                max_health: 1000,
                damage_dealt: 183,
                healing_done: 130,
                defeated: true,
            },
        ],
    }
}

fn output_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("battle_test");
    std::fs::create_dir_all(&dir).expect("create test output dir");
    dir.join(name)
}

/// 解码 GIF 并把指定帧合成导出为 PNG，便于人工检查视觉效果。
fn dump_gif_frames(gif_path: &std::path::Path, stem: &str, wanted: &[usize]) {
    use gif::ColorOutput;

    let mut options = gif::DecodeOptions::new();
    options.set_color_output(ColorOutput::RGBA);
    let file = std::fs::File::open(gif_path).expect("open gif");
    let mut decoder = options.read_info(file).expect("read gif header");

    let mut canvas = ImageBuffer::new(640, 360);
    let mut index = 0_usize;
    let mut delays = Vec::new();
    let mut wanted_iter = wanted.iter().copied();
    let mut upcoming = wanted_iter.next();
    while let Some(frame) = decoder.read_next_frame().expect("decode frame") {
        delays.push(frame.delay);
        let width = usize::from(frame.width);
        for (row, pixels) in frame.buffer.chunks(width * 4).enumerate() {
            for (column, pixel) in pixels.chunks_exact(4).enumerate() {
                let x = u32::from(frame.left) + column as u32;
                let y = u32::from(frame.top) + row as u32;
                if x < 640 && y < 360 {
                    canvas.put_pixel(x, y, Rgba([pixel[0], pixel[1], pixel[2], 255]));
                }
            }
        }
        if upcoming == Some(index) {
            let out = output_path(&format!("{stem}_frame{index:03}.png"));
            image::DynamicImage::ImageRgba8(canvas.clone())
                .save_with_format(&out, image::ImageFormat::Png)
                .expect("save png");
            upcoming = wanted_iter.next();
        }
        index += 1;
    }
    let shown = delays.len().min(8);
    println!(
        "{stem}: {index} frames, delays(first {shown}): {:?}",
        &delays[..shown]
    );
}

#[test]
fn engine_view_tracks_health_changes() {
    use super::extract_view;

    let snapshot = snapshot();
    let outcome = run_battle(&snapshot).expect("engine battle");
    let view = extract_view(&snapshot, &outcome).expect("view");
    for (index, beat) in view.beats.iter().enumerate() {
        let detail = match &beat.strike {
            super::Strike::Damage {
                amount, critical, ..
            } => format!("Damage {amount}{}", if *critical { "!" } else { "" }),
            super::Strike::Heal { amount } => format!("Heal {amount}"),
            super::Strike::Support => "Support".to_string(),
        };
        println!(
            "beat{index}: actor={} {} hp={:?}",
            beat.actor, detail, beat.hp
        );
    }
    assert!(
        view.beats
            .iter()
            .any(|beat| matches!(beat.strike, super::Strike::Damage { .. })),
        "engine battle 应产生伤害节拍"
    );
}

/// 使用真实角色图与技能素材的用户对决：把现有素材复制到独立 fixture 根目录，
/// 以部署时的目录结构（<root>/data/luo_realm/assets/...）渲染 GIF。
#[test]
fn renders_portrait_duels() {
    let root = output_path("portrait_root");
    let data_dir = root.join("data").join("luo_realm");
    let assets_dir = data_dir.join("assets").join("realm");
    let portraits_dir = assets_dir.join("portraits");
    std::fs::create_dir_all(&portraits_dir).expect("create fixture portraits dir");
    for id in ["0", "1"] {
        let source = std::path::Path::new("assets")
            .join("realm")
            .join("portraits")
            .join(format!("{id}.png"));
        std::fs::copy(&source, portraits_dir.join(format!("{id}.png")))
            .expect("copy existing portrait into fixture");
    }
    for dir in ["skill_icons", "skill_effects"] {
        let target = assets_dir.join(dir);
        std::fs::create_dir_all(&target).expect("create fixture skill dir");
        let source_dir = std::path::Path::new("assets").join("realm").join(dir);
        std::fs::read_dir(&source_dir)
            .expect("list skill assets")
            .try_for_each(|entry| {
                let entry = entry?;
                std::fs::copy(entry.path(), target.join(entry.file_name())).map(|_| ())
            })
            .expect("copy skill assets");
    }

    let mut snapshot = snapshot();
    snapshot.combatants[0].character_id = "0".into();
    snapshot.combatants[1].character_id = "1".into();

    let scripted_path = output_path("portrait_duel_scripted.gif");
    let started = Instant::now();
    render::battle(&root, &snapshot, &scripted_outcome(), &scripted_path)
        .expect("render portrait scripted duel");
    println!("portrait scripted duel: {:?}", started.elapsed());
    dump_gif_frames(
        &scripted_path,
        "portrait_scripted",
        &[5, 16, 24, 29, 33, 92],
    );

    let engine_outcome = run_battle(&snapshot).expect("engine battle");
    let engine_path = output_path("portrait_duel_engine.gif");
    let started = Instant::now();
    render::battle(&root, &snapshot, &engine_outcome, &engine_path)
        .expect("render portrait engine duel");
    println!("portrait engine duel: {:?}", started.elapsed());
    dump_gif_frames(&engine_path, "portrait_engine", &[10, 30, 55, 80]);

    for path in [&scripted_path, &engine_path] {
        let size = std::fs::metadata(path).expect("portrait gif exists").len();
        assert!(size > 10_000);
    }
}

#[test]
fn renders_scripted_duel_gif() {
    let snapshot = snapshot();
    let outcome = scripted_outcome();
    let path = output_path("scripted_duel.gif");
    let started = Instant::now();
    render::battle(std::path::Path::new("."), &snapshot, &outcome, &path)
        .expect("render scripted duel");
    let elapsed = started.elapsed();
    let size = std::fs::metadata(&path).expect("gif exists").len();
    println!("scripted duel: {elapsed:?}, {size} bytes");
    dump_gif_frames(&path, "scripted", &[5, 16, 24, 40, 60, 84, 92, 100]);
    assert!(size > 10_000);
    if !cfg!(debug_assertions) {
        assert!(
            elapsed.as_millis() < 1500,
            "渲染耗时异常（并行测试有争抢，单独运行应约 0.6 秒）：{elapsed:?}"
        );
    }
}

#[test]
fn renders_engine_duel_gif() {
    let snapshot = snapshot();
    let outcome = run_battle(&snapshot).expect("engine battle");
    let path = output_path("engine_duel.gif");
    let started = Instant::now();
    render::battle(std::path::Path::new("."), &snapshot, &outcome, &path)
        .expect("render engine duel");
    let elapsed = started.elapsed();
    let size = std::fs::metadata(&path).expect("gif exists").len();
    println!(
        "engine duel: {elapsed:?}, {size} bytes, {} events",
        outcome.events.len()
    );
    dump_gif_frames(&path, "engine", &[10, 30, 55, 80]);
    assert!(size > 10_000);
    if !cfg!(debug_assertions) {
        assert!(
            elapsed.as_millis() < 1500,
            "渲染耗时异常（并行测试有争抢，单独运行应约 0.6 秒）：{elapsed:?}"
        );
    }
}

#[test]
fn renders_empty_battle_gif() {
    let snapshot = snapshot();
    let outcome = CombatOutcome {
        seed: 7,
        winner_team: 0,
        end_reason: BattleEndReason::Timeout,
        elapsed_ticks: 10,
        events: vec![event(
            1,
            10,
            None,
            None,
            CombatEventKind::BattleEnded {
                winner_team: 0,
                reason: BattleEndReason::Timeout,
            },
        )],
        combatants: Vec::new(),
    };
    let path = output_path("empty_battle.gif");
    render::battle(std::path::Path::new("."), &snapshot, &outcome, &path)
        .expect("render empty battle");
    let size = std::fs::metadata(&path).expect("gif exists").len();
    println!("empty battle: {size} bytes");
    assert!(size > 5_000);
}
