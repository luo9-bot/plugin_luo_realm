mod registration;

use std::path::Path;

use rusqlite::Transaction;

use crate::{
    config::{CommandConfig, GameConfig, GameplayConfig, RuntimeConfig},
    core::{Combatant, Player, simulate_combat, stable_seed},
    database::{self, Database, DatabaseError},
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
        Command::Menu => Ok(format!(
            "{}：注册 / 体系 / 选择体系 / 签到 / 状态 / 今日状态 / 战力 / 每日事件 / 世界事件 / 决斗 / 御空试炼 / 兑换 / 排行 / 改名",
            identity::PRODUCT_NAME
        )),
        Command::Systems => Ok(format!("可选修行体系：{}", registration::system_catalog())),
        Command::Register => registration::register(database, user_id, arguments),
        Command::SelectSystem => registration::select_system(database, user_id, arguments),
        Command::Rename => registration::rename(database, user_id, arguments),
        Command::Power => power(database, user_id),
        Command::Profile => profile(database, root, user_id),
        Command::CheckIn => check_in(database, group_id, user_id),
        Command::Event => event(database, group_id, user_id),
        Command::DailyState => daily_state(database, user_id),
        Command::WorldEvent => world_event(database, group_id, user_id),
        Command::Ranking => ranking(database, user_id),
        Command::Duel => duel(
            database,
            root,
            group_id,
            user_id,
            arguments,
            &config.gameplay,
        ),
        Command::AsciiFpv => ascii_fpv(root, user_id, &config.game),
        Command::Redeem => redeem(database, user_id, arguments, &config.game),
    }
}

fn ascii_fpv(root: &Path, user_id: u64, config: &GameConfig) -> Result<String, DatabaseError> {
    if config.reward_public_key.trim().is_empty() {
        return Ok("御空试炼尚未完成兑换公钥配置，请联系管理员。".into());
    }
    match crate::game::issue_ascii_fpv_url(root, user_id, config) {
        Ok(url) => Ok(format!(
            "御空试炼已开启：\n{url}\n游戏可无限重开；兑换次数按每日额度计算，网址 2 小时内有效。"
        )),
        Err(crate::game::GameError::NotConfigured) => Ok("御空试炼当前未开启。".into()),
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

fn event(database: &mut Database, group_id: u64, user_id: u64) -> Result<String, DatabaseError> {
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
    Ok(format!("今日机缘：{}", persisted.definition_id)
        + world_completion_message(contribution.completed))
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
    group_id: u64,
    user_id: u64,
) -> Result<String, DatabaseError> {
    if group_id == 0 {
        return Ok("群世界事件仅在群聊中开放。".into());
    }
    let date = database.local_date()?;
    let transaction = database.immediate_transaction()?;
    database::daily_state::get_or_create(&transaction, user_id, &date)?;
    let summary = database::world_event::summary(&transaction, group_id, &date)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    Ok(summary)
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
    let left_system_name = engine::find_system(&left_cultivation.system_id)
        .map(|system| system.name())
        .ok_or_else(|| DatabaseError::InvalidData("unknown left cultivation system".into()))?;
    let right_system_name = engine::find_system(&right_cultivation.system_id)
        .map(|system| system.name())
        .ok_or_else(|| DatabaseError::InvalidData("unknown right cultivation system".into()))?;
    if group_id != 0 {
        database::group::ensure(&transaction, group_id)?;
    }
    let seed = stable_seed(
        &date,
        "duel",
        &format!("{group_id}:{user_id}:{target_id}"),
        identity::VERSION_SALT,
    );
    let left_profile = engine::build_combat_profile_with_state(
        &left,
        &left_cultivation.system_id,
        left_cultivation.realm_index,
        &date,
        Some(&left_daily),
    );
    let right_profile = engine::build_combat_profile_with_state(
        &right,
        &right_cultivation.system_id,
        right_cultivation.realm_index,
        &date,
        Some(&right_daily),
    );
    let result = simulate_combat(
        Combatant {
            player: &left_profile.player,
            skills: left_profile.skills,
        },
        Combatant {
            player: &right_profile.player,
            skills: right_profile.skills,
        },
        seed,
        30,
    );
    database::combat::record_duel(
        &transaction,
        group_id,
        database::combat::DuelParticipant {
            user_id,
            system_id: &left_cultivation.system_id,
            realm_index: left_cultivation.realm_index,
            power_before: left_profile.power,
            hp_before: left_profile.player.base_hp,
        },
        database::combat::DuelParticipant {
            user_id: target_id,
            system_id: &right_cultivation.system_id,
            realm_index: right_cultivation.realm_index,
            power_before: right_profile.power,
            hp_before: right_profile.player.base_hp,
        },
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

    let image = root
        .join(identity::DATA_DIRECTORY)
        .join("battles")
        .join(format!("{group_id}-{user_id}-{target_id}.gif"));
    let winner_name = if result.winner_id == left_profile.player.user_id {
        &left_profile.player.display_name
    } else {
        &right_profile.player.display_name
    };
    let report = battle_report(
        &result,
        &left_profile.player,
        &right_profile.player,
        winner_name,
    );
    let render_data = render::BattleRenderData {
        left: &left_profile,
        right: &right_profile,
        left_system: left_system_name,
        right_system: right_system_name,
        result: &result,
    };
    let concise = format!("决斗结束：{winner_name} 获胜，共 {} 回合。", result.rounds);
    let completion = world_completion_message(event_completed);
    Ok(match render::battle(root, &render_data, &image) {
        Ok(()) if show_report => {
            format!("{report}\n[CQ:image,file={}]", image.display()) + completion
        }
        Ok(()) => format!("[CQ:image,file={}]", image.display()) + completion,
        Err(error) => {
            eprintln!("[Luo Realm] battle rendering failed: {error}");
            (if show_report { report } else { concise }) + completion
        }
    })
}

fn world_completion_message(completed: bool) -> &'static str {
    if completed {
        "\n群世界事件已完成，奖励已自动发放给今日贡献者。"
    } else {
        ""
    }
}

fn battle_report(
    result: &crate::core::CombatResult,
    left: &Player,
    right: &Player,
    winner_name: &str,
) -> String {
    let actions = result
        .frames
        .iter()
        .map(|frame| {
            let attacker_name = if frame.attacker_id == left.user_id {
                &left.display_name
            } else {
                &right.display_name
            };
            format!(
                "R{} {}施展{}，造成{}{}伤害（{} / {}）",
                frame.round,
                attacker_name,
                frame.skill,
                if frame.critical { "暴击 " } else { "" },
                frame.damage,
                frame.left_hp,
                frame.right_hp
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "决斗结束：{winner_name} 获胜，共 {} 回合。\n{actions}",
        result.rounds
    )
}
