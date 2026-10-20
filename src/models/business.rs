use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Business {
    pub id: Uuid,
    pub legal_name: String,
    pub trading_name: String,
    pub registration_number: Option<String>,
    pub address: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub logo_url: Option<String>,
    pub base_currency: String,
    pub timezone: String,
    pub tax_name: String,
    pub tax_rate_bps: i32,
    pub prices_include_tax: bool,
    pub tax_id: Option<String>,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_by: Uuid,
    pub updated_at: DateTime<Utc>,
}

/// Business-wide settings (FR-03). Tax rate is in basis points (750 = 7.50%).
#[derive(Debug, Clone, Serialize)]
pub struct BusinessSettings {
    pub business_id: Uuid,
    pub base_currency: String,
    pub timezone: String,
    pub tax_name: String,
    pub tax_rate_bps: i32,
    pub prices_include_tax: bool,
    pub tax_id: Option<String>,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
}

impl From<&Business> for BusinessSettings {
    fn from(b: &Business) -> Self {
        Self {
            business_id: b.id,
            base_currency: b.base_currency.clone(),
            timezone: b.timezone.clone(),
            tax_name: b.tax_name.clone(),
            tax_rate_bps: b.tax_rate_bps,
            prices_include_tax: b.prices_include_tax,
            tax_id: b.tax_id.clone(),
            receipt_header: b.receipt_header.clone(),
            receipt_footer: b.receipt_footer.clone(),
            phone: b.phone.clone(),
            email: b.email.clone(),
        }
    }
}
