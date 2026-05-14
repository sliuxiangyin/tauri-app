use std::sync::PoisonError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("Task with name '{0}' already exists")]
    TaskAlreadyExists(String),

    #[error("Task '{0}' not found")]
    TaskNotFound(String),

    #[error("Lock poisoned: {0}")]
    LockError(String),
}

impl<T> From<PoisonError<T>> for SchedulerError {
    fn from(e: PoisonError<T>) -> Self {
        SchedulerError::LockError(e.to_string())
    }
}
