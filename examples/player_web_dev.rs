//! 玩家网页的本地联调入口。
//!
//! 在插件仓库根目录运行（会使用并写入 `./data/luo_realm` 本地数据）：
//!
//! ```powershell
//! cargo run --example player_web_dev
//! ```
//!
//! 示例直接调用票据签发逻辑打印一张测试票据链接；正式插件中同一条路径由
//! 群聊命令 `主页` 触发。

use std::{thread, time::Duration};

use luo_realm::{
    admin,
    config::{RuntimeConfig, RuntimePolicy},
    database::{self, Database},
    identity,
};

fn main() -> std::io::Result<()> {
    let root = std::env::current_dir()?;
    let data_directory = root.join(identity::DATA_DIRECTORY);
    let mut config =
        RuntimeConfig::load(&root).map_err(|error| std::io::Error::other(error.to_string()))?;
    config.player_web.enabled = true;
    config.player_web.base_url = "http://127.0.0.1:18780/player".into();
    config.admin.enabled = true;
    config.admin.port = 18_780;
    config
        .save(&root)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let policy = RuntimePolicy::new(config.clone());

    let database_path = data_directory.join(identity::DATABASE_FILE);
    let ticket_url = {
        let mut database = Database::open(&database_path).expect("open local database");
        seed_demo_player(&mut database);
        issue_demo_ticket(&mut database)
    };

    let _server = admin::start(root, database_path, policy);
    println!("管理后台  http://127.0.0.1:18780/");
    println!("玩家页面  {ticket_url}");
    println!("按 Ctrl+C 停止。");
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

/// 准备一个可演示的本地角色（存在则跳过资产注入）。
fn seed_demo_player(database: &mut Database) {
    let date = database.local_date().expect("local date");
    let transaction = database.immediate_transaction().expect("transaction");
    transaction
        .execute(
            "INSERT INTO players(player_id, status, registration_state, created_at, updated_at)
             VALUES(10001, 'active', 'active', strftime('%s','now'), strftime('%s','now'))
             ON CONFLICT(player_id) DO NOTHING",
            [],
        )
        .expect("seed player");
    transaction
        .execute(
            "INSERT INTO player_profiles(player_id, display_name)
             VALUES(10001, '演示剑修')
             ON CONFLICT(player_id) DO UPDATE SET display_name=excluded.display_name",
            [],
        )
        .expect("seed profile");
    transaction
        .execute(
            "INSERT INTO player_cultivation(player_id, system_id, realm_index, progress, updated_at)
             VALUES(10001, 'sword', 3, 320, strftime('%s','now'))
             ON CONFLICT(player_id) DO UPDATE SET system_id=excluded.system_id,
                 realm_index=excluded.realm_index",
            [],
        )
        .expect("seed cultivation");
    if database::wallet::balance(&transaction, 10001, "coins").expect("balance") == 0 {
        database::wallet::credit(
            &transaction,
            10001,
            "coins",
            1_200,
            "demo_seed",
            "demo:coins",
        )
        .expect("seed coins");
        database::wallet::credit(&transaction, 10001, "marks", 40, "demo_seed", "demo:marks")
            .expect("seed marks");
        let item = database::inventory::add_item(&transaction, 10001, "iron_sword", 1, 0)
            .expect("seed item");
        database::inventory::equip(
            &transaction,
            10001,
            item,
            luo_realm::combat::EquipmentSlot::MainHand,
        )
        .expect("seed equip");
        database::skills::ensure_unlocked(&transaction, 10001, "sword", 3).expect("seed skills");
        database::activity::check_in(&transaction, 10001, &date).expect("seed check-in");
    }
    transaction.commit().expect("commit seed");
}

/// 以管理员 Token 摘要为签名密钥签发一张演示票据，返回完整页面链接。
fn issue_demo_ticket(database: &mut Database) -> String {
    let token_path = std::path::Path::new(identity::DATA_DIRECTORY).join("admin.token");
    let token =
        luo_realm::admin::auth::AdminToken::load_or_create(&token_path).expect("load admin token");
    let key = token.signing_key();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64;
    let transaction = database.immediate_transaction().expect("transaction");
    let ticket = luo_realm::player_web::ticket::issue(
        &transaction,
        10001,
        luo_realm::player_web::session::SCOPE_PROFILE_READ,
        &key,
        now,
        600,
    )
    .expect("issue ticket");
    transaction.commit().expect("commit");
    format!("http://127.0.0.1:18780/player/?ticket={}", ticket.token)
}
