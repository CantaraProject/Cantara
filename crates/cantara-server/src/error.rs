//! How a request fails.
//!
//! Every handler returns [`AppResult`], and every failure becomes the same
//! JSON body ([`cantara_api::ApiError`]) with a fitting status. What went wrong
//! inside — a database error, a file that could not be written — is written to
//! the log and *not* sent: the client is told that something failed, not how
//! the server is built.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use cantara_api::ApiError;
use uuid::Uuid;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not logged in")]
    Unauthorized,
    #[error("not allowed")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    /// The edit started from a version that is no longer the current one.
    #[error("the item was changed in the meantime")]
    VersionConflict { current: Option<Uuid> },
    #[error("{0}")]
    Conflict(String),
    #[error("the file is larger than this server accepts")]
    TooLarge,
    #[error("{0}")]
    Unsupported(String),
    #[error("too many attempts — try again in a minute")]
    TooManyRequests,
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn internal(error: impl std::fmt::Display) -> AppError {
        AppError::Internal(error.to_string())
    }

    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::VersionConflict { .. } => (StatusCode::CONFLICT, "version_conflict"),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            AppError::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
            AppError::Unsupported(_) => (StatusCode::UNPROCESSABLE_ENTITY, "unsupported"),
            AppError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "too_many_requests"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        let message = match &self {
            AppError::Internal(detail) => {
                tracing::error!("{detail}");
                "Something went wrong on the server.".to_string()
            }
            other => other.to_string(),
        };
        let current_version = match &self {
            AppError::VersionConflict { current } => *current,
            _ => None,
        };
        (
            status,
            Json(ApiError {
                code: code.to_string(),
                message,
                current_version,
            }),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> AppError {
        match &error {
            sqlx::Error::RowNotFound => AppError::NotFound,
            sqlx::Error::Database(database) if database.is_unique_violation() => {
                AppError::Conflict("that already exists".to_string())
            }
            _ => AppError::Internal(error.to_string()),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> AppError {
        AppError::Internal(error.to_string())
    }
}
