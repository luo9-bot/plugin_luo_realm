mod registration;

use std::path::Path;

use rusqlite::Transaction;

use crate::{
    combat,
    config::{CommandConfig, GameConfig, GameplayConfig, PlayerWebConfig, RuntimeConfig},
    core::{Player, stable_seed},
    database::{self, Database, DatabaseError},
    domain::shared::GroupId,
    engine, identity, render,
};

const UNREGISTERED_MESSAGE: &str = "尚未注册，请发送“注册 <名称>”创建角色。";
const PENDING_SYSTEM_MESSAGE: &str = "角色尚未确定修行体系，请发送“体系”查看并选择。";
const UNAVAILABLE_MESSAGE: &str = "当前角色已被停用，请联系管理员。";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AccessLevel {
    Public,
    Named,
    Active,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    Menu,
    Systems,
    Register,
    SelectSystem,
    Power,
    Profile,
    CheckIn,
    Event,
    DailyState,
    WorldEvent,
    Ranking,
    Rename,
    Duel,
    AsciiFpv,
    Redeem,
    Skills,
    Tactic,
    Equipment,
    Cultivate,
    HomePage,
}

impl Command {
    fn parse(keyword: &str) -> Option<Self> {
        match keyword {
            "菜单" | "帮助" | "help" => Some(Self::Menu),
            "体系" | "修行" | "cultivation" => Some(Self::Systems),
            "注册" | "register" => Some(Self::Register),
            "选择体系" | "system" => Some(Self::SelectSystem),
            "战力" | "power" => Some(Self::Power),
            "状态" | "属性" | "profile" | "查询" => Some(Self::Profile),
            "签到" | "刻印" | "checkin" => Some(Self::CheckIn),
            "每日事件" | "事件" | "event" => Some(Self::Event),
            "今日状态" | "每日状态" | "daily" => Some(Self::DailyState),
            "世界事件" | "群事件" | "world" => Some(Self::WorldEvent),
            "排行" | "ranking" => Some(Self::Ranking),
            "改名" | "name" => Some(Self::Rename),
            "决斗" | "duel" => Some(Self::Duel),
            "御空试炼" | "飞行试炼" | "fpv" => Some(Self::AsciiFpv),
            "兑换" | "redeem" => Some(Self::Redeem),
            "技能" | "skills" => Some(Self::Skills),
            "战术" | "tactic" => Some(Self::Tactic),
            "装备" | "equipment" => Some(Self::Equipment),
            "修行行动" | "cultivate" => Some(Self::Cultivate),
            "主页" | "个人主页" | "网页" => Some(Self::HomePage),
            _ => None,
        }
    }

    fn access(self) -> AccessLevel {
        match self {
            Self::Menu | Self::Systems | Self::Register => AccessLevel::Public,
            Self::SelectSystem | Self::Rename => AccessLevel::Named,
            _ => AccessLevel::Active,
        }
    }

    pub fn feature_code(self) -> &'static str {
        match self {
            Self::Event | Self::WorldEvent => "event",
            Self::Duel => "combat",
            _ => "general",
        }
    }
}

pub fn feature_code(message: &str, config: &CommandConfig) -> &'static str {
    config
        .command_text(message)
        .and_then(|text| text.split_whitespace().next())
        .and_then(Command::parse)
        .map(Command::feature_code)
        .unwrap_or("general")
}

pub fn handle_message(
    database: &mut Database,
    root: &Path,
    group_id: u64,
    user_id: u64,
    message: &str,
    config: &RuntimeConfig,
) -> Result<Option<String>, DatabaseError> {
    let Some(text) = config.command.command_text(message) else {
        return Ok(None);
    };
    let arguments = text.split_whitespace().collect::<Vec<_>>();
    let Some(command) = arguments
        .first()
        .and_then(|keyword| Command::parse(keyword))
    else {
        return Ok(None);
    };
    let state = database::player::registration_state(database.connection(), user_id)?;
    if let Some(message) = denied_message(state, command.access()) {
        return Ok(Some(message.into()));
    }

    dispatch(
        command,
        &arguments[1..],
        database,
        root,
        group_id,
        user_id,
        config,
    )
    .map(Some)
}

fn denied_message(
    state: database::player::RegistrationState,
    access: AccessLevel,
) -> Option<&'static str> {
    use database::player::RegistrationState;

    match (state, access) {
        (RegistrationState::Unavailable, _) => Some(UNAVAILABLE_MESSAGE),
        (RegistrationState::Missing, AccessLevel::Named | AccessLevel::Active) => {
            Some(UNREGISTERED_MESSAGE)
        }
        (RegistrationState::PendingSystem, AccessLevel::Active) => Some(PENDING_SYSTEM_MESSAGE),
        _ => None,
    }
}

fn dispatch(
    command: Command,
    arguments: &[&str],
    database: &mut Database,
    root: &Path,
    group_id: u64,
    user_id: u64,
    config: &RuntimeConfig,
) -> Result<String, DatabaseError> {
    match command {
        Command::Menu => menu(database, root),
        Command::Systems => systems(database, root),
        Command::Register => registration::register(database, user_id, arguments),
        Command::SelectSystem => registration::select_system(database, user_id, arguments),
        Command::Rename => registration::rename(database, user_id, arguments),
        Command::Power => power(database, user_id),
        Command::Profile => profile(database, root, user_id),
        Command::CheckIn => check_in(database, group_id, user_id),
        Command::Event => event(database, root, group_id, user_id),
        Command::DailyState => daily_state(database, user_id),
        Command::WorldEvent => world_event(database, root, group_id, user_id),
        Command::Ranking => ranking(database, user_id),
        Command::Duel => duel(
            database,
            root,
            group_id,
            user_id,
            arguments,
            &config.gameplay,
        ),
        Command::AsciiFpv => ascii_fpv(database, root, user_id, &config.game),
        Command::Redeem => redeem(database, user_id, arguments, &config.game),
        Command::Skills => skills(database, root, user_id, arguments),
        Command::Tactic => tactic(database, user_id, arguments),
        Command::Equipment => equipment(database, root, user_id, arguments),
        Command::Cultivate => cultivate(database, user_id, arguments),
        Command::HomePage => home_page(database, root, user_id, &config.player_web),
    }
}

/// 把卡片渲染到插件数据目录并返回图片回复；失败时回退文字。
fn card_reply<F>(root: &Path, file_name: &str, render: F, fallback: String) -> String
where
    F: FnOnce(&Path, &Path) -> std::io::Result<()>,
{
    let image = card_path(root, file_name);
    match render(root, &image) {
        Ok(()) => image_reply(&image),
        Err(error) => {
            eprintln!("[Luo Realm] card rendering failed: {error}");
            fallback
        }
    }
}

fn card_path(root: &Path, file_name: &str) -> std::path::PathBuf {
    root.join(identity::DATA_DIRECTORY)
        .join("cards")
        .join(file_name)
}

fn image_reply(path: &Path) -> String {
    format!("[CQ:image,file={}]", path.display())
}

/// 玩法菜单卡片（图片优先）。
fn menu(database: &mut Database, root: &Path) -> Result<String, DatabaseError> {
    let _ = database;
    let fallback = format!(
        "{}：注册 / 体系 / 选择体系 / 签到 / 修行行动 / 技能 / 战术 / 装备 / 状态 / 今日状态 / 战力 / 每日事件 / 世界事件 / 决斗 / 御空试炼 / 兑换 / 排行 / 改名",
        identity::PRODUCT_NAME
    );
    Ok(card_reply(root, "menu.png", render::menu, fallback))
}

/// 修行体系总览卡片（图片优先）。
fn systems(database: &mut Database, root: &Path) -> Result<String, DatabaseError> {
    let _ = database;
    let entries = crate::cultivation::registered_systems()
        .into_iter()
        .map(|system| render::SystemCardEntry {
            name: system.name().to_owned(),
            id: system.id().to_owned(),
            positioning: crate::render::card::system_positioning(system.id()).to_owned(),
        })
        .collect::<Vec<_>>();
    let fallback = format!("可选修行体系：{}", registration::system_catalog());
    Ok(card_reply(
        root,
        "systems.png",
        |root, path| render::systems(root, &entries, path),
        fallback,
    ))
}

/// 签发一次性网页票据并返回档案页链接（设计方案书 20.3）。
fn home_page(
    database: &mut Database,
    root: &Path,
    user_id: u64,
    config: &PlayerWebConfig,
) -> Result<String, DatabaseError> {
    if !config.enabled {
        return Ok("玩家网页尚未启用，请联系管理员在配置中开启。".into());
    }
    let token_path = crate::paths::data_directory(root).join("admin.token");
    let token = crate::admin::auth::AdminToken::load_or_create(&token_path)
        .map_err(|error| DatabaseError::InvalidData(format!("签名密钥不可用：{error}")))?;
    let key = token.signing_key();
    let transaction = database.immediate_transaction()?;
    active_player(&transaction, user_id)?;
    let ticket = crate::player_web::ticket::issue(
        &transaction,
        user_id,
        crate::player_web::session::SCOPE_PROFILE_READ,
        &key,
        crate::database::unix_timestamp(),
        i64::from(config.ticket_ttl_minutes) * 60,
    )?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    Ok(format!(
        "你的专属档案页已生成（{0} 分钟内有效，仅可打开一次）：\n{1}?ticket={2}\n页面只读，修行操作仍在群聊完成。",
        config.ticket_ttl_minutes, config.base_url, ticket.token
    ))
}

fn ascii_fpv(
    database: &mut Database,
    root: &Path,
    user_id: u64,
    config: &GameConfig,
) -> Result<String, DatabaseError> {
    if config.reward_public_key.trim().is_empty() {
        return Ok("御空试炼尚未完成兑换公钥配置，请联系管理员。".into());
    }
    match crate::game::issue_ascii_fpv_url(database, root, user_id, config) {
        Ok(url) => Ok(format!(
            "御空试炼已开启：\n{url}\n游戏可无限重开；兑换次数按每日额度计算，网址 2 小时内有效，兑换码 24 小时内有效。"
        )),
        Err(crate::game::GameError::NotConfigured) => Ok("御空试炼当前未开启。".into()),
        Err(crate::game::GameError::Activation(_)) => {
            Ok("云端试炼会话激活失败，请稍后重试。".into())
        }
        Err(crate::game::GameError::RateLimited) => {
            Ok("御空试炼链接生成过于频繁，请 15 秒后再试。".into())
        }
        Err(error) => Err(DatabaseError::InvalidData(error.to_string())),
    }
}

fn redeem(
    database: &mut Database,
    user_id: u64,
    arguments: &[&str],
    config: &GameConfig,
) -> Result<String, DatabaseError> {
    if !config.ascii_fpv_enabled || config.reward_public_key.trim().is_empty() {
        return Ok("小游戏兑换功能当前未开启。".into());
    }
    let Some(code) = arguments.first() else {
        return Ok("请发送“兑换 <兑换码>”。".into());
    };
    let now = database::unix_timestamp();
    let voucher =
        match crate::game::verify_reward_voucher(code, &config.reward_public_key, user_id, now) {
            Ok(voucher) => voucher,
            Err(crate::game::GameError::ExpiredVoucher) => return Ok("兑换码已经过期。".into()),
            Err(crate::game::GameError::WrongPlayer) => {
                return Ok("该兑换码不属于当前账号。".into());
            }
            Err(crate::game::GameError::NotConfigured) => {
                return Ok("小游戏兑换公钥尚未配置。".into());
            }
            Err(_) => return Ok("兑换码无效或已被修改。".into()),
        };
    let date = database.local_date()?;
    let transaction = database.immediate_transaction()?;
    active_player(&transaction, user_id)?;
    let result = database::game_reward::redeem(
        &transaction,
        &voucher,
        &date,
        config.daily_redemption_limit,
    )?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;

    Ok(match result {
        database::game_reward::RedemptionResult::Redeemed {
            reward,
            balance_after,
            remaining_today,
        } => format!(
            "兑换成功：金币 +{reward}。当前金币 {balance_after}，今日还可兑换 {remaining_today} 次。"
        ),
        database::game_reward::RedemptionResult::AlreadyRedeemed => "该兑换码已经使用过。".into(),
        database::game_reward::RedemptionResult::DailyLimitReached => {
            "今天的小游戏兑换次数已用完，仍可继续游玩。".into()
        }
    })
}

fn active_player(transaction: &Transaction<'_>, user_id: u64) -> Result<Player, DatabaseError> {
    database::player::get_active(transaction, user_id)?.ok_or_else(|| {
        DatabaseError::InvalidData("active player is missing cultivation data".into())
    })
}

fn skills(
    database: &mut Database,
    root: &Path,
    user_id: u64,
    arguments: &[&str],
) -> Result<String, DatabaseError> {
    let transaction = database.immediate_transaction()?;
    if let Some(skill_id) = arguments.first()
        && arguments.get(1).is_some_and(|action| *action == "研习")
    {
        let mastery = database::skills::train(&transaction, user_id, skill_id)?;
        transaction.commit().map_err(DatabaseError::from_sqlite)?;
        return Ok(format!("技能 {skill_id} 熟练度提升至 {mastery}/3。"));
    }
    let player = active_player(&transaction, user_id)?;
    let cultivation = database::cultivation::get(&transaction, user_id)?;
    let skill_list = database::skills::list(&transaction, user_id)?;
    let tactic = database::skills::current_tactic(&transaction, user_id)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    let system = engine::find_system(&cultivation.system_id)
        .ok_or_else(|| DatabaseError::InvalidData("unknown player cultivation system".into()))?;
    let skills = skill_list
        .iter()
        .map(|skill| (skill.definition.name.clone(), skill.mastery))
        .collect::<Vec<_>>();
    let fallback = skill_list
        .iter()
        .map(|skill| {
            format!(
                "{} {}（熟练度 {}/3）",
                skill.definition.id, skill.definition.name, skill.mastery
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let data = render::SkillCardData {
        display_name: &player.display_name,
        system_name: system.name(),
        system_id: &cultivation.system_id,
        tactic_name: tactic.name(),
        skills: &skills,
    };
    Ok(card_reply(
        root,
        &format!("skills_{user_id}.png"),
        |root, path| render::skills(root, &data, path),
        fallback,
    ))
}

fn tactic(
    database: &mut Database,
    user_id: u64,
    arguments: &[&str],
) -> Result<String, DatabaseError> {
    let Some(code) = arguments.first() else {
        return Ok("战术可选：balanced / aggressive / defensive / sustain / control".into());
    };
    let tactic = combat::Tactic::from_code(code)
        .ok_or_else(|| DatabaseError::InvalidData("未知战术方案".into()))?;
    let transaction = database.immediate_transaction()?;
    database::skills::set_tactic(&transaction, user_id, tactic)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    Ok(format!("自动战术已切换为：{}。", tactic.name()))
}

fn equipment(
    database: &mut Database,
    root: &Path,
    user_id: u64,
    arguments: &[&str],
) -> Result<String, DatabaseError> {
    let transaction = database.immediate_transaction()?;
    if arguments.first().is_some_and(|action| *action == "穿戴") {
        let item_id = arguments
            .get(1)
            .and_then(|value| value.parse::<i64>().ok())
            .ok_or_else(|| DatabaseError::InvalidData("请指定物品编号".into()))?;
        let slot = arguments
            .get(2)
            .and_then(|value| combat::EquipmentSlot::from_code(value))
            .ok_or_else(|| DatabaseError::InvalidData("请指定装备槽标识".into()))?;
        database::inventory::equip(&transaction, user_id, item_id, slot)?;
        transaction.commit().map_err(DatabaseError::from_sqlite)?;
        return Ok(format!("装备 {} 已穿戴至 {}。", item_id, slot.code()));
    }
    if arguments.first().is_some_and(|action| *action == "卸下") {
        let slot = arguments
            .get(1)
            .and_then(|value| combat::EquipmentSlot::from_code(value))
            .ok_or_else(|| DatabaseError::InvalidData("请指定装备槽标识".into()))?;
        let changed = database::inventory::unequip(&transaction, user_id, slot)?;
        transaction.commit().map_err(DatabaseError::from_sqlite)?;
        return Ok(if changed {
            format!("已卸下 {}。", slot.code())
        } else {
            "该装备槽为空。".into()
        });
    }
    let view_target = if arguments.first() == Some(&"查看") {
        arguments.get(1).and_then(|value| value.parse::<i64>().ok())
    } else {
        arguments
            .first()
            .and_then(|value| value.parse::<i64>().ok())
    };
    if let Some(item_id) = view_target {
        let Some(detail) = database::inventory::item_detail(&transaction, user_id, item_id)? else {
            transaction.commit().map_err(DatabaseError::from_sqlite)?;
            return Ok(format!("没有找到编号 #{item_id} 的物品。"));
        };
        transaction.commit().map_err(DatabaseError::from_sqlite)?;
        let equipped_slot = detail.equipped_slot.clone();
        let equipped_note = equipped_slot
            .as_ref()
            .map(|slot| format!("已装备于 {slot}"))
            .unwrap_or_else(|| "未装备".into());
        let fallback = format!(
            "#{} {} ×{} 品质 {} +{} {}",
            detail.item_id,
            detail.definition_id,
            detail.quantity,
            detail.quality,
            detail.level,
            equipped_note
        );
        return Ok(card_reply(
            root,
            &format!("item_{item_id}.png"),
            |root, path| {
                render::item_detail(
                    root,
                    &render::ItemDetailData {
                        item_id: detail.item_id,
                        definition_id: &detail.definition_id,
                        quality: &detail.quality,
                        level: detail.level,
                        quantity: detail.quantity,
                        equipped_slot: equipped_slot.as_deref(),
                        modifiers: &detail.modifiers,
                    },
                    path,
                )
            },
            fallback,
        ));
    }
    let player = active_player(&transaction, user_id)?;
    let cultivation = database::cultivation::get(&transaction, user_id)?;
    let items = database::inventory::list(&transaction, user_id)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    let system = engine::find_system(&cultivation.system_id)
        .ok_or_else(|| DatabaseError::InvalidData("unknown player cultivation system".into()))?;
    let equipped = items
        .iter()
        .filter_map(|item| {
            item.equipped_slot
                .clone()
                .map(|slot| render::EquippedSlotView {
                    slot_code: slot,
                    item_name: item.definition_id.clone(),
                    quality: item.quality.clone(),
                })
        })
        .collect::<Vec<_>>();
    let bag = items
        .iter()
        .filter(|item| item.equipped_slot.is_none())
        .map(|item| render::BagItemView {
            name: item.definition_id.clone(),
            quality: item.quality.clone(),
            quantity: item.quantity,
        })
        .collect::<Vec<_>>();
    let fallback = items
        .iter()
        .map(|item| {
            format!(
                "#{} {} x{} {}",
                item.item_id,
                item.definition_id,
                item.quantity,
                item.equipped_slot
                    .clone()
                    .unwrap_or_else(|| "未装备".into())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let data = render::EquipmentCardData {
        display_name: &player.display_name,
        system_name: system.name(),
        system_id: &cultivation.system_id,
        equipped: &equipped,
        bag: &bag,
    };
    Ok(card_reply(
        root,
        &format!("equipment_{user_id}.png"),
        |root, path| render::equipment(root, &data, path),
        fallback,
    ))
}

fn cultivate(
    database: &mut Database,
    user_id: u64,
    arguments: &[&str],
) -> Result<String, DatabaseError> {
    let action = arguments.first().copied().unwrap_or("吐纳");
    let allowed = ["吐纳", "闭关", "研习", "历练", "淬炼", "休养"];
    if !allowed.contains(&action) {
        return Ok("修行行动：吐纳 / 闭关 / 研习 / 历练 / 淬炼 / 休养".into());
    }
    let date = database.local_date()?;
    let transaction = database.immediate_transaction()?;
    let changed = transaction.execute(
        "INSERT INTO player_cultivation_actions(player_id, action_date, action_code, result_json, created_at)
         VALUES(?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![database::player_id(user_id)?, date, action, "{}", database::unix_timestamp()],
    );
    if let Err(error) = changed {
        if matches!(error, rusqlite::Error::SqliteFailure(_, _)) {
            return Ok("今天已经完成主要修行动作。".into());
        }
        return Err(DatabaseError::from_sqlite(error));
    }
    let progress = match action {
        "吐纳" | "闭关" => 35,
        "研习" => 18,
        "历练" => 25,
        "淬炼" => 12,
        "休养" => 5,
        _ => 0,
    };
    transaction
        .execute(
            "UPDATE player_cultivation SET progress=progress+?2, mastery=mastery+?3,
         fatigue=CASE WHEN ?4='休养' THEN MAX(0, fatigue-200) ELSE MIN(10000, fatigue+100) END,
         injury=CASE WHEN ?4='休养' THEN MAX(0, injury-250) ELSE injury END,
         updated_at=?5 WHERE player_id=?1",
            rusqlite::params![
                database::player_id(user_id)?,
                progress,
                if action == "研习" { 8 } else { 2 },
                action,
                database::unix_timestamp()
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    Ok(format!(
        "今日修行完成：{action}，获得修为进度 +{progress}。"
    ))
}

fn power(database: &mut Database, user_id: u64) -> Result<String, DatabaseError> {
    let date = database.local_date()?;
    let transaction = database.immediate_transaction()?;
    let player = active_player(&transaction, user_id)?;
    let cultivation = database::cultivation::get(&transaction, user_id)?;
    let daily_state = database::daily_state::get_or_create(&transaction, user_id, &date)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    let profile = engine::build_combat_profile_with_state(
        &player,
        &cultivation.system_id,
        cultivation.realm_index,
        &date,
        Some(&daily_state),
    );
    let system = engine::find_system(&cultivation.system_id)
        .ok_or_else(|| DatabaseError::InvalidData("unknown player cultivation system".into()))?;
    let realm = system
        .realms()
        .get(cultivation.realm_index as usize)
        .map(|realm| realm.name)
        .unwrap_or("未知境界");
    Ok(format!(
        "当前战力：{:.0}（{}·{}）\n今日状态：{}",
        profile.power,
        system.name(),
        realm,
        daily_state.name,
    ))
}

fn profile(database: &mut Database, root: &Path, user_id: u64) -> Result<String, DatabaseError> {
    let date = database.local_date()?;
    let transaction = database.immediate_transaction()?;
    let player = active_player(&transaction, user_id)?;
    let cultivation = database::cultivation::get(&transaction, user_id)?;
    let daily_state = database::daily_state::get_or_create(&transaction, user_id, &date)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    let system = engine::find_system(&cultivation.system_id)
        .ok_or_else(|| DatabaseError::InvalidData("unknown player cultivation system".into()))?;
    let realm_name = system
        .realms()
        .get(cultivation.realm_index as usize)
        .map(|realm| realm.name)
        .unwrap_or("未知境界");
    let combat_profile = engine::build_combat_profile_with_state(
        &player,
        &cultivation.system_id,
        cultivation.realm_index,
        &date,
        Some(&daily_state),
    );
    let image = root
        .join(identity::DATA_DIRECTORY)
        .join("cards")
        .join(format!("{user_id}.png"));
    let summary = format!(
        "{} · {}·{}\n今日状态：{}\n等级 {} 修为 {} 战力 {:.0}\nHP {} 攻击 {} 防御 {} 速度 {}\n金币 {} 刻印 {} 胜/负 {}/{}",
        player.display_name,
        system.name(),
        realm_name,
        daily_state.name,
        player.level,
        cultivation.progress,
        combat_profile.power,
        combat_profile.player.base_hp,
        combat_profile.player.base_attack,
        combat_profile.player.base_defense,
        combat_profile.player.speed,
        player.coins,
        player.marks,
        player.wins,
        player.losses
    );
    let render_data = render::ProfileRenderData {
        player: &player,
        system_id: &cultivation.system_id,
        system_name: system.name(),
        realm_name,
        realm_index: cultivation.realm_index,
        progress: cultivation.progress,
        power: combat_profile.power,
    };
    Ok(match render::profile(root, &render_data, &image) {
        Ok(()) => format!("{summary}\n[CQ:image,file={}]", image.display()),
        Err(error) => {
            eprintln!("[Luo Realm] profile rendering failed: {error}");
            summary
        }
    })
}

fn check_in(database: &mut Database, group_id: u64, user_id: u64) -> Result<String, DatabaseError> {
    let date = database.local_date()?;
    let transaction = database.immediate_transaction()?;
    active_player(&transaction, user_id)?;
    database::daily_state::get_or_create(&transaction, user_id, &date)?;
    let result = database::activity::check_in(&transaction, user_id, &date)?;
    let reply = match result {
        database::activity::CheckInResult::Completed { streak, reward } => {
            database::wallet::credit(
                &transaction,
                user_id,
                "marks",
                2,
                "daily_checkin",
                &format!("checkin:{user_id}:{date}:marks"),
            )?;
            let contribution = database::world_event::contribute(
                &transaction,
                group_id,
                user_id,
                &date,
                database::world_event::ContributionKind::CheckIn,
            )?;
            let completion = world_completion_message(contribution.completed);
            format!(
                "签到成功！连续 {streak} 天，金币 +100，刻印 +2。当前金币 {}。",
                reward.balance_after
            ) + completion
        }
        database::activity::CheckInResult::AlreadyCompleted => "今天已经签到过了。".into(),
    };
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    Ok(reply)
}

fn event(
    database: &mut Database,
    root: &Path,
    group_id: u64,
    user_id: u64,
) -> Result<String, DatabaseError> {
    let date = database.local_date()?;
    let transaction = database.immediate_transaction()?;
    active_player(&transaction, user_id)?;
    let daily_state = database::daily_state::get_or_create(&transaction, user_id, &date)?;
    let seed = stable_seed(
        &date,
        "event",
        &format!("{group_id}:{user_id}"),
        identity::VERSION_SALT,
    ) ^ daily_state.seed.rotate_left(17)
        ^ (daily_state.modifiers.destiny * 100.0).round() as u64;
    let definition = engine::event::daily_event(seed);
    let persisted = database::destiny::daily_event(
        &transaction,
        user_id,
        &date,
        definition,
        &seed.to_string(),
    )?;
    let contribution = if persisted.created {
        database::world_event::contribute(
            &transaction,
            group_id,
            user_id,
            &date,
            database::world_event::ContributionKind::Destiny,
        )?
    } else {
        database::world_event::ContributionResult::default()
    };
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    let world_line = if contribution.completed {
        "世界事件目标已达成，贡献奖励已入账。"
    } else {
        "群内签到、机缘与决斗会推进今日世界事件。"
    };
    let fallback = format!("今日机缘：{}", persisted.definition_id)
        + world_completion_message(contribution.completed);
    Ok(card_reply(
        root,
        &format!("destiny_{user_id}.png"),
        |root, path| {
            render::destiny(
                root,
                &render::DestinyCardData {
                    destiny_name: &persisted.definition_id,
                    description: engine::event::description(definition),
                    world_event_line: Some(world_line),
                },
                path,
            )
        },
        fallback,
    ))
}

fn daily_state(database: &mut Database, user_id: u64) -> Result<String, DatabaseError> {
    let date = database.local_date()?;
    let transaction = database.immediate_transaction()?;
    let state = database::daily_state::get_or_create(&transaction, user_id, &date)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    Ok(format!(
        "今日状态·{}\n{}\n生命 ×{:.2} 攻击 ×{:.2} 防御 ×{:.2} 速度 ×{:.2} 暴击 ×{:.2} 机缘 ×{:.2}",
        state.name,
        state.description,
        state.modifiers.hp,
        state.modifiers.attack,
        state.modifiers.defense,
        state.modifiers.speed,
        state.modifiers.critical,
        state.modifiers.destiny,
    ))
}

fn world_event(
    database: &mut Database,
    root: &Path,
    group_id: u64,
    user_id: u64,
) -> Result<String, DatabaseError> {
    if group_id == 0 {
        return Ok("群世界事件仅在群聊中开放。".into());
    }
    let date = database.local_date()?;
    let transaction = database.immediate_transaction()?;
    database::daily_state::get_or_create(&transaction, user_id, &date)?;
    let detail = database::world_event::detail(&transaction, group_id, &date)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    let status = if detail.completed {
        "已完成"
    } else {
        "进行中"
    };
    let objectives = detail
        .objectives
        .iter()
        .map(|(label, current, target)| format!("- {label}：{current}/{target}"))
        .collect::<Vec<_>>()
        .join("\n");
    let fallback = format!(
        "今日世界事件·{}（{status}）\n{}\n{}\n完成奖励：金币 {}、刻印 {}",
        detail.name, detail.description, objectives, detail.coin_reward, detail.mark_reward
    );
    Ok(card_reply(
        root,
        &format!("world_event_{group_id}.png"),
        |root, path| {
            render::world_event(
                root,
                &render::WorldEventCardData {
                    event_name: &detail.name,
                    description: &detail.description,
                    status,
                    completed: detail.completed,
                    coin_reward: detail.coin_reward,
                    mark_reward: detail.mark_reward,
                    objectives: &detail.objectives,
                },
                path,
            )
        },
        fallback,
    ))
}

fn ranking(database: &mut Database, user_id: u64) -> Result<String, DatabaseError> {
    let date = database.local_date()?;
    let transaction = database.immediate_transaction()?;
    database::daily_state::get_or_create(&transaction, user_id, &date)?;
    let entries = database::group::ranking(&transaction, 8)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    if entries.is_empty() {
        return Ok("暂无排行数据。".into());
    }
    Ok(entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| format!("{}. {entry}", index + 1))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn duel(
    database: &mut Database,
    root: &Path,
    group_id: u64,
    user_id: u64,
    arguments: &[&str],
    gameplay_config: &GameplayConfig,
) -> Result<String, DatabaseError> {
    let Some(target_id) = arguments
        .first()
        .map(|argument| {
            argument
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<String>()
        })
        .and_then(|target| target.parse::<u64>().ok())
        .filter(|target| *target != user_id)
    else {
        return Ok("请指定另一位有效玩家。".into());
    };

    let date = database.local_date()?;
    let transaction = database.immediate_transaction()?;
    let left = active_player(&transaction, user_id)?;
    let Some(right) = database::player::get_active(&transaction, target_id)? else {
        return Ok("对方尚未完成角色注册，无法决斗。".into());
    };
    let left_cultivation = database::cultivation::get(&transaction, user_id)?;
    let right_cultivation = database::cultivation::get(&transaction, target_id)?;
    let left_daily = database::daily_state::get_or_create(&transaction, user_id, &date)?;
    let right_daily = database::daily_state::get_or_create(&transaction, target_id, &date)?;
    if group_id != 0 {
        database::group::ensure(&transaction, group_id)?;
    }
    let seed = stable_seed(
        &date,
        "duel",
        &format!("{group_id}:{user_id}:{target_id}"),
        identity::VERSION_SALT,
    );
    let left_equipment = database::inventory::equipped(&transaction, user_id)?;
    let right_equipment = database::inventory::equipped(&transaction, target_id)?;
    let left_loadout = database::skills::loadout(
        &transaction,
        user_id,
        &left_cultivation.system_id,
        combat::universal_tier(
            left_cultivation.realm_index,
            engine::find_system(&left_cultivation.system_id)
                .map(|system| system.realms().len())
                .unwrap_or(1),
        ),
    )?;
    let right_loadout = database::skills::loadout(
        &transaction,
        target_id,
        &right_cultivation.system_id,
        combat::universal_tier(
            right_cultivation.realm_index,
            engine::find_system(&right_cultivation.system_id)
                .map(|system| system.realms().len())
                .unwrap_or(1),
        ),
    )?;
    let snapshot = engine::build_combat_snapshot(
        (&left, &left_cultivation, Some(&left_daily), left_equipment),
        (
            &right,
            &right_cultivation,
            Some(&right_daily),
            right_equipment,
        ),
        &date,
        seed,
    );
    let mut snapshot = snapshot;
    if let Some(left_snapshot) = snapshot.combatants.get_mut(0) {
        left_snapshot.active_skills = left_loadout.active;
        left_snapshot.passive_skills = left_loadout.passive;
        left_snapshot.domain_skill = left_loadout.domain;
        left_snapshot.tactic = left_loadout.tactic;
    }
    if let Some(right_snapshot) = snapshot.combatants.get_mut(1) {
        right_snapshot.active_skills = right_loadout.active;
        right_snapshot.passive_skills = right_loadout.passive;
        right_snapshot.domain_skill = right_loadout.domain;
        right_snapshot.tactic = right_loadout.tactic;
    }
    let result = combat::run_battle(&snapshot)
        .map_err(|error| DatabaseError::InvalidData(error.to_string()))?;
    database::combat::record_battle(
        &transaction,
        GroupId::new(group_id),
        &date,
        &snapshot,
        &result,
    )?;
    let event_completed = database::world_event::contribute_many(
        &transaction,
        group_id,
        &[user_id, target_id],
        &date,
        database::world_event::ContributionKind::Duel,
    )?
    .completed;
    let show_report = database::group::battle_report_enabled(
        &transaction,
        group_id,
        gameplay_config.battle_report_enabled,
    )?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;

    let winner_name = snapshot
        .combatants
        .iter()
        .find(|combatant| combatant.team == result.winner_team)
        .map(|combatant| combatant.display_name.as_str())
        .unwrap_or("未知队伍");
    let concise = format!(
        "决斗结束：{winner_name} 获胜，持续 {} 个时间片，产生 {} 个战斗事件。",
        result.elapsed_ticks,
        result.events.len()
    );
    let completion = world_completion_message(event_completed);
    let image = root
        .join(identity::DATA_DIRECTORY)
        .join("battles")
        .join(format!("{group_id}-{user_id}-{target_id}.gif"));
    let rendered = render::battle(root, &snapshot, &result, &image).is_ok();
    let report_suffix = if show_report {
        format!("\n关键事件数：{}", result.events.len())
    } else {
        String::new()
    };
    Ok(if rendered {
        format!(
            "{}\n[CQ:image,file={}]{}{}",
            concise,
            image.display(),
            completion,
            report_suffix
        )
    } else {
        concise + completion + &report_suffix
    })
}

fn world_completion_message(completed: bool) -> &'static str {
    if completed {
        "\n群世界事件已完成，奖励已自动发放给今日贡献者。"
    } else {
        ""
    }
}
