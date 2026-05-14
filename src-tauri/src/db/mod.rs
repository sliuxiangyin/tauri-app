mod connection;
mod error;

use std::sync::Arc;

pub use error::DbError;
use sea_orm::DatabaseConnection;
use tauri::AppHandle;
use tokio::sync::Mutex;

pub struct DbState {
    app: AppHandle,
    conn: Mutex<Option<Arc<DatabaseConnection>>>,
}

impl DbState {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            conn: Mutex::new(None),
        }
    }

    pub async fn get(&self) -> Result<Arc<DatabaseConnection>, DbError> {
        let mut g = self.conn.lock().await;
        if let Some(c) = g.as_ref() {
            return Ok(Arc::clone(c));
        }
        let db = Arc::new(connection::connect_sqlite(&self.app).await?);
        *g = Some(Arc::clone(&db));
        Ok(db)
    }
}
