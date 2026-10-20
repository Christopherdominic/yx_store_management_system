use axum::extract::State;

use crate::{
    errors::AppResult,
    middlewares::auth::AuthUser,
    models::{Business, BusinessId, BusinessSettings, Permission},
    services::{
        access::authorize,
        businesses::{self, UpdateBusinessProfile, UpdateBusinessSettings},
    },
    state::AppState,
    utils::extract::{Json, Path},
};

pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<BusinessId>,
) -> AppResult<Json<Business>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::BusinessView,
    )
    .await?;
    Ok(Json(businesses::get(&state.pool, business_id).await?))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<BusinessId>,
    Json(input): Json<UpdateBusinessProfile>,
) -> AppResult<Json<Business>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::BusinessUpdate,
    )
    .await?;
    Ok(Json(
        businesses::update_profile(&state.pool, user.user_id, business_id, input).await?,
    ))
}

pub async fn get_settings(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<BusinessId>,
) -> AppResult<Json<BusinessSettings>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::BusinessView,
    )
    .await?;
    Ok(Json(
        businesses::get_settings(&state.pool, business_id).await?,
    ))
}

pub async fn update_settings(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<BusinessId>,
    Json(input): Json<UpdateBusinessSettings>,
) -> AppResult<Json<BusinessSettings>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::BusinessSettingsUpdate,
    )
    .await?;
    Ok(Json(
        businesses::update_settings(&state.pool, user.user_id, business_id, input).await?,
    ))
}
