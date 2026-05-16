mod connection;
mod error;

use std::sync::Arc;

pub use error::DbError;
use sea_orm::DatabaseConnection;
use tauri::AppHandle;
use tokio::sync::Mutex;

pub struct DbState {
    app: Option<AppHandle>,
    conn: Mutex<Option<Arc<DatabaseConnection>>>,
}

impl DbState {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app: Some(app),
            conn: Mutex::new(None),
        }
    }

    /// 直接从已有连接构造（仅用于测试场景）
    pub fn from_connection(db: DatabaseConnection) -> Self {
        Self {
            app: None,
            conn: Mutex::new(Some(Arc::new(db))),
        }
    }

    pub async fn get(&self) -> Result<Arc<DatabaseConnection>, DbError> {
        let mut g = self.conn.lock().await;
        if let Some(c) = g.as_ref() {
            return Ok(Arc::clone(c));
        }
        let app = self
            .app
            .as_ref()
            .ok_or_else(|| DbError::Other("DbState 未初始化 AppHandle".into()))?;
        let db = Arc::new(connection::connect_sqlite(app).await?);
        *g = Some(Arc::clone(&db));
        Ok(db)
    }
}
