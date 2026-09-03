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

/// 中文体系名到体系标识的别名表；英文标识始终可用。
const SYSTEM_ALIASES: &[(&str, &str)] = &[
    ("修真", "orthodox"),
    ("修仙", "orthodox"),
    ("剑修", "sword"),
    ("剑", "sword"),
    ("体修", "body"),
    ("武修", "body"),
    ("法修", "mage"),
    ("魔法师", "mage"),
    ("灵修", "soul"),
    ("魂师", "soul"),
    ("气修", "qi"),
    ("斗气", "qi"),
    ("血魔", "blood_demon"),
    ("血魔邪修", "blood_demon"),
    ("邪修", "blood_demon"),
    ("阵修", "formation"),
    ("阵法师", "formation"),
    ("丹器", "alchemy_artifact"),
    ("丹器修", "alchemy_artifact"),
    ("炼丹", "alchemy_artifact"),
    ("炼器", "alchemy_artifact"),
    ("召唤", "summoner"),
    ("召唤流", "summoner"),
    ("巫师", "summoner"),
    ("音修", "music"),
    ("乐师", "music"),
];

/// 把用户输入解析为体系标识：先按英文标识匹配，再按中文别名匹配。
pub fn resolve_system(input: &str) -> Option<String> {
    if engine::find_system(input).is_some() {
        return Some(input.to_owned());
    }
    SYSTEM_ALIASES
        .iter()
        .find(|(alias, _)| *alias == input)
        .map(|(_, id)| (*id).to_owned())
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
    let Some(system_input) = arguments.first().copied() else {
        return Ok("请发送“选择体系 <体系名称>”，可先发送“体系”查看。".into());
    };
    let Some(system_id) = resolve_system(system_input) else {
        return Ok("未知修行体系，请发送“体系”查看。".into());
    };
    let system = engine::find_system(&system_id)
        .ok_or_else(|| DatabaseError::InvalidData("unknown player cultivation system".into()))?;

    let system_name = system.name();
    let transaction = database.immediate_transaction()?;
    let activated = database::player::activate_system(&transaction, user_id, &system_id)?;
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

#[cfg(test)]
mod tests {
    use super::resolve_system;

    #[test]
    fn chinese_names_resolve_to_system_ids() {
        assert_eq!(resolve_system("剑修").as_deref(), Some("sword"));
        assert_eq!(resolve_system("血魔").as_deref(), Some("blood_demon"));
        assert_eq!(
            resolve_system("丹器修").as_deref(),
            Some("alchemy_artifact")
        );
        assert_eq!(resolve_system("召唤流").as_deref(), Some("summoner"));
    }

    #[test]
    fn english_ids_still_resolve() {
        assert_eq!(resolve_system("sword").as_deref(), Some("sword"));
        assert_eq!(resolve_system("music").as_deref(), Some("music"));
    }

    #[test]
    fn unknown_input_is_rejected() {
        assert_eq!(resolve_system("剑圣"), None);
        assert_eq!(resolve_system(""), None);
    }
}
