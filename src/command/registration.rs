use crate::{
    cultivation,
    database::{self, Database, DatabaseError},
    engine,
};

pub fn system_catalog() -> String {
    cultivation::registered_systems()
        .into_iter()
        .map(|system| format!("{}({})", system.name(), system.id()))
        .collect::<Vec<_>>()
        .join("、")
}

pub fn register(
    database: &mut Database,
    user_id: u64,
    arguments: &[&str],
) -> Result<String, DatabaseError> {
    if arguments.is_empty() {
        return Ok("请发送“注册 <名称>”创建角色，名称限 1 至 20 个字符。".into());
    }

    let display_name = arguments.join(" ");
    let transaction = database.immediate_transaction()?;
    let result = match database::player::register(&transaction, user_id, &display_name) {
        Ok(result) => result,
        Err(DatabaseError::InvalidData(_)) => {
            return Ok("角色名称须为 1 至 20 个可见字符。".into());
        }
        Err(error) => return Err(error),
    };
    transaction.commit().map_err(DatabaseError::from_sqlite)?;

    Ok(match result {
        database::player::RegisterResult::Created => format!(
            "角色“{display_name}”已登记。请选择修行体系：\n{}\n发送“选择体系 <体系标识>”完成注册。",
            system_catalog()
        ),
        database::player::RegisterResult::PendingSystem => {
            "角色已经登记，但尚未确定体系。请发送“体系”查看并选择。".into()
        }
        database::player::RegisterResult::AlreadyActive => "角色已经完成注册。".into(),
        database::player::RegisterResult::Unavailable => "当前角色已被停用，请联系管理员。".into(),
    })
}

pub fn select_system(
    database: &mut Database,
    user_id: u64,
    arguments: &[&str],
) -> Result<String, DatabaseError> {
    let Some(system_id) = arguments.first().copied() else {
        return Ok("请发送“选择体系 <体系标识>”，可先发送“体系”查看。".into());
    };
    let Some(system) = engine::find_system(system_id) else {
        return Ok("未知修行体系，请发送“体系”查看。".into());
    };

    let system_name = system.name();
    let transaction = database.immediate_transaction()?;
    let activated = database::player::activate_system(&transaction, user_id, system_id)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)?;

    Ok(if activated {
        format!("已踏入{system_name}之路。体系一经确定，当前版本不可更改。")
    } else {
        "角色已经确定修行体系，不能重复选择。".into()
    })
}

pub fn rename(
    database: &mut Database,
    user_id: u64,
    arguments: &[&str],
) -> Result<String, DatabaseError> {
    if arguments.is_empty() {
        return Ok("请发送“改名 <新名称>”，名称限 1 至 20 个字符。".into());
    }

    let transaction = database.immediate_transaction()?;
    let display_name = match database::player::rename(&transaction, user_id, &arguments.join(" ")) {
        Ok(display_name) => display_name,
        Err(DatabaseError::InvalidData(_)) => {
            return Ok("角色名称须为 1 至 20 个可见字符。".into());
        }
        Err(error) => return Err(error),
    };
    transaction.commit().map_err(DatabaseError::from_sqlite)?;
    Ok(format!("角色名称已修改为：{display_name}"))
}
