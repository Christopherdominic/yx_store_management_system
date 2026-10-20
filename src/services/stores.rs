//! Stores and store-level settings (FR-02, FR-03).

use chrono::Utc;
use serde::Deserialize;
use sqlx::{PgConnection, PgExecutor, PgPool};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    models::store::{Store, StoreSettings, StoreStatus},
    services::audit::AuditEntry,
    utils::validation::{Validator, clean, clean_opt, like_pattern, patch_opt},
};

#[derive(Debug, Deserialize)]
pub struct CreateStore {
    /// Short identifier unique within the business, e.g. "LAG-01".
    pub code: String,
    pub name: String,
    pub address: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub receipt_prefix: Option<String>,
    pub transaction_prefix: Option<String>,
    pub receipt_footer: Option<String>,
}

/// PATCH body: absent fields are unchanged; `""` clears an optional field.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateStore {
    pub code: Option<String>,
    pub name: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateStoreSettings {
    pub receipt_prefix: Option<String>,
    pub transaction_prefix: Option<String>,
    pub receipt_footer: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct StoreFilter {
    /// Matches store name or code (partial), or the exact store ID.
    pub q: Option<String>,
    pub status: Option<StoreStatus>,
}

fn validate(s: &Store) -> AppResult<()> {
    let mut v = Validator::new();
    v.code("code", &s.code, 20);
    v.required("name", &s.name, 200);
    v.required("address", &s.address, 500);
    v.optional("phone", s.phone.as_deref(), 30);
    v.email("email", s.email.as_deref());
    v.code("receipt_prefix", &s.receipt_prefix, 10);
    v.code("transaction_prefix", &s.transaction_prefix, 10);
    v.optional("receipt_footer", s.receipt_footer.as_deref(), 500);
    v.finish()
}

fn code(value: String) -> String {
    value.trim().to_uppercase()
}

pub async fn create(
    pool: &PgPool,
    actor: Uuid,
    business_id: Uuid,
    input: CreateStore,
) -> AppResult<Store> {
    let now = Utc::now();
    let store = Store {
        id: Uuid::new_v4(),
        business_id,
        code: code(input.code),
        name: clean(input.name),
        address: clean(input.address),
        phone: clean_opt(input.phone),
        email: clean_opt(input.email),
        status: StoreStatus::Active,
        receipt_prefix: input
            .receipt_prefix
            .map(code)
            .unwrap_or_else(|| "RCP".into()),
        transaction_prefix: input
            .transaction_prefix
            .map(code)
            .unwrap_or_else(|| "TXN".into()),
        receipt_footer: clean_opt(input.receipt_footer),
        created_by: actor,
        created_at: now,
        updated_by: actor,
        updated_at: now,
    };
    validate(&store)?;

    let mut tx = pool.begin().await?;
    let store = sqlx::query_as::<_, Store>(
        "INSERT INTO stores (id, business_id, code, name, address, phone, email, status,
                             receipt_prefix, transaction_prefix, receipt_footer, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)
         RETURNING *",
    )
    .bind(store.id)
    .bind(business_id)
    .bind(&store.code)
    .bind(&store.name)
    .bind(&store.address)
    .bind(&store.phone)
    .bind(&store.email)
    .bind(store.status)
    .bind(&store.receipt_prefix)
    .bind(&store.transaction_prefix)
    .bind(&store.receipt_footer)
    .bind(actor)
    .fetch_one(&mut *tx)
    .await?;
    AuditEntry::new("store.created", "store")
        .business(business_id)
        .store(Some(store.id))
        .actor(actor)
        .entity(store.id)
        .after(&store)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(store)
}

/// Lists stores in a business. `visible` limits the result to the given
/// store IDs (`None` = all stores).
pub async fn list(
    pool: &PgPool,
    business_id: Uuid,
    visible: Option<Vec<Uuid>>,
    filter: StoreFilter,
) -> AppResult<Vec<Store>> {
    let term = filter
        .q
        .map(|q| q.trim().to_string())
        .filter(|q| !q.is_empty());
    let stores = sqlx::query_as::<_, Store>(
        "SELECT * FROM stores
         WHERE business_id = $1
           AND ($2::uuid[] IS NULL OR id = ANY($2))
           AND ($3::text IS NULL OR name ILIKE $4 OR code ILIKE $4 OR id::text = lower($3))
           AND ($5::text IS NULL OR status = $5)
         ORDER BY name",
    )
    .bind(business_id)
    .bind(visible)
    .bind(&term)
    .bind(term.as_deref().map(like_pattern))
    .bind(filter.status)
    .fetch_all(pool)
    .await?;
    Ok(stores)
}

/// Fetches a store only if it belongs to `business_id`.
pub async fn get(
    executor: impl PgExecutor<'_>,
    business_id: Uuid,
    store_id: Uuid,
) -> AppResult<Store> {
    sqlx::query_as::<_, Store>("SELECT * FROM stores WHERE id = $1 AND business_id = $2")
        .bind(store_id)
        .bind(business_id)
        .fetch_optional(executor)
        .await?
        .ok_or(AppError::NotFound("store"))
}

/// For transactional modules (sales, stock movements, purchases, transfers):
/// a deactivated store must not accept new operations (FR-02).
pub async fn ensure_active(
    executor: impl PgExecutor<'_>,
    business_id: Uuid,
    store_id: Uuid,
) -> AppResult<Store> {
    let store = get(executor, business_id, store_id).await?;
    if store.status != StoreStatus::Active {
        return Err(AppError::Conflict(
            "store is inactive and cannot accept new transactions".into(),
        ));
    }
    Ok(store)
}

async fn lock(conn: &mut PgConnection, business_id: Uuid, store_id: Uuid) -> AppResult<Store> {
    sqlx::query_as::<_, Store>("SELECT * FROM stores WHERE id = $1 AND business_id = $2 FOR UPDATE")
        .bind(store_id)
        .bind(business_id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound("store"))
}

async fn save(conn: &mut PgConnection, s: &Store, actor: Uuid) -> AppResult<Store> {
    let saved = sqlx::query_as::<_, Store>(
        "UPDATE stores SET
            code = $2, name = $3, address = $4, phone = $5, email = $6, status = $7,
            receipt_prefix = $8, transaction_prefix = $9, receipt_footer = $10,
            updated_by = $11, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(s.id)
    .bind(&s.code)
    .bind(&s.name)
    .bind(&s.address)
    .bind(&s.phone)
    .bind(&s.email)
    .bind(s.status)
    .bind(&s.receipt_prefix)
    .bind(&s.transaction_prefix)
    .bind(&s.receipt_footer)
    .bind(actor)
    .fetch_one(conn)
    .await?;
    Ok(saved)
}

pub async fn update(
    pool: &PgPool,
    actor: Uuid,
    business_id: Uuid,
    store_id: Uuid,
    input: UpdateStore,
) -> AppResult<Store> {
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id, store_id).await?;
    let mut s = before.clone();
    if let Some(v) = input.code {
        s.code = code(v);
    }
    if let Some(v) = input.name {
        s.name = clean(v);
    }
    if let Some(v) = input.address {
        s.address = clean(v);
    }
    s.phone = patch_opt(s.phone, input.phone);
    s.email = patch_opt(s.email, input.email);
    validate(&s)?;

    let after = save(&mut tx, &s, actor).await?;
    AuditEntry::new("store.updated", "store")
        .business(business_id)
        .store(Some(store_id))
        .actor(actor)
        .entity(store_id)
        .before(&before)
        .after(&after)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

/// Activates or deactivates a store. Deactivation never deletes the store or
/// its history. Setting the current status again is a no-op.
pub async fn set_status(
    pool: &PgPool,
    actor: Uuid,
    business_id: Uuid,
    store_id: Uuid,
    status: StoreStatus,
) -> AppResult<Store> {
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id, store_id).await?;
    if before.status == status {
        return Ok(before);
    }
    let mut s = before.clone();
    s.status = status;
    let after = save(&mut tx, &s, actor).await?;
    let action = match status {
        StoreStatus::Active => "store.activated",
        StoreStatus::Inactive => "store.deactivated",
    };
    AuditEntry::new(action, "store")
        .business(business_id)
        .store(Some(store_id))
        .actor(actor)
        .entity(store_id)
        .before(&serde_json::json!({ "status": before.status }))
        .after(&serde_json::json!({ "status": after.status }))
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

pub async fn update_settings(
    pool: &PgPool,
    actor: Uuid,
    business_id: Uuid,
    store_id: Uuid,
    input: UpdateStoreSettings,
) -> AppResult<StoreSettings> {
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id, store_id).await?;
    let mut s = before.clone();
    if let Some(v) = input.receipt_prefix {
        s.receipt_prefix = code(v);
    }
    if let Some(v) = input.transaction_prefix {
        s.transaction_prefix = code(v);
    }
    s.receipt_footer = patch_opt(s.receipt_footer, input.receipt_footer);
    validate(&s)?;

    let after = save(&mut tx, &s, actor).await?;
    let (before, after) = (StoreSettings::from(&before), StoreSettings::from(&after));
    AuditEntry::new("store.settings_updated", "store")
        .business(business_id)
        .store(Some(store_id))
        .actor(actor)
        .entity(store_id)
        .before(&before)
        .after(&after)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}
