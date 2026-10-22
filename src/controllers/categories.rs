use axum::{extract::State, http::StatusCode};

use crate::{
    errors::AppResult,
    middlewares::auth::AuthUser,
    models::{BusinessId, Permission, ProductCategory, ProductCategoryId, RecordStatus},
    services::{
        access::authorize,
        categories::{self, CategoryFilter, CreateCategory, UpdateCategory},
    },
    state::AppState,
    utils::extract::{Json, Path, Query},
};

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<BusinessId>,
    Json(input): Json<CreateCategory>,
) -> AppResult<(StatusCode, Json<ProductCategory>)> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::ProductManage,
    )
    .await?;
    let category = categories::create(&state.pool, user.user_id, business_id, input).await?;
    Ok((StatusCode::CREATED, Json(category)))
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<BusinessId>,
    Query(filter): Query<CategoryFilter>,
) -> AppResult<Json<Vec<ProductCategory>>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::ProductView,
    )
    .await?;
    Ok(Json(
        categories::list(&state.pool, business_id, filter).await?,
    ))
}

pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, category_id)): Path<(BusinessId, ProductCategoryId)>,
) -> AppResult<Json<ProductCategory>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::ProductView,
    )
    .await?;
    Ok(Json(
        categories::get(&state.pool, business_id, category_id).await?,
    ))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, category_id)): Path<(BusinessId, ProductCategoryId)>,
    Json(input): Json<UpdateCategory>,
) -> AppResult<Json<ProductCategory>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::ProductManage,
    )
    .await?;
    Ok(Json(
        categories::update(&state.pool, user.user_id, business_id, category_id, input).await?,
    ))
}

pub async fn archive(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, category_id)): Path<(BusinessId, ProductCategoryId)>,
) -> AppResult<Json<ProductCategory>> {
    set_status(
        state,
        user,
        business_id,
        category_id,
        RecordStatus::Archived,
    )
    .await
}

pub async fn restore(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, category_id)): Path<(BusinessId, ProductCategoryId)>,
) -> AppResult<Json<ProductCategory>> {
    set_status(state, user, business_id, category_id, RecordStatus::Active).await
}

async fn set_status(
    state: AppState,
    user: AuthUser,
    business_id: BusinessId,
    category_id: ProductCategoryId,
    status: RecordStatus,
) -> AppResult<Json<ProductCategory>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::ProductManage,
    )
    .await?;
    Ok(Json(
        categories::set_status(&state.pool, user.user_id, business_id, category_id, status).await?,
    ))
}
