use axum::{extract::State, http::StatusCode};
use serde::Deserialize;

use crate::{
    errors::AppResult,
    middlewares::auth::AuthUser,
    models::{
        BusinessId, Permission, PriceHistory, Product, ProductId, ProductVariant, ProductVariantId,
        RecordStatus,
    },
    services::{
        access::authorize,
        products::{
            self, CatalogItem, ChangePrices, CreateProduct, CreateVariant, PriceHistoryFilter,
            ProductFilter, UpdateProduct, UpdateVariant,
        },
    },
    state::AppState,
    utils::extract::{Json, Path, Query},
};

#[derive(Debug, Deserialize)]
pub struct LookupQuery {
    /// A SKU or barcode.
    pub code: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct VariantFilter {
    pub status: Option<RecordStatus>,
}

async fn can(
    state: &AppState,
    user: &AuthUser,
    business_id: BusinessId,
    p: Permission,
) -> AppResult<()> {
    authorize(state, user.user_id, business_id, None, p).await?;
    Ok(())
}

// Products (FR-07) ----------------------------------------------------------

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<BusinessId>,
    Json(input): Json<CreateProduct>,
) -> AppResult<(StatusCode, Json<Product>)> {
    can(&state, &user, business_id, Permission::ProductManage).await?;
    let product = products::create(&state.pool, user.user_id, business_id, input).await?;
    Ok((StatusCode::CREATED, Json(product)))
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<BusinessId>,
    Query(filter): Query<ProductFilter>,
) -> AppResult<Json<Vec<Product>>> {
    can(&state, &user, business_id, Permission::ProductView).await?;
    Ok(Json(
        products::list(&state.pool, business_id, filter).await?,
    ))
}

/// Barcode scanner / SKU lookup (FR-08).
pub async fn lookup(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<BusinessId>,
    Query(query): Query<LookupQuery>,
) -> AppResult<Json<CatalogItem>> {
    can(&state, &user, business_id, Permission::ProductView).await?;
    Ok(Json(
        products::lookup(&state.pool, business_id, &query.code).await?,
    ))
}

pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id)): Path<(BusinessId, ProductId)>,
) -> AppResult<Json<Product>> {
    can(&state, &user, business_id, Permission::ProductView).await?;
    Ok(Json(
        products::get(&state.pool, business_id, product_id).await?,
    ))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id)): Path<(BusinessId, ProductId)>,
    Json(input): Json<UpdateProduct>,
) -> AppResult<Json<Product>> {
    can(&state, &user, business_id, Permission::ProductManage).await?;
    Ok(Json(
        products::update(&state.pool, user.user_id, business_id, product_id, input).await?,
    ))
}

pub async fn archive(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id)): Path<(BusinessId, ProductId)>,
) -> AppResult<Json<Product>> {
    set_status(state, user, business_id, product_id, RecordStatus::Archived).await
}

pub async fn restore(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id)): Path<(BusinessId, ProductId)>,
) -> AppResult<Json<Product>> {
    set_status(state, user, business_id, product_id, RecordStatus::Active).await
}

async fn set_status(
    state: AppState,
    user: AuthUser,
    business_id: BusinessId,
    product_id: ProductId,
    status: RecordStatus,
) -> AppResult<Json<Product>> {
    can(&state, &user, business_id, Permission::ProductManage).await?;
    Ok(Json(
        products::set_status(&state.pool, user.user_id, business_id, product_id, status).await?,
    ))
}

// Prices (FR-09) ------------------------------------------------------------

pub async fn change_prices(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id)): Path<(BusinessId, ProductId)>,
    Json(input): Json<ChangePrices>,
) -> AppResult<Json<Product>> {
    can(&state, &user, business_id, Permission::PriceManage).await?;
    Ok(Json(
        products::change_product_prices(&state.pool, user.user_id, business_id, product_id, input)
            .await?,
    ))
}

pub async fn price_history(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id)): Path<(BusinessId, ProductId)>,
    Query(filter): Query<PriceHistoryFilter>,
) -> AppResult<Json<Vec<PriceHistory>>> {
    can(&state, &user, business_id, Permission::ProductView).await?;
    Ok(Json(
        products::price_history(&state.pool, business_id, product_id, filter).await?,
    ))
}

// Variants (FR-08) ----------------------------------------------------------

pub async fn create_variant(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id)): Path<(BusinessId, ProductId)>,
    Json(input): Json<CreateVariant>,
) -> AppResult<(StatusCode, Json<ProductVariant>)> {
    can(&state, &user, business_id, Permission::ProductManage).await?;
    let variant =
        products::create_variant(&state.pool, user.user_id, business_id, product_id, input).await?;
    Ok((StatusCode::CREATED, Json(variant)))
}

pub async fn list_variants(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id)): Path<(BusinessId, ProductId)>,
    Query(filter): Query<VariantFilter>,
) -> AppResult<Json<Vec<ProductVariant>>> {
    can(&state, &user, business_id, Permission::ProductView).await?;
    Ok(Json(
        products::list_variants(&state.pool, business_id, product_id, filter.status).await?,
    ))
}

pub async fn get_variant(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id, variant_id)): Path<(BusinessId, ProductId, ProductVariantId)>,
) -> AppResult<Json<ProductVariant>> {
    can(&state, &user, business_id, Permission::ProductView).await?;
    Ok(Json(
        products::get_variant(&state.pool, business_id, product_id, variant_id).await?,
    ))
}

pub async fn update_variant(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id, variant_id)): Path<(BusinessId, ProductId, ProductVariantId)>,
    Json(input): Json<UpdateVariant>,
) -> AppResult<Json<ProductVariant>> {
    can(&state, &user, business_id, Permission::ProductManage).await?;
    Ok(Json(
        products::update_variant(
            &state.pool,
            user.user_id,
            business_id,
            product_id,
            variant_id,
            input,
        )
        .await?,
    ))
}

pub async fn archive_variant(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id, variant_id)): Path<(BusinessId, ProductId, ProductVariantId)>,
) -> AppResult<Json<ProductVariant>> {
    set_variant_status(
        state,
        user,
        business_id,
        product_id,
        variant_id,
        RecordStatus::Archived,
    )
    .await
}

pub async fn restore_variant(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id, variant_id)): Path<(BusinessId, ProductId, ProductVariantId)>,
) -> AppResult<Json<ProductVariant>> {
    set_variant_status(
        state,
        user,
        business_id,
        product_id,
        variant_id,
        RecordStatus::Active,
    )
    .await
}

async fn set_variant_status(
    state: AppState,
    user: AuthUser,
    business_id: BusinessId,
    product_id: ProductId,
    variant_id: ProductVariantId,
    status: RecordStatus,
) -> AppResult<Json<ProductVariant>> {
    can(&state, &user, business_id, Permission::ProductManage).await?;
    Ok(Json(
        products::set_variant_status(
            &state.pool,
            user.user_id,
            business_id,
            product_id,
            variant_id,
            status,
        )
        .await?,
    ))
}

pub async fn change_variant_prices(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, product_id, variant_id)): Path<(BusinessId, ProductId, ProductVariantId)>,
    Json(input): Json<ChangePrices>,
) -> AppResult<Json<ProductVariant>> {
    can(&state, &user, business_id, Permission::PriceManage).await?;
    Ok(Json(
        products::change_variant_prices(
            &state.pool,
            user.user_id,
            business_id,
            product_id,
            variant_id,
            input,
        )
        .await?,
    ))
}
