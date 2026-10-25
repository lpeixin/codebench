//! One error type for the whole core.
//!
//! Tauri commands must return something serializable; `String` is the simplest
//! contract and the frontend renders it verbatim. Everything internal funnels
//! through [`AppError`] so we keep `?` ergonomics and one place to format.

use std::fmt;

#[derive(Debug)]
pub enum AppError {
    /// User-facing message that is already fit for display.
    Msg(String),
    Db(rusqlite::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Msg(m) => write!(f, "{m}"),
            AppError::Db(e) => write!(f, "database error: {e}"),
            AppError::Io(e) => write!(f, "io error: {e}"),
            AppError::Json(e) => write!(f, "json error: {e}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Json(e)
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Msg(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Msg(s.to_string())
    }
}

/// Convenience for `map_err` in the command layer.
pub fn msg<E: fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Build a user-facing error.
pub fn user_err<T, S: Into<String>>(message: S) -> AppResult<T> {
    Err(AppError::Msg(message.into()))
}
