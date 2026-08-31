use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(unix)]
use std::fs::File;

use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior, backup::Backup};

use super::{
    error::{DatabaseError, DatabaseResult},
    migrations,
};

pub struct Database {
    connection: Connection,
    path: PathBuf,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> DatabaseResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| DatabaseError::Migration(error.to_string()))?;
        }

        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .map_err(DatabaseError::from_sqlite)?;
        configure(&connection)?;
        migrations::apply(&mut connection)?;
        check_integrity(&connection)?;

        Ok(Self {
            connection,
            path: path.to_path_buf(),
        })
    }

    pub fn open_request(path: impl AsRef<Path>) -> DatabaseResult<Self> {
        let path = path.as_ref();
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(DatabaseError::from_sqlite)?;
        configure(&connection)?;

        Ok(Self {
            connection,
            path: path.to_path_buf(),
        })
    }

    pub fn immediate_transaction(&mut self) -> DatabaseResult<Transaction<'_>> {
        self.connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(DatabaseError::from_sqlite)
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn local_date(&self) -> DatabaseResult<String> {
        self.connection
            .query_row("SELECT date('now', 'localtime')", [], |row| row.get(0))
            .map_err(DatabaseError::from_sqlite)
    }

    pub fn checkpoint_on_shutdown(&self) -> DatabaseResult<()> {
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(DatabaseError::from_sqlite)
    }

    pub fn backup_to(&self, destination: impl AsRef<Path>) -> DatabaseResult<()> {
        let destination = destination.as_ref();
        let temporary = destination.with_extension("sqlite3.tmp");
        let backup = destination.with_extension("sqlite3.bak");
        if temporary.exists() {
            fs::remove_file(&temporary)
                .map_err(|error| DatabaseError::Migration(error.to_string()))?;
        }
        let mut target = Connection::open(&temporary).map_err(DatabaseError::from_sqlite)?;
        {
            let backup =
                Backup::new(&self.connection, &mut target).map_err(DatabaseError::from_sqlite)?;
            backup
                .run_to_completion(128, Duration::from_millis(5), None)
                .map_err(DatabaseError::from_sqlite)?;
        }
        check_integrity(&target)?;
        target
            .close()
            .map_err(|(_, error)| DatabaseError::from_sqlite(error))?;
        if destination.exists() {
            if backup.exists() {
                fs::remove_file(&backup)
                    .map_err(|error| DatabaseError::Migration(error.to_string()))?;
            }
            fs::rename(destination, &backup)
                .map_err(|error| DatabaseError::Migration(error.to_string()))?;
        }
        if let Err(error) = fs::rename(&temporary, destination) {
            if backup.exists() {
                fs::rename(&backup, destination)
                    .map_err(|rollback| DatabaseError::Migration(rollback.to_string()))?;
            }
            return Err(DatabaseError::Migration(error.to_string()));
        }
        fs::File::open(destination)
            .and_then(|file| file.sync_all())
            .map_err(|error| DatabaseError::Migration(error.to_string()))?;
        if let Some(parent) = destination.parent() {
            sync_directory(parent).map_err(|error| DatabaseError::Migration(error.to_string()))?;
        }
        if backup.exists() {
            fs::remove_file(&backup)
                .map_err(|error| DatabaseError::Migration(error.to_string()))?;
            if let Some(parent) = destination.parent() {
                sync_directory(parent)
                    .map_err(|error| DatabaseError::Migration(error.to_string()))?;
            }
        }
        Ok(())
    }

    /// Validates that a snapshot is a complete Luo Realm database compatible
    /// with the currently running schema.
    pub fn validate_snapshot(&self, source: impl AsRef<Path>) -> DatabaseResult<()> {
        let source = open_snapshot(source.as_ref())?;
        check_integrity(&source)?;
        validate_snapshot_schema(&source)?;
        let source_version = schema_version(&source)?;
        if source_version != migrations::CURRENT_SCHEMA_VERSION {
            return Err(DatabaseError::InvalidData(format!(
                "snapshot schema {source_version} does not match {}",
                migrations::CURRENT_SCHEMA_VERSION
            )));
        }
        Ok(())
    }

    /// Restores a validated snapshot through SQLite's online backup API.
    /// Existing WAL connections remain valid because the destination database
    /// is updated by SQLite rather than replaced at the filesystem level.
    pub fn restore_from(&mut self, source: impl AsRef<Path>) -> DatabaseResult<()> {
        self.validate_snapshot(source.as_ref())?;
        let source = open_snapshot(source.as_ref())?;
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(DatabaseError::from_sqlite)?;
        {
            let backup =
                Backup::new(&source, &mut self.connection).map_err(DatabaseError::from_sqlite)?;
            backup
                .run_to_completion(128, Duration::from_millis(5), None)
                .map_err(DatabaseError::from_sqlite)?;
        }
        check_integrity(&self.connection)
    }

    pub fn schema_version(&self) -> DatabaseResult<i64> {
        schema_version(&self.connection)
    }

    pub fn table_exists(&self, table: &str) -> DatabaseResult<bool> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |row| row.get(0),
            )
            .map_err(DatabaseError::from_sqlite)
    }
}

fn open_snapshot(path: &Path) -> DatabaseResult<Connection> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(DatabaseError::from_sqlite)?;
    connection
        .busy_timeout(Duration::from_millis(5_000))
        .map_err(DatabaseError::from_sqlite)?;
    connection
        .execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;")
        .map_err(DatabaseError::from_sqlite)?;
    Ok(connection)
}

fn schema_version(connection: &Connection) -> DatabaseResult<i64> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)
}

fn validate_snapshot_schema(connection: &Connection) -> DatabaseResult<()> {
    let mut expected = Connection::open_in_memory().map_err(DatabaseError::from_sqlite)?;
    expected
        .execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(DatabaseError::from_sqlite)?;
    migrations::apply(&mut expected)?;
    if schema_signature(connection)? != schema_signature(&expected)? {
        return Err(DatabaseError::InvalidData(
            "snapshot schema does not match this Luo Realm build".into(),
        ));
    }
    Ok(())
}

fn schema_signature(
    connection: &Connection,
) -> DatabaseResult<Vec<(String, String, String, String)>> {
    let mut statement = connection
        .prepare(
            "SELECT type, name, tbl_name, COALESCE(sql, '')
             FROM sqlite_master
             WHERE name NOT LIKE 'sqlite_%'
             ORDER BY type, name",
        )
        .map_err(DatabaseError::from_sqlite)?;
    statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)
}

fn configure(connection: &Connection) -> DatabaseResult<()> {
    connection
        .busy_timeout(Duration::from_millis(5_000))
        .map_err(DatabaseError::from_sqlite)?;
    connection
        .execute_batch(
            "PRAGMA foreign_keys=ON;
             PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             PRAGMA wal_autocheckpoint=1000;
             PRAGMA journal_size_limit=67108864;",
        )
        .map_err(DatabaseError::from_sqlite)
}

fn check_integrity(connection: &Connection) -> DatabaseResult<()> {
    let quick_check: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(DatabaseError::from_sqlite)?;
    if quick_check != "ok" {
        return Err(DatabaseError::Corrupt(quick_check));
    }

    let foreign_key_errors: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(DatabaseError::from_sqlite)?;
    if foreign_key_errors != 0 {
        return Err(DatabaseError::Corrupt(format!(
            "{foreign_key_errors} foreign key violations"
        )));
    }

    Ok(())
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> std::io::Result<()> {
    File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> std::io::Result<()> {
    Ok(())
}
