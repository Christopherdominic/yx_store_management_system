//! Product categories (FR-08).

use chrono::Utc;
use serde::Deserialize;
use sqlx::{PgConnection, PgExecutor, PgPool};

use crate::{
    errors::{AppError, AppResult},
    models::{
        AuditAction, BusinessId, ProductCategory, ProductCategoryId, RecordStatus, RecordType,
        UserId,
    },
    services::audit::AuditEntry,
    utils::validation::{Validator, clean, clean_opt, nullable, patch_opt},
};

#[derive(Debug, Deserialize)]
pub struct CreateCategory {
    pub name: String,
    /// Nests the category under another one; omit for a top-level category.
    pub parent_id: Option<ProductCategoryId>,
    pub description: Option<String>,
}

/// PATCH body: absent fields are unchanged; `""` clears the description and
/// `"parent_id": null` makes the category top-level.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateCategory {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub parent_id: Option<Option<ProductCategoryId>>,
    pub description: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CategoryFilter {
    pub status: Option<RecordStatus>,
    /// Only the direct children of this category.
    pub parent_id: Option<ProductCategoryId>,
}

fn validate(c: &ProductCategory, v: &mut Validator) {
    v.required("name", &c.name, 100);
    v.optional("description", c.description.as_deref(), 500);
}

pub async fn create(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    input: CreateCategory,
) -> AppResult<ProductCategory> {
    let now = Utc::now();
    let category = ProductCategory {
        id: ProductCategoryId::generate(),
        business_id,
        parent_id: input.parent_id,
        name: clean(input.name),
        description: clean_opt(input.description),
        status: RecordStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let mut v = Validator::new();
    validate(&category, &mut v);
    v.finish()?;

    let mut tx = pool.begin().await?;
    if let Some(parent_id) = category.parent_id {
        ensure_usable_parent(&mut tx, business_id, parent_id).await?;
    }
    let category = sqlx::query_as::<_, ProductCategory>(
        "INSERT INTO product_categories (id, business_id, parent_id, name, description, status)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING *",
    )
    .bind(category.id)
    .bind(business_id)
    .bind(category.parent_id)
    .bind(&category.name)
    .bind(&category.description)
    .bind(category.status)
    .fetch_one(&mut *tx)
    .await?;
    AuditEntry::new(business_id, AuditAction::Created)
        .actor(actor)
        .record(RecordType::ProductCategory, category.id)
        .after(&category)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(category)
}

pub async fn list(
    pool: &PgPool,
    business_id: BusinessId,
    filter: CategoryFilter,
) -> AppResult<Vec<ProductCategory>> {
    let categories = sqlx::query_as::<_, ProductCategory>(
        "SELECT * FROM product_categories
         WHERE business_id = $1
           AND ($2::text IS NULL OR status = $2)
           AND ($3::uuid IS NULL OR parent_id = $3)
         ORDER BY lower(name)",
    )
    .bind(business_id)
    .bind(filter.status)
    .bind(filter.parent_id)
    .fetch_all(pool)
    .await?;
    Ok(categories)
}

/// Fetches a category only if it belongs to `business_id`.
pub async fn get(
    executor: impl PgExecutor<'_>,
    business_id: BusinessId,
    category_id: ProductCategoryId,
) -> AppResult<ProductCategory> {
    sqlx::query_as::<_, ProductCategory>(
        "SELECT * FROM product_categories WHERE id = $1 AND business_id = $2",
    )
    .bind(category_id)
    .bind(business_id)
    .fetch_optional(executor)
    .await?
    .ok_or(AppError::NotFound("category"))
}

/// New products and subcategories may only be placed in active categories.
pub async fn ensure_active(
    executor: impl PgExecutor<'_>,
    business_id: BusinessId,
    category_id: ProductCategoryId,
    field: &str,
) -> AppResult<ProductCategory> {
    let category = get(executor, business_id, category_id)
        .await
        .map_err(|_| AppError::field(field, "category not found"))?;
    if category.status != RecordStatus::Active {
        return Err(AppError::field(field, "category is archived"));
    }
    Ok(category)
}

async fn ensure_usable_parent(
    conn: &mut PgConnection,
    business_id: BusinessId,
    parent_id: ProductCategoryId,
) -> AppResult<()> {
    ensure_active(conn, business_id, parent_id, "parent_id").await?;
    Ok(())
}

/// Whether `ancestor` is `category` itself or one of its ancestors.
async fn is_self_or_ancestor(
    conn: &mut PgConnection,
    category: ProductCategoryId,
    ancestor: ProductCategoryId,
) -> AppResult<bool> {
    let (found,): (bool,) = sqlx::query_as(
        "WITH RECURSIVE chain AS (
             SELECT id, parent_id FROM product_categories WHERE id = $1
             UNION ALL
             SELECT c.id, c.parent_id FROM product_categories c JOIN chain ON c.id = chain.parent_id
         )
         SELECT EXISTS (SELECT 1 FROM chain WHERE id = $2)",
    )
    .bind(category)
    .bind(ancestor)
    .fetch_one(conn)
    .await?;
    Ok(found)
}

async fn lock(
    conn: &mut PgConnection,
    business_id: BusinessId,
    category_id: ProductCategoryId,
) -> AppResult<ProductCategory> {
    sqlx::query_as::<_, ProductCategory>(
        "SELECT * FROM product_categories WHERE id = $1 AND business_id = $2 FOR UPDATE",
    )
    .bind(category_id)
    .bind(business_id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound("category"))
}

async fn save(conn: &mut PgConnection, c: &ProductCategory) -> AppResult<ProductCategory> {
    let saved = sqlx::query_as::<_, ProductCategory>(
        "UPDATE product_categories SET
            parent_id = $2, name = $3, description = $4, status = $5, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(c.id)
    .bind(c.parent_id)
    .bind(&c.name)
    .bind(&c.description)
    .bind(c.status)
    .fetch_one(conn)
    .await?;
    Ok(saved)
}

pub async fn update(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    category_id: ProductCategoryId,
    input: UpdateCategory,
) -> AppResult<ProductCategory> {
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id, category_id).await?;
    let mut c = before.clone();
    if let Some(name) = input.name {
        c.name = clean(name);
    }
    c.description = patch_opt(c.description, input.description);
    if let Some(parent_id) = input.parent_id {
        c.parent_id = parent_id;
    }
    let mut v = Validator::new();
    validate(&c, &mut v);
    v.finish()?;

    if let Some(parent_id) = c.parent_id
        && c.parent_id != before.parent_id
    {
        ensure_usable_parent(&mut tx, business_id, parent_id).await?;
        // Moving a category under itself or one of its own subcategories
        // would create a loop.
        if is_self_or_ancestor(&mut tx, parent_id, category_id).await? {
            return Err(AppError::field(
                "parent_id",
                "cannot move a category under itself or one of its subcategories",
            ));
        }
    }

    let after = save(&mut tx, &c).await?;
    AuditEntry::new(business_id, AuditAction::Updated)
        .actor(actor)
        .record(RecordType::ProductCategory, category_id)
        .before(&before)
        .after(&after)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

/// Archives or restores a category. Products keep their category either
/// way; archived categories just can't receive new products. Setting the
/// current status again is a no-op.
pub async fn set_status(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    category_id: ProductCategoryId,
    status: RecordStatus,
) -> AppResult<ProductCategory> {
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id, category_id).await?;
    if before.status == status {
        return Ok(before);
    }
    let mut c = before.clone();
    c.status = status;
    let after = save(&mut tx, &c).await?;
    AuditEntry::new(business_id, archive_action(status))
        .actor(actor)
        .record(RecordType::ProductCategory, category_id)
        .before(&serde_json::json!({ "status": before.status }))
        .after(&serde_json::json!({ "status": after.status }))
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

pub fn archive_action(status: RecordStatus) -> AuditAction {
    match status {
        RecordStatus::Active => AuditAction::Reactivated,
        RecordStatus::Archived => AuditAction::Archived,
    }
}
