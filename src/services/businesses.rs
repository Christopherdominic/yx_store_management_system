//! Business profile and business-wide settings (FR-01, FR-03).

use chrono::Utc;
use serde::Deserialize;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    models::{
        business::{Business, BusinessSettings},
        roles::BusinessRole,
    },
    services::audit::AuditEntry,
    utils::validation::{Validator, clean, clean_opt, patch_opt},
};

#[derive(Debug, Deserialize)]
pub struct CreateBusiness {
    pub legal_name: String,
    pub trading_name: String,
    pub registration_number: Option<String>,
    pub address: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub logo_url: Option<String>,
    pub base_currency: Option<String>,
    pub timezone: Option<String>,
}

/// PATCH body: absent fields are unchanged; `""` clears an optional field.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateBusinessProfile {
    pub legal_name: Option<String>,
    pub trading_name: Option<String>,
    pub registration_number: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub logo_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateBusinessSettings {
    pub base_currency: Option<String>,
    pub timezone: Option<String>,
    pub tax_name: Option<String>,
    pub tax_rate_bps: Option<i32>,
    pub prices_include_tax: Option<bool>,
    pub tax_id: Option<String>,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
}

fn validate(b: &Business) -> AppResult<()> {
    let mut v = Validator::new();
    v.required("legal_name", &b.legal_name, 200);
    v.required("trading_name", &b.trading_name, 200);
    v.optional("registration_number", b.registration_number.as_deref(), 50);
    v.required("address", &b.address, 500);
    v.optional("phone", b.phone.as_deref(), 30);
    v.email("email", b.email.as_deref());
    v.optional("logo_url", b.logo_url.as_deref(), 2048);
    if let Some(url) = &b.logo_url {
        v.check(
            url.starts_with("https://") || url.starts_with("http://"),
            "logo_url",
            "must be an http(s) URL",
        );
    }
    v.currency("base_currency", &b.base_currency);
    v.timezone("timezone", &b.timezone);
    v.required("tax_name", &b.tax_name, 30);
    v.check(
        (0..=10_000).contains(&b.tax_rate_bps),
        "tax_rate_bps",
        "must be between 0 and 10000 basis points (0%-100%)",
    );
    v.optional("tax_id", b.tax_id.as_deref(), 50);
    v.optional("receipt_header", b.receipt_header.as_deref(), 500);
    v.optional("receipt_footer", b.receipt_footer.as_deref(), 500);
    v.finish()
}

/// Creates a business and makes the creator its owner.
pub async fn create(pool: &PgPool, actor: Uuid, input: CreateBusiness) -> AppResult<Business> {
    let now = Utc::now();
    let business = Business {
        id: Uuid::new_v4(),
        legal_name: clean(input.legal_name),
        trading_name: clean(input.trading_name),
        registration_number: clean_opt(input.registration_number),
        address: clean(input.address),
        phone: clean_opt(input.phone),
        email: clean_opt(input.email),
        logo_url: clean_opt(input.logo_url),
        base_currency: input
            .base_currency
            .map(|c| c.trim().to_uppercase())
            .unwrap_or_else(|| "NGN".into()),
        timezone: input
            .timezone
            .map(clean)
            .unwrap_or_else(|| "Africa/Lagos".into()),
        tax_name: "VAT".into(),
        tax_rate_bps: 0,
        prices_include_tax: false,
        tax_id: None,
        receipt_header: None,
        receipt_footer: None,
        created_by: actor,
        created_at: now,
        updated_by: actor,
        updated_at: now,
    };
    validate(&business)?;

    let mut tx = pool.begin().await?;
    let business = sqlx::query_as::<_, Business>(
        "INSERT INTO businesses (id, legal_name, trading_name, registration_number, address, phone,
                                 email, logo_url, base_currency, timezone, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
         RETURNING *",
    )
    .bind(business.id)
    .bind(&business.legal_name)
    .bind(&business.trading_name)
    .bind(&business.registration_number)
    .bind(&business.address)
    .bind(&business.phone)
    .bind(&business.email)
    .bind(&business.logo_url)
    .bind(&business.base_currency)
    .bind(&business.timezone)
    .bind(actor)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO business_members (business_id, user_id, business_role) VALUES ($1, $2, $3)",
    )
    .bind(business.id)
    .bind(actor)
    .bind(BusinessRole::BusinessOwner)
    .execute(&mut *tx)
    .await?;
    AuditEntry::new("business.created", "business")
        .business(business.id)
        .actor(actor)
        .entity(business.id)
        .after(&business)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(business)
}

/// Businesses where the user is an active member.
pub async fn list_for_user(pool: &PgPool, user_id: Uuid) -> AppResult<Vec<Business>> {
    let businesses = sqlx::query_as::<_, Business>(
        "SELECT b.* FROM businesses b
         JOIN business_members bm ON bm.business_id = b.id
         WHERE bm.user_id = $1 AND bm.is_active
         ORDER BY b.trading_name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(businesses)
}

pub async fn get(pool: &PgPool, business_id: Uuid) -> AppResult<Business> {
    sqlx::query_as::<_, Business>("SELECT * FROM businesses WHERE id = $1")
        .bind(business_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound("business"))
}

async fn lock(conn: &mut PgConnection, business_id: Uuid) -> AppResult<Business> {
    sqlx::query_as::<_, Business>("SELECT * FROM businesses WHERE id = $1 FOR UPDATE")
        .bind(business_id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound("business"))
}

async fn save(conn: &mut PgConnection, b: &Business, actor: Uuid) -> AppResult<Business> {
    let saved = sqlx::query_as::<_, Business>(
        "UPDATE businesses SET
            legal_name = $2, trading_name = $3, registration_number = $4, address = $5,
            phone = $6, email = $7, logo_url = $8, base_currency = $9, timezone = $10,
            tax_name = $11, tax_rate_bps = $12, prices_include_tax = $13, tax_id = $14,
            receipt_header = $15, receipt_footer = $16, updated_by = $17, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(b.id)
    .bind(&b.legal_name)
    .bind(&b.trading_name)
    .bind(&b.registration_number)
    .bind(&b.address)
    .bind(&b.phone)
    .bind(&b.email)
    .bind(&b.logo_url)
    .bind(&b.base_currency)
    .bind(&b.timezone)
    .bind(&b.tax_name)
    .bind(b.tax_rate_bps)
    .bind(b.prices_include_tax)
    .bind(&b.tax_id)
    .bind(&b.receipt_header)
    .bind(&b.receipt_footer)
    .bind(actor)
    .fetch_one(conn)
    .await?;
    Ok(saved)
}

pub async fn update_profile(
    pool: &PgPool,
    actor: Uuid,
    business_id: Uuid,
    input: UpdateBusinessProfile,
) -> AppResult<Business> {
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id).await?;
    let mut b = before.clone();
    if let Some(v) = input.legal_name {
        b.legal_name = clean(v);
    }
    if let Some(v) = input.trading_name {
        b.trading_name = clean(v);
    }
    if let Some(v) = input.address {
        b.address = clean(v);
    }
    b.registration_number = patch_opt(b.registration_number, input.registration_number);
    b.phone = patch_opt(b.phone, input.phone);
    b.email = patch_opt(b.email, input.email);
    b.logo_url = patch_opt(b.logo_url, input.logo_url);
    validate(&b)?;

    let after = save(&mut tx, &b, actor).await?;
    AuditEntry::new("business.updated", "business")
        .business(business_id)
        .actor(actor)
        .entity(business_id)
        .before(&before)
        .after(&after)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

/// Settings take effect for transactions created after the change; stored
/// transactions keep the values captured when they were recorded, so this
/// never rewrites history (FR-03, FR-30).
pub async fn update_settings(
    pool: &PgPool,
    actor: Uuid,
    business_id: Uuid,
    input: UpdateBusinessSettings,
) -> AppResult<BusinessSettings> {
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id).await?;
    let mut b = before.clone();
    if let Some(v) = input.base_currency {
        b.base_currency = v.trim().to_uppercase();
    }
    if let Some(v) = input.timezone {
        b.timezone = clean(v);
    }
    if let Some(v) = input.tax_name {
        b.tax_name = clean(v);
    }
    if let Some(v) = input.tax_rate_bps {
        b.tax_rate_bps = v;
    }
    if let Some(v) = input.prices_include_tax {
        b.prices_include_tax = v;
    }
    b.tax_id = patch_opt(b.tax_id, input.tax_id);
    b.receipt_header = patch_opt(b.receipt_header, input.receipt_header);
    b.receipt_footer = patch_opt(b.receipt_footer, input.receipt_footer);
    b.phone = patch_opt(b.phone, input.phone);
    b.email = patch_opt(b.email, input.email);
    validate(&b)?;

    let after = save(&mut tx, &b, actor).await?;
    let (before, after) = (
        BusinessSettings::from(&before),
        BusinessSettings::from(&after),
    );
    AuditEntry::new("business.settings_updated", "business")
        .business(business_id)
        .actor(actor)
        .entity(business_id)
        .before(&before)
        .after(&after)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}
