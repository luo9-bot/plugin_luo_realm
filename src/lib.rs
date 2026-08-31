pub mod admin;
mod command;
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

#[derive(Clone, Copy)]
pub enum IncomingContext {
    Group { group_id: u64 },
    Private,
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

    command::handle_message(database, root, group_id, user_id, message, &config.command)
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
    command::handle_message(database, root, group_id, user_id, message, command_config)
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
    if policy.snapshot().admin.enabled {
        let _admin_thread = admin::start(root.clone(), paths::database_path(), policy.clone());
    }
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
