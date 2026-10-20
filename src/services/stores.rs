//! Stores and store-level settings (FR-02, FR-03).

use chrono::Utc;
use serde::Deserialize;
use sqlx::{PgConnection, PgExecutor, PgPool, types::Json};

use crate::{
    errors::{AppError, AppResult},
    models::{
        Address, AuditAction, BusinessId, ContactInfo, RecordType, Store, StoreId, StoreSettings,
        StoreStatus, UserId,
    },
    services::audit::AuditEntry,
    utils::validation::{Validator, clean, clean_address, clean_opt, like_pattern, patch_opt},
};

#[derive(Debug, Deserialize)]
pub struct CreateStore {
    /// Short identifier unique within the business, e.g. "LAG".
    pub code: String,
    pub name: String,
    pub address: Address,
    pub phone: Option<String>,
    pub email: Option<String>,
    /// Defaults to the store code.
    pub receipt_prefix: Option<String>,
    /// Defaults to the store code.
    pub transaction_prefix: Option<String>,
    pub receipt_footer: Option<String>,
}

/// PATCH body: absent fields are unchanged; `""` clears an optional field.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateStore {
    pub code: Option<String>,
    pub name: Option<String>,
    pub address: Option<Address>,
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

fn validate_store(s: &Store, v: &mut Validator) {
    v.code("code", &s.code, 20);
    v.required("name", &s.name, 200);
    v.address("address", &s.address);
    v.optional("phone", s.contact.phone.as_deref(), 30);
    v.email("email", s.contact.email.as_deref());
}

fn validate_settings(s: &StoreSettings, v: &mut Validator) {
    v.code("receipt_prefix", &s.receipt_prefix, 10);
    v.code("transaction_prefix", &s.transaction_prefix, 10);
    v.optional("receipt_footer", s.receipt_footer.as_deref(), 500);
}

fn code(value: String) -> String {
    value.trim().to_uppercase()
}

pub async fn create(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    input: CreateStore,
) -> AppResult<Store> {
    let now = Utc::now();
    let store = Store {
        id: StoreId::generate(),
        business_id,
        code: code(input.code),
        name: clean(input.name),
        address: clean_address(input.address),
        contact: ContactInfo {
            phone: clean_opt(input.phone),
            email: clean_opt(input.email),
        },
        status: StoreStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let settings = StoreSettings {
        store_id: store.id,
        receipt_prefix: input
            .receipt_prefix
            .map(code)
            .unwrap_or_else(|| store.code.clone()),
        transaction_prefix: input
            .transaction_prefix
            .map(code)
            .unwrap_or_else(|| store.code.clone()),
        receipt_footer: clean_opt(input.receipt_footer),
        updated_at: now,
    };
    let mut v = Validator::new();
    validate_store(&store, &mut v);
    validate_settings(&settings, &mut v);
    v.finish()?;

    let mut tx = pool.begin().await?;
    let store = sqlx::query_as::<_, Store>(
        "INSERT INTO stores (id, business_id, code, name, address, phone, email, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING *",
    )
    .bind(store.id)
    .bind(business_id)
    .bind(&store.code)
    .bind(&store.name)
    .bind(Json(&store.address))
    .bind(&store.contact.phone)
    .bind(&store.contact.email)
    .bind(store.status)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO store_settings (store_id, receipt_prefix, transaction_prefix, receipt_footer)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(store.id)
    .bind(&settings.receipt_prefix)
    .bind(&settings.transaction_prefix)
    .bind(&settings.receipt_footer)
    .execute(&mut *tx)
    .await?;
    AuditEntry::new(business_id, AuditAction::Created)
        .store(Some(store.id))
        .actor(actor)
        .record(RecordType::Store, store.id)
        .after(&store)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(store)
}

/// Lists stores in a business. `visible` limits the result to the given
/// store IDs (`None` = all stores).
pub async fn list(
    pool: &PgPool,
    business_id: BusinessId,
    visible: Option<Vec<StoreId>>,
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
    business_id: BusinessId,
    store_id: StoreId,
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
pub async fn ensure_operational(
    executor: impl PgExecutor<'_>,
    business_id: BusinessId,
    store_id: StoreId,
) -> AppResult<Store> {
    let store = get(executor, business_id, store_id).await?;
    if !store.is_operational() {
        return Err(AppError::Conflict(
            "store is inactive and cannot accept new transactions".into(),
        ));
    }
    Ok(store)
}

async fn lock(
    conn: &mut PgConnection,
    business_id: BusinessId,
    store_id: StoreId,
) -> AppResult<Store> {
    sqlx::query_as::<_, Store>("SELECT * FROM stores WHERE id = $1 AND business_id = $2 FOR UPDATE")
        .bind(store_id)
        .bind(business_id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound("store"))
}

async fn save(conn: &mut PgConnection, s: &Store) -> AppResult<Store> {
    let saved = sqlx::query_as::<_, Store>(
        "UPDATE stores SET
            code = $2, name = $3, address = $4, phone = $5, email = $6, status = $7,
            updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(s.id)
    .bind(&s.code)
    .bind(&s.name)
    .bind(Json(&s.address))
    .bind(&s.contact.phone)
    .bind(&s.contact.email)
    .bind(s.status)
    .fetch_one(conn)
    .await?;
    Ok(saved)
}

pub async fn update(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    store_id: StoreId,
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
        s.address = clean_address(v);
    }
    s.contact.phone = patch_opt(s.contact.phone, input.phone);
    s.contact.email = patch_opt(s.contact.email, input.email);
    let mut v = Validator::new();
    validate_store(&s, &mut v);
    v.finish()?;

    let after = save(&mut tx, &s).await?;
    AuditEntry::new(business_id, AuditAction::Updated)
        .store(Some(store_id))
        .actor(actor)
        .record(RecordType::Store, store_id)
        .before(&before)
        .after(&after)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

/// Activates or deactivates a store. Deactivation never deletes the store or
/// its history. Setting the current status again is a no-op.
pub async fn set_status(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    store_id: StoreId,
    status: StoreStatus,
) -> AppResult<Store> {
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id, store_id).await?;
    if before.status == status {
        return Ok(before);
    }
    let mut s = before.clone();
    s.status = status;
    let after = save(&mut tx, &s).await?;
    let action = match status {
        StoreStatus::Active => AuditAction::Reactivated,
        StoreStatus::Inactive => AuditAction::Deactivated,
    };
    AuditEntry::new(business_id, action)
        .store(Some(store_id))
        .actor(actor)
        .record(RecordType::Store, store_id)
        .before(&serde_json::json!({ "status": before.status }))
        .after(&serde_json::json!({ "status": after.status }))
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

async fn read_settings(
    conn: &mut PgConnection,
    business_id: BusinessId,
    store_id: StoreId,
    for_update: bool,
) -> AppResult<StoreSettings> {
    let query = if for_update {
        "SELECT ss.* FROM store_settings ss JOIN stores s ON s.id = ss.store_id
         WHERE ss.store_id = $1 AND s.business_id = $2
         FOR UPDATE OF ss"
    } else {
        "SELECT ss.* FROM store_settings ss JOIN stores s ON s.id = ss.store_id
         WHERE ss.store_id = $1 AND s.business_id = $2"
    };
    sqlx::query_as::<_, StoreSettings>(query)
        .bind(store_id)
        .bind(business_id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound("store"))
}

pub async fn get_settings(
    pool: &PgPool,
    business_id: BusinessId,
    store_id: StoreId,
) -> AppResult<StoreSettings> {
    let mut conn = pool.acquire().await?;
    read_settings(&mut conn, business_id, store_id, false).await
}

pub async fn update_settings(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    store_id: StoreId,
    input: UpdateStoreSettings,
) -> AppResult<StoreSettings> {
    let mut tx = pool.begin().await?;
    let before = read_settings(&mut tx, business_id, store_id, true).await?;
    let mut s = before.clone();
    if let Some(v) = input.receipt_prefix {
        s.receipt_prefix = code(v);
    }
    if let Some(v) = input.transaction_prefix {
        s.transaction_prefix = code(v);
    }
    s.receipt_footer = patch_opt(s.receipt_footer, input.receipt_footer);
    let mut v = Validator::new();
    validate_settings(&s, &mut v);
    v.finish()?;

    let after = sqlx::query_as::<_, StoreSettings>(
        "UPDATE store_settings SET
            receipt_prefix = $2, transaction_prefix = $3, receipt_footer = $4, updated_at = now()
         WHERE store_id = $1
         RETURNING *",
    )
    .bind(store_id)
    .bind(&s.receipt_prefix)
    .bind(&s.transaction_prefix)
    .bind(&s.receipt_footer)
    .fetch_one(&mut *tx)
    .await?;
    AuditEntry::new(business_id, AuditAction::SettingsChanged)
        .store(Some(store_id))
        .actor(actor)
        .record(RecordType::Store, store_id)
        .before(&before)
        .after(&after)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}
