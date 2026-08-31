use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::database::{Database, DatabaseError, DatabaseResult, admin};

const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";
const IMPORT_PENDING_FILE: &str = "database-import.pending";

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("invalid SQLite snapshot")]
    InvalidSnapshot,
    #[error("snapshot storage failed: {0}")]
    Storage(#[from] std::io::Error),
    #[error("database transfer failed: {0}")]
    Database(#[from] DatabaseError),
}

pub struct ImportResult {
    pub backup_name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ImportState {
    Prepared,
    Committed,
}

#[derive(Debug, Deserialize, Serialize)]
struct ImportPending {
    state: ImportState,
    backup_name: String,
}

pub fn export_database(
    plugin_root: &Path,
    database_path: &Path,
) -> Result<(String, Vec<u8>), TransferError> {
    let directory = transfer_directory(plugin_root, "exports")?;
    let name = timestamped_name("luo-realm-export");
    let path = directory.join(&name);
    let database = Database::open_request(database_path)?;
    database.backup_to(&path)?;
    let bytes = fs::read(&path)?;
    let _ = fs::remove_file(path);
    Ok((name, bytes))
}

pub fn import_database(
    plugin_root: &Path,
    database_path: &Path,
    bytes: &[u8],
    reason: &str,
) -> Result<ImportResult, TransferError> {
    if !bytes.starts_with(SQLITE_HEADER) {
        return Err(TransferError::InvalidSnapshot);
    }
    let imports = transfer_directory(plugin_root, "imports")?;
    let staged_path = imports.join(timestamped_name("luo-realm-import"));
    crate::render::assets::atomic_write(&staged_path, bytes)?;

    let result = restore_staged(plugin_root, database_path, &staged_path, reason);
    let _ = fs::remove_file(staged_path);
    result
}

/// Resolves an interrupted database import before the main connection opens.
/// A prepared import is rolled back; a committed import only needs marker cleanup.
pub fn recover_database_import(
    plugin_root: &Path,
    database_path: &Path,
) -> Result<(), TransferError> {
    let pending_path = pending_path(plugin_root);
    crate::render::assets::recover_atomic_write(&pending_path)?;
    if !pending_path.exists() {
        return Ok(());
    }

    let pending = read_pending(&pending_path)?;
    validate_backup_name(&pending.backup_name)?;
    if matches!(pending.state, ImportState::Prepared) {
        let backup_path = transfer_directory(plugin_root, "backups")?.join(&pending.backup_name);
        let mut database = Database::open_request(database_path)?;
        database.restore_from(backup_path)?;
    }
    remove_pending(&pending_path)?;
    Ok(())
}

fn restore_staged(
    plugin_root: &Path,
    database_path: &Path,
    staged_path: &Path,
    reason: &str,
) -> Result<ImportResult, TransferError> {
    let mut database = Database::open_request(database_path)?;
    database.validate_snapshot(staged_path)?;
    let audit_history = read_audit_history(database.connection())?;

    let backups = transfer_directory(plugin_root, "backups")?;
    let backup_name = timestamped_name("luo-realm-before-import");
    let backup_path = backups.join(&backup_name);
    database.backup_to(&backup_path)?;

    let pending_path = pending_path(plugin_root);
    write_pending(&pending_path, ImportState::Prepared, &backup_name)?;

    let import_result = database
        .restore_from(staged_path)
        .and_then(|()| finalize_import(&mut database, &audit_history, reason, &backup_name));
    if let Err(import_error) = import_result {
        if let Err(rollback_error) = database.restore_from(&backup_path) {
            return Err(TransferError::Database(DatabaseError::Corrupt(format!(
                "import failed ({import_error}); rollback failed ({rollback_error})"
            ))));
        }
        if let Err(cleanup_error) = remove_pending(&pending_path) {
            eprintln!(
                "[Luo Realm] database import rolled back, but pending marker cleanup failed: {cleanup_error}"
            );
        }
        return Err(TransferError::Database(import_error));
    }

    if let Err(marker_error) = write_pending(&pending_path, ImportState::Committed, &backup_name) {
        if let Err(rollback_error) = database.restore_from(&backup_path) {
            return Err(TransferError::Database(DatabaseError::Corrupt(format!(
                "commit marker failed ({marker_error}); rollback failed ({rollback_error})"
            ))));
        }
        return Err(marker_error);
    }
    if let Err(cleanup_error) = remove_pending(&pending_path) {
        eprintln!(
            "[Luo Realm] database import committed, pending marker will be cleaned on restart: {cleanup_error}"
        );
    }
    Ok(ImportResult { backup_name })
}

fn pending_path(plugin_root: &Path) -> PathBuf {
    plugin_root
        .join(crate::identity::DATA_DIRECTORY)
        .join(IMPORT_PENDING_FILE)
}

fn write_pending(path: &Path, state: ImportState, backup_name: &str) -> Result<(), TransferError> {
    let bytes = serde_json::to_vec(&ImportPending {
        state,
        backup_name: backup_name.to_owned(),
    })
    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    crate::render::assets::atomic_write(path, &bytes)?;
    Ok(())
}

fn read_pending(path: &Path) -> Result<ImportPending, TransferError> {
    serde_json::from_slice(&fs::read(path)?).map_err(|error| {
        TransferError::Storage(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    })
}

fn validate_backup_name(name: &str) -> Result<(), TransferError> {
    let path = Path::new(name);
    let is_single_file = path
        .file_name()
        .is_some_and(|file_name| file_name == path.as_os_str());
    if is_single_file && name.starts_with("luo-realm-before-import-") && name.ends_with(".sqlite3")
    {
        Ok(())
    } else {
        Err(TransferError::InvalidSnapshot)
    }
}

fn remove_pending(path: &Path) -> Result<(), TransferError> {
    fs::remove_file(path)?;
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

struct AuditRecord {
    audit_id: i64,
    operator: String,
    action_code: String,
    target_type: String,
    target_id: String,
    reason: String,
    before_json: Option<String>,
    after_json: Option<String>,
    result: String,
    created_at: i64,
}

fn read_audit_history(connection: &Connection) -> DatabaseResult<Vec<AuditRecord>> {
    let mut statement = connection
        .prepare(
            "SELECT audit_id, operator, action_code, target_type, target_id, reason,
                    before_json, after_json, result, created_at
             FROM admin_audit_log ORDER BY audit_id",
        )
        .map_err(DatabaseError::from_sqlite)?;
    statement
        .query_map([], |row| {
            Ok(AuditRecord {
                audit_id: row.get(0)?,
                operator: row.get(1)?,
                action_code: row.get(2)?,
                target_type: row.get(3)?,
                target_id: row.get(4)?,
                reason: row.get(5)?,
                before_json: row.get(6)?,
                after_json: row.get(7)?,
                result: row.get(8)?,
                created_at: row.get(9)?,
            })
        })
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)
}

fn finalize_import(
    database: &mut Database,
    audit_history: &[AuditRecord],
    reason: &str,
    backup_name: &str,
) -> DatabaseResult<()> {
    let transaction = database.immediate_transaction()?;
    transaction
        .execute("DELETE FROM admin_audit_log", [])
        .map_err(DatabaseError::from_sqlite)?;
    audit_history
        .iter()
        .try_for_each(|record| insert_audit_record(&transaction, record))?;
    admin::audit_success(
        &transaction,
        admin::AuditEntry {
            operator: "web",
            action: "database.import",
            target_type: "database",
            target_id: "current",
            reason,
            before: Some(serde_json::json!({"backup": backup_name})),
            after: Some(serde_json::json!({"restored": true})),
        },
    )?;
    admin::overview(&transaction)?;
    admin::list_players(&transaction, "", 1, 1)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)
}

fn insert_audit_record(transaction: &Transaction<'_>, record: &AuditRecord) -> DatabaseResult<()> {
    transaction
        .execute(
            "INSERT INTO admin_audit_log(
                 audit_id, operator, action_code, target_type, target_id, reason,
                 before_json, after_json, result, created_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.audit_id,
                record.operator,
                record.action_code,
                record.target_type,
                record.target_id,
                record.reason,
                record.before_json,
                record.after_json,
                record.result,
                record.created_at,
            ],
        )
        .map(|_| ())
        .map_err(DatabaseError::from_sqlite)
}

fn transfer_directory(plugin_root: &Path, name: &str) -> Result<PathBuf, std::io::Error> {
    let directory = plugin_root.join(crate::identity::DATA_DIRECTORY).join(name);
    fs::create_dir_all(&directory)?;
    Ok(directory)
}

fn timestamped_name(prefix: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{prefix}-{timestamp}-{}.sqlite3", std::process::id())
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> std::io::Result<()> {
    fs::File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> std::io::Result<()> {
    Ok(())
}
