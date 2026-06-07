pub mod connection;
mod error;

use std::sync::Arc;

pub use error::DbError;
use sea_orm::DatabaseConnection;
use tauri::AppHandle;
use tokio::sync::Mutex;

/// 数据库状态管理器
///
/// 职责：
/// - 管理 SQLite 数据库连接的懒加载
/// - 提供线程安全的连接访问
/// - 实现 Clone 以支持在异步任务中传递
#[derive(Clone)]
pub struct DbState {
    app: Option<AppHandle>,
    conn: Arc<Mutex<Option<Arc<DatabaseConnection>>>>,
}

impl DbState {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app: Some(app),
            conn: Arc::new(Mutex::new(None)),
        }
    }

    /// 直接从已有连接构造（仅用于测试场景）
    #[allow(dead_code)]
    pub fn from_connection(db: DatabaseConnection) -> Self {
        Self {
            app: None,
            conn: Arc::new(Mutex::new(Some(Arc::new(db)))),
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

    /// 获取内部引用（用于传递给需要 &DbState 的函数）
    #[allow(dead_code)]
    pub fn inner(&self) -> &DbState {
        self
    }
}

// ──────────────────────────────────────────────────────────────
// Trait 实现
// ──────────────────────────────────────────────────────────────

use crate::services::traits::DbAccessor;
use async_trait::async_trait;

#[async_trait]
impl DbAccessor for DbState {
    async fn get(&self) -> Result<Arc<DatabaseConnection>, DbError> {
        DbState::get(self).await
    }
}
