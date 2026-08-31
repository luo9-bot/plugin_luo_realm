pub mod config;
pub mod core;
pub mod cultivation;
pub mod database;
pub mod engine;
pub mod identity;

mod paths;
mod render;

use std::{path::Path, thread, time::Duration};

use config::{CommandConfig, RuntimeConfig, RuntimePolicy};
use database::{Database, DatabaseError};
use luo9_sdk::{
    bus::Bus,
    payload::{BusPayload, MsgType},
    send,
};

use crate::core::{simulate_combat, stable_seed};

#[derive(Clone, Copy)]
pub enum IncomingContext {
    Group { group_id: u64 },
    Private,
}

#[derive(Clone, Copy)]
enum CommandFeature {
    General,
    Event,
    Combat,
}

impl CommandFeature {
    fn from_message(message: &str, command: &CommandConfig) -> Self {
        let text = command.command_text(message).unwrap_or_default();
        match text.split_whitespace().next() {
            Some("每日事件" | "事件" | "event") => Self::Event,
            Some("决斗" | "duel") => Self::Combat,
            _ => Self::General,
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Event => "event",
            Self::Combat => "combat",
        }
    }
}

pub fn route_message(
    database: &mut Database,
    root: &Path,
    policy: &RuntimePolicy,
    context: IncomingContext,
    user_id: u64,
    message: &str,
) -> Result<Option<String>, DatabaseError> {
    let config = policy.snapshot();
    let group_id = match context {
        IncomingContext::Group { group_id } => {
            if !database::group::is_enabled(database.connection(), group_id)? {
                return Ok(None);
            }
            let feature = CommandFeature::from_message(message, &config.command);
            if !database::group::feature_enabled(database.connection(), group_id, feature.code())? {
                return Ok(None);
            }
            group_id
        }
        IncomingContext::Private => {
            if !config.admin.admin_ids.contains(&user_id) {
                return Ok(None);
            }
            0
        }
    };

    handle_message_with_config(database, root, group_id, user_id, message, &config.command)
}

pub fn handle_message(
    database: &mut Database,
    root: &Path,
    group_id: u64,
    user_id: u64,
    message: &str,
) -> Result<Option<String>, DatabaseError> {
    handle_message_with_config(
        database,
        root,
        group_id,
        user_id,
        message,
        &CommandConfig::default(),
    )
}

pub fn handle_message_with_config(
    database: &mut Database,
    root: &Path,
    group_id: u64,
    user_id: u64,
    message: &str,
    command_config: &CommandConfig,
) -> Result<Option<String>, DatabaseError> {
    let Some(text) = command_config.command_text(message) else {
        return Ok(None);
    };
    let arguments = text.split_whitespace().collect::<Vec<_>>();
    let Some(command) = arguments.first().copied() else {
        return Ok(None);
    };

    match command {
        "菜单" | "帮助" | "help" => Ok(Some(format!(
            "{}：签到 / 状态 / 体系 / 选择体系 / 战力 / 每日事件 / 决斗 / 排行 / 改名",
            identity::PRODUCT_NAME
        ))),
        "体系" | "修行" | "cultivation" => {
            let systems = cultivation::registered_systems()
                .into_iter()
                .map(|system| format!("{}({})", system.name(), system.id()))
                .collect::<Vec<_>>();
            Ok(Some(format!("可选修行体系：{}", systems.join("、"))))
        }
        "选择体系" if arguments.len() >= 2 => {
            let system_id = arguments[1];
            if engine::find_system(system_id).is_none() {
                return Ok(Some("未知修行体系，请发送“体系”查看。".into()));
            }
            let transaction = database.immediate_transaction()?;
            database::player::find_or_create(&transaction, user_id)?;
            database::cultivation::select_system(&transaction, user_id, system_id)?;
            transaction.commit().map_err(DatabaseError::from_sqlite)?;
            Ok(Some(format!("已选择修行体系：{system_id}")))
        }
        "战力" | "power" => {
            let date = database.local_date()?;
            let transaction = database.immediate_transaction()?;
            let player = database::player::find_or_create(&transaction, user_id)?;
            let cultivation = database::cultivation::get(&transaction, user_id)?;
            transaction.commit().map_err(DatabaseError::from_sqlite)?;
            let profile = engine::build_combat_profile(
                &player,
                &cultivation.system_id,
                cultivation.realm_index,
                &date,
            );
            Ok(Some(format!(
                "当前战力：{:.0}（{}，境界 {}）",
                profile.power,
                cultivation.system_id,
                cultivation.realm_index + 1
            )))
        }
        "状态" | "属性" | "profile" | "查询" => {
            let transaction = database.immediate_transaction()?;
            let player = database::player::find_or_create(&transaction, user_id)?;
            transaction.commit().map_err(DatabaseError::from_sqlite)?;
            let image = root
                .join(identity::DATA_DIRECTORY)
                .join("cards")
                .join(format!("{user_id}.png"));
            render::profile(&player, &image)
                .map_err(|error| DatabaseError::Migration(error.to_string()))?;
            Ok(Some(format!(
                "{} Lv.{}\nHP {} 攻击 {} 防御 {}\n金币 {} 胜/负 {}/{}\n[CQ:image,file={}]",
                player.display_name,
                player.level,
                player.base_hp,
                player.base_attack,
                player.base_defense,
                player.coins,
                player.wins,
                player.losses,
                image.display()
            )))
        }
        "签到" | "刻印" | "checkin" => {
            let date = database.local_date()?;
            let transaction = database.immediate_transaction()?;
            database::player::find_or_create(&transaction, user_id)?;
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
                    format!(
                        "签到成功！连续 {streak} 天，金币 +100，刻印 +2。当前金币 {}。",
                        reward.balance_after
                    )
                }
                database::activity::CheckInResult::AlreadyCompleted => "今天已经签到过了。".into(),
            };
            transaction.commit().map_err(DatabaseError::from_sqlite)?;
            Ok(Some(reply))
        }
        "每日事件" | "事件" | "event" => {
            let date = database.local_date()?;
            let seed = stable_seed(
                &date,
                "event",
                &format!("{group_id}:{user_id}"),
                identity::VERSION_SALT,
            );
            let definition = engine::event::daily_event(seed);
            let transaction = database.immediate_transaction()?;
            database::player::find_or_create(&transaction, user_id)?;
            let persisted = database::destiny::daily_event(
                &transaction,
                user_id,
                &date,
                definition,
                &seed.to_string(),
            )?;
            transaction.commit().map_err(DatabaseError::from_sqlite)?;
            Ok(Some(format!("今日机缘：{persisted}")))
        }
        "排行" | "ranking" => {
            let entries = database::group::ranking(database.connection(), 8)?;
            if entries.is_empty() {
                Ok(Some("暂无排行数据。".into()))
            } else {
                Ok(Some(
                    entries
                        .into_iter()
                        .enumerate()
                        .map(|(index, entry)| format!("{}. {entry}", index + 1))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ))
            }
        }
        "改名" | "name" if arguments.len() >= 2 => {
            let display_name = arguments[1..]
                .join(" ")
                .chars()
                .take(20)
                .collect::<String>();
            let transaction = database.immediate_transaction()?;
            database::player::find_or_create(&transaction, user_id)?;
            database::player::rename(&transaction, user_id, &display_name)?;
            transaction.commit().map_err(DatabaseError::from_sqlite)?;
            Ok(Some(format!("角色名称已修改为：{display_name}")))
        }
        "决斗" | "duel" if arguments.len() >= 2 => {
            let target_id = arguments[1]
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<String>()
                .parse::<u64>()
                .ok();
            let Some(target_id) = target_id.filter(|target| *target != user_id) else {
                return Ok(Some("请指定另一位有效玩家。".into()));
            };
            let date = database.local_date()?;
            let transaction = database.immediate_transaction()?;
            let left = database::player::find_or_create(&transaction, user_id)?;
            let right = database::player::find_or_create(&transaction, target_id)?;
            let left_cultivation = database::cultivation::get(&transaction, user_id)?;
            let right_cultivation = database::cultivation::get(&transaction, target_id)?;
            if group_id != 0 {
                database::group::ensure(&transaction, group_id)?;
            }
            let seed = stable_seed(
                &date,
                "duel",
                &format!("{group_id}:{user_id}:{target_id}"),
                identity::VERSION_SALT,
            );
            let left_profile = engine::build_combat_profile(
                &left,
                &left_cultivation.system_id,
                left_cultivation.realm_index,
                &date,
            );
            let right_profile = engine::build_combat_profile(
                &right,
                &right_cultivation.system_id,
                right_cultivation.realm_index,
                &date,
            );
            let result = simulate_combat(&left_profile.player, &right_profile.player, seed, 30);
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
            transaction.commit().map_err(DatabaseError::from_sqlite)?;
            let image = root
                .join(identity::DATA_DIRECTORY)
                .join("battles")
                .join(format!("{group_id}-{user_id}-{target_id}.gif"));
            render::battle(&result, &image)
                .map_err(|error| DatabaseError::Migration(error.to_string()))?;
            Ok(Some(format!(
                "决斗结束：{} 获胜，共 {} 回合。\n[CQ:image,file={}]",
                result.winner_id,
                result.rounds,
                image.display()
            )))
        }
        _ => Ok(None),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn plugin_main() {
    let root = paths::plugin_root();
    let config = match RuntimeConfig::load(&root) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("[Luo Realm] config startup failed: {error}");
            return;
        }
    };
    let policy = RuntimePolicy::new(config);
    let mut database = match Database::open(paths::database_path()) {
        Ok(database) => database,
        Err(error) => {
            eprintln!("[Luo Realm] database startup failed: {error}");
            return;
        }
    };
    let topic = Bus::topic("luo9_message");
    let Ok(subscriber) = topic.subscribe() else {
        return;
    };

    loop {
        if let Some(json) = topic.pop(subscriber)
            && let Some(BusPayload::Message(message)) = BusPayload::parse(&json)
        {
            let group_id = message.group_id.unwrap_or(0);
            let context = if message.message_type == MsgType::Group {
                IncomingContext::Group { group_id }
            } else {
                IncomingContext::Private
            };
            match route_message(
                &mut database,
                &root,
                &policy,
                context,
                message.user_id,
                &message.message,
            ) {
                Ok(Some(reply)) if message.message_type == MsgType::Group => {
                    let _ = send::send_group_msg(group_id, &reply);
                }
                Ok(Some(reply)) => {
                    let _ = send::send_private_msg(message.user_id, &reply);
                }
                Ok(None) => {}
                Err(error) => {
                    eprintln!("[Luo Realm] command failed: {error}");
                }
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
}
