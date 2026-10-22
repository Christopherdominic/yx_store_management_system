//! Product catalogue, variants and pricing (FR-07 – FR-09).
//!
//! SKUs and barcodes are unique across products and variants within a
//! business. Both are registered in `catalog_codes` in the same transaction
//! as the record that owns them, so a duplicate is rejected by the database
//! even when two requests race.
//!
//! Prices are changed only through [`change_product_prices`] and
//! [`change_variant_prices`], which write one [`PriceHistory`] entry per
//! changed price. Completed sales keep their own price snapshots, so price
//! changes never alter history.

use std::collections::BTreeMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};

use crate::{
    errors::{AppError, AppResult},
    models::{
        AuditAction, BusinessId, Money, PriceHistory, PriceHistoryId, PriceType, Product,
        ProductCategoryId, ProductId, ProductVariant, ProductVariantId, RecordStatus, RecordType,
        UnitOfMeasure, UserId,
    },
    services::{audit::AuditEntry, categories},
    utils::validation::{
        Validator, clean, clean_attributes, clean_opt, like_pattern, nullable, patch_opt,
    },
};

// ---------------------------------------------------------------------------
// Request bodies and filters
// ---------------------------------------------------------------------------

/// Prices may be sent as JSON strings (`"72500.50"`, preferred) or numbers.
#[derive(Debug, Deserialize)]
pub struct CreateProduct {
    pub name: String,
    pub sku: String,
    pub category_id: Option<ProductCategoryId>,
    pub description: Option<String>,
    pub barcode: Option<String>,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    #[serde(default)]
    pub unit_of_measure: UnitOfMeasure,
    pub cost_price: Money,
    pub selling_price: Money,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
    #[serde(default)]
    pub tracks_batches: bool,
}

/// PATCH body: absent fields are unchanged and `""` clears optional text.
/// `"category_id": null` removes the category. Prices are changed through
/// `PATCH .../prices` instead, so they are rejected here.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProduct {
    pub name: Option<String>,
    pub sku: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub category_id: Option<Option<ProductCategoryId>>,
    pub description: Option<String>,
    pub barcode: Option<String>,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub unit_of_measure: Option<UnitOfMeasure>,
    /// Replaces all attributes.
    pub attributes: Option<BTreeMap<String, String>>,
    pub tracks_batches: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProductFilter {
    /// Matches name, brand, SKU or barcode (partial), including variant
    /// SKUs and barcodes.
    pub q: Option<String>,
    pub category_id: Option<ProductCategoryId>,
    pub status: Option<RecordStatus>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Variant prices default to the parent product's prices when omitted.
#[derive(Debug, Deserialize)]
pub struct CreateVariant {
    pub name: String,
    pub sku: String,
    pub barcode: Option<String>,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
    pub cost_price: Option<Money>,
    pub selling_price: Option<Money>,
}

/// PATCH body for a variant. Prices are changed through `PATCH .../prices`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateVariant {
    pub name: Option<String>,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    /// Replaces all attributes.
    pub attributes: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangePrices {
    pub cost_price: Option<Money>,
    pub selling_price: Option<Money>,
    pub reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PriceHistoryFilter {
    /// Only changes to this variant. Without it, all changes for the product
    /// and its variants are returned.
    pub variant_id: Option<ProductVariantId>,
    pub price_type: Option<PriceType>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Result of a barcode or SKU lookup (FR-08).
#[derive(Debug, Serialize)]
pub struct CatalogItem {
    pub product: Product,
    /// Set when the code belongs to a variant.
    pub variant: Option<ProductVariant>,
}

// ---------------------------------------------------------------------------
// Normalisation and validation
// ---------------------------------------------------------------------------

fn sku(value: String) -> String {
    value.trim().to_uppercase()
}

fn barcode(value: Option<String>) -> Option<String> {
    clean_opt(value)
}

fn validate_product(p: &Product, v: &mut Validator) {
    v.required("name", &p.name, 200);
    v.sku("sku", &p.sku);
    v.barcode("barcode", p.barcode.as_deref());
    v.optional("description", p.description.as_deref(), 2000);
    v.optional("brand", p.brand.as_deref(), 100);
    v.url("image_url", p.image_url.as_deref());
    v.price("cost_price", p.cost_price);
    v.price("selling_price", p.selling_price);
    v.attributes("attributes", &p.attributes);
}

fn validate_variant(variant: &ProductVariant, v: &mut Validator) {
    v.required("name", &variant.name, 200);
    v.sku("sku", &variant.sku);
    v.barcode("barcode", variant.barcode.as_deref());
    v.price("cost_price", variant.cost_price);
    v.price("selling_price", variant.selling_price);
    v.attributes("attributes", &variant.attributes);
}

fn page(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    (
        limit.unwrap_or(50).clamp(1, 200),
        offset.unwrap_or(0).max(0),
    )
}

// ---------------------------------------------------------------------------
// SKU / barcode registry
// ---------------------------------------------------------------------------

async fn register_code(
    conn: &mut PgConnection,
    business_id: BusinessId,
    kind: &str,
    code: &str,
    product_id: ProductId,
    variant_id: Option<ProductVariantId>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO catalog_codes (business_id, kind, code, product_id, variant_id)
         VALUES ($1, $2, upper($3), $4, $5)",
    )
    .bind(business_id)
    .bind(kind)
    .bind(code)
    .bind(product_id)
    .bind(variant_id)
    .execute(conn)
    .await?;
    Ok(())
}

async fn release_code(
    conn: &mut PgConnection,
    business_id: BusinessId,
    kind: &str,
    code: &str,
) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM catalog_codes WHERE business_id = $1 AND kind = $2 AND code = upper($3)",
    )
    .bind(business_id)
    .bind(kind)
    .bind(code)
    .execute(conn)
    .await?;
    Ok(())
}

/// Registers new codes and releases old ones when a SKU or barcode changes.
#[allow(clippy::too_many_arguments)]
async fn update_codes(
    conn: &mut PgConnection,
    business_id: BusinessId,
    product_id: ProductId,
    variant_id: Option<ProductVariantId>,
    old_sku: &str,
    new_sku: &str,
    old_barcode: Option<&str>,
    new_barcode: Option<&str>,
) -> AppResult<()> {
    if !old_sku.eq_ignore_ascii_case(new_sku) {
        release_code(conn, business_id, "sku", old_sku).await?;
        register_code(conn, business_id, "sku", new_sku, product_id, variant_id).await?;
    }
    let barcode_changed = match (old_barcode, new_barcode) {
        (Some(a), Some(b)) => !a.eq_ignore_ascii_case(b),
        (a, b) => a.is_some() != b.is_some(),
    };
    if barcode_changed {
        if let Some(old) = old_barcode {
            release_code(conn, business_id, "barcode", old).await?;
        }
        if let Some(new) = new_barcode {
            register_code(conn, business_id, "barcode", new, product_id, variant_id).await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Products
// ---------------------------------------------------------------------------

pub async fn create(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    input: CreateProduct,
) -> AppResult<Product> {
    let now = Utc::now();
    let product = Product {
        id: ProductId::generate(),
        business_id,
        category_id: input.category_id,
        name: clean(input.name),
        description: clean_opt(input.description),
        sku: sku(input.sku),
        barcode: barcode(input.barcode),
        brand: clean_opt(input.brand),
        image_url: clean_opt(input.image_url),
        unit_of_measure: input.unit_of_measure,
        cost_price: input.cost_price,
        selling_price: input.selling_price,
        attributes: clean_attributes(input.attributes),
        tracks_batches: input.tracks_batches,
        status: RecordStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let mut v = Validator::new();
    validate_product(&product, &mut v);
    v.finish()?;

    let mut tx = pool.begin().await?;
    if let Some(category_id) = product.category_id {
        categories::ensure_active(&mut *tx, business_id, category_id, "category_id").await?;
    }
    let product = sqlx::query_as::<_, Product>(
        "INSERT INTO products (id, business_id, category_id, name, description, sku, barcode,
                               brand, image_url, unit_of_measure, cost_price, selling_price,
                               attributes, tracks_batches, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
         RETURNING *",
    )
    .bind(product.id)
    .bind(business_id)
    .bind(product.category_id)
    .bind(&product.name)
    .bind(&product.description)
    .bind(&product.sku)
    .bind(&product.barcode)
    .bind(&product.brand)
    .bind(&product.image_url)
    .bind(product.unit_of_measure)
    .bind(product.cost_price)
    .bind(product.selling_price)
    .bind(sqlx::types::Json(&product.attributes))
    .bind(product.tracks_batches)
    .bind(product.status)
    .fetch_one(&mut *tx)
    .await?;
    register_code(&mut tx, business_id, "sku", &product.sku, product.id, None).await?;
    if let Some(code) = &product.barcode {
        register_code(&mut tx, business_id, "barcode", code, product.id, None).await?;
    }
    AuditEntry::new(business_id, AuditAction::Created)
        .actor(actor)
        .record(RecordType::Product, product.id)
        .after(&product)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(product)
}

pub async fn list(
    pool: &PgPool,
    business_id: BusinessId,
    filter: ProductFilter,
) -> AppResult<Vec<Product>> {
    let term = filter
        .q
        .map(|q| q.trim().to_string())
        .filter(|q| !q.is_empty());
    let (limit, offset) = page(filter.limit, filter.offset);
    let products = sqlx::query_as::<_, Product>(
        "SELECT * FROM products p
         WHERE p.business_id = $1
           AND ($2::text IS NULL
                OR p.name ILIKE $3 OR p.brand ILIKE $3 OR p.sku ILIKE $3 OR p.barcode ILIKE $3
                OR EXISTS (SELECT 1 FROM product_variants pv
                           WHERE pv.product_id = p.id
                             AND (pv.sku ILIKE $3 OR pv.barcode ILIKE $3 OR pv.name ILIKE $3)))
           AND ($4::uuid IS NULL OR p.category_id = $4)
           AND ($5::text IS NULL OR p.status = $5)
         ORDER BY lower(p.name), p.id
         LIMIT $6 OFFSET $7",
    )
    .bind(business_id)
    .bind(&term)
    .bind(term.as_deref().map(like_pattern))
    .bind(filter.category_id)
    .bind(filter.status)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(products)
}

/// Fetches a product only if it belongs to `business_id`.
pub async fn get(
    executor: impl sqlx::PgExecutor<'_>,
    business_id: BusinessId,
    product_id: ProductId,
) -> AppResult<Product> {
    sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1 AND business_id = $2")
        .bind(product_id)
        .bind(business_id)
        .fetch_optional(executor)
        .await?
        .ok_or(AppError::NotFound("product"))
}

/// Finds the product or variant whose SKU or barcode matches `code` exactly
/// (ignoring case), e.g. from a barcode scanner (FR-08).
pub async fn lookup(pool: &PgPool, business_id: BusinessId, code: &str) -> AppResult<CatalogItem> {
    let code = code.trim();
    if code.is_empty() {
        return Err(AppError::field("code", "is required"));
    }
    let found: Option<(ProductId, Option<ProductVariantId>)> = sqlx::query_as(
        "SELECT product_id, variant_id FROM catalog_codes
         WHERE business_id = $1 AND code = upper($2)
         ORDER BY kind = 'barcode' DESC
         LIMIT 1",
    )
    .bind(business_id)
    .bind(code)
    .fetch_optional(pool)
    .await?;
    let (product_id, variant_id) = found.ok_or(AppError::NotFound("product"))?;
    let product = get(pool, business_id, product_id).await?;
    let variant = match variant_id {
        Some(id) => Some(get_variant(pool, business_id, product_id, id).await?),
        None => None,
    };
    Ok(CatalogItem { product, variant })
}

async fn lock(
    conn: &mut PgConnection,
    business_id: BusinessId,
    product_id: ProductId,
) -> AppResult<Product> {
    sqlx::query_as::<_, Product>(
        "SELECT * FROM products WHERE id = $1 AND business_id = $2 FOR UPDATE",
    )
    .bind(product_id)
    .bind(business_id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound("product"))
}

async fn save(conn: &mut PgConnection, p: &Product) -> AppResult<Product> {
    let saved = sqlx::query_as::<_, Product>(
        "UPDATE products SET
            category_id = $2, name = $3, description = $4, sku = $5, barcode = $6, brand = $7,
            image_url = $8, unit_of_measure = $9, cost_price = $10, selling_price = $11,
            attributes = $12, tracks_batches = $13, status = $14, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(p.id)
    .bind(p.category_id)
    .bind(&p.name)
    .bind(&p.description)
    .bind(&p.sku)
    .bind(&p.barcode)
    .bind(&p.brand)
    .bind(&p.image_url)
    .bind(p.unit_of_measure)
    .bind(p.cost_price)
    .bind(p.selling_price)
    .bind(sqlx::types::Json(&p.attributes))
    .bind(p.tracks_batches)
    .bind(p.status)
    .fetch_one(conn)
    .await?;
    Ok(saved)
}

pub async fn update(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    product_id: ProductId,
    input: UpdateProduct,
) -> AppResult<Product> {
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id, product_id).await?;
    let mut p = before.clone();
    if let Some(name) = input.name {
        p.name = clean(name);
    }
    if let Some(value) = input.sku {
        p.sku = sku(value);
    }
    if let Some(category_id) = input.category_id {
        p.category_id = category_id;
    }
    p.description = patch_opt(p.description, input.description);
    p.barcode = patch_opt(p.barcode, input.barcode);
    p.brand = patch_opt(p.brand, input.brand);
    p.image_url = patch_opt(p.image_url, input.image_url);
    if let Some(unit) = input.unit_of_measure {
        p.unit_of_measure = unit;
    }
    if let Some(attributes) = input.attributes {
        p.attributes = clean_attributes(attributes);
    }
    if let Some(tracks_batches) = input.tracks_batches {
        p.tracks_batches = tracks_batches;
    }
    let mut v = Validator::new();
    validate_product(&p, &mut v);
    v.finish()?;

    if let Some(category_id) = p.category_id
        && p.category_id != before.category_id
    {
        categories::ensure_active(&mut *tx, business_id, category_id, "category_id").await?;
    }
    update_codes(
        &mut tx,
        business_id,
        product_id,
        None,
        &before.sku,
        &p.sku,
        before.barcode.as_deref(),
        p.barcode.as_deref(),
    )
    .await?;
    let after = save(&mut tx, &p).await?;
    AuditEntry::new(business_id, AuditAction::Updated)
        .actor(actor)
        .record(RecordType::Product, product_id)
        .before(&before)
        .after(&after)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

/// Archives or restores a product. Archived products stay readable, with
/// their history, but cannot get new variants or price changes. Setting the
/// current status again is a no-op.
pub async fn set_status(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    product_id: ProductId,
    status: RecordStatus,
) -> AppResult<Product> {
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id, product_id).await?;
    if before.status == status {
        return Ok(before);
    }
    let mut p = before.clone();
    p.status = status;
    let after = save(&mut tx, &p).await?;
    AuditEntry::new(business_id, categories::archive_action(status))
        .actor(actor)
        .record(RecordType::Product, product_id)
        .before(&serde_json::json!({ "status": before.status }))
        .after(&serde_json::json!({ "status": after.status }))
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

fn ensure_product_active(product: &Product) -> AppResult<()> {
    if !product.is_active() {
        return Err(AppError::Conflict(
            "product is archived; restore it first".into(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Variants
// ---------------------------------------------------------------------------

pub async fn create_variant(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    product_id: ProductId,
    input: CreateVariant,
) -> AppResult<ProductVariant> {
    let mut tx = pool.begin().await?;
    // Locking the product keeps it from being archived while the variant is added.
    let product = lock(&mut tx, business_id, product_id).await?;
    ensure_product_active(&product)?;

    let now = Utc::now();
    let variant = ProductVariant {
        id: ProductVariantId::generate(),
        business_id,
        product_id,
        name: clean(input.name),
        sku: sku(input.sku),
        barcode: barcode(input.barcode),
        attributes: clean_attributes(input.attributes),
        cost_price: input.cost_price.unwrap_or(product.cost_price),
        selling_price: input.selling_price.unwrap_or(product.selling_price),
        status: RecordStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let mut v = Validator::new();
    validate_variant(&variant, &mut v);
    v.finish()?;

    let variant = sqlx::query_as::<_, ProductVariant>(
        "INSERT INTO product_variants (id, business_id, product_id, name, sku, barcode,
                                       attributes, cost_price, selling_price, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING *",
    )
    .bind(variant.id)
    .bind(business_id)
    .bind(product_id)
    .bind(&variant.name)
    .bind(&variant.sku)
    .bind(&variant.barcode)
    .bind(sqlx::types::Json(&variant.attributes))
    .bind(variant.cost_price)
    .bind(variant.selling_price)
    .bind(variant.status)
    .fetch_one(&mut *tx)
    .await?;
    register_code(
        &mut tx,
        business_id,
        "sku",
        &variant.sku,
        product_id,
        Some(variant.id),
    )
    .await?;
    if let Some(code) = &variant.barcode {
        register_code(
            &mut tx,
            business_id,
            "barcode",
            code,
            product_id,
            Some(variant.id),
        )
        .await?;
    }
    AuditEntry::new(business_id, AuditAction::Created)
        .actor(actor)
        .record(RecordType::ProductVariant, variant.id)
        .after(&variant)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(variant)
}

pub async fn list_variants(
    pool: &PgPool,
    business_id: BusinessId,
    product_id: ProductId,
    status: Option<RecordStatus>,
) -> AppResult<Vec<ProductVariant>> {
    // 404 rather than an empty list for a product in another business.
    get(pool, business_id, product_id).await?;
    let variants = sqlx::query_as::<_, ProductVariant>(
        "SELECT * FROM product_variants
         WHERE product_id = $1 AND business_id = $2 AND ($3::text IS NULL OR status = $3)
         ORDER BY lower(name), id",
    )
    .bind(product_id)
    .bind(business_id)
    .bind(status)
    .fetch_all(pool)
    .await?;
    Ok(variants)
}

pub async fn get_variant(
    executor: impl sqlx::PgExecutor<'_>,
    business_id: BusinessId,
    product_id: ProductId,
    variant_id: ProductVariantId,
) -> AppResult<ProductVariant> {
    sqlx::query_as::<_, ProductVariant>(
        "SELECT * FROM product_variants WHERE id = $1 AND product_id = $2 AND business_id = $3",
    )
    .bind(variant_id)
    .bind(product_id)
    .bind(business_id)
    .fetch_optional(executor)
    .await?
    .ok_or(AppError::NotFound("variant"))
}

async fn lock_variant(
    conn: &mut PgConnection,
    business_id: BusinessId,
    product_id: ProductId,
    variant_id: ProductVariantId,
) -> AppResult<ProductVariant> {
    sqlx::query_as::<_, ProductVariant>(
        "SELECT * FROM product_variants
         WHERE id = $1 AND product_id = $2 AND business_id = $3
         FOR UPDATE",
    )
    .bind(variant_id)
    .bind(product_id)
    .bind(business_id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound("variant"))
}

async fn save_variant(conn: &mut PgConnection, v: &ProductVariant) -> AppResult<ProductVariant> {
    let saved = sqlx::query_as::<_, ProductVariant>(
        "UPDATE product_variants SET
            name = $2, sku = $3, barcode = $4, attributes = $5, cost_price = $6,
            selling_price = $7, status = $8, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(v.id)
    .bind(&v.name)
    .bind(&v.sku)
    .bind(&v.barcode)
    .bind(sqlx::types::Json(&v.attributes))
    .bind(v.cost_price)
    .bind(v.selling_price)
    .bind(v.status)
    .fetch_one(conn)
    .await?;
    Ok(saved)
}

pub async fn update_variant(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    product_id: ProductId,
    variant_id: ProductVariantId,
    input: UpdateVariant,
) -> AppResult<ProductVariant> {
    let mut tx = pool.begin().await?;
    let before = lock_variant(&mut tx, business_id, product_id, variant_id).await?;
    let mut variant = before.clone();
    if let Some(name) = input.name {
        variant.name = clean(name);
    }
    if let Some(value) = input.sku {
        variant.sku = sku(value);
    }
    variant.barcode = patch_opt(variant.barcode, input.barcode);
    if let Some(attributes) = input.attributes {
        variant.attributes = clean_attributes(attributes);
    }
    let mut v = Validator::new();
    validate_variant(&variant, &mut v);
    v.finish()?;

    update_codes(
        &mut tx,
        business_id,
        product_id,
        Some(variant_id),
        &before.sku,
        &variant.sku,
        before.barcode.as_deref(),
        variant.barcode.as_deref(),
    )
    .await?;
    let after = save_variant(&mut tx, &variant).await?;
    AuditEntry::new(business_id, AuditAction::Updated)
        .actor(actor)
        .record(RecordType::ProductVariant, variant_id)
        .before(&before)
        .after(&after)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

pub async fn set_variant_status(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    product_id: ProductId,
    variant_id: ProductVariantId,
    status: RecordStatus,
) -> AppResult<ProductVariant> {
    let mut tx = pool.begin().await?;
    let before = lock_variant(&mut tx, business_id, product_id, variant_id).await?;
    if before.status == status {
        return Ok(before);
    }
    if status == RecordStatus::Active {
        // A variant can't be restored under an archived product.
        let product = get(&mut *tx, business_id, product_id).await?;
        ensure_product_active(&product)?;
    }
    let mut variant = before.clone();
    variant.status = status;
    let after = save_variant(&mut tx, &variant).await?;
    AuditEntry::new(business_id, categories::archive_action(status))
        .actor(actor)
        .record(RecordType::ProductVariant, variant_id)
        .before(&serde_json::json!({ "status": before.status }))
        .after(&serde_json::json!({ "status": after.status }))
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

// ---------------------------------------------------------------------------
// Prices and price history (FR-09)
// ---------------------------------------------------------------------------

/// The prices before and after one change request.
struct PriceUpdate {
    cost: (Money, Money),
    selling: (Money, Money),
}

impl PriceUpdate {
    fn new(current_cost: Money, current_selling: Money, input: &ChangePrices) -> Self {
        Self {
            cost: (current_cost, input.cost_price.unwrap_or(current_cost)),
            selling: (
                current_selling,
                input.selling_price.unwrap_or(current_selling),
            ),
        }
    }

    fn validate(input: &ChangePrices) -> AppResult<()> {
        let mut v = Validator::new();
        if input.cost_price.is_none() && input.selling_price.is_none() {
            v.error("cost_price", "provide cost_price, selling_price or both");
        }
        if let Some(price) = input.cost_price {
            v.price("cost_price", price);
        }
        if let Some(price) = input.selling_price {
            v.price("selling_price", price);
        }
        v.optional("reason", input.reason.as_deref(), 500);
        v.finish()
    }

    /// One `(type, previous, new)` per price that actually changed.
    fn changes(&self) -> Vec<(PriceType, Money, Money)> {
        [
            (PriceType::Cost, self.cost),
            (PriceType::Selling, self.selling),
        ]
        .into_iter()
        .filter(|(_, (old, new))| old != new)
        .map(|(kind, (old, new))| (kind, old, new))
        .collect()
    }
}

#[allow(clippy::too_many_arguments)]
async fn record_price_changes(
    conn: &mut PgConnection,
    actor: UserId,
    business_id: BusinessId,
    product_id: ProductId,
    variant_id: Option<ProductVariantId>,
    update: &PriceUpdate,
    reason: Option<String>,
) -> AppResult<Vec<PriceHistory>> {
    let now = Utc::now();
    let mut entries = Vec::new();
    for (price_type, previous, new) in update.changes() {
        let entry = sqlx::query_as::<_, PriceHistory>(
            "INSERT INTO product_price_history (id, business_id, product_id, variant_id,
                                                price_type, previous_price, new_price,
                                                effective_at, changed_by, reason)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             RETURNING *",
        )
        .bind(PriceHistoryId::generate())
        .bind(business_id)
        .bind(product_id)
        .bind(variant_id)
        .bind(price_type)
        .bind(previous)
        .bind(new)
        .bind(now)
        .bind(actor)
        .bind(&reason)
        .fetch_one(&mut *conn)
        .await?;
        entries.push(entry);
    }
    Ok(entries)
}

fn price_audit(
    business_id: BusinessId,
    actor: UserId,
    record_type: RecordType,
    record_id: impl Into<uuid::Uuid>,
    update: &PriceUpdate,
    reason: Option<&str>,
) -> AuditEntry {
    let entry = AuditEntry::new(business_id, AuditAction::PriceChanged)
        .actor(actor)
        .record(record_type, record_id)
        .before(&serde_json::json!({
            "cost_price": update.cost.0, "selling_price": update.selling.0,
        }))
        .after(&serde_json::json!({
            "cost_price": update.cost.1, "selling_price": update.selling.1,
        }));
    match reason {
        Some(reason) => entry.details(reason),
        None => entry,
    }
}

/// Changes a product's cost and/or selling price, effective immediately.
/// Unchanged values are ignored; if nothing changes, nothing is recorded.
pub async fn change_product_prices(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    product_id: ProductId,
    input: ChangePrices,
) -> AppResult<Product> {
    PriceUpdate::validate(&input)?;
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id, product_id).await?;
    ensure_product_active(&before)?;
    let update = PriceUpdate::new(before.cost_price, before.selling_price, &input);
    if update.changes().is_empty() {
        return Ok(before);
    }
    let reason = clean_opt(input.reason);
    let mut p = before.clone();
    p.cost_price = update.cost.1;
    p.selling_price = update.selling.1;
    let after = save(&mut tx, &p).await?;
    record_price_changes(
        &mut tx,
        actor,
        business_id,
        product_id,
        None,
        &update,
        reason.clone(),
    )
    .await?;
    price_audit(
        business_id,
        actor,
        RecordType::Product,
        product_id,
        &update,
        reason.as_deref(),
    )
    .record_in(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// Changes a variant's cost and/or selling price, effective immediately.
pub async fn change_variant_prices(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    product_id: ProductId,
    variant_id: ProductVariantId,
    input: ChangePrices,
) -> AppResult<ProductVariant> {
    PriceUpdate::validate(&input)?;
    let mut tx = pool.begin().await?;
    let product = get(&mut *tx, business_id, product_id).await?;
    ensure_product_active(&product)?;
    let before = lock_variant(&mut tx, business_id, product_id, variant_id).await?;
    if before.status != RecordStatus::Active {
        return Err(AppError::Conflict(
            "variant is archived; restore it first".into(),
        ));
    }
    let update = PriceUpdate::new(before.cost_price, before.selling_price, &input);
    if update.changes().is_empty() {
        return Ok(before);
    }
    let reason = clean_opt(input.reason);
    let mut variant = before.clone();
    variant.cost_price = update.cost.1;
    variant.selling_price = update.selling.1;
    let after = save_variant(&mut tx, &variant).await?;
    record_price_changes(
        &mut tx,
        actor,
        business_id,
        product_id,
        Some(variant_id),
        &update,
        reason.clone(),
    )
    .await?;
    price_audit(
        business_id,
        actor,
        RecordType::ProductVariant,
        variant_id,
        &update,
        reason.as_deref(),
    )
    .record_in(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// Price changes for a product and its variants, newest first.
pub async fn price_history(
    pool: &PgPool,
    business_id: BusinessId,
    product_id: ProductId,
    filter: PriceHistoryFilter,
) -> AppResult<Vec<PriceHistory>> {
    get(pool, business_id, product_id).await?;
    let (limit, offset) = page(filter.limit, filter.offset);
    let entries = sqlx::query_as::<_, PriceHistory>(
        "SELECT * FROM product_price_history
         WHERE product_id = $1 AND business_id = $2
           AND ($3::uuid IS NULL OR variant_id = $3)
           AND ($4::text IS NULL OR price_type = $4)
         ORDER BY created_at DESC, id DESC
         LIMIT $5 OFFSET $6",
    )
    .bind(product_id)
    .bind(business_id)
    .bind(filter.variant_id)
    .bind(filter.price_type)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn only_changed_prices_are_recorded() {
        let input = ChangePrices {
            cost_price: Some(dec!(100.00)),
            selling_price: Some(dec!(150)),
            reason: None,
        };
        // Cost is the same value written differently; only selling changes.
        let update = PriceUpdate::new(dec!(100), dec!(120), &input);
        assert_eq!(
            update.changes(),
            vec![(PriceType::Selling, dec!(120), dec!(150))]
        );
    }

    #[test]
    fn a_price_change_needs_at_least_one_price() {
        assert!(PriceUpdate::validate(&ChangePrices::default()).is_err());
    }
}
