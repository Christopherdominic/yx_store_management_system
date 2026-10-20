//! Business profile and business-wide settings (FR-01, FR-03).

use chrono::Utc;
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::{PgConnection, PgPool, types::Json};

use crate::{
    errors::{AppError, AppResult},
    models::{
        Address, AuditAction, Business, BusinessId, BusinessSettings, BusinessStatus, ContactInfo,
        CurrencyCode, RecordType, UserId,
    },
    services::audit::AuditEntry,
    utils::validation::{Validator, clean, clean_address, clean_opt, patch_opt},
};

pub const DEFAULT_CURRENCY: &str = "NGN";
pub const DEFAULT_TIMEZONE: &str = "Africa/Lagos";

#[derive(Debug, Deserialize)]
pub struct CreateBusiness {
    pub legal_name: String,
    pub trading_name: Option<String>,
    pub registration_number: Option<String>,
    pub address: Address,
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
    pub address: Option<Address>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub logo_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateBusinessSettings {
    pub base_currency: Option<String>,
    pub timezone: Option<String>,
    pub tax_name: Option<String>,
    /// Percentage as a decimal string, e.g. `"7.5"`.
    pub tax_rate: Option<Decimal>,
    pub prices_include_tax: Option<bool>,
    pub tax_id: Option<String>,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
}

fn validate_profile(b: &Business, v: &mut Validator) {
    v.required("legal_name", &b.legal_name, 200);
    v.optional("trading_name", b.trading_name.as_deref(), 200);
    v.optional("registration_number", b.registration_number.as_deref(), 50);
    v.address("address", &b.address);
    v.optional("phone", b.contact.phone.as_deref(), 30);
    v.email("email", b.contact.email.as_deref());
    v.optional("logo_url", b.logo_url.as_deref(), 2048);
    if let Some(url) = &b.logo_url {
        v.check(
            url.starts_with("https://") || url.starts_with("http://"),
            "logo_url",
            "must be an http(s) URL",
        );
    }
    v.timezone("timezone", &b.timezone);
}

/// Cleans and validates a new business, recording problems in `v`. The
/// result must only be used once `v.finish()` succeeds.
pub fn prepare(input: CreateBusiness, v: &mut Validator) -> Business {
    let currency = input.base_currency.as_deref().unwrap_or(DEFAULT_CURRENCY);
    let base_currency = v.currency("base_currency", currency);
    let now = Utc::now();
    let business = Business {
        id: BusinessId::generate(),
        legal_name: clean(input.legal_name),
        trading_name: clean_opt(input.trading_name),
        registration_number: clean_opt(input.registration_number),
        address: clean_address(input.address),
        contact: ContactInfo {
            phone: clean_opt(input.phone),
            email: clean_opt(input.email),
        },
        logo_url: clean_opt(input.logo_url),
        // Placeholder when invalid; the recorded error stops the request.
        base_currency: base_currency.unwrap_or_else(|| CurrencyCode::new("XXX").expect("valid")),
        timezone: input
            .timezone
            .map(clean)
            .unwrap_or_else(|| DEFAULT_TIMEZONE.into()),
        status: BusinessStatus::Active,
        created_at: now,
        updated_at: now,
    };
    validate_profile(&business, v);
    business
}

/// Inserts a prepared business with default settings.
pub async fn insert(conn: &mut PgConnection, b: &Business) -> AppResult<Business> {
    let business = sqlx::query_as::<_, Business>(
        "INSERT INTO businesses (id, legal_name, trading_name, registration_number, address, phone,
                                 email, logo_url, base_currency, timezone, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING *",
    )
    .bind(b.id)
    .bind(&b.legal_name)
    .bind(&b.trading_name)
    .bind(&b.registration_number)
    .bind(Json(&b.address))
    .bind(&b.contact.phone)
    .bind(&b.contact.email)
    .bind(&b.logo_url)
    .bind(&b.base_currency)
    .bind(&b.timezone)
    .bind(b.status)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("INSERT INTO business_settings (business_id) VALUES ($1)")
        .bind(business.id)
        .execute(&mut *conn)
        .await?;
    Ok(business)
}

pub async fn get(pool: &PgPool, business_id: BusinessId) -> AppResult<Business> {
    sqlx::query_as::<_, Business>("SELECT * FROM businesses WHERE id = $1")
        .bind(business_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound("business"))
}

async fn lock(conn: &mut PgConnection, business_id: BusinessId) -> AppResult<Business> {
    sqlx::query_as::<_, Business>("SELECT * FROM businesses WHERE id = $1 FOR UPDATE")
        .bind(business_id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound("business"))
}

async fn save(conn: &mut PgConnection, b: &Business) -> AppResult<Business> {
    let saved = sqlx::query_as::<_, Business>(
        "UPDATE businesses SET
            legal_name = $2, trading_name = $3, registration_number = $4, address = $5,
            phone = $6, email = $7, logo_url = $8, base_currency = $9, timezone = $10,
            updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(b.id)
    .bind(&b.legal_name)
    .bind(&b.trading_name)
    .bind(&b.registration_number)
    .bind(Json(&b.address))
    .bind(&b.contact.phone)
    .bind(&b.contact.email)
    .bind(&b.logo_url)
    .bind(&b.base_currency)
    .bind(&b.timezone)
    .fetch_one(conn)
    .await?;
    Ok(saved)
}

pub async fn update_profile(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    input: UpdateBusinessProfile,
) -> AppResult<Business> {
    let mut tx = pool.begin().await?;
    let before = lock(&mut tx, business_id).await?;
    let mut b = before.clone();
    if let Some(v) = input.legal_name {
        b.legal_name = clean(v);
    }
    if let Some(v) = input.address {
        b.address = clean_address(v);
    }
    b.trading_name = patch_opt(b.trading_name, input.trading_name);
    b.registration_number = patch_opt(b.registration_number, input.registration_number);
    b.contact.phone = patch_opt(b.contact.phone, input.phone);
    b.contact.email = patch_opt(b.contact.email, input.email);
    b.logo_url = patch_opt(b.logo_url, input.logo_url);
    let mut v = Validator::new();
    validate_profile(&b, &mut v);
    v.finish()?;

    let after = save(&mut tx, &b).await?;
    AuditEntry::new(business_id, AuditAction::Updated)
        .actor(actor)
        .record(RecordType::Business, business_id)
        .before(&before)
        .after(&after)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

async fn read_settings(
    conn: &mut PgConnection,
    business_id: BusinessId,
    for_update: bool,
) -> AppResult<BusinessSettings> {
    let query = if for_update {
        "SELECT b.id AS business_id, b.base_currency, b.timezone, s.tax_name, s.tax_rate,
                s.prices_include_tax, s.tax_id, s.receipt_header, s.receipt_footer, s.updated_at
         FROM businesses b JOIN business_settings s ON s.business_id = b.id
         WHERE b.id = $1
         FOR UPDATE"
    } else {
        "SELECT b.id AS business_id, b.base_currency, b.timezone, s.tax_name, s.tax_rate,
                s.prices_include_tax, s.tax_id, s.receipt_header, s.receipt_footer, s.updated_at
         FROM businesses b JOIN business_settings s ON s.business_id = b.id
         WHERE b.id = $1"
    };
    sqlx::query_as::<_, BusinessSettings>(query)
        .bind(business_id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound("business"))
}

pub async fn get_settings(pool: &PgPool, business_id: BusinessId) -> AppResult<BusinessSettings> {
    let mut conn = pool.acquire().await?;
    read_settings(&mut conn, business_id, false).await
}

/// Settings take effect for transactions created after the change; stored
/// transactions keep the values captured when they were recorded, so this
/// never rewrites history (FR-03, FR-30).
pub async fn update_settings(
    pool: &PgPool,
    actor: UserId,
    business_id: BusinessId,
    input: UpdateBusinessSettings,
) -> AppResult<BusinessSettings> {
    let mut tx = pool.begin().await?;
    let before = read_settings(&mut tx, business_id, true).await?;
    let mut s = before.clone();
    let mut v = Validator::new();
    if let Some(code) = input.base_currency
        && let Some(code) = v.currency("base_currency", &code)
    {
        s.base_currency = code;
    }
    if let Some(tz) = input.timezone {
        s.timezone = clean(tz);
    }
    if let Some(name) = input.tax_name {
        s.tax_name = clean(name);
    }
    if let Some(rate) = input.tax_rate {
        s.tax_rate = rate;
    }
    if let Some(inclusive) = input.prices_include_tax {
        s.prices_include_tax = inclusive;
    }
    s.tax_id = patch_opt(s.tax_id, input.tax_id);
    s.receipt_header = patch_opt(s.receipt_header, input.receipt_header);
    s.receipt_footer = patch_opt(s.receipt_footer, input.receipt_footer);

    v.timezone("timezone", &s.timezone);
    v.required("tax_name", &s.tax_name, 30);
    v.check(
        s.tax_rate >= Decimal::ZERO
            && s.tax_rate <= Decimal::ONE_HUNDRED
            && s.tax_rate.scale() <= 4,
        "tax_rate",
        "must be a percentage between 0 and 100 with at most 4 decimal places",
    );
    v.optional("tax_id", s.tax_id.as_deref(), 50);
    v.optional("receipt_header", s.receipt_header.as_deref(), 500);
    v.optional("receipt_footer", s.receipt_footer.as_deref(), 500);
    v.finish()?;

    sqlx::query(
        "UPDATE businesses SET base_currency = $2, timezone = $3, updated_at = now() WHERE id = $1",
    )
    .bind(business_id)
    .bind(&s.base_currency)
    .bind(&s.timezone)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE business_settings SET
            tax_name = $2, tax_rate = $3, prices_include_tax = $4, tax_id = $5,
            receipt_header = $6, receipt_footer = $7, updated_at = now()
         WHERE business_id = $1",
    )
    .bind(business_id)
    .bind(&s.tax_name)
    .bind(s.tax_rate)
    .bind(s.prices_include_tax)
    .bind(&s.tax_id)
    .bind(&s.receipt_header)
    .bind(&s.receipt_footer)
    .execute(&mut *tx)
    .await?;
    let after = read_settings(&mut tx, business_id, false).await?;
    AuditEntry::new(business_id, AuditAction::SettingsChanged)
        .actor(actor)
        .record(RecordType::Business, business_id)
        .before(&before)
        .after(&after)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}
