use axum::{extract::FromRequestParts, http::header::AUTHORIZATION, http::request::Parts};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{errors::AppError, services, state::AppState};

/// The authenticated caller. Adding this extractor to a handler makes the
/// route protected: a missing, expired, idle, or revoked token yields 401.
#[derive(Debug, Clone, FromRow)]
pub struct AuthUser {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub full_name: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .ok_or_else(AppError::unauthenticated)?;
        services::auth::authenticate(state, token).await
    }
}
