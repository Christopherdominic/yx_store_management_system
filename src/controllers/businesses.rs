use axum::{extract::State, http::StatusCode};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    middlewares::auth::AuthUser,
    models::{
        business::{Business, BusinessSettings},
        roles::Permission,
    },
    services::{
        access::authorize,
        businesses::{self, CreateBusiness, UpdateBusinessProfile, UpdateBusinessSettings},
    },
    state::AppState,
    utils::extract::{Json, Path},
};

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(input): Json<CreateBusiness>,
) -> AppResult<(StatusCode, Json<Business>)> {
    let business = businesses::create(&state.pool, user.user_id, input).await?;
    Ok((StatusCode::CREATED, Json(business)))
}

pub async fn list(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Business>>> {
    Ok(Json(
        businesses::list_for_user(&state.pool, user.user_id).await?,
    ))
}

pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<Uuid>,
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
    Path(business_id): Path<Uuid>,
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
    Path(business_id): Path<Uuid>,
) -> AppResult<Json<BusinessSettings>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::BusinessView,
    )
    .await?;
    let business = businesses::get(&state.pool, business_id).await?;
    Ok(Json(BusinessSettings::from(&business)))
}

pub async fn update_settings(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<Uuid>,
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
