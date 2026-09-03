pub mod admin;
pub mod combat;
mod command;
pub mod config;
pub mod core;
pub mod cultivation;
pub mod database;
pub mod domain;
pub mod engine;
pub mod equipment;
pub mod game;
pub mod identity;
pub mod player_web;

mod paths;
mod render;

use std::{
    panic::{self, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    thread,
    time::Duration,
};

use config::{CommandConfig, RuntimeConfig, RuntimePolicy};
use database::{Database, DatabaseError};
use luo9_sdk::{
    bus::Bus,
    payload::{BusPayload, MsgType},
    send,
};

#[derive(Clone, Copy)]
pub enum IncomingContext {
    Group { group_id: u64 },
    Private,
}

const MESSAGE_WORKER_COUNT: usize = 4;
const MESSAGE_QUEUE_CAPACITY: usize = 256;

struct MessageWork {
    context: IncomingContext,
    group_id: u64,
    user_id: u64,
    message: String,
    is_group: bool,
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
            let feature = command::feature_code(message, &config.command);
            if !database::group::feature_enabled(database.connection(), group_id, feature)? {
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

    command::handle_message(database, root, group_id, user_id, message, &config)
}

pub fn handle_message(
    database: &mut Database,
    root: &Path,
    group_id: u64,
    user_id: u64,
    message: &str,
) -> Result<Option<String>, DatabaseError> {
    command::handle_message(
        database,
        root,
        group_id,
        user_id,
        message,
        &RuntimeConfig::default(),
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
    let config = RuntimeConfig {
        command: command_config.clone(),
        ..RuntimeConfig::default()
    };
    command::handle_message(database, root, group_id, user_id, message, &config)
}

#[unsafe(no_mangle)]
pub extern "C" fn plugin_main() {
    let root = paths::plugin_root();
    if let Err(error) = paths::migrate_legacy_layout() {
        eprintln!("[Luo Realm] legacy layout migration failed: {error}");
    }
    if let Err(error) = render::assets::recover_asset_tree(&root) {
        eprintln!("[Luo Realm] asset recovery failed: {error}");
    }
    if let Err(error) = admin::recover_asset_import(&root) {
        eprintln!("[Luo Realm] asset bundle recovery failed: {error}");
    }
    let database_path = paths::database_path();
    if let Err(error) = admin::recover_database_import(&root, &database_path) {
        eprintln!("[Luo Realm] database import recovery failed: {error}");
        return;
    }
    let config = match RuntimeConfig::load(&root) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("[Luo Realm] config startup failed: {error}");
            return;
        }
    };
    let policy = RuntimePolicy::new(config);
    let database = match Database::open(&database_path) {
        Ok(database) => database,
        Err(error) => {
            eprintln!("[Luo Realm] database startup failed: {error}");
            return;
        }
    };
    if policy.snapshot().admin.enabled {
        let _admin_thread = admin::start(root.clone(), database_path.clone(), policy.clone());
    }
    let workers = match start_message_workers(&root, &database_path, &policy) {
        Ok(workers) => workers,
        Err(error) => {
            eprintln!("[Luo Realm] message workers failed to start: {error}");
            return;
        }
    };
    drop(database);
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
            let work = MessageWork {
                context,
                group_id,
                user_id: message.user_id,
                message: message.message,
                is_group: message.message_type == MsgType::Group,
            };
            let worker_index = (work.user_id % workers.len() as u64) as usize;
            match workers[worker_index].try_send(work) {
                Ok(()) => {}
                Err(TrySendError::Full(work)) => reply_worker_busy(&work),
                Err(TrySendError::Disconnected(_)) => {
                    eprintln!("[Luo Realm] message worker is unavailable");
                }
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
}

fn reply_worker_busy(work: &MessageWork) {
    const MESSAGE: &str = "当前命令队列繁忙，请稍后重试。";

    if work.is_group {
        let _ = send::send_group_msg(work.group_id, MESSAGE);
    } else {
        let _ = send::send_private_msg(work.user_id, MESSAGE);
    }
}

fn start_message_workers(
    root: &Path,
    database_path: &Path,
    policy: &RuntimePolicy,
) -> std::io::Result<Vec<SyncSender<MessageWork>>> {
    (0..MESSAGE_WORKER_COUNT)
        .map(|worker_index| {
            let (sender, receiver) = sync_channel(MESSAGE_QUEUE_CAPACITY);
            let worker_root = root.to_path_buf();
            let worker_database_path = database_path.to_path_buf();
            let worker_policy = policy.clone();
            thread::Builder::new()
                .name(format!("luo-realm-message-{worker_index}"))
                .spawn(move || {
                    run_message_worker(receiver, worker_root, worker_database_path, worker_policy);
                })
                .map(|_| sender)
        })
        .collect()
}

fn run_message_worker(
    receiver: Receiver<MessageWork>,
    root: PathBuf,
    database_path: PathBuf,
    policy: RuntimePolicy,
) {
    let mut database = match Database::open_request(&database_path) {
        Ok(database) => database,
        Err(error) => {
            eprintln!("[Luo Realm] message worker database failed: {error}");
            return;
        }
    };
    receiver.iter().for_each(|work| {
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            route_message(
                &mut database,
                &root,
                &policy,
                work.context,
                work.user_id,
                &work.message,
            )
        }));
        match result {
            Ok(Ok(Some(reply))) if work.is_group => {
                let _ = send::send_group_msg(work.group_id, &reply);
            }
            Ok(Ok(Some(reply))) => {
                let _ = send::send_private_msg(work.user_id, &reply);
            }
            Ok(Ok(None)) => {}
            Ok(Err(error)) => eprintln!("[Luo Realm] command failed: {error}"),
            Err(_) => eprintln!("[Luo Realm] command panicked and was contained"),
        }
    });
}
