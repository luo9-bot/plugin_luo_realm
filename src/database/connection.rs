use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

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
            let backup = destination.with_extension("sqlite3.bak");
            if backup.exists() {
                fs::remove_file(&backup)
                    .map_err(|error| DatabaseError::Migration(error.to_string()))?;
            }
            fs::rename(destination, backup)
                .map_err(|error| DatabaseError::Migration(error.to_string()))?;
        }
        fs::rename(temporary, destination)
            .map_err(|error| DatabaseError::Migration(error.to_string()))
    }

    pub fn schema_version(&self) -> DatabaseResult<i64> {
        self.connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .map_err(DatabaseError::from_sqlite)
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
