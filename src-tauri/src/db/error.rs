use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("invalid database path: {0}")]
    Path(String),
    #[error(transparent)]
    SeaOrm(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("failed to resolve app data directory: {0}")]
    TauriPath(String),
    #[error("{0}")]
    Other(String),
}
