use std::path::Path;

use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use tauri::{AppHandle, Manager};

use crate::migration::Migrator;

use super::DbError;

const DB_FILE: &str = "app.db";

pub async fn connect_sqlite(app: &AppHandle) -> Result<DatabaseConnection, DbError> {
    let dir: std::path::PathBuf = app
        .path()
        .app_data_dir()
        .map_err(|e| DbError::TauriPath(e.to_string()))?;
    tracing::debug!("DB dir: {}", dir.display());
    std::fs::create_dir_all(&dir)?;
    let db_path = dir.join(DB_FILE);
    let url = sqlite_file_url(&db_path)?;
    let mut opt = ConnectOptions::new(url);
    opt.max_connections(5).sqlx_logging(false);
    let conn = Database::connect(opt).await?;
    Migrator::up(&conn, None).await?;
    Ok(conn)
}

pub fn sqlite_file_url(path: &Path) -> Result<String, DbError> {
    let abs = std::path::absolute(path).map_err(|e| DbError::Path(e.to_string()))?;
    let s = abs.to_string_lossy().replace('\\', "/");
    let url = if s.starts_with('/') {
        format!("sqlite://{}?mode=rwc", s)
    } else {
        format!("sqlite:///{}?mode=rwc", s)
    };
    Ok(url)
}
