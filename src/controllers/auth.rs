use axum::{extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    errors::AppResult,
    middlewares::auth::AuthUser,
    models::{
        roles::{BusinessRole, StoreRole},
        user::User,
    },
    services::auth::{self, ChangePasswordInput, LoginResult, RegisterInput, ResetPasswordInput},
    state::AppState,
    utils::extract::Json,
};

pub async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterInput>,
) -> AppResult<(StatusCode, Json<User>)> {
    let user = auth::register_owner(&state, input).await?;
    Ok((StatusCode::CREATED, Json(user)))
}

#[derive(Debug, Deserialize)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginInput>,
) -> AppResult<Json<LoginResult>> {
    Ok(Json(
        auth::login(&state, &input.email, input.password).await?,
    ))
}

pub async fn logout(State(state): State<AppState>, user: AuthUser) -> AppResult<StatusCode> {
    auth::logout(&state, &user).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn logout_all(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    let revoked = auth::logout_all(&state, &user).await?;
    Ok(Json(json!({ "sessions_revoked": revoked })))
}

pub async fn me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    let profile = auth::get_user(&state.pool, user.user_id).await?;
    let memberships = auth::memberships(&state.pool, user.user_id).await?;
    Ok(Json(json!({ "user": profile, "memberships": memberships })))
}

pub async fn change_password(
    State(state): State<AppState>,
    user: AuthUser,
    Json(input): Json<ChangePasswordInput>,
) -> AppResult<StatusCode> {
    auth::change_password(&state, &user, input).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordInput {
    pub email: String,
}

/// Always 202 with the same message, whether or not the email exists.
pub async fn forgot_password(
    State(state): State<AppState>,
    Json(input): Json<ForgotPasswordInput>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let token = auth::request_password_reset(&state, &input.email).await?;
    let mut body = json!({
        "message": "if an active account exists for this email, password reset instructions have been sent"
    });
    match token {
        // No email provider is wired up yet; development builds hand the
        // token back directly so the flow can be exercised.
        Some(token) if state.config.expose_reset_tokens => body["reset_token"] = json!(token),
        Some(_) => tracing::warn!("password reset requested but no email delivery is configured"),
        None => {}
    }
    Ok((StatusCode::ACCEPTED, Json(body)))
}

pub async fn reset_password(
    State(state): State<AppState>,
    Json(input): Json<ResetPasswordInput>,
) -> AppResult<StatusCode> {
    auth::reset_password(&state, input).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The supported roles and what each one may do.
pub async fn roles() -> Json<Value> {
    let business_roles: Vec<Value> = BusinessRole::ALL
        .iter()
        .map(|r| json!({ "role": r, "scope": "business", "permissions": r.permissions() }))
        .collect();
    let store_roles: Vec<Value> = StoreRole::ALL
        .iter()
        .map(|r| json!({ "role": r, "scope": "store", "permissions": r.permissions() }))
        .collect();
    Json(json!({ "business_roles": business_roles, "store_roles": store_roles }))
}
