use axum::{extract::State, http::StatusCode};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    middlewares::auth::AuthUser,
    models::{
        roles::Permission,
        store::{Store, StoreSettings, StoreStatus},
    },
    services::{
        access::authorize,
        stores::{self, CreateStore, StoreFilter, UpdateStore, UpdateStoreSettings},
    },
    state::AppState,
    utils::extract::{Json, Path, Query},
};

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<Uuid>,
    Json(input): Json<CreateStore>,
) -> AppResult<(StatusCode, Json<Store>)> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::StoreCreate,
    )
    .await?;
    let store = stores::create(&state.pool, user.user_id, business_id, input).await?;
    Ok((StatusCode::CREATED, Json(store)))
}

/// Owners and administrators see every store; other staff see only the
/// stores they are assigned to.
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<Uuid>,
    Query(filter): Query<StoreFilter>,
) -> AppResult<Json<Vec<Store>>> {
    let access = authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::StoreView,
    )
    .await?;
    let visible = access.stores_with(Permission::StoreView);
    Ok(Json(
        stores::list(&state.pool, business_id, visible, filter).await?,
    ))
}

pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, store_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<Store>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        Some(store_id),
        Permission::StoreView,
    )
    .await?;
    Ok(Json(stores::get(&state.pool, business_id, store_id).await?))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, store_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateStore>,
) -> AppResult<Json<Store>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        Some(store_id),
        Permission::StoreUpdate,
    )
    .await?;
    Ok(Json(
        stores::update(&state.pool, user.user_id, business_id, store_id, input).await?,
    ))
}

pub async fn activate(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, store_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<Store>> {
    set_status(state, user, business_id, store_id, StoreStatus::Active).await
}

pub async fn deactivate(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, store_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<Store>> {
    set_status(state, user, business_id, store_id, StoreStatus::Inactive).await
}

async fn set_status(
    state: AppState,
    user: AuthUser,
    business_id: Uuid,
    store_id: Uuid,
    status: StoreStatus,
) -> AppResult<Json<Store>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        Some(store_id),
        Permission::StoreStatusChange,
    )
    .await?;
    Ok(Json(
        stores::set_status(&state.pool, user.user_id, business_id, store_id, status).await?,
    ))
}

pub async fn get_settings(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, store_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<StoreSettings>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        Some(store_id),
        Permission::StoreView,
    )
    .await?;
    let store = stores::get(&state.pool, business_id, store_id).await?;
    Ok(Json(StoreSettings::from(&store)))
}

pub async fn update_settings(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, store_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateStoreSettings>,
) -> AppResult<Json<StoreSettings>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        Some(store_id),
        Permission::StoreSettingsUpdate,
    )
    .await?;
    Ok(Json(
        stores::update_settings(&state.pool, user.user_id, business_id, store_id, input).await?,
    ))
}
